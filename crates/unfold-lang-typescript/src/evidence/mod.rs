//! Link-time evidence (docs/languages/typescript.md#binding-and-evidence): the proof of concept's
//! native producer's target computation (`frontend/native/index.ts`, `modules.ts`, `classes.ts`,
//! `declared-types.ts`), evaluated over the payloads' binding tables.
//!
//! - [`targets`]: what a name, member, or expression denotes ([`Target`]): symbols, imports
//!   (through the export tables), script globals, known globals, namespace members, class
//!   members through receivers, and the members of constant object literals (C-2).
//! - [`classes`]: class bases, member lookup through bases, `this` and `super`, and the shapes
//!   and unknown-override facts of the override census.
//! - [`receivers`]: receiver evidence from written annotations, explicit `new`, assertions, and
//!   declared return types (evidence level 4), with the library tables.
//! - [`narrowing`]: receivers narrowed by `instanceof` or type predicates, which withhold a
//!   declared receiver.
//! - [`builtins`]: what can make a library member reach project code: project classes that may
//!   be instances of a library type (`overridden`), and project writes to ECMAScript's own
//!   objects (`dynamic`).
//! - [`assemble`]: the evidence the shared policy reads, import bindings, and annotations.
//! - [`declarations`]: declaration semantics (concrete targets, parameters, stability,
//!   members, overloads, provenance).
//!
//! Every query starts from scratch with its own recursion guard ([`Walk`]) and reads only
//! tables built once per link, so answers never depend on query order or thread.

mod assemble;
mod builtins;
mod classes;
mod declarations;
mod narrowing;
mod receivers;
mod targets;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_engine::evidence::ExternalLocation;
use unfold_engine::{DeclRef, FileId};

pub(crate) use builtins::{Lineage, Patches};
pub(crate) use classes::{Base, ClassIndex};
pub(crate) use narrowing::RootId;

use crate::payload::binding::Meanings;

/// The deepest chain of nested evaluations one query follows; deeper chains name nothing.
const MAX_DEPTH: u32 = 160;

/// What an expression, name, or member denotes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Target {
    /// Indexed declarations, deduplicated, in discovery order.
    pub(crate) declarations: SmallVec<[DeclRef; 2]>,
    /// The members reachable through it: a module's exports or a namespace's members.
    pub(crate) namespace: Option<Namespace>,
    /// External locations.
    pub(crate) external: SmallVec<[ExternalLocation; 1]>,
    /// Excluded boundaries.
    pub(crate) excluded: SmallVec<[ExternalLocation; 1]>,
    /// More than one thing could be meant.
    pub(crate) ambiguous: bool,
}

/// Where a target's members come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Namespace {
    /// A module's exports.
    Module(FileId),
    /// The member tables of a namespace or enum (several for a script namespace declared in
    /// several files).
    Local(SmallVec<[(FileId, u32); 1]>),
}

impl Target {
    /// Whether the target names anything.
    pub(crate) fn is_empty(&self) -> bool {
        self.declarations.is_empty()
            && self.namespace.is_none()
            && self.external.is_empty()
            && self.excluded.is_empty()
            && !self.ambiguous
    }

    /// An external target at `locations`.
    pub(crate) fn external(locations: impl IntoIterator<Item = ExternalLocation>) -> Self {
        Self {
            external: locations.into_iter().collect(),
            ..Self::default()
        }
    }

    /// An excluded boundary at `locations`.
    pub(crate) fn excluded(locations: impl IntoIterator<Item = ExternalLocation>) -> Self {
        Self {
            excluded: locations.into_iter().collect(),
            ..Self::default()
        }
    }

    /// A target of exactly these declarations.
    pub(crate) fn of(declarations: impl IntoIterator<Item = DeclRef>) -> Self {
        let mut target = Self::default();
        for declaration in declarations {
            target.add_declaration(declaration);
        }
        target
    }

    fn add_declaration(&mut self, declaration: DeclRef) {
        if !self.declarations.contains(&declaration) {
            self.declarations.push(declaration);
        }
    }

    /// Several targets joined: everything they name, ambiguous when they reach different
    /// member namespaces.
    pub(crate) fn join(targets: impl IntoIterator<Item = Self>) -> Self {
        let mut joined = Self::default();
        let mut namespaces: SmallVec<[Namespace; 1]> = SmallVec::new();
        for target in targets {
            joined.ambiguous |= target.ambiguous;
            for declaration in target.declarations {
                joined.add_declaration(declaration);
            }
            if let Some(namespace) = target.namespace {
                merge_namespace(&mut namespaces, namespace);
            }
            for location in target.external {
                if !joined.external.contains(&location) {
                    joined.external.push(location);
                }
            }
            for location in target.excluded {
                if !joined.excluded.contains(&location) {
                    joined.excluded.push(location);
                }
            }
        }
        joined.ambiguous |= namespaces.len() > 1;
        if namespaces.len() == 1 {
            joined.namespace = namespaces.pop();
        }
        joined
    }
}

/// Adds `namespace` to `namespaces`, combining local member tables of one merged namespace.
fn merge_namespace(namespaces: &mut SmallVec<[Namespace; 1]>, namespace: Namespace) {
    if namespaces.contains(&namespace) {
        return;
    }
    namespaces.push(namespace);
}

/// What evidence knows about a receiver: its class (and whether the access is static), or that
/// it is external, excluded, an array, an unbound `this`, or what a replaced ECMAScript global
/// constructs; and the ECMAScript type whose methods it has, when a literal or a written type
/// says so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Receiver {
    pub(crate) class: Option<DeclRef>,
    pub(crate) external: bool,
    pub(crate) excluded: bool,
    pub(crate) locations: SmallVec<[ExternalLocation; 1]>,
    /// The library array object (its indexed elements are unknown).
    pub(crate) array: bool,
    /// The ECMAScript type whose methods it has (`String`, `Array`, `Map`, `JSON`,
    /// `ObjectConstructor`), or the ECMAScript class type it was written with (`Error`), as the
    /// tables spell it.
    pub(crate) builtin: Option<&'static str>,
    /// A written library type: the value may be an instance of a project class that extends or
    /// implements it ([`builtins`]).
    pub(crate) nominal: bool,
    pub(crate) is_static: bool,
    pub(crate) unbound_this: bool,
    /// What `new` of an ECMAScript global the project replaced constructs (or `super` in a
    /// class extending one): the replacement's value, whose members are `dynamic`.
    pub(crate) dynamic: bool,
}

impl Receiver {
    fn external(locations: impl IntoIterator<Item = ExternalLocation>, is_static: bool) -> Self {
        Self {
            external: true,
            locations: locations.into_iter().collect(),
            is_static,
            ..Self::default()
        }
    }

    fn excluded(locations: impl IntoIterator<Item = ExternalLocation>) -> Self {
        Self {
            excluded: true,
            locations: locations.into_iter().collect(),
            ..Self::default()
        }
    }

    fn class(class: DeclRef, is_static: bool) -> Self {
        Self {
            class: Some(class),
            is_static,
            ..Self::default()
        }
    }

    fn unbound() -> Self {
        Self {
            unbound_this: true,
            ..Self::default()
        }
    }

    fn dynamic() -> Self {
        Self {
            dynamic: true,
            ..Self::default()
        }
    }
}

/// One query's recursion guard and memo: the expressions, receivers, types, and declarations
/// being evaluated, how deep the evaluation is, and the targets and receivers already
/// evaluated (a receiver chain asks for the same subexpressions many times).
#[derive(Debug, Default)]
pub(crate) struct Walk {
    targets: FxHashSet<(FileId, u32)>,
    receivers: FxHashSet<(FileId, u32)>,
    types: FxHashSet<(FileId, u32)>,
    symbols: FxHashSet<(FileId, u32)>,
    bases: FxHashSet<DeclRef>,
    depth: u32,
    target_memo: FxHashMap<(FileId, u32), Target>,
    receiver_memo: FxHashMap<(FileId, u32), Receiver>,
    class_receiver_memo: FxHashMap<(FileId, u32), Receiver>,
}

impl Walk {
    /// Enters one nested evaluation; `false` when too deep.
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }
}

/// One merged script-global symbol: every file symbol of that name in the configuration
/// group's global scope.
#[derive(Debug, Clone, Default)]
pub(crate) struct GlobalSymbol {
    /// The file symbols, in file order.
    pub(crate) members: SmallVec<[(FileId, u32); 1]>,
    /// The meanings of all of them.
    pub(crate) meanings: Meanings,
    /// What the merged symbol denotes: every member's declarations, and their namespace member
    /// tables (in member order). Built once per link, since a script namespace declared in
    /// many files is looked up from all of them.
    pub(crate) target: Target,
}

/// The members of one `const` object literal written somewhere (the mutation census).
#[derive(Debug, Clone, Default)]
pub(crate) struct WrittenMembers {
    /// Some write names no member (a computed key): any member may have changed.
    pub(crate) any: bool,
    /// The members written by name.
    pub(crate) names: FxHashSet<Box<str>>,
}

/// The namespace member tables of a script namespace declared in several files, indexed by
/// member name so a lookup doesn't search every table.
#[derive(Debug, Clone, Default)]
pub(crate) struct MergedNamespace {
    /// The tables, in member order (as the merged global's target lists them).
    pub(crate) tables: SmallVec<[(FileId, u32); 1]>,
    /// Member name → each table's symbol of that name, in table order.
    pub(crate) members: FxHashMap<Box<str>, SmallVec<[(FileId, u32); 1]>>,
}

/// The tables evidence reads, built once per link.
#[derive(Debug, Default)]
pub(crate) struct Tables {
    /// By file ID: the configuration group (the index of the file's nearest `tsconfig.json`).
    pub(crate) groups: Vec<u32>,
    /// By group: global name → merged symbol.
    pub(crate) globals: Vec<FxHashMap<Box<str>, GlobalSymbol>>,
    /// By file ID: whether each symbol is declared at file scope.
    pub(crate) file_scope: Vec<Vec<bool>>,
    /// Class declarations → their member tables.
    pub(crate) classes: FxHashMap<DeclRef, ClassIndex>,
    /// Class declarations → their resolved base, once computed.
    pub(crate) bases: FxHashMap<DeclRef, Base>,
    /// Member name (`*` for an unknown name) → classes with an unlinked local base that
    /// declare an overriding member of that name.
    pub(crate) unknown_overrides: FxHashMap<Box<str>, SmallVec<[DeclRef; 2]>>,
    /// Class declarations → the names of the overriding members their known subclasses
    /// declare (`*` for an unknown name).
    pub(crate) subclass_members: FxHashMap<DeclRef, FxHashSet<Box<str>>>,
    /// Narrowed receivers: root → the member paths narrowed under it.
    pub(crate) narrowed: FxHashMap<RootId, FxHashSet<Vec<Box<str>>>>,
    /// `const` object-literal variables → their object (an index into the file's objects).
    pub(crate) objects: FxHashMap<DeclRef, u32>,
    /// Object members written somewhere, by object variable.
    pub(crate) written: FxHashMap<DeclRef, WrittenMembers>,
    /// Merged script namespaces with several tables, by their first table.
    pub(crate) merged_namespaces: FxHashMap<(FileId, u32), MergedNamespace>,
    /// Variables assigned to after their declaration (anywhere, closures included).
    pub(crate) reassigned: FxHashSet<DeclRef>,
    /// Class properties written after their declaration, or redeclared by a subclass.
    pub(crate) written_fields: FxHashSet<DeclRef>,
    /// The project's writes to ECMAScript's own objects.
    pub(crate) patches: Patches,
    /// Member name (`*` for an unknown name) → the library types the project classes declaring
    /// a member of that name (or inheriting one from a project base) may be instances of.
    pub(crate) library_overrides: FxHashMap<Box<str>, Lineage>,
}
