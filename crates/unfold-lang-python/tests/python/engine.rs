//! The census, determinism, incremental compiles, and the facts cache: every call in the parse
//! tree is exactly one call node and every kept module dependency one `imports` link; the graph
//! doesn't depend on file order or thread count; an incremental update equals a full compile of
//! the same folder; and a cached payload Python can't read is a cache miss.

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_core::vocab::{LinkKind, NodeKind};
use unfold_core::{ByteRange, ContentHash, RepoPath, Snapshot};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::settings::{PythonVersion, Settings, SettingsHash};
use unfold_engine::update::{CacheEntry, CacheError, CacheStatus, FactsKey, NewCacheEntry};
use unfold_engine::{
    Engine, EngineOptions, FactsCache, FactsSet, FileFacts, Language, MemoryFactsCache, ProgramFile,
    ProjectInputs, UpdateRequest, Verify,
};
use unfold_lang_python::Python;

use crate::common::{compile_folder_with_threads, copy_folder, testdata};

/// The Python files under `testdata/<folder>`, by path.
fn sources(folder: &str) -> BTreeMap<RepoPath, Snapshot> {
    let root = testdata().join(folder);
    let mut found = BTreeMap::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("a folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "py") {
                let relative = path
                    .strip_prefix(&root)
                    .expect("inside the folder")
                    .to_string_lossy()
                    .replace('\\', "/");
                let path = RepoPath::new(&relative).expect("a path");
                let bytes = std::fs::read(testdata().join(folder).join(&relative)).expect("a file");
                found.insert(path.clone(), Snapshot::new(path, bytes).expect("a snapshot"));
            }
        }
    }
    found
}

#[test]
fn every_call_in_the_parse_tree_is_one_call_node() {
    let mut snapshots = sources("fixtures/python-lowering");
    snapshots.extend(sources("fixtures/python/policy"));
    let settings = Settings::default();
    let files: Vec<ProgramFile> = snapshots
        .iter()
        .map(|(path, snapshot)| ProgramFile {
            path: path.clone(),
            language: unfold_core::vocab::Language::Python,
            facts: Python.extract(snapshot, &settings).ok().map(Arc::new),
        })
        .collect();
    let program = FactsSet::new(files).expect("distinct paths");
    let inputs = ProjectInputs::default();
    let linked = Python.link(&program, &inputs);
    let census = linked.census().expect("Python has a census");
    let mut decorators = 0;
    for (_, file) in program.iter() {
        let Some(facts) = file.facts.as_deref() else {
            continue;
        };
        let mut nodes: Vec<ByteRange> = facts
            .graph
            .nodes()
            .iter()
            .filter(|node| node.kind() == NodeKind::Call)
            .map(|node| node.range)
            .collect();
        nodes.sort_unstable();
        let mut sites: Vec<ByteRange> = census
            .call_sites(&file.path)
            .expect("the census covers the file")
            .iter()
            .map(|site| {
                assert_eq!(site.form, None, "only linking proves a construction");
                site.range
            })
            .collect();
        sites.sort_unstable();
        assert_eq!(nodes, sites, "{}", file.path);
        decorators += facts
            .graph
            .nodes()
            .iter()
            .filter(|node| node.role == Some(unfold_core::vocab::CallRole::Decorator))
            .count();
    }
    assert!(decorators > 0, "decorators are calls the census finds");
}

/// The same bytes always extract to the same facts.
#[test]
fn extraction_is_deterministic() {
    for (path, snapshot) in sources("fixtures/python-lowering") {
        let first = unfold_lang_python::extract(&snapshot, PythonVersion::V3_14)
            .expect("extracts")
            .to_bytes()
            .expect("encodes");
        let second = unfold_lang_python::extract(&snapshot, PythonVersion::V3_14)
            .expect("extracts")
            .to_bytes()
            .expect("encodes");
        assert_eq!(first, second, "{path}");
    }
}

/// The graph depends only on the folder: the same export on one thread or several.
#[test]
fn thread_counts_give_identical_exports() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&testdata().join("fixtures/python-lowering"), folder.path());
    let exports: Vec<String> = [1, 4]
        .into_iter()
        .map(|threads| {
            let compiled = compile_folder_with_threads(folder.path(), threads).valid();
            unfold_core::canonical::to_string(&compiled.graph)
        })
        .collect();
    assert_eq!(exports[0], exports[1]);
}

fn engine(root: &std::path::Path) -> Engine {
    engine_with(root, Arc::new(MemoryFactsCache::new()))
}

fn engine_with(root: &std::path::Path, cache: Arc<dyn FactsCache>) -> Engine {
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: 2,
        ..EngineOptions::default()
    };
    Engine::new(root, vec![Arc::new(Python)], cache, options).expect("an engine for the folder")
}

/// Edits applied with incremental updates end in the graph a full compile of the edited folder
/// gives: a changed class, a removed module, and a new module.
#[test]
fn incremental_updates_equal_a_full_compile() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    copy_folder(&testdata().join("fixtures/python-lowering"), root);
    let mut incremental = engine(root);
    incremental.update(UpdateRequest::compile()).expect("compile");
    let edits: [(&str, Option<&str>); 3] = [
        // `Routed` loses its constructor: its construction now targets the class.
        ("mail/routing.py", Some("class Routed:\n    pass\n")),
        // `notify_reserved` disappears.
        ("booking/notify.py", None),
        // A new module defines what `audit.record` could have meant.
        (
            "shop/records.py",
            Some("from shop.audit import audit\n\naudit.record(\"x\", {})\n"),
        ),
    ];
    for (path, text) in edits {
        let file = root.join(path);
        match text {
            Some(text) => std::fs::write(&file, text).expect("write"),
            None => std::fs::remove_file(&file).expect("remove"),
        }
        let changed = vec![RepoPath::new(path).expect("a path")];
        let mut request = UpdateRequest::background(changed);
        request.verify = Verify::Contents;
        incremental.update(request).expect("an incremental update");
    }
    let mut full = engine(root);
    full.update(UpdateRequest::compile()).expect("compile");
    let export = |engine: &Engine| unfold_core::canonical::to_string(engine.graph());
    let incremental_export = export(&incremental);
    assert!(incremental_export.contains("\"path\":\"shop/records.py\""));
    assert!(!incremental_export.contains("\"path\":\"booking/notify.py\""));
    let construction = incremental
        .graph()
        .file("mail/route.py")
        .expect("the file")
        .links
        .iter()
        .find(|link| link.kind == LinkKind::Calls && &*link.text == "Routed")
        .map(|link| link.to.clone());
    assert_eq!(
        construction,
        Some(unfold_core::LinkTarget::Project(unfold_core::Key::from_text(
            "mail/routing.py#Routed"
        )))
    );
    assert_eq!(incremental_export, export(&full));
}

/// Serves the entries of a cache with every Python payload's version set to an older one, as a
/// cache written by a build with an older payload format holds them under an unchanged key.
struct OlderPayloads(Arc<MemoryFactsCache>);

impl OlderPayloads {
    fn age(bytes: &[u8]) -> Vec<u8> {
        let mut facts = FileFacts::from_bytes(bytes).expect("cached facts decode");
        // The payload starts with its version, one byte in postcard's varint encoding.
        assert_eq!(facts.payload[0], 12, "the payload's version comes first");
        facts.payload[0] = 11;
        facts.to_bytes().expect("facts encode")
    }
}

impl FactsCache for OlderPayloads {
    fn get(&self, keys: &[FactsKey<'_>]) -> Result<Vec<Option<CacheEntry>>, CacheError> {
        let mut found = self.0.get(keys)?;
        for entry in found.iter_mut().flatten() {
            if entry.status == CacheStatus::Parsed {
                entry.bytes = Self::age(&entry.bytes);
            }
        }
        Ok(found)
    }

    fn last_good(
        &self,
        path: &RepoPath,
        engine: &'static str,
        settings: SettingsHash,
    ) -> Result<Option<(ContentHash, Vec<u8>)>, CacheError> {
        Ok(self
            .0
            .last_good(path, engine, settings)?
            .map(|(hash, bytes)| (hash, Self::age(&bytes))))
    }

    fn put(&self, entries: &[NewCacheEntry<'_>]) -> Result<(), CacheError> {
        self.0.put(entries)
    }
}

/// A cached entry whose payload Python can't read (here, an older payload version) is a miss:
/// the file is extracted again, and the update succeeds with the graph a fresh compile gives.
#[test]
fn a_payload_python_cannot_read_is_a_cache_miss() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    copy_folder(&testdata().join("playgrounds/py-orders"), root);
    let cache = Arc::new(MemoryFactsCache::new());
    let compiled = engine_with(root, Arc::clone(&cache) as Arc<dyn FactsCache>)
        .update(UpdateRequest::compile())
        .expect("the first compile");
    assert!(compiled.stats.files_extracted > 0);
    let again = engine_with(root, Arc::new(OlderPayloads(cache)))
        .update(UpdateRequest::compile())
        .expect("a compile over older payloads");
    assert_eq!(again.stats.files_from_cache, 0);
    assert_eq!(again.stats.files_extracted, compiled.stats.files_extracted);
    assert_eq!(
        unfold_core::canonical::to_string(&again.graph),
        unfold_core::canonical::to_string(&compiled.graph)
    );
}
