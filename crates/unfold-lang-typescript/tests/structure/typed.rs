//! Typed records through the real TypeScript front end: signatures, parameters, members, call
//! results, and type links, case by case, and argument pairing across files.
//!
//! - Each source in `testdata/fixtures/typescript-typed/` is compiled alone, validated (with the
//!   census), and its typed records compared with the golden `.typed` file beside it: the
//!   canonical export's `signature`, `parameter`, `member`, `argument`, `result`, and
//!   `type_link` lines, in export order.
//! - `testdata/fixtures/typed-lines/expected/typescript.jsonl` is the snapshot of the
//!   typed-lines fixture's TypeScript project, compiled by the engine from a copy of the
//!   fixture: the canonical lines of its `interface`, `type_alias`, and `enum` blocks and of every
//!   typed record of a file under `ts/`, in export order.
//!
//! Run with `UNFOLD_BLESS=1` to rewrite the goldens after an intended change, and review them.

use std::path::Path;
use std::sync::Arc;

use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_core::typed::{Pairing, TypeLink};
use unfold_core::vocab::{PairingReason, Reason, Resolution};
use unfold_core::{Graph, canonical};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, Language, UpdateRequest};
use unfold_lang_typescript::TypeScript;

use crate::support::{Compiled, Project, compile_source};

const FOLDER: &str = "fixtures/typescript-typed";

/// The record types of typed records in the canonical export.
const TYPED: [&str; 6] = [
    "signature",
    "parameter",
    "member",
    "argument",
    "result",
    "type_link",
];

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

fn is_typed(record: &serde_json::Value) -> bool {
    record
        .get("t")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|t| TYPED.contains(&t))
}

fn check(name: &str) {
    let path = testdata_dir().join(FOLDER).join(format!("{name}.ts"));
    let source = std::fs::read_to_string(&path).expect("read the case");
    let compiled = compile_source("a.ts", &source).valid();
    assert_golden(
        format!("{FOLDER}/{name}.typed"),
        &canonical_lines(&compiled.graph, is_typed),
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

/// The type links of `path`, in canonical order.
fn links<'c>(compiled: &'c Compiled, path: &str) -> Vec<&'c TypeLink> {
    compiled.file(path).types.type_links.iter().collect()
}

#[track_caller]
fn link<'c>(links: &[&'c TypeLink], text: &str) -> &'c TypeLink {
    links
        .iter()
        .copied()
        .find(|link| &*link.text == text)
        .unwrap_or_else(|| panic!("no type link `{text}`: {links:#?}"))
}

#[track_caller]
fn resolved(link: &TypeLink, target: &str) {
    assert_eq!(
        (link.resolution, link.target.as_ref().map(|key| key.as_str())),
        (Resolution::Resolved, Some(target)),
        "{link:?}"
    );
}

#[track_caller]
fn external(link: &TypeLink, module: &str) {
    assert_eq!(
        (
            link.resolution,
            link.module.as_ref().map(|module| module.as_str())
        ),
        (Resolution::External, Some(module)),
        "{link:?}"
    );
}

#[track_caller]
fn unresolved(link: &TypeLink, reason: Reason) {
    assert_eq!(
        (link.resolution, link.reason),
        (Resolution::Unresolved, Some(reason)),
        "{link:?}"
    );
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
    let links = links(&compiled, "src/use.ts");
    resolved(link(&links, "Cart"), "src/models.ts#interface:Cart");
    resolved(link(&links, "Id"), "src/models.ts#type_alias:Id");
    resolved(link(&links, "Model"), "src/models.ts#class:Model");
    resolved(link(&links, "Kind"), "src/models.ts#enum:Kind");
    resolved(link(&links, "models.Cart"), "src/models.ts#interface:Cart");
    resolved(link(&links, "models.NS.Inner"), "src/models.ts#interface:Inner");
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
    let links = links(&compiled, "src/a.ts");
    // Values have no type meaning; a namespace only augments; an ambient class has no block;
    // an interface merged with a class names two blocks.
    unresolved(link(&links, "helper"), Reason::UnknownName);
    unresolved(link(&links, "value"), Reason::UnknownName);
    unresolved(link(&links, "Space"), Reason::UnknownName);
    unresolved(link(&links, "Ambient"), Reason::NotConcrete);
    unresolved(link(&links, "Twice"), Reason::Ambiguous);
}

/// The pairing of each argument of the call or construction whose key ends with `suffix`.
#[track_caller]
fn pairings(compiled: &Compiled, path: &str, suffix: &str) -> Vec<Option<Pairing>> {
    let file = compiled.file(path);
    let block = file
        .blocks
        .iter()
        .find(|block| block.key.as_str().ends_with(suffix))
        .unwrap_or_else(|| panic!("no block {suffix}"));
    file.types
        .arguments
        .iter()
        .filter(|argument| argument.call == block.local_id)
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
        pairings(&compiled, "a.ts", "#expression_statement[0]/call[0]"),
        [Some(P(1)), Some(P(2)), Some(U(R::NoParameter))]
    );
    // A rest parameter takes every further positional argument.
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[1]/call[0]"),
        [Some(P(0)), Some(P(1)), Some(P(1))]
    );
    // A spread makes itself and every later argument `spread`.
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[2]/call[0]"),
        [Some(U(R::Spread)), Some(U(R::Spread))]
    );
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[3]/call[0]"),
        [Some(P(0)), Some(U(R::Spread)), Some(U(R::Spread))]
    );
    // A class without a constructor: nothing without a base, unknown with one.
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[4]/new[0]"),
        [Some(U(R::NoParameter))]
    );
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[5]/new[0]"),
        [Some(U(R::NoSignature))]
    );
    // A direct constructor's parameters; `super(y)` pairs with the base's.
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[6]/new[0]"),
        [Some(P(0))]
    );
    assert_eq!(
        pairings(
            &compiled,
            "a.ts",
            "#class:Constructed/constructor:constructor/expression_statement[0]/call[0]"
        ),
        [Some(P(0))]
    );
    // An unresolved call pairs nothing.
    assert_eq!(
        pairings(&compiled, "a.ts", "#expression_statement[7]/call[0]"),
        [None]
    );
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
        pairings(&compiled, "a.ts", "#expression_statement[0]/new[0]"),
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
        7
    );
    let snapshot = canonical_lines(graph, |record| {
        let field = |name: &str| record.get(name).and_then(serde_json::Value::as_str);
        let declaration =
            field("t") == Some("block") && matches!(field("kind"), Some("interface" | "type_alias" | "enum"));
        let owner = field("block").or(field("call")).or(field("key"));
        (declaration || is_typed(record)) && owner.is_some_and(|owner| owner.starts_with("ts/"))
    });
    assert_golden("fixtures/typed-lines/expected/typescript.jsonl", &snapshot);
}
