//! Unit tests that need the writer's test hooks: faults injected mid-commit, readers during a
//! commit, and the key table (IDs that last, and only live keys).

use std::sync::{Arc, mpsc};

use tempfile::TempDir;
use unfold_core::key::Callee;
use unfold_core::testkit::GraphBuilder;
use unfold_core::testkit::sample::shop;
use unfold_core::vocab::{Form, Language, LinkKind, Reason};
use unfold_core::{FileGraph, Graph, LinkTarget};

use crate::writer::FaultPoint;
use crate::{Definition, DiskStamp, FileChange, Reader, Store, StoreConfig, StoreError, Update, Writer};

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
        apps: Some(&graph.apps),
        ..Update::default()
    }
}

/// The sample with `src/orders.ts` and the entrance into it removed, a link in `shop/store.py`
/// unknown, and `src/extra.ts` (with new keys) added.
fn changed_sample() -> Graph {
    let mut graph = shop().build();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "src/orders.ts");
    graph.apps[0].entrances.remove(1);
    let store = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "shop/store.py")
        .map(Arc::make_mut)
        .expect("the sample has store.py");
    store.links[0].to = LinkTarget::Unknown(Reason::Dynamic);
    let mut builder = GraphBuilder::new();
    let extra = builder.file("src/extra.ts", Language::TypeScript, "f();\n");
    builder.call(extra, &Callee::name("f"), builder.find(extra, "f()"));
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
        apps: (before.apps != after.apps).then_some(&after.apps[..]),
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
    let mut stats = reader.stats().expect("stats");
    // The file's size depends on its history.
    stats.database_bytes = 0;
    format!(
        "{files:?}\n{:?}\n{:?}\n{stats:?}\n{:?}\n{:?}",
        reader.apps().expect("apps"),
        reader.inputs().expect("inputs"),
        reader
            .callers("shop/store.py#save_order", None, 10)
            .expect("callers"),
        reader
            .search("ts", crate::CandidateKind::File, &[], 10)
            .expect("search"),
    )
}

/// The mirror agrees with the committed tables, which hold only live keys.
fn assert_mirror_matches(writer: &Writer) {
    assert_keys_live(writer);
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

fn key_count(writer: &Writer) -> i64 {
    writer
        .conn()
        .query_row("SELECT count(*) FROM keys", [], |row| row.get(0))
        .expect("count keys")
}

/// Every key ID a row holds: each key column, and the search rows' (`crate::search::rowid`).
const KEY_USES: &str = "SELECT key_id AS id FROM nodes UNION SELECT target_key_id FROM links \
     WHERE target_key_id IS NOT NULL UNION SELECT key_id FROM definitions UNION SELECT id FROM apps \
     UNION SELECT id FROM entrances UNION SELECT code_key_id FROM entrance_targets \
     UNION SELECT rowid / 4 FROM search";

/// The key table holds exactly the keys some row refers to.
fn assert_keys_live(writer: &Writer) {
    let mut statement = writer
        .conn()
        .prepare(&format!("SELECT key FROM keys WHERE id NOT IN ({KEY_USES})"))
        .expect("prepare");
    let unused: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert!(unused.is_empty(), "keys no row uses: {unused:?}");
    let dangling: i64 = writer
        .conn()
        .query_row(
            &format!("SELECT count(*) FROM ({KEY_USES}) WHERE id NOT IN (SELECT id FROM keys)"),
            [],
            |row| row.get(0),
        )
        .expect("count dangling key IDs");
    assert_eq!(dangling, 0, "rows hold key IDs the key table lacks");
}

/// The sample, plus `src/renamed.ts`, whose function `f{n}` calls `helper`, and `src/caller.ts`,
/// which calls `f{n}`: the graph after renaming `f0` `n` times.
fn renamed(n: usize) -> Graph {
    let mut builder = shop();
    let name = format!("f{n}");
    let function_text = format!("export function {name}() {{\n  helper();\n}}");
    let module = builder.file(
        "src/renamed.ts",
        Language::TypeScript,
        &format!("{function_text}\nfunction helper() {{}}\n"),
    );
    let function = builder.def(
        module,
        Form::Function,
        &name,
        builder.find(module, &function_text),
    );
    let call = builder.call(
        function,
        &Callee::name("helper"),
        builder.find(module, "helper()"),
    );
    let helper = builder.def(
        module,
        Form::Function,
        "helper",
        builder.find(module, "function helper() {}"),
    );
    let helper = LinkTarget::Project(builder.key(helper));
    builder.link(
        call,
        LinkKind::Calls,
        builder.find(module, "helper"),
        "helper",
        helper,
    );
    let function = LinkTarget::Project(builder.key(function));
    let caller = builder.file("src/caller.ts", Language::TypeScript, &format!("{name}();\n"));
    let call = builder.call(
        caller,
        &Callee::name(&name),
        builder.find(caller, &format!("{name}()")),
    );
    builder.link(
        call,
        LinkKind::Calls,
        builder.find(caller, &name),
        &name,
        function,
    );
    builder.build()
}

/// The definition catalog of each file of `graph`, every definition named by its name (the
/// module by its path).
fn catalogs(graph: &Graph) -> Vec<Vec<Definition>> {
    graph
        .files
        .iter()
        .map(|file| {
            file.nodes
                .iter()
                .zip(0..)
                .filter(|(node, _)| node.kind().is_definition())
                .map(|(node, id)| {
                    let name: Box<str> = node
                        .name
                        .clone()
                        .unwrap_or_else(|| file.file.path.as_str().into());
                    Definition {
                        key: node.key.clone(),
                        node: id,
                        kind: node.kind(),
                        qualified_name: name.clone(),
                        label: name,
                    }
                })
                .collect()
        })
        .collect()
}

/// `update`, a change to `graph`, with the definitions of each file it writes from `catalogs`
/// (one per file of `graph`).
fn with_catalogs<'a>(mut update: Update<'a>, graph: &Graph, catalogs: &'a [Vec<Definition>]) -> Update<'a> {
    for change in &mut update.files {
        let index = graph
            .files
            .iter()
            .position(|file| file.file.path == change.file.path)
            .expect("a file of the graph");
        change.definitions = Some(&catalogs[index]);
    }
    update
}

#[test]
fn a_fault_at_any_point_leaves_the_previous_graph() {
    let before = shop().build();
    let after = changed_sample();
    let points = [
        FaultPoint::AfterRemovals,
        FaultPoint::AfterFiles(1),
        FaultPoint::AfterFiles(2),
        FaultPoint::AfterApps,
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
    let graph = shop().build();
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
    let before = shop().build();
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
    assert!(reader.file("src/orders.ts").expect("file").is_none());
}

#[test]
fn key_ids_last_while_rows_use_them() {
    let before = shop().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let kept = ["shop/store.py#save_order", "shop/payments.py#charge_card"];
    let removed = ["src/orders.ts#loadOrder", "src/orders.ts#"];
    let ids: Vec<Option<i64>> = kept.iter().map(|key| key_id(&store.writer, key)).collect();
    assert!(ids.iter().all(Option::is_some));
    let removed_ids: Vec<Option<i64>> = removed.iter().map(|key| key_id(&store.writer, key)).collect();
    assert!(removed_ids.iter().all(Option::is_some));

    // Removing orders.ts, and the entrance into it, deletes the keys only it used.
    store.writer.commit(&delta(&before, &after)).expect("update");
    assert_keys_live(&store.writer);
    for key in removed {
        assert_eq!(key_id(&store.writer, key), None, "{key}");
    }
    // Adding it back interns them again under new IDs; the keys rows kept using keep theirs.
    store.writer.commit(&delta(&after, &before)).expect("restore");
    assert_keys_live(&store.writer);
    let again: Vec<Option<i64>> = kept.iter().map(|key| key_id(&store.writer, key)).collect();
    assert_eq!(ids, again);
    for (key, old) in removed.iter().zip(removed_ids) {
        assert!(key_id(&store.writer, key) > old, "{key}");
    }

    // Links into a definition point at the same key ID throughout.
    let target: i64 = store
        .writer
        .conn()
        .query_row(
            "SELECT target_key_id FROM links WHERE text = 'save_order' AND kind = 'passes'",
            [],
            |row| row.get(0),
        )
        .expect("a passes link");
    assert_eq!(Some(target), ids[0]);

    // Reopening reloads the same IDs into a fresh mirror.
    let path = store.writer.path().to_path_buf();
    drop(store);
    let mut store = Store::open(&path, &StoreConfig::new("engine")).expect("reopen");
    store
        .writer
        .commit(&delta(&before, &after))
        .expect("update after reopen");
    assert_eq!(key_id(&store.writer, kept[0]), ids[0]);
    assert_mirror_matches(&store.writer);
}

#[test]
fn a_panic_mid_commit_does_not_wedge_the_writer() {
    let before = shop().build();
    let after = changed_sample();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&before))
        .expect("first compile");
    let reader = store.readers.get().expect("reader");
    let original = state(&reader);

    store.writer.hook = Some(Box::new(|at| {
        assert_ne!(at, FaultPoint::AfterApps, "a bug panics mid-commit");
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
    let before = shop().build();
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
    assert!(reader.file("src/orders.ts").expect("file").is_none());
    assert!(reader.file("src/extra.ts").expect("file").is_some());
}

#[test]
fn renames_leave_only_the_live_keys() {
    let graphs: Vec<Graph> = (0..=20).map(renamed).collect();
    let catalogs: Vec<Vec<Vec<Definition>>> = graphs.iter().map(catalogs).collect();
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&with_catalogs(
            first_compile(&graphs[0]),
            &graphs[0],
            &catalogs[0],
        ))
        .expect("first compile");
    assert_keys_live(&store.writer);
    let keys = key_count(&store.writer);
    for n in 1..graphs.len() {
        let update = with_catalogs(delta(&graphs[n - 1], &graphs[n]), &graphs[n], &catalogs[n]);
        let stats = store.writer.commit(&update).expect("rename");
        // The function's key, its call's, and its caller's call's.
        assert_eq!(stats.keys_removed, 3, "rename {n}");
        assert_mirror_matches(&store.writer);
    }
    assert_eq!(key_count(&store.writer), keys);
    assert_eq!(key_id(&store.writer, "src/renamed.ts#f0"), None);
    assert!(key_id(&store.writer, "src/renamed.ts#f20").is_some());

    // A fresh store of the last graph holds as many keys.
    let (_fresh_dir, mut fresh) = temp_store();
    let last = graphs.len() - 1;
    fresh
        .writer
        .commit(&with_catalogs(
            first_compile(&graphs[last]),
            &graphs[last],
            &catalogs[last],
        ))
        .expect("fresh compile");
    assert_eq!(key_count(&fresh.writer), keys);
}

#[test]
fn a_key_any_row_uses_is_kept() {
    let first = renamed(0);
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&first_compile(&first))
        .expect("first compile");
    assert_keys_live(&store.writer);

    // An entrance target: no code has `refund_order`, and rewriting the apps keeps its key.
    const REFUND: &str = "shop/service.py#refund_order";
    let refund = key_id(&store.writer, REFUND).expect("its entrance interned it");
    let mut apps = first.clone();
    apps.apps[0].entrances[0].name = "Place one order".into();
    store.writer.commit(&delta(&first, &apps)).expect("apps");
    assert_eq!(key_id(&store.writer, REFUND), Some(refund));
    assert_keys_live(&store.writer);
    let mut no_refund = apps.clone();
    no_refund.apps[0]
        .entrances
        .retain(|entrance| &*entrance.name != "Refund an order");
    store.writer.commit(&delta(&apps, &no_refund)).expect("entrance");
    assert_eq!(key_id(&store.writer, REFUND), None);
    assert_keys_live(&store.writer);

    // A link: `caller.ts` still calls `f0` after `f0` is renamed, until it follows.
    let caller = |graph: &Graph| {
        graph
            .files
            .iter()
            .position(|file| file.file.path.as_str() == "src/caller.ts")
            .expect("the graph has caller.ts")
    };
    let mut stale = renamed(1);
    stale.apps.clone_from(&no_refund.apps);
    let index = caller(&stale);
    stale.files[index] = Arc::clone(&first.files[caller(&first)]);
    store.writer.commit(&delta(&no_refund, &stale)).expect("rename");
    assert!(key_id(&store.writer, "src/renamed.ts#f0").is_some());
    assert_eq!(key_id(&store.writer, "src/renamed.ts#f0/call:helper"), None);
    assert_keys_live(&store.writer);
    let mut followed = renamed(1);
    followed.apps.clone_from(&no_refund.apps);
    store.writer.commit(&delta(&stale, &followed)).expect("follow");
    assert_eq!(key_id(&store.writer, "src/renamed.ts#f0"), None);
    assert_keys_live(&store.writer);

    // A file's search row: `caller.ts` without nodes keeps its module key until it goes.
    let mut empty = followed.clone();
    let index = caller(&empty);
    let file = Arc::make_mut(&mut empty.files[index]);
    file.nodes.clear();
    file.links.clear();
    store
        .writer
        .commit(&delta(&followed, &empty))
        .expect("empty file");
    assert!(key_id(&store.writer, "src/caller.ts#").is_some());
    assert_keys_live(&store.writer);
    let mut gone = empty.clone();
    gone.files.remove(index);
    store.writer.commit(&delta(&empty, &gone)).expect("remove");
    assert_eq!(key_id(&store.writer, "src/caller.ts#"), None);
    assert_keys_live(&store.writer);
    assert_mirror_matches(&store.writer);
}
