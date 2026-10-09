//! Failures, extraction options, the value table and occurrence facts, the payload, the
//! linking, and the census.

use unfold_core::validate::Census;
use unfold_core::vocab::{EdgeKind, Reason, Resolution};
use unfold_core::{ByteRange, RepoPath, Snapshot};
use unfold_engine::facts::{CallContext, ValueShape};
use unfold_engine::settings::Settings;
use unfold_engine::{FileFacts, Language, LanguageId};
use unfold_lang_typescript::census::TypeScriptCensus;
use unfold_lang_typescript::payload::{ImportKind, Payload, ValueSyntax};
use unfold_lang_typescript::{ExtractOptions, PRODUCER_NAME, TypeScript, extract};

use crate::support::{compile, compile_source};

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
fn values_record_shapes_labels_receivers_and_inline_blocks() {
    let facts =
        facts("api.client.get(handler, this.run, (x) => x, ...rest, items[\"key\"], items[key], make());\n");
    let graph = &facts.graph;
    // The outer call, then `make()`.
    let call = &graph.calls()[0];
    assert_eq!(graph.calls().len(), 2);
    let value = |index: u32| &graph.values()[index as usize];
    let callee = value(call.callee.expect("a callee"));
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
    let arguments: Vec<_> = call
        .arguments
        .iter()
        .map(|argument| {
            (
                argument.position,
                value(argument.value).shape,
                value(argument.value).label.to_string(),
                argument.reference,
            )
        })
        .collect();
    assert_eq!(
        arguments,
        [
            (0, ValueShape::Name, "handler".to_owned(), true),
            (1, ValueShape::Member, "this.run".to_owned(), true),
            (2, ValueShape::InlineFunction, "(…)".to_owned(), true),
            (3, ValueShape::Other, "(…)".to_owned(), false),
            (4, ValueShape::Member, "items[\"key\"]".to_owned(), true),
            (5, ValueShape::Other, "items[…]".to_owned(), true),
            (6, ValueShape::CallResult, "make()".to_owned(), true),
        ]
    );
    let inline = value(call.arguments[2].value);
    assert_eq!(
        graph.blocks()[inline.block.expect("its block") as usize]
            .kind
            .to_string(),
        "function"
    );
    // Every receiver precedes the access that uses it.
    for (index, value) in graph.values().iter().enumerate() {
        assert!(value.receiver.is_none_or(|receiver| (receiver as usize) < index));
    }
}

#[test]
fn constructions_super_calls_and_tagged_templates() {
    let facts = facts("class A extends B { constructor() { super(1); } }\nnew A(2);\ntag`x${y}`;\n");
    let calls: Vec<_> = facts
        .graph
        .calls()
        .iter()
        .map(|call| {
            let label = call
                .callee
                .map(|callee| facts.graph.values()[callee as usize].label.to_string());
            (label, call.edge, call.context, call.arguments.len())
        })
        .collect();
    assert_eq!(
        calls,
        [
            (
                Some("super".to_owned()),
                EdgeKind::Calls,
                CallContext::Construct,
                1
            ),
            (
                Some("A".to_owned()),
                EdgeKind::Constructs,
                CallContext::Construct,
                1
            ),
            (Some("tag".to_owned()), EdgeKind::Calls, CallContext::Call, 0),
        ]
    );
}

#[test]
fn the_payload_aligns_with_values_and_imports() {
    let facts = facts(
        "import a from './a.js';\nexport * from './b.js';\nimport c = require('./c.js');\nconst d = import(name);\nobj.method().next(this, super_);\n",
    );
    let payload = Payload::from_bytes(&facts.payload).expect("decodes");
    assert_eq!(payload.values.len(), facts.graph.values().len());
    assert_eq!(
        payload.imports,
        [
            ImportKind::Import,
            ImportKind::Reexport,
            ImportKind::ImportEquals,
            ImportKind::Dynamic
        ]
    );
    assert_eq!(facts.graph.imports()[3].specifier, None);
    assert!(
        payload
            .values
            .iter()
            .any(|value| matches!(value, ValueSyntax::CallResult { callee: Some(_) }))
    );
    assert!(
        payload
            .values
            .iter()
            .any(|value| matches!(value, ValueSyntax::This { .. }))
    );
    assert_eq!(
        Payload::from_bytes(&payload.to_bytes().expect("encodes")).expect("decodes"),
        payload
    );
}

#[test]
fn linking_resolves_imports_calls_and_references() {
    let compiled = compile_source(
        "a.ts",
        "import x from './b.js';\nconst y = import(name);\nfunction f(g: () => void) { g(); }\nf(f);\nnew Date();\n",
    )
    .valid();
    let file = compiled.file("a.ts");
    let outcomes: Vec<_> = file
        .edges
        .iter()
        .map(|edge| {
            (
                edge.kind,
                edge.resolution,
                edge.reason,
                edge.target.as_ref().map(|key| key.as_str().to_owned()),
                edge.module.as_ref().map(|module| module.as_str().to_owned()),
                edge.position,
            )
        })
        .collect();
    let f = Some("a.ts#function:f".to_owned());
    assert_eq!(
        outcomes,
        [
            (
                EdgeKind::Imports,
                Resolution::Unresolved,
                Some(Reason::NotFound),
                None,
                None,
                None
            ),
            (
                EdgeKind::Imports,
                Resolution::Unresolved,
                Some(Reason::Dynamic),
                None,
                None,
                None
            ),
            (
                EdgeKind::Calls,
                Resolution::Unresolved,
                Some(Reason::Parameter),
                None,
                None,
                Some(0)
            ),
            (EdgeKind::Calls, Resolution::Resolved, None, f.clone(), None, None),
            (EdgeKind::References, Resolution::Resolved, None, f, None, Some(0)),
            (
                EdgeKind::Constructs,
                Resolution::External,
                None,
                None,
                Some("global".to_owned()),
                None
            ),
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
    assert!(compiled.file("good.ts").blocks.len() > 1);
    assert!(compiled.file("bad.ts").blocks.is_empty());
}

#[test]
fn byte_order_marks_line_endings_and_unicode_keep_exact_ranges() {
    let source = "\u{feff}#!/usr/bin/env node\r\nexport const café = () => grüße(\"😀\");\u{2028}// note\r\nfunction ƒ() {}\r\n";
    let compiled = compile_source("a.ts", source).valid();
    let file = compiled.file("a.ts");
    let texts: Vec<&str> = file
        .blocks
        .iter()
        .skip(1)
        .map(|block| &source[block.range.start as usize..block.range.end as usize])
        .collect();
    assert_eq!(
        texts,
        [
            "café = () => grüße(\"😀\")",
            "() => grüße(\"😀\")",
            "grüße(\"😀\")",
            "function ƒ() {}"
        ]
    );
    assert_eq!(file.blocks[0].range.end as usize, source.len());
    assert_eq!(file.blocks[4].lines.start, 4);
}
