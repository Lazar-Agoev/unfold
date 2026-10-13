//! The per-file facts cache.

use unfold_core::testkit::sample::sample;
use unfold_core::{ContentHash, RepoPath};
use unfold_store::{FactsStatus, NewFacts, Store, StoreConfig};

use crate::common::{ENGINE, Prepared, temp_store};

fn hash(text: &str) -> ContentHash {
    ContentHash::of(text.as_bytes())
}

fn put(store: &mut Store, path: &RepoPath, version: &str, settings: &str, status: FactsStatus) {
    store
        .writer
        .put_facts(&[NewFacts {
            path,
            hash: hash(version),
            settings_hash: hash(settings),
            status,
            facts: version.as_bytes(),
        }])
        .expect("put facts");
}

fn cached(store: &Store, path: &RepoPath, version: &str, settings: &str) -> Option<(FactsStatus, String)> {
    store
        .writer
        .facts(path, hash(version), hash(settings))
        .expect("get facts")
        .map(|entry| {
            (
                entry.status,
                String::from_utf8(entry.facts).expect("UTF-8 test facts"),
            )
        })
}

#[test]
fn keeps_the_current_and_last_good_entries() {
    let (_dir, mut store) = temp_store();
    let path = RepoPath::new("src/a.ts").unwrap();
    let other = RepoPath::new("src/b.ts").unwrap();
    put(&mut store, &other, "b1", "s", FactsStatus::Parsed);

    put(&mut store, &path, "v1", "s", FactsStatus::Parsed);
    assert_eq!(
        cached(&store, &path, "v1", "s"),
        Some((FactsStatus::Parsed, "v1".into()))
    );
    put(&mut store, &path, "v2", "s", FactsStatus::Parsed);
    assert_eq!(
        cached(&store, &path, "v1", "s"),
        None,
        "a parsed entry replaces the last one"
    );

    put(&mut store, &path, "v3", "s", FactsStatus::Failed);
    assert_eq!(
        cached(&store, &path, "v3", "s"),
        Some((FactsStatus::Failed, "v3".into()))
    );
    assert!(
        cached(&store, &path, "v2", "s").is_some(),
        "the last good entry stays"
    );
    let last_good = store.writer.last_good_facts(&path, hash("s")).unwrap().unwrap();
    assert_eq!((last_good.hash, last_good.facts), (hash("v2"), b"v2".to_vec()));

    put(&mut store, &path, "v4", "s", FactsStatus::Failed);
    assert_eq!(cached(&store, &path, "v3", "s"), None, "only one failed entry");
    assert!(cached(&store, &path, "v2", "s").is_some());

    put(&mut store, &path, "v5", "s", FactsStatus::Parsed);
    assert_eq!(cached(&store, &path, "v2", "s"), None);
    assert_eq!(cached(&store, &path, "v4", "s"), None);
    assert_eq!(
        store
            .writer
            .last_good_facts(&path, hash("s"))
            .unwrap()
            .unwrap()
            .hash,
        hash("v5")
    );

    // New settings make the old entries useless, even as a last good fallback.
    put(&mut store, &path, "v6", "t", FactsStatus::Failed);
    assert_eq!(cached(&store, &path, "v5", "s"), None);
    assert!(store.writer.last_good_facts(&path, hash("t")).unwrap().is_none());

    // Other paths are untouched.
    assert!(cached(&store, &other, "b1", "s").is_some());
    assert_eq!(store.readers.get().unwrap().stats().unwrap().facts_entries, 2);
}

#[test]
fn loads_the_facts_the_stored_graph_was_built_from() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.sqlite");
    let mut store = Store::open(&db, &StoreConfig::new(ENGINE)).unwrap();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let settings = hash("settings");
    let mut blobs = Vec::new();
    for file in &prepared.graph.files {
        blobs.push((
            file.file.path.clone(),
            file.file.hash.unwrap(),
            format!("facts of {}", file.file.path),
        ));
    }
    let entries: Vec<NewFacts<'_>> = blobs
        .iter()
        .map(|(path, hash, facts)| NewFacts {
            path,
            hash: *hash,
            settings_hash: settings,
            status: FactsStatus::Parsed,
            facts: facts.as_bytes(),
        })
        .collect();
    store.writer.put_facts(&entries).unwrap();
    // An entry for bytes the graph does not know is not loaded.
    let stray = RepoPath::new("src/new.ts").unwrap();
    put(&mut store, &stray, "new", "settings", FactsStatus::Parsed);
    drop(store);

    let store = Store::open(&db, &StoreConfig::new(ENGINE)).unwrap();
    let loaded = store.writer.load_facts().unwrap();
    let paths: Vec<&str> = loaded.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(
        paths,
        ["jobs/tasks.py", "src/helper.ts", "src/orders.ts", "src/server.ts"]
    );
    assert_eq!(loaded[2].facts, b"facts of src/orders.ts");
    assert!(
        loaded
            .iter()
            .all(|entry| entry.status == FactsStatus::Parsed && entry.settings_hash == settings)
    );
    drop(store);

    // Another engine sees nothing: the database is rebuilt.
    let store = Store::open(&db, &StoreConfig::new("engine-2")).unwrap();
    assert!(store.writer.load_facts().unwrap().is_empty());
}

#[test]
fn removing_a_file_prunes_its_facts() {
    let (_dir, mut store) = temp_store();
    let before = Prepared::new(sample().build());
    store.writer.commit(&before.first_compile()).unwrap();
    let tasks = RepoPath::new("jobs/tasks.py").unwrap();
    let helper = RepoPath::new("src/helper.ts").unwrap();
    put(&mut store, &tasks, "t1", "s", FactsStatus::Parsed);
    put(&mut store, &tasks, "t2", "s", FactsStatus::Failed);
    put(&mut store, &helper, "h1", "s", FactsStatus::Parsed);

    let mut graph = before.graph.clone();
    graph.files.retain(|file| file.file.path != tasks);
    let after = Prepared::new(graph);
    store.writer.commit(&after.since(&before)).unwrap();

    assert_eq!(cached(&store, &tasks, "t1", "s"), None);
    assert_eq!(cached(&store, &tasks, "t2", "s"), None);
    assert!(cached(&store, &helper, "h1", "s").is_some());
    assert_eq!(store.readers.get().unwrap().stats().unwrap().facts_entries, 1);
}
