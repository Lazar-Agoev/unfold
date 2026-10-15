//! Board tests: strict parsing, canonical serialization, signatures, guards, the overlay checks
//! on save, health of every kind with its repairs, and board files, on a small hand-built graph
//! whose source is written to a temporary folder so it can be edited and compiled again.

use std::path::Path;

use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, NodeRef, library, project, target, unknown};
use unfold_core::vocab::{CallRole, Form, Language, LinkKind, Reason, TargetState};
use unfold_core::{ByteRange, Graph, Key};

use super::files::{BoardDirs, BoardFile, Change, MAX_BOARD_BYTES, Revision, slug};
use super::*;

/// Three functions: `run` loads a value, logs it when it is there, logs again, and returns it;
/// `load` returns `helper()`; `helper` returns an unknown call.
const SOURCE: &str = "export function run() {\n  const first = load();\n  if (first) { console.log(first); }\n  console.log('done');\n  return first;\n}\nexport function load() {\n  return helper();\n}\nexport function helper() {\n  return dep.fetch();\n}\n";

const MODULE: &str = "main.ts#";
const RUN: &str = "main.ts#run";
const LOAD_CALL: &str = "main.ts#run/call:load";
const IF0: &str = "main.ts#run/if[0]";
const THEN: &str = "main.ts#run/if[0]/then";
const INNER_LOG: &str = "main.ts#run/if[0]/then/call:console.log";
const LOG: &str = "main.ts#run/call:console.log";
const RETURN0: &str = "main.ts#run/return[0]";
const LOAD: &str = "main.ts#load";
const HELPER_CALL: &str = "main.ts#load/return[0]/call:helper";
const HELPER: &str = "main.ts#helper";
const FETCH: &str = "main.ts#helper/return[0]/call:dep.fetch";

const RUN_IT: &str = "entrance:shop/run-it";
const LOAD_IT: &str = "entrance:shop/load-it";

fn range(start: usize, end: usize) -> ByteRange {
    ByteRange {
        start: u32::try_from(start).unwrap(),
        end: u32::try_from(end).unwrap(),
    }
}

/// A call written as `text` (`callee(…)`) at byte `at` under `parent`, with its `calls` link:
/// to a function of `functions` in the file, to the global `console`, or unknown.
fn call(builder: &mut GraphBuilder, parent: NodeRef, at: usize, text: &str, functions: &[&str]) -> NodeRef {
    let written = &text[..text.find('(').unwrap()];
    let mut parts = written.split('.');
    let mut callee = Callee::name(parts.next().unwrap());
    for part in parts {
        callee = callee.member(part);
    }
    let end = at + text.rfind(')').unwrap() + 1;
    let node = builder.call(parent, &callee, range(at, end));
    let head = range(at, at + written.len());
    builder.node_mut(node).head = Some(head);
    let to = if functions.contains(&written) {
        project(&Key::from_text(&format!("main.ts#{written}")))
    } else if written == "console.log" {
        library("global")
    } else {
        unknown(Reason::UnknownReceiver)
    };
    builder.link(node, LinkKind::Calls, head, written, to);
    node
}

/// The graph of `main.ts` written as `text`: top-level `export function name() {…}`s, each
/// with one statement per line: `const x = f();`, `if (…) { console.log(…); }`, a call, or
/// `return …;` (whose value may be a call). The app `Shop` starts at the module; its entrances
/// are `Run it` (code `run`), `Load it`, and `Load again` (both code `load`).
fn graph_of(text: &str) -> Graph {
    let mut builder = GraphBuilder::new();
    let root = builder.file("main.ts", Language::TypeScript, text);
    let names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("export function "))
        .map(|rest| &rest[..rest.find('(').unwrap()])
        .collect();
    let mut offset = 0;
    let mut function = None;
    for line in text.split_inclusive('\n') {
        let at = offset + line.len() - line.trim_start().len();
        let statement = line.trim();
        offset += line.len();
        if let Some(rest) = statement.strip_prefix("export function ") {
            let start = at + "export ".len();
            let end = start + text[start..].find("\n}").unwrap() + 2;
            let name = &rest[..rest.find('(').unwrap()];
            let node = builder.def(root, Form::Function, name, range(start, end));
            builder.node_mut(node).body = Some(range(start + text[start..].find('{').unwrap(), end));
            function = Some(node);
            continue;
        }
        let Some(function) = function.filter(|_| !statement.is_empty() && statement != "}") else {
            continue;
        };
        let end = at + statement.len();
        if statement.starts_with("if (") {
            let decision = builder.block(function, Form::If, range(at, end));
            builder.node_mut(decision).stmt = Some(range(at, end));
            let open = at + statement.find('{').unwrap();
            let part = builder.block(decision, Form::Then, range(at, end));
            builder.node_mut(part).body = Some(range(open, end));
            let inner = &statement[statement.find('{').unwrap() + 2..];
            let node = call(&mut builder, part, open + 2, inner, &names);
            builder.node_mut(node).stmt = Some(range(open + 2, open + 2 + inner.find(';').unwrap() + 1));
        } else if let Some(value) = statement.strip_prefix("return ") {
            let exit = builder.block(function, Form::Return, range(at, end));
            let value_at = at + "return ".len();
            let node = builder.node_mut(exit);
            node.stmt = Some(range(at, end));
            node.value = Some(range(value_at, end - 1));
            if value.contains('(') {
                call(&mut builder, exit, value_at, value, &names);
            }
        } else if let Some(rest) = statement.strip_prefix("const ") {
            let (binds, value) = rest.split_once(" = ").unwrap();
            let value_at = end - value.len();
            let node = call(&mut builder, function, value_at, value, &names);
            let node = builder.node_mut(node);
            node.stmt = Some(range(at, end));
            node.binds = Some(binds.into());
        } else {
            let node = call(&mut builder, function, at, statement, &names);
            builder.node_mut(node).stmt = Some(range(at, end));
        }
    }
    let shop = builder.app(
        "Shop",
        Some("npm start"),
        Some(target(MODULE, TargetState::Found)),
    );
    builder.entrance(
        shop,
        "Run it",
        Some("POST /run"),
        vec![target(RUN, TargetState::Found)],
    );
    for name in ["Load it", "Load again"] {
        builder.entrance(
            shop,
            name,
            Some("GET /load"),
            vec![target(LOAD, TargetState::Found)],
        );
    }
    builder.build()
}

/// A folder holding `text` as `main.ts`, and the board graph of it.
fn folder(text: &str) -> (tempfile::TempDir, BoardGraph) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.ts"), text).unwrap();
    let graph = BoardGraph::from_graph(&graph_of(text), dir.path());
    (dir, graph)
}

/// The same folder after an edit and a compile.
fn recompiled(dir: &Path, text: &str) -> BoardGraph {
    std::fs::write(dir.join("main.ts"), text).unwrap();
    BoardGraph::from_graph(&graph_of(text), dir)
}

/// The exact text of `needle` in `SOURCE`.
fn bytes_of(needle: &str) -> &'static [u8] {
    let start = SOURCE.find(needle).unwrap();
    &SOURCE.as_bytes()[start..start + needle.len()]
}

/// `run`'s source.
fn run_source() -> &'static str {
    &SOURCE[SOURCE.find("function run").unwrap()..SOURCE.find("\n}\n").unwrap() + 2]
}

fn key(text: &str) -> Key {
    Key::from_text(text)
}

fn agent() -> Author {
    Author::agent("test").unwrap()
}

fn person() -> Author {
    Author::person("tester").unwrap()
}

fn unsigned(text: &str) -> Description {
    Description {
        text: text.to_owned(),
        author: agent(),
        signature: None,
    }
}

fn block_card(id: &str, block: &str) -> Card {
    Card {
        id: id.to_owned(),
        reference: CardRef::block(key(block)),
        at: [10.0, 20.0],
        view: None,
        description: None,
        open: Vec::new(),
    }
}

fn start_card(id: &str, start: &str) -> Card {
    Card {
        reference: CardRef::entrance(key(start)),
        ..block_card(id, RUN)
    }
}

fn name(block: &str, text: &str) -> NameEntry {
    NameEntry {
        key: key(block),
        name: unsigned(text),
    }
}

fn combined(id: &str, members: &[&str]) -> Combined {
    Combined {
        id: id.to_owned(),
        title: "Steps".to_owned(),
        members: members.iter().map(|member| key(member)).collect(),
        collapsed: true,
        description: None,
        author: agent(),
        signature: None,
    }
}

fn proposal(id: &str, place: Place) -> Proposal {
    Proposal {
        id: id.to_owned(),
        place,
        title: "Email the customer".to_owned(),
        reason: Some("They asked for a receipt".to_owned()),
        author: agent(),
    }
}

fn after(block: &str) -> Place {
    Place::After { after: key(block) }
}

fn start(block: &str) -> Place {
    Place::Start { start: key(block) }
}

/// A board whose one page shows card `run`.
fn run_board() -> Board {
    let mut board = Board::new("Run explanation");
    board.pages[0].cards.push(block_card("run", RUN));
    board
}

/// `run_board` with `edit` applied to its page.
fn run_board_with(edit: impl FnOnce(&mut Page)) -> Board {
    let mut board = run_board();
    edit(&mut board.pages[0]);
    board
}

fn save(
    board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    prepare_save(board, previous, graph)
}

/// The problem a save refuses `board` with.
fn refusal(board: Board, graph: &BoardGraph) -> (Target, String) {
    match save(board, None, graph) {
        Err(BoardError::Problem { target, message, .. }) => (target, message),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn repair(board: &mut Board, graph: &BoardGraph, repair: &Repair) -> Result<(), BoardError> {
    apply_repair(&mut board.pages[0], graph, repair)
}

/// The health items of `board`'s page as (kind, target, key, repairs).
fn items(health: &Health) -> Vec<(HealthKind, Target, Option<&str>, &[RepairKind])> {
    health
        .items
        .iter()
        .map(|item| {
            (
                item.kind,
                item.target.clone(),
                item.key.as_deref(),
                item.repairs.as_slice(),
            )
        })
        .collect()
}

fn invalid(text: &str) -> (String, String) {
    match parse(text) {
        Err(BoardError::Invalid { path, message }) => (path, message),
        other => panic!("expected an invalid board, got {other:?}"),
    }
}

const MINIMAL: &str = r#"{"format":"unfold-board","pages":[{"cards":[{"at":[0,0],"id":"run","ref":{"block":"main.ts#run"}}],"defaultView":"compact","id":"page-1","title":"P"}],"title":"T","version":1}"#;

/// `MINIMAL` with `fields` (`"name":value,…`) added to its page.
fn with_page(fields: &str) -> String {
    MINIMAL.replace(r#""id":"page-1""#, &format!(r#"{fields},"id":"page-1""#))
}

/// `MINIMAL` with `fields` added to its card.
fn with_card(fields: &str) -> String {
    MINIMAL.replace(r#""id":"run""#, &format!(r#"{fields},"id":"run""#))
}

#[test]
fn parsing_rejects_everything_outside_the_format_with_a_path() {
    assert!(parse(MINIMAL).is_ok());
    let signature = format!("sha256:{}", "0".repeat(64));
    let connection = |fields: &str| {
        with_page(&format!(
            r#""links":[{{"author":"agent:a","from":"run","id":"c","to":"other",{fields}}}],"resources":[{{"anchors":[],"at":[0,0],"author":"agent:a","id":"other","kind":"actor","name":"Customer"}}]"#
        ))
    };
    let open = |paths: &str| with_card(&format!(r#""open":{paths}"#));
    let deep = format!(r#"[["{LOAD_CALL}","{HELPER_CALL}","{FETCH}","{LOG}"]]"#);
    let many = format!(
        "[{}]",
        (0..65)
            .map(|index| format!(r#"["main.ts#run/call:f[{}]"]"#, index + 1))
            .collect::<Vec<_>>()
            .join(",")
    );
    let cases = [
        (
            MINIMAL.replace(r#""title":"T""#, r#""extra":1,"title":"T""#),
            "/extra",
            "unknown field",
        ),
        (
            MINIMAL.replace("unfold-board", "other"),
            "/format",
            "must be \"unfold-board\"",
        ),
        (
            MINIMAL.replace(r#""version":1"#, r#""version":2"#),
            "/version",
            "unsupported version; expected 1",
        ),
        (
            MINIMAL.replace(r#""title":"T""#, r#""folder":"a/b","title":"T""#),
            "/folder",
            "must be a folder name",
        ),
        (
            MINIMAL.replace(r#""compact""#, r#""code""#),
            "/pages/0/defaultView",
            "must be \"compact\" or \"expanded\"",
        ),
        (
            MINIMAL.replace(r#"{"block":"main.ts#run"}"#, r#"{"block":"entrance:shop"}"#),
            "/pages/0/cards/0/ref/block",
            "must be a block key",
        ),
        (
            MINIMAL.replace(r#"{"block":"main.ts#run"}"#, r#"{"entrance":"main.ts#run"}"#),
            "/pages/0/cards/0/ref/entrance",
            "must be an app's or entrance's key",
        ),
        (
            MINIMAL.replace(r#"{"block":"main.ts#run"}"#, r#"{"block":"main.ts"}"#),
            "/pages/0/cards/0/ref/block",
            "must be a block key",
        ),
        (
            with_card(r#""view":"flow""#),
            "/pages/0/cards/0/view",
            "must be \"compact\" or \"expanded\"",
        ),
        (open(&deep), "/pages/0/cards/0/open/0", "must list 1 to 3 keys"),
        (open("[[]]"), "/pages/0/cards/0/open/0", "must list 1 to 3 keys"),
        (open(&many), "/pages/0/cards/0/open", "at most 64 open paths"),
        (
            open(&format!(r#"[["{LOAD_CALL}"],["{LOAD_CALL}"]]"#)),
            "/pages/0/cards/0/open/1",
            "repeats an earlier open path",
        ),
        (
            MINIMAL.replace(
                r#""ref":{"block":"main.ts#run"}"#,
                &format!(r#""open":[["{LOAD_CALL}"]],"ref":{{"entrance":"{RUN_IT}"}}"#),
            ),
            "/pages/0/cards/0/open",
            "only a block card opens calls in place",
        ),
        (
            with_page(&format!(
                r#""names":[{{"author":"agent:a","key":"{IF0}","text":"A"}},{{"author":"agent:a","key":"{IF0}","text":"B"}}]"#
            )),
            "/pages/0/names/1",
            "repeats the name of",
        ),
        (
            with_page(r#""names":[{"author":"agent:a","key":"entrance:shop","text":"A"}]"#),
            "/pages/0/names/0/key",
            "must be a block key",
        ),
        (
            with_page(&format!(
                r#""combined":[{{"collapsed":true,"id":"s","members":["{IF0}"],"title":"S"}}]"#
            )),
            "/pages/0/combined/0",
            "missing field `author`",
        ),
        (
            with_page(
                r#""combined":[{"author":"agent:a","collapsed":true,"id":"s","members":[],"title":"S"}]"#,
            ),
            "/pages/0/combined/0/members",
            "must list at least one block key",
        ),
        (
            with_page(&format!(
                r#""combined":[{{"author":"agent:a","collapsed":true,"id":"s","members":["{IF0}","{IF0}"],"title":"S"}}]"#
            )),
            "/pages/0/combined/0/members/1",
            "repeats an earlier member",
        ),
        (
            with_page(&format!(
                r#""combined":[{{"author":"agent:a","collapsed":true,"id":"s","members":["{IF0}"],"title":"S"}},{{"author":"agent:a","collapsed":false,"id":"t","members":["{LOG}","{IF0}"],"title":"T"}}]"#
            )),
            "/pages/0/combined/1/members/1",
            "is already in combined step `s`; combined steps never overlap",
        ),
        (
            with_page(&format!(
                r#""guards":[{{"key":"{IF0}","kind":"if","owner":"","signature":"{signature}"}}]"#
            )),
            "/pages/0/guards/0/kind",
            "must be a node kind or `entrance`",
        ),
        (
            with_page(&format!(
                r#""guards":[{{"key":"{RUN_IT}","kind":"entrance:http","owner":"","signature":"{signature}"}}]"#
            )),
            "/pages/0/guards/0/kind",
            "must be a node kind or `entrance`",
        ),
        (
            connection(r#""kind":"rpc""#),
            "/pages/0/links/0/kind",
            "must be \"http\", \"queue\", \"reads\", \"writes\", \"calls\", \"relates\", or \"other\"",
        ),
        (
            connection(r#""kind":"event""#),
            "/pages/0/links/0/kind",
            "must be \"http\"",
        ),
        (
            connection(r#""label":{"author":"agent:a","text":"x"}"#),
            "/pages/0/links/0",
            "missing field `kind`",
        ),
        (
            connection(r#""kind":"other""#),
            "/pages/0/links/0",
            "missing field `label`: an `other` connection says what it is",
        ),
        (
            connection(r#""kind":"writes","fromColumn":"id""#),
            "/pages/0/links/0/fromColumn",
            "only a `relates` connection joins columns",
        ),
        (
            connection(&format!(r#""kind":"writes","step":"{RUN_IT}""#)),
            "/pages/0/links/0/step",
            "must be a block key",
        ),
        (
            with_page(&format!(
                r#""proposals":[{{"after":"{LOG}","author":"agent:a","id":"p","start":"{RUN}","title":"T"}}]"#
            )),
            "/pages/0/proposals/0",
            "must have exactly one of `after` or `start`",
        ),
        (
            with_page(r#""proposals":[{"author":"agent:a","id":"p","title":"T"}]"#),
            "/pages/0/proposals/0",
            "must have exactly one of `after` or `start`",
        ),
        (
            with_page(&format!(
                r#""proposals":[{{"after":"{LOG}","author":"agent:a","id":"p","title":"Two\nlines"}}]"#
            )),
            "/pages/0/proposals/0/title",
            "must be one line",
        ),
        (
            with_page(&format!(
                r#""proposals":[{{"after":"{LOG}","author":"agent:a","id":"run","title":"T"}}]"#
            )),
            "/pages/0/proposals/0/id",
            "duplicate id `run`",
        ),
        (
            MINIMAL.replace(r#""ref""#, r#""at2":[0,0],"ref""#),
            "/pages/0/cards/0/at2",
            "unknown field",
        ),
        (
            MINIMAL.replace(r#"[0,0]"#, r#"[0,"x"]"#),
            "/pages/0/cards/0/at/1",
            "must be a finite number",
        ),
    ];
    for (text, path, message) in cases {
        let (got_path, got_message) = invalid(&text);
        assert_eq!(got_path, path, "{text}\n{got_message}");
        assert!(got_message.contains(message), "{message:?} in {got_message:?}");
    }
    assert!(matches!(parse("{"), Err(BoardError::Syntax { .. })));
    assert!(matches!(
        parse(&MINIMAL.replace(r#""title":"T""#, r#""title":"T","title":"U""#)),
        Err(BoardError::Syntax { .. })
    ));
}

#[test]
fn old_board_files_fail_like_any_invalid_file() {
    let card = r#"{"at":[0,0],"id":"run","ref":{"block":"main.ts#run"}}"#;
    let page = |fields: &str| {
        format!(
            r#"{{"format":"unfold-board","pages":[{{"cards":[{card}],"defaultView":"compact","id":"p",{fields}"title":"P"}}],"title":"T","version":1}}"#
        )
    };
    let cases = [
        (page(r#""groups":[],"#), "/pages/0/groups"),
        (page(r#""labels":[],"#), "/pages/0/labels"),
        (
            MINIMAL.replace(r#""id":"run""#, r#""expanded":[],"id":"run""#),
            "/pages/0/cards/0/expanded",
        ),
        (
            MINIMAL.replace(r#""id":"run""#, r#""folded":true,"id":"run""#),
            "/pages/0/cards/0/folded",
        ),
        (
            MINIMAL.replace(r#""title":"T""#, r#""projects":[],"title":"T""#),
            "/projects",
        ),
        (
            MINIMAL.replace(r#""title":"T""#, r#""publishes":"x","title":"T""#),
            "/publishes",
        ),
        (
            MINIMAL.replace(r#""main.ts#run"}"#, r#""main.ts#run","project":"p"}"#),
            "/pages/0/cards/0/ref/project",
        ),
        (
            page(&format!(
                r#""proposals":[{{"after":"{LOG}","author":"agent:a","card":"run","id":"q","title":"T"}}],"#
            )),
            "/pages/0/proposals/0/card",
        ),
        (
            page(r#""links":[{"from":"run","id":"l","label":{"author":"agent:a","text":"x"},"to":"run"}],"#),
            "/pages/0/links/0",
        ),
    ];
    for (text, path) in cases {
        assert_eq!(invalid(&text).0, path, "{text}");
    }
    // Old keys and views fail too.
    let old_key = MINIMAL.replace("main.ts#run", "main.ts#function:run/variable:first");
    assert_eq!(invalid(&old_key).0, "/pages/0/cards/0/ref/block");
    assert_eq!(
        invalid(&MINIMAL.replace(r#""compact""#, r#""diagram""#)).0,
        "/pages/0/defaultView"
    );
    let old_entrance = MINIMAL.replace(
        r#"{"block":"main.ts#run"}"#,
        r#"{"entrance":"entrance:[\"express.route\",\"http\",\"POST\",\"/orders\"]"}"#,
    );
    assert_eq!(invalid(&old_entrance).0, "/pages/0/cards/0/ref/entrance");
}

#[test]
fn parsing_checks_frames_resources_connections_and_notes() {
    let resource = |id: &str, kind: &str, extra: &str| {
        format!(
            r#"{{"anchors":[],"at":[0,0],"author":"agent:a","id":"{id}","kind":"{kind}","name":"n"{extra}}}"#
        )
    };
    let actor = resource("who", "actor", "");
    let frame = |id: &str, members: &str| {
        format!(r#"{{"author":"agent:a","id":"{id}","look":"box","members":{members},"title":"F"}}"#)
    };
    let cases = [
        (
            with_page(&format!(r#""frames":[{}]"#, frame("f", r#"["nope"]"#))),
            "/pages/0/frames/0/members/0",
            "refers to an unknown item `nope`",
        ),
        (
            with_page(&format!(
                r#""frames":[{},{}]"#,
                frame("f", r#"["run"]"#),
                frame("g", r#"["run"]"#)
            )),
            "/pages/0/frames/1/members/0",
            "is already in frame `f`",
        ),
        (
            with_page(&format!(
                r#""frames":[{},{}]"#,
                frame("f", r#"["g"]"#),
                frame("g", r#"["f"]"#)
            )),
            "/pages/0/frames/0",
            "is inside itself",
        ),
        (
            with_page(&format!(
                r#""frames":[{},{},{},{}]"#,
                frame("a", r#"["b"]"#),
                frame("b", r#"["c"]"#),
                frame("c", r#"["d"]"#),
                frame("d", r#"["run"]"#)
            )),
            "/pages/0/frames/3",
            "frames nest at most 3 deep",
        ),
        (
            with_page(&format!(
                r#""resources":[{}]"#,
                resource("q", "queue", r#","columns":[{"name":"id"}]"#)
            )),
            "/pages/0/resources/0/columns",
            "only a table has a type and columns",
        ),
        (
            with_page(&format!(
                r#""resources":[{}]"#,
                resource(
                    "t",
                    "table",
                    &format!(r#","columns":[{{"name":"id","type":"string"}}],"type":"{RUN}""#)
                )
            )),
            "/pages/0/resources/0/columns/0/type",
            "a table backed by a type lists marks only",
        ),
        (
            with_page(&format!(
                r#""resources":[{}]"#,
                resource("t", "table", r#","columns":[{"name":"id","references":"x.id"}]"#)
            )),
            "/pages/0/resources/0/columns/0/references",
            "only a foreign key",
        ),
        (
            with_page(r#""links":[{"author":"agent:a","from":"run","id":"c","kind":"writes","to":"run"}]"#),
            "/pages/0/links/0/to",
            "must be another item than `from`",
        ),
        (
            with_page(&format!(
                r#""links":[{{"author":"agent:a","from":"run","id":"c","kind":"relates","to":"who"}}],"resources":[{actor}]"#
            )),
            "/pages/0/links/0/from",
            "must be a table",
        ),
        (
            with_page(&format!(
                r#""links":[{{"author":"agent:a","from":"who","id":"c","kind":"writes","step":"{LOG}","to":"run"}}],"resources":[{actor}]"#
            )),
            "/pages/0/links/0/step",
            "`from` must be a block card",
        ),
        (
            with_page(
                r#""links":[{"author":"agent:a","from":"n","id":"c","kind":"writes","to":"run"}],"notes":[{"at":[0,0],"author":"agent:a","id":"n","text":"x"}]"#,
            ),
            "/pages/0/links/0/from",
            "`n` is a note; a connection joins cards, resources, and frames",
        ),
        (
            with_page(r#""notes":[{"at":[0,0],"attach":"nope","author":"agent:a","id":"n","text":"x"}]"#),
            "/pages/0/notes/0/attach",
            "refers to an unknown item `nope`",
        ),
        (
            with_page(&format!(
                r#""notes":[{{"at":[0,0],"author":"agent:a","id":"n","text":"{}"}}]"#,
                "x".repeat(401)
            )),
            "/pages/0/notes/0/text",
            "at most 400 characters",
        ),
    ];
    for (text, path, message) in cases {
        let (got_path, got_message) = invalid(&text);
        assert_eq!(got_path, path, "{text}\n{got_message}");
        assert!(got_message.contains(message), "{message:?} in {got_message:?}");
    }
}

/// `full_board`'s canonical text.
const FULL_BOARD: &str = r##"{
  "folder": "Checkout",
  "format": "unfold-board",
  "pages": [
    {
      "cards": [
        {
          "at": [
            -0.5,
            1e+21
          ],
          "id": "a-entrance",
          "ref": {
            "entrance": "entrance:shop/run-it"
          }
        },
        {
          "at": [
            10,
            20
          ],
          "description": {
            "author": "agent:test",
            "signature": "",
            "text": "Loads a value, logs it, and returns it."
          },
          "id": "run",
          "open": [
            [
              "main.ts#run/call:load",
              "main.ts#load/return[0]/call:helper"
            ],
            [
              "main.ts#run/call:load"
            ]
          ],
          "ref": {
            "block": "main.ts#run"
          },
          "view": "compact"
        }
      ],
      "combined": [
        {
          "author": "agent:test",
          "collapsed": true,
          "id": "a-check",
          "members": [
            "main.ts#run/if[0]"
          ],
          "signature": "",
          "title": "Steps"
        },
        {
          "author": "agent:test",
          "collapsed": true,
          "description": {
            "author": "agent:test",
            "signature": "",
            "text": "Log and hand it back"
          },
          "id": "steps",
          "members": [
            "main.ts#run/call:console.log",
            "main.ts#run/return[0]"
          ],
          "signature": "",
          "title": "Steps"
        }
      ],
      "defaultView": "expanded",
      "frames": [
        {
          "author": "agent:test",
          "collapsed": true,
          "direction": "down",
          "icon": "api",
          "id": "server",
          "look": "box",
          "members": [
            "run",
            "a-entrance"
          ],
          "title": "Server"
        }
      ],
      "guards": [],
      "id": "page-1",
      "links": [
        {
          "author": "person:tester",
          "from": "run",
          "id": "note",
          "kind": "other",
          "label": {
            "author": "person:tester",
            "signature": "",
            "text": "Quote \" and\nnewline"
          },
          "step": "main.ts#run/call:console.log",
          "to": "customer"
        },
        {
          "anchors": [
            {
              "key": "entrance:shop/run-it"
            }
          ],
          "author": "agent:test",
          "from": "customer",
          "id": "posts",
          "kind": "http",
          "to": "a-entrance"
        }
      ],
      "names": [
        {
          "author": "agent:test",
          "key": "main.ts#run/call:load",
          "signature": "",
          "text": "Load the value"
        },
        {
          "author": "agent:test",
          "key": "main.ts#run/if[0]",
          "signature": "",
          "text": "Is it there?"
        }
      ],
      "notes": [
        {
          "at": [
            0,
            -40
          ],
          "attach": "posts",
          "author": "agent:test",
          "id": "hint",
          "text": "Read the request first."
        }
      ],
      "proposals": [
        {
          "after": "main.ts#run/call:console.log",
          "author": "agent:test",
          "id": "p-after",
          "reason": "They asked for a receipt",
          "title": "Email the customer"
        },
        {
          "author": "agent:test",
          "id": "p-start",
          "start": "main.ts#run/if[0]/then",
          "title": "Email the customer"
        }
      ],
      "resources": [
        {
          "anchors": [],
          "at": [
            -300,
            0
          ],
          "author": "agent:test",
          "id": "customer",
          "kind": "actor",
          "name": "Customer"
        }
      ],
      "title": "Placing an order"
    },
    {
      "cards": [],
      "combined": [],
      "defaultView": "compact",
      "guards": [],
      "id": "a-second",
      "links": [],
      "names": [],
      "title": "Second"
    }
  ],
  "title": "Orders",
  "version": 1
}
"##;

/// A board with every kind of item, unsigned, in no particular order.
fn full_board() -> Board {
    let mut page = Page::new("page-1", "Placing an order");
    page.default_view = View::Expanded;
    let mut run = block_card("run", RUN);
    run.open = vec![vec![key(LOAD_CALL), key(HELPER_CALL)], vec![key(LOAD_CALL)]];
    run.view = Some(View::Compact);
    run.description = Some(unsigned("Loads a value, logs it, and returns it."));
    let mut entrance = start_card("a-entrance", RUN_IT);
    entrance.at = [-0.5, 1e21];
    page.cards = vec![run, entrance];
    page.names = vec![name(IF0, "Is it there?"), name(LOAD_CALL, "Load the value")];
    let mut steps = combined("steps", &[LOG, RETURN0]);
    steps.description = Some(unsigned("Log and hand it back"));
    page.combined = vec![steps, combined("a-check", &[IF0])];
    let mut post = Connection::new("posts", ConnectionKind::Http, "customer", "a-entrance", agent());
    post.anchors = vec![Anchor::Key { key: key(RUN_IT) }];
    let mut note = Connection::new("note", ConnectionKind::Other, "run", "customer", person());
    note.label = Some(Description {
        text: "Quote \" and\nnewline".to_owned(),
        author: person(),
        signature: None,
    });
    note.step = Some(key(LOG));
    page.links = vec![post, note];
    page.resources = vec![Resource {
        id: "customer".to_owned(),
        kind: ResourceKind::Actor,
        name: "Customer".to_owned(),
        type_key: None,
        columns: Vec::new(),
        anchors: Vec::new(),
        at: [-300.0, 0.0],
        description: None,
        author: agent(),
        signature: None,
    }];
    page.frames = vec![Frame {
        id: "server".to_owned(),
        title: "Server".to_owned(),
        description: None,
        members: vec!["run".to_owned(), "a-entrance".to_owned()],
        look: FrameLook::Box,
        icon: Some(FrameIcon::Api),
        direction: Some(FrameDirection::Down),
        collapsed: true,
        author: agent(),
    }];
    page.notes = vec![Note {
        id: "hint".to_owned(),
        text: "Read the request first.".to_owned(),
        at: [0.0, -40.0],
        attach: Some("posts".to_owned()),
        author: agent(),
    }];
    page.proposals = vec![
        proposal("p-after", after(LOG)),
        Proposal {
            reason: None,
            ..proposal("p-start", start(THEN))
        },
    ];
    // In canonical order, as a parsed file holds them.
    page.cards.sort_by(|a, b| a.id.cmp(&b.id));
    page.names.sort_by(|a, b| a.key.cmp(&b.key));
    page.combined.sort_by(|a, b| a.id.cmp(&b.id));
    page.links.sort_by(|a, b| a.id.cmp(&b.id));
    Board::with_pages(
        "Orders",
        Some("Checkout".to_owned()),
        vec![page, Page::new("a-second", "Second")],
    )
}

#[test]
fn serialization_is_canonical_and_round_trips() {
    let board = full_board();
    let text = serialize(&board);
    let parsed = parse(&text).unwrap();
    assert_eq!(parsed, board);
    assert_eq!(serialize(&parsed), text);
    assert_eq!(text, FULL_BOARD);

    // An agent writing the same board with JSON.stringify and sorted keys and arrays writes the
    // same bytes; any other order reads back to the same canonical text.
    let mut shuffled = board.clone();
    shuffled.pages[0].cards.reverse();
    shuffled.pages[0].names.reverse();
    shuffled.pages[0].combined.reverse();
    shuffled.pages[0].links.reverse();
    assert_eq!(serialize(&shuffled), text);

    // The UI's edit (its serde form) reads back as the same pages.
    let edit = serde_json::json!({ "title": board.title, "pages": board.pages }).to_string();
    assert_eq!(parse_edit(&edit, board.folder.clone()).unwrap(), board);
    assert!(matches!(
        parse_edit(
            &edit.replace(r#""title":"Orders""#, r#""folder":"x","title":"Orders""#),
            None
        ),
        Err(BoardError::Invalid { .. })
    ));
}

#[test]
fn optional_items_are_written_only_when_present() {
    let text = serialize(&parse(MINIMAL).unwrap());
    for absent in [
        "frames",
        "resources",
        "notes",
        "proposals",
        "open",
        "view",
        "folder",
    ] {
        assert!(!text.contains(&format!("\"{absent}\"")), "{absent} in {text}");
    }
    for present in ["names", "combined", "guards", "links"] {
        assert!(
            text.contains(&format!("\"{present}\": []")),
            "{present} in {text}"
        );
    }
}

#[test]
fn descriptions_names_and_combined_steps_are_signed_from_verified_source() {
    let (_dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.cards[0].description = Some(unsigned("Runs it"));
        let mut entrance = start_card("run-it", RUN_IT);
        entrance.description = Some(unsigned("Where a run starts"));
        page.cards.push(entrance);
        page.names.push(name(LOAD_CALL, "Load the value"));
        let mut steps = combined("steps", &[LOAD_CALL, IF0]);
        steps.description = Some(unsigned("Load and check"));
        page.combined.push(steps);
    });
    let (saved, health) = save(board, None, &graph).unwrap();
    assert!(health.is_healthy(), "{health:#?}");
    let page = &saved.pages[0];
    let run = Signature::of(run_source().as_bytes());
    assert_eq!(page.cards[0].description.as_ref().unwrap().signature, Some(run));
    // An entrance card's description is signed over the source of its code.
    let code = Signature::of(format!("{}\n", run_source()).as_bytes());
    assert_eq!(page.cards[1].description.as_ref().unwrap().signature, Some(code));
    assert_eq!(
        page.names[0].name.signature,
        Some(Signature::of(bytes_of("load()")))
    );
    // A combined step is signed over its drawn span: from its first block's statement.
    let span = Signature::of(bytes_of(
        "const first = load();\n  if (first) { console.log(first); }",
    ));
    assert_eq!(page.combined[0].signature, Some(span));
    assert_eq!(
        page.combined[0].description.as_ref().unwrap().signature,
        Some(span)
    );

    // Guards record every guarded reference: a block's kind, owner, and source signature, and
    // an entrance's kind and its `code` keys.
    let guard = |key: &str| page.guard(key).unwrap();
    assert_eq!(
        (guard(RUN).kind.as_str(), guard(RUN).owner.as_str()),
        ("function", MODULE)
    );
    assert_eq!(guard(RUN).signature, run);
    assert_eq!(
        (guard(IF0).kind.as_str(), guard(IF0).owner.as_str()),
        ("decision", RUN)
    );
    assert_eq!(
        (guard(RUN_IT).kind.as_str(), guard(RUN_IT).owner.as_str()),
        ("entrance", "")
    );
    assert_eq!(
        guard(RUN_IT).signature,
        Signature::of(format!("{RUN}\n").as_bytes())
    );
    assert_eq!(page.guards.len(), 4);
}

/// `SOURCE`'s board graph (in `dir`) with one more app, `Ops`: only a heading, with the entrance
/// `Refund` whose code is `code`.
fn with_ops(dir: &Path, code: Vec<unfold_core::EntranceTarget>) -> BoardGraph {
    let mut graph = graph_of(SOURCE);
    graph.apps.push(unfold_core::App {
        key: key("entrance:ops"),
        name: "Ops".into(),
        run: None,
        code: None,
        about: None,
        entrances: vec![unfold_core::Entrance {
            key: key("entrance:ops/refund"),
            name: "Refund".into(),
            on: None,
            about: None,
            code,
        }],
    });
    BoardGraph::from_graph(&graph, dir)
}

#[test]
fn a_description_of_an_app_or_entrance_whose_code_is_not_found_is_signed_over_what_there_is() {
    let (dir, _) = folder(SOURCE);
    let graph = with_ops(
        dir.path(),
        vec![
            target("main.ts#refund", TargetState::Missing),
            target("legacy.ts#refund", TargetState::NotAnalyzed),
        ],
    );
    let mut board = Board::new("Ops");
    for (id, start, text) in [
        ("ops", "entrance:ops", "Run by hand."),
        ("refund", "entrance:ops/refund", "Refunds an order."),
    ] {
        let mut card = start_card(id, start);
        card.description = Some(unsigned(text));
        board.pages[0].cards.push(card);
    }
    let (saved, health) = save(board, None, &graph).expect("a heading or missing code never blocks a save");
    assert!(health.is_healthy(), "{health:#?}");
    let nothing = Some(Signature::of(b""));
    for card in &saved.pages[0].cards {
        assert_eq!(
            card.description.as_ref().unwrap().signature,
            nothing,
            "{}",
            card.id
        );
    }

    // Once the entrance's code is found, its description goes to review; the heading's stays.
    let found = with_ops(dir.path(), vec![target(RUN, TargetState::Found)]);
    assert_eq!(
        items(&board_health(&saved, &found)),
        [(
            HealthKind::Outdated,
            Target::Card("refund".to_owned()),
            None,
            &[RepairKind::Resign, RepairKind::RemoveDescription][..]
        )]
    );
}

#[test]
fn source_that_does_not_verify_is_never_signed() {
    let (dir, graph) = folder(SOURCE);
    std::fs::write(dir.path().join("main.ts"), SOURCE.replace("done", "DONE")).unwrap();
    let stale = BoardGraph::from_graph(&graph_of(SOURCE), dir.path());
    let board = run_board_with(|page| page.names.push(name(LOG, "Log it")));
    assert_eq!(
        save(board.clone(), None, &stale).unwrap_err(),
        BoardError::Unverified { what: LOG.to_owned() }
    );
    assert!(save(board, None, &graph).is_ok());
}

#[test]
fn saving_restores_dropped_guards_drops_unused_ones_and_reuses_unchanged_pages() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = run_board_with(|page| page.names.push(name(IF0, "Is it there?")));
    board.pages.push(Page::new("page-2", "Second"));
    let (saved, health) = save(board, None, &graph).unwrap();
    let mut edited = saved.clone();
    edited.pages[0].guards = vec![GuardEntry {
        key: key("main.ts#unused"),
        kind: "call".to_owned(),
        owner: String::new(),
        signature: Signature::of(b"x"),
    }];
    edited.pages[0].title = "Retitled".to_owned();
    let (again, _) = save(edited, Some((&saved, &health)), &graph).unwrap();
    assert_eq!(again.pages[0].guards, saved.pages[0].guards);
    assert_eq!(
        serialize(&again),
        serialize(&saved).replace("\"Page 1\"", "\"Retitled\"")
    );

    // An unchanged page is not checked again: its health (even a stale one) is kept as is.
    let mut stale = health;
    stale.append(Health {
        items: vec![HealthItem {
            page: "page-1".to_owned(),
            kind: HealthKind::Outdated,
            target: Target::Card("run".to_owned()),
            key: None,
            anchor: None,
            reconnect: None,
            message: "kept".to_owned(),
            repairs: Vec::new(),
        }],
        ..Health::default()
    });
    let mut edited = saved.clone();
    edited.pages[1].cards.push(block_card("again", RUN));
    let (again, again_health) = save(edited, Some((&saved, &stale)), &graph).unwrap();
    assert_eq!(again_health.label().as_deref(), Some("1 outdated"));
    assert!(again.pages[1].guard(RUN).is_some(), "page-2 was checked");
}

#[test]
fn combined_steps_follow_the_overlay_rules_on_save() {
    let (_dir, graph) = folder(SOURCE);
    let with = |steps: Vec<Combined>| run_board_with(|page| page.combined = steps);
    // Consecutive steps of one body, or one whole control block.
    for members in [
        vec![LOAD_CALL, IF0],
        vec![IF0, LOG, RETURN0],
        vec![IF0],
        vec![INNER_LOG],
    ] {
        let (_, health) = save(with(vec![combined("s", &members)]), None, &graph).unwrap();
        assert!(health.is_healthy(), "{members:?}: {health:#?}");
    }
    // Two combined steps side by side, page-wide.
    let (_, health) = save(
        with(vec![
            combined("a", &[LOAD_CALL]),
            combined("b", &[LOG, RETURN0]),
            combined("c", &["main.ts#load/return[0]"]),
        ]),
        None,
        &graph,
    )
    .unwrap();
    assert!(health.is_healthy());

    let refused = |steps: Vec<Combined>, expected: &str| {
        let (target, message) = refusal(with(steps), &graph);
        assert!(matches!(target, Target::Combined(_)), "{target:?}");
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    };
    refused(vec![combined("s", &[LOAD_CALL, LOG])], "doesn't come right after");
    refused(vec![combined("s", &[LOG, IF0])], "doesn't come right after");
    refused(vec![combined("s", &[INNER_LOG, LOG])], "is not in the body of");
    refused(vec![combined("s", &[THEN])], "can't be combined");
    refused(vec![combined("s", &[MODULE])], "can't be combined");
    refused(
        vec![combined("s", &["main.ts#run/call:gone"])],
        "is not a block in the graph",
    );
    // Never nested: a block inside another combined step's block.
    refused(
        vec![combined("outer", &[IF0]), combined("inner", &[INNER_LOG])],
        "never nest",
    );
}

#[test]
fn proposals_go_after_a_block_or_at_the_start_of_a_body() {
    let (_dir, graph) = folder(SOURCE);
    let with = |place: Place| run_board_with(|page| page.proposals.push(proposal("p", place)));
    for place in [
        after(LOAD_CALL),
        after(IF0),
        after(INNER_LOG),
        after(RETURN0),
        after(RUN),
        start(RUN),
        start(THEN),
        start(MODULE),
    ] {
        let (saved, health) = save(with(place.clone()), None, &graph).unwrap();
        assert!(health.is_healthy(), "{place:?}");
        assert!(
            saved.pages[0].guard(place.key().as_str()).is_some(),
            "its place is guarded"
        );
    }
    for (place, expected) in [
        (after(THEN), "can't go after"),
        (start(LOAD_CALL), "can't go at the start of"),
        (start(IF0), "can't go at the start of"),
    ] {
        let (target, message) = refusal(with(place), &graph);
        assert_eq!(target, Target::Proposal("p".to_owned()));
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    }
    let (_, message) = refusal(with(after("main.ts#run/call:gone")), &graph);
    assert!(message.contains("is missing"), "{message}");

    // A proposal is never signed.
    let (saved, health) = save(with(after(LOG)), None, &graph).unwrap();
    let mut board = saved.clone();
    assert!(matches!(
        repair(
            &mut board,
            &graph,
            &Repair::Resign(Target::Proposal("p".to_owned()))
        ),
        Err(BoardError::Problem { .. })
    ));
    // Moving it is a new placement, checked again; one placed as before isn't.
    let mut moved = saved.clone();
    moved.pages[0].proposals[0].place = after(THEN);
    assert!(save(moved, Some((&saved, &health)), &graph).is_err());
}

#[test]
fn cards_show_definitions_and_open_project_calls_and_callbacks() {
    let (_dir, graph) = folder(SOURCE);
    let with = |card: Card| {
        let mut board = Board::new("B");
        board.pages[0].cards.push(card);
        board
    };
    let opening = |paths: &[&[&str]]| Card {
        open: paths
            .iter()
            .map(|path| path.iter().map(|step| key(step)).collect())
            .collect(),
        ..block_card("run", RUN)
    };
    for card in [
        block_card("m", MODULE),
        block_card("h", HELPER),
        start_card("e", RUN_IT),
        start_card("a", "entrance:shop"),
        opening(&[&[LOAD_CALL], &[LOAD_CALL, HELPER_CALL]]),
    ] {
        let (_, health) = save(with(card.clone()), None, &graph).unwrap();
        assert!(health.is_healthy(), "{card:?}");
    }
    let refused = |card: Card, expected: &str| {
        let (target, message) = refusal(with(card), &graph);
        assert_eq!(target, Target::Card("run".to_owned()));
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    };
    refused(
        block_card("run", LOAD_CALL),
        "is a call: a card shows a module, a class, a function, or a type",
    );
    refused(opening(&[&[LOG]]), "can't open in place");
    refused(opening(&[&[HELPER_CALL]]), "is not drawn in `main.ts#run`");
    refused(
        opening(&[&[LOAD_CALL, HELPER_CALL, FETCH]]),
        "`main.ts#helper/return[0]/call:dep.fetch` can't open in place",
    );
    refused(opening(&[&["main.ts#run/call:gone"]]), "is not a block");

    // Open paths are never guarded.
    let (saved, _) = save(with(opening(&[&[LOAD_CALL]])), None, &graph).unwrap();
    assert!(saved.pages[0].guard(LOAD_CALL).is_none());
}

#[test]
fn health_reports_outdated_text_after_the_code_changes() {
    let (dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.cards[0].description = Some(unsigned("Runs it"));
        page.names.push(name(LOG, "Say it's done"));
        page.names.push(name(LOAD_CALL, "Load the value"));
        let mut steps = combined("steps", &[IF0, LOG]);
        steps.description = Some(unsigned("Log it"));
        page.combined.push(steps);
    });
    let (saved, health) = save(board, None, &graph).unwrap();
    assert!(health.is_healthy());

    let edited = recompiled(dir.path(), &SOURCE.replace("'done'", "'finished'"));
    let health = board_health(&saved, &edited);
    use RepairKind::{Remove, RemoveDescription, Resign};
    assert_eq!(
        items(&health),
        [
            (
                HealthKind::Outdated,
                Target::Card("run".to_owned()),
                None,
                &[Resign, RemoveDescription][..]
            ),
            (
                HealthKind::Outdated,
                Target::Name(LOG.to_owned()),
                None,
                &[Resign, Remove][..]
            ),
            (
                HealthKind::Outdated,
                Target::Combined("steps".to_owned()),
                None,
                &[Resign, RemoveDescription, Remove][..]
            ),
        ]
    );
    assert_eq!(health.label().as_deref(), Some("3 outdated"));

    // Outdated text never refuses a save, and resigning keeps the words.
    let mut board = saved.clone();
    let (_, kept) = save(board.clone(), None, &edited).unwrap();
    assert_eq!(kept.outdated, 3);
    for target in [
        Target::Card("run".to_owned()),
        Target::Name(LOG.to_owned()),
        Target::Combined("steps".to_owned()),
    ] {
        repair(&mut board, &edited, &Repair::Resign(target)).unwrap();
    }
    assert!(board_health(&board, &edited).is_healthy());
    assert_eq!(board.pages[0].names, {
        let mut names = saved.pages[0].names.clone();
        names[0].name.signature = edited.signature(LOG);
        names
    });

    // Or the description goes.
    let mut board = saved;
    repair(
        &mut board,
        &edited,
        &Repair::RemoveDescription(Target::Card("run".to_owned())),
    )
    .unwrap();
    assert!(board.pages[0].cards[0].description.is_none());
}

const INSERTED_IF: &str = "  if (ready) { console.log(ready); }\n";

#[test]
fn position_based_references_whose_code_changed_are_held_for_review() {
    let (dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.names.push(name(IF0, "Is it there?"));
        page.names.push(name(LOG, "Say it's done"));
        page.proposals.push(proposal("p", after(IF0)));
        page.combined.push(combined("steps", &[LOAD_CALL, IF0]));
    });
    let (saved, _) = save(board, None, &graph).unwrap();

    // A new `if` before the old one takes its key, `if[0]`.
    let text = SOURCE.replace("  if (first)", &format!("{INSERTED_IF}  if (first)"));
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    use RepairKind::{Accept, Reconnect, Remove};
    let held = &[Accept, Reconnect, Remove][..];
    assert_eq!(
        items(&health),
        [
            (HealthKind::Review, Target::Name(IF0.to_owned()), None, held),
            (
                HealthKind::Review,
                Target::Combined("steps".to_owned()),
                Some(IF0),
                held
            ),
            (
                HealthKind::Outdated,
                Target::Combined("steps".to_owned()),
                None,
                &[RepairKind::Resign, Remove][..]
            ),
            (
                HealthKind::Review,
                Target::Proposal("p".to_owned()),
                Some(IF0),
                held
            ),
        ],
        "a key without a position (the second log) follows its code"
    );
    assert_eq!(health.review, 3);

    // Text can't be signed against a held reference; accepting the current code releases it.
    let mut board = saved.clone();
    assert!(repair(&mut board, &edited, &Repair::Resign(Target::Name(IF0.to_owned()))).is_err());
    repair(&mut board, &edited, &Repair::Accept(IF0.to_owned())).unwrap();
    repair(
        &mut board,
        &edited,
        &Repair::Resign(Target::Combined("steps".to_owned())),
    )
    .unwrap();
    let health = board_health(&board, &edited);
    assert_eq!(
        items(&health),
        [(
            HealthKind::Outdated,
            Target::Name(IF0.to_owned()),
            None,
            &[RepairKind::Resign, Remove][..]
        )]
    );

    // A guarded reference whose kind or owner changed is held too.
    let mut board = saved;
    board.pages[0]
        .guards
        .iter_mut()
        .find(|guard| guard.key.as_str() == LOG)
        .unwrap()
        .kind = "exit".to_owned();
    let health = board_health(&board, &graph);
    assert!(health.has("page-1", HealthKind::Review, &Target::Name(LOG.to_owned())));
    assert_eq!(
        health.items[0].message,
        "Saved reference kind or owner changed; needs review"
    );
}

/// A call written between two blocks of a combined step joins the step, which goes to review:
/// Resign signs it again over the code now, with the call among its blocks.
#[test]
fn a_call_written_inside_a_combined_step_joins_it_and_sends_it_to_review() {
    const RECORD: &str = "main.ts#run/call:record";
    let (dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        let mut pair = combined("pair", &[IF0, LOG]);
        pair.description = Some(unsigned("Log what was found"));
        page.combined.push(pair);
    });
    let (saved, _) = save(board, None, &graph).unwrap();

    let text = SOURCE.replace(
        "  console.log('done');",
        "  record('placing');\n  console.log('done');",
    );
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    use RepairKind::{Remove, RemoveDescription, Resign};
    assert_eq!(
        items(&health),
        [(
            HealthKind::Outdated,
            Target::Combined("pair".to_owned()),
            None,
            &[Resign, RemoveDescription, Remove][..]
        )]
    );
    assert_eq!(
        health.items[0].message,
        "Code changed since these steps were combined"
    );
    assert_eq!(health.label().as_deref(), Some("1 outdated"));
    // Review never refuses a save.
    let (kept, _) = save(saved.clone(), None, &edited).unwrap();
    assert_eq!(kept.pages[0].combined, saved.pages[0].combined);

    let mut board = saved;
    repair(
        &mut board,
        &edited,
        &Repair::Resign(Target::Combined("pair".to_owned())),
    )
    .unwrap();
    let pair = &board.pages[0].combined[0];
    assert_eq!(pair.members, [key(IF0), key(RECORD), key(LOG)]);
    let span = edited.span_signature(IF0, LOG);
    assert!(span.is_some());
    assert_eq!(pair.signature, span);
    assert_eq!(pair.description.as_ref().unwrap().signature, span);
    assert!(board_health(&board, &edited).is_healthy());
    // The call that joined it is guarded like the rest once the board saves.
    let (resaved, health) = save(board, None, &edited).unwrap();
    assert!(health.is_healthy(), "{:#?}", health.items);
    assert_eq!(
        resaved.pages[0].guard(RECORD),
        current_guard(&edited, RECORD).as_ref()
    );
}

/// A combined step breaks only when its blocks stop following the rules: a block is gone, they
/// are no longer in written order, or a block written between them is another combined step's.
#[test]
fn combined_steps_break_when_their_blocks_stop_following_the_rules() {
    let (dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.combined.push(combined("pair", &[IF0, LOG]));
        page.combined.push(combined("tail", &[RETURN0]));
    });
    let (saved, _) = save(board, None, &graph).unwrap();
    let broken = |health: &Health, id: &str| {
        health
            .items
            .iter()
            .find(|item| {
                item.kind == HealthKind::BrokenCombined && item.target == Target::Combined(id.to_owned())
            })
            .cloned()
            .unwrap_or_else(|| panic!("`{id}` is broken: {:#?}", health.items))
    };

    // Its two blocks, written the other way round.
    let text = SOURCE.replace(
        "  if (first) { console.log(first); }\n  console.log('done');\n",
        "  console.log('done');\n  if (first) { console.log(first); }\n",
    );
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    let item = broken(&health, "pair");
    assert!(
        item.message.contains("doesn't come right after"),
        "{}",
        item.message
    );
    assert_eq!(item.repairs, [RepairKind::Remove]);
    assert_eq!(health.label().as_deref(), Some("1 broken combined step"));
    // Resign can't sign it; removing it keeps its blocks.
    let mut board = saved.clone();
    assert!(matches!(
        repair(
            &mut board,
            &edited,
            &Repair::Resign(Target::Combined("pair".to_owned()))
        ),
        Err(BoardError::Problem { .. })
    ));
    assert_eq!(board, saved, "a refused repair changes nothing");
    repair(
        &mut board,
        &edited,
        &Repair::Remove(Target::Combined("pair".to_owned())),
    )
    .unwrap();
    assert_eq!(board.pages[0].combined.len(), 1);
    assert!(board_health(&board, &edited).is_healthy());

    // A block that is gone breaks it too, and Reconnect is offered for that block.
    let text = SOURCE.replace(
        "  return first;\n}\nexport function load",
        "  console.log(first);\n}\nexport function load",
    );
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    let item = broken(&health, "tail");
    assert_eq!(
        (item.key.as_deref(), item.repairs.as_slice()),
        (Some(RETURN0), &[RepairKind::Reconnect, RepairKind::Remove][..])
    );

    // Another combined step's block moved between two of its blocks: the two overlap.
    let board = run_board_with(|page| {
        page.combined.push(combined("head", &[LOAD_CALL, IF0]));
        page.combined.push(combined("done", &[LOG]));
    });
    let (saved, _) = save(board, None, &graph).unwrap();
    let text = SOURCE.replace(
        "  if (first) { console.log(first); }\n  console.log('done');\n",
        "  console.log('done');\n  if (first) { console.log(first); }\n",
    );
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    for id in ["head", "done"] {
        let item = broken(&health, id);
        assert!(
            item.message.contains("is in two combined steps"),
            "{}",
            item.message
        );
        assert_eq!(item.repairs, [RepairKind::Remove]);
    }
}

#[test]
fn reconnecting_a_renamed_function_moves_what_is_still_inside_it() {
    const RUN_ALL: &str = "main.ts#runAll";
    const RUN_HELPER: &str = "main.ts#run/call:helper";
    let original = SOURCE.replace(
        "  console.log('done');\n",
        "  console.log('done');\n  helper();\n",
    );
    let (dir, graph) = folder(&original);
    let board = run_board_with(|page| {
        page.cards[0].open = vec![vec![key(LOAD_CALL), key(HELPER_CALL)], vec![key(RUN_HELPER)]];
        page.cards.push(block_card("load", LOAD));
        page.names.push(name(IF0, "Is it there?"));
        page.names.push(name(INNER_LOG, "Say what it found"));
        page.combined.push(combined("head", &[LOAD_CALL, IF0]));
        page.combined.push(combined("tail", &[LOG, RUN_HELPER, RETURN0]));
        page.proposals.push(proposal("p", after(LOAD_CALL)));
        page.proposals.push(proposal("q", after(LOG)));
        page.links.push(Connection {
            step: Some(key(LOAD_CALL)),
            label: Some(unsigned("asks for it")),
            ..Connection::new("asks", ConnectionKind::Other, "run", "load", person())
        });
        page.links.push(Connection {
            step: Some(key(RUN_HELPER)),
            label: Some(unsigned("helps")),
            ..Connection::new("helps", ConnectionKind::Other, "run", "load", person())
        });
    });
    let (saved, _) = save(board, None, &graph).unwrap();

    // `run` is renamed; the log inside its `if`, the log after it, and its `helper()` are gone.
    let text = original
        .replace("function run()", "function runAll()")
        .replace("{ console.log(first); }", "{ helper(first); }")
        .replace("  console.log('done');\n  helper();\n", "");
    let edited = recompiled(dir.path(), &text);
    let before = board_health(&saved, &edited);
    let mut board = saved.clone();
    repair(
        &mut board,
        &edited,
        &Repair::Reconnect {
            from: RUN.to_owned(),
            to: RUN_ALL.to_owned(),
        },
    )
    .unwrap();
    let moved = |old: &str| key(&format!("{RUN_ALL}{}", &old[RUN.len()..]));
    let page = &board.pages[0];
    assert_eq!(page.cards[0].reference, CardRef::block(key(RUN_ALL)));
    assert_eq!(
        page.cards[0].open,
        [vec![moved(LOAD_CALL), key(HELPER_CALL)], vec![key(RUN_HELPER)]]
    );
    assert_eq!(
        page.cards[1].reference,
        CardRef::block(key(LOAD)),
        "outside `run`"
    );
    let names: Vec<&Key> = page.names.iter().map(|entry| &entry.key).collect();
    assert_eq!(names, [&moved(IF0), &key(INNER_LOG)]);
    // A combined step moves whole or not at all.
    assert_eq!(page.combined[0].members, [moved(LOAD_CALL), moved(IF0)]);
    assert_eq!(
        page.combined[1].members,
        [key(LOG), key(RUN_HELPER), key(RETURN0)]
    );
    assert_eq!(page.proposals[0].place, after(moved(LOAD_CALL).as_str()));
    assert_eq!(page.proposals[1].place, after(LOG));
    assert_eq!(page.links[0].step, Some(moved(LOAD_CALL)));
    assert_eq!(page.links[1].step, Some(key(RUN_HELPER)));

    // Moved keys have fresh guards; the guards of what stayed stay with it.
    for moved in [moved(IF0), moved(LOAD_CALL)] {
        assert_eq!(
            page.guard(moved.as_str()),
            current_guard(&edited, moved.as_str()).as_ref()
        );
    }
    assert!(page.guard(IF0).is_none());
    assert_eq!(page.guard(INNER_LOG), saved.pages[0].guard(INNER_LOG));

    // What stayed is missing at its old key, as before the repair, so the save goes through.
    let health = board_health(&board, &edited);
    let missing = |health: &Health| -> Vec<(HealthKind, Target)> {
        health
            .items
            .iter()
            .filter(|item| item.kind != HealthKind::Outdated)
            .map(|item| (item.kind, item.target.clone()))
            .collect()
    };
    assert_eq!(
        missing(&health),
        [
            (HealthKind::Missing, Target::Name(INNER_LOG.to_owned())),
            (HealthKind::BrokenCombined, Target::Combined("tail".to_owned())),
            (HealthKind::BrokenLink, Target::Connection("helps".to_owned())),
            (HealthKind::Missing, Target::Proposal("q".to_owned())),
        ]
    );
    assert!(
        missing(&health)
            .iter()
            .all(|item| missing(&before).contains(item))
    );
    assert!(save(board, Some((&saved, &before)), &edited).is_ok());
}

#[test]
fn missing_references_wait_for_reconnect_or_removal() {
    let (dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.cards[0].description = Some(unsigned("Runs it"));
        page.cards.push(block_card("load", LOAD));
        page.names.push(name(LOG, "Say it's done"));
        page.combined.push(combined("tail", &[LOG, RETURN0]));
        page.proposals.push(proposal("p", start(RUN)));
        page.links.push(Connection {
            label: Some(unsigned("feeds")),
            ..Connection::new("feeds", ConnectionKind::Other, "load", "run", person())
        });
    });
    let (saved, _) = save(board, None, &graph).unwrap();

    let text = SOURCE.replace("function run()", "function runAll()");
    let edited = recompiled(dir.path(), &text);
    let health = board_health(&saved, &edited);
    use RepairKind::{Reconnect, Remove};
    assert_eq!(
        items(&health),
        [
            (
                HealthKind::Missing,
                Target::Card("run".to_owned()),
                None,
                &[Reconnect, Remove][..]
            ),
            (
                HealthKind::Missing,
                Target::Name(LOG.to_owned()),
                None,
                &[Reconnect, Remove][..]
            ),
            (
                HealthKind::BrokenCombined,
                Target::Combined("tail".to_owned()),
                Some(LOG),
                &[Reconnect, Remove][..]
            ),
            (
                HealthKind::BrokenLink,
                Target::Connection("feeds".to_owned()),
                None,
                &[Remove][..]
            ),
            (
                HealthKind::Missing,
                Target::Proposal("p".to_owned()),
                Some(RUN),
                &[Reconnect, Remove][..]
            ),
        ]
    );
    assert_eq!(health.items[0].reconnect, None, "a block offers no key");
    assert_eq!(
        health.label().as_deref(),
        Some("3 missing · 1 broken combined step · 1 broken link")
    );

    // A page with a missing reference can still be edited and saved; a new one can't.
    let mut retitled = saved.clone();
    retitled.pages[0].title = "Retitled".to_owned();
    assert!(save(retitled, Some((&saved, &health)), &edited).is_ok());
    let mut more = saved.clone();
    more.pages[0].names.push(name(IF0, "Is it there?"));
    assert!(matches!(
        save(more, Some((&saved, &health)), &edited),
        Err(BoardError::Problem { .. })
    ));

    // Reconnect moves the key, with a fresh guard, and everything keyed inside it.
    const RUN_ALL: &str = "main.ts#runAll";
    let mut board = saved.clone();
    repair(
        &mut board,
        &edited,
        &Repair::Reconnect {
            from: RUN.to_owned(),
            to: RUN_ALL.to_owned(),
        },
    )
    .unwrap();
    let page = &board.pages[0];
    assert_eq!(page.cards[0].reference, CardRef::block(key(RUN_ALL)));
    assert_eq!(page.proposals[0].place, start(RUN_ALL));
    assert!(page.guard(RUN).is_none());
    assert_eq!(page.guard(RUN_ALL), current_guard(&edited, RUN_ALL).as_ref());
    let health = board_health(&board, &edited);
    assert_eq!(
        items(&health),
        [(
            HealthKind::Outdated,
            Target::Card("run".to_owned()),
            None,
            &[RepairKind::Resign, RepairKind::RemoveDescription][..]
        )]
    );

    // Reconnecting to a missing key, or a block card to an entrance, is refused.
    for to in ["main.ts#absent", RUN_IT] {
        let mut board = saved.clone();
        assert!(matches!(
            repair(
                &mut board,
                &edited,
                &Repair::Reconnect {
                    from: RUN.to_owned(),
                    to: to.to_owned()
                }
            ),
            Err(BoardError::Problem { .. })
        ));
    }

    // Removing the card takes its connections; its name, combined step, and proposal stay,
    // since they apply page-wide.
    let mut board = saved;
    repair(
        &mut board,
        &edited,
        &Repair::Remove(Target::Card("run".to_owned())),
    )
    .unwrap();
    let page = &board.pages[0];
    assert!(page.links.is_empty());
    assert_eq!(
        (page.names.len(), page.combined.len(), page.proposals.len()),
        (1, 1, 1)
    );
    for target in [
        Target::Name(LOG.to_owned()),
        Target::Combined("tail".to_owned()),
        Target::Proposal("p".to_owned()),
    ] {
        repair(&mut board, &edited, &Repair::Remove(target)).unwrap();
    }
    assert!(board_health(&board, &edited).is_healthy());
}

#[test]
fn a_missing_entrance_offers_the_one_with_the_same_code() {
    let (_dir, graph) = folder(SOURCE);
    let board = run_board_with(|page| {
        page.cards.push(start_card("run-it", RUN_IT));
        page.cards.push(start_card("load-it", LOAD_IT));
    });
    let (mut saved, _) = save(board, None, &graph).unwrap();
    // The entrances were renamed in unfold.toml: the board still holds the old keys, and their
    // guards record the old entrances' code.
    let page = &mut saved.pages[0];
    for (card, old) in [(1, "entrance:shop/run"), (2, "entrance:shop/load")] {
        let current = page.cards[card].reference.key().clone();
        page.cards[card].reference = CardRef::entrance(key(old));
        page.guards
            .iter_mut()
            .find(|guard| guard.key == current)
            .unwrap()
            .key = key(old);
    }
    let health = board_health(&saved, &graph);
    let missing: Vec<(&Target, Option<&str>)> = health
        .items
        .iter()
        .map(|item| (&item.target, item.reconnect.as_deref()))
        .collect();
    assert_eq!(
        missing,
        [
            (&Target::Card("run-it".to_owned()), Some(RUN_IT)),
            (&Target::Card("load-it".to_owned()), None),
        ],
        "two entrances have `load` as their code, so neither is offered"
    );
    let mut board = saved;
    repair(
        &mut board,
        &graph,
        &Repair::Reconnect {
            from: "entrance:shop/run".to_owned(),
            to: RUN_IT.to_owned(),
        },
    )
    .unwrap();
    assert_eq!(board.pages[0].cards[1].reference, CardRef::entrance(key(RUN_IT)));
    // An entrance card reconnects only to an app or entrance.
    assert!(
        repair(
            &mut board,
            &graph,
            &Repair::Reconnect {
                from: "entrance:shop/load".to_owned(),
                to: LOAD.to_owned(),
            },
        )
        .is_err()
    );
}

#[test]
fn health_labels_name_every_kind() {
    let mut health = Health::default();
    assert_eq!(health.label(), None);
    for (kind, count) in [
        (HealthKind::Missing, 1),
        (HealthKind::Outdated, 2),
        (HealthKind::Review, 3),
        (HealthKind::BrokenCombined, 1),
        (HealthKind::BrokenLink, 2),
    ] {
        for index in 0..count {
            health.append(Health {
                items: vec![HealthItem {
                    page: "p".to_owned(),
                    kind,
                    target: Target::Card(format!("{kind:?}-{index}")),
                    key: None,
                    anchor: None,
                    reconnect: None,
                    message: String::new(),
                    repairs: Vec::new(),
                }],
                ..Health::default()
            });
        }
    }
    assert_eq!(
        health.label().as_deref(),
        Some("1 missing · 2 outdated · 3 to review · 1 broken combined step · 2 broken links")
    );
    assert_eq!(health.of_page("p"), health);
    assert!(health.of_page("q").is_healthy());
}

#[test]
fn board_files_are_saved_in_place_and_created_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let dirs = BoardDirs::new(root);
    let board = parse(MINIMAL).unwrap();
    let text = serialize(&board);

    let name = slug("  Order placement: v2!  ");
    assert_eq!(name, "order-placement-v2");
    let file = BoardFile::new(&name).unwrap();
    assert_eq!(file.relative(), ".unfold/boards/order-placement-v2.json");
    let revision = dirs.create(&file, &text).unwrap();
    assert_eq!(std::fs::read_to_string(dirs.path(&file)).unwrap(), text);
    let loaded = dirs.load(&file).unwrap();
    assert_eq!((loaded.board, loaded.revision), (board, revision));
    assert_eq!(dirs.change(&file, revision).unwrap(), Change::Unchanged);

    // Creating never overwrites; the collision names the file and offers a free name.
    let other = text.replace("\"T\"", "\"Other\"");
    match dirs.create(&file, &other) {
        Err(BoardError::NameTaken { path, suggestion }) => {
            assert_eq!(path, ".unfold/boards/order-placement-v2.json");
            assert_eq!(suggestion, "order-placement-v2-2");
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(dirs.change(&file, revision).unwrap(), Change::Unchanged);

    // An external edit is detected.
    std::fs::write(dirs.path(&file), &other).unwrap();
    assert_eq!(
        dirs.change(&file, revision).unwrap(),
        Change::Changed(Revision::of(other.as_bytes()))
    );

    // Saving replaces whatever the file holds, with no revision check, and recreates a deleted
    // file.
    let saved = dirs.save(&file, &text).unwrap();
    assert_eq!(saved, revision);
    assert_eq!(std::fs::read_to_string(dirs.path(&file)).unwrap(), text);
    dirs.remove(&file).unwrap();
    assert_eq!(dirs.change(&file, revision).unwrap(), Change::Missing);
    dirs.save(&file, &other).unwrap();
    assert_eq!(dirs.list().unwrap(), std::slice::from_ref(&file));

    // Files that aren't boards are never listed.
    std::fs::write(root.join(".unfold/boards/notes.txt"), "x").unwrap();
    std::fs::write(root.join(".unfold/boards/.hidden.json"), "x").unwrap();
    assert_eq!(dirs.list().unwrap(), std::slice::from_ref(&file));
    dirs.remove(&file).unwrap();
    dirs.remove(&file).unwrap();
    assert!(dirs.list().unwrap().is_empty());

    // Atomic replacement leaves no temporary files behind.
    let target = root.join("atomic.json");
    files::write_atomic(&target, b"one").unwrap();
    files::write_atomic(&target, b"two").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"two");
    let leftovers = std::fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);

    for bad in [
        "../escape",
        "a/b",
        "",
        ".hidden",
        "a\\b",
        "x".repeat(101).as_str(),
    ] {
        assert!(
            matches!(BoardFile::new(bad), Err(BoardError::InvalidName { .. })),
            "{bad}"
        );
    }
    assert_eq!(slug("¿¿"), "board");
    assert_eq!(slug("Café Crème"), "cafe-creme");
}

#[test]
fn board_directories_must_not_be_links() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path());
    std::fs::create_dir(dir.path().join(".unfold")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".unfold/boards")).unwrap();
    let text = serialize(&parse(MINIMAL).unwrap());
    let file = BoardFile::new("x").unwrap();
    assert!(matches!(dirs.create(&file, &text), Err(BoardError::Io { .. })));
    assert!(matches!(dirs.save(&file, &text), Err(BoardError::Io { .. })));
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());

    // A board-looking file outside the folder is never read, listed, or deleted through the
    // link (a cloned repository can commit `.unfold/boards` as a link).
    let victim = outside.path().join("x.json");
    std::fs::write(&victim, &text).unwrap();
    let refused = |result: Result<(), BoardError>| matches!(result, Err(BoardError::Io { .. }));
    assert!(refused(dirs.remove(&file)));
    assert!(refused(dirs.load(&file).map(drop)));
    assert!(refused(dirs.change(&file, Revision::of(b"")).map(drop)));
    assert!(refused(dirs.list().map(drop)));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);

    // A board file that is itself a link is refused for reading; removing it removes the link.
    std::fs::remove_file(dir.path().join(".unfold/boards")).unwrap();
    std::fs::create_dir(dir.path().join(".unfold/boards")).unwrap();
    std::os::unix::fs::symlink(&victim, dirs.path(&file)).unwrap();
    assert!(refused(dirs.load(&file).map(drop)));
    assert!(refused(dirs.change(&file, Revision::of(b"")).map(drop)));
    dirs.remove(&file).unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);
}

#[test]
fn free_names_stay_valid_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path());
    let text = serialize(&parse(MINIMAL).unwrap());
    // A taken name at the length limit still gets a valid alternative.
    let long = BoardFile::new(&"a".repeat(100)).unwrap();
    dirs.create(&long, &text).unwrap();
    match dirs.create(&long, &text) {
        Err(BoardError::NameTaken { suggestion, .. }) => {
            assert_eq!(suggestion, format!("{}-2", "a".repeat(98)));
            assert!(BoardFile::new(&suggestion).is_ok());
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(dirs.free_name("fresh"), "fresh");
}

#[test]
fn oversized_board_files_are_refused_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path());
    let file = BoardFile::new("big").unwrap();
    dirs.save(&file, &serialize(&parse(MINIMAL).unwrap())).unwrap();
    let handle = std::fs::OpenOptions::new()
        .write(true)
        .open(dirs.path(&file))
        .unwrap();
    handle.set_len(MAX_BOARD_BYTES + 1).unwrap();
    assert!(matches!(dirs.load(&file), Err(BoardError::Io { .. })));
}

/// A board whose file would be too large to read again is never written: not saved over its
/// file, and not created.
#[test]
fn boards_too_large_to_read_again_are_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path());
    let file = BoardFile::new("big").unwrap();
    let small = serialize(&parse(MINIMAL).unwrap());
    dirs.save(&file, &small).unwrap();
    let mut board = parse(MINIMAL).unwrap();
    board.title = "x".repeat(usize::try_from(MAX_BOARD_BYTES).unwrap());
    let large = serialize(&board);
    let size = u64::try_from(large.len()).unwrap();
    assert!(matches!(dirs.save(&file, &large), Err(BoardError::TooLarge { bytes }) if bytes == size));
    assert_eq!(std::fs::read_to_string(dirs.path(&file)).unwrap(), small);
    let error = dirs
        .create(&BoardFile::new("other").unwrap(), &large)
        .unwrap_err();
    assert!(
        error.to_string().starts_with("the board is too large to save"),
        "{error}"
    );
    assert_eq!(dirs.list().unwrap(), [file]);
}

/// Files in the boards folder whose names a board file can't have are listed apart, with
/// which names are allowed; hidden and temporary files and other files aren't listed at all.
#[test]
fn misnamed_board_files_are_listed_with_the_names_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path());
    let text = serialize(&parse(MINIMAL).unwrap());
    let file = BoardFile::new("orders").unwrap();
    dirs.create(&file, &text).unwrap();
    let folder = dir.path().join(".unfold/boards");
    for name in [
        "Order flow copy.json",
        "._orders.json",
        ".hidden.json",
        ".orders.json.1-2-3.tmp",
        "notes.txt",
    ] {
        std::fs::write(folder.join(name), &text).unwrap();
    }
    assert_eq!(dirs.list().unwrap(), std::slice::from_ref(&file));
    let misnamed = dirs.misnamed().unwrap();
    assert_eq!(
        misnamed
            .iter()
            .map(|(name, error)| (name.as_str(), error.to_string()))
            .collect::<Vec<_>>(),
        [(
            "Order flow copy",
            "board name `Order flow copy` must be 1 to 100 ASCII letters, digits, `.`, `_`, or `-`, \
             starting with a letter or digit"
                .to_owned()
        )]
    );
    assert_eq!(files::listed_stem("orders.json"), Some("orders"));
    for unlisted in [".json", "._orders.json", "sub/orders.json", "orders.json.tmp"] {
        assert_eq!(files::listed_stem(unlisted), None, "{unlisted}");
    }
    // Creating leaves nothing else behind: no temporary file and no claim.
    let names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".tmp") && name != ".orders.json.1-2-3.tmp")
        .collect();
    assert!(names.is_empty(), "{names:?}");
}

/// A decorated definition is signed over its decorators too: changing `@router.get` to
/// `@router.post` makes what was signed over it outdated.
#[test]
fn a_definition_is_signed_from_its_first_decorator() {
    let compiled = |text: &'static str| {
        let mut builder = GraphBuilder::new();
        let file = builder.file("api.py", Language::Python, text);
        let decorator = range(1, text.find('\n').unwrap());
        let call = builder.call(file, &Callee::name("router"), decorator);
        let definition = builder.def(
            file,
            Form::Function,
            "a",
            builder.find(file, "def a():\n    return 1"),
        );
        let node = builder.node_mut(call);
        node.role = Some(CallRole::Decorator);
        node.r#for = Some(definition.local_id());
        builder.build()
    };
    const BEFORE: &str = "@router.get(\"/a\")\ndef a():\n    return 1\n";
    const AFTER: &str = "@router.post(\"/a\")\ndef a():\n    return 1\n";
    let signed = |text: &'static str| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("api.py"), text).unwrap();
        let graph = compiled(text);
        BoardGraph::from_graph(&graph, dir.path()).signature("api.py#a")
    };
    // From the decorator's call (after its `@`) to the definition's end.
    assert_eq!(
        signed(BEFORE),
        Some(Signature::of(BEFORE[1..].trim_end().as_bytes()))
    );
    assert_ne!(signed(BEFORE), signed(AFTER));
}
