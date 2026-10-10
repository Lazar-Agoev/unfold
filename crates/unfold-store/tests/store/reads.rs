//! Reader queries: blocks, children, incoming edges with paging, entrances, definitions,
//! applications, and statistics.

use std::collections::BTreeSet;

use unfold_core::testkit::sample::sample;
use unfold_core::testkit::{GraphBuilder, Outcome};
use unfold_core::vocab::{BlockKind, EdgeKind, FileState, Language, Role};
use unfold_store::Cursor;

use crate::common::{Prepared, temp_store};

const CREATE: &str = "src/orders.ts#function:createOrder";
const SAVE: &str = "src/orders.ts#function:save";

#[test]
fn blocks_children_and_files() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let reader = store.readers.get().unwrap();

    let located = reader.block(CREATE).unwrap().unwrap();
    assert_eq!(located.path.as_str(), "src/orders.ts");
    assert_eq!(located.language, Language::TypeScript);
    assert_eq!(located.block.kind, BlockKind::Function);
    assert!(reader.block("src/orders.ts#function:absent").unwrap().is_none());

    let children: Vec<String> = reader
        .children(CREATE)
        .unwrap()
        .into_iter()
        .map(|block| block.key.to_string())
        .collect();
    assert_eq!(
        children,
        [
            "src/orders.ts#function:createOrder/if[0]",
            "src/orders.ts#function:createOrder/return[0]"
        ]
    );
    assert!(reader.children("nothing#").unwrap().is_empty());

    let files = reader.files().unwrap();
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        ["jobs/tasks.py", "src/helper.ts", "src/orders.ts", "src/server.ts"]
    );
    let orders = reader.file("src/orders.ts").unwrap().unwrap();
    assert_eq!(orders.state, FileState::Current);
    assert_eq!(orders.facts_hash, orders.disk.hash);
    assert_eq!(orders.disk.mtime_ns, Some(1_700_000_000_000_000_000));
    assert_eq!(
        orders.record(),
        prepared.graph.file("src/orders.ts").unwrap().file
    );
}

#[test]
fn incoming_edges_carry_their_source() {
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();
    let page = reader.incoming(SAVE, None, 10).unwrap();
    assert_eq!(page.next, None);
    assert_eq!(page.items.len(), 1);
    let edge = &page.items[0];
    assert_eq!(edge.source_path.as_str(), "src/orders.ts");
    assert_eq!(
        edge.source.as_str(),
        "src/orders.ts#function:createOrder/return[0]/call[0]"
    );
    assert_eq!(edge.kind, EdgeKind::Calls);
    assert_eq!(&*edge.text, "save");
    assert_eq!(reader.incoming_count(CREATE).unwrap(), 1);
    assert_eq!(reader.incoming_count("absent#").unwrap(), 0);
}

#[test]
fn incoming_edges_paginate_without_gaps_or_repeats() {
    let mut g = GraphBuilder::new();
    let text = "function target() {}\n".to_owned() + &"target();\n".repeat(25);
    let file = g.file("src/many.ts", Language::TypeScript, &text);
    let target_range = g.find(file, "function target() {}");
    let target = g.block(
        file,
        BlockKind::Function,
        Some(Role::Body),
        Some("target"),
        target_range,
    );
    let target_key = g.key(target);
    let mut from = target_range.end;
    for _ in 0..25 {
        let range = g.find_from(file, "target()", from);
        from = range.end;
        let statement = g.block(
            file,
            BlockKind::ExpressionStatement,
            Some(Role::Body),
            None,
            range,
        );
        let call = g.block(statement, BlockKind::Call, Some(Role::Expression), None, range);
        g.edge(
            call,
            EdgeKind::Calls,
            Outcome::Resolved(target_key.clone()),
            "target",
            None,
        );
    }
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(g.build()).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();

    let mut seen = BTreeSet::new();
    let mut after: Option<Cursor> = None;
    let mut pages = 0;
    loop {
        let page = reader.incoming(target_key.as_str(), after, 7).unwrap();
        pages += 1;
        for edge in &page.items {
            assert!(seen.insert(edge.source.clone()), "repeated {}", edge.source);
        }
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(seen.len(), 25);
    assert_eq!(pages, 4);
    assert_eq!(reader.incoming_count(target_key.as_str()).unwrap(), 25);
}

#[test]
fn entrances_definitions_and_applications() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let reader = store.readers.get().unwrap();

    let in_package = reader.entrances_in("package.json").unwrap();
    assert_eq!(in_package.len(), 1);
    assert_eq!(in_package[0].trigger.as_deref(), Some("start"));
    let http = &prepared
        .graph
        .entrances
        .iter()
        .find(|e| e.method.is_some())
        .unwrap();
    assert_eq!(reader.entrance(http.key.as_str()).unwrap().as_ref(), Some(*http));
    assert!(reader.entrance("entrance:[]").unwrap().is_none());

    let definitions = reader.definitions_in("src/orders.ts").unwrap();
    let names: Vec<(&str, &str)> = definitions
        .iter()
        .map(|definition| (&*definition.label, &*definition.qualified_name))
        .collect();
    assert_eq!(names, [("createOrder", "createOrder"), ("save", "save")]);
    let save = reader.definition(SAVE).unwrap().unwrap();
    assert_eq!(save.path.as_str(), "src/orders.ts");
    assert_eq!(save.kind, BlockKind::Function);

    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page = reader.definitions(after, 2).unwrap();
        all.extend(page.items.into_iter().map(|definition| definition.key));
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    all.sort();
    let expected: Vec<_> = prepared
        .definitions
        .iter()
        .flatten()
        .map(|definition| definition.key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(all, expected);

    let applications = reader.applications().unwrap();
    assert_eq!(applications.len(), 1);
    assert_eq!(applications[0].startup.as_str(), "src/server.ts#");
    assert_eq!(reader.memberships().unwrap(), prepared.memberships);
    assert_eq!(prepared.memberships.len(), 2);
}

#[test]
fn definition_counts_and_entrance_applications_are_grouped() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let reader = store.readers.get().unwrap();

    // Indexed files count their definitions (even none); other paths are left out.
    let counts = reader
        .definition_counts(&["src/server.ts", "README.md", "src/orders.ts"])
        .unwrap();
    let expected = |path: &str| {
        let index = prepared
            .graph
            .files
            .iter()
            .position(|file| file.file.path.as_str() == path)
            .unwrap();
        u32::try_from(prepared.definitions[index].len()).unwrap()
    };
    let counts: Vec<(&str, u32)> = counts
        .iter()
        .map(|(path, count)| (path.as_str(), *count))
        .collect();
    assert_eq!(
        counts,
        [("src/orders.ts", 2), ("src/server.ts", expected("src/server.ts"))]
    );
    assert!(reader.definition_counts(&[]).unwrap().is_empty());

    // Members name their application; the launch names the application it starts.
    let mut keys: Vec<&str> = prepared
        .graph
        .entrances
        .iter()
        .map(|entrance| entrance.key.as_str())
        .collect();
    keys.push("entrance:[]");
    let named = reader.entrance_applications(&keys).unwrap();
    let mut expected: Vec<&str> = prepared
        .graph
        .entrances
        .iter()
        .map(|entrance| entrance.key.as_str())
        .collect();
    expected.sort_unstable();
    assert_eq!(
        named.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>(),
        expected,
        "every sample entrance has an application, listed once"
    );
    assert!(named.iter().all(|(_, name)| &**name == "start"));
    assert!(reader.entrance_applications(&[]).unwrap().is_empty());
}

#[test]
fn stats_count_rows() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let stats = store.readers.get().unwrap().stats().unwrap();
    let graph = &prepared.graph;
    let count = |f: fn(&unfold_core::FileGraph) -> usize| graph.files.iter().map(f).sum::<usize>() as u64;
    assert_eq!(stats.files_current, 4);
    assert_eq!(stats.files_failed + stats.files_last_good, 0);
    assert_eq!(stats.blocks, count(|file| file.blocks.len()));
    assert_eq!(stats.slots, count(|file| file.slots.len()));
    assert_eq!(
        stats.edges_resolved + stats.edges_external + stats.edges_unresolved,
        count(|file| file.edges.len())
    );
    assert_eq!(stats.edges_external, 1);
    assert_eq!(stats.entrances, 3);
    assert_eq!(stats.inputs, 2);
    assert_eq!(stats.applications, 1);
    assert!(stats.database_bytes > 0);
}

#[test]
fn readers_work_from_other_threads() {
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let pool = store.readers.clone();
            std::thread::spawn(move || pool.get().unwrap().files().unwrap().len())
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap(), 4);
    }
    let reader = store.readers.get().unwrap();
    let moved = std::thread::spawn(move || reader.entrances().unwrap().len());
    assert_eq!(moved.join().unwrap(), 3);
}
