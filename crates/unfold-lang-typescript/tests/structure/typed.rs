//! Typed records through the real TypeScript front end: signatures, parameters, members,
//! arguments, where call values land, and type names' `types` and `extends` links, case by case,
//! and argument pairing across files.
//!
//! - Each source in `testdata/fixtures/typescript-typed/` is compiled alone, validated (with the
//!   census), and compared with the golden `.typed` file beside it: the canonical export's
//!   `signature`, `parameter`, `member`, and `argument` lines, its `types` and `extends` links,
//!   and its call nodes (which record `binds`, `exit_value`, and `awaited`), in export order.
//! - `testdata/fixtures/typed-lines/expected/typescript.jsonl` is the snapshot of the
//!   typed-lines fixture's TypeScript project, compiled by the engine from a copy of the
//!   fixture: the canonical lines of its `type` nodes, its typed records, and its `types` and
//!   `extends` links of a file under `ts/`, in export order.
//!
//! Run with `UNFOLD_BLESS=1` to rewrite the goldens after an intended change, and review them.

use std::path::Path;
use std::sync::Arc;

use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_core::typed::Pairing;
use unfold_core::vocab::{LinkKind, PairingReason, Reason};
use unfold_core::{Graph, Link, LinkTarget, canonical};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, Language, UpdateRequest};
use unfold_lang_typescript::TypeScript;

use crate::support::{Compiled, Project, compile_source, outcome};

const FOLDER: &str = "fixtures/typescript-typed";

/// The record types of typed records in the canonical export.
const TYPED: [&str; 4] = ["signature", "parameter", "member", "argument"];

/// The canonical export lines of `graph` that `keep` accepts (given each line's parsed JSON).
fn canonical_lines(graph: &Graph, keep: impl Fn(&serde_json::Value) -> bool) -> String {
    let mut bytes = Vec::new();
    canonical::write(graph, &mut bytes).expect("write the export");
    let text = String::from_utf8(bytes).expect("UTF-8 export");
    let mut out = String::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).expect("a JSON record");
        if keep(&record) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// A typed record, or a type name's link.
fn is_typed(record: &serde_json::Value) -> bool {
    let field = |name: &str| record.get(name).and_then(serde_json::Value::as_str);
    match field("t") {
        Some("link") => matches!(field("kind"), Some("types" | "extends")),
        Some(t) => TYPED.contains(&t),
        None => false,
    }
}

/// A typed record, a type name's link, or a call node.
fn is_typed_or_call(record: &serde_json::Value) -> bool {
    let field = |name: &str| record.get(name).and_then(serde_json::Value::as_str);
    is_typed(record) || (field("t") == Some("node") && field("kind") == Some("call"))
}

fn check(name: &str) {
    let path = testdata_dir().join(FOLDER).join(format!("{name}.ts"));
    let source = std::fs::read_to_string(&path).expect("read the case");
    let compiled = compile_source("a.ts", &source).valid();
    assert_golden(
        format!("{FOLDER}/{name}.typed"),
        &canonical_lines(&compiled.graph, is_typed_or_call),
    );
}

#[test]
fn signatures_and_parameters() {
    check("signatures");
}

#[test]
fn members() {
    check("members");
}

#[test]
fn results() {
    check("results");
}

#[test]
fn type_names() {
    check("type-names");
}

#[test]
fn heritage() {
    check("heritage");
}

#[test]
fn heritage_names_are_bases_and_their_type_arguments_plain_links() {
    let source =
        std::fs::read_to_string(testdata_dir().join(FOLDER).join("heritage.ts")).expect("read the case");
    let compiled = compile_source("a.ts", &source).valid();
    let links = type_links(&compiled, "a.ts");
    let bases = |node: &str| -> Vec<(&str, LinkKind, bool)> {
        let file = compiled.file("a.ts");
        links
            .iter()
            .filter(|link| file.nodes[link.from as usize].key.as_str() == format!("a.ts#{node}"))
            .map(|link| (&*link.text, link.kind, link.implements))
            .collect()
    };
    use LinkKind::{Extends, Types};
    assert_eq!(
        bases("Both"),
        [
            ("Base", Extends, false),
            ("Named", Extends, false),
            ("Missing", Extends, false)
        ],
        "the interface's own type parameter `T` is no link"
    );
    assert_eq!(
        bases("Leaf"),
        [
            ("Root", Extends, false),
            ("Named", Extends, true),
            ("Base", Extends, true),
            ("Leaf", Types, false),
            ("Leaf", Types, false),
        ],
        "the class's type parameter `L` is no link; `Leaf<L>` in a type argument is a plain link"
    );
    assert_eq!(bases("Mixed"), [], "`mixin(Root)` is a call, not a type name");
    assert_eq!(bases("Ambient"), [], "an ambient class is no node");
    assert_eq!(bases("Qualified"), [("ns.Inner", Extends, false)]);
    resolved(link(&links, "ns.Inner"), "a.ts#ns.Inner");
    external(link(&links, "Error"), "global");
    unresolved(link(&links, "Missing"), Reason::UnknownName);
    assert_eq!(
        bases("made"),
        [("Root", Extends, false)],
        "a class expression's base"
    );
}

/// The `types` and `extends` links of `path`, in canonical order.
fn type_links<'c>(compiled: &'c Compiled, path: &str) -> Vec<&'c Link> {
    compiled
        .file(path)
        .links
        .iter()
        .filter(|link| matches!(link.kind, LinkKind::Types | LinkKind::Extends))
        .collect()
}

#[track_caller]
fn link<'c>(links: &[&'c Link], text: &str) -> &'c Link {
    links
        .iter()
        .copied()
        .find(|link| &*link.text == text)
        .unwrap_or_else(|| panic!("no type link `{text}`: {links:#?}"))
}

#[track_caller]
fn resolved(link: &Link, target: &str) {
    assert_eq!(outcome(&link.to), format!("project {target}"), "{link:?}");
}

#[track_caller]
fn external(link: &Link, module: &str) {
    assert_eq!(outcome(&link.to), format!("library {module}"), "{link:?}");
}

#[track_caller]
fn unresolved(link: &Link, reason: Reason) {
    assert_eq!(link.to, LinkTarget::Unknown(reason), "{link:?}");
}

#[test]
fn type_links_resolve_through_imports_namespaces_and_the_library() {
    let compiled = Project::new()
        .input(
            "package.json",
            r#"{ "name": "app", "dependencies": { "stripe": "1", "express": "4" } }"#,
        )
        .source(
            "src/models.ts",
            "export interface Cart { id: string }\nexport type Id = string;\nexport enum Kind { A }\n\
             export class Model {}\nexport interface Merged { a: number }\nexport interface Merged { b: number }\n\
             export namespace NS { export interface Inner {} }\n",
        )
        .source(
            "src/use.ts",
            "import type { Cart, Id } from \"./models\";\nimport { Model, type Kind } from \"./models\";\n\
             import * as models from \"./models\";\nimport type { Merged } from \"./models\";\n\
             import Stripe from \"stripe\";\nimport type { Request } from \"express\";\n\
             export function use(a: Cart, b: Id, c: Model, d: Kind, e: models.Cart, f: models.NS.Inner, \
             g: Merged, h: Stripe.PaymentIntent, i: Request, j: Promise<Cart>, k: Partial<Id>, \
             l: Missing, m: NodeJS.Timeout): void {}\n",
        )
        .compile();
    let links = type_links(&compiled, "src/use.ts");
    resolved(link(&links, "Cart"), "src/models.ts#Cart");
    resolved(link(&links, "Id"), "src/models.ts#Id");
    resolved(link(&links, "Model"), "src/models.ts#Model");
    resolved(link(&links, "Kind"), "src/models.ts#Kind");
    resolved(link(&links, "models.Cart"), "src/models.ts#Cart");
    resolved(link(&links, "models.NS.Inner"), "src/models.ts#NS.Inner");
    unresolved(link(&links, "Merged"), Reason::Ambiguous);
    external(link(&links, "Stripe.PaymentIntent"), "stripe");
    external(link(&links, "Request"), "express");
    external(link(&links, "Promise"), "global");
    external(link(&links, "Partial"), "global");
    external(link(&links, "NodeJS.Timeout"), "global");
    unresolved(link(&links, "Missing"), Reason::UnknownName);
    // `Cart` inside `Promise<Cart>` is a link of its own.
    assert_eq!(
        links.iter().filter(|link| &*link.text == "Cart").count(),
        2,
        "{links:#?}"
    );
    assert_eq!(links.len(), 15, "{links:#?}");
}

#[test]
fn a_type_name_resolves_only_to_a_type_declaration() {
    let compiled = Project::new()
        .source(
            "src/a.ts",
            "export function helper() {}\nexport const value = 1;\nnamespace Space { export const x = 1; }\n\
             declare class Ambient {}\ninterface Twice {}\nclass Twice {}\n\
             export function use(a: helper, b: value, c: Space, d: Ambient, e: Twice): void {}\n",
        )
        .compile();
    let links = type_links(&compiled, "src/a.ts");
    // Values have no type meaning; a namespace only augments; an ambient class has no node;
    // an interface merged with a class names two nodes.
    unresolved(link(&links, "helper"), Reason::UnknownName);
    unresolved(link(&links, "value"), Reason::UnknownName);
    unresolved(link(&links, "Space"), Reason::UnknownName);
    unresolved(link(&links, "Ambient"), Reason::NotConcrete);
    unresolved(link(&links, "Twice"), Reason::Ambiguous);
}

/// The pairing of each argument of the call node with `key`.
#[track_caller]
fn pairings(compiled: &Compiled, path: &str, key: &str) -> Vec<Option<Pairing>> {
    let file = compiled.file(path);
    let call = file.local_id(key).unwrap_or_else(|| panic!("no node {key}"));
    file.types
        .arguments
        .iter()
        .filter(|argument| argument.call == call)
        .map(|argument| argument.pairing)
        .collect()
}

#[test]
fn arguments_pair_with_parameters_by_the_calling_rules() {
    let source = "function withThis(this: Window, a: number, b?: string) {}\n\
                  function rest(a: number, ...more: number[]) {}\n\
                  class NoConstructor {}\n\
                  class Base { constructor(x: number) {} }\n\
                  class Derived extends Base {}\n\
                  class Constructed extends Base { constructor(y: number, z = 1) { super(y); } }\n\
                  withThis(1, \"x\", true);\n\
                  rest(1, 2, 3);\n\
                  rest(...[1], 2);\n\
                  rest(1, ...[2], 3);\n\
                  new NoConstructor(1);\n\
                  new Derived(1);\n\
                  new Constructed(1);\n\
                  unknown(1);\n";
    let compiled = Project::new().source("a.ts", source).compile();
    use Pairing::{Parameter as P, Unpaired as U};
    use PairingReason as R;
    // The `this` parameter (index 0) takes no argument; a third argument has no parameter.
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:withThis"),
        [Some(P(1)), Some(P(2)), Some(U(R::NoParameter))]
    );
    // A rest parameter takes every further positional argument.
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:rest"),
        [Some(P(0)), Some(P(1)), Some(P(1))]
    );
    // A spread makes itself and every later argument `spread`.
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:rest[1]"),
        [Some(U(R::Spread)), Some(U(R::Spread))]
    );
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:rest[2]"),
        [Some(P(0)), Some(U(R::Spread)), Some(U(R::Spread))]
    );
    // A class without a constructor: nothing without a base, unknown with one.
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:NoConstructor"),
        [Some(U(R::NoParameter))]
    );
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:Derived"),
        [Some(U(R::NoSignature))]
    );
    // A direct constructor's parameters; `super(y)` pairs with the base's.
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:Constructed"),
        [Some(P(0))]
    );
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#Constructed.constructor/call:super"),
        [Some(P(0))]
    );
    // An unresolved call pairs nothing.
    assert_eq!(pairings(&compiled, "a.ts", "a.ts#/call:unknown"), [None]);
}

/// A decorator may replace a class with anything that takes arguments, so a decorated class
/// without a constructor or base isn't proven to take nothing.
#[test]
fn a_decorated_class_is_not_proven_to_take_nothing() {
    let source = "function sealed(target: unknown) {}\n\
                  @sealed\n\
                  class Sealed {}\n\
                  new Sealed(1);\n";
    let compiled = Project::new().source("a.ts", source).compile();
    use Pairing::Unpaired as U;
    use PairingReason as R;
    assert_eq!(
        pairings(&compiled, "a.ts", "a.ts#/call:Sealed"),
        [Some(U(R::NoSignature))]
    );
}

/// Copies `from` into `to`, without `.unfold/local/`.
fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create folder");
    for entry in std::fs::read_dir(from).expect("read folder") {
        let entry = entry.expect("folder entry");
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            if source.ends_with(".unfold/local") {
                continue;
            }
            copy_folder(&source, &target);
        } else {
            std::fs::copy(&source, &target).expect("copy file");
        }
    }
}

#[test]
fn the_typed_lines_fixture_matches_its_snapshot() {
    let copy = tempfile::tempdir().expect("temp dir");
    copy_folder(&testdata_dir().join("fixtures/typed-lines"), copy.path());
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        ..EngineOptions::default()
    };
    let language: Arc<dyn Language> = Arc::new(TypeScript::new());
    let mut engine = Engine::new(
        copy.path(),
        vec![language],
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("engine");
    // The engine validates the graph against the snapshots and the TypeScript census.
    let outcome = engine.update(UpdateRequest::compile()).expect("compile");
    let graph = outcome.graph.as_ref();
    assert_eq!(
        graph
            .files
            .iter()
            .filter(|file| file.file.path.as_str().starts_with("ts/"))
            .count(),
        9
    );
    let snapshot = canonical_lines(graph, |record| {
        let field = |name: &str| record.get(name).and_then(serde_json::Value::as_str);
        let declaration = field("t") == Some("node") && field("kind") == Some("type");
        let owner = field("node").or(field("call")).or(field("from")).or(field("key"));
        (declaration || is_typed(record)) && owner.is_some_and(|owner| owner.starts_with("ts/"))
    });
    assert_golden("fixtures/typed-lines/expected/typescript.jsonl", &snapshot);
}
