//! Board tests: strict parsing, deterministic serialization, signatures, guards, groups, drawn
//! links, health and repairs across projects, saving, the unpublished-changes count, and board
//! files, on a small hand-built graph whose source is written to a temporary folder (a second
//! folder with the same source plays another project).

use std::collections::BTreeMap;
use std::path::Path;

use unfold_core::testkit::{GraphBuilder, Outcome};
use unfold_core::vocab::{BlockKind, EdgeKind, Language, Role, SlotName};
use unfold_core::{ByteRange, Graph, Key};

use super::check::group_problem;
use super::files::{BoardDirs, BoardFile, Change, DRAFTS_DIR, MAX_BOARD_BYTES, Place, Revision, slug};
use super::*;

const SOURCE: &str = "export function run() {\n  const first = 1;\n  if (first) { console.log(first); }\n  console.log('done');\n  return first;\n}\n";

const RUN: &str = "main.ts#function:run";
const FIRST: &str = "main.ts#function:run/variable:first";
const IF0: &str = "main.ts#function:run/if[0]";
const LOG0: &str = "main.ts#function:run/expression_statement[0]";
const RETURN0: &str = "main.ts#function:run/return[0]";

fn range(start: usize, end: usize) -> ByteRange {
    ByteRange {
        start: u32::try_from(start).unwrap(),
        end: u32::try_from(end).unwrap(),
    }
}

fn console_log(
    builder: &mut GraphBuilder,
    parent: unfold_core::testkit::BlockRef,
    role: Role,
    at: usize,
    text: &str,
) {
    let statement_text = &text[at..at + text[at..].find(';').unwrap() + 1];
    let statement = builder.block(
        parent,
        BlockKind::ExpressionStatement,
        Some(role),
        None,
        range(at, at + statement_text.len()),
    );
    let call = range(at, at + statement_text.len() - 1);
    builder.slot(statement, SlotName::Expression, None, call);
    let block = builder.block(statement, BlockKind::Call, Some(Role::Expression), None, call);
    builder.slot(block, SlotName::Callee, None, range(at, at + "console.log".len()));
    builder.edge(
        block,
        EdgeKind::Calls,
        Outcome::external("global"),
        "console.log",
        None,
    );
}

/// The graph of one function written as `text`: one statement per line, each a `const`, an
/// `if (…) { console.log(…); }`, a `console.log(…);`, or a `return`.
fn graph_of(text: &str) -> Graph {
    let mut builder = GraphBuilder::new();
    let root = builder.file("main.ts", Language::TypeScript, text);
    let start = text.find("function ").unwrap();
    let name = &text[start + "function ".len()..text.find('(').unwrap()];
    let end = text.rfind('}').unwrap() + 1;
    let function = builder.block(
        root,
        BlockKind::Function,
        Some(Role::Body),
        Some(name),
        range(start, end),
    );
    let mut offset = text.find('\n').unwrap() + 1;
    for line in text[offset..].lines() {
        let indent = line.len() - line.trim_start().len();
        let at = offset + indent;
        let statement = line.trim();
        if let Some(rest) = statement.strip_prefix("const ") {
            let name = &rest[..rest.find(' ').unwrap()];
            let block = builder.block(
                function,
                BlockKind::Variable,
                Some(Role::Body),
                Some(name),
                range(at, at + statement.len() - 1),
            );
            let initializer = at + statement.find("= ").unwrap() + 2;
            builder.slot(
                block,
                SlotName::Initializer,
                None,
                range(initializer, at + statement.len() - 1),
            );
        } else if statement.starts_with("if (") {
            let block = builder.block(
                function,
                BlockKind::If,
                Some(Role::Body),
                None,
                range(at, at + statement.len()),
            );
            let close = statement.find(')').unwrap();
            builder.slot(block, SlotName::Condition, None, range(at + 4, at + close));
            let then = at + statement.find('{').unwrap();
            builder.slot(block, SlotName::Then, None, range(then, at + statement.len()));
            console_log(&mut builder, block, Role::Then, then + 2, text);
        } else if statement.starts_with("console.log") {
            console_log(&mut builder, function, Role::Body, at, text);
        } else if statement.starts_with("return ") {
            let block = builder.block(
                function,
                BlockKind::Return,
                Some(Role::Body),
                None,
                range(at, at + statement.len()),
            );
            builder.slot(
                block,
                SlotName::Expression,
                None,
                range(at + 7, at + statement.len() - 1),
            );
        }
        offset += line.len() + 1;
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

fn author() -> Author {
    Author::agent("test").unwrap()
}

fn web() -> ProjectName {
    ProjectName::parse("web").unwrap()
}

fn unsigned(text: &str) -> Description {
    Description {
        text: text.to_owned(),
        author: author(),
        signature: None,
    }
}

fn card(id: &str, reference: CardRef) -> Card {
    Card {
        id: id.to_owned(),
        reference,
        at: [10.0, 20.0],
        view: None,
        folded: false,
        description: None,
        expanded: Vec::new(),
    }
}

fn run_card(id: &str, project: Option<ProjectName>) -> Card {
    card(id, CardRef::block(Key::from_text(RUN), project))
}

fn label(project: Option<ProjectName>, key: &str, text: &str) -> LabelEntry {
    LabelEntry {
        project,
        key: Key::from_text(key),
        label: unsigned(text),
    }
}

fn link(id: &str, from: &str, to: &str, text: Option<&str>) -> Link {
    Link {
        label: text.map(unsigned),
        ..Link::new(id, from, to)
    }
}

/// A board with one Flow page holding card `run`.
fn sample_board() -> Board {
    let mut board = Board::new("Run explanation");
    board.pages[0].default_view = View::Flow;
    board.pages[0].cards.push(run_card("run", None));
    board
}

fn group(members: &[&str]) -> Group {
    Group {
        id: "steps".to_owned(),
        card: "run".to_owned(),
        title: "Steps".to_owned(),
        members: members.iter().map(|key| Key::from_text(key)).collect(),
        collapsed: false,
        description: None,
        author: None,
    }
}

/// Saves `board` (over `previous`) against the own graph only.
fn save(
    board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    prepare_save(board, previous, &PageGraphs::own_only(graph))
}

fn page_health(board: &Board, graph: &BoardGraph) -> Health {
    board_health(board, &PageGraphs::own_only(graph))
}

fn repair(board: &mut Board, graph: &BoardGraph, repair: &Repair) -> Result<(), BoardError> {
    apply_repair(&mut board.pages[0], &PageGraphs::own_only(graph), repair)
}

fn own_label(key: &str) -> Target {
    Target::Label(ProjectKey::new(None, key))
}

fn invalid(text: &str) -> (String, String) {
    match parse(text) {
        Err(BoardError::Invalid { path, message }) => (path, message),
        other => panic!("expected an invalid board, got {other:?}"),
    }
}

const MINIMAL: &str = r#"{"format":"unfold-board","pages":[{"cards":[{"at":[0,0],"id":"run","ref":{"block":"main.ts#function:run"}}],"defaultView":"code","groups":[],"id":"page-1","title":"P"}],"projects":[],"title":"T","version":1}"#;

/// `MINIMAL` with `"groups":[]` replaced by `groups` (and whatever follows it in the page).
fn with_page_fields(fields: &str) -> String {
    MINIMAL.replace("\"groups\":[]", fields)
}

#[test]
fn parsing_rejects_everything_outside_the_format_with_a_path() {
    assert!(parse(MINIMAL).is_ok());
    let card_ref = r#"{"block":"main.ts#function:run"}"#;
    let web_card = r#"{"at":[0,0],"id":"web","ref":{"block":"main.ts#function:run","project":"web"}}"#;
    let signature = format!("sha256:{}", "0".repeat(64));
    let cases = [
        (
            MINIMAL.replace("\"title\":\"T\"", "\"extra\":1,\"title\":\"T\""),
            "/extra",
            "unknown field",
        ),
        (
            MINIMAL.replace("\"id\":\"run\"", "\"id\":\"run\",\"state\":\"code\""),
            "/pages/0/cards/0/state",
            "unknown field",
        ),
        (
            MINIMAL.replace(
                card_ref,
                r#"{"block":"main.ts#function:run","entrance":"entrance:x"}"#,
            ),
            "/pages/0/cards/0/ref",
            "exactly one",
        ),
        (
            MINIMAL.replace(card_ref, r#"{"block":"entrance:x"}"#),
            "/pages/0/cards/0/ref/block",
            "block key",
        ),
        (
            MINIMAL.replace(card_ref, r#"{"block":"main.ts#function:run","project":"no way"}"#),
            "/pages/0/cards/0/ref/project",
            "shared name",
        ),
        (
            MINIMAL.replace("[0,0]", "[0]"),
            "/pages/0/cards/0/at",
            "two numbers",
        ),
        (
            MINIMAL.replace("[0,0]", "[0,\"1\"]"),
            "/pages/0/cards/0/at/1",
            "finite number",
        ),
        (
            MINIMAL.replace("\"version\":1", "\"version\":3"),
            "/version",
            "unsupported version",
        ),
        (
            MINIMAL.replace("unfold-board", "codegraph-board"),
            "/format",
            "unfold-board",
        ),
        (
            MINIMAL.replace("\"code\"", "\"canvas\""),
            "/pages/0/defaultView",
            "\"code\", \"flow\", or \"diagram\"",
        ),
        (MINIMAL.replace("\"T\"", "\"  \""), "/title", "must not be empty"),
        (
            MINIMAL.replace("\"P\"", "\"\""),
            "/pages/0/title",
            "must not be empty",
        ),
        (
            MINIMAL.replace(",\"groups\":[]", ""),
            "/pages/0",
            "missing field `groups`",
        ),
        (
            MINIMAL.replace("\"projects\":[],", ""),
            "/",
            "missing field `projects`",
        ),
        (
            MINIMAL.replace(
                &MINIMAL[MINIMAL.find('[').unwrap()..MINIMAL.find(",\"projects\"").unwrap()],
                "[]",
            ),
            "/pages",
            "at least one page",
        ),
        (
            MINIMAL.replace(
                "\"projects\":[]",
                "\"projects\":[],\"publishes\":\"boards/x.json\"",
            ),
            "/publishes",
            "unknown field",
        ),
        (
            MINIMAL.replace("\"id\":\"run\"", "\"id\":\"run card\""),
            "/pages/0/cards/0/id",
            "letter or digit",
        ),
        (
            MINIMAL.replace("\"page-1\"", "\"-1\""),
            "/pages/0/id",
            "letter or digit",
        ),
        (
            MINIMAL.replace("\"projects\":[]", "\"projects\":[\"web\"]"),
            "/projects",
            "exactly the other projects",
        ),
        (
            MINIMAL.replace("\"projects\":[]", "\"projects\":[\"-\"]"),
            "/projects/0",
            "shared name",
        ),
        (
            MINIMAL.replace("\"cards\":[", &format!("\"cards\":[{web_card},")),
            "/projects",
            "[\"web\"]",
        ),
        (
            MINIMAL.replace("\"format\"", "\"folder\":\"\",\"format\""),
            "/folder",
            "folder path",
        ),
        (
            MINIMAL.replace("\"format\"", "\"folder\":\"a//b\",\"format\""),
            "/folder",
            "folder path",
        ),
        (
            MINIMAL.replace(
                "\"id\":\"run\"",
                r#""id":"run","description":{"author":"you","text":"x"}"#,
            ),
            "/pages/0/cards/0/description/author",
            "person:<name>",
        ),
        (
            MINIMAL.replace(
                "\"id\":\"run\"",
                r#""id":"run","description":{"author":"person:a","text":"x","signature":"sha256:XYZ"}"#,
            ),
            "/pages/0/cards/0/description/signature",
            "sha256",
        ),
        (
            with_page_fields(
                r#""groups":[{"card":"nope","collapsed":false,"id":"g","members":["main.ts#function:run/if[0]"],"title":"G"}]"#,
            ),
            "/pages/0/groups/0/card",
            "unknown card",
        ),
        (
            with_page_fields(
                r#""groups":[{"card":"run","collapsed":false,"id":"run","members":["main.ts#a"],"title":"G"}]"#,
            ),
            "/pages/0/groups/0/id",
            "duplicate id",
        ),
        (
            with_page_fields(
                r#""groups":[{"card":"run","collapsed":false,"id":"g","members":["main.ts#a","main.ts#a"],"title":"G"}]"#,
            ),
            "/pages/0/groups/0/members/1",
            "repeats",
        ),
        (
            with_page_fields(&format!(
                r#""groups":[],"guards":[{{"key":"main.ts#a","kind":"lambda","owner":"","signature":"{signature}"}}]"#
            )),
            "/pages/0/guards/0/kind",
            "block kind",
        ),
        (
            with_page_fields(&format!(
                r#""groups":[],"guards":[{{"key":"main.ts#a","kind":"if","owner":"","signature":"{signature}"}},{{"key":"main.ts#a","kind":"if","owner":"","signature":"{signature}"}}]"#
            )),
            "/pages/0/guards/1",
            "repeats the guard",
        ),
        (
            with_page_fields(
                r#""groups":[],"labels":[{"author":"agent:x","key":"a/b#c","text":"t","extra":1}]"#,
            ),
            "/pages/0/labels/0/extra",
            "unknown field",
        ),
        (
            with_page_fields(
                r##""groups":[],"labels":[{"author":"agent:x","key":"a#c","text":"t"},{"author":"agent:x","key":"a#c","text":"u"}]"##,
            ),
            "/pages/0/labels/1",
            "repeats the label",
        ),
        (
            with_page_fields(
                r##""groups":[],"labels":[{"author":"agent:x","key":"entrance:x","text":"t"}]"##,
            ),
            "/pages/0/labels/0/key",
            "block key",
        ),
        (
            with_page_fields(r#""groups":[],"links":[{"from":"run","id":"l","to":"run"}]"#),
            "/pages/0/links/0/to",
            "another item",
        ),
        (
            with_page_fields(r#""groups":[],"links":[{"from":"nope","id":"l","to":"run"}]"#),
            "/pages/0/links/0/from",
            "unknown item `nope`",
        ),
        (
            with_page_fields(r#""groups":[],"links":[{"from":"run","id":"run","to":"run"}]"#),
            "/pages/0/links/0/id",
            "duplicate id",
        ),
        (
            with_page_fields(
                r#""groups":[],"links":[{"from":"run","id":"l","to":"x","label":{"author":"agent:x","text":" "}}]"#,
            ),
            "/pages/0/links/0/label/text",
            "must not be empty",
        ),
    ];
    for (text, path, message) in cases {
        let (actual_path, actual_message) = invalid(&text);
        assert_eq!(actual_path, path, "{text}");
        assert!(
            actual_message.contains(message),
            "{actual_message} lacks {message}"
        );
    }
    // Two pages with one id.
    let page = &MINIMAL[MINIMAL.find("[{\"cards\"").unwrap() + 1..MINIMAL.find("],\"projects\"").unwrap()];
    assert_eq!(
        invalid(&MINIMAL.replace(page, &format!("{page},{page}"))),
        ("/pages/1/id".to_owned(), "duplicate page id `page-1`".to_owned())
    );
    // The same key may be labeled once per project.
    let both = with_page_fields(
        r##""groups":[],"labels":[{"author":"agent:x","key":"a#c","text":"t"},{"author":"agent:x","key":"a#c","project":"web","text":"u"}]"##,
    );
    assert_eq!(parse(&both).unwrap().pages[0].labels.len(), 2);
    assert!(
        matches!(parse("{\"a\":1,\"a\":2}"), Err(BoardError::Syntax { .. })),
        "duplicate keys"
    );
    assert!(matches!(parse("[1,"), Err(BoardError::Syntax { line: 1, .. })));
    assert_eq!(invalid("[]").0, "/");
}

/// `MINIMAL` with a second card `two`, a table `t` listing column `id`, and `fields` in the page.
fn with_items(fields: &str) -> String {
    MINIMAL.replace(
        "\"groups\":[]",
        &format!(
            r#""groups":[],"resources":[{{"anchors":[],"at":[0,0],"author":"agent:x","columns":[{{"name":"id"}}],"id":"t","kind":"table","name":"t"}}]{fields}"#
        ),
    )
    .replace(
        "\"cards\":[",
        r#""cards":[{"at":[0,0],"id":"two","ref":{"entrance":"entrance:x"}},"#,
    )
}

#[test]
fn parsing_checks_frames_resources_connections_and_notes() {
    let frame = |id: &str, members: &str| {
        format!(r#"{{"author":"agent:x","id":"{id}","look":"box","members":[{members}],"title":"F"}}"#)
    };
    let resource = |fields: &str| {
        with_page_fields(&format!(
            r#""groups":[],"resources":[{{"anchors":[],"at":[0,0],"author":"agent:x","id":"r","name":"r",{fields}}}]"#
        ))
    };
    let anchor = |anchor: &str| {
        with_page_fields(&format!(
            r#""groups":[],"resources":[{{"anchors":[{anchor}],"at":[0,0],"author":"agent:x","id":"r","kind":"queue","name":"r"}}]"#
        ))
    };
    let note = |fields: &str| {
        with_items(&format!(
            r#","notes":[{{"at":[0,0],"author":"agent:x","id":"n",{fields}}}]"#
        ))
    };
    let long = "x".repeat(MAX_NOTE_CHARACTERS + 1);
    let cases = [
        // Frames.
        (
            with_items(&format!(r#","frames":[{}]"#, frame("f", ""))),
            "/pages/0/frames/0/members",
            "at least one item",
        ),
        (
            with_items(&format!(r#","frames":[{}]"#, frame("f", r#""run","run""#))),
            "/pages/0/frames/0/members/1",
            "repeats an earlier member",
        ),
        (
            with_items(&format!(r#","frames":[{}]"#, frame("f", r#""nope""#))),
            "/pages/0/frames/0/members/0",
            "unknown item `nope`",
        ),
        (
            with_items(r#","frames":[{"author":"agent:x","id":"f","look":"box","members":["run"],"title":" "}]"#),
            "/pages/0/frames/0/title",
            "must not be empty",
        ),
        (
            with_items(r#","frames":[{"author":"agent:x","id":"f","look":"round","members":["run"],"title":"F"}]"#),
            "/pages/0/frames/0/look",
            "must be \"box\" or \"zone\"",
        ),
        (
            with_items(r#","frames":[{"author":"agent:x","icon":"db","id":"f","look":"box","members":["run"],"title":"F"}]"#),
            "/pages/0/frames/0/icon",
            "\"library\", or \"system\"",
        ),
        (
            with_items(r#","frames":[{"id":"f","look":"box","members":["run"],"title":"F"}]"#),
            "/pages/0/frames/0",
            "missing field `author`",
        ),
        (
            with_items(&format!(
                r#","frames":[{},{}]"#,
                frame("f", r#""run""#),
                frame("g", r#""run""#)
            )),
            "/pages/0/frames/1/members/0",
            "already in frame `f`",
        ),
        (
            with_items(&format!(
                r#","frames":[{},{}]"#,
                frame("f", r#""g""#),
                frame("g", r#""f""#)
            )),
            "/pages/0/frames/0",
            "inside itself",
        ),
        (
            with_items(&format!(r#","frames":[{}]"#, frame("f", r#""f""#))),
            "/pages/0/frames/0",
            "inside itself",
        ),
        (
            with_items(&format!(
                r#","frames":[{},{},{},{}]"#,
                frame("a", r#""run""#),
                frame("b", r#""a""#),
                frame("c", r#""b""#),
                frame("d", r#""c""#)
            )),
            "/pages/0/frames/0",
            "nested 4 frames deep",
        ),
        (
            with_page_fields(&format!(
                r#""groups":[{{"card":"run","collapsed":false,"id":"g","members":["main.ts#a"],"title":"G"}}],"frames":[{}]"#,
                frame("f", r#""g""#)
            )),
            "/pages/0/frames/0/members/0",
            "`g` is a group; a frame holds cards, resources, notes, and frames",
        ),
        (
            with_items(&format!(r#","frames":[{}]"#, frame("run", r#""two""#))),
            "/pages/0/frames/0/id",
            "duplicate id `run`",
        ),
        // Resource cards and anchors.
        (resource(r#""kind":"db""#), "/pages/0/resources/0/kind", "\"external\", or \"actor\""),
        (
            resource(r#""kind":"queue","columns":[]"#),
            "/pages/0/resources/0/columns",
            "only a table",
        ),
        (
            resource(r#""kind":"queue","type":"a.ts#interface:A""#),
            "/pages/0/resources/0/type",
            "only a table",
        ),
        (
            resource(r#""kind":"table","project":"web""#),
            "/pages/0/resources/0/project",
            "only with `type`",
        ),
        (
            resource(r#""kind":"table","columns":[{"name":"id"},{"name":"id"}]"#),
            "/pages/0/resources/0/columns/1/name",
            "repeats column `id`",
        ),
        (
            resource(r#""kind":"table","type":"a.ts#interface:A","columns":[{"name":"id","type":"string"}]"#),
            "/pages/0/resources/0/columns/0/type",
            "marks only",
        ),
        (
            resource(r#""kind":"table","columns":[{"name":"id","references":"a.id"}]"#),
            "/pages/0/resources/0/columns/0/references",
            "only a foreign key",
        ),
        (
            with_page_fields(r#""groups":[],"resources":[{"at":[0,0],"author":"agent:x","id":"r","kind":"actor","name":"r"}]"#),
            "/pages/0/resources/0",
            "missing field `anchors`",
        ),
        (
            anchor(r#"{"key":"a.ts#b","file":"a.sql","lines":[1,1]}"#),
            "/pages/0/resources/0/anchors/0",
            "exactly one of `key` or `file`",
        ),
        (
            anchor(r#"{"key":"orders"}"#),
            "/pages/0/resources/0/anchors/0/key",
            "entrance key",
        ),
        (
            anchor(r#"{"key":"a.ts#b","lines":[1,2]}"#),
            "/pages/0/resources/0/anchors/0/lines",
            "only a file anchor",
        ),
        (
            anchor(r#"{"file":"../a.sql","lines":[1,2]}"#),
            "/pages/0/resources/0/anchors/0/file",
            "relative to the project's folder",
        ),
        (
            anchor(r#"{"file":"a.sql"}"#),
            "/pages/0/resources/0/anchors/0",
            "missing field `lines`",
        ),
        (
            anchor(r#"{"file":"a.sql","lines":[0,2]}"#),
            "/pages/0/resources/0/anchors/0/lines/0",
            "line number",
        ),
        (
            anchor(r#"{"file":"a.sql","lines":[5,2]}"#),
            "/pages/0/resources/0/anchors/0/lines/1",
            "before the first line",
        ),
        (
            anchor(r#"{"file":"a.sql","lines":[2]}"#),
            "/pages/0/resources/0/anchors/0/lines",
            "two line numbers",
        ),
        // Connections.
        (
            with_items(r#","links":[{"author":"agent:x","from":"run","id":"l","kind":"ftp","to":"t"}]"#),
            "/pages/0/links/0/kind",
            "\"relates\", or \"other\"",
        ),
        (
            with_items(r#","links":[{"from":"run","id":"l","kind":"http","to":"t"}]"#),
            "/pages/0/links/0",
            "missing field `author`",
        ),
        (
            with_items(r#","links":[{"author":"agent:x","from":"run","fromColumn":"id","id":"l","kind":"writes","to":"t"}]"#),
            "/pages/0/links/0/fromColumn",
            "only a `relates` connection",
        ),
        (
            with_items(r#","links":[{"cardinality":"one-to-one","from":"run","id":"l","to":"t"}]"#),
            "/pages/0/links/0/cardinality",
            "only a `relates` connection",
        ),
        (
            with_items(r#","links":[{"author":"agent:x","from":"run","id":"l","kind":"relates","to":"t"}]"#),
            "/pages/0/links/0/from",
            "must be a table",
        ),
        (
            with_items(&format!(
                r#","frames":[{}],"links":[{{"author":"agent:x","from":"t","id":"l","kind":"relates","to":"f","toColumn":"id"}}]"#,
                frame("f", r#""two""#)
            )),
            "/pages/0/links/0/to",
            "must be a table",
        ),
        (
            with_items(r#","links":[{"author":"agent:x","from":"t","fromColumn":"nope","id":"l","kind":"relates","to":"t2"}]"#)
                .replace(
                    r#""resources":["#,
                    r#""resources":[{"anchors":[],"at":[0,0],"author":"agent:x","id":"t2","kind":"table","name":"t2"},"#,
                ),
            "/pages/0/links/0/fromColumn",
            "names no column of table `t`",
        ),
        (
            with_items(r#","links":[{"author":"agent:x","from":"t","id":"l","kind":"writes","step":"main.ts#function:run/if[0]","to":"run"}]"#),
            "/pages/0/links/0/step",
            "must be a block card",
        ),
        (
            with_items(r#","links":[{"author":"agent:x","from":"two","id":"l","kind":"calls","step":"main.ts#function:run/if[0]","to":"run"}]"#),
            "/pages/0/links/0/step",
            "must be a block card",
        ),
        (
            with_items(r#","links":[{"carriesProject":"web","from":"run","id":"l","to":"t"}]"#),
            "/pages/0/links/0/carriesProject",
            "only with `carries`",
        ),
        (
            with_items(r#","links":[{"anchors":[{"key":"main.ts#a"}],"from":"run","id":"l","to":"t"}]"#),
            "/pages/0/links/0/anchors",
            "only a connection (a link with `kind`)",
        ),
        (
            with_items(r#","links":[{"description":{"author":"person:a","text":"T"},"from":"run","id":"l","to":"t"}]"#),
            "/pages/0/links/0/description",
            "a drawn link has a label",
        ),
        (
            with_items(r#","notes":[{"at":[0,0],"author":"agent:x","id":"n","text":"N"}],"links":[{"from":"run","id":"l","to":"n"}]"#),
            "/pages/0/links/0/to",
            "`n` is a note; a link joins cards, resources, and frames",
        ),
        // Notes.
        (
            note(&format!(r#""text":"{long}""#)),
            "/pages/0/notes/0/text",
            "at most 400 characters",
        ),
        (note(r#""text":" ""#), "/pages/0/notes/0/text", "must not be empty"),
        (
            note(r#""text":"N","attach":"nope""#),
            "/pages/0/notes/0/attach",
            "unknown item `nope`",
        ),
        (
            with_page_fields(
                r#""groups":[{"card":"run","collapsed":false,"id":"g","members":["main.ts#a"],"title":"G"}],"notes":[{"at":[0,0],"attach":"g","author":"agent:x","id":"n","text":"N"}]"#,
            ),
            "/pages/0/notes/0/attach",
            "`g` is a group; a note attaches to a card, frame, resource, or link",
        ),
        // Step groups name who wrote their title.
        (
            with_page_fields(
                r#""groups":[{"author":"robot","card":"run","collapsed":false,"id":"g","members":["main.ts#a"],"title":"G"}]"#,
            ),
            "/pages/0/groups/0/author",
            "person:<name>",
        ),
    ];
    for (text, path, message) in cases {
        let (actual_path, actual_message) = invalid(&text);
        assert_eq!(actual_path, path, "{text}\n{actual_message}");
        assert!(
            actual_message.contains(message),
            "{actual_message} lacks {message}"
        );
    }

    // Everything at once reads: Diagram views, frames nested three deep, links to tables and
    // frames, a relationship between columns, and notes on a link and a frame.
    let valid = with_items(&format!(
        r#","frames":[{},{},{}],"links":[{{"anchors":[{{"file":"db/schema.sql","lines":[3,9]}},{{"key":"entrance:x","project":"web"}}],"author":"agent:x","channel":"orders","from":"run","id":"l","kind":"queue","step":"main.ts#function:run/if[0]","to":"c"}},{{"author":"person:a","cardinality":"one-to-many","from":"t","fromColumn":"id","id":"m","kind":"relates","to":"u","toColumn":"order_id"}}],"notes":[{{"at":[0,0],"attach":"l","author":"agent:x","id":"n","text":"{}"}},{{"at":[1,1],"attach":"c","author":"agent:x","id":"o","text":"Free"}}]"#,
        frame("a", r#""run","n""#),
        frame("b", r#""a""#),
        frame("c", r#""b","t""#),
        "y".repeat(MAX_NOTE_CHARACTERS),
    ))
    .replace("\"defaultView\":\"code\"", "\"defaultView\":\"diagram\"")
    .replace("\"id\":\"two\",", "\"id\":\"two\",\"view\":\"diagram\",")
    .replace(
        r#""resources":["#,
        r#""resources":[{"anchors":[],"at":[0,0],"author":"agent:x","id":"u","kind":"table","name":"u","project":"web","type":"a.ts#interface:Item"},"#,
    )
    .replace("\"projects\":[]", "\"projects\":[\"web\"]");
    let error = parse(&valid).err();
    assert!(error.is_none(), "{error:?}");
    let parsed = parse(&valid).unwrap();
    assert_eq!(
        parsed.projects,
        [web()],
        "anchors' and types' projects are the board's projects"
    );
    assert_eq!(parsed.pages[0].default_view, View::Diagram);
    assert_eq!(parsed.pages[0].card("two").unwrap().view, Some(View::Diagram));
    assert!(
        parsed.pages[0].link("m").unwrap().relates(),
        "a type-backed table's column"
    );
}

#[test]
fn new_items_are_written_only_when_present() {
    // Empty arrays and false flags on input are the defaults: they're never written.
    let text = with_page_fields(
        r#""groups":[{"author":"person:a","card":"run","collapsed":false,"id":"g","members":["main.ts#a"],"title":"G"}],"frames":[],"notes":[],"resources":[],"links":[{"anchors":[],"from":"run","id":"l","to":"run2"}]"#,
    )
    .replace(
        "\"cards\":[",
        r#""cards":[{"at":[0,0],"id":"run2","ref":{"entrance":"entrance:x"}},"#,
    );
    let written = serialize(&parse(&text).unwrap());
    for absent in ["\"frames\"", "\"notes\"", "\"resources\"", "\"anchors\""] {
        assert!(!written.contains(absent), "{absent} in {written}");
    }
    assert!(written.contains("\"author\": \"person:a\""), "the group's author");

    // One of each item, written canonically: keys sorted, items by id, optional fields only
    // when present.
    let mut page = Page::new("p", "P");
    page.cards.push(run_card("run", None));
    let agent = author();
    page.frames.push(Frame {
        id: "f".to_owned(),
        title: "Unit".to_owned(),
        description: None,
        members: vec!["run".to_owned(), "t".to_owned()],
        look: FrameLook::Box,
        icon: Some(FrameIcon::Api),
        direction: Some(FrameDirection::Down),
        collapsed: true,
        author: agent.clone(),
    });
    page.resources.push(Resource {
        id: "t".to_owned(),
        kind: ResourceKind::Table,
        name: "orders".to_owned(),
        project: Some(web()),
        type_key: Some(Key::from_text("a.ts#interface:Order")),
        columns: vec![Column {
            name: "id".to_owned(),
            written_type: None,
            key: Some(ColumnKey::Primary),
            references: None,
            optional: false,
        }],
        anchors: vec![Anchor::File {
            file: unfold_core::RepoPath::new("db/schema.sql").unwrap(),
            lines: [3, 9],
            project: None,
        }],
        at: [1.5, -2.0],
        description: None,
        author: agent.clone(),
        signature: None,
    });
    page.links.push(Link {
        kind: Some(LinkKind::Writes),
        step: Some(Key::from_text(IF0)),
        author: Some(agent.clone()),
        anchors: vec![Anchor::Key {
            key: Key::from_text(RUN),
            project: None,
        }],
        ..Link::new("w", "run", "t")
    });
    page.notes.push(Note {
        id: "n".to_owned(),
        text: "Mind the gap".to_owned(),
        at: [0.0, 10.0],
        attach: Some("w".to_owned()),
        author: agent,
    });
    let board = Board::with_pages("T", None, vec![page]);
    assert_eq!(board.projects, [web()], "the table's type is in web");
    let text = serialize(&board);
    let expected = r#"{
  "format": "unfold-board",
  "pages": [
    {
      "cards": [
        {
          "at": [
            10,
            20
          ],
          "id": "run",
          "ref": {
            "block": "main.ts#function:run"
          }
        }
      ],
      "defaultView": "diagram",
      "frames": [
        {
          "author": "agent:test",
          "collapsed": true,
          "direction": "down",
          "icon": "api",
          "id": "f",
          "look": "box",
          "members": [
            "run",
            "t"
          ],
          "title": "Unit"
        }
      ],
      "groups": [],
      "guards": [],
      "id": "p",
      "labels": [],
      "links": [
        {
          "anchors": [
            {
              "key": "main.ts#function:run"
            }
          ],
          "author": "agent:test",
          "from": "run",
          "id": "w",
          "kind": "writes",
          "step": "main.ts#function:run/if[0]",
          "to": "t"
        }
      ],
      "notes": [
        {
          "at": [
            0,
            10
          ],
          "attach": "w",
          "author": "agent:test",
          "id": "n",
          "text": "Mind the gap"
        }
      ],
      "resources": [
        {
          "anchors": [
            {
              "file": "db/schema.sql",
              "lines": [
                3,
                9
              ]
            }
          ],
          "at": [
            1.5,
            -2
          ],
          "author": "agent:test",
          "columns": [
            {
              "key": "primary",
              "name": "id"
            }
          ],
          "id": "t",
          "kind": "table",
          "name": "orders",
          "project": "web",
          "type": "a.ts#interface:Order"
        }
      ],
      "title": "P"
    }
  ],
  "projects": [
    "web"
  ],
  "title": "T",
  "version": 1
}
"#;
    assert_eq!(text, expected);
    let reparsed = parse(&text).unwrap();
    assert_eq!(reparsed, board);
    // What the window receives (serde) has the file's shape, numbers aside (`10.0` is `10`).
    fn numbers(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Number(number) => number.as_f64().map_or(serde_json::Value::Null, Into::into),
            serde_json::Value::Array(items) => items.into_iter().map(numbers).collect(),
            serde_json::Value::Object(map) => {
                map.into_iter().map(|(key, item)| (key, numbers(item))).collect()
            }
            other => other,
        }
    }
    let ipc = numbers(serde_json::to_value(&board.pages[0]).unwrap());
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(ipc, numbers(file["pages"][0].clone()));
}

/// A board using every part of the format: a folder, two pages, a card from another project,
/// labels and guards in both projects, a group, and a labeled link.
fn full_board() -> Board {
    let signature = Signature::of(b"x");
    let mut first = Page::new("page-1", "Placing an order");
    first.default_view = View::Flow;
    first.cards.push(Card {
        at: [-0.5, 1e21],
        view: Some(View::Code),
        folded: true,
        description: Some(unsigned("Quote \" and\nnewline")),
        ..card(
            "z-last",
            CardRef::entrance(
                Key::from_text("entrance:[\"manual\",\"worker\",\"\",\"\",\"x\"]"),
                Some(web()),
            ),
        )
    });
    first.cards.push(Card {
        at: [0.0, 0.0],
        ..run_card("run", None)
    });
    first.groups.push(group(&[IF0]));
    first.labels.push(label(Some(web()), IF0, "Web"));
    first.labels.push(label(None, IF0, "Own"));
    first.guards.push(GuardEntry {
        project: Some(web()),
        key: Key::from_text("entrance:[\"manual\",\"worker\",\"\",\"\",\"x\"]"),
        kind: "entrance:worker".to_owned(),
        owner: String::new(),
        signature,
    });
    first.guards.push(GuardEntry {
        project: None,
        key: Key::from_text(RUN),
        kind: "function".to_owned(),
        owner: RUN.to_owned(),
        signature,
    });
    first
        .links
        .push(link("calls", "z-last", "run", Some("calls over HTTP")));
    let second = Page::new("a-second", "Second");
    Board::with_pages("T", Some("Checkout/Orders".to_owned()), vec![first, second])
}

#[test]
fn serialization_is_canonical_and_round_trips() {
    let board = full_board();
    assert_eq!(board.projects, [web()]);
    let text = serialize(&board);
    let signature = format!("sha256:{}", Signature::of(b"x").0);
    let expected = format!(
        r#"{{
  "folder": "Checkout/Orders",
  "format": "unfold-board",
  "pages": [
    {{
      "cards": [
        {{
          "at": [
            0,
            0
          ],
          "id": "run",
          "ref": {{
            "block": "main.ts#function:run"
          }}
        }},
        {{
          "at": [
            -0.5,
            1e+21
          ],
          "description": {{
            "author": "agent:test",
            "signature": "",
            "text": "Quote \" and\nnewline"
          }},
          "folded": true,
          "id": "z-last",
          "ref": {{
            "entrance": "entrance:[\"manual\",\"worker\",\"\",\"\",\"x\"]",
            "project": "web"
          }},
          "view": "code"
        }}
      ],
      "defaultView": "flow",
      "groups": [
        {{
          "card": "run",
          "collapsed": false,
          "id": "steps",
          "members": [
            "main.ts#function:run/if[0]"
          ],
          "title": "Steps"
        }}
      ],
      "guards": [
        {{
          "key": "main.ts#function:run",
          "kind": "function",
          "owner": "main.ts#function:run",
          "signature": "{signature}"
        }},
        {{
          "key": "entrance:[\"manual\",\"worker\",\"\",\"\",\"x\"]",
          "kind": "entrance:worker",
          "owner": "",
          "project": "web",
          "signature": "{signature}"
        }}
      ],
      "id": "page-1",
      "labels": [
        {{
          "author": "agent:test",
          "key": "main.ts#function:run/if[0]",
          "signature": "",
          "text": "Own"
        }},
        {{
          "author": "agent:test",
          "key": "main.ts#function:run/if[0]",
          "project": "web",
          "signature": "",
          "text": "Web"
        }}
      ],
      "links": [
        {{
          "from": "z-last",
          "id": "calls",
          "label": {{
            "author": "agent:test",
            "signature": "",
            "text": "calls over HTTP"
          }},
          "to": "run"
        }}
      ],
      "title": "Placing an order"
    }},
    {{
      "cards": [],
      "defaultView": "diagram",
      "groups": [],
      "guards": [],
      "id": "a-second",
      "labels": [],
      "links": [],
      "title": "Second"
    }}
  ],
  "projects": [
    "web"
  ],
  "title": "T",
  "version": 1
}}
"#
    );
    assert_eq!(text, expected);
    let reparsed = parse(&text).unwrap();
    assert_eq!(serialize(&reparsed), text);
    assert_eq!(reparsed.pages[0].card("z-last"), board.pages[0].card("z-last"));
    // The writer derives `projects` from the cards, whatever the field holds.
    let mut stale = reparsed.clone();
    stale.projects.clear();
    assert_eq!(serialize(&stale), text);
    // A false fold is the default and is never written.
    let mut unfolded = reparsed;
    unfolded.pages[0]
        .cards
        .iter_mut()
        .for_each(|card| card.folded = false);
    assert!(!serialize(&unfolded).contains("folded"));
    // The schema is the documented one and is valid JSON.
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    assert_eq!(schema["properties"]["format"]["const"], FORMAT);
    let mirror = std::fs::read_to_string(
        unfold_core::testkit::testdata_dir().join("../docs/schemas/unfold-board.schema.json"),
    )
    .unwrap();
    assert_eq!(mirror, SCHEMA, "docs/schemas mirrors the crate's schema");
}

#[test]
fn edits_and_changes_copies_have_their_own_framing() {
    let board = full_board();
    // An edit holds the title and pages only; the folder is the caller's, projects derived.
    let edit = serde_json::json!({ "title": board.title, "pages": board.pages }).to_string();
    let edited = parse_edit(&edit, board.folder.clone()).unwrap();
    assert_eq!(edited, board);
    assert_eq!(parse_edit(&edit, None).unwrap().folder, None);
    let framed = edit.replacen('{', "{\"format\":\"unfold-board\",", 1);
    assert_eq!(
        match parse_edit(&framed, None) {
            Err(BoardError::Invalid { path, .. }) => path,
            other => panic!("{other:?}"),
        },
        "/format"
    );

    // A changes copy names its published board and the revision it started from.
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path(), &dir.path().join(DRAFTS_DIR));
    let published = BoardFile::published("orders").unwrap();
    let copy = BoardFile::changes_of(&published).unwrap();
    assert_eq!(copy.relative(), ".unfold/local/drafts/orders.changes.json");
    assert_eq!(copy.publishes(), Some(published));
    let base = Revision::of(b"published");
    let text = serialize_changes(&board, "orders", base);
    assert!(text.contains(&format!("\"baseRevision\": \"{}\"", base.0)));
    assert!(text.contains("\"publishes\": \"boards/orders.json\""));
    let revision = dirs.save_draft(&copy, &text).unwrap();
    let loaded = dirs.load(&copy).unwrap();
    let canonical = parse(&serialize(&board)).unwrap();
    assert_eq!(
        (loaded.board, loaded.revision, loaded.base),
        (canonical, revision, Some(base))
    );
    // The framing belongs to the copy alone.
    assert!(matches!(parse(&text), Err(BoardError::Invalid { path, .. }) if path == "/baseRevision"));
    let other = BoardFile::changes_of(&BoardFile::published("other").unwrap()).unwrap();
    dirs.save_draft(&other, &text).unwrap();
    assert!(matches!(dirs.load(&other), Err(BoardError::Invalid { path, .. }) if path == "/publishes"));
    dirs.save_draft(&other, &serialize(&board)).unwrap();
    assert!(
        matches!(dirs.load(&other), Err(BoardError::Invalid { path, message }) if path == "/" && message.contains("baseRevision"))
    );
    dirs.save_draft(&copy, &text.replace(&base.0.to_string(), "abc"))
        .unwrap();
    assert!(matches!(dirs.load(&copy), Err(BoardError::Invalid { path, .. }) if path == "/baseRevision"));
}

#[test]
fn descriptions_are_signed_from_verified_source_and_go_outdated() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].cards[0].description = Some(unsigned("Explains run"));
    board.pages[0].labels.push(label(None, FIRST, "Start with one"));
    let (mut board, report) = save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    let card_signature = board.pages[0].cards[0].description.as_ref().unwrap().signature;
    assert_eq!(card_signature, graph.signature(RUN));
    assert_eq!(board.pages[0].labels[0].label.signature, graph.signature(FIRST));

    // The file changes on disk but is not compiled yet: nothing can be signed against it.
    std::fs::write(
        dir.path().join("main.ts"),
        SOURCE.replace("first = 1", "first = 2"),
    )
    .unwrap();
    let stale = BoardGraph::from_graph(&graph_of(SOURCE), dir.path());
    assert_eq!(page_health(&board, &stale).outdated, 2);
    let mut unsigned_card = board.clone();
    unsigned_card.pages[0].cards[0].description = Some(unsigned("x"));
    assert!(matches!(
        save(unsigned_card, Some((&board, &report)), &stale),
        Err(BoardError::Unverified { .. })
    ));

    // After the compile, the named references reconnect and their text is outdated.
    let updated = recompiled(dir.path(), &SOURCE.replace("first = 1", "first = 2"));
    let report = page_health(&board, &updated);
    assert_eq!((report.outdated, report.review, report.missing), (2, 0, 0));
    assert_eq!(report.label().as_deref(), Some("2 outdated"));
    assert!(report.items.iter().all(|item| item.page == "page-1"
        && item.project.is_none()
        && item.repairs.contains(&RepairKind::Resign)));
    // Outdated text never refuses a save.
    assert!(save(board.clone(), Some((&board, &report)), &updated).is_ok());
    repair(
        &mut board,
        &updated,
        &Repair::Resign(Target::Card("run".to_owned())),
    )
    .unwrap();
    repair(&mut board, &updated, &Repair::Resign(own_label(FIRST))).unwrap();
    assert!(page_health(&board, &updated).is_healthy());
    repair(&mut board, &updated, &Repair::RemoveDescription(own_label(FIRST))).unwrap();
    assert!(board.pages[0].labels.is_empty());
}

#[test]
fn an_ordinal_reference_whose_code_changed_is_held_for_review() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].groups.push(group(&[IF0]));
    board.pages[0].labels.push(label(None, IF0, "Log when set"));
    board.pages[0].labels.push(label(None, FIRST, "Start with one"));
    let (saved, report) = save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    let page = &saved.pages[0];
    assert_eq!(page.guard(None, IF0).unwrap().kind, "if");
    assert_eq!(page.guard(None, IF0).unwrap().owner, RUN);
    assert_eq!(page.guards.len(), 3, "the card, the group member, and the labels");

    let inserted = SOURCE.replace(
        "  if (first)",
        "  if (!first) { console.log('inserted'); }\n  if (first)",
    );
    let updated = recompiled(dir.path(), &inserted);
    let report = page_health(&saved, &updated);
    let held: Vec<&HealthItem> = report
        .items
        .iter()
        .filter(|item| item.kind == HealthKind::Review)
        .collect();
    assert_eq!(held.len(), 2, "{:#?}", report.items);
    assert!(
        held.iter()
            .all(|item| item.message.contains("Position-based reference changed"))
    );
    assert!(
        report.items.iter().all(|item| item.target != own_label(FIRST)),
        "a named, unchanged reference stays healthy"
    );
    let mut described = saved.clone();
    described.pages[0].groups[0].description = Some(unsigned("New text"));
    assert!(save(described, Some((&saved, &report)), &updated).is_err());
    // Mermaid never applies the held group or label to the new code.
    let output = crate::mermaid::page(&saved, "page-1", &PageGraphs::own_only(&updated)).unwrap();
    assert!(output.contains("Group needs review: Steps"));
    assert!(!output.contains("Log when set"));
    // Saving with the problem carried over unchanged is allowed; accepting clears it.
    let mut moved = saved.clone();
    moved.pages[0].cards[0].at = [99.0, 99.0];
    let (moved, moved_report) = save(moved, Some((&saved, &report)), &updated).unwrap();
    assert_eq!(moved_report.review, 2);
    assert_eq!(
        moved.pages[0].guard(None, IF0),
        saved.pages[0].guard(None, IF0),
        "guards survive until accepted"
    );
    let mut accepted = moved;
    repair(
        &mut accepted,
        &updated,
        &Repair::Accept(ProjectKey::new(None, IF0)),
    )
    .unwrap();
    let after = page_health(&accepted, &updated);
    assert_eq!(after.review, 0);
    assert_eq!(after.outdated, 1, "the accepted label must still be re-signed");
}

#[test]
fn groups_are_consecutive_siblings_in_one_body() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let then_statement = "main.ts#function:run/if[0]/expression_statement[0]";
    let cases: [(&[&str], Option<&str>); 6] = [
        (&[FIRST, IF0], None),
        (&[FIRST, LOG0], Some("consecutive siblings")),
        (&[LOG0, IF0], Some("consecutive siblings")),
        (&[IF0, then_statement], Some("siblings in the same body")),
        (&[RUN], Some("inside their card's body")),
        (&["main.ts#function:run/nope[0]"], Some("missing block")),
    ];
    for (members, problem) in cases {
        board.pages[0].groups = vec![group(members)];
        let page = &board.pages[0];
        let found = group_problem(page, &graph, &page.groups[0]);
        match problem {
            Some(expected) => assert!(
                found.is_some_and(|text| text.contains(expected)),
                "{members:?}: {found:?}"
            ),
            None => assert_eq!(found, None, "{members:?}"),
        }
    }
    // Overlapping and nested groups are broken, and a save that adds them is refused.
    board.pages[0].groups = vec![
        group(&[FIRST, IF0]),
        Group {
            id: "inner".to_owned(),
            ..group(&[then_statement])
        },
    ];
    assert_eq!(page_health(&board, &graph).broken_groups, 2);
    assert!(matches!(
        save(board.clone(), None, &graph),
        Err(BoardError::Problem { page, target: Target::Group(_), .. }) if page == "page-1"
    ));
    repair(&mut board, &graph, &Repair::Ungroup("inner".to_owned())).unwrap();
    assert!(page_health(&board, &graph).is_healthy());
    // A group on an entrance card is broken.
    board.pages[0].cards.push(card(
        "entrance",
        CardRef::entrance(Key::from_text("entrance:none"), None),
    ));
    let entrance_group = Group {
        card: "entrance".to_owned(),
        ..group(&[FIRST])
    };
    assert_eq!(
        group_problem(&board.pages[0], &graph, &entrance_group),
        Some("Group must belong to a block card")
    );
}

#[test]
fn missing_references_are_placeholders_with_repairs() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0]
        .labels
        .push(label(None, RETURN0, "Hand back first"));
    let (saved, _) = save(board, None, &graph).unwrap();
    let renamed = SOURCE.replace("function run", "function start");
    let updated = recompiled(dir.path(), &renamed);
    let report = page_health(&saved, &updated);
    assert_eq!(report.missing, 2);
    let card_item = report
        .items
        .iter()
        .find(|item| item.target == Target::Card("run".to_owned()))
        .unwrap();
    assert_eq!(card_item.repairs, [RepairKind::Reconnect, RepairKind::RemoveCard]);
    // A position update keeps the placeholder; a new missing reference is refused.
    let mut moved = saved.clone();
    moved.pages[0].cards[0].at = [1.0, 2.0];
    assert!(save(moved.clone(), Some((&saved, &report)), &updated).is_ok());
    let new = Card {
        id: "new".to_owned(),
        reference: CardRef::block(Key::from_text("main.ts#function:nope"), None),
        ..moved.pages[0].cards[0].clone()
    };
    moved.pages[0].cards.push(new);
    assert!(matches!(
        save(moved, Some((&saved, &report)), &updated),
        Err(BoardError::Problem { target: Target::Card(id), .. }) if id == "new"
    ));
    // The manual repair after a rename.
    let mut repaired = saved;
    for (from, to) in [
        (RUN, "main.ts#function:start"),
        (RETURN0, "main.ts#function:start/return[0]"),
    ] {
        repair(
            &mut repaired,
            &updated,
            &Repair::Reconnect {
                project: None,
                from: from.to_owned(),
                to: to.to_owned(),
            },
        )
        .unwrap();
    }
    let report = page_health(&repaired, &updated);
    assert_eq!(report.missing, 0, "{:#?}", report.items);
    let page = &repaired.pages[0];
    assert!(page.guard(None, "main.ts#function:start").is_some());
    assert!(page.guard(None, RUN).is_none());
    assert_eq!(
        page.label(None, "main.ts#function:start/return[0]"),
        Some("Hand back first")
    );
    assert!(
        repair(
            &mut repaired,
            &updated,
            &Repair::Reconnect {
                project: None,
                from: "a".to_owned(),
                to: "main.ts#nope".to_owned()
            }
        )
        .is_err()
    );
    repair(&mut repaired, &updated, &Repair::RemoveCard("run".to_owned())).unwrap();
    assert!(repaired.pages[0].cards.is_empty());
}

#[test]
fn reconnecting_onto_a_member_never_repeats_it() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].groups.push(group(&[FIRST, IF0]));
    board.pages[0].labels.push(label(None, FIRST, "From"));
    board.pages[0].labels.push(label(None, IF0, "To"));
    repair(
        &mut board,
        &graph,
        &Repair::Reconnect {
            project: None,
            from: FIRST.to_owned(),
            to: IF0.to_owned(),
        },
    )
    .unwrap();
    assert_eq!(board.pages[0].groups[0].members, [Key::from_text(IF0)]);
    assert_eq!(board.pages[0].labels.len(), 1, "labels stay unique per key");
    assert_eq!(board.pages[0].label(None, IF0), Some("From"));
    assert_eq!(
        parse(&serialize(&board)).unwrap(),
        board,
        "the board still reads back"
    );
}

#[test]
fn saving_signs_restores_dropped_guards_and_drops_unused_ones() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].cards[0].description = Some(unsigned("Unsigned until saved"));
    board.pages[0].groups.push(group(&[FIRST, IF0]));
    board.pages[0].groups[0].description = Some(unsigned("Set up"));
    let (saved, report) = save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    let page = &saved.pages[0];
    assert_eq!(
        page.cards[0].description.as_ref().unwrap().signature,
        graph.signature(RUN)
    );
    assert_eq!(
        page.groups[0].description.as_ref().unwrap().signature,
        graph.span_signature(FIRST, IF0)
    );
    let mut edited = saved.clone();
    edited.pages[0].guards = vec![GuardEntry {
        project: None,
        key: Key::from_text("main.ts#unused"),
        kind: "if".to_owned(),
        owner: String::new(),
        signature: Signature::of(b"x"),
    }];
    edited.pages[0].title = "Retitled".to_owned();
    let (again, _) = save(edited, Some((&saved, &report)), &graph).unwrap();
    assert_eq!(again.pages[0].guards, saved.pages[0].guards);
    assert_eq!(
        serialize(&again),
        serialize(&saved).replace("\"Page 1\"", "\"Retitled\"")
    );
}

#[test]
fn saving_carries_new_items_and_never_writes_a_dangling_reference() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let page = &mut board.pages[0];
    page.resources.push(Resource {
        id: "store".to_owned(),
        kind: ResourceKind::Storage,
        name: "uploads".to_owned(),
        project: None,
        type_key: None,
        columns: Vec::new(),
        anchors: vec![Anchor::Key {
            key: Key::from_text(RUN),
            project: None,
        }],
        at: [400.0, 0.0],
        description: Some(unsigned("Where uploads go")),
        author: Author::person("tester").unwrap(),
        signature: None,
    });
    page.frames.push(Frame {
        id: "unit".to_owned(),
        title: "Unit".to_owned(),
        description: Some(unsigned("The runner")),
        members: vec!["run".to_owned()],
        look: FrameLook::Box,
        icon: None,
        direction: None,
        collapsed: false,
        author: author(),
    });
    page.links.push(Link {
        kind: Some(LinkKind::Writes),
        author: Some(author()),
        ..link("writes", "run", "store", Some("uploads"))
    });
    page.links.push(link("to-unit", "store", "unit", None));
    let (saved, health) = save(board.clone(), None, &graph).unwrap();
    assert!(
        health.is_healthy(),
        "links to resources and frames aren't broken: {health:?}"
    );
    let page = &saved.pages[0];
    // The claims are signed: the resource card and its description over its anchor's bytes, the
    // frame's description over its member card's signature.
    let run_bytes = format!(
        "{}\n",
        &SOURCE[SOURCE.find("function").unwrap()..SOURCE.rfind('}').unwrap() + 1]
    );
    let resource = Signature::of(run_bytes.as_bytes());
    assert_eq!(page.resources[0].signature, Some(resource));
    assert_eq!(
        page.resources[0].description.as_ref().unwrap().signature,
        Some(resource)
    );
    let members = graph.signature(RUN).unwrap().0.to_string();
    assert_eq!(
        page.frames[0].description.as_ref().unwrap().signature,
        Some(Signature::of(members.as_bytes()))
    );
    assert_eq!(
        page.link("writes").unwrap().label.as_ref().unwrap().signature,
        None,
        "a label with a resource end stays unsigned"
    );

    // Rust's own edits are checked like a parsed file before anything is written.
    let mut dangling = saved.clone();
    dangling.pages[0].cards.clear();
    dangling.pages[0].groups.clear();
    assert_eq!(
        save(dangling, Some((&saved, &health)), &graph).unwrap_err(),
        BoardError::Invalid {
            path: "/pages/0/frames/0/members/0".to_owned(),
            message: "refers to an unknown item `run`".to_owned()
        }
    );
}

#[test]
fn saving_reuses_unchanged_pages_and_refreshes_projects() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages.push(Page::new("page-2", "Second"));
    let (saved, report) = save(board, None, &graph).unwrap();
    // An unchanged page is not checked again: its health (even a stale one) is kept as is.
    let mut stale = report;
    stale.append(Health {
        items: vec![HealthItem {
            page: "page-1".to_owned(),
            kind: HealthKind::Outdated,
            target: Target::Card("run".to_owned()),
            project: None,
            key: None,
            anchor: None,
            message: "kept".to_owned(),
            repairs: Vec::new(),
        }],
        ..Health::default()
    });
    let mut edited = saved.clone();
    edited.pages[1].title = "Second, retitled".to_owned();
    edited.pages[1].cards.push(run_card("web-run", Some(web())));
    edited.projects.clear();
    let (again, again_report) = save(edited, Some((&saved, &stale)), &graph).unwrap();
    assert_eq!(again.projects, [web()], "projects are derived again");
    assert_eq!(again_report.outdated, 1, "page-1's health was reused");
    assert_eq!(again_report.not_connected, 1, "page-2 was checked");
    assert_eq!(again_report.of_page("page-2").items.len(), 1);
    assert_eq!(
        again_report.label().as_deref(),
        Some("1 outdated · 1 not connected")
    );
}

/// Two projects: the board's own (`main.ts` in one folder) and `web` (the same source in
/// another folder, so the same keys exist in both).
struct TwoProjects {
    _own_dir: tempfile::TempDir,
    web_dir: tempfile::TempDir,
    own: BoardGraph,
    others: BTreeMap<ProjectName, BoardGraph>,
}

impl TwoProjects {
    fn new() -> Self {
        let (own_dir, own) = folder(SOURCE);
        let (web_dir, web_graph) = folder(SOURCE);
        Self {
            _own_dir: own_dir,
            web_dir,
            own,
            others: BTreeMap::from([(web(), web_graph)]),
        }
    }

    fn graphs(&self) -> PageGraphs<'_> {
        PageGraphs::new(&self.own, &self.others)
    }
}

#[test]
fn cards_from_other_projects_are_checked_against_their_own_graph() {
    let mut projects = TwoProjects::new();
    let gone = ProjectName::parse("gone").unwrap();
    let mut board = sample_board();
    let page = &mut board.pages[0];
    page.cards.push(run_card("web-run", Some(web())));
    page.cards.push(Card {
        description: Some(unsigned("Cannot be signed yet")),
        ..run_card("gone-run", Some(gone.clone()))
    });
    page.groups.push(Group {
        id: "gone-steps".to_owned(),
        card: "gone-run".to_owned(),
        ..group(&[RUN])
    });
    page.labels.push(label(Some(web()), FIRST, "Web first"));
    page.labels.push(label(Some(gone.clone()), FIRST, "Gone first"));
    page.guards.push(GuardEntry {
        project: Some(gone.clone()),
        key: Key::from_text(RUN),
        kind: "function".to_owned(),
        owner: RUN.to_owned(),
        signature: Signature::of(b"elsewhere"),
    });
    board.refresh_projects();
    assert_eq!(board.projects, [gone.clone(), web()]);
    assert_eq!(board.references(Some(&web())).keys, [RUN, FIRST]);
    assert_eq!(board.references(None).keys, [RUN]);

    // A project that is not connected is never a save error: its cards are placeholders, and
    // its text, groups, and guards are kept unchecked.
    let (saved, report) = prepare_save(board, None, &projects.graphs()).unwrap();
    assert_eq!(report.not_connected, 1, "{:#?}", report.items);
    assert_eq!(report.items.len(), 1);
    let item = &report.items[0];
    assert_eq!(
        (
            item.page.as_str(),
            &item.target,
            item.project.as_ref(),
            item.message.as_str()
        ),
        (
            "page-1",
            &Target::Card("gone-run".to_owned()),
            Some(&gone),
            "From gone, not connected"
        )
    );
    assert_eq!(item.repairs, [RepairKind::RemoveCard]);
    assert_eq!(report.label().as_deref(), Some("1 not connected"));
    let page = &saved.pages[0];
    assert_eq!(page.guards.len(), 4, "own run, web run and first, gone run");
    assert_eq!(
        page.guard(Some(&gone), RUN).unwrap().signature,
        Signature::of(b"elsewhere"),
        "a not-connected project's guard is kept as it is"
    );
    assert_eq!(
        page.label_entry(Some(&web()), FIRST).unwrap().label.signature,
        projects.others[&web()].signature(FIRST)
    );
    assert_eq!(
        page.label_entry(Some(&gone), FIRST).unwrap().label.signature,
        None
    );
    assert_eq!(
        page.card("gone-run")
            .unwrap()
            .description
            .as_ref()
            .unwrap()
            .signature,
        None
    );

    // A change in `web` outdates web's text only; the board's own project is unaffected.
    let changed = SOURCE.replace("first = 1", "first = 2");
    projects
        .others
        .insert(web(), recompiled(projects.web_dir.path(), &changed));
    let report = health(&saved.pages[0], &projects.graphs());
    let outdated: Vec<(&Target, Option<&ProjectName>)> = report
        .items
        .iter()
        .filter(|item| item.kind == HealthKind::Outdated)
        .map(|item| (&item.target, item.project.as_ref()))
        .collect();
    let web_label = Target::Label(ProjectKey::new(Some(&web()), FIRST));
    assert_eq!(outdated, [(&web_label, Some(&web()))]);

    // A new missing reference in a connected project is refused like the board's own.
    let mut edited = saved.clone();
    edited.pages[0].cards.push(card(
        "web-nope",
        CardRef::block(Key::from_text("main.ts#function:nope"), Some(web())),
    ));
    assert!(matches!(
        prepare_save(edited, Some((&saved, &report)), &projects.graphs()),
        Err(BoardError::Problem { target: Target::Card(id), .. }) if id == "web-nope"
    ));
    // Reconnecting needs the project connected.
    let mut board = saved.clone();
    let page = board.page_mut("page-1").unwrap();
    assert!(
        apply_repair(
            page,
            &projects.graphs(),
            &Repair::Reconnect {
                project: Some(gone.clone()),
                from: RUN.to_owned(),
                to: FIRST.to_owned(),
            }
        )
        .is_err()
    );
    // Removing the placeholder drops the project from the board.
    apply_repair(
        page,
        &projects.graphs(),
        &Repair::RemoveCard("gone-run".to_owned()),
    )
    .unwrap();
    assert!(page.groups.iter().all(|group| group.card != "gone-run"));
    let (board, _) = prepare_save(board, Some((&saved, &report)), &projects.graphs()).unwrap();
    assert_eq!(board.projects, [web()]);
    assert!(
        board.pages[0].guard(Some(&gone), RUN).is_none(),
        "nothing references it any more"
    );

    // Mermaid draws the placeholder.
    let output = crate::mermaid::page(&saved, "page-1", &projects.graphs()).unwrap();
    assert!(
        output.contains("c2[\"From gone, not connected\"]\n  class c2 opaque"),
        "{output}"
    );
    assert!(output.contains("%% notConnected: From gone, not connected"));
    assert!(crate::mermaid::page(&saved, "nope", &projects.graphs()).is_none());
}

#[test]
fn drawn_links_are_signed_over_their_ends_and_break_with_them() {
    let projects = TwoProjects::new();
    let mut board = sample_board();
    let page = &mut board.pages[0];
    page.default_view = View::Code;
    page.cards.push(run_card("web-run", Some(web())));
    page.cards
        .push(run_card("gone-run", Some(ProjectName::parse("gone").unwrap())));
    page.links
        .push(link("calls", "web-run", "run", Some("calls over HTTP")));
    page.links
        .push(link("to-gone", "run", "gone-run", Some("hands off")));
    page.links.push(link("plain", "run", "web-run", None));
    board.refresh_projects();
    let (saved, report) = prepare_save(board, None, &projects.graphs()).unwrap();
    assert_eq!(report.broken_links, 0, "a not-connected end is not missing");
    let page = &saved.pages[0];
    let hex = |project: Option<&ProjectName>| page.guard(project, RUN).unwrap().signature.0;
    let expected = Signature::of(format!("{}\n{}", hex(Some(&web())), hex(None)).as_bytes());
    assert_eq!(
        page.link("calls").unwrap().label.as_ref().unwrap().signature,
        Some(expected)
    );
    assert_eq!(
        page.link("to-gone").unwrap().label.as_ref().unwrap().signature,
        None,
        "an end without a guard leaves the label unsigned"
    );

    // Mermaid draws drawn links as dashed arrows with their label.
    let output = crate::mermaid::page(&saved, "page-1", &projects.graphs()).unwrap();
    assert!(output.contains("  c1 -.->|\"calls over HTTP\"| c0\n"), "{output}");
    assert!(output.contains("  c0 -.-> c1\n"), "{output}");
    assert!(output.contains("  c0 -.->|\"hands off\"| c2\n"), "{output}");

    // The outline follows drawn links across projects.
    let rows: Vec<(String, u32)> = outline(page, &projects.graphs())
        .rows
        .into_iter()
        .map(|row| (row.card, row.depth))
        .collect();
    assert_eq!(
        rows,
        [
            ("run".to_owned(), 0),
            ("web-run".to_owned(), 1),
            ("gone-run".to_owned(), 1)
        ]
    );

    // A code change never outdates a link label; a missing end breaks the link.
    let renamed = SOURCE.replace("function run", "function start");
    let others = BTreeMap::from([(web(), recompiled(projects.web_dir.path(), &renamed))]);
    let graphs = PageGraphs::new(&projects.own, &others);
    let report = health(page, &graphs);
    assert_eq!(
        (report.missing, report.broken_links, report.outdated),
        (1, 2, 0),
        "{:#?}",
        report.items
    );
    let broken = report
        .items
        .iter()
        .find(|item| item.target == Target::Link("calls".to_owned()))
        .unwrap();
    assert_eq!(broken.message, "Linked card web-run is missing");
    assert_eq!(broken.repairs, [RepairKind::RemoveLink]);
    assert_eq!(
        report.label().as_deref(),
        Some("1 missing · 1 not connected · 2 broken links")
    );
    // A new link to the missing card is refused; carried broken links are kept.
    let mut edited = saved.clone();
    edited.pages[0]
        .links
        .push(link("again", "web-run", "gone-run", None));
    assert!(matches!(
        prepare_save(edited, Some((&saved, &report)), &graphs),
        Err(BoardError::Problem { target: Target::Link(id), .. }) if id == "again"
    ));
    let mut moved = saved.clone();
    moved.pages[0].cards[0].at = [5.0, 5.0];
    assert!(prepare_save(moved, Some((&saved, &report)), &graphs).is_ok());
    // Link repairs.
    let mut page = saved.pages[0].clone();
    apply_repair(&mut page, &graphs, &Repair::RemoveLink("calls".to_owned())).unwrap();
    apply_repair(
        &mut page,
        &graphs,
        &Repair::RemoveDescription(Target::Link("to-gone".to_owned())),
    )
    .unwrap();
    assert_eq!(page.link("to-gone").unwrap().label, None);
    apply_repair(&mut page, &graphs, &Repair::RemoveCard("web-run".to_owned())).unwrap();
    assert_eq!(
        page.links.iter().map(|link| link.id.as_str()).collect::<Vec<_>>(),
        ["to-gone"]
    );
    let mut resigned = saved.pages[0].clone();
    resigned.links[0].label.as_mut().unwrap().signature = None;
    let target = Target::Link(resigned.links[0].id.clone());
    apply_repair(&mut resigned, &projects.graphs(), &Repair::Resign(target)).unwrap();
    assert_eq!(resigned, saved.pages[0]);
}

#[test]
fn the_outline_follows_calls_and_counts_unlabeled_nodes() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let graphs = PageGraphs::own_only(&graph);
    let empty = outline(&board.pages[0], &graphs);
    assert_eq!(empty.page, "page-1");
    assert_eq!(
        empty.rows,
        [OutlineRow {
            card: "run".to_owned(),
            depth: 0
        }]
    );
    let unlabeled = empty.unlabeled.len();
    assert_eq!(unlabeled, 5, "first, if, its then statement, the log, the return");
    board.pages[0].labels.push(label(None, FIRST, "Start with one"));
    assert_eq!(outline(&board.pages[0], &graphs).unlabeled.len(), unlabeled - 1);
    // A label in another project never names this project's node.
    board.pages[0].labels.push(label(Some(web()), IF0, "Elsewhere"));
    assert_eq!(outline(&board.pages[0], &graphs).unlabeled.len(), unlabeled - 1);
    assert!(card_links(&board.pages[0], &graphs).is_empty());
}

#[test]
fn steps_a_collapsed_group_hides_are_not_unlabeled() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let graphs = PageGraphs::own_only(&graph);
    let keys = |board: &Board| -> Vec<String> {
        let outline = outline(&board.pages[0], &graphs);
        outline
            .unlabeled
            .iter()
            .map(|item| item.key.as_str().to_owned())
            .collect()
    };
    let all = keys(&board);
    // Expanded, a group hides nothing.
    board.pages[0].groups.push(group(&[IF0, LOG0]));
    assert_eq!(keys(&board), all);
    // Collapsed, it hides its members and the steps under them (the if's then statement); the
    // return still counts.
    board.pages[0].groups[0].collapsed = true;
    assert_eq!(keys(&board), [FIRST, RETURN0]);
    // A group of another card hides nothing on this one.
    board.pages[0].groups[0].card = "other".to_owned();
    assert_eq!(keys(&board), all);
}

#[test]
fn changes_count_structural_differences() {
    let published = full_board();
    assert_eq!(changes(&published, &published), 0);
    let mut working = published.clone();
    working.title = "Retitled".to_owned();
    working.folder = None;
    assert_eq!(changes(&working, &published), 2);

    let mut working = published.clone();
    working.pages.reverse();
    assert_eq!(changes(&working, &published), 1, "page order");
    working.pages.push(Page::new("page-3", "Third"));
    working.pages.retain(|page| page.id != "a-second");
    assert_eq!(
        changes(&working, &published),
        2,
        "one added, one removed; order alone is equal"
    );

    let mut working = published.clone();
    let page = &mut working.pages[0];
    page.title = "New title".to_owned();
    page.default_view = View::Code;
    page.cards[1].at = [1.0, 1.0];
    page.cards.push(run_card("added", None));
    page.groups.clear();
    page.links[0].label = None;
    page.labels[0].label.text = "Changed".to_owned();
    page.labels.push(label(Some(web()), FIRST, "New label"));
    page.guards.clear();
    page.cards[0].description.as_mut().unwrap().signature = Some(Signature::of(b"new"));
    // title, view, card changed, card added, signature on a card (a changed card), group
    // removed, link changed, label changed, label added; guards don't count.
    assert_eq!(changes(&working, &published), 9);
    assert_eq!(changes(&published, &working), 9, "symmetric");
}

#[test]
fn board_files_publish_once_and_write_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let dirs = BoardDirs::new(root, &root.join(DRAFTS_DIR));
    let board = parse(MINIMAL).unwrap();
    let text = serialize(&board);

    let draft = BoardFile::draft("7f3c2a").unwrap();
    assert_eq!(draft.relative(), ".unfold/local/drafts/7f3c2a.json");
    let revision = dirs.save_draft(&draft, &text).unwrap();
    assert!(
        !root.join(".unfold/boards").exists(),
        "a draft writes nothing to share"
    );
    let loaded = dirs.load(&draft).unwrap();
    assert_eq!(
        (loaded.board.clone(), loaded.revision, loaded.base),
        (board, revision, None)
    );
    assert_eq!(dirs.change(&draft, revision).unwrap(), Change::Unchanged);

    // An external edit is detected.
    std::fs::write(dirs.path(&draft), text.replace("\"T\"", "\"Edited\"")).unwrap();
    assert!(matches!(
        dirs.change(&draft, revision).unwrap(),
        Change::Changed(_)
    ));

    let name = slug("  Order placement: v2!  ");
    assert_eq!(name, "order-placement-v2");
    let (published, published_revision) = dirs.create_published(&name, &text).unwrap();
    assert_eq!(published.relative(), ".unfold/boards/order-placement-v2.json");
    assert_eq!(std::fs::read_to_string(dirs.path(&published)).unwrap(), text);
    assert!(
        dirs.save_draft(&published, &text).is_err(),
        "published files are written only by Publish"
    );

    // Publish never overwrites; the collision names the file and offers a free name.
    let other = text.replace("\"T\"", "\"Other\"");
    match dirs.create_published(&name, &other) {
        Err(BoardError::NameTaken { path, suggestion }) => {
            assert_eq!(path, ".unfold/boards/order-placement-v2.json");
            assert_eq!(suggestion, "order-placement-v2-2");
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(
        dirs.change(&published, published_revision).unwrap(),
        Change::Unchanged
    );

    // A changes copy is published over the file on purpose, and recreates a deleted one.
    let copy = BoardFile::changes_of(&published).unwrap();
    assert_eq!(BoardFile::changes_of(&draft), None);
    dirs.save_draft(
        &copy,
        &serialize_changes(&parse(&other).unwrap(), &name, published_revision),
    )
    .unwrap();
    assert_eq!(dirs.changed().unwrap(), std::slice::from_ref(&published));
    let overwritten = dirs.overwrite_published(&published, &other).unwrap();
    assert_eq!(overwritten, Revision::of(other.as_bytes()));
    assert_eq!(std::fs::read_to_string(dirs.path(&published)).unwrap(), other);
    dirs.remove(&published).unwrap();
    dirs.overwrite_published(&published, &text).unwrap();
    assert_eq!(dirs.list(Place::Published).unwrap(), [published]);
    assert!(dirs.overwrite_published(&draft, &text).is_err());

    // Drafts never list changes copies, and never take a copy's name.
    assert_eq!(dirs.list(Place::Draft).unwrap(), std::slice::from_ref(&draft));
    assert!(matches!(
        BoardFile::draft("orders.changes"),
        Err(BoardError::InvalidName { .. })
    ));
    assert_eq!(dirs.free_draft("7f3c2a").unwrap().name, "7f3c2a-2");
    assert_eq!(dirs.free_draft("x.changes").unwrap().name, "x");
    assert_eq!(BoardFile::draft("x").unwrap().publishes(), None);
    dirs.remove(&copy).unwrap();
    assert!(dirs.changed().unwrap().is_empty());
    dirs.remove(&draft).unwrap();
    dirs.remove(&draft).unwrap();
    assert!(dirs.list(Place::Draft).unwrap().is_empty());

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
            matches!(BoardFile::published(bad), Err(BoardError::InvalidName { .. })),
            "{bad}"
        );
    }
    assert_eq!(slug("¿¿"), "board");
    assert_eq!(slug("Café Crème"), "cafe-creme");
}

#[cfg(unix)]
#[test]
fn board_directories_must_not_be_links() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path(), &dir.path().join(DRAFTS_DIR));
    std::fs::create_dir(dir.path().join(".unfold")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".unfold/boards")).unwrap();
    let text = serialize(&parse(MINIMAL).unwrap());
    assert!(matches!(
        dirs.create_published("x", &text),
        Err(BoardError::Io { .. })
    ));
    let file = BoardFile::published("x").unwrap();
    assert!(matches!(
        dirs.overwrite_published(&file, &text),
        Err(BoardError::Io { .. })
    ));
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());

    // A board-looking file outside the folder is never read, listed, or deleted through the
    // link (a cloned repository can commit `.unfold/boards` as a link).
    let victim = outside.path().join("x.json");
    std::fs::write(&victim, &text).unwrap();
    let refused = |result: Result<(), BoardError>| matches!(result, Err(BoardError::Io { .. }));
    assert!(refused(dirs.remove(&file)));
    assert!(refused(dirs.load(&file).map(drop)));
    assert!(refused(dirs.change(&file, Revision::of(b"")).map(drop)));
    assert!(refused(dirs.list(Place::Published).map(drop)));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);

    // A board file that is itself a link is refused for reading; removing it removes the link.
    std::fs::remove_file(dir.path().join(".unfold/boards")).unwrap();
    std::fs::create_dir(dir.path().join(".unfold/boards")).unwrap();
    std::os::unix::fs::symlink(&victim, dirs.path(&file)).unwrap();
    assert!(refused(dirs.load(&file).map(drop)));
    assert!(refused(dirs.change(&file, Revision::of(b"")).map(drop)));
    dirs.remove(&file).unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);

    // The drafts directory must be a real directory too.
    let linked = BoardDirs::new(dir.path(), &dir.path().join("drafts-link"));
    std::os::unix::fs::symlink(outside.path(), dir.path().join("drafts-link")).unwrap();
    let draft = BoardFile::draft("d").unwrap();
    assert!(matches!(
        linked.save_draft(&draft, &text),
        Err(BoardError::Io { .. })
    ));
    assert!(refused(linked.list(Place::Draft).map(drop)));
}

#[test]
fn free_names_stay_valid_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path(), &dir.path().join(DRAFTS_DIR));
    let text = serialize(&parse(MINIMAL).unwrap());
    // A taken name at the length limit still gets a valid alternative (this used to loop
    // forever, because every suffixed candidate was too long).
    let long = "a".repeat(100);
    dirs.create_published(&long, &text).unwrap();
    match dirs.create_published(&long, &text) {
        Err(BoardError::NameTaken { suggestion, .. }) => {
            assert_eq!(suggestion, format!("{}-2", "a".repeat(98)));
            assert!(BoardFile::published(&suggestion).is_ok());
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(dirs.free_name("fresh"), "fresh");
}

#[test]
fn board_names_avoid_reserved_device_names() {
    for reserved in ["con", "CON", "aux.v2", "nul", "com1", "LPT9"] {
        assert!(
            matches!(
                BoardFile::published(reserved),
                Err(BoardError::InvalidName { .. })
            ),
            "{reserved}"
        );
    }
    for fine in ["console", "com0", "com10", "auxiliary", "board"] {
        assert!(BoardFile::published(fine).is_ok(), "{fine}");
    }
    assert_eq!(slug("Con"), "con-board");
    assert!(BoardFile::published(&slug("AUX")).is_ok());
}

#[test]
fn oversized_board_files_are_refused_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BoardDirs::new(dir.path(), &dir.path().join(DRAFTS_DIR));
    let file = BoardFile::draft("big").unwrap();
    dirs.save_draft(&file, &serialize(&parse(MINIMAL).unwrap()))
        .unwrap();
    let handle = std::fs::OpenOptions::new()
        .write(true)
        .open(dirs.path(&file))
        .unwrap();
    handle.set_len(MAX_BOARD_BYTES + 1).unwrap();
    assert!(matches!(dirs.load(&file), Err(BoardError::Io { .. })));
}

#[test]
fn health_labels_name_every_kind() {
    let mut health = Health::default();
    assert_eq!(health.label(), None);
    for (kind, count) in [
        (HealthKind::Missing, 1),
        (HealthKind::Outdated, 2),
        (HealthKind::BrokenGroup, 1),
        (HealthKind::Review, 3),
        (HealthKind::NotConnected, 2),
        (HealthKind::BrokenLink, 1),
    ] {
        for index in 0..count {
            health.append(Health {
                items: vec![HealthItem {
                    page: "p".to_owned(),
                    kind,
                    target: Target::Card(format!("{kind:?}-{index}")),
                    project: None,
                    key: None,
                    anchor: None,
                    message: String::new(),
                    repairs: Vec::new(),
                }],
                ..Health::default()
            });
        }
    }
    assert_eq!(
        health.label().as_deref(),
        Some("1 missing · 2 outdated · 1 broken group · 3 to review · 2 not connected · 1 broken link")
    );
    assert_eq!(health.of_page("p"), health);
    assert!(health.of_page("q").is_healthy());
}

/// The call of the `console.log('done');` statement in `SOURCE`.
const LOG_CALL: &str = "main.ts#function:run/expression_statement[0]/call[0]";

/// A proposed step on card `run`, going after `after`.
fn proposal(id: &str, after: &str) -> Proposal {
    Proposal {
        id: id.to_owned(),
        card: "run".to_owned(),
        after: Key::from_text(after),
        path: None,
        title: "Email a confirmation".to_owned(),
        reason: None,
        author: author(),
    }
}

/// Proposed step `email` on card `run`, going after `if (first)` on its yes path.
fn email() -> Proposal {
    Proposal {
        path: Some(ProposalPath::Yes),
        ..proposal("email", IF0)
    }
}

/// Saving `board` over `previous` refuses proposal `id` with a message containing `expected`.
fn refuses_proposal(
    board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
    id: &str,
    expected: &str,
) {
    match save(board, previous, graph) {
        Err(BoardError::Problem { target, message, .. }) => {
            assert_eq!(target, Target::Proposal(id.to_owned()), "{message}");
            assert!(message.contains(expected), "{expected:?} in {message:?}");
        }
        other => panic!("expected `{expected}`, got {other:?}"),
    }
}

#[test]
fn old_boards_read_as_before_and_new_pages_show_diagram() {
    // A board without expanded calls or proposals reads and writes out exactly as before.
    let canonical = serialize(&parse(MINIMAL).unwrap());
    assert!(!canonical.contains("expanded") && !canonical.contains("proposals"));
    assert_eq!(serialize(&parse(&canonical).unwrap()), canonical);
    // A saved default view is kept as written; only new pages start in Diagram.
    for view in ["code", "flow", "diagram"] {
        let text = MINIMAL.replace("\"defaultView\":\"code\"", &format!("\"defaultView\":\"{view}\""));
        let board = parse(&text).unwrap();
        assert_eq!(board.pages[0].default_view.as_str(), view);
        assert!(serialize(&board).contains(&format!("\"defaultView\": \"{view}\"")));
    }
    assert_eq!(Page::new("p", "P").default_view, View::Diagram);
    assert_eq!(Board::new("B").pages[0].default_view, View::Diagram);
}

#[test]
fn expanded_calls_and_proposals_round_trip() {
    let mut board = sample_board();
    let page = &mut board.pages[0];
    page.cards[0].expanded = vec![
        vec![Key::from_text(LOG_CALL)],
        vec![
            Key::from_text("main.ts#function:run/if[0]/expression_statement[0]/call[0]"),
            Key::from_text("lib.ts#function:log/call[0]"),
        ],
    ];
    page.proposals.push(Proposal {
        path: Some(ProposalPath::No),
        reason: Some("Direct orders get no confirmation \"yet\"".to_owned()),
        ..proposal("email", IF0)
    });
    page.proposals.push(proposal("audit", LOG0));
    let text = serialize(&board);
    let card = r#"
        {
          "at": [
            10,
            20
          ],
          "expanded": [
            [
              "main.ts#function:run/expression_statement[0]/call[0]"
            ],
            [
              "main.ts#function:run/if[0]/expression_statement[0]/call[0]",
              "lib.ts#function:log/call[0]"
            ]
          ],
          "id": "run",
          "ref": {
            "block": "main.ts#function:run"
          }
        }"#;
    let proposals = r#"
      "proposals": [
        {
          "after": "main.ts#function:run/expression_statement[0]",
          "author": "agent:test",
          "card": "run",
          "id": "audit",
          "title": "Email a confirmation"
        },
        {
          "after": "main.ts#function:run/if[0]",
          "author": "agent:test",
          "card": "run",
          "id": "email",
          "path": "no",
          "reason": "Direct orders get no confirmation \"yet\"",
          "title": "Email a confirmation"
        }
      ],
      "title": "Page 1""#;
    assert!(text.contains(card), "{text}");
    assert!(text.contains(proposals), "{text}");
    let reparsed = parse(&text).unwrap();
    let mut sorted = board.clone();
    sorted.pages[0].proposals.reverse();
    assert_eq!(reparsed, sorted, "proposals read back in id order");
    assert_eq!(serialize(&reparsed), text);
    // What the window receives has the file's shape.
    let ipc = serde_json::to_value(&board.pages[0]).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        ipc["cards"][0]["expanded"],
        file["pages"][0]["cards"][0]["expanded"]
    );
    let mut by_id = ipc["proposals"].as_array().unwrap().clone();
    by_id.reverse();
    assert_eq!(serde_json::Value::Array(by_id), file["pages"][0]["proposals"]);
    // Neither is a reference the page keeps a guard for, except each proposal's anchor.
    assert_eq!(board.pages[0].references(None).keys, [RUN, IF0, LOG0]);
}

/// `MINIMAL` with entrance card `door` before card `run`, `card` added to `run`'s fields, and
/// the page's `proposals`.
fn with_proposals(card: &str, proposals: &str) -> String {
    MINIMAL
        .replace("\"id\":\"run\"", &format!("\"id\":\"run\"{card}"))
        .replace(
            "\"cards\":[",
            r#""cards":[{"at":[0,0],"id":"door","ref":{"entrance":"entrance:x"}},"#,
        )
        .replace(
            "\"groups\":[]",
            &format!("\"groups\":[],\"proposals\":[{proposals}]"),
        )
}

#[test]
fn parsing_refuses_bad_expanded_calls_and_proposals() {
    let proposal = serde_json::json!({
        "after": IF0, "author": "agent:x", "card": "run", "id": "p", "path": "yes", "reason": "R", "title": "T"
    });
    let good = proposal.to_string();
    let with = |field: &str, value: &str| {
        let mut changed = proposal.clone();
        changed[field] = value.into();
        changed.to_string()
    };
    let key = |index: usize| format!("\"main.ts#function:run/call[{index}]\"");
    let paths = |count: usize| {
        let items: Vec<String> = (0..count).map(|index| format!("[{}]", key(index))).collect();
        format!(",\"expanded\":[{}]", items.join(","))
    };
    let proposals = |count: usize| {
        let items: Vec<String> = (0..count)
            .map(|index| good.replace("\"id\":\"p\"", &format!("\"id\":\"p{index}\"")))
            .collect();
        items.join(",")
    };
    // The limits themselves are allowed.
    let three = format!(",\"expanded\":[[{},{},{}]]", key(0), key(1), key(2));
    for text in [
        with_proposals(&three, &good),
        with_proposals(&paths(MAX_EXPANDED_CALLS), &proposals(MAX_PROPOSALS)),
        with_proposals("", &with("title", &"t".repeat(MAX_PROPOSAL_TITLE))),
        with_proposals("", &with("reason", &"r\n".repeat(MAX_PROPOSAL_REASON / 2))),
        with_proposals(",\"expanded\":[]", "").replace("\"id\":\"door\"", "\"expanded\":[],\"id\":\"door\""),
    ] {
        assert!(parse(&text).is_ok(), "{text}");
    }
    let four = format!(",\"expanded\":[[{},{},{},{}]]", key(0), key(1), key(2), key(3));
    let repeated = format!(",\"expanded\":[[{}],[{}]]", key(0), key(0));
    let at_door = with_proposals("", "").replace(
        "\"id\":\"door\"",
        &format!("\"expanded\":[[{}]],\"id\":\"door\"", key(0)),
    );
    let cases = [
        (
            with_proposals(&four, ""),
            "/pages/0/cards/1/expanded/0",
            "must list 1 to 3 keys",
        ),
        (
            with_proposals(",\"expanded\":[[]]", ""),
            "/pages/0/cards/1/expanded/0",
            "must list 1 to 3 keys",
        ),
        (
            with_proposals(&repeated, ""),
            "/pages/0/cards/1/expanded/1",
            "repeats an earlier expanded call",
        ),
        (
            with_proposals(&paths(MAX_EXPANDED_CALLS + 1), ""),
            "/pages/0/cards/1/expanded",
            "at most 64 expanded calls",
        ),
        (
            with_proposals(",\"expanded\":[[\"entrance:x\"]]", ""),
            "/pages/0/cards/1/expanded/0/0",
            "must be a block key",
        ),
        (
            with_proposals(&format!(",\"expanded\":[{}]", key(0)), ""),
            "/pages/0/cards/1/expanded/0",
            "must be an array",
        ),
        (
            at_door,
            "/pages/0/cards/0/expanded",
            "only a block card expands calls in place",
        ),
        (
            with_proposals("", &with("card", "door")),
            "/pages/0/proposals/0/card",
            "`door` is an entrance card; a proposed step goes after a step of a block card",
        ),
        (
            with_proposals("", &with("card", "ghost")),
            "/pages/0/proposals/0/card",
            "refers to an unknown item `ghost`",
        ),
        (
            with_proposals("", &with("title", "  ")),
            "/pages/0/proposals/0/title",
            "must not be empty",
        ),
        (
            with_proposals("", &with("title", "two\nlines")),
            "/pages/0/proposals/0/title",
            "must be one line, without control characters",
        ),
        (
            with_proposals("", &with("title", &"t".repeat(MAX_PROPOSAL_TITLE + 1))),
            "/pages/0/proposals/0/title",
            "must be at most 120 characters",
        ),
        (
            with_proposals("", &with("reason", &"r".repeat(MAX_PROPOSAL_REASON + 1))),
            "/pages/0/proposals/0/reason",
            "must be at most 400 characters",
        ),
        (
            with_proposals("", &with("reason", " ")),
            "/pages/0/proposals/0/reason",
            "must not be empty",
        ),
        (
            with_proposals("", &with("path", "maybe")),
            "/pages/0/proposals/0/path",
            r#"must be "yes", "no", "each", or "done""#,
        ),
        (
            with_proposals("", &with("id", "run")),
            "/pages/0/proposals/0/id",
            "duplicate id `run`",
        ),
        (
            with_proposals("", &format!("{good},{good}")),
            "/pages/0/proposals/1/id",
            "duplicate id `p`",
        ),
        (
            with_proposals("", &with("after", "entrance:x")),
            "/pages/0/proposals/0/after",
            "must be a block key",
        ),
        (
            with_proposals("", &with("author", "robot:x")),
            "/pages/0/proposals/0/author",
            "must be `person:<name>` or `agent:<name>`",
        ),
        (
            with_proposals("", &good.replace("\"author\":\"agent:x\",", "")),
            "/pages/0/proposals/0",
            "missing field `author`",
        ),
        (
            with_proposals("", &good.replace("\"id\"", "\"status\":\"built\",\"id\"")),
            "/pages/0/proposals/0/status",
            "unknown field",
        ),
        (
            with_proposals("", &proposals(MAX_PROPOSALS + 1)),
            "/pages/0/proposals",
            "must hold at most 200 proposed steps",
        ),
    ];
    for (text, path, message) in cases {
        let (found_path, found_message) = invalid(&text);
        assert_eq!(found_path, path, "{text}");
        assert!(
            found_message.contains(message),
            "{path}: {message:?} in {found_message:?}"
        );
    }
}

#[test]
fn proposals_go_after_a_step_inside_their_card_and_are_never_signed() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].proposals.push(email());
    let (saved, report) = save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    let page = &saved.pages[0];
    assert_eq!(page.proposals, [email()], "saved as written");
    let guard = page.guard(None, IF0).expect("the anchor is guarded");
    assert_eq!((guard.kind.as_str(), guard.owner.as_str()), ("if", RUN));
    assert!(
        repair(
            &mut saved.clone(),
            &graph,
            &Repair::Resign(Target::Proposal("email".to_owned()))
        )
        .is_err(),
        "a proposal has nothing to sign"
    );

    // A proposal placed elsewhere must go after a step of its card's Flow that something runs
    // after, on one of that step's paths when it has several (the UI's placement rule).
    for (after, path, expected) in [
        (RUN, None, "Can't place it after its card itself"),
        (
            "main.ts#function:runner/if[0]",
            None,
            "that is not inside its card's block. Choose a statement or call under main.ts#function:run/",
        ),
        (
            "main.ts#function:run/if[7]",
            None,
            "that step is not in the graph",
        ),
        (LOG_CALL, None, "that is not a step of its card's Flow"),
        (RETURN0, None, "nothing runs after it on its path"),
        (IF0, None, "that step has 2 paths: choose yes or no"),
        (
            IF0,
            Some(ProposalPath::Each),
            "that step has no each path: choose yes or no",
        ),
        (LOG0, Some(ProposalPath::Yes), "that step has one way on"),
    ] {
        let mut moved = saved.clone();
        moved.pages[0].proposals[0].after = Key::from_text(after);
        moved.pages[0].proposals[0].path = path;
        refuses_proposal(moved, Some((&saved, &report)), &graph, "email", expected);
        let mut added = saved.clone();
        added.pages[0].proposals.push(Proposal {
            path,
            ..proposal("new", after)
        });
        refuses_proposal(added, Some((&saved, &report)), &graph, "new", expected);
    }
    // Its words change freely; a path-only edit is a new placement, checked again.
    let mut retitled = saved.clone();
    retitled.pages[0].proposals[0].title = "Send a receipt".to_owned();
    retitled.pages[0].proposals[0].path = Some(ProposalPath::No);
    assert!(save(retitled, Some((&saved, &report)), &graph).is_ok());
    for (path, expected) in [
        (None, "that step has 2 paths"),
        (Some(ProposalPath::Done), "that step has no done path"),
    ] {
        let mut rerouted = saved.clone();
        rerouted.pages[0].proposals[0].path = path;
        refuses_proposal(rerouted, Some((&saved, &report)), &graph, "email", expected);
    }
    let mut after_log = saved.clone();
    after_log.pages[0].proposals.push(proposal("log", LOG0));
    assert!(save(after_log, Some((&saved, &report)), &graph).is_ok());
    // One saved before this rule, placed as before, still reads and saves: only a new placement
    // is checked.
    let mut older = saved.clone();
    older.pages[0].proposals.push(proposal("older", IF0));
    let older = parse(&serialize(&older)).unwrap();
    let older_report = page_health(&older, &graph);
    assert!(older_report.is_healthy(), "{:#?}", older_report.items);
    let mut reworded = older.clone();
    reworded.pages[0].proposals[1].title = "Send a receipt".to_owned();
    assert!(save(reworded, Some((&older, &older_report)), &graph).is_ok());
    // A card whose project isn't connected can't take a proposal.
    let mut elsewhere = saved.clone();
    elsewhere.pages[0].cards.push(run_card("web-run", Some(web())));
    elsewhere.pages[0].proposals.push(Proposal {
        card: "web-run".to_owned(),
        ..proposal("web-email", IF0)
    });
    refuses_proposal(
        elsewhere,
        Some((&saved, &report)),
        &graph,
        "web-email",
        "its card's project web is not connected",
    );
}

#[test]
fn a_proposal_whose_step_is_gone_waits_for_an_explicit_repair() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].proposals.push(email());
    board.pages[0].proposals.push(proposal("log", LOG0));
    let (saved, _) = save(board, None, &graph).unwrap();
    let updated = recompiled(dir.path(), &SOURCE.replace("  console.log('done');\n", ""));
    let report = page_health(&saved, &updated);
    assert_eq!((report.missing, report.review), (1, 0), "{:#?}", report.items);
    let item = &report.items[0];
    assert_eq!(
        (
            &item.target,
            item.kind,
            item.key.as_deref(),
            item.project.as_ref()
        ),
        (
            &Target::Proposal("log".to_owned()),
            HealthKind::Missing,
            Some(LOG0),
            None
        )
    );
    assert_eq!(item.repairs, [RepairKind::Reconnect, RepairKind::RemoveProposal]);
    assert!(
        item.message
            .starts_with("The step this proposal goes after is missing: ")
    );
    assert_eq!(
        saved.pages[0].proposal("log"),
        Some(&proposal("log", LOG0)),
        "kept as written"
    );

    // Placed as before, it never refuses a save, even retitled.
    let mut moved = saved.clone();
    moved.pages[0].cards[0].at = [5.0, 5.0];
    moved.pages[0].proposals[1].title = "Log the result".to_owned();
    let (moved, moved_report) = save(moved, Some((&saved, &report)), &updated).unwrap();
    assert_eq!(moved_report.missing, 1);
    assert_eq!(
        moved.pages[0].proposals[1].after.as_str(),
        LOG0,
        "never reattached"
    );

    // Reconnect is explicit and must stay inside the card's block.
    for (to, expected) in [
        (RUN, "after its card itself"),
        ("main.ts#", "not inside its card's block"),
        (RETURN0, "nothing runs after it"),
        (IF0, "that step has 2 paths"),
    ] {
        let mut wrong = saved.clone();
        match repair(
            &mut wrong,
            &updated,
            &Repair::Reconnect {
                project: None,
                from: LOG0.to_owned(),
                to: to.to_owned(),
            },
        ) {
            Err(BoardError::Problem { target, message, .. }) => {
                assert_eq!(target, Target::Proposal("log".to_owned()));
                assert!(message.contains(expected), "{message}");
            }
            other => panic!("{to}: {other:?}"),
        }
        assert_eq!(wrong, saved, "a refused repair changes nothing");
    }
    let mut repaired = saved.clone();
    repair(
        &mut repaired,
        &updated,
        &Repair::Reconnect {
            project: None,
            from: LOG0.to_owned(),
            to: FIRST.to_owned(),
        },
    )
    .unwrap();
    let page = &repaired.pages[0];
    assert_eq!(page.proposal("log").unwrap().after.as_str(), FIRST);
    assert!(page.guard(None, FIRST).is_some() && page.guard(None, LOG0).is_none());
    assert!(page_health(&repaired, &updated).is_healthy());
    assert!(save(repaired, Some((&saved, &report)), &updated).is_ok());

    let mut removed = saved;
    repair(&mut removed, &updated, &Repair::RemoveProposal("log".to_owned())).unwrap();
    assert_eq!(removed.pages[0].proposals, [email()]);
    assert!(page_health(&removed, &updated).is_healthy());
}

#[test]
fn a_proposal_whose_step_changed_is_held_for_review() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].proposals.push(email());
    let (saved, _) = save(board, None, &graph).unwrap();
    let inserted = SOURCE.replace(
        "  if (first)",
        "  if (!first) { console.log('inserted'); }\n  if (first)",
    );
    let updated = recompiled(dir.path(), &inserted);
    let report = page_health(&saved, &updated);
    assert_eq!((report.review, report.missing), (1, 0), "{:#?}", report.items);
    let item = &report.items[0];
    assert_eq!(
        (&item.target, item.kind, item.key.as_deref()),
        (
            &Target::Proposal("email".to_owned()),
            HealthKind::Review,
            Some(IF0)
        )
    );
    assert_eq!(
        item.repairs,
        [
            RepairKind::Accept,
            RepairKind::Reconnect,
            RepairKind::RemoveProposal
        ]
    );
    assert!(
        item.message
            .contains("position-based and its code changed; check where it goes")
    );

    // Accepting keeps it where it is, after the code now at that position.
    let mut accepted = saved.clone();
    repair(
        &mut accepted,
        &updated,
        &Repair::Accept(ProjectKey::new(None, IF0)),
    )
    .unwrap();
    assert!(page_health(&accepted, &updated).is_healthy());
    assert_eq!(accepted.pages[0].proposals, saved.pages[0].proposals);
    // Reconnecting moves it after the step it meant.
    let moved_if = "main.ts#function:run/if[1]";
    let mut reconnected = saved;
    repair(
        &mut reconnected,
        &updated,
        &Repair::Reconnect {
            project: None,
            from: IF0.to_owned(),
            to: moved_if.to_owned(),
        },
    )
    .unwrap();
    assert_eq!(reconnected.pages[0].proposals[0].after.as_str(), moved_if);
    assert!(page_health(&reconnected, &updated).is_healthy());
}

#[test]
fn reconnecting_a_card_carries_its_proposals_and_removing_it_removes_them() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.pages[0].proposals.push(Proposal {
        path: Some(ProposalPath::Yes),
        ..proposal("email", IF0)
    });
    let (saved, _) = save(board, None, &graph).unwrap();
    let updated = recompiled(dir.path(), &SOURCE.replace("function run", "function start"));
    let report = page_health(&saved, &updated);
    assert!(report.has(
        "page-1",
        HealthKind::Missing,
        &Target::Proposal("email".to_owned())
    ));
    let mut repaired = saved.clone();
    repair(
        &mut repaired,
        &updated,
        &Repair::Reconnect {
            project: None,
            from: RUN.to_owned(),
            to: "main.ts#function:start".to_owned(),
        },
    )
    .unwrap();
    let page = &repaired.pages[0];
    let moved = "main.ts#function:start/if[0]";
    assert_eq!(page.proposals[0].after.as_str(), moved);
    assert_eq!(page.proposals[0].path, Some(ProposalPath::Yes));
    assert!(page.guard(None, moved).is_some() && page.guard(None, IF0).is_none());
    assert!(page_health(&repaired, &updated).is_healthy());
    let (repaired, _) = save(repaired, Some((&saved, &report)), &updated).unwrap();

    let mut removed = repaired;
    repair(&mut removed, &updated, &Repair::RemoveCard("run".to_owned())).unwrap();
    assert!(removed.pages[0].cards.is_empty() && removed.pages[0].proposals.is_empty());
}

#[test]
fn expansions_and_proposals_count_as_changes_and_stay_out_of_mermaid() {
    let (_dir, graph) = folder(SOURCE);
    let (published, _) = save(sample_board(), None, &graph).unwrap();
    let mut working = published.clone();
    working.pages[0].cards[0].expanded = vec![vec![Key::from_text(LOG_CALL)]];
    working.pages[0].proposals.push(proposal("email", IF0));
    assert_eq!(
        changes(&working, &published),
        2,
        "a changed card and a new proposal"
    );
    let graphs = PageGraphs::own_only(&graph);
    assert_eq!(
        crate::mermaid::page(&working, "page-1", &graphs),
        crate::mermaid::page(&published, "page-1", &graphs)
    );
}
