//! Unit tests that need the writer's test hooks: faults injected mid-commit, readers during a
//! commit, and key-ID stability.

use std::sync::mpsc;

use tempfile::TempDir;
use unfold_core::testkit::sample::sample;
use unfold_core::vocab::{Reason, Resolution};
use unfold_core::{FileGraph, Graph};

use crate::writer::FaultPoint;
use crate::{DiskStamp, EntranceChanges, FileChange, Reader, Store, StoreConfig, StoreError, Update, Writer};

fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create a temporary directory");
    let store = Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("engine")).expect("open");
    (dir, store)
}

fn first_compile(graph: &Graph) -> Update<'_> {
    Update {
        files: graph
            .files
            .iter()
            .map(|file| FileChange::full(file, DiskStamp::default()))
            .collect(),
        inputs: Some(&graph.inputs),
        entrances: EntranceChanges::all(&graph.entrances),
        ..Update::default()
    }
}

/// The sample with `jobs/tasks.py` and its manual entrance removed, an edge in `src/orders.ts`
/// unresolved, and `src/extra.ts` (with new keys) added.
fn changed_sample() -> Graph {
    let mut graph = sample().build();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "jobs/tasks.py");
    graph
        .entrances
        .retain(|entrance| entrance.evidence.path.as_str() != "unfold.toml");
    let orders = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "src/orders.ts")
        .expect("the sample has orders.ts");
    let edge = &mut orders.edges[0];
    edge.target = None;
    edge.resolution = Resolution::Unresolved;
    edge.reason = Some(Reason::Dynamic);
    edge.module = None;
    let mut builder = unfold_core::testkit::GraphBuilder::new();
    let extra = builder.file(
        "src/extra.ts",
        unfold_core::vocab::Language::TypeScript,
        "let x = 1;\n",
    );
    let range = builder.find(extra, "let x = 1;");
    builder.block(
        extra,
        unfold_core::vocab::BlockKind::Variable,
        Some(unfold_core::vocab::Role::Body),
        Some("x"),
        range,
    );
    graph.files.extend(builder.build().files);
    graph.normalize();
    graph
}

fn delta<'a>(before: &Graph, after: &'a Graph) -> Update<'a> {
    let mut files = Vec::new();
    for file in &after.files {
        match before.file(file.file.path.as_str()) {
            None => files.push(FileChange::full(file, DiskStamp::default())),
            Some(old) => {
                let change = FileChange::between(old, file, DiskStamp::default());
                if change.has_parts() || old.file != file.file {
                    files.push(change);
                }
            }
        }
    }
    Update {
        files,
        removed_files: before
            .files
            .iter()
            .filter(|file| after.file(file.file.path.as_str()).is_none())
            .map(|file| file.file.path.clone())
            .collect(),
        entrances: EntranceChanges::between(&before.entrances, &after.entrances),
        ..Update::default()
    }
}

/// Everything readable, for before/after comparisons.
fn state(reader: &Reader) -> String {
    let files: Vec<Option<FileGraph>> = reader
        .files()
        .expect("files")
        .iter()
        .map(|file| reader.file_graph(file.path.as_str()).expect("file graph"))
        .collect();
    let stats = reader.stats().expect("stats");
    format!(
        "{files:?}\n{:?}\n{:?}\n{stats:?}\n{:?}",
        reader.entrances().expect("entrances"),
        reader.inputs().expect("inputs"),
        reader
            .search("x", crate::CandidateKind::File, &[], 10)
            .expect("search"),
    )
}

/// The mirror agrees with the committed tables.
fn assert_mirror_matches(writer: &Writer) {
    let keys: i64 = writer
        .conn()
        .query_row("SELECT count(*) FROM keys", [], |row| row.get(0))
        .expect("count keys");
    let mirror = writer.mirror().expect("the mirror is loaded after a commit");
    assert_eq!(mirror.key_count() as i64, keys);
    let mut statement = writer
        .conn()
        .prepare("SELECT id, path FROM files")
        .expect("prepare");
    let rows: Vec<(i64, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    for (id, path) in &rows {
        assert_eq!(mirror.file_id(path), Some(*id), "{path}");
    }
    assert_eq!(
        mirror.file_id("src/extra.ts").is_some(),
        rows.iter().any(|(_, path)| path == "src/extra.ts")
    );
}

fn key_id(writer: &Writer, key: &str) -> Option<i64> {
    writer
        .conn()
        .query_row("SELECT id FROM keys WHERE key = ?1", [key], |row| row.get(0))
        .ok()
}

#[test]
fn a_fault_at_any_point_leaves_the_previous_graph() {
    let before = sample().build();
    let after = changed_sample();
    let points = [
        FaultPoint::AfterRemovals,
        FaultPoint::AfterFiles(1),
        FaultPoint::AfterFiles(2),
        FaultPoint::AfterEntrances,
        FaultPoint::BeforeCommit,
    ];
    let mut expected_after = None;
    for point in points {
        let (_dir, mut store) = temp_store();
        store
            .writer
            .commit(&first_compile(&before))
            .expect("first compile");
        let reader = store.readers.get().expect("reader");
        let original = state(&reader);

        store.writer.hook = Some(Box::new(move |at| {
            if at == point {
                Err(StoreError::Injected(at))
            } else {
                Ok(())
            }
        }));
        let error = store
            .writer
            .commit(&delta(&before, &after))
            .expect_err("the fault aborts");
        assert!(
            matches!(error, StoreError::Injected(at) if at == point),
            "{error}"
        );
        assert_eq!(state(&reader), original, "fault at {point:?}");
        assert_mirror_matches(&store.writer);
        assert_eq!(key_id(&store.writer, "src/extra.ts#"), None);

        // The same update then succeeds, and the mirror hands out IDs without gaps.
        store.writer.hook = None;
        let max_before: i64 = store
            .writer
            .conn()
            .query_row("SELECT max(id) FROM keys", [], |row| row.get(0))
            .expect("max id");
        store.writer.commit(&delta(&before, &after)).expect("commit");
        assert_mirror_matches(&store.writer);
        assert_eq!(key_id(&store.writer, "src/extra.ts#"), Some(max_before + 1));
        let result = state(&reader);
        assert_ne!(result, original);
        if let Some(expected) = &expected_after {
            assert_eq!(&result, expected);
        }
        expected_after = Some(result);
    }
}

#[test]
fn a_fault_during_a_first_compile_leaves_an_empty_database() {
    let graph = sample().build();
    let (_dir, mut store) = temp_store();
    store.writer.hook = Some(Box::new(|at| match at {
        FaultPoint::AfterFiles(3) => Err(StoreError::Injected(at)),
        _ => Ok(()),
    }));
    assert!(store.writer.commit(&first_compile(&graph)).is_err());
    let reader = store.readers.get().expect("reader");
    assert!(reader.files().expect("files").is_empty());
    assert_eq!(reader.stats().expect("stats").keys, 0);
    assert_mirror_matches(&store.writer);
    store.writer.hook = None;
    store.writer.commit(&first_compile(&graph)).expect("commit");
    assert_eq!(reader.files().expect("files").len(), 4);
}

#[test]
fn readers_see_the_old_graph_until_the_commit_ends() {
    let before = sample().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let reader = store.readers.get().expect("reader");
    let original = state(&reader);

    let (reached_tx, reached) = mpsc::channel();
    let (resume, resume_rx) = mpsc::channel::<()>();
    store.writer.hook = Some(Box::new(move |at| {
        if at == FaultPoint::BeforeCommit {
            reached_tx.send(()).expect("signal the test");
            resume_rx.recv().expect("wait for the test");
        }
        Ok(())
    }));
    let mut writer = store.writer;
    let committer = std::thread::spawn(move || {
        let after = after;
        writer.commit(&delta(&before, &after)).map(|_| ())
    });

    reached.recv().expect("the commit reaches its last step");
    // Every write is done but not committed: readers, old and new, see the old graph.
    assert_eq!(state(&reader), original);
    let fresh = store.readers.get().expect("another reader");
    assert_eq!(state(&fresh), original);
    assert!(fresh.file("src/extra.ts").expect("file").is_none());
    // A snapshot taken now stays on the old graph even after the commit.
    let pinned = store.readers.get().expect("a pinned reader");
    let during = pinned.snapshot(|pinned| {
        let first = pinned.files()?;
        resume.send(()).expect("resume the writer");
        committer.join().expect("the writer thread").expect("commit");
        let second = pinned.files()?;
        Ok((first, second))
    });
    let (first, second) = during.expect("snapshot");
    assert_eq!(first, second);

    assert_ne!(state(&reader), original);
    assert!(reader.file("src/extra.ts").expect("file").is_some());
    assert!(reader.file("jobs/tasks.py").expect("file").is_none());
}

#[test]
fn key_ids_survive_updates() {
    let before = sample().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let watched = [
        "src/orders.ts#function:save",
        "src/helper.ts#function:helper",
        "jobs/tasks.py#function:run",
        "src/server.ts#",
    ];
    let ids: Vec<Option<i64>> = watched.iter().map(|key| key_id(&store.writer, key)).collect();
    assert!(ids.iter().all(Option::is_some));

    store.writer.commit(&delta(&before, &after)).expect("update");
    // Removing tasks.py and adding it back reuses its keys' IDs.
    store.writer.commit(&delta(&after, &before)).expect("restore");
    let again: Vec<Option<i64>> = watched.iter().map(|key| key_id(&store.writer, key)).collect();
    assert_eq!(ids, again);

    // Edges into a definition point at the same key ID throughout.
    let target: i64 = store
        .writer
        .conn()
        .query_row(
            "SELECT target_key_id FROM edges WHERE target_text = 'helper' AND kind = 'references'",
            [],
            |row| row.get(0),
        )
        .expect("reference edge");
    assert_eq!(Some(target), ids[1]);

    // Reopening reloads the same IDs into a fresh mirror.
    let path = store.writer.path().to_path_buf();
    drop(store);
    let mut store = Store::open(&path, &StoreConfig::new("engine")).expect("reopen");
    store
        .writer
        .commit(&delta(&before, &after))
        .expect("update after reopen");
    assert_eq!(key_id(&store.writer, watched[0]), ids[0]);
    assert_mirror_matches(&store.writer);
}

#[test]
fn a_panic_mid_commit_does_not_wedge_the_writer() {
    let before = sample().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let reader = store.readers.get().expect("reader");
    let original = state(&reader);

    store.writer.hook = Some(Box::new(|at| {
        assert_ne!(at, FaultPoint::AfterEntrances, "a bug panics mid-commit");
        Ok(())
    }));
    let update = delta(&before, &after);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.writer.commit(&update)));
    assert!(panicked.is_err());
    store.writer.hook = None;
    assert_eq!(state(&reader), original);

    // The facts cache and clearing work again at once, and so does the next commit.
    store
        .writer
        .put_facts(&[crate::NewFacts {
            path: &unfold_core::RepoPath::new("src/orders.ts").expect("path"),
            hash: unfold_core::ContentHash::of(b"v1"),
            settings_hash: unfold_core::ContentHash::of(b"s"),
            status: crate::FactsStatus::Parsed,
            facts: b"facts",
        }])
        .expect("put facts after the panic");
    store.writer.commit(&update).expect("commit after the panic");
    assert_mirror_matches(&store.writer);
    assert!(reader.file("src/extra.ts").expect("file").is_some());
    store.writer.clear().expect("clear");
    store
        .writer
        .commit(&first_compile(&before))
        .expect("commit after clear");
    assert_mirror_matches(&store.writer);
    assert_eq!(state(&reader), original);
}

#[test]
fn a_fault_during_replace_leaves_the_previous_graph() {
    let before = sample().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let reader = store.readers.get().expect("reader");
    let original = state(&reader);
    for point in [FaultPoint::AfterRemovals, FaultPoint::BeforeCommit] {
        store.writer.hook = Some(Box::new(move |at| {
            if at == point {
                Err(StoreError::Injected(at))
            } else {
                Ok(())
            }
        }));
        assert!(store.writer.replace(&first_compile(&after)).is_err());
        assert_eq!(state(&reader), original, "fault at {point:?}");
        assert_mirror_matches(&store.writer);
    }
    store.writer.hook = None;
    store.writer.replace(&first_compile(&after)).expect("replace");
    assert_mirror_matches(&store.writer);
    assert!(reader.file("jobs/tasks.py").expect("file").is_none());
    assert!(reader.file("src/extra.ts").expect("file").is_some());
}
