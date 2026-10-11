use std::collections::BTreeMap;

use super::*;
use crate::key::Key;
use crate::model::{Entrance, Graph, ModuleLabel, Slot};
use crate::testkit::sample::sample;
use crate::testkit::{GraphBuilder, Outcome};
use crate::vocab::{
    DetectorId, EdgeKind, EntranceKind, FileState, HttpMethod, InputRole, Reason, Resolution, Role, SlotName,
    TargetRole, TriggerStatus,
};

fn sources(builder: &GraphBuilder) -> BTreeMap<RepoPath, Snapshot> {
    builder
        .snapshots()
        .into_iter()
        .map(|snapshot| (snapshot.path().clone(), snapshot))
        .collect()
}

fn messages(violations: &[Violation]) -> Vec<String> {
    violations.iter().map(ToString::to_string).collect()
}

/// Validates `graph` against the sample's snapshots and asserts some violation contains
/// `needle`.
fn assert_violation(graph: &Graph, needle: &str) {
    let builder = sample();
    let sources = sources(&builder);
    let violations = Validator::new().with_sources(&sources).validate(graph);
    let messages = messages(&violations);
    assert!(
        messages.iter().any(|message| message.contains(needle)),
        "no violation contains `{needle}`; found {messages:#?}"
    );
}

fn file_mut<'a>(graph: &'a mut Graph, path: &str) -> &'a mut FileGraph {
    graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == path)
        .expect("the sample has this file")
}

fn block_id(graph: &Graph, key: &str) -> (usize, usize) {
    for (file_index, file) in graph.files.iter().enumerate() {
        if let Some(index) = file.blocks.iter().position(|block| block.key.as_str() == key) {
            return (file_index, index);
        }
    }
    panic!("no block {key}");
}

struct AcceptAll(Language);

impl Census for AcceptAll {
    fn language(&self) -> Language {
        self.0
    }
    fn is_token_range(&self, _path: &RepoPath, _range: ByteRange) -> bool {
        true
    }
}

#[test]
fn the_sample_is_valid() {
    let builder = sample();
    let graph = builder.build();
    assert_eq!(messages(&validate(&graph)), Vec::<String>::new());
    let sources = sources(&builder);
    let typescript = AcceptAll(Language::TypeScript);
    let python = AcceptAll(Language::Python);
    let violations = Validator::new()
        .with_sources(&sources)
        .with_census(&typescript)
        .with_census(&python)
        .validate(&graph);
    assert_eq!(messages(&violations), Vec::<String>::new());
}

#[test]
fn files_must_be_sorted_and_consistent() {
    let mut graph = sample().build();
    graph.files.swap(0, 1);
    assert_violation(&graph, "not in strictly increasing path order");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/helper.ts").file.state = FileState::Failed;
    assert_violation(&graph, "a failed file has no facts");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/helper.ts").file.state = FileState::LastGood;
    assert_violation(&graph, "a last_good file needs its failure");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/helper.ts").file.hash = Some(crate::text::ContentHash::of(b"other"));
    assert_violation(&graph, "fingerprint does not match");
}

#[test]
fn blocks_must_form_a_valid_tree() {
    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").blocks[0].range.end -= 1;
    assert_violation(&graph, "a file root spans the whole file");

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:createOrder/if[0]/throw[0]");
    graph.files[file].blocks[index].range.end = graph.files[file].blocks[index - 1].range.end + 1;
    assert_violation(&graph, "does not contain");

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:createOrder/return[0]");
    // `return` claims the `if` as its parent although the `if`'s subtree has closed.
    graph.files[file].blocks[index + 1].parent = Some(3);
    assert_violation(&graph, "not in preorder");

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:save");
    graph.files[file].blocks[index].name = Some("renamed".into());
    assert_violation(
        &graph,
        "key differs from its derivation `src/orders.ts#function:renamed`",
    );

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:save/return[0]");
    graph.files[file].blocks[index].kind = BlockKind::With;
    assert_violation(&graph, "kind with does not occur in typescript files");

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:save/return[0]");
    graph.files[file].blocks[index].role = Some(Role::Header);
    assert_violation(
        &graph,
        "role header under function does not occur in typescript files",
    );

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#import[0]");
    graph.files[file].blocks[index].name = Some("helper".into());
    assert_violation(&graph, "an import has no name");
}

#[test]
fn ranges_are_checked_against_the_text() {
    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:save/return[0]");
    graph.files[file].blocks[index].range.start -= 1;
    assert_violation(&graph, "range starts with whitespace");

    let mut graph = sample().build();
    let (file, index) = block_id(&graph, "src/orders.ts#function:save/return[0]");
    graph.files[file].blocks[index].lines.end += 1;
    assert_violation(&graph, "disagree with the text");
}

#[test]
fn ranges_must_fall_on_character_boundaries() {
    let mut builder = GraphBuilder::new();
    let file = builder.file("a.ts", Language::TypeScript, "f(\"é\");\n");
    let statement = builder.block(
        file,
        BlockKind::ExpressionStatement,
        Some(Role::Body),
        None,
        builder.find(file, "f(\"é\");"),
    );
    let call = builder.block(
        statement,
        BlockKind::Call,
        Some(Role::Expression),
        None,
        builder.find(file, "f(\"é\")"),
    );
    builder.edge(
        call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "f",
        None,
    );
    builder.types(call).results.push(crate::typed::CallResult {
        call: call.local_id(),
        binding: crate::vocab::ResultBinding::Discard,
        awaited: false,
        target: None,
        position: None,
        bound: None,
    });
    let mut graph = builder.build();
    let sources = sources(&builder);
    assert_eq!(
        messages(&Validator::new().with_sources(&sources).validate(&graph)),
        Vec::<String>::new()
    );
    // Split the two-byte `é`.
    graph.files[0].slots.push(Slot {
        block: 2,
        slot: SlotName::Argument,
        index: Some(0),
        range: ByteRange { start: 2, end: 4 },
    });
    let messages = messages(&Validator::new().with_sources(&sources).validate(&graph));
    assert!(
        messages.iter().any(|m| m.contains("not on character boundaries")),
        "{messages:#?}"
    );
}

#[test]
fn slots_follow_the_slot_table() {
    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    let slot = Slot {
        block: 2,
        slot: SlotName::Heritage,
        index: None,
        range: file.blocks[2].range,
    };
    file.slots.insert(4, slot);
    assert_violation(&graph, "a typescript function has no heritage slot");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    let argument = file
        .slots
        .iter_mut()
        .find(|slot| slot.slot == SlotName::Argument && slot.index == Some(1))
        .unwrap();
    argument.index = Some(2);
    assert_violation(&graph, "indices must be contiguous from zero; expected 1");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    let name = file
        .slots
        .iter_mut()
        .find(|slot| slot.slot == SlotName::Name)
        .unwrap();
    name.index = Some(0);
    assert_violation(&graph, "a single slot has no index");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    file.slots.swap(1, 2);
    assert_violation(&graph, "slots are not in block, then start, order");
}

#[test]
fn occurrences_pair_with_exactly_one_edge() {
    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    file.edges.retain(|edge| edge.kind != EdgeKind::Constructs);
    assert_violation(&graph, "a new has exactly one occurrence edge, found 0");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    file.edges[1].kind = EdgeKind::Calls;
    assert_violation(&graph, "a typescript new cannot source a calls edge");
}

#[test]
fn references_follow_their_occurrence_in_argument_order() {
    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    file.edges[3].position = Some(2);
    assert_violation(&graph, "reference position 2 is outside the 2 arguments");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    let mut duplicate = file.edges[3].clone();
    duplicate.seq = 2;
    file.edges.push(duplicate);
    assert_violation(&graph, "reference positions must be distinct and increasing");

    let mut graph = sample().build();
    let file = file_mut(&mut graph, "src/orders.ts");
    file.edges.swap(2, 3);
    assert_violation(&graph, "a reference must follow its source's occurrence edge");
}

#[test]
fn outcomes_carry_exactly_their_fields() {
    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[0].reason = Some(Reason::Dynamic);
    assert_violation(&graph, "only an unresolved edge has a reason");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[1].module = None;
    assert_violation(&graph, "an external edge needs a module");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[1].module = Some(ModuleLabel::from_text(r#"["b","a"]"#));
    assert_violation(&graph, "is not a label or a sorted set of labels");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[0].target = Some(Key::from_text("src/missing.ts#"));
    assert_violation(&graph, "is not a block in the graph");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[0].target =
        Some(Key::from_text("src/helper.ts#function:helper"));
    assert_violation(&graph, "a imports edge cannot target a function");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[2].target =
        Some(Key::from_text("jobs/tasks.py#function:run"));
    assert_violation(&graph, "a resolved edge cannot cross languages");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/orders.ts").edges[2].text = "x".repeat(121).into();
    assert_violation(&graph, "text exceeds 120 UTF-16 code units");
}

#[test]
fn parameter_positions_stay_within_the_owner() {
    let mut graph = sample().build();
    file_mut(&mut graph, "jobs/tasks.py").edges[0].position = Some(1);
    assert_violation(&graph, "within its owner's 1 parameters");

    let mut graph = sample().build();
    file_mut(&mut graph, "src/server.ts").edges[0].position = Some(0);
    assert_violation(&graph, "position 0 is only for an unresolved parameter call");
}

#[test]
fn constructions_target_a_declared_constructor() {
    let mut builder = GraphBuilder::new();
    let file = builder.file(
        "a.ts",
        Language::TypeScript,
        "class A {\n  constructor() {}\n}\nnew A();\n",
    );
    let class = builder.block(
        file,
        BlockKind::Class,
        Some(Role::Body),
        Some("A"),
        builder.find(file, "class A {\n  constructor() {}\n}"),
    );
    builder.block(
        class,
        BlockKind::Constructor,
        Some(Role::Body),
        Some("constructor"),
        builder.find(file, "constructor() {}"),
    );
    let statement = builder.block(
        file,
        BlockKind::ExpressionStatement,
        Some(Role::Body),
        None,
        builder.find(file, "new A();"),
    );
    let construction = builder.block(
        statement,
        BlockKind::New,
        Some(Role::Expression),
        None,
        builder.find(file, "new A()"),
    );
    builder.edge(
        construction,
        EdgeKind::Constructs,
        Outcome::Resolved(builder.key(class)),
        "A",
        None,
    );
    let violations = messages(&validate(&builder.build()));
    assert!(
        violations
            .iter()
            .any(|m| m.contains("a construction targets the constructor")),
        "{violations:#?}"
    );
}

#[test]
fn entrances_are_ordered_and_keyed() {
    let mut graph = sample().build();
    graph.entrances.swap(0, 2);
    assert_violation(&graph, "entrances are not in evidence order");

    let mut graph = sample().build();
    graph.entrances[0].key = Key::from_text("entrance:wrong");
    assert_violation(
        &graph,
        "key differs from its derivation `entrance:[\"package.script\",\"process\",\"\",\"start\",\"src/server.ts\"]`",
    );

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).key =
        Key::from_text(r#"entrance:["manual","worker","","orders","jobs/tasks.py#run"]"#);
    assert_violation(&graph, "a manual entrance's key is `entrance:configured:<id>`");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).key = Key::configured_entrance("orders");
    assert_violation(&graph, "key differs from its derivation");
}

fn entrance_mut(graph: &mut Graph, detector: DetectorId) -> &mut Entrance {
    graph
        .entrances
        .iter_mut()
        .find(|entrance| entrance.detector == detector)
        .expect("the sample has this detector")
}

#[test]
fn entrance_fields_follow_their_detector() {
    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).kind = EntranceKind::Worker;
    assert_violation(&graph, "detector express.route does not produce worker entrances");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).method = Some(HttpMethod::Get);
    assert_violation(&graph, "only an http entrance has a method");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).trigger_status = TriggerStatus::Unknown;
    assert_violation(
        &graph,
        "trigger status is unknown exactly when the trigger is absent",
    );

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).label = Some("Orders".into());
    assert_violation(&graph, "only manual entrances have labels");
}

#[test]
fn evidence_matches_its_source() {
    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).evidence.block = None;
    assert_violation(&graph, "source evidence needs its block");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute)
        .evidence
        .range
        .start += 1;
    assert_violation(&graph, "does not have the evidence's file and range");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).evidence.pointer = None;
    assert_violation(&graph, "configuration evidence needs a pointer");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).evidence.pointer = Some("/entrance/~2".into());
    assert_violation(&graph, "is not a valid JSON pointer");

    let mut graph = sample().build();
    graph.inputs[1].role = InputRole::Tsconfig;
    assert_violation(
        &graph,
        "manual evidence belongs in a unfold-config input, not tsconfig",
    );

    let mut graph = sample().build();
    graph.inputs[0].hash = crate::text::ContentHash::of(b"changed");
    assert_violation(&graph, "input package.json: fingerprint");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::Manual).evidence.range.end = 10_000;
    assert_violation(&graph, "outside the file or not on character boundaries");
}

#[test]
fn targets_follow_their_entrance() {
    let mut graph = sample().build();
    let process = entrance_mut(&mut graph, DetectorId::PackageScript);
    let mut second = process.targets[0].clone();
    second.position = 1;
    process.targets.push(second);
    assert_violation(&graph, "a process has exactly one startup target, found 2");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).targets[0].role = TargetRole::Startup;
    assert_violation(&graph, "a http entrance's targets have role handler");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).targets[0].target =
        Some(Key::from_text("src/orders.ts#function:createOrder/if[0]"));
    assert_violation(&graph, "a handler target of express.route cannot be a if");

    let mut graph = sample().build();
    entrance_mut(&mut graph, DetectorId::ExpressRoute).targets[0].target =
        Some(Key::from_text("jobs/tasks.py#function:run"));
    assert_violation(&graph, "express.route targets must be typescript");

    let mut graph = sample().build();
    let manual = entrance_mut(&mut graph, DetectorId::Manual);
    let mut second = manual.targets[0].clone();
    second.position = 1;
    second.target = Some(Key::from_text("src/orders.ts#function:save"));
    manual.targets.push(second);
    assert_violation(&graph, "resolved targets cannot mix languages");

    let mut graph = sample().build();
    let target = &mut entrance_mut(&mut graph, DetectorId::Manual).targets[0];
    target.resolution = Resolution::Unresolved;
    assert_violation(&graph, "an unresolved target needs a reason");
}

struct Strict;

impl Census for Strict {
    fn language(&self) -> Language {
        Language::TypeScript
    }
    fn is_token_range(&self, _path: &RepoPath, range: ByteRange) -> bool {
        range.start != 9
    }
    fn call_sites(&self, path: &RepoPath) -> Option<Vec<CallSite>> {
        (path.as_str() == "src/server.ts").then(|| {
            vec![CallSite {
                range: ByteRange { start: 0, end: 32 },
                block_kind: BlockKind::Call,
                edge_kind: Some(EdgeKind::Constructs),
                ineligible_references: vec![1],
            }]
        })
    }
    fn module_dependencies(&self, path: &RepoPath) -> Option<Vec<ByteRange>> {
        (path.as_str() == "src/orders.ts").then(Vec::new)
    }
}

#[test]
fn the_census_is_independent_evidence() {
    let graph = sample().build();
    let violations = messages(&Validator::new().with_census(&Strict).validate(&graph));
    let expect = [
        "is not on token boundaries",
        "occurrence edge is calls, but the front end found constructs",
        "the argument at this position cannot supply a named reference",
        "the import does not match exactly one module dependency",
    ];
    for needle in expect {
        assert!(
            violations.iter().any(|m| m.contains(needle)),
            "no `{needle}` in {violations:#?}"
        );
    }
    assert!(
        !violations.iter().any(|m| m.contains("jobs/tasks.py")),
        "the census covers TypeScript only: {violations:#?}"
    );
}

#[test]
fn duplicate_keys_across_files_are_reported() {
    let mut graph = sample().build();
    let block = graph.files[1].blocks[1].clone();
    graph.files[2].blocks.push(block);
    assert_violation(&graph, "duplicate structural key");
}

#[test]
fn malformed_local_ids_are_reported_without_panicking() {
    let builder = sample();
    let sources = sources(&builder);
    let mut graph = builder.build();
    for file in &mut graph.files {
        for block in &mut file.blocks {
            block.local_id = block.local_id.wrapping_add(1_000_000);
        }
    }
    let violations = messages(
        &Validator::new()
            .with_sources(&sources)
            .with_census(&Strict)
            .validate(&graph),
    );
    assert!(
        violations
            .iter()
            .any(|m| m.contains("is not its preorder position")),
        "{violations:#?}"
    );
}

/// One corruption of a numeric field the validator indexes with.
#[derive(Debug, Clone)]
enum Corruption {
    LocalId(usize, u32),
    Parent(usize, Option<u32>),
    Range(usize, u32, u32),
    SlotBlock(usize, u32),
    EdgeSource(usize, u32),
    EdgePosition(usize, Option<u32>),
}

fn corruption() -> impl proptest::strategy::Strategy<Value = Corruption> {
    use proptest::prelude::*;
    let id = prop_oneof![0u32..40, Just(u32::MAX), Just(u32::MAX - 1)];
    prop_oneof![
        (any::<usize>(), id.clone()).prop_map(|(at, id)| Corruption::LocalId(at, id)),
        (any::<usize>(), proptest::option::of(id.clone())).prop_map(|(at, id)| Corruption::Parent(at, id)),
        (any::<usize>(), id.clone(), id.clone()).prop_map(|(at, a, b)| Corruption::Range(at, a, b)),
        (any::<usize>(), id.clone()).prop_map(|(at, id)| Corruption::SlotBlock(at, id)),
        (any::<usize>(), id.clone()).prop_map(|(at, id)| Corruption::EdgeSource(at, id)),
        (any::<usize>(), proptest::option::of(id)).prop_map(|(at, id)| Corruption::EdgePosition(at, id)),
    ]
}

fn corrupt(file: &mut FileGraph, corruption: &Corruption) {
    fn pick<T>(items: &mut [T], at: usize) -> Option<&mut T> {
        let len = items.len();
        (len > 0).then(|| &mut items[at % len])
    }
    match *corruption {
        Corruption::LocalId(at, id) => pick(&mut file.blocks, at).map(|block| block.local_id = id),
        Corruption::Parent(at, parent) => pick(&mut file.blocks, at).map(|block| block.parent = parent),
        Corruption::Range(at, start, end) => {
            pick(&mut file.blocks, at).map(|block| block.range = ByteRange { start, end })
        }
        Corruption::SlotBlock(at, id) => pick(&mut file.slots, at).map(|slot| slot.block = id),
        Corruption::EdgeSource(at, id) => pick(&mut file.edges, at).map(|edge| edge.source = id),
        Corruption::EdgePosition(at, position) => {
            pick(&mut file.edges, at).map(|edge| edge.position = position)
        }
    };
}

proptest::proptest! {
    /// The validator reports malformed graphs; it never panics on them.
    #[test]
    fn corrupted_graphs_never_panic(
        corruptions in proptest::collection::vec((0usize..4, corruption()), 1..6)
    ) {
        let builder = sample();
        let sources = sources(&builder);
        let mut graph = builder.build();
        for (file, corruption) in &corruptions {
            let count = graph.files.len();
            corrupt(&mut graph.files[file % count], corruption);
        }
        let _ = Validator::new()
            .with_sources(&sources)
            .with_census(&Strict)
            .with_census(&AcceptAll(Language::Python))
            .validate(&graph);
    }
}
