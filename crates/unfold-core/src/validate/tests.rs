use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::model::{Link, LinkTarget};
use crate::testkit::sample::shop;
use crate::testkit::{GraphBuilder, target};
use crate::text::ContentHash;
use crate::typed::Signature;
use crate::vocab::{FileState, Flag, LinkKind, NodeKind, Reason, TargetState};

fn sample() -> Graph {
    shop().build()
}

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

/// Every violation of `graph`, validated against the sample's snapshots.
fn violations(graph: &Graph) -> Vec<String> {
    let sources = sources(&shop());
    messages(&Validator::new().with_sources(&sources).validate(graph))
}

/// Asserts that validating `graph` reports exactly `expected` among its violations.
#[track_caller]
fn assert_violation(graph: &Graph, expected: &str) {
    let found = violations(graph);
    assert!(
        found.iter().any(|message| message == expected),
        "no violation is `{expected}`; found {found:#?}"
    );
}

fn file_mut<'g>(graph: &'g mut Graph, path: &str) -> &'g mut FileGraph {
    graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == path)
        .map(Arc::make_mut)
        .expect("the sample has this file")
}

/// The file and local ID of the node `key` names.
fn locate(graph: &Graph, key: &str) -> (usize, u32) {
    for (index, file) in graph.files.iter().enumerate() {
        if let Some(id) = file.local_id(key) {
            return (index, id);
        }
    }
    panic!("no node {key}");
}

fn node_mut<'g>(graph: &'g mut Graph, key: &str) -> &'g mut Node {
    let (file, id) = locate(graph, key);
    &mut Arc::make_mut(&mut graph.files[file]).nodes[id as usize]
}

/// The links of node `key`'s file that start at it.
fn links_mut<'g>(graph: &'g mut Graph, key: &str) -> impl Iterator<Item = &'g mut Link> {
    let (file, id) = locate(graph, key);
    Arc::make_mut(&mut graph.files[file])
        .links
        .iter_mut()
        .filter(move |link| link.from == id)
}

/// Puts a file's links back in `from` order and renumbers their `seq`s.
fn renumber(file: &mut FileGraph) {
    file.links.sort_by_key(|link| (link.from, link.seq));
    let mut previous = None;
    let mut seq = 0;
    for link in &mut file.links {
        seq = if previous == Some(link.from) { seq + 1 } else { 0 };
        previous = Some(link.from);
        link.seq = seq;
    }
}

/// A census computed from the graph itself: every call node is a call site (with its form, for
/// TypeScript).
struct GraphCensus {
    language: Language,
    calls: BTreeMap<RepoPath, Vec<CallSite>>,
}

impl GraphCensus {
    fn of(graph: &Graph, language: Language) -> Self {
        let calls = graph
            .files
            .iter()
            .filter(|file| file.file.language == language)
            .map(|file| {
                let calls = file
                    .nodes
                    .iter()
                    .filter(|node| node.kind() == NodeKind::Call)
                    .map(|node| CallSite {
                        range: node.range,
                        form: (language == Language::TypeScript).then_some(node.form),
                    })
                    .collect();
                (file.file.path.clone(), calls)
            })
            .collect();
        Self { language, calls }
    }
}

impl Census for GraphCensus {
    fn language(&self) -> Language {
        self.language
    }

    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]> {
        self.calls.get(path).map(Vec::as_slice)
    }
}

const SERVICE: &str = "shop/service.py";
const PLACE_ORDER: &str = "shop/service.py#place_order";

#[test]
fn the_sample_is_valid() {
    let graph = sample();
    assert_eq!(messages(&Validator::new().validate(&graph)), Vec::<String>::new());
    let sources = sources(&shop());
    let python = GraphCensus::of(&graph, Language::Python);
    let typescript = GraphCensus::of(&graph, Language::TypeScript);
    let validator = Validator::new()
        .with_sources(&sources)
        .with_census(&python)
        .with_census(&typescript);
    assert_eq!(messages(&validator.validate(&graph)), Vec::<String>::new());
}

#[test]
fn entrance_states_are_data_not_errors() {
    let mut graph = sample();
    let states: Vec<TargetState> = graph.apps[0]
        .entrances
        .iter()
        .flat_map(|entrance| &entrance.code)
        .map(|target| target.state)
        .collect();
    assert!(states.contains(&TargetState::Missing));
    assert_eq!(violations(&graph), Vec::<String>::new());
    // A state that disagrees with the graph is still data: the store decides states.
    for entrance in &mut graph.apps[0].entrances {
        entrance
            .code
            .push(target("shop/absent.py#gone", TargetState::Missing));
        for target in &mut entrance.code {
            target.state = TargetState::Stale;
        }
    }
    assert_eq!(violations(&graph), Vec::<String>::new());
}

#[test]
fn files_and_inputs_are_sorted_and_consistent() {
    let mut graph = sample();
    graph.files.swap(0, 1);
    assert_violation(
        &graph,
        "file shop/payments.py: files are not in strictly increasing path order",
    );

    let mut graph = sample();
    file_mut(&mut graph, "shop/store.py").file.state = FileState::Failed;
    assert_violation(
        &graph,
        "file shop/store.py: a failed file has no facts: no hash, nodes, links, or typed records",
    );

    let mut graph = sample();
    file_mut(&mut graph, "shop/store.py").file.hash = Some(ContentHash::of(b"other"));
    let snapshot = sources(&shop())[&RepoPath::new("shop/store.py").unwrap()].hash();
    assert_violation(
        &graph,
        &format!("file shop/store.py: hash does not match the snapshot's {snapshot}"),
    );

    let mut graph = sample();
    graph.inputs[0].hash = ContentHash::of(b"other");
    let found = violations(&graph);
    assert!(
        found
            .iter()
            .any(|message| message.starts_with("input unfold.toml: hash ")),
        "{found:#?}"
    );
}

#[test]
fn nodes_are_in_preorder() {
    let mut graph = sample();
    // `call:sum` claims the `if`'s `then` part, whose subtree has closed.
    let (_, then) = locate(&graph, "shop/service.py#place_order/if[0]/then");
    node_mut(&mut graph, "shop/service.py#place_order/call:sum").parent = Some(then);
    assert_violation(
        &graph,
        &format!(
            "node shop/service.py#place_order/call:sum: parent {then} is not an open ancestor, so nodes are not in preorder"
        ),
    );
}

#[test]
fn containment_follows_the_grammar() {
    // An exit inside a call's arguments.
    let mut graph = sample();
    let status = "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]/call:res.status().json/call:res.status";
    node_mut(&mut graph, status).form = Form::Return;
    assert_violation(
        &graph,
        &format!("node {status}: a call holds only the expression blocks in its receiver and arguments"),
    );

    // A node inside a type.
    let mut graph = sample();
    node_mut(&mut graph, "src/orders.ts#/call:Router").parent = Some(1);
    assert_violation(&graph, "node src/orders.ts#/call:Router: a type contains nothing");

    // A part directly in a function.
    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/call:load_order").form = Form::Body;
    assert_violation(
        &graph,
        "node shop/service.py#place_order/call:load_order: parts sit directly in control blocks, not in function nodes",
    );

    // A call in a loop that is not in the loop's header.
    let mut graph = sample();
    node_mut(&mut graph, "shop/store.py#reserve_stock/for[0]").head = None;
    assert_violation(
        &graph,
        "node shop/store.py#reserve_stock/for[0]/call:sorted: an expression block lies in the header of its for block",
    );
}

#[test]
fn control_blocks_have_their_parts_in_order() {
    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/if[0]/then").form = Form::Else;
    assert_violation(
        &graph,
        "node shop/service.py#place_order/if[0]: if blocks need a then part",
    );
    assert_violation(
        &graph,
        "node shop/service.py#place_order/if[0]/elif[0]: the elif part is out of written order",
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/try[0]/except[0]").form = Form::Elif;
    assert_violation(
        &graph,
        "node shop/service.py#place_order/try[0]/except[0]: try blocks have no elif part",
    );
}

#[test]
fn forms_and_flags_follow_language_and_form() {
    let mut graph = sample();
    node_mut(&mut graph, "src/orders.ts#loadOrder/return[0]").form = Form::Raise;
    assert_violation(
        &graph,
        "node src/orders.ts#loadOrder/return[0]: form raise does not occur in typescript files",
    );

    let mut graph = sample();
    node_mut(&mut graph, PLACE_ORDER).flags.insert(Flag::Awaited);
    assert_violation(
        &graph,
        "node shop/service.py#place_order: flag awaited does not apply to function nodes",
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/call:save_order")
        .flags
        .insert(Flag::ExitValue);
    assert_violation(
        &graph,
        "node shop/service.py#place_order/call:save_order: an exit_value block is written as its exit's value",
    );
}

#[test]
fn fields_belong_to_their_kinds() {
    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/return[0]").binds = Some("x".into());
    assert_violation(
        &graph,
        "node shop/service.py#place_order/return[0]: only a call, ternary, or comprehension binds a target",
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/return[0]").target = Some(1);
    assert_violation(
        &graph,
        "node shop/service.py#place_order/return[0]: only break and continue have a target",
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/return[0]").stmt = None;
    assert_violation(
        &graph,
        "node shop/service.py#place_order/return[0]: an exit needs its statement's range as its stmt",
    );

    // The decorator's `for` names a call, not a definition.
    let mut graph = sample();
    let (_, dict) = locate(&graph, "shop/payments.py#/call:logging.getLogger");
    node_mut(&mut graph, "shop/payments.py#/call:retry").r#for = Some(dict);
    assert_violation(
        &graph,
        &format!("node shop/payments.py#/call:retry: `for` {dict} is not a definition in the same body"),
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/payments.py#/call:retry").r#for = None;
    assert_violation(
        &graph,
        "node shop/payments.py#/call:retry: decorator calls name the definition they belong to in `for`",
    );

    // A callback may carry the name of the property it is written as; its key stays its
    // position. Any other inline function is unnamed.
    let mut graph = sample();
    node_mut(
        &mut graph,
        "shop/store.py#reserve_stock/for[0]/call:sorted/callback[0]",
    )
    .name = Some("key".into());
    assert_eq!(violations(&graph), Vec::<String>::new());
    node_mut(&mut graph, "shop/payments.py#retry/return[0]/lambda[0]").name = Some("wrapper".into());
    assert_violation(
        &graph,
        "node shop/payments.py#retry/return[0]/lambda[0]: only a callback is named, by the property it is written as",
    );
}

#[test]
fn ranges_lie_in_their_parents() {
    let mut graph = sample();
    let parent = node_mut(&mut graph, PLACE_ORDER).range;
    let node = node_mut(&mut graph, "shop/service.py#place_order/call:load_order");
    node.range.end = parent.end + 1;
    let range = node.range;
    assert_violation(
        &graph,
        &format!(
            "node shop/service.py#place_order/call:load_order: range {range} is outside its parent's range {parent}"
        ),
    );

    let mut graph = sample();
    let node = node_mut(&mut graph, "shop/service.py#place_order/return[0]");
    node.value = Some(ByteRange { start: 0, end: 1 });
    let range = node.range;
    assert_violation(
        &graph,
        &format!(
            "node shop/service.py#place_order/return[0]: value [0, 1) is outside the node's range {range}"
        ),
    );

    // A member lies in its type, and its written type in it.
    let mut graph = sample();
    let (file, id) = locate(&graph, "src/orders.ts#Order");
    let node = graph.files[file].nodes[id as usize].range;
    let member = &mut Arc::make_mut(&mut graph.files[file]).types.members[0];
    member.range.end = node.end + 1;
    let range = member.range;
    assert_violation(
        &graph,
        &format!(
            "member 0 of src/orders.ts#Order: range {range} is empty or outside its node's range {node}"
        ),
    );

    let mut graph = sample();
    let (file, _) = locate(&graph, "src/orders.ts#Order");
    let member = &mut Arc::make_mut(&mut graph.files[file]).types.members[1];
    member.range.end = member.range.start + 1;
    let ty = member.ty.as_ref().expect("a written type").range;
    assert_violation(
        &graph,
        &format!("member 1 of src/orders.ts#Order: type range {ty} is outside the member"),
    );
}

#[test]
fn ranges_are_checked_against_the_text() {
    let mut graph = sample();
    let node = node_mut(&mut graph, "shop/service.py#place_order/return[0]");
    node.range.start -= 1;
    node.stmt = Some(node.range);
    assert_violation(
        &graph,
        "node shop/service.py#place_order/return[0]: range starts with whitespace",
    );

    let mut graph = sample();
    let node = node_mut(&mut graph, "shop/service.py#place_order/return[0]");
    node.lines.end += 1;
    let lines = node.lines;
    let found = violations(&graph);
    assert!(
        found.iter().any(|message| message.starts_with(&format!(
            "node shop/service.py#place_order/return[0]: {lines:?} disagree with the text"
        ))),
        "{found:#?}"
    );
}

#[test]
fn keys_are_unique() {
    let mut graph = sample();
    node_mut(&mut graph, "shop/store.py#/call:dict").key = Key::from_text(PLACE_ORDER);
    assert_violation(
        &graph,
        "node shop/service.py#place_order: another node has this key",
    );
}

#[test]
fn keys_are_rederived_from_the_tree() {
    let mut graph = sample();
    node_mut(&mut graph, "shop/store.py#save_all").name = Some("store_all".into());
    assert_violation(
        &graph,
        "node shop/store.py#save_all: key differs from its derivation `shop/store.py#store_all`",
    );

    // A part's ordinal that disagrees with its siblings.
    let mut graph = sample();
    node_mut(&mut graph, "shop/service.py#place_order/if[0]/elif[0]").key =
        Key::from_text("shop/service.py#place_order/if[0]/elif[1]");
    assert_violation(
        &graph,
        "node shop/service.py#place_order/if[0]/elif[1]: key differs from its derivation `shop/service.py#place_order/if[0]/elif[0]`",
    );

    // Only a call that takes a callback is keyed by its first literal.
    let mut graph = sample();
    node_mut(&mut graph, "shop/store.py#/call:dict").key = Key::from_text("shop/store.py#/call:dict(\"x\")");
    assert_violation(
        &graph,
        "node shop/store.py#/call:dict(\"x\"): the key carries a first literal, but no callback is written in the call's arguments",
    );

    let mut graph = sample();
    node_mut(&mut graph, "shop/store.py#/call:dict").key = Key::from_text("shop/store.py#/call:");
    assert_violation(
        &graph,
        "node shop/store.py#/call:: `shop/store.py#/call:` is not a key: a callee is empty",
    );
}

#[test]
fn links_start_and_end_where_their_kind_allows() {
    let mut graph = sample();
    links_mut(&mut graph, "shop/payments.py#").next().unwrap().kind = LinkKind::Extends;
    assert_violation(
        &graph,
        "link 0 of shop/payments.py#: `extends` links cannot start at module nodes",
    );

    let mut graph = sample();
    let link = links_mut(&mut graph, "shop/service.py#place_order/call:load_order")
        .next()
        .unwrap();
    link.to = LinkTarget::Project(Key::from_text("shop/store.py#absent"));
    assert_violation(
        &graph,
        "link 0 of shop/service.py#place_order/call:load_order: target `shop/store.py#absent` is not a node in the graph",
    );

    let mut graph = sample();
    let link = links_mut(&mut graph, "shop/service.py#place_order/call:load_order")
        .next()
        .unwrap();
    link.to = LinkTarget::Project(Key::from_text("shop/store.py#"));
    assert_violation(
        &graph,
        "link 0 of shop/service.py#place_order/call:load_order: `calls` links cannot reach module nodes",
    );

    let mut graph = sample();
    let link = links_mut(&mut graph, "shop/store.py#save_all/call:list/call:map")
        .nth(1)
        .unwrap();
    link.to = LinkTarget::Unknown(Reason::UnknownName);
    assert_violation(
        &graph,
        "link 1 of shop/store.py#save_all/call:list/call:map: a `passes` link reaches a project definition",
    );
}

#[test]
fn links_are_ordered_and_every_call_has_one_calls_link() {
    let mut graph = sample();
    file_mut(&mut graph, SERVICE).links.swap(0, 1);
    assert_violation(
        &graph,
        "link 0 of shop/service.py#: links are not in from, then seq, order",
    );

    let mut graph = sample();
    let (file, id) = locate(&graph, "shop/service.py#place_order/call:save_order");
    Arc::make_mut(&mut graph.files[file])
        .links
        .retain(|link| link.from != id);
    assert_violation(
        &graph,
        "node shop/service.py#place_order/call:save_order: a call has exactly one `calls` link, found 0",
    );
}

#[test]
fn a_link_from_a_definition_is_not_written_in_a_nearer_one() {
    let mut graph = sample();
    let handler = "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]";
    let (file, id) = locate(&graph, handler);
    let link = Arc::make_mut(&mut graph.files[file])
        .links
        .iter_mut()
        .find(|link| link.from == id && link.kind == LinkKind::Types)
        .unwrap();
    link.from = 0;
    link.seq = u32::MAX;
    let range = link.range;
    renumber(Arc::make_mut(&mut graph.files[file]));
    assert_violation(
        &graph,
        &format!("link 1 of src/orders.ts#: range {range} lies in {handler}, a nearer definition"),
    );
}

#[test]
fn typed_records_have_their_cardinalities() {
    let mut graph = sample();
    let (file, id) = locate(&graph, "shop/payments.py#charge_card");
    Arc::make_mut(&mut graph.files[file])
        .types
        .parameters
        .retain(|parameter| !(parameter.node == id && parameter.index == 0));
    assert_violation(
        &graph,
        "parameter 1 of shop/payments.py#charge_card: indexes are contiguous from zero; expected 0",
    );

    let mut graph = sample();
    let (file, id) = locate(&graph, "shop/service.py#place_order/call:sum");
    Arc::make_mut(&mut graph.files[file])
        .types
        .signatures
        .push(Signature {
            node: id,
            returns: None,
        });
    Arc::make_mut(&mut graph.files[file]).types.sort();
    assert_violation(
        &graph,
        "signature of shop/service.py#place_order/call:sum: call nodes have no signature",
    );

    // An argument of a call that reaches a project function is paired.
    let mut graph = sample();
    let (file, id) = locate(&graph, "shop/service.py#place_order/call:load_order");
    for argument in &mut Arc::make_mut(&mut graph.files[file]).types.arguments {
        if argument.call == id {
            argument.pairing = None;
        }
    }
    assert_violation(
        &graph,
        "argument 0 of shop/service.py#place_order/call:load_order: an argument of a call that reaches a project definition needs a pairing",
    );
}

#[test]
fn app_and_entrance_keys_are_derived_and_unique() {
    let mut graph = sample();
    let copy = graph.apps[0].clone();
    graph.apps.push(copy);
    assert_violation(&graph, "app entrance:shop: another app has this key");
    assert_violation(
        &graph,
        "entrance entrance:shop/place-an-order: another entrance has this key",
    );

    let mut graph = sample();
    graph.apps[0].entrances[0].name = "Take an order".into();
    assert_violation(
        &graph,
        "entrance entrance:shop/place-an-order: key differs from its derivation `entrance:shop/take-an-order`",
    );

    let mut graph = sample();
    graph.apps[0].entrances[0].code.clear();
    assert_violation(
        &graph,
        "entrance entrance:shop/place-an-order: an entrance starts at one or more code keys",
    );
}

#[test]
fn censuses_find_every_call() {
    let graph = sample();
    let sources = sources(&shop());
    let validate = |census: &GraphCensus| {
        messages(
            &Validator::new()
                .with_sources(&sources)
                .with_census(census)
                .validate(&graph),
        )
    };

    let mut census = GraphCensus::of(&graph, Language::Python);
    let service = RepoPath::new(SERVICE).unwrap();
    census.calls.get_mut(&service).unwrap().remove(0);
    assert_eq!(
        validate(&census),
        ["node shop/service.py#/call:logging.getLogger: the call does not match exactly one call site"]
    );

    let mut census = GraphCensus::of(&graph, Language::TypeScript);
    let orders = RepoPath::new("src/orders.ts").unwrap();
    census.calls.get_mut(&orders).unwrap()[0].form = Some(Form::New);
    assert_eq!(
        validate(&census),
        ["node src/orders.ts#/call:Router: the node is a call, but the front end found a new"]
    );
}
