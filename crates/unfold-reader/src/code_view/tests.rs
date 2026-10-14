//! The code panel's model: what each kind of block shows, the path,
//! hover, `⊕`, the states, and UTF-16 columns.

use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::testkit::{GraphBuilder, NodeRef, sample, unknown};
use unfold_core::vocab::{FileState, Form, InputRole, Language, LinkKind, MemberKind, Reason};
use unfold_core::{ByteRange, Callee, Key, Member, RepoPath};

use super::*;
use crate::testing::Playground;

const SERVICE: &str = "shop/service.py";
const PLACE_ORDER: &str = "shop/service.py#place_order";

/// Marks every whole word of `WORDS` as a keyword.
struct Keywords;

const WORDS: &[&str] = &["def", "return", "try", "except", "if", "elif", "for", "in", "as"];

impl Highlighter for Keywords {
    fn language(&self) -> Language {
        Language::Python
    }

    fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let mut start = None;
        for (index, character) in text.char_indices().chain([(text.len(), ' ')]) {
            let word = character.is_alphanumeric() || character == '_';
            match (word, start) {
                (true, None) => start = Some(index),
                (false, Some(from)) => {
                    if WORDS.contains(&&text[from..index]) {
                        spans.push(HighlightSpan {
                            range: ByteRange::new(from as u32, index as u32).unwrap(),
                            class: TokenClass::Keyword,
                        });
                    }
                    start = None;
                }
                _ => {}
            }
        }
        spans
    }
}

fn shop() -> Playground {
    Playground::built(&sample::shop())
}

fn view_of(playground: &Playground, selected: Target, show: Option<Target>) -> CodeView {
    view_with(playground, selected, show, true)
}

fn view_with(
    playground: &Playground,
    selected: Target,
    show: Option<Target>,
    settings_parse: bool,
) -> CodeView {
    let sources = CodeSources {
        facts: &playground.graph,
        root: &playground.root,
        inputs: &playground.graph.inputs,
        apps: &playground.graph.apps,
        settings_parse,
        highlighters: &[&Keywords],
    };
    code_view(&sources, &CodeRequest { selected, show }).unwrap()
}

fn view(playground: &Playground, selected: Target) -> CodeView {
    view_of(playground, selected, None)
}

fn block(key: &str) -> Target {
    Target::Block(Key::from_text(key))
}

fn path(value: &str) -> RepoPath {
    RepoPath::new(value).unwrap()
}

fn lines(path: &str, lines: [u32; 2]) -> Target {
    Target::Lines {
        path: self::path(path),
        lines,
        mark: None,
        title: None,
    }
}

/// The shown lines as `number text`.
fn shown(view: &CodeView) -> Vec<String> {
    view.lines
        .iter()
        .map(|line| format!("{} {}", line.number, line.text))
        .collect()
}

fn numbers(view: &CodeView) -> [u32; 2] {
    [
        view.lines.first().unwrap().number,
        view.lines.last().unwrap().number,
    ]
}

/// The path as labels, the showing step in `*…*`.
fn steps(view: &CodeView) -> Vec<String> {
    view.path
        .iter()
        .map(|step| {
            if step.showing {
                format!("*{}*", step.label)
            } else {
                step.label.clone()
            }
        })
        .collect()
}

fn marked_lines(view: &CodeView) -> Option<[u32; 2]> {
    view.mark.as_ref().and_then(|mark| mark.lines)
}

/// The text each expression range of the mark covers.
fn expression(view: &CodeView) -> Vec<String> {
    let Some(mark) = &view.mark else {
        return Vec::new();
    };
    mark.expression
        .iter()
        .map(|range| {
            let line = view.lines.iter().find(|line| line.number == range.line).unwrap();
            utf16_slice(&line.text, range.start, range.end)
        })
        .collect()
}

fn utf16_slice(text: &str, start: u32, end: u32) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16(&units[start as usize..end as usize]).unwrap()
}

#[test]
fn a_definition_card_shows_its_lines_from_its_first_decorator() {
    let shop = shop();
    let place = view(&shop, block(PLACE_ORDER));
    assert_eq!(place.state, CodeState::Current);
    assert_eq!(place.file, Some(path(SERVICE)));
    assert_eq!(numbers(&place), [9, 28]);
    assert_eq!(place.lines[0].text, "def place_order(order_id):");
    assert_eq!(
        place.lines[1].text,
        "    \"\"\"Charge an order and save it.\"\"\""
    );
    assert_eq!(place.mark, None);
    assert_eq!(steps(&place), ["shop/service.py", "*place_order*"]);
    assert_eq!(place.path[1].key, Some(Key::from_text(PLACE_ORDER)));
    assert_eq!(place.path[0].key, None);

    let charge = view(&shop, block("shop/payments.py#charge_card"));
    assert_eq!(numbers(&charge), [16, 21]);
    assert_eq!(charge.lines[0].text, "@retry(3)");
    // The decorator's line belongs to the definition, and names the decorator's call.
    assert_eq!(
        charge.lines[0].block,
        Some(Key::from_text("shop/payments.py#charge_card"))
    );
    assert_eq!(
        charge.lines[0].calls[0].key,
        Key::from_text("shop/payments.py#/call:retry")
    );
    assert_eq!(charge.mark, None);
    assert_eq!(
        charge.definitions,
        [DefinitionLine {
            line: 16,
            key: Key::from_text("shop/payments.py#charge_card"),
        }]
    );
}

#[test]
fn a_module_shows_the_whole_file() {
    let shop = shop();
    let module = view(&shop, block("shop/service.py#"));
    // The file ends with a newline, which starts no line of its own.
    assert_eq!(numbers(&module), [1, 28]);
    assert_eq!(module.mark, None);
    assert_eq!(steps(&module), ["*shop/service.py*"]);
}

#[test]
fn a_control_block_shows_its_header_to_the_end_of_its_last_part() {
    let shop = shop();
    let try_block = view(&shop, block("shop/service.py#place_order/try[0]"));
    assert_eq!(
        shown(&try_block),
        [
            "18 try:",
            "19     receipt = charge_card(order[\"card\"], total)",
            "20 except PaymentDeclined as error:",
            "21     logger.warning(\"declined: %s\", error)",
            "22     return mark_failed(order)",
        ]
    );
    assert_eq!(try_block.mark, None);
    assert_eq!(steps(&try_block), ["shop/service.py", "place_order", "*try*"]);

    let decision = view(&shop, block("shop/service.py#place_order/if[0]"));
    assert_eq!(numbers(&decision), [12, 15]);
    assert_eq!(steps(&decision), ["shop/service.py", "place_order", "*if*"]);

    let loop_block = view(&shop, block("shop/service.py#place_order/for[0]"));
    assert_eq!(numbers(&loop_block), [26, 27]);
    assert_eq!(
        steps(&loop_block),
        ["shop/service.py", "place_order", "*for item in order[\"items\"]*"]
    );
}

#[test]
fn a_part_shows_its_whole_control_block_with_the_part_marked() {
    let shop = shop();
    let handler = view(&shop, block("shop/service.py#place_order/try[0]/except[0]"));
    assert_eq!(numbers(&handler), [18, 22]);
    assert_eq!(marked_lines(&handler), Some([20, 22]));
    assert!(handler.mark.as_ref().unwrap().expression.is_empty());
    // The control block above a part is bold from the start: it is what the panel shows.
    assert_eq!(
        steps(&handler),
        [
            "shop/service.py",
            "place_order",
            "*try*",
            "except PaymentDeclined as error"
        ]
    );

    // A `then` part starts at its `if` line; `elif` and `else` show the whole `if`.
    let then = view(&shop, block("shop/service.py#place_order/if[0]/then"));
    assert_eq!(numbers(&then), [12, 15]);
    assert_eq!(marked_lines(&then), Some([12, 13]));
    assert_eq!(
        steps(&then),
        ["shop/service.py", "place_order", "*if*", "order is None"]
    );
    let elif = view(&shop, block("shop/service.py#place_order/if[0]/elif[0]"));
    assert_eq!(numbers(&elif), [12, 15]);
    assert_eq!(marked_lines(&elif), Some([14, 15]));
    assert_eq!(steps(&elif)[3], "elif order[\"paid\"]");

    // A `body` part starts on its control block's header, and is left out of the path.
    let body = view(&shop, block("shop/service.py#place_order/try[0]/body"));
    assert_eq!(numbers(&body), [18, 22]);
    assert_eq!(marked_lines(&body), Some([18, 19]));
    assert_eq!(steps(&body), ["shop/service.py", "place_order", "*try*"]);
    let loop_body = view(&shop, block("shop/service.py#place_order/for[0]/body"));
    assert_eq!(numbers(&loop_body), [26, 27]);
    assert_eq!(marked_lines(&loop_body), Some([26, 27]));
}

#[test]
fn parts_in_a_switch_a_match_and_a_loop_else() {
    let ts = Playground::load("ts-orders");
    let case = view(&ts, block("src/cli.ts#main/switch[0]/case[1]"));
    assert_eq!(numbers(&case), [8, 27]);
    assert_eq!(case.lines[0].text, "switch (command) {");
    assert_eq!(marked_lines(&case), Some([12, 16]));
    assert_eq!(
        steps(&case),
        ["src/cli.ts", "main", "*switch (command)*", "case \"report\""]
    );
    let otherwise = view(&ts, block("src/lib/validate.ts#validateOrder/if[1]/elif[0]"));
    let label = &otherwise.path.last().unwrap().label;
    assert!(label.starts_with("else if typeof email"), "{label}");
    let default = view(&ts, block("src/cli.ts#main/switch[0]/default"));
    assert_eq!(marked_lines(&default), Some([25, 26]));
    assert_eq!(steps(&default)[3], "default");
    let catch = view(
        &ts,
        block("src/jobs/import-orders.ts#importOrders/for[0]/body/try[0]/catch"),
    );
    assert_eq!(numbers(&catch), [12, 19]);
    assert_eq!(marked_lines(&catch), Some([16, 19]));
    assert_eq!(
        steps(&catch),
        [
            "src/jobs/import-orders.ts",
            "importOrders",
            "for (const [index, value] of input.entries())",
            "*try*",
            "catch (error)"
        ]
    );

    let py = Playground::load("py-orders");
    let wildcard = view(&py, block("src/orders_playground/cli.py#main/match[0]/default"));
    assert_eq!(numbers(&wildcard), [18, 26]);
    assert_eq!(marked_lines(&wildcard), Some([25, 26]));
    assert_eq!(
        steps(&wildcard),
        [
            "src/orders_playground/cli.py",
            "main",
            "*match arguments.command*",
            "case _"
        ]
    );
    let first = view(&py, block("src/orders_playground/cli.py#main/match[0]/case[0]"));
    assert_eq!(steps(&first)[3], "case \"import\"");
    let loop_else = view(
        &py,
        block("src/orders_playground/features/control.py#scan/for[0]/else"),
    );
    assert_eq!(numbers(&loop_else), [20, 26]);
    assert_eq!(marked_lines(&loop_else), Some([25, 26]));
    assert_eq!(steps(&loop_else)[2..], ["*for value in cleaned*", "else"]);
    let finally = view(
        &py,
        block("src/orders_playground/features/control.py#scan/try[0]/finally"),
    );
    assert_eq!(numbers(&finally), [27, 34]);
    assert_eq!(marked_lines(&finally), Some([33, 34]));
    let with = view(
        &py,
        block("src/orders_playground/features/control.py#scan/with[0]/body"),
    );
    assert_eq!(numbers(&with), [35, 36]);
    assert_eq!(marked_lines(&with), Some([35, 36]));
    assert_eq!(steps(&with)[2..], ["*with StringIO(\"a\\nb\") as source*"]);
}

#[test]
fn a_call_shows_its_statement_with_its_expression_marked() {
    let shop = shop();
    let charge = view(
        &shop,
        block("shop/service.py#place_order/try[0]/body/call:charge_card"),
    );
    assert_eq!(
        shown(&charge),
        ["19 receipt = charge_card(order[\"card\"], total)"]
    );
    assert_eq!(marked_lines(&charge), None);
    assert_eq!(
        charge.mark.as_ref().unwrap().expression,
        [LineRange {
            line: 19,
            start: 10,
            end: 43,
        }]
    );
    assert_eq!(
        steps(&charge),
        ["shop/service.py", "place_order", "try", "*charge_card*"]
    );

    // A call inside another block's statement shows that statement.
    let mark_failed = view(
        &shop,
        block("shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed"),
    );
    assert_eq!(shown(&mark_failed), ["22 return mark_failed(order)"]);
    assert_eq!(expression(&mark_failed), ["mark_failed(order)"]);
    assert_eq!(
        steps(&mark_failed),
        [
            "shop/service.py",
            "place_order",
            "try",
            "except PaymentDeclined as error",
            "return",
            "*mark_failed*"
        ]
    );

    // Library and builtin calls open their call site like any call.
    let warning = view(
        &shop,
        block("shop/service.py#place_order/try[0]/except[0]/call:logger.warning"),
    );
    assert_eq!(shown(&warning), ["21 logger.warning(\"declined: %s\", error)"]);
    let sum = view(&shop, block("shop/service.py#place_order/call:sum"));
    assert_eq!(numbers(&sum), [17, 17]);
    assert_eq!(
        expression(&sum),
        ["sum(item[\"price\"] for item in order[\"items\"])"]
    );
    let nested = view(&shop, block("shop/store.py#save_all/call:list/call:map"));
    assert_eq!(shown(&nested), ["26 list(map(save_order, orders))"]);
    assert_eq!(expression(&nested), ["map(save_order, orders)"]);

    // A TypeScript call under `await`.
    let ts = Playground::load("ts-orders");
    let import = view(&ts, block("src/cli.ts#main/switch[0]/case[0]/call:importOrders"));
    assert_eq!(shown(&import), ["10 await importOrders(path);"]);
    assert_eq!(expression(&import), ["importOrders(path)"]);
}

#[test]
fn a_call_in_a_test_or_a_header_shows_that_line() {
    let py = Playground::load("py-orders");
    let control = "src/orders_playground/features/control.py";
    let header = view(&py, block(&format!("{control}#scan/with[0]/call:StringIO")));
    assert_eq!(shown(&header), ["35 with StringIO(\"a\\nb\") as source:"]);
    assert_eq!(expression(&header), ["StringIO(\"a\\nb\")"]);
    let test = view(
        &py,
        block(&format!("{control}#scan/comprehension[0]/body/call:len")),
    );
    assert_eq!(numbers(&test), [16, 16]);
    assert_eq!(expression(&test), ["len(value)"]);
    // A decorator runs when its definition is created: it shows its own line.
    let decorator = view(&py, block(&format!("{control}#/call:asynccontextmanager")));
    assert_eq!(shown(&decorator), ["54 @asynccontextmanager"]);
    let header_loop = view(
        &py,
        block(&format!(
            "{control}#collect/with[0]/body/for[0]/call:async_records"
        )),
    );
    assert_eq!(shown(&header_loop), ["66 async for value in async_records():"]);

    let ts = Playground::load("ts-orders");
    let condition = view(
        &ts,
        block(
            "src/jobs/cleanup.ts#cleanupExpiredOrders/if[0]/then/do[0]/body/if[0]/then/call:orderService.remove",
        ),
    );
    assert_eq!(shown(&condition), ["23 if (orderService.remove(id)) removed++;"]);
    assert_eq!(expression(&condition), ["orderService.remove(id)"]);
}

#[test]
fn an_exit_shows_its_statement() {
    let shop = shop();
    let exit = view(
        &shop,
        block("shop/service.py#place_order/try[0]/except[0]/return[0]"),
    );
    assert_eq!(shown(&exit), ["22 return mark_failed(order)"]);
    assert_eq!(exit.mark, None);
    assert_eq!(steps(&exit).last().unwrap(), "*return*");
    let raise = view(&shop, block("shop/service.py#place_order/if[0]/then/raise[0]"));
    assert_eq!(shown(&raise), ["13 raise ValueError(\"unknown order\")"]);
    assert_eq!(steps(&raise)[3..], ["order is None", "*raise*"]);
}

#[test]
fn combined_steps_rows_and_gap_ticks_show_their_lines() {
    let shop = shop();
    let combined = view(
        &shop,
        Target::Lines {
            path: path(SERVICE),
            lines: [25, 27],
            mark: None,
            title: Some("Save it and reserve stock".to_owned()),
        },
    );
    assert_eq!(
        shown(&combined),
        [
            "25 save_order(order)",
            "26 for item in order[\"items\"]:",
            "27     reserve_stock(item)",
        ]
    );
    assert_eq!(combined.mark, None);
    assert_eq!(
        steps(&combined),
        ["shop/service.py", "place_order", "*⊞ Save it and reserve stock*"]
    );

    let tick = view(&shop, lines(SERVICE, [24, 24]));
    assert_eq!(shown(&tick), ["24 order[\"receipt\"] = receipt"]);
    assert_eq!(steps(&tick), ["shop/service.py", "place_order", "*line 24*"]);
    let inside = view(&shop, lines(SERVICE, [21, 22]));
    assert_eq!(
        steps(&inside),
        [
            "shop/service.py",
            "place_order",
            "try",
            "except PaymentDeclined as error",
            "*lines 21–22*"
        ]
    );
    // Lines past the end are cut to the file.
    assert_eq!(numbers(&view(&shop, lines(SERVICE, [27, 90]))), [27, 28]);
}

#[test]
fn a_step_inside_a_call_opened_in_place_shows_in_the_callees_file() {
    let shop = shop();
    let inner = view(&shop, block("shop/payments.py#charge_card/call:logger.info"));
    assert_eq!(inner.file, Some(path("shop/payments.py")));
    assert_eq!(shown(&inner), ["20 logger.info(\"charging %s\", card)"]);
    assert_eq!(
        steps(&inner),
        ["shop/payments.py", "charge_card", "*logger.info*"]
    );
}

/// The byte span of `needle` in the sample's `unfold.toml`.
fn toml_span(needle: &str) -> ByteRange {
    let start = sample::UNFOLD_TOML.find(needle).unwrap() as u32;
    ByteRange::new(start, start + needle.len() as u32).unwrap()
}

#[test]
fn an_entrance_or_app_card_shows_its_lines_in_unfold_toml() {
    let shop = shop();
    let entrance_span = toml_span(
        "{ name = \"Place an order\", on = \"POST /orders/{order_id}\", code = \"shop/service.py#place_order\" }",
    );
    let entrance = Target::Start {
        key: Key::from_text("entrance:shop/place-an-order"),
        span: entrance_span,
    };
    let opened = view(&shop, entrance.clone());
    assert_eq!(opened.state, CodeState::Current);
    assert_eq!(opened.file, Some(path("unfold.toml")));
    assert_eq!(numbers(&opened), [6, 6]);
    assert!(opened.lines[0].text.starts_with("{ name = \"Place an order\""));
    assert!(opened.lines[0].tokens.is_empty());
    assert_eq!(opened.mark, None);
    assert_eq!(steps(&opened), ["unfold.toml", "Shop", "*Place an order*"]);
    assert_eq!(opened.path[1].key, Some(Key::from_text("entrance:shop")));

    let app = Target::Start {
        key: Key::from_text("entrance:shop"),
        span: toml_span(sample::UNFOLD_TOML.trim_end()),
    };
    let whole = view(&shop, app.clone());
    assert_eq!(numbers(&whole), [1, 10]);
    assert_eq!(whole.lines[0].text, "[[app]]");
    assert_eq!(steps(&whole), ["unfold.toml", "*Shop*"]);

    // Widened to its app, the entrance stays marked.
    let widened = view_of(&shop, entrance, Some(app));
    assert_eq!(numbers(&widened), [1, 10]);
    assert_eq!(marked_lines(&widened), Some([6, 6]));
    assert_eq!(steps(&widened), ["unfold.toml", "*Shop*", "Place an order"]);

    let missing = view(
        &shop,
        Target::Start {
            key: Key::from_text("entrance:shop/absent"),
            span: entrance_span,
        },
    );
    assert_eq!(missing.state, CodeState::NotFound);
}

#[test]
fn a_field_of_a_type_card_shows_its_line() {
    let shop = shop();
    let field = view(
        &shop,
        Target::Field {
            key: Key::from_text("src/orders.ts#Order"),
            index: 1,
        },
    );
    assert_eq!(shown(&field), ["6 total: number;"]);
    assert_eq!(field.mark, None);
    assert_eq!(steps(&field), ["src/orders.ts", "Order", "*total*"]);
    let absent = view(
        &shop,
        Target::Field {
            key: Key::from_text("src/orders.ts#Order"),
            index: 9,
        },
    );
    assert_eq!(absent.state, CodeState::NotFound);
}

#[test]
fn a_field_without_a_written_type_shows_its_line() {
    let mut graph = sample::shop();
    let module = graph.file("m.ts", Language::TypeScript, "class Box {\n  items = [];\n}\n");
    let class = graph.def(
        module,
        Form::Class,
        "Box",
        graph.find(module, "class Box {\n  items = [];\n}"),
    );
    let range = graph.find_in(class, "items = [];");
    graph.types(class).members.push(Member {
        node: class.local_id(),
        index: 0,
        range,
        kind: MemberKind::Property,
        name: Some("items".into()),
        optional: false,
        readonly: false,
        default: true,
        ty: None,
        value: None,
    });
    let playground = Playground::built(&graph);
    let field = view(
        &playground,
        Target::Field {
            key: graph.key(class),
            index: 0,
        },
    );
    assert_eq!(shown(&field), ["2 items = [];"]);
    assert_eq!(steps(&field), ["m.ts", "Box", "*items*"]);
}

/// A graph with an evidence file next to the sample's sources.
fn with_evidence() -> Playground {
    let mut graph = sample::shop();
    graph.input(
        "docs/queues.md",
        InputRole::Evidence,
        "# Queues\n\n  orders: SQS orders-queue\n  emails: SQS emails-queue\n",
    );
    Playground::built(&graph)
}

#[test]
fn a_resource_or_connection_shows_the_lines_it_cites() {
    let playground = with_evidence();
    let cited = view(
        &playground,
        Target::Lines {
            path: path("docs/queues.md"),
            lines: [3, 4],
            mark: Some([3, 4]),
            title: None,
        },
    );
    assert_eq!(cited.state, CodeState::Current);
    assert_eq!(
        shown(&cited),
        ["3 orders: SQS orders-queue", "4 emails: SQS emails-queue"]
    );
    assert_eq!(marked_lines(&cited), Some([3, 4]));
    assert!(
        cited
            .lines
            .iter()
            .all(|line| line.tokens.is_empty() && line.block.is_none())
    );
    assert_eq!(steps(&cited), ["docs/queues.md", "*lines 3–4*"]);

    // A citation of source lines marks them in the source.
    let source = view(
        &playground,
        Target::Lines {
            path: path(SERVICE),
            lines: [19, 19],
            mark: Some([19, 19]),
            title: None,
        },
    );
    assert_eq!(marked_lines(&source), Some([19, 19]));
}

#[test]
fn a_file_shows_whole_with_evidence_as_plain_text() {
    let playground = with_evidence();
    let evidence = view(&playground, Target::File(path("docs/queues.md")));
    assert_eq!(evidence.state, CodeState::Current);
    assert_eq!(
        shown(&evidence),
        [
            "1 # Queues",
            "2 ",
            "3   orders: SQS orders-queue",
            "4   emails: SQS emails-queue"
        ]
    );
    assert!(evidence.definitions.is_empty());
    assert_eq!(steps(&evidence), ["*docs/queues.md*"]);

    let source = view(&playground, Target::File(path("shop/payments.py")));
    assert_eq!(numbers(&source), [1, 21]);
    let starts: Vec<(u32, &str)> = source
        .definitions
        .iter()
        .map(|definition| (definition.line, definition.key.as_str()))
        .collect();
    assert_eq!(
        starts,
        [
            (8, "shop/payments.py#PaymentDeclined"),
            (12, "shop/payments.py#retry"),
            (13, "shop/payments.py#retry/return[0]/lambda[0]"),
            (16, "shop/payments.py#charge_card"),
        ]
    );
}

#[test]
fn hovering_a_line_finds_its_block_and_its_calls() {
    let shop = shop();
    let place = view(&shop, block(PLACE_ORDER));
    let block_of = |number: u32| {
        place.lines[(number - 9) as usize]
            .block
            .as_ref()
            .map(|key| key.as_str().trim_start_matches(PLACE_ORDER).to_owned())
    };
    assert_eq!(block_of(9).as_deref(), Some(""));
    assert_eq!(
        block_of(10).as_deref(),
        Some(""),
        "the docstring is the definition's"
    );
    assert_eq!(block_of(11).as_deref(), Some("/call:load_order"));
    assert_eq!(block_of(12).as_deref(), Some("/if[0]"));
    assert_eq!(block_of(13).as_deref(), Some("/if[0]/then/raise[0]"));
    assert_eq!(block_of(14).as_deref(), Some("/if[0]/elif[0]"));
    assert_eq!(block_of(16), None, "a blank line");
    assert_eq!(block_of(18).as_deref(), Some("/try[0]"));
    assert_eq!(block_of(20).as_deref(), Some("/try[0]/except[0]"));
    assert_eq!(block_of(24), None, "code no block covers lights up its gap tick");
    assert_eq!(block_of(26).as_deref(), Some("/for[0]"));
    assert_eq!(block_of(27).as_deref(), Some("/for[0]/body/call:reserve_stock"));

    // A line with several calls names each, and the line belongs to the statement's first
    // block.
    let handler = view(&shop, block(sample::ORDERS_HANDLER));
    let line = handler.lines.iter().find(|line| line.number == 13).unwrap();
    assert_eq!(line.text, "  res.status(200).json(order);");
    let json = format!("{}/call:res.status().json", sample::ORDERS_HANDLER);
    assert_eq!(line.block.as_ref().map(Key::as_str), Some(json.as_str()));
    let names: Vec<(String, &str)> = line
        .calls
        .iter()
        .map(|call| (utf16_slice(&line.text, call.start, call.end), call.key.as_str()))
        .collect();
    let status = format!("{json}/call:res.status");
    assert_eq!(
        names,
        [
            ("res.status".to_owned(), status.as_str()),
            ("json".to_owned(), json.as_str())
        ]
    );
    // The callback's first line is its registration's statement.
    let first = &handler.lines[0];
    assert_eq!(first.number, 11);
    assert_eq!(
        first.block.as_ref().map(Key::as_str),
        Some("src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")")
    );

    // A one-line `if` belongs to the `if`, not to what it runs.
    let ts = Playground::load("ts-orders");
    let one_line = view(&ts, block("src/jobs/import-orders.ts#importOrders"));
    let line = one_line.lines.iter().find(|line| line.number == 8).unwrap();
    assert_eq!(
        line.block.as_ref().map(Key::as_str),
        Some("src/jobs/import-orders.ts#importOrders/if[0]")
    );
    let line = one_line.lines.iter().find(|line| line.number == 16).unwrap();
    assert_eq!(
        line.block.as_ref().map(Key::as_str),
        Some("src/jobs/import-orders.ts#importOrders/for[0]/body/try[0]/catch")
    );
}

#[test]
fn definitions_with_no_card_get_their_first_line() {
    let shop = shop();
    let module = view(&shop, block("src/orders.ts#"));
    let starts: Vec<(u32, &str)> = module
        .definitions
        .iter()
        .map(|definition| (definition.line, definition.key.as_str()))
        .collect();
    assert_eq!(
        starts,
        [
            (4, "src/orders.ts#Order"),
            (11, sample::ORDERS_HANDLER),
            (16, "src/orders.ts#loadOrder"),
        ]
    );
    // Only the definitions whose first line is shown.
    let call = view(&shop, block("shop/store.py#reserve_stock/for[0]/call:sorted"));
    assert_eq!(
        call.definitions,
        [DefinitionLine {
            line: 21,
            key: Key::from_text("shop/store.py#reserve_stock/for[0]/call:sorted/callback[0]"),
        }]
    );
}

#[test]
fn widening_shows_a_step_of_the_path_and_keeps_the_selection_marked() {
    let shop = shop();
    let handler = block("shop/service.py#place_order/try[0]/except[0]");
    let widened = view_of(&shop, handler.clone(), Some(block(PLACE_ORDER)));
    assert_eq!(numbers(&widened), [9, 28]);
    assert_eq!(marked_lines(&widened), Some([20, 22]));
    assert_eq!(
        steps(&widened),
        [
            "shop/service.py",
            "*place_order*",
            "try",
            "except PaymentDeclined as error"
        ]
    );
    let file = view_of(&shop, handler, Some(Target::File(path(SERVICE))));
    assert_eq!(numbers(&file), [1, 28]);
    assert_eq!(marked_lines(&file), Some([20, 22]));
    assert_eq!(steps(&file)[0], "*shop/service.py*");

    // A call keeps its lines tinted and its expression marked.
    let call = block("shop/service.py#place_order/try[0]/body/call:charge_card");
    let wide = view_of(&shop, call, Some(block("shop/service.py#place_order/try[0]")));
    assert_eq!(numbers(&wide), [18, 22]);
    assert_eq!(marked_lines(&wide), Some([19, 19]));
    assert_eq!(expression(&wide), ["charge_card(order[\"card\"], total)"]);
    assert_eq!(
        steps(&wide),
        ["shop/service.py", "place_order", "*try*", "charge_card"]
    );

    // A gap tick widened to its definition.
    let tick = view_of(&shop, lines(SERVICE, [24, 24]), Some(block(PLACE_ORDER)));
    assert_eq!(marked_lines(&tick), Some([24, 24]));
    assert_eq!(steps(&tick), ["shop/service.py", "*place_order*", "line 24"]);
}

#[test]
fn lines_are_dedented_with_real_numbers_and_utf16_columns() {
    let text = "class Cart:\n    def total(self):\n        label = \"é😀\"; price = compute(label)\n        return price\n";
    let mut graph = GraphBuilder::new();
    let module = graph.file("cart.py", Language::Python, text);
    let class = graph.def(module, Form::Class, "Cart", graph.find(module, text.trim_end()));
    let method = graph.def(
        class,
        Form::Method,
        "total",
        graph.find(
            module,
            "def total(self):\n        label = \"é😀\"; price = compute(label)\n        return price",
        ),
    );
    graph.node_mut(method).head = Some(graph.find(module, "def total(self)"));
    let call = graph.call(
        method,
        &Callee::name("compute"),
        graph.find(module, "compute(label)"),
    );
    graph.node_mut(call).head = Some(graph.find(module, "compute"));
    graph.node_mut(call).stmt = Some(graph.find(module, "price = compute(label)"));
    let playground = Playground::built(&graph);

    let method_view = view(&playground, Target::Block(graph.key(method)));
    assert_eq!(
        shown(&method_view),
        [
            "2 def total(self):",
            "3     label = \"é😀\"; price = compute(label)",
            "4     return price",
        ]
    );
    // `def` and `return` keep their columns after the shared indentation is dropped.
    assert_eq!(method_view.lines[0].tokens, [Token(0, 3, TokenClass::Keyword)]);
    assert_eq!(method_view.lines[2].tokens, [Token(4, 10, TokenClass::Keyword)]);
    let line = &method_view.lines[1];
    // `label = "` is 9 units, `é` one, `😀` two, `"; price = ` eleven.
    assert_eq!(line.calls[0].start, 4 + 23);
    assert_eq!(
        utf16_slice(&line.text, line.calls[0].start, line.calls[0].end),
        "compute"
    );

    // Views are whole lines: the statement shares its line, dedented on its own.
    let call_view = view(&playground, Target::Block(graph.key(call)));
    assert_eq!(shown(&call_view), ["3 label = \"é😀\"; price = compute(label)"]);
    assert_eq!(
        call_view.mark.as_ref().unwrap().expression,
        [LineRange {
            line: 3,
            start: 23,
            end: 37,
        }]
    );
    let widened = view_of(
        &playground,
        Target::Block(graph.key(call)),
        Some(Target::Block(graph.key(method))),
    );
    assert_eq!(numbers(&widened), [2, 4]);
    assert_eq!(marked_lines(&widened), Some([3, 3]));
    assert_eq!(
        widened.mark.as_ref().unwrap().expression,
        [LineRange {
            line: 3,
            start: 27,
            end: 41,
        }]
    );
    assert_eq!(expression(&widened), ["compute(label)"]);
}

#[test]
fn a_changed_file_shows_whole_as_it_is_now_with_nothing_marked() {
    let shop = shop();
    let now = format!("# edited\n{}", sample::SERVICE_PY);
    std::fs::write(shop.root.join(SERVICE), &now).unwrap();
    let changed = view(&shop, block("shop/service.py#place_order/try[0]/except[0]"));
    assert_eq!(changed.state, CodeState::Changed);
    assert_eq!(numbers(&changed), [1, 29]);
    assert_eq!(changed.lines[0].text, "# edited");
    assert_eq!(changed.mark, None);
    assert!(changed.definitions.is_empty());
    assert!(
        changed
            .lines
            .iter()
            .all(|line| line.block.is_none() && line.calls.is_empty())
    );
    // Still highlighted; the path keeps its records' labels, without source text.
    assert_eq!(changed.lines[9].tokens, [Token(0, 3, TokenClass::Keyword)]);
    assert_eq!(
        steps(&changed),
        ["*shop/service.py*", "place_order", "try", "except"]
    );

    // unfold.toml changed: the whole file.
    std::fs::write(shop.root.join("unfold.toml"), "[[app]]\nname = \"Shop\"\n").unwrap();
    let start = view(
        &shop,
        Target::Start {
            key: Key::from_text("entrance:shop"),
            span: toml_span("[[app]]"),
        },
    );
    assert_eq!(start.state, CodeState::Changed);
    assert_eq!(shown(&start), ["1 [[app]]", "2 name = \"Shop\""]);
}

#[test]
fn a_file_that_stopped_parsing_shows_whole() {
    let mut shop = shop();
    let file = shop
        .graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == SERVICE)
        .map(std::sync::Arc::make_mut)
        .unwrap();
    file.file.state = FileState::LastGood;
    std::fs::write(shop.root.join(SERVICE), "def place_order(:\n").unwrap();
    let broken = view(&shop, block("shop/service.py#place_order/call:sum"));
    assert_eq!(broken.state, CodeState::DoesNotParse);
    assert_eq!(shown(&broken), ["1 def place_order(:"]);
    assert_eq!(broken.mark, None);
    assert_eq!(steps(&broken), ["*shop/service.py*", "place_order", "sum"]);
}

#[test]
fn an_unfold_toml_that_does_not_parse_shows_whole() {
    let shop = shop();
    std::fs::write(shop.root.join("unfold.toml"), "[[app]\nname = \"Shop\"\n").unwrap();
    let app = Target::Start {
        key: Key::from_text("entrance:shop"),
        span: toml_span("[[app]]"),
    };
    let broken = view_with(&shop, app.clone(), None, false);
    assert_eq!(broken.state, CodeState::DoesNotParse);
    assert_eq!(shown(&broken), ["1 [[app]", "2 name = \"Shop\""]);
    assert_eq!(broken.mark, None);
    let file = view_with(&shop, Target::File(path("unfold.toml")), None, false);
    assert_eq!(file.state, CodeState::DoesNotParse);

    // Whether it parses is the settings' state alone: the same text that parses reads changed.
    assert_eq!(view(&shop, app).state, CodeState::Changed);
}

#[test]
fn a_file_outside_the_graph_is_not_analyzed() {
    let shop = shop();
    std::fs::create_dir_all(shop.root.join("scripts")).unwrap();
    std::fs::write(shop.root.join("scripts/seed.py"), "def seed():\n    return 1\n").unwrap();
    let outside = view(&shop, Target::File(path("scripts/seed.py")));
    assert_eq!(outside.state, CodeState::NotAnalyzed);
    assert_eq!(shown(&outside), ["1 def seed():", "2     return 1"]);
    assert!(outside.lines.iter().all(|line| line.tokens.is_empty()));
    assert_eq!(steps(&outside), ["*scripts/seed.py*"]);
    let cited = view(&shop, lines("scripts/seed.py", [2, 2]));
    assert_eq!(cited.state, CodeState::NotAnalyzed);
    assert_eq!(numbers(&cited), [1, 2]);
}

#[test]
fn a_key_that_no_longer_resolves_is_not_found() {
    let shop = shop();
    let gone = view(&shop, block("shop/service.py#refund_order"));
    assert_eq!(
        gone,
        CodeView {
            file: None,
            state: CodeState::NotFound,
            lines: Vec::new(),
            mark: None,
            path: Vec::new(),
            definitions: Vec::new(),
        }
    );
    std::fs::remove_file(shop.root.join(SERVICE)).unwrap();
    let deleted = view(&shop, block(PLACE_ORDER));
    assert_eq!(deleted.state, CodeState::NotFound);
    assert_eq!(deleted.file, Some(path(SERVICE)));
    assert!(deleted.lines.is_empty());
    let absent = view(&shop, Target::File(path("absent.py")));
    assert_eq!(absent.state, CodeState::NotFound);
}

#[test]
fn the_model_serializes_in_camel_case() {
    let shop = shop();
    let charge = view(
        &shop,
        block("shop/service.py#place_order/try[0]/body/call:charge_card"),
    );
    let json = serde_json::to_value(&charge).unwrap();
    assert_eq!(json["state"], "current");
    assert_eq!(json["lines"][0]["number"], 19);
    assert_eq!(json["mark"]["expression"][0]["start"], 10);
    assert!(json["mark"].get("lines").is_none());
    assert_eq!(json["path"][3]["showing"], true);
    let changed = serde_json::to_value(CodeState::DoesNotParse).unwrap();
    assert_eq!(changed, "doesNotParse");
}

/// A call in a part of a ternary or comprehension is inside an expression: it shows the whole
/// statement that holds the expression, not only its own line.
#[test]
fn a_call_in_a_ternary_shows_the_whole_statement() {
    const STATEMENT: &str = "x = (\n        first()\n        if c\n        else second()\n    )";
    const CODE: &str =
        "def pick(c):\n    x = (\n        first()\n        if c\n        else second()\n    )\n";
    let mut g = GraphBuilder::new();
    let file = g.file("v.py", Language::Python, CODE);
    let function = g.def(file, Form::Function, "pick", g.find(file, CODE.trim_end()));
    let statement = g.find(file, STATEMENT);
    g.node_mut(function).head = Some(g.find(file, "def pick(c)"));
    g.node_mut(function).body = Some(statement);
    let written = g.find(file, "first()\n        if c\n        else second()");
    let ternary = g.block(function, Form::Ternary, written);
    g.node_mut(ternary).stmt = Some(statement);
    g.node_mut(ternary).binds = Some("x".into());
    let call = |g: &mut GraphBuilder, part: NodeRef, callee: &str| {
        let call = g.call(part, &Callee::name(callee), g.find(file, &format!("{callee}()")));
        let head = g.find(file, callee);
        g.link(call, LinkKind::Calls, head, callee, unknown(Reason::NotFound));
    };
    let then = g.block(ternary, Form::Then, written);
    let test = g.find(file, "if c");
    g.node_mut(then).head = Some(ByteRange::new(test.end - 1, test.end).unwrap());
    g.node_mut(then).body = Some(written);
    call(&mut g, then, "first");
    let otherwise = g.block(ternary, Form::Else, written);
    g.node_mut(otherwise).body = Some(written);
    call(&mut g, otherwise, "second");

    let playground = Playground::built(&g);
    let first = view(&playground, block("v.py#pick/ternary[0]/then/call:first"));
    assert_eq!(
        shown(&first),
        [
            "2 x = (",
            "3     first()",
            "4     if c",
            "5     else second()",
            "6 )"
        ]
    );
    assert_eq!(expression(&first), ["first()"]);
    let second = view(&playground, block("v.py#pick/ternary[0]/else/call:second"));
    assert_eq!(numbers(&second), [2, 6]);
}
