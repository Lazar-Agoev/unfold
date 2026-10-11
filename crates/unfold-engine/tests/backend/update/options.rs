//! Extraction options from project inputs, context files, and input problems: a configuration
//! edit re-extracts exactly the files whose options changed, languages see the declaration files
//! discovery found, and problems a language finds in its inputs are listed.

use std::sync::{Arc, Mutex};

use unfold_core::highlight::Highlighter;
use unfold_core::{Graph, RepoPath, Snapshot};
use unfold_engine::settings::{Settings, SettingsHash, SettingsHasher};
use unfold_engine::update::{DiskState, MemoryFactsCache, RestoreStats};
use unfold_engine::{
    Engine, ExtractionOptions, FactsSet, FileFacts, InputProblem, Language, LanguageId, Linked, ProblemKind,
    Producer, ProjectInputs, SourceFailure, UpdateRequest,
};

use super::{Counting, Folder, background, canonical, compile, options, template};

/// The alien language with options from the nearest `tsconfig.json`: a file's options are that
/// file's text (or empty), and `extract_with` checks it received them.
struct Configured {
    inner: Arc<Counting>,
    /// The paths extracted, in order of extraction (sorted by each test).
    extracted: Mutex<Vec<String>>,
    /// The context files the last link saw.
    context: Mutex<Vec<String>>,
}

impl Configured {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Counting::new(),
            extracted: Mutex::new(Vec::new()),
            context: Mutex::new(Vec::new()),
        })
    }

    fn take_extracted(&self) -> Vec<String> {
        let mut paths = std::mem::take(&mut *self.extracted.lock().expect("lock"));
        paths.sort();
        paths
    }

    fn nearest<'a>(path: &RepoPath, inputs: &'a ProjectInputs) -> &'a str {
        let mut folder = path.as_str().rsplit_once('/').map_or("", |(folder, _)| folder);
        loop {
            let candidate = if folder.is_empty() {
                "tsconfig.json".to_owned()
            } else {
                format!("{folder}/tsconfig.json")
            };
            if let Some(input) = RepoPath::new(&candidate)
                .ok()
                .and_then(|candidate| inputs.inputs.get(&candidate))
            {
                return input.snapshot.text();
            }
            match folder.rsplit_once('/') {
                Some((parent, _)) => folder = parent,
                None if !folder.is_empty() => folder = "",
                None => return "",
            }
        }
    }
}

impl Language for Configured {
    fn id(&self) -> LanguageId {
        self.inner.id()
    }

    fn producer(&self) -> Producer {
        self.inner.producer()
    }

    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        self.inner.owns(path, settings)
    }

    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash {
        self.inner.settings_hash(path, settings)
    }

    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure> {
        self.inner.extract(file, settings)
    }

    fn extraction_options(
        &self,
        paths: &[&RepoPath],
        _settings: &Settings,
        inputs: &ProjectInputs,
    ) -> Vec<ExtractionOptions> {
        paths
            .iter()
            .map(|path| {
                let text: Arc<str> = Self::nearest(path, inputs).into();
                let hash = SettingsHasher::new("test.options").add(&*text).finish();
                ExtractionOptions::with(hash, Arc::new(text))
            })
            .collect()
    }

    fn extract_with(
        &self,
        file: &Snapshot,
        settings: &Settings,
        options: &ExtractionOptions,
    ) -> Result<FileFacts, SourceFailure> {
        assert!(
            options.get::<Arc<str>>().is_some(),
            "the options come back to the language"
        );
        self.extracted
            .lock()
            .expect("lock")
            .push(file.path().as_str().to_owned());
        self.inner.extract(file, settings)
    }

    fn input_problems(&self, inputs: &ProjectInputs) -> Vec<InputProblem> {
        inputs
            .inputs
            .iter()
            .filter(|(_, input)| input.snapshot.text().contains("broken"))
            .map(|(path, _)| InputProblem {
                path: path.clone(),
                message: "this configuration is broken".into(),
            })
            .collect()
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        *self.context.lock().expect("lock") = inputs
            .context
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect();
        assert!(inputs.root.is_some(), "a real folder's inputs know their root");
        self.inner.link(files, inputs)
    }

    fn highlighter(&self) -> &dyn Highlighter {
        self.inner.highlighter()
    }
}

fn engine(folder: &Folder, language: Arc<Configured>) -> Engine {
    engine_over(folder, language, Arc::new(MemoryFactsCache::new()))
}

fn engine_over(folder: &Folder, language: Arc<Configured>, cache: Arc<MemoryFactsCache>) -> Engine {
    Engine::new(
        folder.path(),
        vec![language as Arc<dyn Language>],
        cache,
        options(2),
    )
    .expect("engine")
}

/// A new session over `cache`, restored from `engine`'s graph and disk state.
fn reopen(
    folder: &Folder,
    language: &Arc<Configured>,
    cache: &Arc<MemoryFactsCache>,
    engine: &Engine,
) -> (Engine, RestoreStats) {
    let graph: Graph = (**engine.graph()).clone();
    let disks: Vec<(RepoPath, DiskState)> = graph
        .files
        .iter()
        .filter_map(|file| Some((file.file.path.clone(), engine.disk_state(&file.file.path)?)))
        .collect();
    let mut reopened = engine_over(folder, Arc::clone(language), Arc::clone(cache));
    let stats = reopened
        .restore(graph, Default::default(), disks, None)
        .expect("restore");
    (reopened, stats)
}

fn project(folder: &Folder) {
    folder.write("tsconfig.json", "{}");
    folder.write("a/tsconfig.json", "{\"a\": 1}");
    folder.write("main.ts", &template(0, 0));
    folder.write("a/one.ts", &template(0, 1));
    folder.write("a/deep/two.ts", &template(0, 2));
    folder.write("b/three.ts", &template(0, 1));
}

#[test]
fn a_configuration_change_re_extracts_exactly_the_files_whose_options_changed() {
    let folder = Folder::new();
    project(&folder);
    let language = Configured::new();
    let mut engine = engine(&folder, Arc::clone(&language));
    compile(&mut engine);
    assert_eq!(
        language.take_extracted(),
        ["a/deep/two.ts", "a/one.ts", "b/three.ts", "main.ts"]
    );

    // Nothing changed: nothing is extracted.
    compile(&mut engine);
    assert!(language.take_extracted().is_empty());

    // The nested configuration changes the options of the files beneath it only.
    folder.write("a/tsconfig.json", "{\"a\": 2}");
    let outcome = background(&mut engine, &["a/tsconfig.json"]);
    assert_eq!(language.take_extracted(), ["a/deep/two.ts", "a/one.ts"]);
    assert_eq!(outcome.stats.files_extracted, 2);

    // A new configuration deeper down takes over its folder.
    folder.write("a/deep/tsconfig.json", "{\"deep\": true}");
    background(&mut engine, &["a/deep/tsconfig.json"]);
    assert_eq!(language.take_extracted(), ["a/deep/two.ts"]);

    // Editing a source under unchanged options extracts it with its options.
    folder.write("b/three.ts", &template(8, 1));
    background(&mut engine, &["b/three.ts"]);
    assert_eq!(language.take_extracted(), ["b/three.ts"]);

    // Removing a configuration falls back to the next one up.
    folder.remove("a/deep/tsconfig.json");
    background(&mut engine, &["a/deep/tsconfig.json"]);
    assert_eq!(language.take_extracted(), ["a/deep/two.ts"]);

    // Changing the root configuration touches the files that use it.
    folder.write("tsconfig.json", "{\"root\": 1}");
    background(&mut engine, &["tsconfig.json"]);
    assert_eq!(language.take_extracted(), ["b/three.ts", "main.ts"]);
}

/// Facts are cached under each file's extraction-options hash, so a restore looks them up by
/// that hash: reopening an unchanged folder reads and extracts nothing, and a configuration
/// edit made while the folder was closed re-extracts exactly the files whose options changed.
#[test]
fn reopening_restores_facts_cached_under_extraction_options() {
    let folder = Folder::new();
    project(&folder);
    folder.write("a/broken.ts", "(fn");
    let cache = Arc::new(MemoryFactsCache::new());
    let language = Configured::new();
    let mut engine = engine_over(&folder, Arc::clone(&language), Arc::clone(&cache));
    let first = compile(&mut engine);
    language.take_extracted();

    let (mut reopened, restored) = reopen(&folder, &language, &cache, &engine);
    assert_eq!((restored.files, restored.restored, restored.missing), (5, 5, 0));
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(outcome.stats.files_read, 0, "no file is read again");
    assert_eq!(outcome.stats.inputs_read, 0, "no input is read again");
    assert_eq!(
        (outcome.stats.files_extracted, outcome.stats.files_from_cache),
        (0, 0)
    );
    assert!(language.take_extracted().is_empty());
    assert_eq!(canonical(&outcome), canonical(&first));

    // The nested configuration changes while the folder is closed: the files beneath it miss
    // the cache on restore, and the reopen check extracts exactly those.
    drop(reopened);
    folder.write("a/tsconfig.json", "{\"a\": 2}");
    let (mut reopened, restored) = reopen(&folder, &language, &cache, &engine);
    assert_eq!((restored.restored, restored.missing), (3, 2));
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(language.take_extracted(), ["a/deep/two.ts", "a/one.ts"]);
    assert_eq!(outcome.stats.files_extracted, 2);
    assert_eq!(outcome.stats.files_read, 2);
    assert_eq!(
        outcome.stats.inputs_read, 0,
        "restore read the changed input already"
    );
}

#[test]
fn languages_see_context_files_and_report_input_problems() {
    let folder = Folder::new();
    project(&folder);
    folder.write("types/globals.d.ts", "declare const x: number;");
    let language = Configured::new();
    let mut engine = engine(&folder, Arc::clone(&language));
    let first = compile(&mut engine);
    assert_eq!(*language.context.lock().expect("lock"), ["types/globals.d.ts"]);
    assert!(first.complete);
    assert!(first.problems.is_empty());

    // A new declaration file relinks, and the language sees it.
    folder.write("lib/extra.d.mts", "export {};");
    let added = background(&mut engine, &["lib/extra.d.mts"]);
    assert!(added.stats.relinked);
    assert_eq!(
        *language.context.lock().expect("lock"),
        ["lib/extra.d.mts", "types/globals.d.ts"]
    );
    folder.remove("types/globals.d.ts");
    background(&mut engine, &["types/globals.d.ts"]);
    assert_eq!(*language.context.lock().expect("lock"), ["lib/extra.d.mts"]);

    // A problem the language finds in an input is listed and makes the graph incomplete,
    // until the input is fixed.
    folder.write("a/tsconfig.json", "{\"broken\": true}");
    let broken = background(&mut engine, &["a/tsconfig.json"]);
    assert!(!broken.complete);
    assert_eq!(broken.problems.len(), 1);
    assert_eq!(broken.problems[0].kind, ProblemKind::Input);
    assert_eq!(&*broken.problems[0].path, "a/tsconfig.json");
    folder.write("a/tsconfig.json", "{}");
    let fixed = background(&mut engine, &["a/tsconfig.json"]);
    assert!(fixed.complete);
    assert!(fixed.problems.is_empty());
}

#[test]
fn project_inputs_know_what_exists() {
    let folder = Folder::new();
    folder.write("ignored/kept.js", "");
    let inputs = ProjectInputs {
        unindexed: vec![RepoPath::new("legacy/a.ts").expect("path")],
        context: vec![RepoPath::new("types/a.d.ts").expect("path")],
        root: Some(folder.path().to_owned()),
        ..ProjectInputs::default()
    };
    let exists = |path: &str| inputs.exists(&RepoPath::new(path).expect("path"));
    assert!(exists("legacy/a.ts"));
    assert!(exists("types/a.d.ts"));
    assert!(exists("ignored/kept.js"));
    assert!(exists("ignored"));
    assert!(!exists("ignored/missing.js"));
    let memory = ProjectInputs::default();
    assert!(!memory.exists(&RepoPath::new("ignored/kept.js").expect("path")));
}
