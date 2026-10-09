//! End to end through the engine: on the `ts-orders` playground, launch targets go through the
//! emit map (`node dist/server.js` starts `src/server.ts`), manual locators resolve through
//! exports and top-level declarations, every import edge matches the reference, and the
//! `tsconfig.json` is a graph input; in a folder being edited, a `tsconfig.json` change
//! re-extracts exactly the files whose options changed.

use std::sync::Arc;

use unfold_core::highlight::Highlighter;
use unfold_core::testkit::testdata_dir;
use unfold_core::text::ByteRange;
use unfold_core::validate::Census;
use unfold_core::vocab::{DetectorId, EntranceKind, InputRole, Resolution};
use unfold_core::{Graph, RepoPath, Snapshot, canonical};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::entrances::{
    CandidateEvidence, DetectContext, EntranceCandidate, EntranceDetector, TargetCandidate, Trigger,
};
use unfold_engine::evidence::{
    ClassShape, ConcreteTarget, Evidence, Export, ExternalLocation, LocatorMembers, MemberFact, ModuleTarget,
    ParameterFact, Stability, ValueContext,
};
use unfold_engine::settings::{Settings, SettingsHash};
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{
    BlockRef, CallRef, DeclRef, EmitSource, Engine, EngineOptions, ExtractionOptions, FactsSet, FileFacts,
    FileId, InputProblem, Language, LanguageId, Linked, Producer, ProjectInputs, SourceFailure,
    UpdateRequest, ValueRef,
};
use unfold_lang_typescript::TypeScript;

/// TypeScript plus a stand-in for the package-script detector (not built yet) that launches
/// `dist/server.js` and `dist/cli.js` with the evidence the reference records.
struct WithLaunches(TypeScript);

struct Launches;

impl EntranceDetector for Launches {
    fn id(&self) -> DetectorId {
        DetectorId::PackageScript
    }

    fn detect(&self, _context: &DetectContext<'_>) -> Vec<EntranceCandidate> {
        let launch = |trigger: &str, start: u32, end: u32, pointer: &str, path: &str| EntranceCandidate {
            detector: DetectorId::PackageScript,
            kind: EntranceKind::Process,
            method: None,
            trigger: Trigger::Exact(trigger.into()),
            label: None,
            evidence: CandidateEvidence::Input {
                path: RepoPath::new("package.json").expect("path"),
                range: ByteRange { start, end },
                pointer: pointer.into(),
            },
            targets: vec![TargetCandidate::Launch {
                path: path.into(),
                cwd: "".into(),
            }],
        };
        vec![
            launch("start", 187, 217, "/scripts/start", "dist/server.js"),
            launch("orders", 83, 106, "/bin/orders", "dist/cli.js"),
            launch(
                "legacy:export",
                392,
                433,
                "/scripts/legacy:export",
                "scripts/export",
            ),
        ]
    }
}

struct Linking<'a>(Box<dyn Linked + 'a>);

impl Linked for Linking<'_> {
    fn language(&self) -> LanguageId {
        self.0.language()
    }
    fn callee_evidence(&self, call: CallRef) -> Evidence {
        self.0.callee_evidence(call)
    }
    fn value_evidence(&self, value: ValueRef) -> Evidence {
        self.0.value_evidence(value)
    }
    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        self.0.concrete_target(declaration, context)
    }
    fn parameter(&self, declaration: DeclRef) -> Option<ParameterFact> {
        self.0.parameter(declaration)
    }
    fn binding_stability(&self, declaration: DeclRef) -> Stability {
        self.0.binding_stability(declaration)
    }
    fn accessor(&self, declaration: DeclRef) -> bool {
        self.0.accessor(declaration)
    }
    fn member(&self, declaration: DeclRef) -> MemberFact<'_> {
        self.0.member(declaration)
    }
    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.0.overload_of(signature, implementation)
    }
    fn locator_top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        self.0.locator_top_level(file, name)
    }
    fn locator_concrete(&self, declaration: DeclRef) -> Option<BlockRef> {
        self.0.locator_concrete(declaration)
    }
    fn locator_members(&self, owner: DeclRef, name: &str) -> LocatorMembers {
        self.0.locator_members(owner, name)
    }
    fn locator_overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.0.locator_overload_of(signature, implementation)
    }
    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        self.0.module_dependencies(file)
    }
    fn module_exports(&self, file: FileId) -> &[Export] {
        self.0.module_exports(file)
    }
    fn classes(&self) -> &[ClassShape] {
        self.0.classes()
    }
    fn is_package_specifier(&self, file: FileId, specifier: &str) -> bool {
        self.0.is_package_specifier(file, specifier)
    }
    fn label_for_specifier(&self, file: FileId, specifier: &str) -> String {
        self.0.label_for_specifier(file, specifier)
    }
    fn label_for_location(&self, file: FileId, location: &ExternalLocation) -> String {
        self.0.label_for_location(file, location)
    }
    fn global_label(&self, file: FileId) -> String {
        self.0.global_label(file)
    }
    fn inputs_read(&self) -> Vec<RepoPath> {
        self.0.inputs_read()
    }
    fn emitted_source(&self, output: &RepoPath) -> EmitSource {
        self.0.emitted_source(output)
    }
    fn detectors(&self) -> Vec<Box<dyn EntranceDetector + '_>> {
        vec![Box::new(Launches)]
    }
    fn census(&self) -> Option<&dyn Census> {
        self.0.census()
    }
}

impl Language for WithLaunches {
    fn id(&self) -> LanguageId {
        self.0.id()
    }
    fn producer(&self) -> Producer {
        self.0.producer()
    }
    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        self.0.owns(path, settings)
    }
    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash {
        self.0.settings_hash(path, settings)
    }
    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure> {
        self.0.extract(file, settings)
    }
    fn extraction_options(
        &self,
        paths: &[&RepoPath],
        settings: &Settings,
        inputs: &ProjectInputs,
    ) -> Vec<ExtractionOptions> {
        self.0.extraction_options(paths, settings, inputs)
    }
    fn extract_with(
        &self,
        file: &Snapshot,
        settings: &Settings,
        options: &ExtractionOptions,
    ) -> Result<FileFacts, SourceFailure> {
        self.0.extract_with(file, settings, options)
    }
    fn input_problems(&self, inputs: &ProjectInputs) -> Vec<InputProblem> {
        self.0.input_problems(inputs)
    }
    fn requested_inputs(&self, inputs: &ProjectInputs) -> Vec<(RepoPath, InputRole)> {
        self.0.requested_inputs(inputs)
    }
    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        Box::new(Linking(self.0.link(files, inputs)))
    }
    fn highlighter(&self) -> &dyn Highlighter {
        self.0.highlighter()
    }
}

fn compile_playground() -> Graph {
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        ..EngineOptions::default()
    };
    let language: Arc<dyn Language> = Arc::new(WithLaunches(TypeScript::new()));
    let mut engine = Engine::new(
        &testdata_dir().join("playgrounds/ts-orders"),
        vec![language],
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("engine");
    let outcome = engine.update(UpdateRequest::compile()).expect("compile");
    assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
    outcome.graph.as_ref().clone()
}

#[test]
fn the_playground_compiles_end_to_end() {
    let graph = compile_playground();
    let target = |trigger: &str| {
        let entrance = graph
            .entrances
            .iter()
            .find(|entrance| entrance.trigger.as_deref() == Some(trigger))
            .unwrap_or_else(|| panic!("no entrance {trigger}"));
        let target = &entrance.targets[0];
        (
            target.resolution,
            target.target.as_ref().map(|key| key.as_str().to_owned()),
            target.reason,
        )
    };
    // Launches go through the emit map.
    assert_eq!(
        target("start"),
        (Resolution::Resolved, Some("src/server.ts#".to_owned()), None)
    );
    assert_eq!(
        target("orders"),
        (Resolution::Resolved, Some("src/cli.ts#".to_owned()), None)
    );
    // C-5: the extensionless `scripts/export` probes `scripts/export.js`, which exists.
    assert_eq!(
        target("legacy:export"),
        (
            Resolution::Unresolved,
            None,
            Some(unfold_core::vocab::Reason::NotIndexed)
        )
    );
    // Manual locators resolve through exports and top-level declarations, as in the reference.
    let reference_text =
        std::fs::read_to_string(testdata_dir().join("reference/canonical/playground-ts-native.jsonl"))
            .expect("reference");
    let reference = canonical::read(&reference_text).expect("reference parses");
    for entrance in reference
        .entrances
        .iter()
        .filter(|entrance| entrance.detector == DetectorId::Manual)
    {
        let actual = graph
            .entrances
            .iter()
            .find(|candidate| candidate.key == entrance.key)
            .unwrap_or_else(|| panic!("no entrance {}", entrance.key.as_str()));
        assert_eq!(actual.targets, entrance.targets, "{}", entrance.key.as_str());
    }
    // The configuration that shaped the graph is one of its inputs.
    let inputs: Vec<(&str, InputRole)> = graph
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), input.role))
        .collect();
    assert!(
        inputs.contains(&("tsconfig.json", InputRole::Tsconfig)),
        "{inputs:?}"
    );
    // Every import edge matches the reference.
    let imports = |graph: &Graph| -> Vec<String> {
        graph
            .files
            .iter()
            .flat_map(|file| {
                file.edges
                    .iter()
                    .filter(|edge| edge.kind == unfold_core::vocab::EdgeKind::Imports)
                    .map(move |edge| format!("{} {:?}", file.blocks[edge.source as usize].key.as_str(), edge))
            })
            .collect()
    };
    let (actual, expected) = (imports(&graph), imports(&reference));
    assert_eq!(actual.len(), 76);
    assert_eq!(actual, expected);
}

fn write(root: &std::path::Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("folders");
    std::fs::write(full, text).expect("write");
}

#[test]
fn a_tsconfig_change_re_extracts_exactly_the_files_it_affects() {
    let folder = tempfile::tempdir().expect("temporary folder");
    let root = folder.path();
    write(root, "tsconfig.json", "{}");
    write(root, "strict/tsconfig.json", r#"{"extends": "../tsconfig.json"}"#);
    write(root, "a.ts", "import {} from './b';\nexport const a = 1;");
    write(root, "b.ts", "export const b = 1;");
    write(root, "strict/c.ts", "import {} from '../b';");
    write(root, "strict/deep/d.ts", "import {} from '../../b';");
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        ..EngineOptions::default()
    };
    let language: Arc<dyn Language> = Arc::new(TypeScript::new());
    let mut engine =
        Engine::new(root, vec![language], Arc::new(MemoryFactsCache::new()), options).expect("engine");
    let imports = |graph: &Graph| -> usize {
        graph
            .files
            .iter()
            .flat_map(|file| &file.edges)
            .filter(|edge| edge.kind == unfold_core::vocab::EdgeKind::Imports)
            .count()
    };
    let first = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(first.stats.files_extracted, 4);
    assert_eq!(imports(&first.graph), 0, "empty imports are erased");

    // The nested configuration turns on verbatimModuleSyntax: only its two files change.
    write(
        root,
        "strict/tsconfig.json",
        r#"{"extends": "../tsconfig.json", "compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    let changed = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(changed.stats.files_extracted, 2);
    assert_eq!(imports(&changed.graph), 2);

    // Through the chain: the root configuration turns it on for everything; only `a.ts` and
    // `b.ts` change (the strict files already had it).
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    let root_changed = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(root_changed.stats.files_extracted, 2);
    assert_eq!(imports(&root_changed.graph), 3);
    let inputs: Vec<&str> = root_changed
        .graph
        .inputs
        .iter()
        .map(|input| input.path.as_str())
        .collect();
    assert_eq!(inputs, ["strict/tsconfig.json", "tsconfig.json"]);

    // A missing base is a problem, not a failure.
    write(
        root,
        "strict/tsconfig.json",
        r#"{"extends": "@tsconfig/missing"}"#,
    );
    let broken = engine.update(UpdateRequest::compile()).expect("compile");
    assert!(!broken.complete);
    assert_eq!(broken.problems.len(), 1, "{:?}", broken.problems);
    assert_eq!(&*broken.problems[0].path, "strict/tsconfig.json");
}
