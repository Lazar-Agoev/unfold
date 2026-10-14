use unfold_core::key::{Callee, Child, KeyBuilder};
use unfold_core::text::{LineSpan, utf16_len};
use unfold_core::typed::{Parameter, Signature};
use unfold_core::vocab::{Flags, ParameterKind};
use unfold_core::{Key, RepoPath};

use super::*;

fn range(start: u32, end: u32) -> ByteRange {
    ByteRange::new(start, end).unwrap()
}

fn node(key: Key, parent: Option<u32>, form: Form, name: Option<&str>, at: ByteRange) -> Node {
    Node {
        key,
        parent,
        form,
        name: name.map(Into::into),
        range: at,
        lines: LineSpan { start: 1, end: 1 },
        head: None,
        body: None,
        doc: None,
        stmt: None,
        value: None,
        binds: None,
        role: None,
        r#for: None,
        flags: Flags::NONE,
        target: None,
    }
}

/// `module > [function f > call g, class C]`.
fn nodes() -> Vec<Node> {
    let mut keys = KeyBuilder::new(&RepoPath::new("a.x").unwrap());
    let g = Callee::name("g");
    let call = Child::Call {
        callee: &g,
        first_literal: None,
        takes_callback: false,
    };
    let f = keys.push(0, Child::Definition("f")).unwrap();
    let g_call = keys.push(1, call).unwrap();
    let class = keys.push(0, Child::Definition("C")).unwrap();
    vec![
        node(keys.keys()[0].clone(), None, Form::Module, None, range(0, 40)),
        node(f, Some(0), Form::Function, Some("f"), range(0, 20)),
        node(g_call, Some(1), Form::Call, Some("g"), range(10, 15)),
        node(class, Some(0), Form::Class, Some("C"), range(25, 40)),
    ]
}

fn values() -> Vec<ValueFact> {
    let mut member = ValueFact::new(
        ValueShape::Member,
        &format!("{}.x", "y".repeat(200)),
        range(12, 13),
    );
    member.receiver = Some(0);
    vec![ValueFact::new(ValueShape::Name, "g", range(10, 11)), member]
}

fn calls() -> Vec<CallFact> {
    vec![CallFact {
        node: 2,
        callee: 0,
        context: CallContext::Call,
        arguments: vec![ArgumentFact {
            range: range(12, 13),
            value: Some(1),
            expand: None,
            keyword: None,
        }],
    }]
}

fn imports() -> Vec<ImportFact> {
    vec![ImportFact {
        from: 0,
        range: range(30, 33),
        specifier: Some("pkg".into()),
        conditional: false,
    }]
}

fn type_refs() -> Vec<TypeRefFact> {
    vec![TypeRefFact::new(
        3,
        range(27, 28),
        "Base",
        false,
        TypeUse::Extends,
    )]
}

fn types() -> TypeFacts {
    let parameter = Parameter {
        node: 1,
        index: 0,
        range: range(3, 4),
        kind: ParameterKind::Positional,
        name: Some("x".into()),
        optional: false,
        default: false,
        ty: None,
    };
    TypeFacts::new(
        vec![Signature {
            node: 1,
            returns: None,
        }],
        vec![parameter],
        Vec::new(),
    )
}

fn valid() -> GraphFacts {
    GraphFacts::new(nodes(), values(), calls(), imports(), type_refs(), types()).unwrap()
}

#[test]
fn facts_cut_labels_and_round_trip_through_the_cache_encoding() {
    let facts = valid();
    assert_eq!(utf16_len(&facts.values()[1].label), 120);
    let file = FileFacts::new(facts, vec![1, 2, 3]);
    let bytes = file.to_bytes().unwrap();
    assert_eq!(FileFacts::from_bytes(&bytes).unwrap(), file);
    assert!(matches!(
        FileFacts::from_bytes(&bytes[..bytes.len() / 2]),
        Err(FactsError::Decode(_))
    ));
    assert!(ENGINE_VERSION.ends_with("+facts.6"));
}

#[test]
fn the_nodes_are_a_preorder_tree_under_the_module() {
    let new = |nodes: Vec<Node>| GraphFacts::new(nodes, values(), calls(), imports(), type_refs(), types());
    assert_eq!(new(Vec::new()), Err(FactsError::Tree { node: 0 }));
    let mut later_parent = nodes();
    later_parent[1].parent = Some(2);
    assert_eq!(new(later_parent), Err(FactsError::Tree { node: 1 }));
    let mut two_modules = nodes();
    two_modules[3].form = Form::Module;
    assert_eq!(new(two_modules), Err(FactsError::Tree { node: 3 }));
}

#[test]
fn every_call_node_has_exactly_one_call_fact() {
    let new =
        |calls: Vec<CallFact>| GraphFacts::new(nodes(), values(), calls, imports(), type_refs(), types());
    assert_eq!(new(Vec::new()), Err(FactsError::Call { node: 2 }));
    let mut twice = calls();
    twice.extend(calls());
    assert_eq!(new(twice), Err(FactsError::Call { node: 2 }));
    let mut elsewhere = calls();
    elsewhere[0].node = 1;
    assert_eq!(new(elsewhere), Err(FactsError::Call { node: 2 }));
    let mut callee = calls();
    callee[0].callee = 9;
    assert_eq!(new(callee), Err(FactsError::Value { value: 9 }));
    let mut argument = calls();
    argument[0].arguments[0].value = Some(7);
    assert_eq!(new(argument), Err(FactsError::Value { value: 7 }));
}

#[test]
fn receivers_precede_their_values() {
    let mut bad = values();
    bad[0].receiver = Some(0);
    assert_eq!(
        GraphFacts::new(nodes(), bad, calls(), imports(), type_refs(), types()),
        Err(FactsError::Value { value: 0 })
    );
}

#[test]
fn links_start_at_nodes_that_may_source_them() {
    // An import in a class body links from the class.
    let mut class_body = imports();
    class_body[0].from = 3;
    assert!(GraphFacts::new(nodes(), values(), calls(), class_body, type_refs(), types()).is_ok());
    let mut dynamic = imports();
    dynamic[0].from = 2;
    assert!(GraphFacts::new(nodes(), values(), calls(), dynamic, type_refs(), types()).is_ok());
    let mut base = type_refs();
    base[0].from = 1;
    assert_eq!(
        GraphFacts::new(nodes(), values(), calls(), imports(), base, types()),
        Err(FactsError::TypeRef { index: 0, from: 1 })
    );
    let annotation = vec![TypeRefFact::new(
        1,
        range(3, 4),
        "Cart",
        false,
        TypeUse::Annotation,
    )];
    assert!(GraphFacts::new(nodes(), values(), calls(), imports(), annotation, types()).is_ok());
}

#[test]
fn typed_records_name_nodes_of_their_kind() {
    let mut signature = types();
    signature.signatures[0].node = 3;
    assert_eq!(
        GraphFacts::new(nodes(), values(), calls(), imports(), type_refs(), signature),
        Err(FactsError::Typed {
            what: "signature",
            node: 3
        })
    );
    let mut parameter = types();
    parameter.parameters[0].node = 2;
    assert_eq!(
        GraphFacts::new(nodes(), values(), calls(), imports(), type_refs(), parameter),
        Err(FactsError::Typed {
            what: "parameter",
            node: 2
        })
    );
}

#[test]
fn file_graphs_take_the_links_arguments_and_constructions() {
    let facts = valid();
    let file = File {
        path: RepoPath::new("a.x").unwrap(),
        language: unfold_core::vocab::Language::TypeScript,
        state: unfold_core::vocab::FileState::Current,
        hash: None,
        failure: None,
    };
    let links = FileLinks {
        constructions: vec![2],
        ..FileLinks::default()
    };
    let graph = facts.file_graph(file, links);
    assert_eq!(graph.nodes[2].form, Form::New);
    assert_eq!(graph.nodes[1].form, Form::Function);
    assert_eq!(graph.types.signatures.len(), 1);
    assert_eq!(graph.types.parameters.len(), 1);
}
