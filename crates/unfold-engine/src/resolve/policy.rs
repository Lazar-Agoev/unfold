//! The value policy and its reason order.
//!
//! [`resolve`] applies its tests in order (numbered in the code); the first test that matches
//! decides the outcome. The order is part of the contract: reordering two tests changes which
//! reason readers see.

use smallvec::SmallVec;
use unfold_core::vocab::{NodeKind, Reason};

use super::{Outcome, Resolver};
use crate::error::BackendError;
use crate::evidence::{Candidate, ImportTarget, Origin, Refusal, Stability, ValueContext};
use crate::facts::{ValueFact, ValueShape};
use crate::labels;
use crate::links::text;
use crate::program::{CallRef, DeclRef, FileId, NodeRef, TypeRef, ValueRef};

/// The expression being resolved: a value of a file (a callee, or a passed argument), or a
/// written type name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expr {
    Value(ValueRef),
    Type(TypeRef),
}

impl Expr {
    pub(crate) const fn file(self) -> FileId {
        match self {
            Self::Value(value) => value.file,
            Self::Type(reference) => reference.file,
        }
    }

    pub(crate) const fn value_ref(self) -> Option<ValueRef> {
        match self {
            Self::Value(value) => Some(value),
            Self::Type(_) => None,
        }
    }

    pub(crate) const fn type_ref(self) -> Option<TypeRef> {
        match self {
            Self::Value(_) => None,
            Self::Type(reference) => Some(reference),
        }
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
                .map_or(text::OTHER, |fact| &fact.text);
        }
        self.fact(resolver).map_or(text::OTHER, |fact| &fact.label)
    }
}

const fn unresolved(reason: Reason) -> Result<Outcome, BackendError> {
    Ok(Outcome::Unresolved(reason))
}

pub(crate) fn resolve(
    resolver: &Resolver<'_>,
    expr: Expr,
    context: ValueContext,
    call: Option<CallRef>,
) -> Result<Outcome, BackendError> {
    let linked = resolver.linked();
    let shape = expr.shape(resolver);
    let evidence = match (expr, call) {
        (Expr::Type(reference), _) => linked.type_evidence(reference),
        (Expr::Value(_), Some(call)) => linked.callee_evidence(call),
        (Expr::Value(value), None) => {
            // Only a name or a member access can pass a definition; the shape decides before
            // any evidence is consulted.
            if context == ValueContext::Reference && !shape.is_reference() {
                return unresolved(Reason::Dynamic);
            }
            linked.value_evidence(value)
        }
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
    // An external target the receiver may dispatch elsewhere (another class may define the
    // member) is overridden.
    let external = || {
        if evidence.overridden {
            return unresolved(Reason::Overridden);
        }
        labels::external_label(resolver, expr, &evidence).map(Outcome::External)
    };
    // 6: a proven package-import root, unless an indexed candidate contradicts it.
    let package = labels::external_package_binding(linked, expr.file(), evidence.import_binding.as_ref());
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

    let mut targets: SmallVec<[(NodeRef, DeclRef); 2]> = SmallVec::new();
    let mut nonconcrete: SmallVec<[DeclRef; 2]> = SmallVec::new();
    let mut members: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for &declaration in &declarations {
        let concrete = linked
            .concrete_target(declaration, context)
            .filter(|concrete| resolver.program().node(concrete.target).is_some());
        let Some(concrete) = concrete else {
            nonconcrete.push(declaration);
            continue;
        };
        // Like a keyed map: one entry per target node, holding the latest implementation.
        match targets.iter_mut().find(|(node, _)| *node == concrete.target) {
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
            .any(|&declaration| linked.is_parameter(declaration))
        {
            return unresolved(Reason::Parameter);
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
    // 14–15: target kinds for references and type names, then the one permitted target.
    Ok(permitted(resolver, target, context))
}

/// Steps 14 and 15 for a single target node: a reference reaches a function or class, a type
/// name a class or type; a callee may reach any concrete target.
fn permitted(resolver: &Resolver<'_>, target: NodeRef, context: ValueContext) -> Outcome {
    let kind = resolver.program().node(target).map(|node| node.kind());
    let permitted = match context {
        ValueContext::Reference => matches!(kind, Some(NodeKind::Function | NodeKind::Class)),
        ValueContext::Type => matches!(kind, Some(NodeKind::Class | NodeKind::Type)),
        ValueContext::Call | ValueContext::Construct => true,
    };
    if permitted {
        Outcome::Resolved(target)
    } else {
        Outcome::Unresolved(Reason::Unsupported)
    }
}
