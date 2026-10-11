//! Typed lines on hand-written graphs: labels, contracts, direct throws, type definitions and
//! uses, and type text spans.

use std::path::Path;

use serde_json::json;
use unfold_core::testkit::sample::typed;
use unfold_core::testkit::{BlockRef, GraphBuilder, Outcome};
use unfold_core::vocab::{
    BlockKind as K, EdgeKind, Expansion, Language, MemberKind, PairingReason, ParameterKind, Reason,
    Resolution, ResultBinding, Role, SlotName as S,
};
use unfold_core::{
    Argument, ByteRange, CallResult, FileTypes, Graph, Key, ModuleLabel, Pairing, Parameter, Signature,
    TypeLink, WrittenType,
};

use super::text::type_text;
use super::{
    Facts, LabelInput, LineLabel, TypeKind, TypeSpan, TypeStyle, TypeText, call_contract, line_label,
    type_definition, type_uses,
};
use crate::code::{FoldPolicy, render};
use crate::flow::{FlowNodeKind, NoContext, build_flow};
use crate::source::SourceState;
use crate::{file_owners, project};

/// Writes every file of `builder` under `dir`, so its source verifies.
fn write_sources(dir: &Path, builder: &GraphBuilder) {
    for snapshot in builder.snapshots() {
        let path = dir.join(snapshot.path().as_str());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, snapshot.bytes()).unwrap();
    }
}

fn add(
    g: &mut GraphBuilder,
    parent: BlockRef,
    kind: K,
    role: Role,
    name: Option<&str>,
    needle: &str,
) -> BlockRef {
    let range = g.find(parent, needle);
    g.block(parent, kind, Some(role), name, range)
}

/// `needle`'s first range at or after `anchor`'s.
fn after(g: &GraphBuilder, block: BlockRef, needle: &str, anchor: &str) -> ByteRange {
    let from = g.find(block, anchor).start;
    g.find_from(block, needle, from)
}

fn positional(block: BlockRef, index: u32, name: &str, ty: Option<WrittenType>) -> Parameter {
    Parameter {
        block: block.local_id(),
        index,
        kind: ParameterKind::Positional,
        name: Some(name.into()),
        optional: false,
        default: false,
        ty,
    }
}

fn argument(call: BlockRef, index: u32, pairing: Option<Pairing>) -> Argument {
    Argument {
        call: call.local_id(),
        index,
        expand: None,
        keyword: None,
        pairing,
        bound: None,
    }
}

fn result(call: BlockRef, binding: ResultBinding, target: Option<BlockRef>) -> CallResult {
    CallResult {
        call: call.local_id(),
        binding,
        awaited: false,
        target: target.map(BlockRef::local_id),
        position: None,
        bound: None,
    }
}

fn declared(start: u32, end: u32, target: &Key) -> TypeSpan {
    TypeSpan {
        start,
        end,
        style: TypeStyle::Declared,
        target: Some(target.clone()),
        module: None,
        note: None,
    }
}

fn plain(text: &str) -> TypeText {
    TypeText {
        text: text.to_owned(),
        spans: Vec::new(),
    }
}

fn typed_input(ty: TypeText) -> LabelInput {
    LabelInput {
        ty: Some(ty),
        ..LabelInput::default()
    }
}

const RISKY_TS: &str = "export class Declined {\n  constructor(code: string) {}\n}\n\nexport function risky(n: number, label?: string): Résumé<Declined> {\n  if (n) {\n    throw new Declined(\"x\");\n  }\n  try {\n    throw new Declined(\"caught\");\n  } catch (e) {\n    throw e;\n  }\n  const inner = () => {\n    throw new Declined(\"nested\");\n  };\n  throw new Missing();\n}\n\nfunction main(list: number[]) {\n  try {\n    risky(1);\n  } catch (e) {}\n  const made = new Declined(\"c\");\n  list.map(risky);\n}\n";

/// Keys of `src/risky.ts`.
struct Risky {
    builder: GraphBuilder,
    class: Key,
    constructor: Key,
    risky: Key,
    risky_call: Key,
    construction: Key,
    map_call: Key,
}

/// A TypeScript file whose `risky` throws in every way that matters, called from `main` inside a
/// `try`, constructed, and passed as a callback.
fn risky() -> Risky {
    let mut g = GraphBuilder::new();
    let f = g.file("src/risky.ts", Language::TypeScript, RISKY_TS);

    let class = add(
        &mut g,
        f,
        K::Class,
        Role::Body,
        Some("Declined"),
        "export class Declined {\n  constructor(code: string) {}\n}",
    );
    g.slot(class, S::Name, None, g.find(f, "Declined"));
    let constructor = add(
        &mut g,
        class,
        K::Constructor,
        Role::Body,
        None,
        "constructor(code: string) {}",
    );
    g.slot(constructor, S::Parameter, Some(0), g.find(f, "code: string"));
    g.slot(constructor, S::Body, None, after(&g, f, "{}", "code: string"));

    let risky_text = &RISKY_TS
        [RISKY_TS.find("export function risky").unwrap()..RISKY_TS.find("\n\nfunction main").unwrap()];
    let risky = add(&mut g, f, K::Function, Role::Body, Some("risky"), risky_text);
    g.slot(risky, S::Name, None, after(&g, f, "risky", "export function"));
    g.slot(risky, S::Parameter, Some(0), g.find(f, "n: number"));
    g.slot(risky, S::Parameter, Some(1), g.find(f, "label?: string"));
    g.slot(risky, S::ReturnType, None, g.find(f, "Résumé<Declined>"));
    let branch = add(
        &mut g,
        risky,
        K::If,
        Role::Body,
        None,
        "if (n) {\n    throw new Declined(\"x\");\n  }",
    );
    let first = add(
        &mut g,
        branch,
        K::Throw,
        Role::Then,
        None,
        "throw new Declined(\"x\");",
    );
    g.slot(first, S::Expression, None, g.find(f, "new Declined(\"x\")"));
    let first_new = add(
        &mut g,
        first,
        K::New,
        Role::Expression,
        None,
        "new Declined(\"x\")",
    );
    let attempt = add(
        &mut g,
        risky,
        K::Try,
        Role::Body,
        None,
        "try {\n    throw new Declined(\"caught\");\n  } catch (e) {\n    throw e;\n  }",
    );
    let guarded = add(
        &mut g,
        attempt,
        K::Throw,
        Role::Body,
        None,
        "throw new Declined(\"caught\");",
    );
    g.slot(
        guarded,
        S::Expression,
        None,
        g.find(f, "new Declined(\"caught\")"),
    );
    let guarded_new = add(
        &mut g,
        guarded,
        K::New,
        Role::Expression,
        None,
        "new Declined(\"caught\")",
    );
    let handler = add(
        &mut g,
        attempt,
        K::Catch,
        Role::Catch,
        None,
        "catch (e) {\n    throw e;\n  }",
    );
    let rethrow = add(&mut g, handler, K::Throw, Role::Body, None, "throw e;");
    g.slot(rethrow, S::Expression, None, after(&g, f, "e", "throw e"));
    let inner = add(
        &mut g,
        risky,
        K::Variable,
        Role::Body,
        Some("inner"),
        "const inner = () => {\n    throw new Declined(\"nested\");\n  };",
    );
    let callback = add(
        &mut g,
        inner,
        K::Function,
        Role::Initializer,
        None,
        "() => {\n    throw new Declined(\"nested\");\n  }",
    );
    let nested = add(
        &mut g,
        callback,
        K::Throw,
        Role::Body,
        None,
        "throw new Declined(\"nested\");",
    );
    g.slot(nested, S::Expression, None, g.find(f, "new Declined(\"nested\")"));
    let nested_new = add(
        &mut g,
        nested,
        K::New,
        Role::Expression,
        None,
        "new Declined(\"nested\")",
    );
    let last = add(&mut g, risky, K::Throw, Role::Body, None, "throw new Missing();");
    g.slot(last, S::Expression, None, g.find(f, "new Missing()"));
    let missing = add(&mut g, last, K::New, Role::Expression, None, "new Missing()");

    let main_text = &RISKY_TS[RISKY_TS.find("function main").unwrap()..RISKY_TS.len() - 1];
    let main = add(&mut g, f, K::Function, Role::Body, Some("main"), main_text);
    g.slot(main, S::Parameter, Some(0), g.find(f, "list: number[]"));
    let main_try = add(
        &mut g,
        main,
        K::Try,
        Role::Body,
        None,
        "try {\n    risky(1);\n  } catch (e) {}",
    );
    let statement = add(
        &mut g,
        main_try,
        K::ExpressionStatement,
        Role::Body,
        None,
        "risky(1);",
    );
    let risky_call = add(&mut g, statement, K::Call, Role::Expression, None, "risky(1)");
    g.slot(risky_call, S::Argument, Some(0), after(&g, f, "1", "risky(1)"));
    add(&mut g, main_try, K::Catch, Role::Catch, None, "catch (e) {}");
    let made = add(
        &mut g,
        main,
        K::Variable,
        Role::Body,
        Some("made"),
        "const made = new Declined(\"c\");",
    );
    let construction = add(
        &mut g,
        made,
        K::New,
        Role::Initializer,
        None,
        "new Declined(\"c\")",
    );
    g.slot(construction, S::Argument, Some(0), g.find(f, "\"c\""));
    let mapping = add(
        &mut g,
        main,
        K::ExpressionStatement,
        Role::Body,
        None,
        "list.map(risky);",
    );
    let map_call = add(
        &mut g,
        mapping,
        K::Call,
        Role::Expression,
        None,
        "list.map(risky)",
    );
    g.slot(map_call, S::Argument, Some(0), after(&g, f, "risky", "list.map("));

    let (class_key, constructor_key, risky_key) = (g.key(class), g.key(constructor), g.key(risky));
    for new in [first_new, guarded_new, nested_new] {
        g.edge(
            new,
            EdgeKind::Constructs,
            Outcome::Resolved(constructor_key.clone()),
            "Declined",
            None,
        );
    }
    g.edge(
        missing,
        EdgeKind::Constructs,
        Outcome::Unresolved(Reason::UnknownName),
        "Missing",
        None,
    );
    g.edge(
        risky_call,
        EdgeKind::Calls,
        Outcome::Resolved(risky_key.clone()),
        "risky",
        None,
    );
    g.edge(
        construction,
        EdgeKind::Constructs,
        Outcome::Resolved(constructor_key.clone()),
        "Declined",
        None,
    );
    g.edge(
        map_call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownReceiver),
        "list.map",
        None,
    );
    g.edge(
        map_call,
        EdgeKind::References,
        Outcome::Resolved(risky_key.clone()),
        "risky",
        Some(0),
    );

    let returns_at = g.find(f, "Résumé<Declined>").start;
    g.type_link(
        risky,
        "Résumé",
        returns_at,
        Outcome::Unresolved(Reason::UnknownName),
    );
    g.type_link(
        risky,
        "Declined",
        returns_at,
        Outcome::Resolved(class_key.clone()),
    );
    let parameters = [
        positional(
            constructor,
            0,
            "code",
            Some(g.written_from(f, "string", g.find(f, "code:").start)),
        ),
        positional(risky, 0, "n", Some(g.written_from(f, "number", 0))),
        Parameter {
            optional: true,
            ..positional(
                risky,
                1,
                "label",
                Some(g.written_from(f, "string", g.find(f, "label?").start)),
            )
        },
        positional(main, 0, "list", Some(g.written_from(f, "number[]", 0))),
    ];
    let returns = g.written_from(f, "Résumé<Declined>", 0);
    let types = g.types(f);
    types.signatures.extend([
        Signature {
            block: constructor.local_id(),
            returns: None,
        },
        Signature {
            block: risky.local_id(),
            returns: Some(returns),
        },
        Signature {
            block: callback.local_id(),
            returns: None,
        },
        Signature {
            block: main.local_id(),
            returns: None,
        },
    ]);
    types.parameters.extend(parameters);
    types.arguments.extend([
        argument(first_new, 0, Some(Pairing::Parameter(0))),
        argument(guarded_new, 0, Some(Pairing::Parameter(0))),
        argument(nested_new, 0, Some(Pairing::Parameter(0))),
        argument(risky_call, 0, Some(Pairing::Parameter(0))),
        argument(construction, 0, Some(Pairing::Parameter(0))),
        // `callback` is passed as a callable: a TypeScript function binds nothing.
        Argument {
            bound: Some(0),
            ..argument(map_call, 0, None)
        },
    ]);
    // Calls resolved to a function-like block carry linking's bound count (0 in TypeScript).
    let bound = |result: CallResult| CallResult {
        bound: Some(0),
        ..result
    };
    types.results.extend([
        bound(result(first_new, ResultBinding::Other, None)),
        bound(result(guarded_new, ResultBinding::Other, None)),
        bound(result(nested_new, ResultBinding::Other, None)),
        result(missing, ResultBinding::Other, None),
        bound(result(risky_call, ResultBinding::Discard, None)),
        bound(result(construction, ResultBinding::Variable, Some(made))),
        result(map_call, ResultBinding::Discard, None),
    ]);
    Risky {
        risky_call: g.key(risky_call),
        construction: g.key(construction),
        map_call: g.key(map_call),
        class: class_key,
        constructor: constructor_key,
        risky: risky_key,
        builder: g,
    }
}

/// `Résumé<Declined>`: an unresolved name with a non-ASCII letter, then a declared one.
fn resume(class: &Key) -> TypeText {
    TypeText {
        text: "Résumé<Declined>".to_owned(),
        spans: vec![
            TypeSpan {
                start: 0,
                end: 6,
                style: TypeStyle::Unresolved,
                target: None,
                module: None,
                note: Some("unknown name".to_owned()),
            },
            declared(7, 15, class),
        ],
    }
}

#[test]
fn a_call_is_labeled_with_the_types_its_arguments_fill() {
    let graph = typed().build();
    let dir = tempfile::tempdir().unwrap();
    let mut facts = Facts::new(&graph, dir.path());
    let interface = Key::from_text("src/cart.ts#interface:Cart");
    let label = line_label(&mut facts, "src/charge.ts#function:charge/return[0]/call[0]", 0)
        .unwrap()
        .expect("a resolved call has a label");
    assert_eq!(
        label,
        LineLabel {
            inputs_unknown: false,
            inputs: vec![typed_input(TypeText {
                text: "Cart".to_owned(),
                spans: vec![declared(0, 4, &interface)],
            })],
            output: None,
        }
    );
    // The keyword argument keeps its keyword; a parameter without a type shows its name.
    let settle = "jobs/settle.py#expression_statement[0]/call[0]";
    let label = line_label(&mut facts, settle, 0).unwrap().expect("label");
    assert_eq!(
        serde_json::to_value(&label).unwrap(),
        json!({"inputs": [
            {"type": {"text": "int", "spans": [{"start": 0, "end": 3, "style": "builtin", "module": "python:builtins"}]}},
            {"keyword": "currency", "name": "currency"}
        ], "inputsUnknown": false})
    );
    // Imports, unknown occurrences, and missing edges have no label.
    assert_eq!(
        line_label(&mut facts, "src/charge.ts#import[0]", 0).unwrap(),
        None
    );
    assert_eq!(line_label(&mut facts, "src/nowhere.ts#call[0]", 0).unwrap(), None);
    assert_eq!(line_label(&mut facts, settle, 3).unwrap(), None);
}

#[test]
fn constructions_and_references_are_labeled_from_the_callee() {
    let risky = risky();
    let graph = risky.builder.build();
    let dir = tempfile::tempdir().unwrap();
    let mut facts = Facts::new(&graph, dir.path());
    let label = line_label(&mut facts, risky.construction.as_str(), 0)
        .unwrap()
        .unwrap();
    assert_eq!(
        label,
        LineLabel {
            inputs_unknown: false,
            inputs: vec![typed_input(plain("string"))],
            output: Some(TypeText {
                text: "Declined".to_owned(),
                spans: vec![declared(0, 8, &risky.class)],
            }),
        }
    );
    // A passed callable shows its own signature; its return type keeps UTF-16 offsets.
    let label = line_label(&mut facts, risky.map_call.as_str(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(
        label,
        LineLabel {
            inputs_unknown: false,
            inputs: vec![typed_input(plain("number")), typed_input(plain("string"))],
            output: Some(resume(&risky.class)),
        }
    );
    // The unresolved call itself has no label, and a reference is no call.
    assert_eq!(line_label(&mut facts, risky.map_call.as_str(), 0).unwrap(), None);
    assert_eq!(
        call_contract(&mut facts, risky.map_call.as_str(), 1).unwrap(),
        None
    );
    assert_eq!(
        call_contract(&mut facts, risky.map_call.as_str(), 0).unwrap(),
        None
    );
}

#[test]
fn a_contract_lists_direct_throws_and_whether_the_call_is_caught() {
    let risky = risky();
    let graph = risky.builder.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &risky.builder);
    let mut facts = Facts::new(&graph, dir.path());
    let contract = call_contract(&mut facts, risky.risky_call.as_str(), 0)
        .unwrap()
        .expect("a resolved call has a contract");
    assert_eq!(contract.target, risky.risky);
    assert_eq!(contract.caller_line, 22);
    assert!(contract.caught, "the call sits in a try with a catch");
    // Line 7 throws a resolved construction (named after the constructor's class); line 10 is
    // guarded, line 12 rethrows a name, line 15 is a nested callable's; line 17 is unresolved.
    let throws: Vec<(u32, TypeText)> = contract
        .throws
        .iter()
        .map(|thrown| (thrown.line, thrown.ty.clone()))
        .collect();
    assert_eq!(
        throws,
        vec![
            (
                7,
                TypeText {
                    text: "Declined".to_owned(),
                    spans: vec![declared(0, 8, &risky.class)],
                }
            ),
            (
                17,
                TypeText {
                    text: "Missing".to_owned(),
                    spans: vec![TypeSpan {
                        start: 0,
                        end: 7,
                        style: TypeStyle::Unresolved,
                        target: None,
                        module: None,
                        note: Some("unknown name".to_owned()),
                    }],
                }
            ),
        ]
    );
    assert_eq!(contract.callee_lines, vec![5, 7, 17]);
    assert_eq!(contract.returns.binding, ResultBinding::Discard);
    assert_eq!(contract.returns.ty, Some(resume(&risky.class)));
    // `risky(1)` fills `n`; the optional `label` is sent nothing.
    let sends = serde_json::to_value(&contract.sends).unwrap();
    assert_eq!(
        sends,
        json!([
            {"argument": {"index": 0, "text": "1"},
             "parameter": {"index": 0, "name": "n", "kind": "positional", "optional": false, "default": false,
                           "type": {"text": "number", "spans": []}}},
            {"parameter": {"index": 1, "name": "label", "kind": "positional", "optional": true, "default": false,
                           "type": {"text": "string", "spans": []}}}
        ])
    );

    // A construction: the variable it lands in, the class it makes, no throws, not caught.
    let contract = call_contract(&mut facts, risky.construction.as_str(), 0)
        .unwrap()
        .unwrap();
    assert_eq!(contract.target, risky.constructor);
    assert!(!contract.caught);
    assert!(contract.throws.is_empty());
    assert_eq!(contract.callee_lines, vec![2]);
    assert_eq!(
        serde_json::to_value(&contract.returns).unwrap(),
        json!({"binding": "variable", "text": "made", "awaited": false,
               "type": {"text": "Declined", "spans": [{"start": 0, "end": 8, "style": "declared", "target": risky.class}]}})
    );
}

#[test]
fn a_changed_caller_or_callee_keeps_its_facts_but_drops_text_and_slot_lines() {
    let risky = risky();
    let graph = risky.builder.build();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/risky.ts"), "// edited\n").unwrap();
    let mut facts = Facts::new(&graph, dir.path());
    let contract = call_contract(&mut facts, risky.risky_call.as_str(), 0)
        .unwrap()
        .unwrap();
    let argument = contract.sends[0].argument.as_ref().unwrap();
    assert_eq!(argument.text, None);
    assert_eq!(contract.callee_lines, vec![7, 17], "only the stored throw lines");
    assert_eq!(contract.throws.len(), 2);
}

#[test]
fn the_sample_contract_reads_the_callee_and_the_python_keywords() {
    let builder = typed();
    let graph = builder.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &builder);
    let mut facts = Facts::new(&graph, dir.path());
    let contract = call_contract(&mut facts, "src/charge.ts#function:charge/return[0]/call[0]", 0)
        .unwrap()
        .unwrap();
    assert_eq!(contract.returns.binding, ResultBinding::Return);
    assert!(!contract.caught);
    assert_eq!(contract.callee_lines, vec![7, 8]);
    assert_eq!(
        serde_json::to_value(&contract.throws).unwrap(),
        json!([{"type": {"text": "Error", "spans": [{"start": 0, "end": 5, "style": "builtin", "module": "global"}]}, "line": 8}])
    );
    let contract = call_contract(&mut facts, "jobs/settle.py#expression_statement[0]/call[0]", 0)
        .unwrap()
        .unwrap();
    let sends = serde_json::to_value(&contract.sends).unwrap();
    assert_eq!(
        sends[1]["argument"],
        json!({"index": 1, "text": "currency=\"eur\"", "keyword": "currency"})
    );
    assert_eq!(sends[1]["parameter"]["kind"], "keyword_only");
    assert_eq!(sends[1]["parameter"]["default"], true);
    assert_eq!(sends.as_array().unwrap().len(), 2, "every parameter is filled");
}

const GATEWAY_PY: &str = "class Gateway:\n    def charge(self, cart: Cart, *, currency: str = \"usd\") -> Receipt:\n        pass\n\ndef run(gateway: Gateway):\n    gateway.charge(cart, currency=\"eur\")\n    Gateway.charge(gateway, cart)\n    gateway.charge(*carts)\n";

#[test]
fn python_contracts_leave_out_the_bound_receiver_and_unknown_fills() {
    let mut g = GraphBuilder::new();
    let f = g.file("shop/gateway.py", Language::Python, GATEWAY_PY);
    let class = add(
        &mut g,
        f,
        K::Class,
        Role::Body,
        Some("Gateway"),
        "class Gateway:\n    def charge(self, cart: Cart, *, currency: str = \"usd\") -> Receipt:\n        pass",
    );
    let charge_text = "def charge(self, cart: Cart, *, currency: str = \"usd\") -> Receipt:\n        pass";
    let charge = add(&mut g, class, K::Method, Role::Body, Some("charge"), charge_text);
    g.slot(charge, S::Parameter, Some(0), after(&g, f, "self", "def charge"));
    g.slot(charge, S::Parameter, Some(1), g.find(f, "cart: Cart"));
    g.slot(
        charge,
        S::Parameter,
        Some(2),
        g.find(f, "currency: str = \"usd\""),
    );
    g.slot(charge, S::ReturnType, None, g.find(f, "Receipt"));
    let run_text = &GATEWAY_PY[GATEWAY_PY.find("def run").unwrap()..GATEWAY_PY.len() - 1];
    let run = add(&mut g, f, K::Function, Role::Body, Some("run"), run_text);
    let mut calls = Vec::new();
    for text in [
        "gateway.charge(cart, currency=\"eur\")",
        "Gateway.charge(gateway, cart)",
        "gateway.charge(*carts)",
    ] {
        let statement = add(&mut g, run, K::ExpressionStatement, Role::Body, None, text);
        let call = add(&mut g, statement, K::Call, Role::Expression, None, text);
        calls.push(call);
    }
    let arguments = [
        (calls[0], 0, "cart", "gateway.charge("),
        (calls[0], 1, "currency=\"eur\"", "gateway.charge("),
        (calls[1], 0, "gateway", "Gateway.charge("),
        (calls[1], 1, "cart", "Gateway.charge("),
        (calls[2], 0, "*carts", "gateway.charge(*"),
    ];
    for (call, index, needle, anchor) in arguments {
        let range = after(&g, f, needle, anchor);
        g.slot(call, S::Argument, Some(index), range);
    }
    let charge_key = g.key(charge);
    let receipt = g.written_from(f, "Receipt", 0);
    for &call in &calls {
        g.edge(
            call,
            EdgeKind::Calls,
            Outcome::Resolved(charge_key.clone()),
            "charge",
            None,
        );
    }
    g.type_link(charge, "Cart", 0, Outcome::Unresolved(Reason::UnknownName));
    g.type_link(charge, "str", 0, Outcome::external("python:builtins"));
    g.type_link(charge, "Receipt", 0, Outcome::Unresolved(Reason::UnknownName));
    let parameters = [
        positional(charge, 0, "self", None),
        positional(charge, 1, "cart", Some(g.written_from(f, "Cart", 0))),
        Parameter {
            kind: ParameterKind::KeywordOnly,
            default: true,
            ..positional(charge, 2, "currency", Some(g.written_from(f, "str", 0)))
        },
        positional(run, 0, "gateway", Some(g.written_from(f, "Gateway", 60))),
    ];
    let types = g.types(f);
    types.signatures.extend([
        Signature {
            block: charge.local_id(),
            returns: Some(receipt),
        },
        Signature {
            block: run.local_id(),
            returns: None,
        },
    ]);
    types.parameters.extend(parameters);
    types.arguments.extend([
        argument(calls[0], 0, Some(Pairing::Parameter(1))),
        Argument {
            keyword: Some("currency".into()),
            ..argument(calls[0], 1, Some(Pairing::Parameter(2)))
        },
        argument(calls[1], 0, Some(Pairing::Parameter(0))),
        argument(calls[1], 1, Some(Pairing::Parameter(1))),
        Argument {
            expand: Some(Expansion::Spread),
            ..argument(calls[2], 0, Some(Pairing::Unpaired(PairingReason::Spread)))
        },
    ]);
    // Through an instance the call binds `self`; through the class, nothing.
    for (&call, bound) in calls.iter().zip([Some(1), Some(0), Some(0)]) {
        types.results.push(CallResult {
            bound,
            ..result(call, ResultBinding::Discard, None)
        });
    }
    let keys: Vec<Key> = calls.iter().map(|&call| g.key(call)).collect();
    let graph = g.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &g);
    let mut facts = Facts::new(&graph, dir.path());

    // Through an instance: `self` is bound, every other parameter is filled.
    let label = line_label(&mut facts, keys[0].as_str(), 0).unwrap().unwrap();
    let label = serde_json::to_value(&label).unwrap();
    assert_eq!(label["inputs"][0]["type"]["text"], "Cart");
    assert_eq!(
        label["inputs"][1],
        json!({"keyword": "currency", "type": {"text": "str", "spans": [{"start": 0, "end": 3, "style": "builtin", "module": "python:builtins"}]}})
    );
    assert_eq!(label["output"]["spans"][0]["style"], "unresolved");
    let contract = call_contract(&mut facts, keys[0].as_str(), 0).unwrap().unwrap();
    assert_eq!(contract.sends.len(), 2);
    assert_eq!(contract.callee_lines, vec![2]);

    // Through the class: `self` is sent, `currency` (keyword-only, defaulted) is not.
    let contract = call_contract(&mut facts, keys[1].as_str(), 0).unwrap().unwrap();
    let sends = serde_json::to_value(&contract.sends).unwrap();
    assert_eq!(sends.as_array().unwrap().len(), 3);
    assert_eq!(sends[0]["parameter"]["name"], "self");
    assert_eq!(sends[2]["argument"], serde_json::Value::Null);
    assert_eq!(sends[2]["parameter"]["name"], "currency");
    assert_eq!(sends[2]["parameter"]["default"], true);

    // A spread could fill anything: no unfilled parameters are claimed.
    let contract = call_contract(&mut facts, keys[2].as_str(), 0).unwrap().unwrap();
    let sends = serde_json::to_value(&contract.sends).unwrap();
    assert_eq!(
        sends,
        json!([{"argument": {"index": 0, "text": "*carts", "expand": "spread"},
                "unpaired": {"reason": "spread", "note": "spread argument"}}])
    );
    let label = line_label(&mut facts, keys[2].as_str(), 0).unwrap().unwrap();
    assert_eq!(label.inputs, vec![LabelInput::default()]);
}

#[test]
fn type_definitions_show_members_aliases_and_uses() {
    let builder = typed();
    let graph = builder.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &builder);
    let mut facts = Facts::new(&graph, dir.path());
    let alias = Key::from_text("src/cart.ts#type_alias:CartId");
    let cart = type_definition(&mut facts, "src/cart.ts#interface:Cart")
        .unwrap()
        .unwrap();
    assert_eq!(cart.kind, TypeKind::Interface);
    assert_eq!((cart.name.as_str(), cart.line, cart.uses), ("Cart", 1, 2));
    assert_eq!(cart.aliased, None);
    assert_eq!(
        serde_json::to_value(&cart.members).unwrap(),
        json!([
            {"name": "id", "kind": "property", "optional": false, "readonly": false, "default": false,
             "type": {"text": "CartId", "spans": [{"start": 0, "end": 6, "style": "declared", "target": alias}]}},
            {"name": "coupon", "kind": "property", "optional": true, "readonly": false, "default": false,
             "type": {"text": "string", "spans": []}}
        ])
    );
    let id = type_definition(&mut facts, alias.as_str()).unwrap().unwrap();
    assert_eq!(id.kind, TypeKind::TypeAlias);
    assert_eq!(id.aliased, Some(plain("string")));
    assert_eq!(id.uses, 1);
    let status = type_definition(&mut facts, "src/cart.ts#enum:Status")
        .unwrap()
        .unwrap();
    assert_eq!(status.members[0].kind, MemberKind::Value);
    assert_eq!(status.members[0].value.as_deref(), Some("\"open\""));
    assert_eq!(status.uses, 0);
    // Only type declarations and classes have definitions.
    assert_eq!(
        type_definition(&mut facts, "src/charge.ts#function:charge").unwrap(),
        None
    );
    assert_eq!(
        type_definition(&mut facts, "src/cart.ts#interface:Nope").unwrap(),
        None
    );

    // The aliased type needs the verified source.
    let changed = tempfile::tempdir().unwrap();
    let mut facts = Facts::new(&graph, changed.path());
    let id = type_definition(&mut facts, alias.as_str()).unwrap().unwrap();
    assert_eq!(id.aliased, None);
}

#[test]
fn type_uses_page_through_the_places_a_type_is_written() {
    let builder = typed();
    let graph = builder.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &builder);
    let mut facts = Facts::new(&graph, dir.path());
    let uses = type_uses(&mut facts, "src/cart.ts#interface:Cart", 0, 50)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&uses).unwrap(),
        json!({"total": 2, "places": [
            {"owner": "src/charge.ts#function:charge", "label": "charge", "path": "src/charge.ts", "line": 3},
            {"owner": "src/charge.ts#function:pay", "label": "pay", "path": "src/charge.ts", "line": 7}
        ]})
    );
    let page = type_uses(&mut facts, "src/cart.ts#interface:Cart", 1, 1)
        .unwrap()
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.places.len(), 1);
    assert_eq!(page.places[0].label, "pay");
    let alias = type_uses(&mut facts, "src/cart.ts#type_alias:CartId", 0, 50)
        .unwrap()
        .unwrap();
    assert_eq!(alias.places[0].owner.as_str(), "src/cart.ts#interface:Cart");
    assert_eq!(alias.places[0].line, 2);
    assert_eq!(
        type_uses(&mut facts, "src/charge.ts#function:pay", 0, 50).unwrap(),
        None
    );
}

const KINDS_TS: &str = "interface A { a: string }
interface B extends A { b: string }
interface C extends A { c: string }
interface D extends B, C, Remote, Gone { d: string }
interface E extends E2 { e: string }
interface E2 extends E { e2: string }
class K extends Gone implements A { k: string }
";

/// One declaration per line of `text` (`interface` or `class`, a name, an optional heritage
/// clause, and one `string` member), each base linked: resolved to a declaration of the file,
/// external `remote-lib` for `Remote`, unresolved for any other name. A class's `implements`
/// names are `implements` links.
fn declarations(path: &str, text: &str) -> (GraphBuilder, Vec<BlockRef>) {
    use unfold_core::Member;
    use unfold_core::vocab::Heritage;
    let mut g = GraphBuilder::new();
    let file = g.file(path, Language::TypeScript, text);
    let mut blocks = Vec::new();
    let mut names = Vec::new();
    let mut start = 0;
    for line in text.lines() {
        let range = g.find_from(file, line, start);
        start = range.end;
        let class = line.starts_with("class ");
        let name = line.split_whitespace().nth(1).unwrap();
        let kind = if class { K::Class } else { K::Interface };
        let block = g.block(file, kind, Some(Role::Body), Some(name), range);
        g.slot(block, S::Name, None, g.find_from(file, name, range.start));
        let body = g.find_from(file, "{", range.start).start;
        let heritage = line
            .find("extends ")
            .map(|at| range.start + u32::try_from(at).unwrap())
            .map(|at| {
                let end = line.find(" implements").or_else(|| line.find(" {")).unwrap();
                ByteRange::new(at, range.start + u32::try_from(end).unwrap()).unwrap()
            });
        if let Some(heritage) = heritage {
            g.slot(block, S::Heritage, None, heritage);
        }
        if !class {
            g.slot(block, S::Body, None, ByteRange::new(body, range.end).unwrap());
        }
        let member = line[line.find("{ ").unwrap() + 2..line.find(':').unwrap()].to_owned();
        let ty = g.written_from(file, "string", body);
        g.types(file).members.push(Member {
            block: block.local_id(),
            index: 0,
            kind: MemberKind::Property,
            name: Some(member.into()),
            optional: false,
            readonly: false,
            default: false,
            ty: Some(ty),
            value: None,
        });
        blocks.push(block);
        names.push(name.to_owned());
    }
    for (block, line) in blocks.iter().zip(text.lines()) {
        let from = g.range(*block).start;
        let head = &line[..line.find(" {").unwrap()];
        let clauses = [
            ("extends ", Heritage::Extends),
            ("implements ", Heritage::Implements),
        ];
        for (word, heritage) in clauses {
            let Some(at) = head.find(word) else {
                continue;
            };
            let list = head[at + word.len()..].split(" implements").next().unwrap();
            for base in list.split(", ") {
                let outcome = match names.iter().position(|name| name == base) {
                    Some(index) => Outcome::Resolved(g.key(blocks[index])),
                    None if base == "Remote" => Outcome::external("remote-lib"),
                    None => Outcome::Unresolved(Reason::UnknownName),
                };
                let at = from + u32::try_from(at).unwrap();
                g.heritage_link(*block, base, at, outcome, heritage);
            }
        }
    }
    (g, blocks)
}

#[test]
fn definitions_list_inherited_members_depth_first_and_each_type_once() {
    let (builder, blocks) = declarations("src/kinds.ts", KINDS_TS);
    let graph = builder.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &builder);
    let mut facts = Facts::new(&graph, dir.path());
    let groups = |facts: &mut Facts<'_>, name: &str| -> Vec<(String, Vec<String>)> {
        let kind = if name == "K" { "class" } else { "interface" };
        type_definition(facts, &format!("src/kinds.ts#{kind}:{name}"))
            .unwrap()
            .unwrap()
            .inherited
            .into_iter()
            .map(|group| {
                let members = group
                    .members
                    .into_iter()
                    .filter_map(|member| member.name)
                    .collect();
                (group.from.text, members)
            })
            .collect()
    };
    let named = |groups: &[(&str, &[&str])]| -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .map(|(from, members)| {
                (
                    (*from).to_owned(),
                    members.iter().map(|&member| member.to_owned()).collect(),
                )
            })
            .collect()
    };
    // B, then B's base A, then C (whose A is listed already), then the bases with no
    // declaration, which have no members.
    assert_eq!(
        groups(&mut facts, "D"),
        named(&[
            ("B", &["b"]),
            ("A", &["a"]),
            ("C", &["c"]),
            ("Remote", &[]),
            ("Gone", &[])
        ])
    );
    // A cycle stops at the definition itself.
    assert_eq!(groups(&mut facts, "E"), named(&[("E2", &["e2"])]));
    // A class inherits from what it extends, not from what it implements.
    assert_eq!(groups(&mut facts, "K"), named(&[("Gone", &[])]));
    assert_eq!(groups(&mut facts, "A"), named(&[]));

    // Each group names its base with one span.
    let d = type_definition(&mut facts, "src/kinds.ts#interface:D")
        .unwrap()
        .unwrap();
    let froms: Vec<serde_json::Value> = d
        .inherited
        .iter()
        .map(|group| serde_json::to_value(&group.from).unwrap())
        .collect();
    assert_eq!(
        froms[0],
        json!({"text": "B", "spans": [{"start": 0, "end": 1, "style": "declared", "target": graph.files[0].blocks[blocks[1].local_id() as usize].key}]})
    );
    assert_eq!(
        froms[3],
        json!({"text": "Remote", "spans": [{"start": 0, "end": 6, "style": "external", "module": "remote-lib", "note": "remote-lib"}]})
    );
    assert_eq!(
        froms[4],
        json!({"text": "Gone", "spans": [{"start": 0, "end": 4, "style": "unresolved", "note": "unknown name"}]})
    );

    // Heritage places count as uses and say so.
    let uses = type_uses(&mut facts, "src/kinds.ts#interface:A", 0, 50)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&uses).unwrap(),
        json!({"total": 3, "places": [
            {"owner": "src/kinds.ts#interface:B", "label": "B", "path": "src/kinds.ts", "line": 2, "heritage": "extends"},
            {"owner": "src/kinds.ts#interface:C", "label": "C", "path": "src/kinds.ts", "line": 3, "heritage": "extends"},
            {"owner": "src/kinds.ts#class:K", "label": "K", "path": "src/kinds.ts", "line": 7, "heritage": "implements"}
        ]})
    );
    assert_eq!(
        type_definition(&mut facts, "src/kinds.ts#interface:A")
            .unwrap()
            .unwrap()
            .uses,
        3
    );
}

#[test]
fn inherited_members_reach_a_bounded_depth() {
    use super::MAX_INHERITED_DEPTH;
    let levels = MAX_INHERITED_DEPTH + 2;
    let text: String = (0..=levels)
        .map(|level| match level {
            level if level == levels => format!("interface T{level} {{ m{level}: string }}\n"),
            level => format!(
                "interface T{level} extends T{} {{ m{level}: string }}\n",
                level + 1
            ),
        })
        .collect();
    let (builder, _) = declarations("src/chain.ts", &text);
    let graph = builder.build();
    let dir = tempfile::tempdir().unwrap();
    let mut facts = Facts::new(&graph, dir.path());
    let chain = type_definition(&mut facts, "src/chain.ts#interface:T0")
        .unwrap()
        .unwrap();
    let froms: Vec<String> = chain.inherited.into_iter().map(|group| group.from.text).collect();
    let expected: Vec<String> = (1..=MAX_INHERITED_DEPTH)
        .map(|level| format!("T{level}"))
        .collect();
    assert_eq!(froms, expected);
}

#[test]
fn type_text_spans_are_utf16_and_drop_names_the_cut_text_lost() {
    let link = |start: u32, end: u32, resolution: Resolution| TypeLink {
        block: 0,
        range: ByteRange::new(start, end).unwrap(),
        text: "x".into(),
        resolution,
        target: (resolution == Resolution::Resolved).then(|| Key::from_text("a.ts#interface:Ä")),
        reason: (resolution == Resolution::Unresolved).then_some(Reason::Ambiguous),
        module: (resolution == Resolution::External).then(|| ModuleLabel::from_set(["b", "a"]).unwrap()),
        heritage: None,
    };
    // `Map<Ä, 😀Stripe.Intent>` written at byte 100: `Ä` is two bytes and one unit, `😀` four
    // bytes and two units.
    let text = "Map<Ä, 😀Stripe.Intent>";
    let at = |needle: &str| 100 + u32::try_from(text.find(needle).unwrap()).unwrap();
    let links = [
        link(at("Map"), at("Map") + 3, Resolution::External),
        link(at("Ä"), at("Ä") + 2, Resolution::Resolved),
        link(at("Stripe"), at(">"), Resolution::Unresolved),
    ];
    let written = WrittenType {
        text: text.into(),
        range: ByteRange::new(100, 100 + u32::try_from(text.len()).unwrap()).unwrap(),
    };
    let shown = type_text(&written, links.iter());
    let spans: Vec<(u32, u32, TypeStyle)> = shown
        .spans
        .iter()
        .map(|span| (span.start, span.end, span.style))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0, 3, TypeStyle::External),
            (4, 5, TypeStyle::Declared),
            (9, 22, TypeStyle::Unresolved)
        ]
    );
    assert_eq!(shown.spans[0].note.as_deref(), Some("a or b"));
    assert_eq!(shown.spans[2].note.as_deref(), Some("ambiguous target"));
    // Cut after `😀St`: the last name no longer fits whole and is dropped.
    let cut = WrittenType {
        text: "Map<Ä, 😀St".into(),
        range: written.range,
    };
    assert_eq!(type_text(&cut, links.iter()).spans.len(), 2);
    // A link outside the written range, or one splitting a character, is dropped too.
    let outside = [
        link(90, 95, Resolution::External),
        link(at("Ä") + 1, at("Ä") + 2, Resolution::External),
    ];
    assert!(type_text(&written, outside.iter()).spans.is_empty());
}

const LOCAL_TS: &str = "export function local() {\n  interface Shape { size: number }\n  type Size = number;\n  return go();\n}\nexport enum Mode { A = make() }\n";

#[test]
fn type_declarations_are_passed_through_catalogued_and_outlined() {
    let mut g = GraphBuilder::new();
    let f = g.file("src/local.ts", Language::TypeScript, LOCAL_TS);
    let local = add(
        &mut g,
        f,
        K::Function,
        Role::Body,
        Some("local"),
        &LOCAL_TS[..LOCAL_TS.find("\nexport enum").unwrap()],
    );
    g.slot(local, S::Body, None, after(&g, f, "{", "local()"));
    let shape = add(
        &mut g,
        local,
        K::Interface,
        Role::Body,
        Some("Shape"),
        "interface Shape { size: number }",
    );
    g.slot(shape, S::Body, None, g.find(f, "{ size: number }"));
    add(
        &mut g,
        local,
        K::TypeAlias,
        Role::Body,
        Some("Size"),
        "type Size = number;",
    );
    let ret = add(&mut g, local, K::Return, Role::Body, None, "return go();");
    g.slot(ret, S::Expression, None, g.find(f, "go()"));
    let go = add(&mut g, ret, K::Call, Role::Expression, None, "go()");
    let mode = add(
        &mut g,
        f,
        K::Enum,
        Role::Body,
        Some("Mode"),
        "export enum Mode { A = make() }",
    );
    let make = add(&mut g, mode, K::Call, Role::Body, None, "make()");
    g.edge(
        go,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "go",
        None,
    );
    g.edge(
        make,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "make",
        None,
    );
    let graph = g.build();
    let file = &graph.files[0];

    // The owner walk passes through the enum: its call belongs to the file.
    let owners = file_owners(&file.blocks);
    assert_eq!(owners[make.local_id() as usize], 0);
    assert_eq!(owners[go.local_id() as usize], local.local_id());
    assert_eq!(owners[shape.local_id() as usize], local.local_id());
    // In the catalog; a declaration inside a function is named after it.
    let projections = project(&graph);
    let names: Vec<(&str, &str)> = projections.definitions[0]
        .iter()
        .map(|definition| (&*definition.label, &*definition.qualified_name))
        .collect();
    assert_eq!(
        names,
        [
            ("local", "local"),
            ("Shape", "local.Shape"),
            ("Size", "local.Size"),
            ("Mode", "Mode")
        ]
    );
    // Flow draws only the return; the outline lists the declarations too, never as group
    // members, and folds a multi-line one at its body.
    let flow = build_flow(
        file,
        local.local_id(),
        Some(LOCAL_TS),
        SourceState::Current,
        &NoContext,
    )
    .unwrap();
    let kinds: Vec<FlowNodeKind> = flow.nodes.iter().map(|node| node.kind).collect();
    assert_eq!(
        kinds,
        [FlowNodeKind::Start, FlowNodeKind::Return, FlowNodeKind::End]
    );
    let code = render(file, local.local_id(), LOCAL_TS, None, FoldPolicy::Card).unwrap();
    let outline: Vec<(K, bool)> = code
        .outline
        .iter()
        .map(|item| (item.kind, item.groupable))
        .collect();
    assert_eq!(
        outline,
        [(K::Interface, false), (K::TypeAlias, false), (K::Return, true)]
    );
    // Every view still renders the declarations themselves without failing.
    for block in [shape, mode] {
        build_flow(
            file,
            block.local_id(),
            Some(LOCAL_TS),
            SourceState::Current,
            &NoContext,
        )
        .unwrap();
        render(file, block.local_id(), LOCAL_TS, None, FoldPolicy::Card).unwrap();
    }
}

#[test]
fn a_graph_without_typed_records_reads_as_unknown() {
    // A version 1 graph (no typed records): labels exist but say nothing.
    let mut graph: Graph = typed().build();
    for file in &mut graph.files {
        file.types = FileTypes::default();
    }
    let dir = tempfile::tempdir().unwrap();
    let mut facts = Facts::new(&graph, dir.path());
    let label = line_label(&mut facts, "src/charge.ts#function:charge/return[0]/call[0]", 0)
        .unwrap()
        .unwrap();
    assert_eq!(
        label,
        LineLabel {
            inputs_unknown: false,
            inputs: Vec::new(),
            output: None
        }
    );
    let contract = call_contract(&mut facts, "src/charge.ts#function:charge/return[0]/call[0]", 0)
        .unwrap()
        .unwrap();
    assert_eq!(contract.returns.binding, ResultBinding::Other);
    let cart = type_definition(&mut facts, "src/cart.ts#interface:Cart")
        .unwrap()
        .unwrap();
    assert!(cart.members.is_empty());
    assert_eq!(cart.uses, 0);
}

/// A contract lists unfilled parameters only after the ones the call binds, and only when
/// linking proved how many it binds; a passed callable's label drops what it binds, or says
/// its inputs are unknown when that isn't proven.
#[test]
fn bound_counts_shape_contracts_and_passed_callable_labels() {
    let mut g = GraphBuilder::new();
    let text = "class G:\n    def ping(self, timeout):\n        return timeout\n\ng.ping()\nrun(g.ping)\n";
    let f = g.file("app.py", Language::Python, text);
    let class = g.block(
        f,
        K::Class,
        Some(Role::Body),
        Some("G"),
        g.find(
            f,
            "class G:\n    def ping(self, timeout):\n        return timeout",
        ),
    );
    g.slot(class, S::Name, None, g.find(f, "G"));
    let ping_text = "def ping(self, timeout):\n        return timeout";
    let ping = g.block(
        class,
        K::Method,
        Some(Role::Body),
        Some("ping"),
        g.find(f, ping_text),
    );
    g.slot(ping, S::Name, None, g.find(f, "ping"));
    g.slot(ping, S::Parameter, Some(0), g.find(f, "self"));
    g.slot(ping, S::Parameter, Some(1), g.find(f, "timeout"));
    g.slot(ping, S::Body, None, g.find(f, ":\n        return timeout"));
    let returned = g.block(
        ping,
        K::Return,
        Some(Role::Body),
        None,
        g.find(f, "return timeout"),
    );
    g.slot(
        returned,
        S::Expression,
        None,
        g.find_from(f, "timeout", g.find(f, "return").start),
    );
    let statement = g.block(
        f,
        K::ExpressionStatement,
        Some(Role::Body),
        None,
        g.find(f, "g.ping()"),
    );
    g.slot(statement, S::Expression, None, g.find(f, "g.ping()"));
    let call = g.block(
        statement,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(f, "g.ping()"),
    );
    g.slot(call, S::Callee, None, g.find(f, "g.ping"));
    let passing = g.block(
        f,
        K::ExpressionStatement,
        Some(Role::Body),
        None,
        g.find(f, "run(g.ping)"),
    );
    g.slot(passing, S::Expression, None, g.find(f, "run(g.ping)"));
    let run = g.block(
        passing,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(f, "run(g.ping)"),
    );
    g.slot(run, S::Callee, None, g.find(f, "run"));
    g.slot(
        run,
        S::Argument,
        Some(0),
        g.find_from(f, "g.ping", g.find(f, "run(").start),
    );
    g.edge(
        call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(ping)),
        "g.ping",
        None,
    );
    g.edge(
        run,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "run",
        None,
    );
    g.edge(
        run,
        EdgeKind::References,
        Outcome::Resolved(g.key(ping)),
        "g.ping",
        Some(0),
    );
    let parameters = vec![
        positional(ping, 0, "self", None),
        positional(ping, 1, "timeout", None),
    ];
    let types = g.types(f);
    types.signatures.push(Signature {
        block: ping.local_id(),
        returns: None,
    });
    types.parameters.extend(parameters);
    types.arguments.push(Argument {
        bound: Some(1),
        ..argument(run, 0, None)
    });
    types.results.extend([
        CallResult {
            bound: Some(1),
            ..result(call, ResultBinding::Discard, None)
        },
        result(run, ResultBinding::Discard, None),
    ]);
    let (call_key, run_key) = (g.key(call), g.key(run));
    let mut graph = g.build();
    let dir = tempfile::tempdir().unwrap();
    write_sources(dir.path(), &g);

    let mut facts = Facts::new(&graph, dir.path());
    let names = |contract: &super::CallContract| -> Vec<Option<String>> {
        contract
            .sends
            .iter()
            .map(|send| {
                send.parameter
                    .as_ref()
                    .and_then(|parameter| parameter.name.clone())
            })
            .collect()
    };
    // `g.ping()` binds `self`: only `timeout` is unfilled.
    let contract = call_contract(&mut facts, call_key.as_str(), 0).unwrap().unwrap();
    assert_eq!(names(&contract), [Some("timeout".to_owned())]);
    // `run(g.ping)`: the passed method's label drops `self`.
    let label = line_label(&mut facts, run_key.as_str(), 1).unwrap().unwrap();
    assert!(!label.inputs_unknown);
    assert_eq!(label.inputs.len(), 1);
    assert_eq!(label.inputs[0].name.as_deref(), Some("timeout"));

    // Without proven counts: no unfilled parameter is claimed, and the label's inputs are unknown.
    for file in &mut graph.files {
        for result in &mut file.types.results {
            result.bound = None;
        }
        for argument in &mut file.types.arguments {
            argument.bound = None;
        }
    }
    let mut facts = Facts::new(&graph, dir.path());
    let contract = call_contract(&mut facts, call_key.as_str(), 0).unwrap().unwrap();
    assert!(names(&contract).is_empty());
    let label = line_label(&mut facts, run_key.as_str(), 1).unwrap().unwrap();
    assert!(label.inputs_unknown);
    assert!(label.inputs.is_empty());
}
