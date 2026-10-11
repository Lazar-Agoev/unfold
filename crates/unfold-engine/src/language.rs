//! The contract between the engine and a language front end (docs/architecture.md#ownership).
//!
//! A language answers narrow questions; the shared backend owns the policy. Each language
//! implements two traits:
//!
//! - [`Language`]: per-file extraction, which is cached, plus linking, highlighting, and the
//!   settings that shape its files.
//! - [`Linked`]: what the backend asks about the whole program after linking. Every question is
//!   phrased with program IDs ([`crate::program`]) over cached facts, never syntax nodes: callee
//!   and value evidence, declaration semantics, module dependencies and exports, class shapes,
//!   module-label spelling, provenance facts, locator lookups, entrance detectors, and the
//!   independent census validation compares the graph with.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use unfold_core::highlight::Highlighter;
use unfold_core::validate::Census;
use unfold_core::vocab::InputRole;
use unfold_core::{RepoPath, Snapshot};

use crate::entrances::EntranceDetector;
use crate::evidence::{
    ClassShape, ConcreteTarget, Evidence, Export, ExternalLocation, LocatorMembers, MemberFact, ModuleTarget,
    OriginDeclaration, ParameterFact, Stability, ValueContext,
};
use crate::facts::{FileFacts, Invocation};
use crate::program::{BlockRef, CallRef, DeclRef, FactsSet, FileId, TypeRef, ValueRef};
use crate::settings::{Settings, SettingsHash};

/// A language's identity in the graph model (`typescript` or `python`).
pub type LanguageId = unfold_core::vocab::Language;

/// The name and version of a language front end, stored with the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Producer {
    /// The front end's name, such as `unfold-lang-typescript`.
    pub name: &'static str,
    /// Its version.
    pub version: &'static str,
}

/// Why a file could not be extracted: a syntax error, an unsupported encoding declaration, or a
/// grammar the selected language version rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailure {
    /// What went wrong, in words.
    pub message: Box<str>,
    /// The byte offset it was found at, if known.
    pub offset: Option<u32>,
}

impl fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "{} (at byte {offset})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for SourceFailure {}

/// One configuration input a language may read while linking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInput {
    /// What the input is.
    pub role: InputRole,
    /// Its bytes, read once.
    pub snapshot: Snapshot,
}

/// Everything outside the indexed sources that shapes linking.
#[derive(Debug, Clone, Default)]
pub struct ProjectInputs {
    /// The folder's validated settings.
    pub settings: Settings,
    /// Configuration inputs (`tsconfig.json` chains, `package.json`, `pyproject.toml`) by path.
    pub inputs: BTreeMap<RepoPath, ProjectInput>,
    /// Repository files that exist but are outside the indexed scope, so references to them are
    /// `not_indexed` rather than `not_found`. Sorted by path.
    pub unindexed: Vec<RepoPath>,
    /// Context files discovery found (TypeScript declaration files: `.d.ts`, `.d.mts`, `.d.cts`),
    /// whatever their scope status. They never get blocks; a language may resolve modules to
    /// them. The set is part of what shapes the graph, so adding or removing one relinks.
    /// Sorted by path.
    pub context: Vec<RepoPath>,
    /// The open folder, for the few existence checks discovery can't answer (paths under ignored
    /// or always-excluded folders, which are never walked); see [`ProjectInputs::exists`].
    /// `None` outside a real folder, as in tests that link in memory.
    pub root: Option<PathBuf>,
}

impl ProjectInputs {
    /// Whether a file or folder exists at `path`: an unindexed file, a context file, an input,
    /// or (when [`ProjectInputs::root`] is set) anything on disk inside the folder, without
    /// following a symlink out of it. Indexed sources are the language's own knowledge.
    pub fn exists(&self, path: &RepoPath) -> bool {
        self.unindexed.binary_search(path).is_ok()
            || self.context.binary_search(path).is_ok()
            || self.inputs.contains_key(path)
            || self
                .root
                .as_deref()
                .is_some_and(|root| crate::discovery::exists_in_root(root, path))
    }
}

/// Per-file extraction options a language derives from the settings and the project inputs
/// (for TypeScript, `verbatimModuleSyntax` from the file's `tsconfig.json`).
///
/// `hash` replaces [`Language::settings_hash`] as the file's settings hash, so it is part of the
/// facts-cache key and must cover everything the options change about the facts. `options` is
/// opaque to the engine: it hands the value back to [`Language::extract_with`] unchanged.
#[derive(Clone)]
pub struct ExtractionOptions {
    /// The file's settings hash, covering every option.
    pub hash: SettingsHash,
    /// The language's own options for the file, if it has any.
    pub options: Option<Arc<dyn Any + Send + Sync>>,
}

impl ExtractionOptions {
    /// Options that are only a settings hash.
    pub fn new(hash: SettingsHash) -> Self {
        Self { hash, options: None }
    }

    /// Options with a language value.
    pub fn with(hash: SettingsHash, options: Arc<dyn Any + Send + Sync>) -> Self {
        Self {
            hash,
            options: Some(options),
        }
    }

    /// The language value, if it is a `T`.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.options
            .as_deref()
            .and_then(|options| options.downcast_ref::<T>())
    }
}

impl fmt::Debug for ExtractionOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractionOptions")
            .field("hash", &self.hash)
            .field("options", &self.options.is_some())
            .finish()
    }
}

/// A problem with a configuration input that a language found while reading it, such as a
/// `tsconfig.json` whose `extends` chain names a package that isn't installed. The language
/// carries on with what it could read; the problem is listed and the graph is incomplete.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputProblem {
    /// The input the problem is in.
    pub path: RepoPath,
    /// What went wrong, in words.
    pub message: Box<str>,
}

/// A problem a language found while linking, such as an import several source roots could
/// supply, or a configuration input a detector reads that isn't valid. It is listed in the
/// Problems view with the configuration-input problems.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageProblem {
    /// The file or input the problem is in.
    pub path: RepoPath,
    /// What went wrong, in words.
    pub message: Box<str>,
    /// Whether the graph lacks something because of it (an unreadable input whose entrances
    /// are missing), rather than describing a fact about the code (an ambiguous import, whose
    /// edges are already `ambiguous`).
    pub incomplete: bool,
}

/// Where an emitted output path comes from, through a language's emit map (TypeScript's
/// `outDir`/`rootDir` mapping: `dist/server.js` comes from `src/server.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EmitSource {
    /// The language has no source that emits this path.
    #[default]
    Unknown,
    /// Exactly one source emits it.
    Source(RepoPath),
    /// Several sources emit it.
    Ambiguous,
}

/// One source language. Extraction is per file and cached; linking sees the whole program.
pub trait Language: Send + Sync {
    /// The language's identity.
    fn id(&self) -> LanguageId;

    /// The front end's name and version, stored with the graph.
    fn producer(&self) -> Producer;

    /// Whether this language indexes `path` (by extension and settings).
    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool;

    /// The fingerprint of the settings that shape `path`'s facts. It is part of the facts-cache
    /// key, so it must cover everything [`Language::extract`] reads from `settings`.
    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash;

    /// Parses one file and extracts its facts, driving [`crate::select`] for the shared part.
    /// It must never panic, whatever the bytes.
    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure>;

    /// The extraction options of `paths` (all owned by this language), in the same order, from
    /// the settings and the project inputs. The engine calls it once per update for the files
    /// whose options it needs, after the inputs are read, and uses each result's hash as the
    /// file's settings hash; a file whose hash changes is extracted again. The default is just
    /// [`Language::settings_hash`].
    ///
    /// The options may depend only on `settings` and the configuration inputs
    /// (`inputs.inputs`): the engine keeps a file's hash across updates that change neither.
    fn extraction_options(
        &self,
        paths: &[&RepoPath],
        settings: &Settings,
        _inputs: &ProjectInputs,
    ) -> Vec<ExtractionOptions> {
        paths
            .iter()
            .map(|path| ExtractionOptions::new(self.settings_hash(path, settings)))
            .collect()
    }

    /// Extracts one file with the options [`Language::extraction_options`] gave it. The default
    /// ignores them and calls [`Language::extract`]. It must never panic, whatever the bytes.
    fn extract_with(
        &self,
        file: &Snapshot,
        settings: &Settings,
        _options: &ExtractionOptions,
    ) -> Result<FileFacts, SourceFailure> {
        self.extract(file, settings)
    }

    /// Problems in the configuration inputs this language reads, found without linking (the
    /// engine asks once per update, after the inputs are read). They are listed with the other
    /// input problems and make the graph incomplete.
    fn input_problems(&self, _inputs: &ProjectInputs) -> Vec<InputProblem> {
        Vec::new()
    }

    /// Configuration files this language needs beyond the ones discovery found, such as the
    /// `extends` targets of a `tsconfig.json` chain that live under `node_modules`. The engine
    /// reads the paths it returns that exist, adds them to `inputs`, and asks again until
    /// nothing new is requested (a bounded number of rounds), all before linking.
    fn requested_inputs(&self, _inputs: &ProjectInputs) -> Vec<(RepoPath, InputRole)> {
        Vec::new()
    }

    /// Links this language's files of the program (and nothing else), from their cached facts.
    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a>;

    /// The highlighter for this language's Code view.
    fn highlighter(&self) -> &dyn Highlighter;
}

/// What the shared backend asks one language about the linked program. Evidence can withhold a
/// target, never invent one.
///
/// Queries take IDs from the backend's [`FactsSet`]. A query about an ID of another language,
/// or one the language doesn't recognize, should answer with empty evidence or `None`, which the
/// policy turns into an unresolved outcome. Answers must be deterministic: the same program
/// gives the same answers in the same order, whatever the thread.
pub trait Linked: Send + Sync {
    /// The language these answers are for.
    fn language(&self) -> LanguageId;

    // --- Evidence -------------------------------------------------------------------------

    /// The authoritative evidence for a call site's callee.
    fn callee_evidence(&self, call: CallRef) -> Evidence;

    /// How a call site is invoked, when linking proves more than extraction could. Python keeps
    /// every call a `call` block, but a callee with proven class identity makes its occurrence a
    /// `constructs` edge resolved in the construct context. `None` (the default) keeps the
    /// edge kind and context of the extracted [`CallFact`](crate::facts::CallFact). The answer
    /// must fit the call's block kind (validation checks it).
    fn invocation(&self, _call: CallRef) -> Option<Invocation> {
        None
    }

    /// Evidence for a value: an identifier, a property or literal-key access, or a supported
    /// `this`/`super` value.
    fn value_evidence(&self, value: ValueRef) -> Evidence;

    /// Evidence for a written type name, looked up in the type meaning: its candidate
    /// declarations (a class, interface, type alias, or enum the language can name, through
    /// imports, namespaces, and module members), external origins, and the root's import
    /// binding, as [`Linked::value_evidence`] gives them for values. The default is none, which
    /// the policy turns into an unresolved outcome.
    fn type_evidence(&self, _reference: TypeRef) -> Evidence {
        Evidence::default()
    }

    /// How many of `target`'s leading parameters `call` binds without an argument: 1 for a
    /// Python method called through an instance, `cls`, or `super()`, or an `__init__` reached
    /// by a construction; 0 for a static method or a plain function. `None` when it can't be
    /// proven, which leaves every argument unpaired. The default is 0 (TypeScript, where an
    /// explicit `this` parameter has its own kind).
    fn bound_parameters(&self, _call: CallRef, _target: BlockRef) -> Option<u32> {
        Some(0)
    }

    /// The same question for a callable passed as an argument: how many of `target`'s leading
    /// parameters the passed value binds itself (`obj.method` binds the instance;
    /// `Class.method` binds nothing; `Class.classmethod` binds the class). `None` when it can't
    /// be proven. The default is 0, as for [`Linked::bound_parameters`].
    fn reference_bound_parameters(&self, _value: ValueRef, _target: BlockRef) -> Option<u32> {
        Some(0)
    }

    /// Whether constructing `class` (a class block without a direct constructor) is proven to
    /// take no arguments: it has no base, no decorator that could replace it, and nothing else
    /// that supplies a constructor (a Python `__new__`). Its arguments are then `no_parameter`
    /// rather than `no_signature`. The default proves nothing.
    fn class_takes_no_arguments(&self, _class: BlockRef) -> bool {
        false
    }

    /// Direct written type-reference evidence on a parameter, variable, or property, used by
    /// provenance; `None` when the declaration has no supported annotation.
    fn annotation_evidence(&self, _declaration: DeclRef) -> Option<Evidence> {
        None
    }

    // --- Declarations ---------------------------------------------------------------------

    /// The declaration's concrete body for `context`, if it has one: a non-ambient function,
    /// method, or constructor with a body; a supported constant initializer; for a
    /// construction, a class's direct constructor with a body or else the class; for a
    /// reference or handler, a class itself.
    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget>;

    /// Parameter facts, if the declaration is a parameter or is bound inside one.
    fn parameter(&self, declaration: DeclRef) -> Option<ParameterFact>;

    /// Whether the declaration's binding can be reassigned.
    fn binding_stability(&self, declaration: DeclRef) -> Stability;

    /// Whether the declaration is an accessor (getter or setter).
    fn accessor(&self, declaration: DeclRef) -> bool;

    /// Class-member facts for the override census.
    fn member(&self, declaration: DeclRef) -> MemberFact<'_>;

    /// Whether `signature` is a compatible overload signature of `implementation` (same owner,
    /// name, and declaration category, without a body).
    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool;

    /// What provenance may follow from the declaration.
    fn origin_declaration(&self, _declaration: DeclRef) -> OriginDeclaration {
        OriginDeclaration::default()
    }

    // --- Locators (manual entrance targets) -------------------------------------------------

    /// The top-level declarations of `file` named `name`.
    fn locator_top_level(&self, file: FileId, name: &str) -> Vec<DeclRef>;

    /// The block a locator names for a declaration: a function, class, method, constructor, or
    /// accessor body, or a constant function or class initializer. Locator rules deliberately
    /// differ from the value policy.
    fn locator_concrete(&self, declaration: DeclRef) -> Option<BlockRef>;

    /// The members named `name` of a class or constant object literal.
    fn locator_members(&self, owner: DeclRef, name: &str) -> LocatorMembers;

    /// Whether `signature` is an overload signature of `implementation`, by locator rules.
    fn locator_overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool;

    // --- Modules and classes ----------------------------------------------------------------

    /// How each runtime import of `file` resolved, aligned with the file's
    /// [`ImportFact`](crate::facts::ImportFact)s (same order and length).
    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget];

    /// Whether the program keeps a conditional `import` block of this language's files
    /// ([`GraphFacts::conditional`](crate::facts::GraphFacts::conditional)), such as a Python
    /// named child import whose attribute is proven absent from its package. Unkept blocks are
    /// dropped from the graph after resolution. The default keeps none: a language that marks
    /// blocks conditional must say which survive. [`Linked::module_dependencies`] still answers
    /// for every import fact, kept or not, and the census lists only kept ones.
    fn keeps_import(&self, _block: BlockRef) -> bool {
        false
    }

    /// The runtime exports of `file`, sorted by name, after re-exports are followed.
    fn module_exports(&self, file: FileId) -> &[Export];

    /// Every class of this language's files, for the override census.
    fn classes(&self) -> &[ClassShape];

    // --- Module labels ----------------------------------------------------------------------

    /// Whether `specifier`, written in `file`, names a package (not a relative or absolute path).
    fn is_package_specifier(&self, file: FileId, specifier: &str) -> bool;

    /// The module label for a package specifier written in `file` (`express/router` names
    /// `express`; `fs` names `node:fs`).
    fn label_for_specifier(&self, file: FileId, specifier: &str) -> String;

    /// The module label for an external declaration location seen from `file`.
    fn label_for_location(&self, file: FileId, location: &ExternalLocation) -> String;

    /// The label for an undeclared, unimported root such as `console` (`global`, or
    /// `python:builtins`).
    fn global_label(&self, file: FileId) -> String;

    // --- Inputs and launches -----------------------------------------------------------------

    /// The configuration inputs (from [`ProjectInputs::inputs`]) that shaped this language's
    /// part of the graph, such as the `tsconfig.json` chains it applied. They are recorded as
    /// the graph's inputs, with `unfold.toml` and every input an entrance cites as
    /// evidence.
    fn inputs_read(&self) -> Vec<RepoPath> {
        Vec::new()
    }

    /// The source an emitted output path comes from, for launch targets such as
    /// `node dist/server.js`.
    fn emitted_source(&self, _output: &RepoPath) -> EmitSource {
        EmitSource::Unknown
    }

    // --- Entrances and validation -----------------------------------------------------------

    /// This language's entrance detectors, in the fixed order they run.
    fn detectors(&self) -> Vec<Box<dyn EntranceDetector + '_>> {
        Vec::new()
    }

    /// The independent census of token boundaries, call sites, and module dependencies that
    /// validation compares this language's files with.
    fn census(&self) -> Option<&dyn Census> {
        None
    }

    /// Problems linking found, in a deterministic order. The engine lists them with the other
    /// problems of the update that linked (and keeps them until the next link); one marked
    /// [`LanguageProblem::incomplete`] makes the graph incomplete. The default is none.
    fn problems(&self) -> Vec<LanguageProblem> {
        Vec::new()
    }
}
