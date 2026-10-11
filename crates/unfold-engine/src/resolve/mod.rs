//! The precision policy: one place that turns language evidence into resolved, external, or
//! unresolved outcomes.
//!
//! A [`Resolver`] pairs the program's facts with one language's [`Linked`] answers and the
//! override census built from its class shapes. [`Resolver::resolve_call`],
//! [`Resolver::resolve_value`], and [`Resolver::resolve_type`] apply the value policy for the
//! call, construct, reference, handler, and type contexts, testing the fifteen reasons in their
//! fixed order (see `docs/graph-model.md`, "Value policy and reason order").

mod overrides;
mod policy;

use unfold_core::vocab::{Reason, Resolution};
use unfold_core::{Key, ModuleLabel};

use crate::error::BackendError;
use crate::evidence::ValueContext;
use crate::facts::{CallContext, Invocation};
use crate::language::{LanguageId, Linked};
use crate::program::{BlockRef, CallRef, FactsSet, FileId, TypeRef, ValueRef};

use overrides::OverrideCensus;
pub(crate) use policy::Expr;

/// How a value, occurrence, or entrance target resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// One proven target block.
    Resolved(BlockRef),
    /// A proven origin outside the graph.
    External(ModuleLabel),
    /// No reliable target.
    Unresolved {
        /// Why.
        reason: Reason,
        /// For an unresolved `parameter` call, the owner's parameter position.
        position: Option<u32>,
    },
}

impl Outcome {
    /// An unresolved outcome without a position.
    pub const fn unresolved(reason: Reason) -> Self {
        Self::Unresolved {
            reason,
            position: None,
        }
    }

    /// The outcome's resolution.
    pub const fn resolution(&self) -> Resolution {
        match self {
            Self::Resolved(_) => Resolution::Resolved,
            Self::External(_) => Resolution::External,
            Self::Unresolved { .. } => Resolution::Unresolved,
        }
    }

    /// The reason, exactly when unresolved.
    pub const fn reason(&self) -> Option<Reason> {
        match self {
            Self::Unresolved { reason, .. } => Some(*reason),
            _ => None,
        }
    }

    /// The graph fields of this outcome: target key, resolution, reason, module, and position.
    /// A resolved outcome whose block doesn't exist in `program` has no key; the policy never
    /// produces one.
    pub(crate) fn into_fields(self, program: &FactsSet) -> OutcomeFields {
        match self {
            Self::Resolved(block) => OutcomeFields {
                target: program.block(block).map(|block| block.key.clone()),
                resolution: Resolution::Resolved,
                reason: None,
                module: None,
                position: None,
            },
            Self::External(module) => OutcomeFields {
                target: None,
                resolution: Resolution::External,
                reason: None,
                module: Some(module),
                position: None,
            },
            Self::Unresolved { reason, position } => OutcomeFields {
                target: None,
                resolution: Resolution::Unresolved,
                reason: Some(reason),
                module: None,
                position,
            },
        }
    }
}

/// An outcome spelled as graph fields.
pub(crate) struct OutcomeFields {
    pub(crate) target: Option<Key>,
    pub(crate) resolution: Resolution,
    pub(crate) reason: Option<Reason>,
    pub(crate) module: Option<ModuleLabel>,
    pub(crate) position: Option<u32>,
}

/// The shared policy over one language's linked program.
pub struct Resolver<'a> {
    program: &'a FactsSet,
    linked: &'a dyn Linked,
    overrides: OverrideCensus,
}

impl std::fmt::Debug for Resolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver")
            .field("language", &self.linked.language())
            .field("files", &self.program.files().len())
            .finish_non_exhaustive()
    }
}

impl<'a> Resolver<'a> {
    /// A resolver for `linked`'s language over `program`. Builds the override census.
    pub fn new(program: &'a FactsSet, linked: &'a dyn Linked) -> Self {
        Self {
            program,
            linked,
            overrides: OverrideCensus::new(linked),
        }
    }

    /// The program's facts.
    pub fn program(&self) -> &'a FactsSet {
        self.program
    }

    /// The language's answers.
    pub fn linked(&self) -> &'a dyn Linked {
        self.linked
    }

    /// The language this resolver resolves.
    pub fn language(&self) -> LanguageId {
        self.linked.language()
    }

    /// Whether `file` belongs to this resolver's language.
    pub fn owns(&self, file: FileId) -> bool {
        self.program.language(file) == Some(self.language())
    }

    /// How a call site is invoked: the language's link-time answer
    /// ([`Linked::invocation`]) when it has one, otherwise the extracted fact's.
    pub fn invocation(&self, call: CallRef) -> Option<Invocation> {
        let fact = self.program.call(call)?;
        Some(self.linked.invocation(call).unwrap_or_else(|| fact.invocation()))
    }

    /// Resolves a call site's callee with its authoritative call evidence, in the call's
    /// context (`call`, or `construct` for constructions and `super(...)`; see
    /// [`Resolver::invocation`]).
    pub fn resolve_call(&self, call: CallRef) -> Result<Outcome, BackendError> {
        self.resolve_invoked(call, self.invocation(call))
    }

    /// [`Resolver::resolve_call`] with the call's [`Resolver::invocation`] already known (the
    /// language may compute it with evidence, so callers that also need it ask once).
    pub(crate) fn resolve_invoked(
        &self,
        call: CallRef,
        invocation: Option<Invocation>,
    ) -> Result<Outcome, BackendError> {
        let (Some(fact), Some(invocation)) = (self.program.call(call), invocation) else {
            return Ok(Outcome::unresolved(Reason::Dynamic));
        };
        let context = match invocation.context {
            CallContext::Call => ValueContext::Call,
            CallContext::Construct => ValueContext::Construct,
        };
        let expr = Expr {
            file: call.file,
            value: fact.callee,
            ty: None,
        };
        policy::resolve(self, expr, context, Some(call))
    }

    /// Resolves a value outside a call site: a reference argument, an entrance handler, or a
    /// value in a call or construct context.
    pub fn resolve_value(&self, value: ValueRef, context: ValueContext) -> Result<Outcome, BackendError> {
        let expr = Expr {
            file: value.file,
            value: Some(value.value),
            ty: None,
        };
        policy::resolve(self, expr, context, None)
    }

    /// Resolves a written type name in the `type` context: through the language's
    /// [`Linked::type_evidence`], to a `class`, `interface`, `type_alias`, or `enum` block.
    pub fn resolve_type(&self, reference: TypeRef) -> Result<Outcome, BackendError> {
        let expr = Expr {
            file: reference.file,
            value: None,
            ty: Some(reference.index),
        };
        policy::resolve(self, expr, ValueContext::Type, None)
    }
}
