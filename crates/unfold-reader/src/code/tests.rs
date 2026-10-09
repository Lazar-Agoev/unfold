//! The Code render model on hand-written files.

use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::testkit::{BlockRef, GraphBuilder, Outcome};
use unfold_core::vocab::{BlockKind as K, EdgeKind, Language, Reason, Role, SlotName as S};
use unfold_core::{ByteRange, FileGraph, Graph, Key};

use super::{CodeSource, CodeText, FoldPolicy, code_view, group_for_lines, render};

/// Highlights every occurrence of a few keywords, wherever they are.
struct Keywords;

impl Highlighter for Keywords {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
        let mut spans: Vec<HighlightSpan> = ["function", "const", "def", "//"]
            .iter()
            .flat_map(|word| text.match_indices(word).map(move |(at, _)| (at, word.len())))
            .map(|(at, len)| HighlightSpan {
                range: ByteRange::new(to_u32(at), to_u32(at + len)).unwrap(),
                class: TokenClass::Keyword,
            })
            .collect();
        spans.sort();
        spans
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap()
}

/// The UTF-16 offset of byte `at` in `text`.
fn units(text: &str, at: usize) -> u32 {
    to_u32(text[..at].encode_utf16().count())
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

fn slot(g: &mut GraphBuilder, block: BlockRef, name: S, needle: &str) {
    let range = g.find(block, needle);
    g.slot(block, name, None, range);
}

fn only_file(graph: Graph) -> FileGraph {
    let mut file = graph.files.into_iter().next().unwrap();
    file.edges.sort_by_key(|edge| (edge.source, edge.seq));
    file.slots.sort_by_key(|slot| (slot.block, slot.range.start));
    file
}

fn current(text: CodeSource) -> CodeText {
    match text {
        CodeSource::Current(text) => text,
        other => panic!("expected verified source, found {other:?}"),
    }
}

const HANDLE_TS: &str = "// lead\nfunction handle(req, res) {\n  const id = parse(req);\n  res.status(202).json({ id });\n  items.forEach((item) => {\n    log(item);\n  });\n}\nfunction parse(value) {}\n";

/// `handle` with a resolved call, a chained call, and a callback; returns the file, the local ID
/// of `handle`, and the callback's key.
fn handle() -> (FileGraph, u32, Key) {
    let mut g = GraphBuilder::new();
    let file = g.file("src/handle.ts", Language::TypeScript, HANDLE_TS);
    let handle = add(
        &mut g,
        file,
        K::Function,
        Role::Body,
        Some("handle"),
        "function handle(req, res) {\n  const id = parse(req);\n  res.status(202).json({ id });\n  items.forEach((item) => {\n    log(item);\n  });\n}",
    );
    slot(&mut g, handle, S::Body, "{\n  const id");
    let variable = add(
        &mut g,
        handle,
        K::Variable,
        Role::Body,
        Some("id"),
        "const id = parse(req);",
    );
    let parse_call = add(&mut g, variable, K::Call, Role::Initializer, None, "parse(req)");
    slot(&mut g, parse_call, S::Callee, "parse");
    let chained_statement = add(
        &mut g,
        handle,
        K::ExpressionStatement,
        Role::Body,
        None,
        "res.status(202).json({ id });",
    );
    let json = add(
        &mut g,
        chained_statement,
        K::Call,
        Role::Expression,
        None,
        "res.status(202).json({ id })",
    );
    slot(&mut g, json, S::Callee, "res.status(202).json");
    let status = add(&mut g, json, K::Call, Role::Callee, None, "res.status(202)");
    slot(&mut g, status, S::Callee, "res.status");
    let each_statement = add(
        &mut g,
        handle,
        K::ExpressionStatement,
        Role::Body,
        None,
        "items.forEach((item) => {\n    log(item);\n  });",
    );
    let each = add(
        &mut g,
        each_statement,
        K::Call,
        Role::Expression,
        None,
        "items.forEach((item) => {\n    log(item);\n  })",
    );
    slot(&mut g, each, S::Callee, "items.forEach");
    let callback = add(
        &mut g,
        each,
        K::Function,
        Role::Argument,
        None,
        "(item) => {\n    log(item);\n  }",
    );
    let log_statement = add(
        &mut g,
        callback,
        K::ExpressionStatement,
        Role::Body,
        None,
        "log(item);",
    );
    let log = add(
        &mut g,
        log_statement,
        K::Call,
        Role::Expression,
        None,
        "log(item)",
    );
    slot(&mut g, log, S::Callee, "log");
    let parse = add(
        &mut g,
        file,
        K::Function,
        Role::Body,
        Some("parse"),
        "function parse(value) {}",
    );
    g.edge(
        parse_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(parse)),
        "parse",
        None,
    );
    g.edge(
        json,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::Parameter),
        "res.status(202).json",
        Some(1),
    );
    g.edge(
        status,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::Parameter),
        "res.status",
        Some(1),
    );
    g.edge(
        each,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownReceiver),
        "items.forEach",
        None,
    );
    g.edge(log, EdgeKind::Calls, Outcome::external("console"), "log", None);
    let callback_key = g.key(callback);
    (only_file(g.build()), handle.local_id(), callback_key)
}

#[test]
fn a_card_has_lines_spans_and_call_links() {
    let (file, handle, callback) = handle();
    let text = current(CodeSource::Current(
        render(&file, handle, HANDLE_TS, Some(&Keywords), FoldPolicy::Card).unwrap(),
    ));
    let start = HANDLE_TS.find("function handle").unwrap();
    assert!(text.text.starts_with("function handle(req, res) {"));
    assert!(text.text.ends_with("});\n}"));
    let numbers: Vec<u32> = text.lines.iter().map(|line| line.number).collect();
    assert_eq!(numbers, [2, 3, 4, 5, 6, 7, 8]);
    assert_eq!((text.lines[1].indent, text.lines[4].indent), (2, 4));
    assert_eq!(text.dedent, 0);
    assert_eq!((text.lines[0].start, text.lines[0].end), (0, 27));

    // The comment before the card is clipped away; `function` and `const` remain.
    let classes: Vec<(u32, u32)> = text.spans.iter().map(|span| (span.0, span.1)).collect();
    let local = |needle: &str| to_u32(HANDLE_TS.find(needle).unwrap() - start);
    assert_eq!(classes, [(0, 8), (local("const"), local("const") + 5)]);

    type Row<'a> = (&'a str, Option<[u32; 2]>, Option<&'a str>);
    let links: Vec<Row<'_>> = text
        .links
        .iter()
        .map(|link| (&*link.text, link.span, link.note.as_deref()))
        .collect();
    let span = |needle: &str| Some([local(needle), local(needle) + to_u32(needle.len())]);
    assert_eq!(
        links,
        [
            ("parse", span("parse"), None),
            // The outer link starts after the nested call that begins its callee, without the dot.
            ("res.status(202).json", span("json"), Some("parameter")),
            ("res.status", span("res.status"), Some("parameter")),
            (
                "items.forEach",
                span("items.forEach"),
                Some("receiver type unknown")
            ),
            ("log", span("log"), Some("console")),
        ]
    );
    // The callback owns its call; the arrow for each link leaves from the occurrence's line.
    assert_eq!(text.links[4].owner, callback);
    assert_eq!(text.links[4].line, 6);
    assert_eq!(text.links[0].owner, file.blocks[handle as usize].key);
    assert!(text.links[0].target.is_some());
}

#[test]
fn outline_rows_and_groups() {
    let (file, handle, callback) = handle();
    let text = render(&file, handle, HANDLE_TS, None, FoldPolicy::Card).unwrap();
    let rows: Vec<(K, Option<u32>, bool)> = text
        .outline
        .iter()
        .map(|item| (item.kind, item.parent, item.groupable))
        .collect();
    assert_eq!(
        rows,
        [
            (K::Variable, None, true),
            (K::ExpressionStatement, None, true),
            (K::ExpressionStatement, None, true),
            (K::Function, Some(2), false),
            (K::ExpressionStatement, Some(3), true),
        ]
    );
    // The callback folds (multi-line callable) but starts open in a short card.
    let fold = text.outline[3].fold.as_ref().unwrap();
    assert!(!fold.folded);
    assert_eq!(fold.summary, "2 lines");
    assert_eq!(text.outline[3].key, callback);
    assert!(text.spans.is_empty());

    let keys = |from, to| group_for_lines(&text.outline, from, to);
    assert_eq!(
        keys(3, 5),
        [
            text.outline[0].key.clone(),
            text.outline[1].key.clone(),
            text.outline[2].key.clone()
        ]
    );
    assert_eq!(keys(4, 3), keys(3, 4));
    // Strictly inside the statement that holds the callback: only the callback is there, and a
    // nested definition is never a group member.
    assert!(keys(6, 6).is_empty());
}

/// A function longer than a card's limit when `filler` is set, with a three-check `else if`
/// chain and a callback.
fn long_function(filler: bool) -> (String, FileGraph, u32) {
    let mut source = String::from(
        "function big(x) {\n  if (x === 1) {\n    a();\n  } else if (x === 2) {\n    b();\n  } else if (x === 3) {\n    c();\n  }\n  run(() => {\n    d();\n  });\n",
    );
    if filler {
        for line in 0..35 {
            source.push_str(&format!("  // filler {line}\n"));
        }
    }
    source.push_str("}\n");
    let mut g = GraphBuilder::new();
    let file = g.file("src/big.ts", Language::TypeScript, &source);
    let body_end = source.len() - 1;
    let big = g.block(
        file,
        K::Function,
        Some(Role::Body),
        Some("big"),
        ByteRange::new(0, to_u32(body_end)).unwrap(),
    );
    let first = g.find(file, "if (x === 1)").start;
    let chain_end = g.find(file, "    c();\n  }").end;
    let if1 = g.block(
        big,
        K::If,
        Some(Role::Body),
        None,
        ByteRange::new(first, chain_end).unwrap(),
    );
    slot(&mut g, if1, S::Then, "{\n    a();");
    add(&mut g, if1, K::ExpressionStatement, Role::Then, None, "a();");
    let second = g.find(file, "if (x === 2)").start;
    let if2 = g.block(
        if1,
        K::If,
        Some(Role::Else),
        None,
        ByteRange::new(second, chain_end).unwrap(),
    );
    slot(&mut g, if2, S::Then, "{\n    b();");
    add(&mut g, if2, K::ExpressionStatement, Role::Then, None, "b();");
    let third = g.find(file, "if (x === 3)").start;
    let if3 = g.block(
        if2,
        K::If,
        Some(Role::Else),
        None,
        ByteRange::new(third, chain_end).unwrap(),
    );
    slot(&mut g, if3, S::Then, "{\n    c();");
    add(&mut g, if3, K::ExpressionStatement, Role::Then, None, "c();");
    let statement = add(
        &mut g,
        big,
        K::ExpressionStatement,
        Role::Body,
        None,
        "run(() => {\n    d();\n  });",
    );
    let call = add(
        &mut g,
        statement,
        K::Call,
        Role::Expression,
        None,
        "run(() => {\n    d();\n  })",
    );
    let callback = add(
        &mut g,
        call,
        K::Function,
        Role::Argument,
        None,
        "() => {\n    d();\n  }",
    );
    add(&mut g, callback, K::ExpressionStatement, Role::Body, None, "d();");
    (source, only_file(g.build()), big.local_id())
}

fn folded(text: &CodeText) -> Vec<(K, Option<Role>)> {
    text.outline
        .iter()
        .filter(|item| item.fold.as_ref().is_some_and(|fold| fold.folded))
        .map(|item| (item.kind, item.role))
        .collect()
}

#[test]
fn long_cards_fold_callbacks_and_long_chains() {
    let (source, file, big) = long_function(true);
    let text = render(&file, big, &source, None, FoldPolicy::Card).unwrap();
    assert!(text.lines.len() > super::LONG_CARD_LINES);
    // The chain's head folds; its `else if` links and the short `if`s do not.
    assert_eq!(
        folded(&text),
        [(K::If, Some(Role::Body)), (K::Function, Some(Role::Argument))]
    );
    let head = text.outline[0].fold.as_ref().unwrap();
    assert_eq!(head.chain, Some(3));
    assert!(head.braces);
    assert_eq!(head.header_end, to_u32(source.find("{\n    a();").unwrap()));
    assert_eq!(head.summary, "6 lines");
    assert_eq!(text.outline[2].fold.as_ref().unwrap().chain, Some(2));

    // A source tab never starts folded; nor does a short card.
    let tab = render(&file, big, &source, None, FoldPolicy::SourceTab).unwrap();
    assert!(folded(&tab).is_empty());
    let (source, file, big) = long_function(false);
    let short = render(&file, big, &source, None, FoldPolicy::Card).unwrap();
    assert!(folded(&short).is_empty());
}

#[test]
fn python_folds_keep_their_header_and_offsets_are_utf16() {
    let source = "# é😀\r\ndef greet():\r\n    if ready:\r\n        print(\"héllo 😀\")\r\n";
    let mut g = GraphBuilder::new();
    let file = g.file("greet.py", Language::Python, source);
    let greet = add(
        &mut g,
        file,
        K::Function,
        Role::Body,
        Some("greet"),
        "def greet():\r\n    if ready:\r\n        print(\"héllo 😀\")",
    );
    slot(&mut g, greet, S::Body, "if ready:");
    let branch = add(
        &mut g,
        greet,
        K::If,
        Role::Body,
        None,
        "if ready:\r\n        print(\"héllo 😀\")",
    );
    slot(&mut g, branch, S::Then, "print(");
    let statement = add(
        &mut g,
        branch,
        K::ExpressionStatement,
        Role::Then,
        None,
        "print(\"héllo 😀\")",
    );
    let call = add(
        &mut g,
        statement,
        K::Call,
        Role::Expression,
        None,
        "print(\"héllo 😀\")",
    );
    slot(&mut g, call, S::Callee, "print");
    g.edge(
        call,
        EdgeKind::Calls,
        Outcome::external("python:builtins"),
        "print",
        None,
    );
    let file = only_file(g.build());

    let text = render(&file, 0, source, Some(&Keywords), FoldPolicy::SourceTab).unwrap();
    // CRLF ends lines (the final one adds no empty line); offsets count UTF-16 units.
    let bounds: Vec<(u32, u32)> = text.lines.iter().map(|line| (line.start, line.end)).collect();
    let line_starts: Vec<usize> = [
        0,
        source.find("def").unwrap(),
        source.find("    if").unwrap(),
        source.find("        print").unwrap(),
    ]
    .into();
    let line_ends: Vec<usize> = line_starts
        .iter()
        .map(|&start| start + source[start..].find("\r\n").unwrap())
        .collect();
    let expected: Vec<(u32, u32)> = line_starts
        .iter()
        .zip(&line_ends)
        .map(|(&start, &end)| (units(source, start), units(source, end)))
        .collect();
    assert_eq!(bounds, expected);
    assert_eq!(text.lines[0].end, 5);
    let print = source.find("print").unwrap();
    assert_eq!(
        text.links[0].span,
        Some([units(source, print), units(source, print) + 5])
    );
    assert_eq!(text.links[0].note.as_deref(), Some("python:builtins"));
    assert_eq!(text.spans.len(), 1);
    assert_eq!(text.spans[0].0, units(source, source.find("def").unwrap()));

    // Python folds hide everything after the written header line, with no braces.
    let function = text.outline[0].fold.as_ref().unwrap();
    assert_eq!(
        function.header_end,
        units(source, source.find("():").unwrap() + 3)
    );
    assert!(!function.braces);
    let inner = text.outline[1].fold.as_ref().unwrap();
    assert_eq!(
        inner.header_end,
        units(source, source.find("ready:").unwrap() + 6)
    );

    // A card of the function starts at its own first line, relative offsets included.
    let card = render(&file, 1, source, None, FoldPolicy::Card).unwrap();
    assert_eq!(card.lines[0].number, 2);
    assert_eq!(card.dedent, 4);
    let card_start = source.find("def").unwrap();
    assert_eq!(
        card.links[0].span.unwrap()[0],
        units(&source[card_start..], print - card_start)
    );
}

#[test]
fn code_views_verify_the_source_first() {
    let (file, handle, _) = handle();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    let path = dir.path().join("src/handle.ts");
    std::fs::write(&path, HANDLE_TS).unwrap();
    let view = code_view(dir.path(), &file, handle, None, FoldPolicy::Card).unwrap();
    assert_eq!((view.start_line, view.end_line), (2, 8));
    assert!(matches!(view.source, CodeSource::Current(_)));

    // Changed bytes: the old ranges are never applied, and no text is returned.
    std::fs::write(&path, HANDLE_TS.replace("parse(req)", "parse(request)")).unwrap();
    let view = code_view(dir.path(), &file, handle, None, FoldPolicy::Card).unwrap();
    assert_eq!(view.source, CodeSource::Changed);
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["source"], serde_json::json!({"state": "changed"}));
    assert_eq!(json["startLine"], 2);

    std::fs::remove_file(&path).unwrap();
    let view = code_view(dir.path(), &file, handle, None, FoldPolicy::Card).unwrap();
    assert_eq!(view.source, CodeSource::Missing);
    assert!(code_view(dir.path(), &file, 999, None, FoldPolicy::Card).is_err());
}

#[test]
fn the_model_serializes_compactly_in_camel_case() {
    let (file, handle, _) = handle();
    let text = render(&file, handle, HANDLE_TS, Some(&Keywords), FoldPolicy::Card).unwrap();
    let json = serde_json::to_value(CodeSource::Current(text)).unwrap();
    assert_eq!(json["state"], "current");
    assert_eq!(json["spans"][0], serde_json::json!([0, 8, "keyword"]));
    assert_eq!(
        json["lines"][0],
        serde_json::json!({"number": 2, "start": 0, "end": 27, "indent": 0})
    );
    assert!(json["outline"][3]["fold"]["headerEnd"].is_u64());
    assert!(json["outline"][0].get("fold").is_none());
    let link = &json["links"][1];
    assert_eq!(link["resolution"], "unresolved");
    assert_eq!(link["reason"], "parameter");
    assert!(link.get("target").is_none());
}

#[test]
fn ranges_that_do_not_fit_the_text_are_errors_not_panics() {
    let (file, handle, _) = handle();
    assert!(render(&file, handle, "short", None, FoldPolicy::Card).is_err());
    // A range inside a multi-byte character.
    let mut broken = file;
    broken.blocks[handle as usize].range = ByteRange::new(2, 3).unwrap();
    assert!(render(&broken, handle, "aéb", None, FoldPolicy::Card).is_err());
}
