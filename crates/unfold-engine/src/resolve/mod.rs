//! The precision policy: one place that turns language evidence into outcomes.
//!
//! A [`Resolver`] pairs the program's facts with one language's [`Linked`] answers and the
//! override census built from its class shapes. [`Resolver::resolve_call`],
//! [`Resolver::resolve_value`], and [`Resolver::resolve_type`] apply the value policy for the
//! call, construct, reference, and type contexts, testing the reasons in their fixed order (see
//! `policy`). [`crate::links`] turns each outcome into a link's target.

mod overrides;
mod policy;

use unfold_core::ModuleLabel;
use unfold_core::vocab::Reason;

use crate::error::BackendError;
use crate::evidence::ValueContext;
use crate::facts::CallContext;
use crate::language::{LanguageId, Linked};
use crate::program::{CallRef, FactsSet, FileId, NodeRef, TypeRef, ValueRef};

use overrides::OverrideCensus;
pub(crate) use policy::Expr;

/// How a callee, value, or type name resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// One proven target node.
    Resolved(NodeRef),
    /// A proven origin outside the graph: a library, or the language's builtins.
    External(ModuleLabel),
    /// No proven target, for this reason.
    Unresolved(Reason),
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

    /// Resolves a call's callee with its authoritative callee evidence, in `context`: the
    /// call fact's own, or the construct context when linking proves a construction
    /// ([`Linked::constructs`]).
    pub fn resolve_call(&self, call: CallRef, context: CallContext) -> Result<Outcome, BackendError> {
        let Some(fact) = self.program.call(call) else {
            return Ok(Outcome::Unresolved(Reason::Dynamic));
        };
        let context = match context {
            CallContext::Call => ValueContext::Call,
            CallContext::Construct => ValueContext::Construct,
        };
        let callee = ValueRef {
            file: call.file,
            value: fact.callee,
        };
        policy::resolve(self, Expr::Value(callee), context, Some(call))
    }

    /// Resolves a value outside a callee: a passed argument (in the reference context), or a
    /// value in a call or construct context.
    pub fn resolve_value(&self, value: ValueRef, context: ValueContext) -> Result<Outcome, BackendError> {
        policy::resolve(self, Expr::Value(value), context, None)
    }

    /// Resolves a written type name in the `type` context: through the language's
    /// [`Linked::type_evidence`], to a class or type node.
    pub fn resolve_type(&self, reference: TypeRef) -> Result<Outcome, BackendError> {
        policy::resolve(self, Expr::Type(reference), ValueContext::Type, None)
    }
}
