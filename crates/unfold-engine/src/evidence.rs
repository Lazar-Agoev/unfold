//! What a language answers when the shared backend asks about a value, a type name, a
//! declaration, or a module: semantic evidence, phrased with program IDs.
//!
//! Evidence can withhold a target, never invent one. The shared policy ([`crate::resolve`])
//! turns evidence into outcomes; a language never decides a link's outcome or reason itself.

use smallvec::SmallVec;

use crate::program::{DeclRef, FileId, NodeRef, ValueRef};

/// Which target computation a value is resolved for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueContext {
    /// A callee: a concrete callable body.
    Call,
    /// A construction: a callable function or method, a class's explicit constructor with a
    /// body, or otherwise the class.
    Construct,
    /// A definition passed as an argument: a concrete function, method, or class.
    Reference,
    /// A type name in an annotation: a `class`, `interface`, `type_alias`, or `enum`
    /// declaration.
    Type,
}

/// Where an external declaration lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalLocation {
    /// The language's default library or known globals.
    Library,
    /// The language's own builtins where its default library holds more than them:
    /// ECMAScript's standard objects, beside the DOM and Node globals of TypeScript's.
    Builtin,
    /// An installed or declared package, by its package name.
    Package(Box<str>),
    /// A repository file outside the indexed files (excluded, failed, or a declaration file).
    File(Box<str>),
}

/// Where a candidate declaration comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// An indexed file of this program.
    Indexed,
    /// Outside the program: a library, a package, or an unindexed repository file.
    External,
    /// A repository file excluded from the index.
    Excluded,
}

/// One possible declaration of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Where it comes from.
    pub origin: Origin,
    /// The declaration, for an indexed candidate the language can name. An indexed candidate
    /// without one stops resolution as `unknown_name`.
    pub declaration: Option<DeclRef>,
    /// A namespace or type declaration that augments a value without supplying a body.
    pub augmentation: bool,
    /// For an external candidate, where it lives (required to label an external outcome).
    pub location: Option<ExternalLocation>,
}

impl Candidate {
    /// An indexed declaration.
    pub const fn indexed(declaration: DeclRef) -> Self {
        Self {
            origin: Origin::Indexed,
            declaration: Some(declaration),
            augmentation: false,
            location: None,
        }
    }

    /// An external declaration at `location`.
    pub const fn external(location: ExternalLocation) -> Self {
        Self {
            origin: Origin::External,
            declaration: None,
            augmentation: false,
            location: Some(location),
        }
    }

    /// Whether this candidate is in an unindexed repository file: excluded, or external at a
    /// file location.
    pub fn is_unindexed_repository(&self) -> bool {
        self.origin == Origin::Excluded
            || (self.origin == Origin::External && matches!(self.location, Some(ExternalLocation::File(_))))
    }
}

/// A language's withholding-only refusal. It can stop a target, never authorize one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// A call result, decorator, or partial supplies the callable.
    Wrapper,
    /// There is no static shape.
    Dynamic,
    /// A recognized form the supported syntax doesn't cover.
    Unsupported,
    /// No proven concrete body.
    NotConcrete,
}

/// How an import binding's module resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportTarget {
    /// An indexed file.
    Indexed,
    /// Outside the repository (a package or built-in).
    External,
    /// A relative or configured path that names nothing.
    NotFound,
    /// An existing repository file outside the index.
    NotIndexed,
    /// More than one module could be meant.
    Ambiguous,
}

/// The import a value's access root is bound by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    /// The written module specifier.
    pub specifier: Box<str>,
    /// The imported export: `default`, `*`, a name, or `export=`.
    pub imported: Box<str>,
    /// How the module resolved.
    pub target: ImportTarget,
    /// Where an external module boundary lives, independently of final declarations.
    pub location: Option<ExternalLocation>,
}

/// Evidence for one value: its candidate declarations and the facts that can withhold them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Evidence {
    /// The candidate declarations, in a deterministic order.
    pub candidates: SmallVec<[Candidate; 2]>,
    /// A withholding-only refusal.
    pub refusal: Option<Refusal>,
    /// The language cannot tell which of several targets is meant.
    pub ambiguous: bool,
    /// A proven receiver family may dispatch to another or an unknown implementation.
    pub overridden: bool,
    /// The receiver's class, for a member access.
    pub receiver_class: Option<DeclRef>,
    /// The access goes through `super`, so overrides don't apply.
    pub super_access: bool,
    /// The access is static, so overrides don't apply.
    pub static_access: bool,
    /// `this` inside a non-arrow function that isn't a member.
    pub unbound_this: bool,
    /// The import binding of the access root.
    pub import_binding: Option<ImportBinding>,
    /// Whether the access root's name is declared in indexed source; `Some(false)` lets an
    /// undeclared, unimported root be labeled `global`.
    pub root_is_indexed: Option<bool>,
}

impl Evidence {
    /// Evidence with only these candidates.
    pub fn candidates(candidates: impl IntoIterator<Item = Candidate>) -> Self {
        Self {
            candidates: candidates.into_iter().collect(),
            ..Self::default()
        }
    }

    /// The candidates that can supply a body: those that are not augmentations, or every
    /// candidate when all of them are.
    pub fn effective(&self) -> impl Iterator<Item = &Candidate> {
        let all_augmentations = self.candidates.iter().all(|candidate| candidate.augmentation);
        self.candidates
            .iter()
            .filter(move |candidate| all_augmentations || !candidate.augmentation)
    }
}

/// A declaration's concrete body for one context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConcreteTarget {
    /// The node a resolved link reaches (for a construction, possibly the class's constructor).
    pub target: NodeRef,
    /// The declaration used to compare overload signatures.
    pub implementation: DeclRef,
}

/// Whether a binding can be reassigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stability {
    /// Never reassigned.
    Stable,
    /// Writes can replace the value (`let`, `var`, competing Python writes, ...).
    Rebindable,
    /// Not a binding the language reasons about.
    Unknown,
}

/// Class-member facts for the override census.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemberFact<'a> {
    /// The class declaring the member.
    pub owner: Option<DeclRef>,
    /// The member's name; `None` for a computed name that isn't a literal.
    pub name: Option<&'a str>,
    /// A static member.
    pub is_static: bool,
    /// A private name, which denotes its own slot.
    pub is_private: bool,
    /// Whether calls to this member must be checked against subclass overrides (instance
    /// methods and callable fields).
    pub check_override: bool,
    /// Whether this member can override an inherited one (not a constructor, not ambient).
    pub can_override: bool,
}

/// What provenance may follow from a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OriginDeclaration {
    /// For an indexed, non-ambient constant initialized by a call or construction, the callee
    /// of that initializer.
    pub factory_callee: Option<ValueRef>,
    /// Whether the declaration can carry a written annotation (parameters, variables,
    /// properties); provenance then asks for [`Linked::annotation_evidence`](crate::Linked::annotation_evidence).
    pub annotation: bool,
}

/// How one written module dependency (a file's [`ImportFact`](crate::facts::ImportFact)) resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleTarget {
    /// An indexed file of the program.
    Indexed(FileId),
    /// Outside the repository, or an existing repository file outside the index (labeled with
    /// its path).
    External(ExternalLocation),
    /// A relative or configured path that names nothing.
    NotFound,
    /// An existing repository module that is not indexed and has no external label.
    NotIndexed,
    /// More than one module could be meant.
    Ambiguous,
    /// A non-literal specifier.
    Dynamic,
    /// A dependency the language recognizes but does not resolve (for example, before its
    /// module resolution exists); the link is `unknown` with `unsupported`.
    Unsupported,
}

/// One class of the program, for the override census.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassShape {
    /// The class declaration.
    pub class: DeclRef,
    /// Its declared bases, in written order (TypeScript has zero or one).
    pub bases: Vec<DeclRef>,
    /// Its own members.
    pub members: Vec<ClassMember>,
}

/// One member of a [`ClassShape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassMember {
    /// The member declaration.
    pub declaration: DeclRef,
    /// Whether it is static.
    pub is_static: bool,
}
