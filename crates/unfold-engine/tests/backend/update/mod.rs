//! `update()` end to end over real temporary folders, with the alien language registered as an
//! `Arc<dyn Language>` (its files are named `.ts` so discovery classifies them as sources).

mod cache;
mod incremental;
mod lifecycle;
mod options;

use std::cell::Cell;
use std::fs::FileTimes;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tempfile::TempDir;
use unfold_core::highlight::Highlighter;
use unfold_core::validate::Census;
use unfold_core::{FileGraph, RepoPath, Snapshot};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::entrances::EntranceDetector;
use unfold_engine::evidence::{
    ClassShape, ConcreteTarget, Evidence, Export, ExternalLocation, LocatorMembers, MemberFact, ModuleTarget,
    OriginDeclaration, ParameterFact, Stability, ValueContext,
};
use unfold_engine::settings::{Settings, SettingsHash};
use unfold_engine::update::{MemoryFactsCache, UpdateOutcome};
use unfold_engine::{
    BlockRef, CallRef, CancelToken, DeclRef, EmitSource, Engine, EngineOptions, FactsSet, FileFacts, FileId,
    Invocation, Language, LanguageId, LanguageProblem, Linked, Producer, ProjectInputs, SourceFailure, Stage,
    UpdateRequest, ValueRef,
};

use crate::alien::Alien;

/// A temporary folder whose writes get strictly increasing modification times, so the cheap
/// size-and-time check always notices them.
pub struct Folder {
    dir: TempDir,
    clock: Cell<u64>,
}

impl Folder {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temporary folder"),
            clock: Cell::new(0),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, path: &str, text: &str) {
        let full = self.path().join(path);
        std::fs::create_dir_all(full.parent().expect("a parent folder")).expect("create folders");
        std::fs::write(&full, text).expect("write file");
        self.touch(path);
    }

    /// Gives `path` a new modification time without changing its bytes.
    pub fn touch(&self, path: &str) {
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + tick);
        let file = std::fs::File::options()
            .write(true)
            .open(self.path().join(path))
            .expect("open file");
        file.set_times(FileTimes::new().set_modified(time))
            .expect("set times");
    }

    pub fn remove(&self, path: &str) {
        let full = self.path().join(path);
        if full.is_dir() {
            std::fs::remove_dir_all(full).expect("remove folder");
        } else if full.exists() {
            std::fs::remove_file(full).expect("remove file");
        }
    }
}

/// The alien language with an extraction counter and test switches.
#[derive(Default)]
pub struct Counting {
    pub extractions: AtomicUsize,
    /// Hash each file's owning `[[python.project]]` into its settings hash.
    pub settings_aware: bool,
    /// Time each extraction takes, to make cancellation observable.
    pub delay: Duration,
    /// Make linking break its contract (module dependencies that don't match the imports).
    pub break_link: AtomicBool,
    /// A cancellation armed for a late stage.
    pub trip: Trip,
}

impl Counting {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn count(&self) -> usize {
        self.extractions.swap(0, Ordering::SeqCst)
    }
}

impl Language for Counting {
    fn id(&self) -> LanguageId {
        Alien.id()
    }

    fn producer(&self) -> Producer {
        Alien.producer()
    }

    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        Alien.owns(path, settings)
    }

    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash {
        if self.settings_aware {
            settings.python_hash(path)
        } else {
            Alien.settings_hash(path, settings)
        }
    }

    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure> {
        self.extractions.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Alien.extract(file, settings)
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        self.trip.fire(Stage::Link);
        Box::new(Wrapped {
            inner: Alien.link(files, inputs),
            broken: self.break_link.load(Ordering::SeqCst),
            trip: &self.trip,
        })
    }

    fn highlighter(&self) -> &dyn Highlighter {
        Alien.highlighter()
    }
}

/// A cancellation armed for one stage: the first time an update reaches it, the token is
/// cancelled and the moment recorded, so tests can measure how soon the update stops.
#[derive(Default)]
pub struct Trip {
    armed: Mutex<Option<(Stage, CancelToken)>>,
    fired: Mutex<Option<Instant>>,
}

impl Trip {
    /// Cancels `cancel` when an update next reaches `stage`.
    pub fn arm(&self, stage: Stage, cancel: CancelToken) {
        *self.armed.lock().expect("lock") = Some((stage, cancel));
        *self.fired.lock().expect("lock") = None;
    }

    /// When the armed cancellation fired, if it did.
    pub fn fired(&self) -> Option<Instant> {
        *self.fired.lock().expect("lock")
    }

    fn fire(&self, stage: Stage) {
        let mut armed = self.armed.lock().expect("lock");
        if armed.as_ref().is_some_and(|(at, _)| *at == stage)
            && let Some((_, cancel)) = armed.take()
        {
            *self.fired.lock().expect("lock") = Some(Instant::now());
            cancel.cancel();
        }
    }
}

/// The alien language's linked answers, delegated in full, with two test switches: `broken`
/// makes module dependencies never line up with the imports (a contract break the backend
/// reports as an error), and `trip` fires when an update reaches resolve, detect, or validate.
struct Wrapped<'a> {
    inner: Box<dyn Linked + 'a>,
    broken: bool,
    trip: &'a Trip,
}

impl Linked for Wrapped<'_> {
    fn language(&self) -> LanguageId {
        self.inner.language()
    }
    fn callee_evidence(&self, call: CallRef) -> Evidence {
        self.trip.fire(Stage::Resolve);
        self.inner.callee_evidence(call)
    }
    fn value_evidence(&self, value: ValueRef) -> Evidence {
        self.inner.value_evidence(value)
    }
    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        self.inner.concrete_target(declaration, context)
    }
    fn parameter(&self, declaration: DeclRef) -> Option<ParameterFact> {
        self.inner.parameter(declaration)
    }
    fn binding_stability(&self, declaration: DeclRef) -> Stability {
        self.inner.binding_stability(declaration)
    }
    fn accessor(&self, declaration: DeclRef) -> bool {
        self.inner.accessor(declaration)
    }
    fn member(&self, declaration: DeclRef) -> MemberFact<'_> {
        self.inner.member(declaration)
    }
    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.inner.overload_of(signature, implementation)
    }
    fn locator_top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        self.inner.locator_top_level(file, name)
    }
    fn locator_concrete(&self, declaration: DeclRef) -> Option<BlockRef> {
        self.inner.locator_concrete(declaration)
    }
    fn locator_members(&self, owner: DeclRef, name: &str) -> LocatorMembers {
        self.inner.locator_members(owner, name)
    }
    fn locator_overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.inner.locator_overload_of(signature, implementation)
    }
    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        if self.broken {
            &[]
        } else {
            self.inner.module_dependencies(file)
        }
    }
    fn module_exports(&self, file: FileId) -> &[Export] {
        self.inner.module_exports(file)
    }
    fn classes(&self) -> &[ClassShape] {
        self.inner.classes()
    }
    fn is_package_specifier(&self, file: FileId, specifier: &str) -> bool {
        self.inner.is_package_specifier(file, specifier)
    }
    fn label_for_specifier(&self, file: FileId, specifier: &str) -> String {
        self.inner.label_for_specifier(file, specifier)
    }
    fn label_for_location(&self, file: FileId, location: &ExternalLocation) -> String {
        self.inner.label_for_location(file, location)
    }
    fn global_label(&self, file: FileId) -> String {
        self.inner.global_label(file)
    }
    fn detectors(&self) -> Vec<Box<dyn EntranceDetector + '_>> {
        self.trip.fire(Stage::Detect);
        self.inner.detectors()
    }
    fn census(&self) -> Option<&dyn Census> {
        self.trip.fire(Stage::Validate);
        self.inner.census()
    }
    fn invocation(&self, call: CallRef) -> Option<Invocation> {
        self.inner.invocation(call)
    }
    fn annotation_evidence(&self, declaration: DeclRef) -> Option<Evidence> {
        self.inner.annotation_evidence(declaration)
    }
    fn origin_declaration(&self, declaration: DeclRef) -> OriginDeclaration {
        self.inner.origin_declaration(declaration)
    }
    fn keeps_import(&self, block: BlockRef) -> bool {
        self.inner.keeps_import(block)
    }
    fn inputs_read(&self) -> Vec<RepoPath> {
        self.inner.inputs_read()
    }
    fn emitted_source(&self, output: &RepoPath) -> EmitSource {
        self.inner.emitted_source(output)
    }
    fn problems(&self) -> Vec<LanguageProblem> {
        self.inner.problems()
    }
}

/// Options for deterministic tests: `threads` everywhere, no global Git excludes.
pub fn options(threads: usize) -> EngineOptions {
    EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: threads,
        background_threads: threads,
        ..EngineOptions::default()
    }
}

/// An engine over `folder` with `language` and `cache`.
pub fn engine_with(
    folder: &Folder,
    language: Arc<Counting>,
    cache: Arc<MemoryFactsCache>,
    threads: usize,
) -> Engine {
    Engine::new(
        folder.path(),
        vec![language as Arc<dyn Language>],
        cache,
        options(threads),
    )
    .expect("engine")
}

/// An engine with a fresh counting language and cache.
pub fn engine(folder: &Folder) -> Engine {
    engine_with(folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2)
}

pub fn compile(engine: &mut Engine) -> UpdateOutcome {
    engine.update(UpdateRequest::compile()).expect("compile")
}

pub fn background(engine: &mut Engine, paths: &[&str]) -> UpdateOutcome {
    let paths = paths
        .iter()
        .map(|path| RepoPath::new(path).expect("valid path"))
        .collect();
    engine
        .update(UpdateRequest::background(paths))
        .expect("background update")
}

pub fn canonical(outcome: &UpdateOutcome) -> String {
    unfold_core::canonical::to_string(&outcome.graph)
}

pub fn file<'g>(outcome: &'g UpdateOutcome, path: &str) -> &'g FileGraph {
    outcome
        .graph
        .file(path)
        .unwrap_or_else(|| panic!("no file {path}"))
}

/// Alien sources that reference each other, written relative to a file `depth` folders deep.
pub fn template(index: usize, depth: usize) -> String {
    let up = if depth == 0 {
        "./".to_owned()
    } else {
        "../".repeat(depth)
    };
    match index {
        0 => "(fn helper (param cb) (call (ref cb)))".to_owned(),
        1 => format!("(use helper \"{up}lib/util\") (fn go) (stmt (call (ref helper) (ref go)))"),
        2 => "(class Base (method ping) (method run (stmt (call (get (this) ping)))))".to_owned(),
        3 => format!("(use Base \"{up}lib/base\") (class Derived :base=Base (method ping) (ctor (stmt (super (ref Base)))))"),
        4 => "(use express \"express\" :export=default) (const app (call (ref express))) (fn handle) (stmt (call (get (ref app) post) \"/orders\" (ref handle)))".to_owned(),
        5 => format!(
            "(import \"{up}jobs/run\") (import \"{up}gone\") (use helper \"{up}lib/util\" (import \"{up}lib/base\" :when-indexed) (import \"{up}gone\" :when-indexed) (import \"{up}main\" :when-indexed)) (fn f (param x) (call (ref x)))"
        ),
        6 => "(fn".to_owned(),
        7 => "(call (ref x)".to_owned(),
        _ => format!("(use helper \"{up}lib/util\") (fn f{index} (param x) (call (ref f{index})) (call (ref helper) (ref f{index})))"),
    }
}

/// Whether [`template`] `index` doesn't parse.
pub fn is_broken(index: usize) -> bool {
    matches!(index, 6 | 7)
}
