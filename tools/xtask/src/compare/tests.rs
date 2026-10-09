use unfold_core::testkit::sample::sample;
use unfold_core::vocab::{InputRole, Reason, Resolution};
use unfold_core::{ByteRange, ContentHash, InputRecord, RepoPath};

use super::*;

fn categories(differences: &[Difference]) -> Vec<(RecordType, Category)> {
    differences
        .iter()
        .map(|difference| (difference.id.record, difference.change.category()))
        .collect()
}

#[test]
fn identical_graphs_have_no_differences() {
    let graph = sample().build();
    assert!(diff(&graph, &graph, &Scope::ALL).unwrap().is_empty());
}

#[test]
fn reports_missing_extra_and_changed_records_by_identity() {
    let expected = sample().build();
    let mut actual = expected.clone();

    // A resolved edge that no longer resolves: three fields change, reported in field order.
    let file = actual
        .files
        .iter_mut()
        .find(|file| {
            file.edges
                .iter()
                .any(|edge| edge.resolution == Resolution::Resolved)
        })
        .unwrap();
    let edge_index = file
        .edges
        .iter()
        .position(|edge| edge.resolution == Resolution::Resolved)
        .unwrap();
    let edge_key = file.blocks[file.edges[edge_index].source as usize].key.clone();
    let edge_seq = file.edges[edge_index].seq;
    let edge = &mut file.edges[edge_index];
    let old_target = edge.target.clone().unwrap();
    edge.resolution = Resolution::Unresolved;
    edge.reason = Some(Reason::Dynamic);
    edge.target = None;
    // A slot whose range moved.
    let slot = &mut file.slots[0];
    slot.range = ByteRange::new(slot.range.start, slot.range.end + 1).unwrap();
    // Order inside the export must not matter.
    file.slots.reverse();
    // One input removed, one added.
    let removed = actual.inputs.remove(0);
    actual.inputs.push(InputRecord {
        path: RepoPath::new("extra/pyproject.toml").unwrap(),
        role: InputRole::Pyproject,
        hash: ContentHash::of(b"x"),
    });
    // An entrance dropped: it and its targets are missing.
    let dropped = actual.entrances.pop().unwrap();

    let differences = diff(&expected, &actual, &Scope::ALL).unwrap();
    let mut wanted = vec![
        (RecordType::Slot, Category::Changed),
        (RecordType::Edge, Category::Changed),
    ];
    wanted.push((RecordType::Input, Category::Missing));
    wanted.push((RecordType::Input, Category::Extra));
    wanted.push((RecordType::Entrance, Category::Missing));
    wanted.extend(
        dropped
            .targets
            .iter()
            .map(|_| (RecordType::Target, Category::Missing)),
    );
    assert_eq!(categories(&differences), wanted);

    let edge_difference = &differences[1];
    assert_eq!(edge_difference.id.key.as_ref(), edge_key.as_str());
    assert_eq!(edge_difference.id.seq, Some(edge_seq));
    let Change::Changed(fields) = &edge_difference.change else {
        panic!("expected a change");
    };
    let expected_fields = [
        FieldChange {
            field: "target",
            expected: Some(old_target.as_str().to_owned()),
            actual: None,
        },
        FieldChange {
            field: "resolution",
            expected: Some("resolved".to_owned()),
            actual: Some("unresolved".to_owned()),
        },
        FieldChange {
            field: "reason",
            expected: None,
            actual: Some("dynamic".to_owned()),
        },
    ];
    assert_eq!(fields.as_slice(), expected_fields);

    assert_eq!(differences[2].id.key.as_ref(), removed.path.as_str());
    assert_eq!(differences[4].id.key.as_ref(), dropped.key.as_str());

    let summary = Summary::of_differences(&differences);
    assert_eq!(summary.total(), differences.len());
    let json = summary.to_json();
    assert_eq!(json["input"]["missing"], 1);
    assert_eq!(json["edge"]["changed"], 1);
    assert_eq!(json["block"]["extra"], 0);
    let listed: Vec<&Difference> = differences.iter().collect();
    let text = render_differences(&listed, 1);
    assert!(text.contains("input missing (1):"));
    assert!(text.contains("resolution: "));
    assert!(text.contains("reason: "));
}

#[test]
fn scopes_limit_the_record_types() {
    let expected = sample().build();
    let mut actual = expected.clone();
    actual.inputs.clear();
    actual.files[0].blocks[1].name = Some("renamed".into());
    let only_inputs = diff(&expected, &actual, &[Scope::Inputs]).unwrap();
    assert!(only_inputs.iter().all(|d| d.id.record == RecordType::Input));
    assert!(!only_inputs.is_empty());
    let only_blocks = diff(&expected, &actual, &[Scope::Blocks]).unwrap();
    assert_eq!(categories(&only_blocks), [(RecordType::Block, Category::Changed)]);
    let Change::Changed(fields) = &only_blocks[0].change else {
        panic!("expected a change");
    };
    assert_eq!(fields[0].actual.as_deref(), Some("renamed"));
}

#[test]
fn absent_values_render_as_absent() {
    let expected = sample().build();
    let mut actual = expected.clone();
    let block = actual.files[0]
        .blocks
        .iter_mut()
        .find(|block| block.name.is_some())
        .unwrap();
    block.name = None;
    let differences = diff(&expected, &actual, &[Scope::Blocks]).unwrap();
    let Change::Changed(fields) = &differences[0].change else {
        panic!("expected a change");
    };
    assert_eq!(fields[0].actual, None);
    assert!(render::render_fields(fields).ends_with(&format!("-> {ABSENT}")));
}

#[test]
fn repeated_identities_are_an_error() {
    let expected = sample().build();
    let mut actual = expected.clone();
    let input = actual.inputs[0].clone();
    actual.inputs.push(input);
    assert!(diff(&expected, &actual, &Scope::ALL).is_err());
}
