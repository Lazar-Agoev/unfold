//! Structural parity with the proof of concept: the `py-orders` playground's files, blocks, and
//! slots against `testdata/reference/canonical/python-playground.jsonl`, record for record,
//! compiled with the engine (so the named child imports linking keeps are included).

mod common;

use std::collections::BTreeSet;

use common::{canonical_records, compile_folder, compile_folder_with_threads, testdata};

const KINDS: &[&str] = &["file", "block", "slot"];

#[test]
fn py_orders_blocks_and_slots_match_the_reference() {
    let compiled = compile_folder(&testdata().join("playgrounds/py-orders")).valid();
    assert!(compiled.failures.is_empty(), "{:?}", compiled.failures);
    let actual: Vec<String> = compiled.records(KINDS);
    let reference = std::fs::read_to_string(testdata().join("reference/canonical/python-playground.jsonl"))
        .expect("read the reference");
    let expected: Vec<String> = canonical_records(&reference, KINDS);
    let actual_set: BTreeSet<&String> = actual.iter().collect();
    let expected_set: BTreeSet<&String> = expected.iter().collect();
    let missing: Vec<&&String> = expected_set.difference(&actual_set).collect();
    let extra: Vec<&&String> = actual_set.difference(&expected_set).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{} missing, {} extra\nmissing:\n{}\nextra:\n{}",
        missing.len(),
        extra.len(),
        missing
            .iter()
            .take(40)
            .map(|record| record.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        extra
            .iter()
            .take(40)
            .map(|record| record.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    assert_eq!(actual, expected, "the same records in the same order");
    // 50 files, 888 blocks (six of them named child imports, which linking keeps), and 1,598
    // slots in the reference.
    assert_eq!(actual.len(), 50 + 888 + 1598);
}

#[test]
fn py_orders_exports_identically_across_thread_counts() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let source = testdata().join("playgrounds/py-orders");
    let mut exports = Vec::new();
    for threads in [1, 4] {
        let copy = root.path().join(format!("t{threads}"));
        std::fs::create_dir_all(&copy).expect("create a folder");
        common::copy_folder(&source, &copy);
        let compiled = compile_folder_with_threads(&copy, threads).valid();
        exports.push(unfold_core::canonical::to_string(&compiled.graph));
    }
    assert_eq!(exports[0], exports[1]);
    assert!(exports[0].contains("\"kind\":\"constructs\""));
}

/// With the detectors, the whole canonical export (files, blocks, slots, edges, entrances,
/// targets, and inputs) equals the reference record for record: the divergence ledger has no
/// entry for this corpus. The reference is a version-1 export, which predates typed records, so
/// the graph is compared as version 1 describes it (`canonical::version_one`, written with a
/// version-1 header); Python has no type declaration blocks, so nothing else is left out.
#[test]
fn py_orders_exports_exactly_the_reference() {
    let compiled = compile_folder(&testdata().join("playgrounds/py-orders")).valid();
    assert!(
        compiled.graph.files.iter().any(|file| !file.types.is_empty()),
        "the graph has typed records, which the version-1 view leaves out"
    );
    let mut bytes = Vec::new();
    unfold_core::canonical::write_version(
        &unfold_core::canonical::version_one(&compiled.graph),
        1,
        &mut bytes,
    )
    .expect("write the version-1 export");
    let actual = String::from_utf8(bytes).expect("UTF-8");
    let reference = std::fs::read_to_string(testdata().join("reference/canonical/python-playground.jsonl"))
        .expect("read the reference");
    let actual: Vec<&str> = actual.lines().collect();
    let expected: Vec<&str> = reference.lines().collect();
    let differing: Vec<String> = expected
        .iter()
        .filter(|line| !actual.contains(line))
        .map(|line| format!("missing {line}"))
        .chain(
            actual
                .iter()
                .filter(|line| !expected.contains(line))
                .map(|line| format!("extra {line}")),
        )
        .take(20)
        .collect();
    assert!(differing.is_empty(), "{}", differing.join("\n"));
    assert_eq!(actual, expected, "the same records in the same order");
}
