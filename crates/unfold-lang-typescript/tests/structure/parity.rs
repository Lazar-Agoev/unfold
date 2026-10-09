//! Parity with the proof of concept's native producer: every file of the TypeScript playground
//! and the fixtures is extracted with its folder's configuration, linked, validated, and compared
//! record for record with the canonical references: files, blocks, slots, and every edge. The
//! playground's call edges differ only where a defect is fixed (C-2, C-8), and every edge Unfold
//! resolves names the checker-backed reference's target (its precision proxy).

use std::collections::BTreeMap;
use std::path::Path;

use unfold_core::testkit::testdata_dir;
use unfold_core::{Graph, canonical};
use unfold_lang_typescript::TypeScript;

use unfold_core::vocab::{EdgeKind, Reason, Resolution};

use crate::support::{Compiled, compile_with, folder_inputs, sources};

fn compile_folder(folder: &str) -> Compiled {
    let root = testdata_dir().join(folder);
    let files = sources(&root);
    assert!(!files.is_empty(), "no TypeScript sources in {folder}");
    compile_with(&TypeScript::new(), &files, &folder_inputs(&root)).valid()
}

fn reference(name: &str) -> Graph {
    let path = testdata_dir()
        .join("reference/canonical")
        .join(format!("{name}.jsonl"));
    let text = std::fs::read_to_string(&path).expect("read the canonical reference");
    canonical::read(&text).expect("parse the canonical reference")
}

/// Every difference in files, blocks, and slots between the reference and the actual graph,
/// matched by identity (path, block key, and slot name and index).
fn differences(expected: &Graph, actual: &Graph) -> Vec<String> {
    let mut out = Vec::new();
    let actual_files: BTreeMap<&str, _> = actual
        .files
        .iter()
        .map(|file| (file.file.path.as_str(), file))
        .collect();
    let expected_paths: Vec<&str> = expected
        .files
        .iter()
        .map(|file| file.file.path.as_str())
        .collect();
    for path in actual_files.keys().filter(|path| !expected_paths.contains(path)) {
        out.push(format!("extra file {path}"));
    }
    for expected_file in &expected.files {
        let path = expected_file.file.path.as_str();
        let Some(actual_file) = actual_files.get(path) else {
            out.push(format!("missing file {path}"));
            continue;
        };
        let (want, have) = (&expected_file.file, &actual_file.file);
        if (want.language, want.state, want.hash, &want.failure)
            != (have.language, have.state, have.hash, &have.failure)
        {
            out.push(format!("file {path}: expected {want:?}, found {have:?}"));
        }
        let key_of = |file: &unfold_core::FileGraph, local: Option<u32>| {
            local.map(|local| file.blocks[local as usize].key.as_str().to_owned())
        };
        let blocks = |file: &unfold_core::FileGraph| -> BTreeMap<String, String> {
            file.blocks
                .iter()
                .map(|block| {
                    let summary = format!(
                        "kind={} role={:?} name={:?} range={} lines={:?} parent={:?}",
                        block.kind,
                        block.role,
                        block.name,
                        block.range,
                        block.lines,
                        key_of(file, block.parent)
                    );
                    (block.key.as_str().to_owned(), summary)
                })
                .collect()
        };
        let slots = |file: &unfold_core::FileGraph| -> BTreeMap<String, String> {
            file.slots
                .iter()
                .map(|slot| {
                    let key = format!(
                        "{} {}{}",
                        file.blocks[slot.block as usize].key,
                        slot.slot,
                        slot.index.map_or(String::new(), |index| format!("[{index}]"))
                    );
                    (key, slot.range.to_string())
                })
                .collect()
        };
        let imports = |file: &unfold_core::FileGraph| -> BTreeMap<String, String> {
            file.edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Imports)
                .map(|edge| {
                    let summary = format!(
                        "target={:?} text={} resolution={} reason={:?} module={:?}",
                        edge.target.as_ref().map(|key| key.as_str()),
                        edge.text,
                        edge.resolution,
                        edge.reason,
                        edge.module.as_ref().map(|module| module.as_str())
                    );
                    (file.blocks[edge.source as usize].key.as_str().to_owned(), summary)
                })
                .collect()
        };
        compare(&mut out, "block", &blocks(expected_file), &blocks(actual_file));
        compare(&mut out, "slot", &slots(expected_file), &slots(actual_file));
        compare(
            &mut out,
            "imports edge",
            &imports(expected_file),
            &imports(actual_file),
        );
    }
    out
}

fn compare(
    out: &mut Vec<String>,
    what: &str,
    expected: &BTreeMap<String, String>,
    actual: &BTreeMap<String, String>,
) {
    for (key, want) in expected {
        match actual.get(key) {
            None => out.push(format!("missing {what} {key}: {want}")),
            Some(have) if have != want => {
                out.push(format!("changed {what} {key}: expected {want}, found {have}"));
            }
            Some(_) => {}
        }
    }
    for (key, have) in actual {
        if !expected.contains_key(key) {
            out.push(format!("extra {what} {key}: {have}"));
        }
    }
}

#[track_caller]
fn assert_parity(folder: &str, reference_name: &str) {
    let actual = compile_folder(folder);
    let expected = reference(reference_name);
    let found = differences(&expected, &actual.graph);
    assert!(
        found.is_empty(),
        "{} differences from {reference_name}:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn playground_blocks_slots_and_imports_match_the_native_reference() {
    assert_parity("playgrounds/ts-orders", "playground-ts-native");
}

#[test]
fn entrance_fixture_blocks_slots_and_imports_match_the_reference() {
    assert_parity("fixtures/entrances", "entrances");
}

/// Every edge of a graph by `(source key, seq)`.
fn edges(graph: &Graph) -> BTreeMap<(String, u32), &unfold_core::Edge> {
    graph
        .files
        .iter()
        .flat_map(|file| {
            file.edges.iter().map(move |edge| {
                (
                    (
                        file.blocks[edge.source as usize].key.as_str().to_owned(),
                        edge.seq,
                    ),
                    edge,
                )
            })
        })
        .collect()
}

/// The playground edge C-8 resolves: `printReport()` on a binding destructured from
/// `await import("./jobs/report.ts")`, which both references leave `not_concrete`.
const C8_EDGE: &str = "src/cli.ts#function:main/switch[0]/case[1]/expression_statement[0]/call[0]";

#[test]
fn playground_edges_match_the_native_reference_but_for_fixed_defects() {
    let actual = compile_folder("playgrounds/ts-orders");
    let native = reference("playground-ts-native");
    let checker = reference("playground-ts-checker");
    let (actual, expected, checker) = (edges(&actual.graph), edges(&native), edges(&checker));
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "the same edges exist"
    );
    let mut fixed = Vec::new();
    for (key, edge) in &actual {
        let want = expected[key];
        if *edge == want {
            continue;
        }
        // A fixed defect only ever resolves an edge the reference left unresolved.
        assert_eq!(edge.resolution, Resolution::Resolved, "{key:?}");
        assert_eq!(want.resolution, Resolution::Unresolved, "{key:?}");
        if key.0 == C8_EDGE {
            assert_eq!(want.reason, Some(Reason::NotConcrete));
        } else {
            // C-2: a constant object literal's member, as the checker resolves it.
            assert_eq!(want.reason, Some(Reason::UnknownReceiver), "{key:?}");
            assert_eq!(edge.target, checker[key].target, "{key:?}");
        }
        fixed.push(key.0.clone());
    }
    assert_eq!(fixed.len(), 26, "25 C-2 edges and one C-8 edge: {fixed:#?}");
}

#[test]
fn every_resolved_playground_edge_names_the_checker_target() {
    let actual = compile_folder("playgrounds/ts-orders");
    let checker = reference("playground-ts-checker");
    let checker = edges(&checker);
    let mut resolved = 0;
    for (key, edge) in edges(&actual.graph) {
        if edge.resolution != Resolution::Resolved || edge.kind == EdgeKind::Imports {
            continue;
        }
        resolved += 1;
        let want = checker[&key];
        if key.0 == C8_EDGE {
            assert_eq!(want.reason, Some(Reason::NotConcrete));
            continue;
        }
        assert_eq!(want.resolution, Resolution::Resolved, "{key:?}");
        assert_eq!(edge.target, want.target, "{key:?}");
    }
    assert_eq!(resolved, 87, "72 calls and constructions and 15 references");
}

/// Recall as the proof of concept measured it (`docs/testing.md`, front-end parity): of the
/// calls and constructions the checker-backed reference resolves, how many resolve to the same
/// target. The native producer reached 46 of 71; C-2 adds the other 25.
#[test]
fn playground_recall_reaches_every_checker_resolved_call() {
    let actual = compile_folder("playgrounds/ts-orders");
    let checker = reference("playground-ts-checker");
    let actual = edges(&actual.graph);
    let mut resolved = 0;
    let mut matched = 0;
    for (key, want) in edges(&checker) {
        if !matches!(want.kind, EdgeKind::Calls | EdgeKind::Constructs)
            || want.resolution != Resolution::Resolved
        {
            continue;
        }
        resolved += 1;
        if actual
            .get(&key)
            .is_some_and(|edge| edge.resolution == Resolution::Resolved && edge.target == want.target)
        {
            matched += 1;
        }
    }
    assert_eq!((matched, resolved), (71, 71));
}

#[test]
fn entrance_fixture_edges_match_the_reference() {
    let actual = compile_folder("fixtures/entrances");
    let expected = reference("entrances");
    let (actual, expected) = (edges(&actual.graph), edges(&expected));
    assert_eq!(actual.len(), expected.len());
    for (key, edge) in &actual {
        assert_eq!(*edge, expected[key], "{key:?}");
    }
}

#[test]
fn fixtures_without_references_extract_and_validate() {
    for folder in ["fixtures/worked", "fixtures/classes", "fixtures/playground-v1"] {
        let compiled = compile_folder(folder);
        assert!(
            compiled.graph.files.iter().all(|file| file.blocks.len() > 1),
            "{folder}"
        );
    }
}

/// The reference counts, so a silently empty comparison cannot pass.
#[test]
fn the_playground_reference_is_not_empty() {
    let expected = reference("playground-ts-native");
    let blocks: usize = expected.files.iter().map(|file| file.blocks.len()).sum();
    let slots: usize = expected.files.iter().map(|file| file.slots.len()).sum();
    let imports = expected
        .files
        .iter()
        .flat_map(|file| &file.edges)
        .filter(|edge| edge.kind == EdgeKind::Imports)
        .count();
    assert_eq!(
        (expected.files.len(), blocks, slots, imports),
        (34, 676, 1168, 76)
    );
    assert!(Path::new(&testdata_dir().join("playgrounds/ts-orders/src")).is_dir());
}
