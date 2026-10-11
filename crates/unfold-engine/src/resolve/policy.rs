//! The value policy and its reason order.
//!
//! [`resolve`] applies, in order, the tests of `docs/graph-model.md` ("Value policy and reason
//! order"); the first test that matches decides the outcome. The order is part of the contract:
//! reordering two tests changes which reason readers see.

use smallvec::SmallVec;
use unfold_core::vocab::{BlockKind, EdgeKind, Reason, Role};

use super::{Outcome, Resolver};
use crate::error::BackendError;
use crate::evidence::{Candidate, Evidence, ImportTarget, Origin, Refusal, Stability, ValueContext};
use crate::facts::{ValueFact, ValueShape};
use crate::labels;
use crate::program::{BlockRef, CallRef, DeclRef, FileId, TypeRef, ValueRef};

/// The expression being resolved: a value of a file, a call site without a callee, or a written
/// type name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Expr {
    pub(crate) file: FileId,
    pub(crate) value: Option<u32>,
    /// A type reference's index, for a type name ([`ValueContext::Type`]).
    pub(crate) ty: Option<u32>,
}

impl Expr {
    pub(crate) fn value_ref(self) -> Option<ValueRef> {
        self.value.map(|value| ValueRef {
            file: self.file,
            value,
        })
    }

    pub(crate) fn type_ref(self) -> Option<TypeRef> {
        self.ty.map(|index| TypeRef {
            file: self.file,
            index,
        })
    }

    pub(crate) fn fact<'a>(self, resolver: &Resolver<'a>) -> Option<&'a ValueFact> {
        resolver.program().value(self.value_ref()?)
    }

    /// The shape the no-candidate reasons read: a type name is a name, or, qualified, a member.
    fn shape(self, resolver: &Resolver<'_>) -> ValueShape {
        if let Some(reference) = self.type_ref() {
            return match resolver.program().type_ref(reference) {
                Some(fact) if fact.qualified => ValueShape::Member,
                Some(_) => ValueShape::Name,
                None => ValueShape::Other,
            };
        }
        self.fact(resolver).map_or(ValueShape::Other, |fact| fact.shape)
    }

    /// The written text: a value's label or a type name.
    pub(crate) fn label<'a>(self, resolver: &Resolver<'a>) -> &'a str {
        if let Some(reference) = self.type_ref() {
            return resolver
                .program()
                .type_ref(reference)
                .map_or(crate::edges::text::OTHER, |fact| &fact.text);
        }
        self.fact(resolver)
            .map_or(crate::edges::text::OTHER, |fact| &fact.label)
    }
}

const fn unresolved(reason: Reason) -> Result<Outcome, BackendError> {
    Ok(Outcome::unresolved(reason))
}

pub(crate) fn resolve(
    resolver: &Resolver<'_>,
    expr: Expr,
    context: ValueContext,
    call: Option<CallRef>,
) -> Result<Outcome, BackendError> {
    let linked = resolver.linked();
    let shape = expr.shape(resolver);
    let evidence = match (call, expr.value_ref()) {
        _ if context == ValueContext::Type => match expr.type_ref() {
            Some(reference) => linked.type_evidence(reference),
            None => Evidence::default(),
        },
        (Some(call), _) => linked.callee_evidence(call),
        (None, value) if matches!(context, ValueContext::Reference | ValueContext::Handler) => {
            // Handler and reference shape checks come before any evidence is consulted.
            if context == ValueContext::Handler && shape == ValueShape::CallResult {
                return unresolved(Reason::Wrapper);
            }
            if matches!(shape, ValueShape::InlineFunction | ValueShape::InlineClass) {
                return Ok(inline(resolver, expr, context));
            }
            match value {
                Some(value) if shape.is_reference() => linked.value_evidence(value),
                _ => return unresolved(Reason::Dynamic),
            }
        }
        (None, Some(value)) => linked.value_evidence(value),
        (None, None) => Evidence::default(),
    };

    // 1–4: language withholding, ambiguity, unbound `this`, and failed import bindings.
    if let Some(refusal) = evidence.refusal {
        return unresolved(match refusal {
            Refusal::Wrapper => Reason::Wrapper,
            Refusal::Dynamic => Reason::Dynamic,
            Refusal::Unsupported => Reason::Unsupported,
            Refusal::NotConcrete => Reason::NotConcrete,
        });
    }
    if evidence.ambiguous {
        return unresolved(Reason::Ambiguous);
    }
    if evidence.unbound_this {
        return unresolved(Reason::UnboundThis);
    }
    match evidence.import_binding.as_ref().map(|binding| binding.target) {
        Some(ImportTarget::NotFound) => return unresolved(Reason::NotFound),
        Some(ImportTarget::NotIndexed) => return unresolved(Reason::NotIndexed),
        Some(ImportTarget::Ambiguous) => return unresolved(Reason::Ambiguous),
        _ => {}
    }

    let candidates: SmallVec<[&Candidate; 2]> = evidence
        .candidates
        .iter()
        .filter(|candidate| !candidate.augmentation)
        .collect();
    // 5: an unindexed repository candidate.
    if evidence.effective().any(Candidate::is_unindexed_repository) {
        return unresolved(Reason::NotIndexed);
    }
    let external = || labels::external_label(resolver, expr, &evidence).map(Outcome::External);
    // 6: a proven package-import root, unless an indexed candidate contradicts it.
    let package = labels::external_package_binding(linked, expr.file, evidence.import_binding.as_ref());
    if package.is_some()
        && !evidence
            .effective()
            .any(|candidate| candidate.origin == Origin::Indexed)
    {
        return external();
    }
    // 7: no candidates that can supply a body.
    if candidates.is_empty() {
        let augmentations_external = !evidence.candidates.is_empty()
            && evidence
                .candidates
                .iter()
                .all(|candidate| candidate.origin == Origin::External);
        if augmentations_external {
            return external();
        }
        return unresolved(match shape {
            ValueShape::Name => Reason::UnknownName,
            ValueShape::Member => Reason::UnknownReceiver,
            _ => Reason::Dynamic,
        });
    }
    // 8: external or mixed origins.
    if candidates
        .iter()
        .all(|candidate| candidate.origin == Origin::External)
    {
        return external();
    }
    if candidates
        .iter()
        .any(|candidate| candidate.origin != Origin::Indexed)
    {
        return unresolved(Reason::Ambiguous);
    }
    // 9: an indexed candidate the language cannot name.
    let mut declarations: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for candidate in &candidates {
        match candidate.declaration {
            Some(declaration) => declarations.push(declaration),
            None => return unresolved(Reason::UnknownName),
        }
    }

    let mut targets: SmallVec<[(BlockRef, DeclRef); 2]> = SmallVec::new();
    let mut nonconcrete: SmallVec<[DeclRef; 2]> = SmallVec::new();
    let mut members: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for &declaration in &declarations {
        let concrete = linked
            .concrete_target(declaration, context)
            .filter(|concrete| resolver.program().block(concrete.target).is_some());
        let Some(concrete) = concrete else {
            nonconcrete.push(declaration);
            continue;
        };
        // Like a keyed map: one entry per target block, holding the latest implementation.
        match targets.iter_mut().find(|(block, _)| *block == concrete.target) {
            Some(entry) => entry.1 = concrete.implementation,
            None => targets.push((concrete.target, concrete.implementation)),
        }
        // Overrides don't apply to types.
        if context != ValueContext::Type && linked.member(declaration).check_override {
            members.push(declaration);
        }
    }
    // 10: several distinct concrete targets.
    if targets.len() > 1 {
        return unresolved(Reason::Ambiguous);
    }
    // 11: no target, with reasons in a fixed order independent of candidate order.
    let Some(&(target, implementation)) = targets.first() else {
        if nonconcrete
            .iter()
            .any(|&declaration| linked.parameter(declaration).is_some())
        {
            let position = match (context, call) {
                (ValueContext::Call, Some(call)) => parameter_position(resolver, call, &nonconcrete),
                _ => None,
            };
            return Ok(Outcome::Unresolved {
                reason: Reason::Parameter,
                position,
            });
        }
        if nonconcrete
            .iter()
            .any(|&declaration| linked.binding_stability(declaration) == Stability::Rebindable)
        {
            return unresolved(Reason::MutableBinding);
        }
        if nonconcrete
            .iter()
            .any(|&declaration| linked.accessor(declaration))
        {
            return unresolved(Reason::Accessor);
        }
        return unresolved(Reason::NotConcrete);
    };
    // 12: non-concrete declarations beside the target, other than its compatible overloads.
    if nonconcrete
        .iter()
        .any(|&declaration| !linked.overload_of(declaration, implementation))
    {
        return unresolved(Reason::Ambiguous);
    }
    // 13: overrides.
    if evidence.overridden
        || members
            .iter()
            .any(|&member| resolver.overrides.is_overridden(linked, member, &evidence))
    {
        return unresolved(Reason::Overridden);
    }
    // 14–15: target kinds for references and handlers, then the one permitted target.
    Ok(permitted(resolver, target, context))
}

/// Steps 14 and 15 for a single target block.
fn permitted(resolver: &Resolver<'_>, target: BlockRef, context: ValueContext) -> Outcome {
    let kind = resolver.program().block(target).map(|block| block.kind);
    let unsupported = match context {
        ValueContext::Handler if kind == Some(BlockKind::Class) => true,
        ValueContext::Reference | ValueContext::Handler => {
            !kind.is_some_and(|kind| EdgeKind::References.target_kinds().contains(&kind))
        }
        ValueContext::Type => !kind.is_some_and(BlockKind::is_type_declaration),
        ValueContext::Call | ValueContext::Construct => false,
    };
    if unsupported {
        Outcome::unresolved(Reason::Unsupported)
    } else {
        Outcome::Resolved(target)
    }
}

/// An inline function or class in a reference or handler context has its own identity: its
/// block. Without a block it has no concrete body.
fn inline(resolver: &Resolver<'_>, expr: Expr, context: ValueContext) -> Outcome {
    match expr.fact(resolver).and_then(|fact| fact.block) {
        Some(local) => permitted(
            resolver,
            BlockRef {
                file: expr.file,
                local,
            },
            context,
        ),
        None => Outcome::unresolved(Reason::NotConcrete),
    }
}

/// The parameter position of an unresolved `parameter` call: recorded only when exactly one
/// containing parameter is involved and it belongs to the call's owner. Constructions and outer
/// closure parameters leave it absent. Positions are the language's runtime positions (C-4:
/// TypeScript skips an explicit `this`); nothing here adjusts them.
fn parameter_position(resolver: &Resolver<'_>, call: CallRef, declarations: &[DeclRef]) -> Option<u32> {
    let program = resolver.program();
    let fact = program.call(call)?;
    let call_block = BlockRef {
        file: call.file,
        local: fact.block,
    };
    if program.block(call_block)?.kind == BlockKind::New {
        return None;
    }
    let mut parameter = None;
    for &declaration in declarations {
        let Some(found) = resolver.linked().parameter(declaration) else {
            continue;
        };
        match parameter {
            None => parameter = Some(found),
            Some(existing) if existing.parameter == found.parameter => {}
            Some(_) => return None,
        }
    }
    let parameter = parameter?;
    let owner = call_owner(resolver, call_block)?;
    if parameter.owner != Some(owner) {
        return None;
    }
    parameter.position
}

/// The owner of an occurrence: the nearest ancestor that is the file, a class (unless reached
/// through a Python `header`), or a function-like block reached through its body or a parameter.
fn call_owner(resolver: &Resolver<'_>, block: BlockRef) -> Option<BlockRef> {
    let blocks = resolver.program().facts(block.file)?.graph.blocks();
    let mut saved = blocks.get(block.local as usize)?;
    loop {
        let parent = blocks.get(saved.parent? as usize)?;
        let owns = match parent.kind {
            BlockKind::File => true,
            BlockKind::Class => saved.role != Some(Role::Header),
            kind if kind.is_function_like() => matches!(saved.role, Some(Role::Body | Role::Parameter)),
            _ => false,
        };
        if owns {
            return Some(BlockRef {
                file: block.file,
                local: parent.local_id,
            });
        }
        saved = parent;
    }
}
