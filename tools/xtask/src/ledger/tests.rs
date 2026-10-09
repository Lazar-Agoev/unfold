use std::collections::BTreeSet;

use unfold_core::testkit::sample::sample;
use unfold_core::vocab::{Reason, Resolution};
use unfold_core::{Graph, Key};

use super::*;
use crate::compare::diff;

const CORPUS: &str = "sample";

fn corpora() -> BTreeSet<&'static str> {
    BTreeSet::from([CORPUS, "other"])
}

/// The sample graph, and a copy where the first resolved edge no longer resolves (three changed
/// fields) and the first input is gone. Returns the edge's source key and seq.
fn graphs() -> (Graph, Graph, Key, u32) {
    let expected = sample().build();
    let mut actual = expected.clone();
    let file = actual
        .files
        .iter_mut()
        .find(|file| {
            file.edges
                .iter()
                .any(|edge| edge.resolution == Resolution::Resolved)
        })
        .unwrap();
    let index = file
        .edges
        .iter()
        .position(|edge| edge.resolution == Resolution::Resolved)
        .unwrap();
    let key = file.blocks[file.edges[index].source as usize].key.clone();
    let seq = file.edges[index].seq;
    let edge = &mut file.edges[index];
    edge.resolution = Resolution::Unresolved;
    edge.reason = Some(Reason::Dynamic);
    edge.target = None;
    actual.inputs.remove(0);
    (expected, actual, key, seq)
}

fn edge_entry(key: &Key, seq: u32, rest: &str) -> String {
    format!(
        "[[divergence]]\ncorpus = \"{CORPUS}\"\nrecord = \"edge\"\nkey = \"{key}\"\nseq = {seq}\n{rest}\nreason = \"test\"\n"
    )
}

fn input_entry(path: &str, change: &str) -> String {
    format!(
        "[[divergence]]\ncorpus = \"{CORPUS}\"\nrecord = \"input\"\nkey = \"{path}\"\nchange = \"{change}\"\nreason = \"test\"\n"
    )
}

#[test]
fn an_empty_ledger_lists_nothing() {
    let (expected, actual, ..) = graphs();
    let differences = diff(&expected, &actual, &Scope::ALL).unwrap();
    let ledger = Ledger::parse("# nothing yet\n", &corpora()).unwrap();
    let outcome = ledger.check(CORPUS, &Scope::ALL, &differences);
    assert!(!outcome.passed());
    // Three changed edge fields and one missing input.
    assert_eq!(outcome.unlisted.len(), 4);
    assert!(outcome.listed.is_empty() && outcome.stale.is_empty());
    assert!(describe_unit(&outcome.unlisted[0]).contains(": target: "));
}

#[test]
fn whole_record_and_field_entries_cover_their_units() {
    let (expected, actual, key, seq) = graphs();
    let differences = diff(&expected, &actual, &Scope::ALL).unwrap();
    let removed = expected.inputs[0].path.as_str();

    // A whole-record entry covers every changed field.
    let text = edge_entry(&key, seq, "change = \"changed\"") + &input_entry(removed, "missing");
    let ledger = Ledger::parse(&text, &corpora()).unwrap();
    let outcome = ledger.check(CORPUS, &Scope::ALL, &differences);
    assert!(outcome.passed(), "{outcome:?}");
    assert_eq!(outcome.listed.len(), 4);

    // Field entries cover one field each; the uncovered field stays unlisted.
    let text = edge_entry(
        &key,
        seq,
        "change = \"changed\"\nfield = \"resolution\"\nexpected = \"resolved\"\nactual = \"unresolved\"",
    ) + &edge_entry(
        &key,
        seq,
        "change = \"changed\"\nfield = \"reason\"\nexpected = \"(absent)\"",
    ) + &input_entry(removed, "missing");
    let ledger = Ledger::parse(&text, &corpora()).unwrap();
    let outcome = ledger.check(CORPUS, &Scope::ALL, &differences);
    assert!(!outcome.passed());
    assert!(outcome.stale.is_empty());
    let unlisted: Vec<String> = outcome.unlisted.iter().map(describe_unit).collect();
    assert_eq!(unlisted.len(), 1);
    assert!(unlisted[0].contains(": target: "), "{unlisted:?}");
}

#[test]
fn entries_that_no_longer_occur_are_stale() {
    let (expected, actual, key, seq) = graphs();
    let differences = diff(&expected, &actual, &Scope::ALL).unwrap();
    let removed = expected.inputs[0].path.as_str();
    let text = edge_entry(&key, seq, "change = \"changed\"")
        + &input_entry(removed, "missing")
        // Not a difference at all.
        + &input_entry("never/package.json", "extra")
        // The right record but the wrong category.
        + &edge_entry(&key, seq, "change = \"missing\"")
        // The right field but a different value: stale, and the field is still covered above.
        + &edge_entry(&key, seq, "change = \"changed\"\nfield = \"reason\"\nactual = \"unsupported\"");
    let ledger = Ledger::parse(&text, &corpora()).unwrap();
    let outcome = ledger.check(CORPUS, &Scope::ALL, &differences);
    assert!(outcome.unlisted.is_empty());
    let stale: Vec<usize> = outcome.stale.iter().map(|(number, _)| *number).collect();
    assert_eq!(stale, [3, 4, 5]);
    assert!(
        describe_entry(3, outcome.stale[0].1).starts_with("divergence 3 (input never/package.json extra)")
    );

    // Once the divergence is gone, its entry is stale too.
    let outcome = ledger.check(CORPUS, &Scope::ALL, &[]);
    assert_eq!(outcome.stale.len(), 5);
}

#[test]
fn other_corpora_and_scopes_are_ignored() {
    let (expected, actual, key, seq) = graphs();
    let removed = expected.inputs[0].path.as_str();
    let text = edge_entry(&key, seq, "change = \"changed\"").replace(CORPUS, "other")
        + &input_entry(removed, "missing");
    let ledger = Ledger::parse(&text, &corpora()).unwrap();

    let edges_only = diff(&expected, &actual, &[Scope::Edges]).unwrap();
    let outcome = ledger.check(CORPUS, &[Scope::Edges], &edges_only);
    // The edge entry belongs to another corpus; the input entry is outside the scope.
    assert_eq!(outcome.unlisted.len(), 3);
    assert!(outcome.stale.is_empty());

    let outcome = ledger.check("other", &[Scope::Edges], &edges_only);
    assert!(outcome.passed());
}

#[test]
fn malformed_entries_are_rejected() {
    let key = Key::from_text("src/a.ts#call[0]");
    let reject = |text: &str| assert!(Ledger::parse(text, &corpora()).is_err(), "{text}");
    // An edge needs its seq.
    reject(&edge_entry(&key, 0, "change = \"missing\"").replace("seq = 0\n", ""));
    // A field only goes with `changed`, and must be a field of the record type.
    reject(&edge_entry(
        &key,
        0,
        "change = \"missing\"\nfield = \"resolution\"",
    ));
    reject(&edge_entry(&key, 0, "change = \"changed\"\nfield = \"range\""));
    // Values need a field.
    reject(&edge_entry(
        &key,
        0,
        "change = \"changed\"\nexpected = \"resolved\"",
    ));
    // Slots need a slot name; other records must not have one.
    reject(&input_entry("package.json", "missing").replace("reason", "slot = \"body\"\nreason"));
    // Unknown corpora, unknown keys, empty reasons, and repeated selectors.
    reject(&input_entry("package.json", "missing").replace(CORPUS, "absent"));
    reject(&input_entry("package.json", "missing").replace("reason", "note = \"x\"\nreason"));
    reject(&input_entry("package.json", "missing").replace("\"test\"", "\" \""));
    reject(&(input_entry("package.json", "missing") + &input_entry("package.json", "missing")));
    // A well-formed slot entry is accepted.
    let slot = "[[divergence]]\ncorpus = \"sample\"\nrecord = \"slot\"\nkey = \"src/a.ts#function:f\"\n\
                slot = \"parameter\"\nindex = 1\nchange = \"extra\"\nreason = \"test\"\n";
    let ledger = Ledger::parse(slot, &corpora()).unwrap();
    assert_eq!(
        ledger.entries[0].record_id().to_string(),
        "slot src/a.ts#function:f parameter[1]"
    );
}

#[test]
fn the_repository_ledger_is_valid() {
    let corpora =
        crate::reference::corpora::Corpora::load(&crate::reference::corpora::Corpora::default_path())
            .unwrap();
    let names: BTreeSet<&str> = corpora.corpus.iter().map(|corpus| corpus.name.as_str()).collect();
    Ledger::load(&Ledger::default_path(), &names).unwrap();
}
