//! Bounded proof of framework API identity, and the broader negative census that withholds it
//! (the proof of concept's `FrameworkOrigins`). It never changes ordinary call evidence.
//!
//! **Positive proof.** A receiver is a framework object when it is a call of a framework class
//! with proven imported identity (`fastapi.FastAPI(...)`), a name or attribute proven to be one
//! stable declaration initialized by such a call (through aliases and re-exports), or a
//! parameter or variable whose written annotation has that identity (annotation only: it can't
//! name a concrete object, and an indexed subclass of the family makes it uncertain).
//!
//! **Negative census.** Every indexed write that could replace a framework API invalidates it:
//! attribute and item writes (`app.get = ...`, `app.__dict__['get'] = ...`, through delegated
//! fields and aliases, stored receivers and literal containers), deletion, loop and `with`
//! targets, augmented assignment, builtin `setattr`/`delattr` and `dict` mutators, and mutating
//! methods (`update`, `__setitem__`, ...) on anything that may be the API. A write that reaches a
//! proven receiver invalidates that receiver; one that reaches the framework class (or only an
//! annotation) invalidates the whole family. The census follows every possible write, ambiguous
//! and rebound ones included, so star imports and dynamic scopes can't hide known writes; it
//! only ever withholds.
//!
//! The census is in [`census`]; the withholding queries detectors ask about possible values
//! and families are in [`queries`].

mod census;
mod flow;
mod queries;

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use super::Source;
use crate::bind::ir::{ExprKind, NodeKind};
use crate::link::binder::Ctx;
use crate::link::children::Raw;
use crate::link::program::{ExprRef, NodeRef, Sym};

pub(super) use flow::Flow;

/// The most nested steps one census walk takes; a longer walk withholds everything it could
/// have proven (the census is negative evidence, so giving up must invalidate).
const MAX_DEPTH: u32 = 64;

/// `dict` methods that change the mapping in place.
pub(super) const DICT_CHANGES: &[&str] = &[
    "update",
    "__setitem__",
    "__delitem__",
    "setdefault",
    "pop",
    "popitem",
    "clear",
    "__ior__",
];

/// One framework API class (or factory) the detector recognizes by imported identity.
#[derive(Debug)]
pub(super) struct Spec {
    /// The family whose APIs one mutation can replace together.
    pub(super) family: &'static str,
    /// What the class is to the detector (`application`, `router`, `scheduler`, ...).
    pub(super) kind: &'static str,
    /// Every import path of the class.
    pub(super) imports: &'static [&'static str],
    /// Calls on a proven receiver that don't count as mutations.
    pub(super) safe_updates: &'static [SafeUpdate],
}

/// A narrow allowance: `receiver.<path>(...)` with only these unique keywords.
#[derive(Debug)]
pub(super) struct SafeUpdate {
    pub(super) path: &'static [&'static str],
    pub(super) keywords: &'static [&'static str],
}

/// What identifies one framework object: the constructor call itself, or the declaration it
/// initializes (or annotates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Identity {
    Call(ExprRef),
    Node(NodeRef),
}

/// A proven framework object.
#[derive(Debug, Clone, Copy)]
pub(super) struct Receiver {
    pub(super) spec: &'static Spec,
    pub(super) identity: Identity,
    /// The constructor call, unless only an annotation proves the object.
    pub(super) factory: Option<ExprRef>,
    pub(super) annotation_only: bool,
}

/// One step from a stored location's root: an attribute or an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Segment {
    Member(Sym),
    Item,
}

type Path = SmallVec<[Segment; 4]>;

/// The framework origins of one detector's specs over the whole program: the census of what
/// could replace those APIs, over the shared [`Flow`].
pub(super) struct Origins<'f, 's, 'b, 'a> {
    flow: &'f Flow<'s, 'b, 'a>,
    specs: &'static [Spec],
    invalid_families: FxHashSet<&'static str>,
    uncertain_annotations: FxHashSet<&'static str>,
    invalid_receivers: FxHashSet<Identity>,
    /// The families some import (or builtin name) of the program could name at all.
    reachable: FxHashSet<&'static str>,
    families: RefCell<FxHashMap<Raw, Rc<FxHashSet<&'static str>>>>,
}

/// An origin answers every flow question too (identities, stored values, containers).
impl<'s, 'b, 'a> Deref for Origins<'_, 's, 'b, 'a> {
    type Target = Flow<'s, 'b, 'a>;

    fn deref(&self) -> &Self::Target {
        self.flow
    }
}

impl<'f, 's, 'b, 'a> Origins<'f, 's, 'b, 'a> {
    /// Runs the census for `specs` over every Python file.
    pub(super) fn new(flow: &'f Flow<'s, 'b, 'a>, specs: &'static [Spec]) -> Self {
        let mut origins = Self {
            flow,
            specs,
            invalid_families: FxHashSet::default(),
            uncertain_annotations: FxHashSet::default(),
            invalid_receivers: FxHashSet::default(),
            reachable: reachable(flow.source, specs),
            families: RefCell::default(),
        };
        origins.collect_subtypes();
        origins.collect_mutations();
        if origins.exhausted.get() {
            origins
                .invalid_families
                .extend(specs.iter().map(|spec| spec.family));
        }
        origins
    }

    /// The detector's source.
    pub(super) fn source(&self) -> &'s Source<'b, 'a> {
        self.flow.source
    }

    /// The identity, unless its family's API may have been replaced.
    pub(super) fn trusted_import(&self, expr: Option<ExprRef>) -> Option<Rc<str>> {
        let identity = self.imported(expr)?;
        match self.spec(&identity) {
            Some(spec) if self.invalid_families.contains(spec.family) => None,
            _ => Some(identity),
        }
    }

    fn spec(&self, identity: &str) -> Option<&'static Spec> {
        self.specs.iter().find(|spec| spec.imports.contains(&identity))
    }

    fn spec_of(&self, expr: Option<ExprRef>) -> Option<&'static Spec> {
        self.spec(&self.imported(expr)?)
    }

    // --- Receivers --------------------------------------------------------------------------

    /// A framework object `raw` is proven to be, before the census is consulted.
    pub(super) fn unguarded_receiver(&self, raw: Raw) -> Option<Receiver> {
        let Raw::Expr(expr) = raw else {
            return None;
        };
        let source = self.source;
        let binder = source.binder;
        match source.kind(expr)? {
            ExprKind::Call { .. } => {
                let spec = self.spec_of(source.callee(expr))?;
                Some(Receiver {
                    spec,
                    identity: Identity::Call(expr),
                    factory: Some(expr),
                    annotation_only: false,
                })
            }
            ExprKind::Name(_) | ExprKind::Member { .. } => {
                let mut ctx = Ctx::new();
                let value = binder.resolve(expr, &mut ctx);
                if value.refusal.is_some()
                    || value.ambiguous
                    || value.excluded.is_some()
                    || value.external.is_some()
                    || value.module.is_some()
                    || value
                        .binding
                        .is_some_and(|binding| !binder.stable(binding, &mut ctx))
                {
                    return None;
                }
                let [declaration] = value.declarations.as_slice() else {
                    return None;
                };
                let declaration = *declaration;
                let binding = binder.program.binding_of(declaration)?;
                if !binder.stable(binding, &mut ctx) {
                    return None;
                }
                match binder.program.node(declaration)?.kind {
                    NodeKind::Assignment {
                        value: Some(assigned),
                        ..
                    } => {
                        let assigned = self.unchain(ExprRef {
                            file: declaration.file,
                            expr: assigned,
                        });
                        let spec = match source.kind(assigned)? {
                            ExprKind::Call { .. } => self.spec_of(source.callee(assigned))?,
                            _ => return None,
                        };
                        Some(Receiver {
                            spec,
                            identity: Identity::Node(declaration),
                            factory: Some(assigned),
                            annotation_only: false,
                        })
                    }
                    NodeKind::Assignment {
                        value: None,
                        annotation: Some(annotation),
                        ..
                    }
                    | NodeKind::Parameter {
                        annotation: Some(annotation),
                        ..
                    } => {
                        let spec = self.spec_of(Some(ExprRef {
                            file: declaration.file,
                            expr: annotation,
                        }))?;
                        Some(Receiver {
                            spec,
                            identity: Identity::Node(declaration),
                            factory: None,
                            annotation_only: true,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A framework object `expr` is proven to be, unless the census withholds it.
    pub(super) fn receiver(&self, expr: Option<ExprRef>) -> Option<Receiver> {
        let receiver = self.unguarded_receiver(Raw::Expr(expr?))?;
        let withheld = self.invalid_families.contains(receiver.spec.family)
            || self.invalid_receivers.contains(&receiver.identity)
            || receiver.annotation_only && self.uncertain_annotations.contains(receiver.spec.family);
        (!withheld).then_some(receiver)
    }

    // --- Walk helpers -----------------------------------------------------------------------
}

/// The families of `specs` whose identities some import write could start: an external
/// `module` (or `module.member`) that is a prefix of a spec import, or (for `builtins`
/// identities) a builtin name written anywhere. A superset, so it only skips work.
fn reachable(source: &Source<'_, '_>, specs: &'static [Spec]) -> FxHashSet<&'static str> {
    let program = &source.binder.program;
    let mut roots: FxHashSet<String> = FxHashSet::default();
    for write in &program.writes {
        let Some(imported) = &write.import else {
            continue;
        };
        if imported.module.external.is_none() {
            continue;
        }
        let mut root = imported.module.specifier.to_string();
        if let Some(member) = imported.member {
            root.push('.');
            root.push_str(program.text(member));
        }
        roots.insert(root);
    }
    let mut builtins: FxHashSet<&str> = FxHashSet::default();
    for (index, bind) in program.files.iter().enumerate() {
        let Some(bind) = bind else {
            continue;
        };
        for fact in &bind.exprs {
            if let ExprKind::Name(name) = fact.kind {
                builtins.insert(program.text(program.sym(unfold_engine::FileId(index as u32), name)));
            }
        }
    }
    let starts = |identity: &str| {
        roots.iter().any(|root| {
            identity == root
                || identity
                    .strip_prefix(root.as_str())
                    .is_some_and(|rest| rest.starts_with('.'))
                || root
                    .strip_prefix(identity)
                    .is_some_and(|rest| rest.starts_with('.'))
        }) || identity
            .strip_prefix("builtins.")
            .is_some_and(|name| builtins.contains(name.split('.').next().unwrap_or(name)))
    };
    specs
        .iter()
        .filter(|spec| spec.imports.iter().any(|identity| starts(identity)))
        .map(|spec| spec.family)
        .collect()
}

/// How a stored location relates to a queried one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relation {
    /// The stored location is a prefix of (or equal to) the queried one.
    Prefix,
    /// The two are equal.
    Exact,
    /// The stored location extends (or equals) the queried one.
    Extends,
}

/// What the mutation census invalidates.
#[derive(Debug, Default)]
struct Census {
    families: FxHashSet<&'static str>,
    receivers: FxHashSet<Identity>,
}
