//! Failures, extraction options, the value table and call facts, the payload, the linking, and
//! the census.

use unfold_core::validate::Census;
use unfold_core::vocab::{Expansion, Form};
use unfold_core::{ByteRange, RepoPath, Snapshot};
use unfold_engine::facts::{CallContext, ValueShape};
use unfold_engine::settings::Settings;
use unfold_engine::{FileFacts, Language, LanguageId};
use unfold_lang_typescript::census::TypeScriptCensus;
use unfold_lang_typescript::payload::{ImportKind, Payload};
use unfold_lang_typescript::{ExtractOptions, PRODUCER_NAME, TypeScript, extract};

use crate::support::{compile, compile_source, link_line};

fn facts(source: &str) -> FileFacts {
    facts_with(source, ExtractOptions::default())
}

fn facts_with(source: &str, options: ExtractOptions) -> FileFacts {
    let snapshot = Snapshot::from_text(RepoPath::new("a.ts").expect("path"), source).expect("text");
    extract(&snapshot, options).expect("extracts")
}

#[test]
fn syntax_errors_fail_the_file_with_a_message_and_offset() {
    let snapshot =
        Snapshot::from_text(RepoPath::new("a.ts").expect("path"), "let x = ;\nf();").expect("text");
    let failure = extract(&snapshot, ExtractOptions::default()).expect_err("a syntax error");
    assert_eq!(failure.offset, Some(8), "{failure}");
    assert!(!failure.message.is_empty());
    // A recoverable error still fails the file: the recovered tree is never used.
    let recoverable = Snapshot::from_text(
        RepoPath::new("b.ts").expect("path"),
        "class C { constructor(this: X) {} }",
    )
    .expect("text");
    assert!(extract(&recoverable, ExtractOptions::default()).is_err());
}

#[test]
fn declaration_files_are_never_extracted_or_owned() {
    let language = TypeScript::new();
    let settings = Settings::default();
    for path in ["types.d.ts", "src/a.d.mts", "a.d.cts", "a.js"] {
        assert!(
            !language.owns(&RepoPath::new(path).expect("path"), &settings),
            "{path}"
        );
    }
    for path in ["a.ts", "a.tsx", "a.mts", "a.cts"] {
        assert!(
            language.owns(&RepoPath::new(path).expect("path"), &settings),
            "{path}"
        );
    }
    let snapshot = Snapshot::from_text(
        RepoPath::new("types.d.ts").expect("path"),
        "declare const x: number;",
    )
    .expect("text");
    assert!(extract(&snapshot, ExtractOptions::default()).is_err());
    assert_eq!(language.id(), LanguageId::TypeScript);
    assert_eq!(language.producer().name, PRODUCER_NAME);
}

#[test]
fn verbatim_module_syntax_keeps_type_only_and_empty_named_imports() {
    let source = "import { type A } from './a.js';\nimport {} from './b.js';\nexport { type C } from './c.js';\nimport type D from './d.js';\n";
    assert!(facts(source).graph.imports().is_empty());
    let options = ExtractOptions {
        verbatim_module_syntax: true,
        ..ExtractOptions::default()
    };
    let kept: Vec<_> = facts_with(source, options)
        .graph
        .imports()
        .iter()
        .map(|import| import.specifier.as_deref().map(str::to_owned))
        .collect();
    // A declaration-level `import type` is erased either way.
    assert_eq!(
        kept,
        [
            Some("./a.js".to_owned()),
            Some("./b.js".to_owned()),
            Some("./c.js".to_owned())
        ]
    );
    let settings = Settings::default();
    let path = RepoPath::new("a.ts").expect("path");
    assert_ne!(
        TypeScript::new().settings_hash(&path, &settings),
        TypeScript::with_options(options).settings_hash(&path, &settings)
    );
}

#[test]
fn values_record_shapes_labels_receivers_and_passed_names() {
    let facts =
        facts("api.client.get(handler, this.run, (x) => x, ...rest, items[\"key\"], items[key], make());\n");
    let graph = &facts.graph;
    // The outer call, then `make()`.
    assert_eq!(graph.calls().len(), 2);
    let call = &graph.calls()[0];
    let value = |index: u32| &graph.values()[index as usize];
    let callee = value(call.callee);
    assert_eq!(
        (callee.shape, &*callee.label, callee.member.as_deref()),
        (ValueShape::Member, "api.client.get", Some("get"))
    );
    let receiver = value(callee.receiver.expect("a receiver"));
    assert_eq!(
        (&*receiver.label, receiver.member.as_deref()),
        ("api.client", Some("client"))
    );
    assert_eq!(value(receiver.receiver.expect("a root")).shape, ValueShape::Name);
    let source =
        "api.client.get(handler, this.run, (x) => x, ...rest, items[\"key\"], items[key], make());\n";
    let arguments: Vec<_> = call
        .arguments
        .iter()
        .map(|argument| {
            let written = &source[argument.range.start as usize..argument.range.end as usize];
            let passed = argument
                .value
                .map(|index| (value(index).shape, value(index).label.to_string()));
            (written, passed, argument.expand)
        })
        .collect();
    // Only a plain name or member access that isn't spread may pass a definition.
    assert_eq!(
        arguments,
        [
            ("handler", Some((ValueShape::Name, "handler".to_owned())), None),
            (
                "this.run",
                Some((ValueShape::Member, "this.run".to_owned())),
                None
            ),
            ("(x) => x", None, None),
            ("...rest", None, Some(Expansion::Spread)),
            (
                "items[\"key\"]",
                Some((ValueShape::Member, "items[\"key\"]".to_owned())),
                None
            ),
            ("items[key]", None, None),
            ("make()", None, None),
        ]
    );
    // A value's range is where its link is written.
    assert_eq!(
        &source[callee.range.start as usize..callee.range.end as usize],
        "api.client.get"
    );
    // Every receiver precedes the access that uses it.
    for (index, value) in graph.values().iter().enumerate() {
        assert!(value.receiver.is_none_or(|receiver| (receiver as usize) < index));
    }
}

#[test]
fn constructions_super_calls_tagged_templates_and_dynamic_imports() {
    let facts = facts(
        "class A extends B { constructor() { super(1); } }\nnew A(2);\ntag`x${y}`;\nimport(\"./m.js\");\n",
    );
    let graph = &facts.graph;
    let calls: Vec<_> = graph
        .calls()
        .iter()
        .map(|call| {
            let node = &graph.nodes()[call.node as usize];
            let label = graph.values()[call.callee as usize].label.to_string();
            (node.form, label, call.context, call.arguments.len())
        })
        .collect();
    assert_eq!(
        calls,
        [
            (Form::Call, "super".to_owned(), CallContext::Construct, 1),
            (Form::New, "A".to_owned(), CallContext::Construct, 1),
            (Form::Call, "tag".to_owned(), CallContext::Call, 0),
            // A dynamic import's callee is its `import` keyword, which names nothing.
            (Form::Call, "import".to_owned(), CallContext::Call, 1),
        ]
    );
    let import = &graph.imports()[0];
    assert_eq!(import.from, graph.calls()[3].node);
    assert_eq!(import.specifier.as_deref(), Some("./m.js"));
}

#[test]
fn the_payload_aligns_with_values_and_imports() {
    let facts = facts(
        "import a from './a.js';\nexport * from './b.js';\nimport c = require('./c.js');\nconst d = import(name);\nobj.method().next(this, super_);\n",
    );
    let payload = Payload::from_bytes(&facts.payload).expect("decodes");
    assert_eq!(payload.binding.value_exprs.len(), facts.graph.values().len());
    assert_eq!(
        payload.imports,
        [
            ImportKind::Import,
            ImportKind::Reexport,
            ImportKind::ImportEquals,
            ImportKind::Dynamic
        ]
    );
    let imports = facts.graph.imports();
    assert_eq!(imports[3].specifier, None);
    // Static imports are written from the module; a dynamic one from its call.
    assert!(imports[..3].iter().all(|import| import.from == 0));
    assert_ne!(imports[3].from, 0);
    assert_eq!(
        Payload::from_bytes(&payload.to_bytes().expect("encodes")).expect("decodes"),
        payload
    );
}

#[test]
fn linking_resolves_imports_calls_and_passed_definitions() {
    let source = "import x from './b.js';\nconst y = import(name);\nfunction f(g: () => void) { g(); }\nf(f);\nnew Date();\n";
    let compiled = compile_source("a.ts", source).valid();
    let file = compiled.file("a.ts");
    let links: Vec<String> = file
        .links
        .iter()
        .map(|link| format!("{} {}", file.nodes[link.from as usize].key, link_line(link)))
        .collect();
    assert_eq!(
        links,
        [
            "a.ts# imports `./b.js` → unknown not_found",
            "a.ts#/call:import calls `import` → unknown dynamic",
            "a.ts#/call:import imports `(…)` → unknown dynamic",
            "a.ts#f/call:g calls `g` → unknown parameter",
            "a.ts#/call:f calls `f` → project a.ts#f",
            "a.ts#/call:f passes `f` → project a.ts#f",
            "a.ts#/call:Date calls `Date` → builtin ecmascript",
        ]
    );
}

#[test]
fn the_census_checks_token_boundaries_from_the_payload() {
    let source = "const total = sum(a, b); // done\n";
    let payload = Payload::from_bytes(&facts(source).payload).expect("decodes");
    let path = RepoPath::new("a.ts").expect("path");
    let census = TypeScriptCensus::new([(path.clone(), payload.census)]);
    let range = |start: u32, end: u32| ByteRange { start, end };
    assert!(census.is_token_range(&path, range(6, 23)), "total = sum(a, b)");
    assert!(census.is_token_range(&path, range(14, 23)), "sum(a, b)");
    assert!(
        !census.is_token_range(&path, range(5, 23)),
        "starts at whitespace"
    );
    assert!(!census.is_token_range(&path, range(14, 21)), "ends at whitespace");
    assert!(!census.is_token_range(&path, range(25, 32)), "inside a comment");
    assert!(!census.is_token_range(&RepoPath::new("b.ts").expect("path"), range(6, 23)));
    let sites = census.call_sites(&path).expect("sites");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].range, range(14, 23));
    assert_eq!(sites[0].form, Some(Form::Call));
}

#[test]
fn the_census_lists_every_call_and_dependency() {
    let source = "import a from './a.js';\n@deco class C { @Field() x = 1; }\nnew C(import('./b.js'), tag`t`);\ntype T = ReturnType<typeof f>;\n";
    let payload = Payload::from_bytes(&facts(source).payload).expect("decodes");
    let path = RepoPath::new("a.ts").expect("path");
    let census = TypeScriptCensus::new([(path.clone(), payload.census)]);
    let text = |range: ByteRange| &source[range.start as usize..range.end as usize];
    let sites: Vec<(&str, Option<Form>)> = census
        .call_sites(&path)
        .expect("sites")
        .iter()
        .map(|site| (text(site.range), site.form))
        .collect();
    // A decorator that isn't a call is a call site at its expression; type syntax has none.
    assert_eq!(
        sites,
        [
            ("deco", Some(Form::Call)),
            ("Field()", Some(Form::Call)),
            ("new C(import('./b.js'), tag`t`)", Some(Form::New)),
            ("import('./b.js')", Some(Form::Call)),
            ("tag`t`", Some(Form::Call)),
        ]
    );
    let dependencies: Vec<&str> = census
        .module_dependencies(&path)
        .expect("dependencies")
        .iter()
        .map(|&range| text(range))
        .collect();
    assert_eq!(dependencies, ["'./a.js'", "'./b.js'"]);
}

#[test]
fn a_failed_file_does_not_stop_the_others() {
    let files = [
        ("bad.ts".to_owned(), b"function (".to_vec()),
        ("good.ts".to_owned(), b"export const ok = () => run();".to_vec()),
    ];
    let compiled = compile(&TypeScript::new(), &files);
    assert_eq!(compiled.failures.len(), 1);
    assert!(compiled.violations.is_empty(), "{:?}", compiled.violations);
    assert!(compiled.file("good.ts").nodes.len() > 1);
    assert!(compiled.file("bad.ts").nodes.is_empty());
}

#[test]
fn byte_order_marks_line_endings_and_unicode_keep_exact_ranges() {
    let source = "\u{feff}#!/usr/bin/env node\r\nexport const café = () => grüße(\"😀\");\u{2028}// note\r\nfunction ƒ() {}\r\n";
    let compiled = compile_source("a.ts", source).valid();
    let file = compiled.file("a.ts");
    let texts: Vec<&str> = file
        .nodes
        .iter()
        .skip(1)
        .map(|node| &source[node.range.start as usize..node.range.end as usize])
        .collect();
    assert_eq!(texts, ["() => grüße(\"😀\")", "grüße(\"😀\")", "function ƒ() {}"]);
    assert_eq!(file.nodes[0].range.end as usize, source.len());
    assert_eq!(file.nodes[3].lines.start, 4);
}
