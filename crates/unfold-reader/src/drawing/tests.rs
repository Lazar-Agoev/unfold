//! The drawing function and agent text: the route rules, the presentation rules, the nine
//! worked examples, `place_order` drawn and shaped, and the
//! playgrounds' drawn-flow goldens.

use std::path::Path;

use rustc_hash::FxHashMap;
use unfold_core::highlight::Highlighter;
use unfold_core::testkit::{GraphBuilder, assert_golden, builtin, library, project, sample, unknown};
use unfold_core::vocab::{Flag, Form, Language, LinkKind, Outcome, Reason};
use unfold_core::{Key, LineSpan, LinkTarget};

use super::examples::{self, Code, ORDERS_HANDLER};
use super::*;
use crate::source::SourceCache;
use crate::testing::Playground;

/// The real highlighters.
const HIGHLIGHTERS: &[&dyn Highlighter] = &[
    &unfold_lang_python::PythonHighlighter,
    &unfold_lang_typescript::TypeScriptHighlighter,
];

/// Text from the files under `root`, keeping what verified in `kept`.
fn sources<'a>(root: &'a Path, kept: &'a SourceCache) -> TextSources<'a> {
    TextSources {
        root,
        highlighters: HIGHLIGHTERS,
        kept,
    }
}

fn key(text: &str) -> Key {
    Key::from_text(text)
}

const PLACE_ORDER: &str = "shop/service.py#place_order";

/// Draws `definition` from `graph` with `overlay`, opening `open`.
fn drawn_with(
    graph: &GraphBuilder,
    definition: &str,
    overlay: &Overlay,
    open: &[&[&str]],
    all: bool,
) -> DrawnFlow {
    let playground = Playground::built(graph);
    let trees = playground.trees();
    let card = Card {
        definition: key(definition),
        open: open
            .iter()
            .map(|path| path.iter().map(|step| key(step)).collect())
            .collect(),
        show_all_calls: all,
    };
    draw(
        &trees,
        &sources(&playground.root, &SourceCache::new()),
        overlay,
        &card,
    )
    .expect("the definition draws")
}

fn drawn(graph: &GraphBuilder, definition: &str) -> DrawnFlow {
    drawn_with(graph, definition, &Overlay::default(), &[], false)
}

/// The flow's agent text without the keys at the ends of its lines.
fn plain(flow: &DrawnFlow) -> String {
    let mut out = String::new();
    for line in flow_text(flow).lines() {
        out.push_str(line.split_once("   key=").map_or(line, |(text, _)| text));
        out.push('\n');
    }
    out
}

/// What each id of `sequence` draws, for reading routes.
fn names(sequence: &Sequence, out: &mut FxHashMap<u32, String>) {
    for step in &sequence.steps {
        let name = match &step.shape {
            Shape::Call(call) => call.callee.clone(),
            Shape::Thin(row) => row.calls[0].callee.clone(),
            Shape::Exit(exit) => match &exit.value {
                Some(ExitValue::Call(call)) => format!("{} {}", exit.form, call.callee),
                Some(ExitValue::Text { text }) => format!("{} {text}", exit.form),
                None => exit.form.to_string(),
            },
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &frame.lanes {
                    let test = lane.test.as_ref().and_then(|test| test.text.as_deref());
                    let lane_name = match test {
                        Some(test) => format!("{} {test}", lane.form),
                        None => lane.form.to_string(),
                    };
                    out.insert(lane.id, lane_name);
                    names(&lane.steps, out);
                }
                frame.form.to_string()
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &combined.steps {
                    names(steps, out);
                }
                combined.title.clone()
            }
            Shape::Proposed(proposed) => proposed.title.clone(),
        };
        out.insert(step.id, name);
    }
}

/// The flow's routes as `from -kind-> to` lines.
fn routes(flow: &DrawnFlow) -> Vec<String> {
    let mut known = FxHashMap::default();
    known.insert(0, "start".to_owned());
    names(&flow.steps, &mut known);
    let name = |id: u32| known.get(&id).cloned().unwrap_or_else(|| format!("#{id}"));
    flow.routes
        .iter()
        .map(|route| {
            let to = match route.to {
                RouteEnd::Step(id) => name(id),
                RouteEnd::End => "End".to_owned(),
                RouteEnd::Caller => "caller".to_owned(),
            };
            let through: Vec<String> = route.through.iter().map(|&id| name(id)).collect();
            let via = if through.is_empty() {
                String::new()
            } else {
                format!(" via {}", through.join(", "))
            };
            format!("{} -{:?}-> {to}{via}", name(route.from), route.kind)
        })
        .collect()
}

fn assert_routes(flow: &DrawnFlow, expected: &[&str]) {
    let actual = routes(flow);
    for route in expected {
        assert!(
            actual.iter().any(|known| known == route),
            "{route} in {actual:#?}"
        );
    }
}

/// The steps of `sequence` and of the lanes and open combined steps inside, in order.
fn all_steps(sequence: &Sequence) -> Vec<&Step> {
    let mut out = Vec::new();
    for step in &sequence.steps {
        out.push(step);
        match &step.shape {
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &frame.lanes {
                    out.extend(all_steps(&lane.steps));
                }
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &combined.steps {
                    out.extend(all_steps(steps));
                }
            }
            _ => {}
        }
    }
    out
}

const PLACE_ORDER_DRAWN: &str = r#"( place_order )
  ▸ load_order → order
  IF order is None
      RAISE ValueError → the caller
  ELIF order["paid"]
      RETURN order → End
  ⋯ 1 line
  TRY
      ▸ charge_card → receipt
  ON ERROR → except PaymentDeclined as error
      · logger.warning
      RETURN ▸ mark_failed → End
  ⋯ 1 line
  ▸ save_order
  LOOP for item in order["items"]
      ▸ reserve_stock
  RETURN order → End
( End )
"#;

/// Example 01 drawn: every step connected, builtins hidden, the library
/// call `logger.warning` in a thin row, and the two gap ticks for `total = sum(…)` and
/// `order["receipt"] = receipt`.
#[test]
fn place_order_draws_as_the_design_shows() {
    let flow = drawn(&sample::shop(), PLACE_ORDER);
    assert_eq!(plain(&flow), PLACE_ORDER_DRAWN);
    assert_eq!(
        flow.header.description.as_deref(),
        Some("Charge an order and save it.")
    );
    assert_eq!(
        flow.header.signature.as_deref(),
        Some("def place_order(order_id)")
    );
    assert_eq!(
        (flow.header.path.as_str(), flow.header.line),
        ("shop/service.py", 9)
    );
    assert!(
        all_steps(&flow.steps)
            .iter()
            .all(|step| step.state == RouteState::Reached)
    );
    let gaps: Vec<LineSpan> = all_steps(&flow.steps)
        .iter()
        .filter_map(|step| step.gap.as_ref().map(|gap| gap.lines))
        .collect();
    assert_eq!(
        gaps,
        [LineSpan { start: 17, end: 17 }, LineSpan { start: 24, end: 24 }]
    );
}

/// While its file changes on disk or stops parsing, a card keeps the words of the text its facts
/// came from, kept from when it last verified, and its header says so in the code panel's words.
/// With no such text kept, it says so without the words.
#[test]
fn a_changed_file_keeps_its_words_and_says_so() {
    let mut shop = Playground::built(&sample::shop());
    let card = Card {
        definition: key(PLACE_ORDER),
        open: Vec::new(),
        show_all_calls: false,
    };
    let draw_from = |shop: &Playground, kept: &SourceCache| {
        draw(
            &shop.trees(),
            &sources(&shop.root, kept),
            &Overlay::default(),
            &card,
        )
        .expect("the definition draws")
    };
    let kept = SourceCache::new();
    let current = draw_from(&shop, &kept);
    assert_eq!(current.header.state, FlowState::Current);
    assert_eq!(plain(&current), PLACE_ORDER_DRAWN);

    let service = shop.root.join("shop/service.py");
    let text = std::fs::read_to_string(&service).unwrap();
    std::fs::write(&service, format!("{text}\n# edited\n")).unwrap();
    let changed = draw_from(&shop, &kept);
    assert_eq!(changed.header.state, FlowState::Changed);
    assert_eq!(
        plain(&changed),
        PLACE_ORDER_DRAWN.replacen(
            "( place_order )",
            "( place_order ) · Changed since the last compile",
            1
        )
    );
    assert_eq!(
        changed.header,
        Header {
            state: FlowState::Changed,
            ..current.header
        }
    );

    let unseen = draw_from(&shop, &SourceCache::new());
    assert_eq!(unseen.header.state, FlowState::Changed);
    assert_eq!(
        (
            unseen.header.signature.as_deref(),
            unseen.header.description.as_deref()
        ),
        (None, None)
    );
    assert!(plain(&unseen).contains("  IF\n"), "{}", plain(&unseen));

    let file = shop
        .graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "shop/service.py")
        .map(std::sync::Arc::make_mut)
        .expect("the service's file");
    file.file.state = unfold_core::vocab::FileState::LastGood;
    std::fs::write(&service, "def place_order(:\n").unwrap();
    let broken = draw_from(&shop, &kept);
    assert_eq!(broken.header.state, FlowState::DoesNotParse);
    assert_eq!(
        plain(&broken),
        PLACE_ORDER_DRAWN.replacen("( place_order )", "( place_order ) · Doesn't parse", 1)
    );
}

#[test]
fn place_order_routes_follow_the_routes_table() {
    let flow = drawn(&sample::shop(), PLACE_ORDER);
    assert_eq!(
        routes(&flow),
        [
            "start -On-> load_order",
            "load_order -On-> if",
            "if -On-> then order is None",
            "then order is None -Yes-> raise ValueError",
            "then order is None -No-> elif order[\"paid\"]",
            "raise ValueError -Raise-> caller",
            "elif order[\"paid\"] -Yes-> return order",
            "elif order[\"paid\"] -No-> try",
            "return order -Return-> End",
            "try -On-> body",
            "body -OnError-> except except PaymentDeclined as error",
            "body -On-> charge_card",
            "charge_card -On-> save_order",
            "except except PaymentDeclined as error -On-> logger.warning",
            "logger.warning -On-> return mark_failed",
            "return mark_failed -Return-> End",
            "save_order -On-> for",
            "for -Each-> body",
            "for -Done-> return order",
            "body -On-> reserve_stock",
            "reserve_stock -Next-> for",
            "return order -Return-> End",
        ]
    );
}

/// Example 02: a `switch` whose cases return and throw, and whose no-match continues.
#[test]
fn a_switch_is_an_ordinary_decision() {
    let flow = drawn(&examples::place_order_ts(), "src/service.ts#placeOrder");
    assert_eq!(
        plain(&flow),
        r#"( placeOrder )
  ▸ loadOrder → order · AWAIT
  SWITCH order.status
  CASE "paid"
      RETURN order → End
  CASE "cancelled"
      THROW Error → the caller
  ▸ chargeCard → order.receipt · AWAIT
  ▸ saveOrder · AWAIT
  RETURN order → End
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "switch -Case-> case \"paid\"",
            "switch -Case-> case \"cancelled\"",
            "switch -Default-> chargeCard",
            "throw Error -Raise-> caller",
        ],
    );
    assert!(flow.header.is_async);
}

/// Example 03: a with frame around a try with every part; the `return` inside the handler
/// leaves through `finally`.
#[test]
fn a_try_routes_through_its_parts_and_finally() {
    let flow = drawn(&examples::sync_inbox(), "inbox/sync.py#sync_inbox");
    assert_eq!(
        plain(&flow),
        r#"( sync_inbox )
  WITH mailbox_lock(account) · ▸ mailbox_lock
      TRY
          ▸ fetch_messages → messages
      ON ERROR → except RateLimited as error
          ▸ schedule_retry
          RETURN 0 → finally → End
      ELSE
          ▸ store_messages
      FINALLY
          ▸ record_attempt
  ▸ notify_synced
  RETURN len → End
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "with -On-> body",
            "body -On-> try",
            "body -OnError-> except except RateLimited as error",
            "fetch_messages -On-> else",
            "store_messages -Finally-> finally",
            "return 0 -Return-> End via finally",
            "record_attempt -On-> notify_synced",
        ],
    );
}

/// Example 04: `continue` goes back to the header, `break` past the loop and its `else`.
#[test]
fn loops_route_each_next_done_break_and_continue() {
    let flow = drawn(
        &examples::reserve_first_free(),
        "booking/slots.py#reserve_first_free",
    );
    assert_eq!(
        plain(&flow),
        r#"( reserve_first_free )
  LOOP for slot in list_slots(calendar, day) · ▸ list_slots
      IF slot.blocked
          CONTINUE → next
      IF is_free(slot) · ▸ is_free
          ▸ reserve
          BREAK → after the loop
  LOOP ELSE
      RAISE ▸ NoFreeSlot → the caller
  ▸ notify_reserved
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "for -Each-> body",
            "for -Done-> else",
            "continue -Continue-> for",
            "break -Break-> notify_reserved",
            "then is_free(slot) -No-> for",
            "raise NoFreeSlot -Raise-> caller",
        ],
    );
}

/// Example 05: a ternary is a decision and a comprehension a loop whose filter is its body's
/// test; a short-circuit is not a decision, so its call is SOMETIMES.
#[test]
fn expressions_that_decide_or_repeat_are_frames() {
    let flow = drawn(&examples::route_message(), "mail/route.py#route_message");
    assert_eq!(
        plain(&flow),
        r#"( route_message )
  ▸ cached_profile
  ▸ fetch_profile · SOMETIMES
  TERNARY → decision
      IF override
          ▸ Routed · new
      ELSE
          ▸ classify
  COMPREHENSION for part in message.parts → labels
      WHEN is_relevant(part) · ▸ is_relevant
      ▸ label_for
  RETURN ▸ deliver → End
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "ternary -On-> then override",
            "then override -Yes-> Routed",
            "then override -No-> else",
            "comprehension -Each-> body is_relevant(part)",
            "body is_relevant(part) -Yes-> label_for",
            "body is_relevant(part) -Next-> comprehension",
            "label_for -Next-> comprehension",
            "comprehension -Done-> return deliver",
        ],
    );
}

/// Example 06: project calls in the order they run, each marked with the call it feeds; the
/// loop that calls nothing is hidden and named in its gap tick.
#[test]
fn calls_in_calls_run_inside_out() {
    let flow = drawn(&examples::checkout(), "shop/checkout.py#checkout");
    assert_eq!(
        plain(&flow),
        r#"( checkout )
  ▸ charge_card · into build_receipt(…)
  ▸ build_receipt · into save_order(…)
  ▸ save_order
  ⋯ 3 lines · for loop
  ▸ order_label · into logger.info(…)
  · logger.info ?
  ▸ summary · into json.dumps(…)
  · audit.record ? + json.dumps
( End )
"#
    );
    let gap = all_steps(&flow.steps)
        .into_iter()
        .find_map(|step| step.gap.clone())
        .expect("a gap tick");
    assert_eq!(gap.lines, LineSpan { start: 15, end: 17 });
    assert_eq!(gap.hidden[0].key.as_str(), "shop/checkout.py#checkout/for[0]");
}

/// Example 07: a module's own code is a flow; `main` opens in place, and `place_order` opens
/// again inside it.
#[test]
fn a_module_draws_its_code_and_opens_calls_in_place() {
    let flow = drawn_with(
        &examples::cli(),
        "shop/cli.py#",
        &Overlay::default(),
        &[
            &["shop/cli.py#/if[0]/then/call:main"],
            &[
                "shop/cli.py#/if[0]/then/call:main",
                "shop/cli.py#main/call:place_order",
            ],
        ],
        false,
    );
    let inset: String = PLACE_ORDER_DRAWN
        .lines()
        .map(|line| match line {
            "( End )" => "                ( returns )\n".to_owned(),
            line => format!("                {}\n", line.replace("→ End", "→ returns")),
        })
        .collect();
    let expected = format!(
        r#"( shop/cli.py )
  imports · sys, shop.service
  IF __name__ == "__main__"
      ▸ main
          ( main )
            ▸ place_order
{inset}          ( returns )
( End )
"#
    );
    assert_eq!(plain(&flow), expected);
}

/// Example 08: decorators and defaults are calls of the enclosing body, gathered into one thin
/// row there, and listed as "runs when defined" on the definition's card.
#[test]
fn decorators_and_defaults_run_in_the_enclosing_body() {
    let graph = examples::api();
    assert_eq!(
        plain(&drawn(&graph, "shop/api.py#")),
        r#"( shop/api.py )
  imports · fastapi, shop.auth, shop.service
  · FastAPI → app, APIRouter → router, @router.post ?, Depends, app.include_router ?   5 calls, 2 unknown
( End )
"#
    );
    let card = drawn(&graph, "shop/api.py#create_order");
    assert_eq!(
        plain(&card),
        r#"( create_order )
  runs when defined · @router.post ?, Depends (default)
  RETURN ▸ place_order → End
( End )
"#
    );
    let roles: Vec<Option<CallRole>> = card.header.defined_with.iter().map(|call| call.role).collect();
    assert_eq!(roles, [Some(CallRole::Decorator), Some(CallRole::Default)]);
}

/// Example 09: an inline handler is a callback row under its registration call, and a card of
/// its own.
#[test]
fn an_inline_handler_is_a_callback_and_a_card() {
    let graph = examples::orders_route();
    let module = drawn(&graph, "src/routes/orders.ts#");
    assert_eq!(
        plain(&module),
        r#"( src/routes/orders.ts )
  imports · express, ../service
  · Router → ordersRouter
  □ ordersRouter.post ?
      ↳ callback async (req, res) =>
( End )
"#
    );
    let Shape::Call(post) = &module.steps.steps[1].shape else {
        panic!("the registration call is a box");
    };
    assert!(post.callbacks[0].actions.open);
    assert_eq!(post.callbacks[0].key.as_str(), ORDERS_HANDLER);
    assert_eq!(
        plain(&drawn(&graph, ORDERS_HANDLER)),
        r#"( callback to ordersRouter.post )
  TRY
      ▸ placeOrder → order · AWAIT
      · res.json ?
  ON ERROR → catch (error)
      · res.status().json ?
( End )
"#
    );
    let opened = drawn_with(
        &graph,
        "src/routes/orders.ts#",
        &Overlay::default(),
        &[&[ORDERS_HANDLER]],
        false,
    );
    assert!(
        plain(&opened)
            .contains("      ↳ callback async (req, res) =>\n          ( callback to ordersRouter.post )\n")
    );
}

/// The overlay of a `shape_card` example: a title, a named handler, and three
/// combined steps.
fn shaped() -> Overlay {
    let step = |relative: &str| key(&format!("{PLACE_ORDER}/{relative}"));
    let combine = |id: &str, title: &str, members: &[&str]| Combine {
        id: id.to_owned(),
        title: title.to_owned(),
        members: members.iter().map(|member| step(member)).collect(),
        collapsed: true,
    };
    Overlay {
        names: [
            (key(PLACE_ORDER), "Place an order".to_owned()),
            (step("try[0]/except[0]"), "Card declined".to_owned()),
        ]
        .into_iter()
        .collect(),
        combined: vec![
            combine("stop", "Stop if it's missing or already paid", &["if[0]"]),
            combine(
                "failed",
                "Mark the order failed",
                &[
                    "try[0]/except[0]/call:logger.warning",
                    "try[0]/except[0]/return[0]",
                ],
            ),
            combine(
                "save",
                "Save it and reserve stock",
                &["call:save_order", "for[0]"],
            ),
        ],
        proposed: Vec::new(),
    }
}

/// The `shape_card` result, line for line (keys aside).
#[test]
fn the_shaped_place_order_reads_as_the_shape_card_result() {
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &shaped(), &[], false);
    assert_eq!(
        plain(&flow),
        r#"( Place an order )
  ▸ load_order → order
  ⊞ Stop if it's missing or already paid   2 conditions · raises → the caller · returns → End
  ⋯ 1 line
  TRY
      ▸ charge_card → receipt
  ON ERROR → Card declined · except PaymentDeclined as error
      ⊞ Mark the order failed   mark_failed · 1 library · returns → End
  ⋯ 1 line
  ⊞ Save it and reserve stock   1 loop · save_order · reserve_stock
  RETURN order → End
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "Stop if it's missing or already paid -Raise-> caller",
            "Stop if it's missing or already paid -Return-> End",
            "Stop if it's missing or already paid -On-> try",
            "Mark the order failed -Return-> End",
            "Save it and reserve stock -On-> return order",
        ],
    );
    let text = flow_text(&flow);
    assert!(text.contains(
        "⊞ Save it and reserve stock   1 loop · save_order · reserve_stock   key=shop/service.py#place_order/call:save_order shop/service.py#place_order/for[0]\n"
    ));
    let Shape::Combined(stop) = &flow.steps.steps[1].shape else {
        panic!("the decision is combined");
    };
    assert_eq!(stop.lines, LineSpan { start: 12, end: 15 });
    assert!(stop.steps.is_none());
}

/// A step written between a combined step's blocks since it was combined joins it: the combined
/// step draws from its first block through its last, as if it listed every block between.
#[test]
fn a_step_written_between_a_combined_steps_blocks_draws_inside_it() {
    let combined = |members: &[&str]| {
        let overlay = Overlay {
            combined: vec![Combine {
                id: "head".to_owned(),
                title: "Load it and charge it".to_owned(),
                members: members
                    .iter()
                    .map(|member| key(&format!("{PLACE_ORDER}/{member}")))
                    .collect(),
                collapsed: true,
            }],
            ..Overlay::default()
        };
        let flow = drawn_with(&sample::shop(), PLACE_ORDER, &overlay, &[], false);
        let Shape::Combined(step) = &flow.steps.steps[0].shape else {
            panic!("the first step is combined:\n{}", plain(&flow));
        };
        (plain(&flow), step.clone())
    };
    let (text, joined) = combined(&["call:load_order", "try[0]"]);
    let (listed_text, listed) = combined(&["call:load_order", "if[0]", "try[0]"]);
    assert_eq!(text, listed_text);
    assert_eq!(
        CombinedStep {
            members: listed.members.clone(),
            ..joined
        },
        listed
    );
    assert_eq!(listed.conditions, 2, "the decision between them is inside");
}

/// Combined steps apply page-wide: inside another card's opened call too. Open, a combined
/// step draws its steps under its title.
#[test]
fn combined_steps_apply_inside_insets_and_open_in_place() {
    let flow = drawn_with(
        &examples::cli(),
        "shop/cli.py#main",
        &shaped(),
        &[&["shop/cli.py#main/call:place_order"]],
        false,
    );
    let text = plain(&flow);
    assert!(text.contains("    ( Place an order )\n"), "{text}");
    assert!(text.contains(
        "⊞ Stop if it's missing or already paid   2 conditions · raises → the caller · returns → returns"
    ));

    let mut open = shaped();
    for combine in &mut open.combined {
        combine.collapsed = false;
    }
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &open, &[], false);
    assert!(plain(&flow).contains(
        "  ⊟ Save it and reserve stock   1 loop · save_order · reserve_stock\n      ▸ save_order\n      LOOP for item in order[\"items\"]\n"
    ));
    assert_routes(
        &flow,
        &[
            "Save it and reserve stock -On-> save_order",
            "for -Done-> return order",
        ],
    );
}

/// A combined step's blocks, and what is written in them, can't be combined again; collapsed,
/// it holds every one of them, drawn or not, so it outlines them.
#[test]
fn a_combined_steps_blocks_are_held_and_not_combined_again() {
    let in_place_order = |relative: &str| key(&format!("{PLACE_ORDER}/{relative}"));
    let mut open = shaped();
    for combine in &mut open.combined {
        combine.collapsed = false;
    }
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &open, &[], false);
    let Shape::Call(load) = &flow.steps.steps[0].shape else {
        panic!("load_order is a box");
    };
    assert!(load.actions.combine.is_some());
    let Shape::Combined(stop) = &flow.steps.steps[1].shape else {
        panic!("the decision is combined");
    };
    assert!(stop.holds.is_empty());
    let inner = stop
        .steps
        .as_ref()
        .expect("an open combined step draws its steps");
    let Shape::Decision(decision) = &inner.steps[0].shape else {
        panic!("the decision is a frame");
    };
    assert_eq!(decision.actions.combine, None);
    let Shape::Exit(raise) = &decision.lanes[0].steps.steps[0].shape else {
        panic!("the raise is an exit");
    };
    assert_eq!(raise.actions.combine, None);
    assert!(raise.actions.after);

    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &shaped(), &[], false);
    let Shape::Combined(stop) = &flow.steps.steps[1].shape else {
        panic!("the decision is combined");
    };
    assert_eq!(
        stop.holds,
        [
            "if[0]",
            "if[0]/then",
            "if[0]/then/raise[0]",
            "if[0]/then/raise[0]/call:ValueError",
            "if[0]/elif[0]",
            "if[0]/elif[0]/return[0]",
        ]
        .map(in_place_order)
    );
}

/// `walk` calls itself, so its card can open the call in place at every level.
fn walker() -> GraphBuilder {
    const CODE: &str = "def walk(node):\n    walk(node.next)\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "tree/walk.py", Language::Python, CODE);
    let module = c.module;
    let walk = c.function(module, "walk", 1, 2, "def walk(node)");
    c.call(
        walk,
        "walk",
        2,
        "walk(node.next)",
        project(&key("tree/walk.py#walk")),
    );
    g
}

/// Whether the first call of `flow`, and of each inset opened under it, offers to open in place.
fn opens_by_level(flow: &DrawnFlow) -> Vec<bool> {
    let mut levels = Vec::new();
    let mut at = Some(flow);
    while let Some(flow) = at {
        let Shape::Call(call) = &flow.steps.steps[0].shape else {
            panic!("the call is a box");
        };
        levels.push(call.actions.open);
        at = call.inset.as_deref();
    }
    levels
}

/// A call opens in place within three levels and 64 paths per card; an open one can always be
/// closed.
#[test]
fn calls_open_in_place_within_three_levels_and_sixty_four_paths() {
    const WALK: &str = "tree/walk.py#walk";
    const CALL: &str = "tree/walk.py#walk/call:walk";
    let deep: &[&[&str]] = &[&[CALL], &[CALL, CALL], &[CALL, CALL, CALL]];
    let flow = drawn_with(&walker(), WALK, &Overlay::default(), deep, false);
    assert_eq!(opens_by_level(&flow), [true, true, true, false]);

    let others: Vec<String> = (0..63).map(|index| format!("{WALK}/call:other{index}")).collect();
    let with_others = |count: usize| -> Vec<Vec<&str>> {
        std::iter::once(vec![CALL])
            .chain(others[..count].iter().map(|other| vec![other.as_str()]))
            .collect()
    };
    let open = with_others(62);
    let paths: Vec<&[&str]> = open.iter().map(Vec::as_slice).collect();
    let flow = drawn_with(&walker(), WALK, &Overlay::default(), &paths, false);
    assert_eq!(opens_by_level(&flow), [true, true]);
    let full = with_others(63);
    let paths: Vec<&[&str]> = full.iter().map(Vec::as_slice).collect();
    let flow = drawn_with(&walker(), WALK, &Overlay::default(), &paths, false);
    assert_eq!(opens_by_level(&flow), [true, false]);
}

/// A part carries the text that opens it, as the code panel's path labels it, and the lines it
/// marks there; a TypeScript `else if` reads `ELSE IF` in agent text.
#[test]
fn parts_carry_their_opening_text_and_lines() {
    let flow = drawn(&sample::shop(), PLACE_ORDER);
    let Shape::Decision(decision) = &flow.steps.steps[1].shape else {
        panic!("the decision is a frame");
    };
    let lanes: Vec<(&str, LineSpan)> = decision
        .lanes
        .iter()
        .map(|lane| (lane.label.as_str(), lane.lines))
        .collect();
    assert_eq!(
        lanes,
        [
            ("order is None", LineSpan { start: 12, end: 13 }),
            ("elif order[\"paid\"]", LineSpan { start: 14, end: 15 }),
        ]
    );
    let Shape::Try(attempt) = &flow.steps.steps[2].shape else {
        panic!("the try is a frame");
    };
    assert_eq!(attempt.lanes[0].lines, LineSpan { start: 18, end: 19 });
    assert_eq!(attempt.lanes[1].label, "except PaymentDeclined as error");

    let ts = Playground::load("ts-orders");
    let trees = ts.trees();
    let card = Card {
        definition: key("src/lib/validate.ts#validateOrder"),
        open: Vec::new(),
        show_all_calls: false,
    };
    let flow = draw(
        &trees,
        &sources(&ts.root, &SourceCache::new()),
        &Overlay::default(),
        &card,
    )
    .expect("the definition draws");
    let text = plain(&flow);
    assert!(text.contains("\n  ELSE IF typeof email !== \"string\""), "{text}");
    assert!(!text.contains("ELIF"), "{text}");
}

/// Show all calls draws the builtins; a name lifts a library or unknown call out of its thin
/// row into a box, which keeps its `?`.
#[test]
fn builtins_show_on_request_and_names_lift_calls_into_boxes() {
    let all = drawn_with(&sample::shop(), PLACE_ORDER, &Overlay::default(), &[], true);
    let text = plain(&all);
    assert!(text.contains("  ▸ load_order → order\n"));
    assert!(
        text.contains("      RETURN order → End\n  · sum → total\n  TRY\n"),
        "{text}"
    );

    let overlay = Overlay {
        names: [(
            key(&format!("{PLACE_ORDER}/try[0]/except[0]/call:logger.warning")),
            "Log the decline".to_owned(),
        )]
        .into_iter()
        .collect(),
        ..Overlay::default()
    };
    let named = drawn_with(&sample::shop(), PLACE_ORDER, &overlay, &[], false);
    assert!(plain(&named).contains("      □ Log the decline · logger.warning\n"));
}

/// Proposed steps go after a block or at the start of a body, on the route.
#[test]
fn proposals_sit_after_a_block_or_at_the_start_of_a_body() {
    let overlay = Overlay {
        proposed: vec![
            Proposal {
                id: "audit".to_owned(),
                spot: ProposalSpot::After(key(&format!("{PLACE_ORDER}/call:save_order"))),
                title: "Write an audit row".to_owned(),
            },
            Proposal {
                id: "retry".to_owned(),
                spot: ProposalSpot::Start(key(&format!("{PLACE_ORDER}/try[0]/except[0]"))),
                title: "Retry once".to_owned(),
            },
        ],
        ..Overlay::default()
    };
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &overlay, &[], false);
    let text = plain(&flow);
    assert!(
        text.contains("  ▸ save_order\n  + PROPOSED Write an audit row\n  LOOP"),
        "{text}"
    );
    assert!(text.contains(
        "  ON ERROR → except PaymentDeclined as error\n      + PROPOSED Retry once\n      · logger.warning\n"
    ));
    assert_routes(
        &flow,
        &[
            "save_order -On-> Write an audit row",
            "Write an audit row -On-> for",
        ],
    );
}

/// `sum.py`: a loop with nothing drawn inside, which hides it.
fn summing() -> GraphBuilder {
    const CODE: &str =
        "def add(xs):\n    total = 0\n    for x in xs:\n        total += x\n    return total\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "sum.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "add", 1, 5, "def add(xs)");
    let header = c.at(3, "for x in xs");
    let loop_block = c.block(function, Form::For, c.lines(3, 4), Some(header), None);
    c.suite(loop_block, Form::Body, 4, 4);
    c.exit(function, Form::Return, 5, Some("total"));
    g
}

fn propose(spots: &[(&str, ProposalSpot)]) -> Overlay {
    Overlay {
        proposed: spots
            .iter()
            .map(|(title, spot)| Proposal {
                id: title.to_lowercase().replace(' ', "-"),
                spot: spot.clone(),
                title: (*title).to_owned(),
            })
            .collect(),
        ..Overlay::default()
    }
}

/// A proposal after a hidden control block sits after the code the block's gap tick counts.
#[test]
fn a_proposal_after_a_hidden_block_follows_its_gap_tick() {
    let overlay = propose(&[("Log the total", ProposalSpot::After(key("sum.py#add/for[0]")))]);
    let flow = drawn_with(&summing(), "sum.py#add", &overlay, &[], false);
    assert_eq!(
        plain(&flow),
        "( add )\n  ⋯ 3 lines · for loop\n  + PROPOSED Log the total\n  RETURN total → End\n( End )\n"
    );
}

/// What the overlay puts inside a control block draws it as a frame, though nothing else in it
/// is drawn: a proposal at the start of its body, or a name on one of its parts.
#[test]
fn the_overlay_draws_a_hidden_block_it_touches() {
    let overlay = propose(&[
        (
            "Validate each item",
            ProposalSpot::Start(key("sum.py#add/for[0]/body")),
        ),
        ("Log the total", ProposalSpot::After(key("sum.py#add/for[0]"))),
    ]);
    let flow = drawn_with(&summing(), "sum.py#add", &overlay, &[], false);
    assert_eq!(
        plain(&flow),
        "( add )
  ⋯ 1 line
  LOOP for x in xs
      + PROPOSED Validate each item
      ⋯ 1 line
  + PROPOSED Log the total
  RETURN total → End
( End )
"
    );
    assert_routes(
        &flow,
        &[
            "body -On-> Validate each item",
            "Validate each item -Next-> for",
            "for -Done-> Log the total",
        ],
    );

    let named = drawn_with(
        &summing(),
        "sum.py#add",
        &naming("sum.py#add", &[("for[0]/body", "Add each item")]),
        &[],
        false,
    );
    assert_eq!(
        plain(&named),
        "( add )\n  ⋯ 1 line\n  LOOP Add each item · for x in xs\n      ⋯ 1 line\n  RETURN total → End\n( End )\n"
    );
    // A name on the block itself is its gap tick's.
    let tick = drawn_with(
        &summing(),
        "sum.py#add",
        &naming("sum.py#add", &[("for[0]", "Add them up")]),
        &[],
        false,
    );
    assert_eq!(
        plain(&tick),
        "( add )\n  ⋯ 3 lines · Add them up · for loop\n  RETURN total → End\n( End )\n"
    );
}

/// A gap tick counts every line of a string, comment marks and all, and no line after a case's
/// `break` that only a comment follows.
#[test]
fn gap_ticks_count_the_lines_of_strings_and_skip_comments() {
    const PYTHON: &str =
        "def f():\n    text = \"\"\"\n# looks like a comment\n\"\"\"\n    # a comment\n    send(text)\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "f.py", Language::Python, PYTHON);
    let module = c.module;
    let function = c.function(module, "f", 1, 6, "def f()");
    c.call(function, "send", 6, "send(text)", project(&key("f.py#send")));
    assert_eq!(
        plain(&drawn(&g, "f.py#f")),
        "( f )\n  ⋯ 3 lines\n  ▸ send\n( End )\n"
    );

    const TYPESCRIPT: &str = "function g(x: string) {\n  switch (x) {\n    case \"a\":\n      a();\n      break; // done with a\n  }\n}\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "g.ts", Language::TypeScript, TYPESCRIPT);
    let module = c.module;
    let function = c.def(
        module,
        Form::Function,
        "g",
        c.lines(1, 7),
        c.at(1, "function g(x: string)"),
        Code::span(c.at(1, "{"), c.line(7)),
    );
    let switch = c.block(function, Form::Switch, c.lines(2, 6), Some(c.at(2, "x")), None);
    let case = c.block(
        switch,
        Form::Case,
        c.lines(3, 5),
        Some(c.at(3, "\"a\"")),
        Some(c.lines(4, 5)),
    );
    c.call(case, "a", 4, "a()", project(&key("g.ts#a")));
    assert_eq!(
        plain(&drawn(&g, "g.ts#g")),
        "( g )\n  SWITCH x\n  CASE \"a\"\n      ▸ a\n( End )\n"
    );
}

/// `checks.py`: tests and headers that call unknown, builtin, and library functions.
fn checks() -> GraphBuilder {
    const CODE: &str = r#"def check(xs, path, thing):
    if thing.mystery():
        a()
    if len(xs):
        a()
    if os.path.exists(path):
        a()
    for x in thing.fetch():
        a()
    with thing.resource() as r:
        a()
    if thing.idle():
        x = 1
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "checks.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "check", 1, 13, "def check(xs, path, thing)");
    let a = || project(&key("checks.py#a"));
    let decision = |c: &mut Code<'_>, line: u32, test: &str, callee: &str, to: LinkTarget| {
        let decision = c.block(function, Form::If, c.lines(line, line + 1), None, None);
        let then = c.part(decision, Form::Then, line, line + 1, Some(test));
        let call = c.call(then, callee, line, test, to);
        c.set(call).role = Some(CallRole::Test);
        c.set(call).stmt = None;
        then
    };
    for (line, test, callee, to) in [
        (
            2,
            "thing.mystery()",
            "thing.mystery",
            unknown(Reason::UnknownReceiver),
        ),
        (4, "len(xs)", "len", builtin("python:builtins")),
        (6, "os.path.exists(path)", "os.path.exists", library("os")),
    ] {
        let then = decision(&mut c, line, test, callee, to);
        c.call(then, "a", line + 1, "a()", a());
    }
    let header = c.at(8, "for x in thing.fetch()");
    let loop_block = c.block(function, Form::For, c.lines(8, 9), Some(header), None);
    c.call(
        loop_block,
        "thing.fetch",
        8,
        "thing.fetch()",
        unknown(Reason::UnknownReceiver),
    );
    let body = c.suite(loop_block, Form::Body, 9, 9);
    c.call(body, "a", 9, "a()", a());
    let header = c.at(10, "thing.resource() as r");
    let with = c.block(function, Form::With, c.lines(10, 11), Some(header), None);
    c.call(
        with,
        "thing.resource",
        10,
        "thing.resource()",
        unknown(Reason::UnknownReceiver),
    );
    let body = c.suite(with, Form::Body, 11, 11);
    c.call(body, "a", 11, "a()", a());
    decision(
        &mut c,
        12,
        "thing.idle()",
        "thing.idle",
        unknown(Reason::UnknownReceiver),
    );
    g
}

/// A test's or header's unknown calls are listed after it with their `?`, like the boxes drawn
/// on it, so a block whose test calls one is drawn. Library and builtin calls stay its written
/// text, with Show all calls too.
#[test]
fn unknown_calls_in_tests_and_headers_keep_their_question_mark() {
    const DRAWN: &str = "( check )
  IF thing.mystery() · thing.mystery ?
      ▸ a
  IF len(xs)
      ▸ a
  IF os.path.exists(path)
      ▸ a
  LOOP for x in thing.fetch() · thing.fetch ?
      ▸ a
  WITH thing.resource() as r · thing.resource ?
      ▸ a
  IF thing.idle() · thing.idle ?
      ⋯ 1 line
( End )
";
    let flow = drawn(&checks(), "checks.py#check");
    assert_eq!(plain(&flow), DRAWN);
    let all = drawn_with(&checks(), "checks.py#check", &Overlay::default(), &[], true);
    assert_eq!(plain(&all), DRAWN);

    let text = flow_text(&flow);
    assert!(
        text.contains("IF thing.mystery() · thing.mystery ?   key=checks.py#check/if[0] checks.py#check/if[0]/then checks.py#check/if[0]/then/call:thing.mystery\n"),
        "{text}"
    );
    assert!(
        text.contains("LOOP for x in thing.fetch() · thing.fetch ?   key=checks.py#check/for[0] checks.py#check/for[0]/body checks.py#check/for[0]/call:thing.fetch\n"),
        "{text}"
    );
    let Shape::Loop(frame) = &flow.steps.steps[3].shape else {
        panic!("a loop: {:?}", flow.steps.steps[3].shape);
    };
    let unknown: Vec<&str> = frame
        .header_unknown
        .iter()
        .map(|call| call.callee.as_str())
        .collect();
    assert_eq!(unknown, ["thing.fetch"]);
    let Shape::Decision(frame) = &flow.steps.steps[0].shape else {
        panic!("a decision: {:?}", flow.steps.steps[0].shape);
    };
    let test = frame.lanes[0].test.as_ref().expect("the test");
    let unknown: Vec<&str> = test.unknown.iter().map(|call| call.key.as_str()).collect();
    assert_eq!(unknown, ["checks.py#check/if[0]/then/call:thing.mystery"]);
}

/// `leave.py`: library and unknown calls written inside a test's box and an exit's value.
fn leaving() -> GraphBuilder {
    const CODE: &str = r#"def leave(x, logger):
    if check(x.baz()):
        return a(x.foo(), logger.info("hi"))
    raise ValueError(x.bar())
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "leave.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "leave", 1, 4, "def leave(x, logger)");
    let decision = c.block(function, Form::If, c.lines(2, 3), None, None);
    let then = c.part(decision, Form::Then, 2, 3, Some("check(x.baz())"));
    let check = c.call(
        then,
        "check",
        2,
        "check(x.baz())",
        project(&key("leave.py#check")),
    );
    c.set(check).role = Some(CallRole::Test);
    c.set(check).stmt = None;
    c.call(check, "x.baz", 2, "x.baz()", unknown(Reason::UnknownReceiver));
    let exit = c.exit(then, Form::Return, 3, Some("a(x.foo(), logger.info(\"hi\"))"));
    let value = c.call(
        exit,
        "a",
        3,
        "a(x.foo(), logger.info(\"hi\"))",
        project(&key("leave.py#a")),
    );
    c.flag(value, Flag::ExitValue);
    c.call(value, "x.foo", 3, "x.foo()", unknown(Reason::UnknownReceiver));
    c.call(value, "logger.info", 3, "logger.info(\"hi\")", library("logging"));
    let raise = c.exit(function, Form::Raise, 4, Some("ValueError(x.bar())"));
    let error = c.call(
        raise,
        "ValueError",
        4,
        "ValueError(x.bar())",
        builtin("python:builtins"),
    );
    c.flag(error, Flag::ExitValue);
    c.call(error, "x.bar", 4, "x.bar()", unknown(Reason::UnknownReceiver));
    g
}

/// The calls listed on a box drawn on a test, or on an exit's value, are printed with it, and
/// every line carries their keys.
#[test]
fn calls_listed_on_tests_and_exits_are_printed_with_their_keys() {
    let flow = drawn(&leaving(), "leave.py#leave");
    assert_eq!(
        plain(&flow),
        "( leave )
  IF check(x.baz()) · ▸ check + x.baz ?
      RETURN ▸ a + x.foo ?, logger.info → End
  RAISE ValueError + x.bar ? → the caller
( End )
"
    );
    let text = flow_text(&flow);
    let then = "leave.py#leave/if[0]/then";
    for line in [
        format!("   key=leave.py#leave/if[0] {then} {then}/call:check {then}/call:check/call:x.baz\n"),
        format!("   key={then}/return[0] {then}/return[0]/call:a {then}/return[0]/call:a/call:x.foo {then}/return[0]/call:a/call:logger.info\n"),
        "   key=leave.py#leave/raise[0] leave.py#leave/raise[0]/call:ValueError leave.py#leave/raise[0]/call:ValueError/call:x.bar\n".to_owned(),
    ] {
        assert!(text.contains(&line), "{line} in {text}");
    }
}

/// A callback written as an object's property reads as that property, then its signature.
#[test]
fn a_callback_row_shows_its_property_then_its_signature() {
    const CODE: &str = "function useThing() {\n  useMutation({ onSuccess: () => refresh(), onError: (e: unknown) => report(e) });\n  [1].map((n) => n);\n}\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "t.ts", Language::TypeScript, CODE);
    let module = c.module;
    let function = c.def(
        module,
        Form::Function,
        "useThing",
        c.lines(1, 4),
        c.at(1, "function useThing()"),
        Code::span(c.at(1, "{"), c.line(4)),
    );
    let mutation = c.call(
        function,
        "useMutation",
        2,
        "useMutation({ onSuccess: () => refresh(), onError: (e: unknown) => report(e) })",
        library("react-query"),
    );
    for (index, (property, written, head, body)) in [
        ("onSuccess", "() => refresh()", "() =>", "refresh()"),
        (
            "onError",
            "(e: unknown) => report(e)",
            "(e: unknown) =>",
            "report(e)",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let callback = c.callback(
            mutation,
            u32::try_from(index).unwrap(),
            c.at(2, written),
            c.at(2, head),
            c.at(2, body),
        );
        c.set(callback).name = Some(property.into());
    }
    let map = c.call(function, "[1].map", 3, "[1].map((n) => n)", builtin("ecmascript"));
    c.callback(map, 0, c.at(3, "(n) => n"), c.at(3, "(n) =>"), c.last(3, "n"));
    let text = plain(&drawn(&g, "t.ts#useThing"));
    assert_eq!(
        text,
        "( useThing )
  □ useMutation
      ↳ callback onSuccess () =>
      ↳ callback onError (e: unknown) =>
  □ [1].map
      ↳ callback (n) =>
( End )
"
    );
}

/// An overlay that names each of `blocks` (keys relative to `definition`).
fn naming(definition: &str, blocks: &[(&str, &str)]) -> Overlay {
    Overlay {
        names: blocks
            .iter()
            .map(|&(block, name)| (key(&format!("{definition}/{block}")), name.to_owned()))
            .collect(),
        ..Overlay::default()
    }
}

/// The names of `sequence`'s frames and of the frames inside them, by key.
fn frame_names(sequence: &Sequence) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for step in all_steps(sequence) {
        if let Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) =
            &step.shape
        {
            out.push((frame.key.as_str().to_owned(), frame.name.clone()));
        }
    }
    out
}

/// A name on any control block's frame is drawn and printed beside its code, as a name on one
/// of its lanes is; a hidden control block's name goes in its gap tick.
#[test]
fn every_control_block_shows_its_name() {
    let overlay = naming(
        PLACE_ORDER,
        &[
            ("if[0]", "Stop early"),
            ("if[0]/then", "Is it missing?"),
            ("for[0]", "Reserve each item"),
            ("try[0]", "Charge the card"),
        ],
    );
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &overlay, &[], false);
    let text = plain(&flow);
    assert!(
        text.contains("  IF Stop early · Is it missing? · order is None\n"),
        "{text}"
    );
    assert!(text.contains("  ELIF order[\"paid\"]\n"), "{text}");
    assert!(text.contains("  TRY · Charge the card\n"), "{text}");
    assert!(
        text.contains("  LOOP Reserve each item · for item in order[\"items\"]\n"),
        "{text}"
    );
    let decision = format!("{PLACE_ORDER}/if[0]");
    assert!(
        flow_text(&flow).contains(&format!(
            "IF Stop early · Is it missing? · order is None   key={decision} {decision}/then\n"
        )),
        "the frame's key is on its first lane's line"
    );
    let names = frame_names(&flow.steps);
    for block in ["if[0]", "for[0]", "try[0]"] {
        let key = format!("{PLACE_ORDER}/{block}");
        let name = names
            .iter()
            .find(|(frame, _)| *frame == key)
            .and_then(|(_, name)| name.clone());
        assert!(name.is_some(), "{block} in {names:?}");
    }

    let both = naming(
        PLACE_ORDER,
        &[
            ("try[0]", "Charge the card"),
            ("try[0]/body", "Ask the bank"),
            ("for[0]", "Reserve each item"),
            ("for[0]/body", "Reserve one"),
        ],
    );
    let flow = drawn_with(&sample::shop(), PLACE_ORDER, &both, &[], false);
    let text = plain(&flow);
    assert!(
        text.contains("  TRY · Charge the card · Ask the bank\n"),
        "a block's own name and its body's both show: {text}"
    );
    assert!(
        text.contains("  LOOP Reserve each item · Reserve one · for item in order[\"items\"]\n"),
        "{text}"
    );

    let definition = "src/service.ts#placeOrder";
    let overlay = naming(definition, &[("switch[0]", "By status")]);
    let flow = drawn_with(&examples::place_order_ts(), definition, &overlay, &[], false);
    assert!(plain(&flow).contains("  SWITCH By status · order.status\n"));
    assert_eq!(
        frame_names(&flow.steps),
        [(format!("{definition}/switch[0]"), Some("By status".to_owned()))]
    );

    let definition = "inbox/sync.py#sync_inbox";
    let overlay = naming(
        definition,
        &[
            ("with[0]", "Hold the mailbox"),
            ("with[0]/body/try[0]", "Fetch the mail"),
        ],
    );
    let flow = drawn_with(&examples::sync_inbox(), definition, &overlay, &[], false);
    let text = plain(&flow);
    assert!(
        text.contains(
            "  WITH Hold the mailbox · mailbox_lock(account) · ▸ mailbox_lock\n      TRY · Fetch the mail\n"
        ),
        "{text}"
    );
    assert_eq!(
        frame_names(&flow.steps),
        [
            (
                format!("{definition}/with[0]"),
                Some("Hold the mailbox".to_owned())
            ),
            (
                format!("{definition}/with[0]/body/try[0]"),
                Some("Fetch the mail".to_owned())
            ),
        ]
    );

    let definition = "shop/checkout.py#checkout";
    let overlay = naming(definition, &[("for[0]", "Log each line")]);
    let flow = drawn_with(&examples::checkout(), definition, &overlay, &[], false);
    assert!(plain(&flow).contains("  ⋯ 3 lines · Log each line · for loop\n"));
}

/// A small Python file, `stop.py`.
fn stop() -> GraphBuilder {
    const CODE: &str = r#"def stop(order):
    if order:
        return order
    else:
        raise ValueError("no order")
    save(order)


def guard(order):
    try:
        raise ValueError("bad")
    except ValueError:
        return None
    except KeyError:
        return None
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "stop.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "stop", 1, 6, "def stop(order)");
    let decision = c.block(function, Form::If, c.lines(2, 5), None, None);
    let then = c.part(decision, Form::Then, 2, 3, Some("order"));
    c.exit(then, Form::Return, 3, Some("order"));
    let otherwise = c.part(decision, Form::Else, 4, 5, None);
    let raise = c.exit(otherwise, Form::Raise, 5, Some("ValueError(\"no order\")"));
    let error = c.call(
        raise,
        "ValueError",
        5,
        "ValueError(\"no order\")",
        unknown(Reason::NotFound),
    );
    c.flag(error, Flag::ExitValue);
    c.call(function, "save", 6, "save(order)", project(&key("stop.py#save")));
    let guard = c.function(module, "guard", 9, 15, "def guard(order)");
    let try_block = c.block(guard, Form::Try, c.lines(10, 15), None, None);
    let body = c.suite(try_block, Form::Body, 11, 11);
    let raise = c.exit(body, Form::Raise, 11, Some("ValueError(\"bad\")"));
    let error = c.call(
        raise,
        "ValueError",
        11,
        "ValueError(\"bad\")",
        builtin("python:builtins"),
    );
    c.flag(error, Flag::ExitValue);
    for (line, class) in [(12, "ValueError"), (14, "KeyError")] {
        let clause = format!("except {class}");
        let handler = c.part(try_block, Form::Except, line, line + 1, Some(&clause));
        let named = c.last(line, class);
        c.g.link(handler, LinkKind::Types, named, class, builtin("python:builtins"));
        c.exit(handler, Form::Return, line + 1, Some("None"));
    }
    g
}

/// Code after a decision whose every lane exits never runs; a raise in a try body goes to the
/// handler that catches it.
#[test]
fn unreachable_steps_never_run_and_raises_reach_handlers() {
    let graph = stop();
    let flow = drawn(&graph, "stop.py#stop");
    assert_eq!(
        plain(&flow),
        r#"( stop )
  IF order
      RETURN order → End
  ELSE
      RAISE ValueError ? → the caller
  ▸ save · never runs
( End )
"#
    );
    let guard = drawn(&graph, "stop.py#guard");
    assert!(plain(&guard).contains("      RAISE ValueError → except ValueError\n"));
    assert_routes(
        &guard,
        &[
            "raise ValueError -Raise-> except except ValueError",
            "body -OnError-> except except KeyError",
        ],
    );
    assert!(
        !routes(&guard)
            .iter()
            .any(|route| route == "raise ValueError -Raise-> except except KeyError")
    );
}

/// `pick.ts`: a TypeScript `break` that leaves a `switch` early.
fn pick() -> GraphBuilder {
    const CODE: &str = r#"function pick(kind: string) {
  switch (kind) {
    case "a":
      if (kind) break;
      first();
  }
  second();
}
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "pick.ts", Language::TypeScript, CODE);
    let module = c.module;
    let body = Code::span(c.at(1, "{"), c.lines(8, 8));
    let function = c.def(
        module,
        Form::Function,
        "pick",
        c.lines(1, 8),
        c.at(1, "function pick(kind: string)"),
        body,
    );
    let switch = c.block(
        function,
        Form::Switch,
        c.lines(2, 6),
        Some(c.at(2, "switch (kind)")),
        None,
    );
    let case = c.block(
        switch,
        Form::Case,
        c.lines(3, 5),
        Some(c.at(3, "\"a\"")),
        Some(c.lines(4, 5)),
    );
    let decision = c.block(case, Form::If, c.line(4), None, None);
    let then = c.block(
        decision,
        Form::Then,
        c.line(4),
        Some(c.at(4, "(kind)")),
        Some(c.at(4, "break;")),
    );
    let leave = c.block(then, Form::Break, c.at(4, "break;"), None, None);
    c.set(leave).stmt = Some(c.at(4, "break;"));
    c.set(leave).target = Some(switch.local_id());
    c.call(case, "first", 5, "first()", project(&key("pick.ts#first")));
    c.call(function, "second", 7, "second()", project(&key("pick.ts#second")));
    g
}

/// A TypeScript `break` that leaves a `switch` early goes past the switch.
#[test]
fn a_break_may_leave_a_switch() {
    let g = pick();
    let flow = drawn(&g, "pick.ts#pick");
    assert_eq!(
        plain(&flow),
        r#"( pick )
  SWITCH switch (kind)
  CASE "a"
      IF (kind)
          BREAK → after the switch
      ▸ first
  ▸ second
( End )
"#
    );
    assert_routes(&flow, &["break -Break-> second", "switch -Default-> second"]);
}

/// A case that falls through runs into the next case; a default takes what no case matches.
#[test]
fn cases_fall_through_and_default() {
    let playground = Playground::load("ts-orders");
    let trees = playground.trees();
    let card = Card {
        definition: key("src/cli.ts#main"),
        open: Vec::new(),
        show_all_calls: false,
    };
    let flow = draw(
        &trees,
        &sources(&playground.root, &SourceCache::new()),
        &Overlay::default(),
        &card,
    )
    .expect("main draws");
    assert_routes(
        &flow,
        &[
            "case \"replay\" -On-> case \"retry\"",
            "switch -Default-> default",
            "switch -Case-> case \"import\"",
        ],
    );
    assert!(
        !routes(&flow)
            .iter()
            .any(|route| route.starts_with("switch -Default-> ") && route.ends_with("End"))
    );
}

/// A `yield` goes on: the path continues when the consumer resumes it.
#[test]
fn a_yield_goes_on() {
    let playground = Playground::load("py-orders");
    let trees = playground.trees();
    let card = Card {
        definition: key("src/orders_playground/features/control.py#records"),
        open: Vec::new(),
        show_all_calls: false,
    };
    let flow = draw(
        &trees,
        &sources(&playground.root, &SourceCache::new()),
        &Overlay::default(),
        &card,
    )
    .expect("records draws");
    assert_routes(&flow, &["yield normalize -Next-> for", "for -Done-> End"]);
}

/// A truncated block may continue: it keeps a dashed route on.
#[test]
fn truncated_steps_may_continue() {
    const CODE: &str = "def run(jobs):\n    start(jobs)\n    finish(jobs)\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "run.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "run", 1, 3, "def run(jobs)");
    let start = c.call(function, "start", 2, "start(jobs)", project(&key("run.py#start")));
    c.flag(start, Flag::Truncated);
    c.call(
        function,
        "finish",
        3,
        "finish(jobs)",
        project(&key("run.py#finish")),
    );
    let flow = drawn(&g, "run.py#run");
    assert_eq!(
        plain(&flow),
        "( run )\n  ▸ start · truncated\n  ▸ finish\n( End )\n"
    );
    assert_routes(&flow, &["start -On-> finish", "start -MayContinue-> finish"]);
    assert_eq!(flow.steps.steps[0].state, RouteState::Truncated);
}

/// Each block says what the overlay may do to it, from the overlay rules.
#[test]
fn blocks_carry_their_overlay_actions() {
    let flow = drawn(&sample::shop(), PLACE_ORDER);
    let body = CombineWith::Steps {
        body: key(PLACE_ORDER),
    };
    assert_eq!(
        flow.header.actions.combine,
        Some(CombineWith::Steps {
            body: key("shop/service.py#")
        })
    );
    assert!(flow.header.actions.start);
    assert_eq!(flow.combinable.len(), 7);
    let Shape::Call(load) = &flow.steps.steps[0].shape else {
        panic!("load_order is a box");
    };
    assert_eq!(
        load.actions,
        Actions {
            combine: Some(body.clone()),
            after: true,
            start: false,
            open: true,
        }
    );
    let Shape::Try(frame) = &flow.steps.steps[2].shape else {
        panic!("the try is a frame");
    };
    assert_eq!(frame.actions.combine, Some(body));
    let handler = &frame.lanes[1];
    assert!(handler.actions.start && !handler.actions.after && handler.actions.combine.is_none());
    assert_eq!(handler.combinable.len(), 2);
    assert_eq!(handler.types[0].outcome, Outcome::Project);

    let route = drawn(&examples::route_message(), "mail/route.py#route_message");
    let Shape::Decision(ternary) = &route.steps.steps[2].shape else {
        panic!("the ternary is a frame");
    };
    assert_eq!(
        ternary.actions.combine,
        Some(CombineWith::Steps {
            body: key("mail/route.py#route_message")
        })
    );
}

/// The drawn flow crosses IPC as camelCase JSON with its kinds as words.
#[test]
fn the_drawn_flow_serializes_for_the_canvas() {
    let flow = drawn(&examples::checkout(), "shop/checkout.py#checkout");
    let json = serde_json::to_value(&flow).expect("the flow serializes");
    assert_eq!(json["header"]["key"], "shop/checkout.py#checkout");
    let first = &json["steps"]["steps"][0];
    assert_eq!(first["kind"], "call");
    assert_eq!(first["state"], "reached");
    assert_eq!(first["feeds"]["callee"], "build_receipt");
    assert_eq!(first["to"]["outcome"], "project");
    assert_eq!(
        json["steps"]["steps"][3]["gap"]["lines"],
        serde_json::json!([15, 17])
    );
    assert_eq!(
        json["routes"][0],
        serde_json::json!({"from": 0, "kind": "on", "to": {"step": 1}, "through": []})
    );
}

/// `read_code`'s compiled tree: the header, one line per block with its short label and full
/// key, and the code that isn't a block.
#[test]
fn the_compiled_tree_prints_every_block_with_its_key() {
    let playground = Playground::built(&sample::shop());
    let (tree, id) = playground.block(PLACE_ORDER);
    let code = code_text(&tree, &sources(&playground.root, &SourceCache::new()), id);
    let lines: Vec<&str> = code
        .text
        .lines()
        .map(|line| {
            line.split_once("   key=")
                .map_or(line, |(text, _)| text)
                .trim_end()
        })
        .collect();
    assert_eq!(
        lines,
        [
            "shop/service.py#place_order",
            "  function · lines 9–28 · doc \"Charge an order and save it.\"",
            "",
            "L11  call:load_order                           ▸ shop/store.py#load_order · binds order",
            "L12  if[0]                                     if",
            "L12  if[0]/then                                test order is None",
            "L13  if[0]/then/raise[0]                       raise · value ValueError(\"unknown order\")",
            "L13  if[0]/then/raise[0]/call:ValueError       builtin · exit value",
            "L14  if[0]/elif[0]                             test order[\"paid\"]",
            "L15  if[0]/elif[0]/return[0]                   return · value order",
            "L17  call:sum                                  builtin · binds total",
            "L18  try[0]                                    try",
            "L19  try[0]/body                               body",
            "L19  try[0]/body/call:charge_card              ▸ shop/payments.py#charge_card · binds receipt",
            "L20  try[0]/except[0]                          clause except PaymentDeclined as error · types ▸ shop/payments.py#PaymentDeclined",
            "L21  try[0]/except[0]/call:logger.warning      library python:logging",
            "L22  try[0]/except[0]/return[0]                return · value mark_failed(order)",
            "L22  try[0]/except[0]/return[0]/call:mark_failed ▸ shop/store.py#mark_failed · exit value",
            "L24  ⋯ order[\"receipt\"] = receipt              code",
            "L25  call:save_order                           ▸ shop/store.py#save_order",
            "L26  for[0]                                    for · for item in order[\"items\"]",
            "L27  for[0]/body                               body",
            "L27  for[0]/body/call:reserve_stock            ▸ shop/store.py#reserve_stock",
            "L28  return[0]                                 return · value order",
        ]
    );
    assert!(code.text.contains(
        "  ▸ shop/payments.py#charge_card · binds receipt   key=shop/service.py#place_order/try[0]/body/call:charge_card\n"
    ));
    let label = code
        .labels
        .iter()
        .find(|label| label.label == "try[0]/except[0]")
        .expect("the handler is labeled");
    assert_eq!(label.key.as_str(), "shop/service.py#place_order/try[0]/except[0]");
    assert_eq!(code.labels.len(), 20);
    // The label map is the tree's, in the printed order; it reads no file.
    let printed: Vec<&str> = lines[3..]
        .iter()
        .filter(|line| !line.contains('⋯'))
        .map(|line| line.split_whitespace().nth(1).expect("a label"))
        .collect();
    let mapped = labels(&tree, id);
    assert_eq!(mapped, code.labels);
    assert_eq!(
        mapped
            .iter()
            .map(|label| label.label.as_str())
            .collect::<Vec<_>>(),
        printed
    );
    assert_eq!(
        code_text(&tree, &sources(Path::new("/nowhere"), &SourceCache::new()), id).labels,
        mapped
    );

    let module = Playground::built(&examples::orders_route());
    let (tree, id) = module.block("src/routes/orders.ts#");
    let code = code_text(&tree, &sources(&module.root, &SourceCache::new()), id);
    assert!(
        code.text
            .contains("L6  call:ordersRouter.post(\"/orders/:id\")/callback[0]"),
        "{}",
        code.text
    );
    assert!(!code.text.contains("call:placeOrder"));
}

/// Every definition of a playground, drawn with no overlay and printed as agent text.
fn playground_flows(playground: &Playground) -> String {
    let trees = playground.trees();
    let overlay = Overlay::default();
    let mut out = String::new();
    for tree in trees.iter() {
        for node in tree.nodes().iter().filter(|node| node.kind().is_definition()) {
            let card = Card {
                definition: node.key.clone(),
                open: Vec::new(),
                show_all_calls: false,
            };
            let flow = draw(
                &trees,
                &sources(&playground.root, &SourceCache::new()),
                &overlay,
                &card,
            )
            .expect("a definition draws");
            out.push_str("=== ");
            out.push_str(node.key.as_str());
            out.push('\n');
            out.push_str(&flow_text(&flow));
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_playgrounds_draw_as_their_goldens() {
    for name in ["ts-orders", "py-orders"] {
        assert_golden(
            format!("goldens/{name}.flows.txt"),
            &playground_flows(&Playground::load(name)),
        );
    }
    assert_golden(
        "goldens/mixed-playground.flows.txt",
        &playground_flows(&Playground::mixed()),
    );
}

/// `t.py`: tries whose body only returns or raises, with a `finally` and code after.
fn finally_after_exit() -> GraphBuilder {
    const CODE: &str = r#"def f():
    try:
        return helper(1)
    finally:
        helper(2)
    helper(3)


def g():
    try:
        raise Stop()
    finally:
        helper(2)
    helper(3)
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "t.py", Language::Python, CODE);
    let module = c.module;
    let helper = || project(&key("t.py#helper"));
    for (name, first, form, value) in [
        ("f", 1, Form::Return, "helper(1)"),
        ("g", 9, Form::Raise, "Stop()"),
    ] {
        let function = c.function(module, name, first, first + 5, &format!("def {name}()"));
        let try_block = c.block(function, Form::Try, c.lines(first + 1, first + 4), None, None);
        let body = c.suite(try_block, Form::Body, first + 2, first + 2);
        let exit = c.exit(body, form, first + 2, Some(value));
        let callee = &value[..value.find('(').unwrap()];
        let to = if form == Form::Return {
            helper()
        } else {
            unknown(Reason::NotFound)
        };
        let call = c.call(exit, callee, first + 2, value, to);
        c.flag(call, Flag::ExitValue);
        let finally = c.part(try_block, Form::Finally, first + 3, first + 4, None);
        c.call(finally, "helper", first + 4, "helper(2)", helper());
        c.call(function, "helper", first + 5, "helper(3)", helper());
    }
    g
}

/// An exit that passes through `finally` runs the lane and goes on to its own target: the
/// code after the try never runs, and a combined try takes no "on" route to it.
#[test]
fn an_exit_through_finally_goes_on_to_its_own_target() {
    let graph = finally_after_exit();
    let flow = drawn(&graph, "t.py#f");
    assert_eq!(
        plain(&flow),
        r#"( f )
  TRY
      RETURN ▸ helper → finally → End
  FINALLY
      ▸ helper
  ▸ helper · never runs
( End )
"#
    );
    let raised = drawn(&graph, "t.py#g");
    assert_eq!(
        plain(&raised),
        r#"( g )
  TRY
      RAISE Stop ? → finally → the caller
  FINALLY
      ▸ helper
  ▸ helper · never runs
( End )
"#
    );

    let overlay = Overlay {
        combined: vec![Combine {
            id: "try".to_owned(),
            title: "Try it".to_owned(),
            members: vec![key("t.py#f/try[0]")],
            collapsed: true,
        }],
        ..Overlay::default()
    };
    let combined = drawn_with(&graph, "t.py#f", &overlay, &[], false);
    assert_eq!(
        plain(&combined),
        "( f )\n  ⊞ Try it   helper · helper · returns → End\n  ▸ helper · never runs\n( End )\n"
    );
    let from_group: Vec<String> = routes(&combined)
        .into_iter()
        .filter(|route| route.starts_with("Try it "))
        .collect();
    assert_eq!(from_group, ["Try it -Return-> End"]);
}

/// `h.py`: a comprehension and a ternary written in tests.
fn expressions_in_tests() -> GraphBuilder {
    const CODE: &str = r#"def h(xs, b):
    if [helper(x) for x in xs]:
        done()
    if (a() if b else c()):
        done()
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "h.py", Language::Python, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("h.py#{name}")));
    let function = c.function(module, "h", 1, 5, "def h(xs, b)");

    let first = c.block(function, Form::If, c.lines(2, 3), None, None);
    let then = c.part(first, Form::Then, 2, 3, Some("[helper(x) for x in xs]"));
    let written = c.at(2, "[helper(x) for x in xs]");
    let comprehension = c.block(
        then,
        Form::Comprehension,
        written,
        Some(c.at(2, "for x in xs")),
        None,
    );
    let body = c.block(comprehension, Form::Body, written, None, Some(written));
    let helper = c.call(body, "helper", 2, "helper(x)", to("helper"));
    c.set(helper).stmt = None;
    c.call(then, "done", 3, "done()", to("done"));

    let second = c.block(function, Form::If, c.lines(4, 5), None, None);
    let then = c.part(second, Form::Then, 4, 5, Some("(a() if b else c())"));
    let written = c.at(4, "a() if b else c()");
    let ternary = c.block(then, Form::Ternary, written, None, None);
    let yes = c.block(
        ternary,
        Form::Then,
        written,
        Some(c.at(4, "b else")),
        Some(written),
    );
    let a = c.call(yes, "a", 4, "a()", to("a"));
    c.set(a).stmt = None;
    let no = c.block(ternary, Form::Else, written, None, Some(written));
    let other = c.call(no, "c", 4, "c()", to("c"));
    c.set(other).stmt = None;
    c.call(then, "done", 5, "done()", to("done"));
    g
}

/// A ternary or comprehension written in a test is part of the test: its boxes are drawn on
/// the test, the branch starts with what follows, and it is no step of the branch.
#[test]
fn expressions_in_a_test_belong_to_the_test() {
    let graph = expressions_in_tests();
    let flow = drawn(&graph, "h.py#h");
    assert_eq!(
        plain(&flow),
        r#"( h )
  IF [helper(x) for x in xs] · ▸ helper
      ▸ done
  IF (a() if b else c()) · ▸ a, ▸ c
      ▸ done
( End )
"#
    );
    assert_routes(
        &flow,
        &[
            "then [helper(x) for x in xs] -Yes-> done",
            "then (a() if b else c()) -Yes-> done",
        ],
    );
    let playground = Playground::built(&graph);
    for block in [
        "h.py#h/if[0]/then/comprehension[0]",
        "h.py#h/if[1]/then/ternary[0]",
    ] {
        let (tree, id) = playground.block(block);
        assert!(!crate::overlay::is_step(&tree, id), "{block}");
        assert_eq!(crate::overlay::combinable(&tree, id), None, "{block}");
        assert!(!crate::overlay::can_follow(&tree, id), "{block}");
    }
    let (tree, then) = playground.block("h.py#h/if[0]/then");
    assert_eq!(crate::overlay::steps(&tree, then).count(), 1);
}

/// A call written in a handler's clause is drawn on the clause, as a test's calls are on the
/// test, and is no step of the handler.
#[test]
fn a_call_in_a_handlers_clause_is_drawn_on_it() {
    const CODE: &str = "def f():\n    try:\n        work()\n    except errors():\n        return None\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "m.py", Language::Python, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("m.py#{name}")));
    let function = c.function(module, "f", 1, 5, "def f()");
    let try_block = c.block(function, Form::Try, c.lines(2, 5), None, None);
    let body = c.suite(try_block, Form::Body, 3, 3);
    c.call(body, "work", 3, "work()", to("work"));
    let handler = c.part(try_block, Form::Except, 4, 5, Some("except errors()"));
    let clause = c.call(handler, "errors", 4, "errors()", to("errors"));
    c.set(clause).role = Some(CallRole::Test);
    c.set(clause).stmt = None;
    c.exit(handler, Form::Return, 5, Some("None"));

    let flow = drawn(&g, "m.py#f");
    assert_eq!(
        plain(&flow),
        "( f )
  TRY
      ▸ work
  ON ERROR → except errors() · ▸ errors
      RETURN None → End
( End )
"
    );
    let Shape::Try(frame) = &flow.steps.steps[0].shape else {
        panic!("a try: {:?}", flow.steps.steps[0].shape);
    };
    let test = frame.lanes[1].test.as_ref().expect("the clause");
    assert_eq!(test.text.as_deref(), Some("except errors()"));
    let calls: Vec<&str> = test.calls.iter().map(|call| call.key.as_str()).collect();
    assert_eq!(calls, ["m.py#f/try[0]/except[0]/call:errors"]);
    assert!(test.calls[0].actions.open);
    assert_routes(&flow, &["body -OnError-> except except errors()"]);
}

/// A thin row counts the unknown calls listed inside its calls too.
#[test]
fn a_thin_row_counts_the_unknowns_inside_its_calls() {
    const CODE: &str = "def log(msg):\n    logger.info(msg.get(\"a\"))\n    logger.debug(msg.get(\"b\"))\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "log.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "log", 1, 3, "def log(msg)");
    for (line, method, argument) in [(2, "logger.info", "\"a\""), (3, "logger.debug", "\"b\"")] {
        let written = format!("{method}(msg.get({argument}))");
        let outer = c.call(function, method, line, &written, unknown(Reason::NotFound));
        c.call(
            outer,
            "msg.get",
            line,
            &format!("msg.get({argument})"),
            unknown(Reason::NotFound),
        );
    }
    let flow = drawn(&g, "log.py#log");
    assert_eq!(
        plain(&flow),
        "( log )\n  · logger.info ? + msg.get ?, logger.debug ? + msg.get ?   2 calls, 4 unknown\n( End )\n"
    );
    let Shape::Thin(row) = &flow.steps.steps[0].shape else {
        panic!("the calls fold into a thin row");
    };
    assert_eq!(row.unknown, 4);
}

/// A thin row covers the lines from its first call's statement to its last one's. A call with no
/// statement of its own (here, inside a hidden builtin) is in the nearest one above it, like the
/// code panel shows it.
#[test]
fn a_thin_row_spans_its_calls_statements() {
    const CODE: &str = "def log(msg):\n    print(\n        msg.get(\"a\"),\n    )\n    logger.debug(msg)\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "log.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "log", 1, 5, "def log(msg)");
    let print = c.call(function, "print", 2, "print(", builtin("print"));
    let statement = c.lines(2, 4);
    c.set(print).range = statement;
    c.set(print).stmt = Some(statement);
    c.call(print, "msg.get", 3, "msg.get(\"a\")", unknown(Reason::NotFound));
    c.call(
        function,
        "logger.debug",
        5,
        "logger.debug(msg)",
        unknown(Reason::NotFound),
    );
    let flow = drawn(&g, "log.py#log");
    assert_eq!(
        plain(&flow),
        "( log )\n  · msg.get ?, logger.debug ?   2 calls, 2 unknown\n( End )\n"
    );
    let Shape::Thin(row) = &flow.steps.steps[0].shape else {
        panic!("the calls fold into a thin row");
    };
    assert_eq!(row.lines, LineSpan { start: 2, end: 5 });
}

/// A combined step that breaks out of a `switch` says where it goes.
#[test]
fn a_combined_step_that_leaves_a_switch_says_so() {
    let overlay = Overlay {
        combined: vec![Combine {
            id: "early".to_owned(),
            title: "Leave early".to_owned(),
            members: vec![key("pick.ts#pick/switch[0]/case[0]/if[0]")],
            collapsed: true,
        }],
        ..Overlay::default()
    };
    let flow = drawn_with(&pick(), "pick.ts#pick", &overlay, &[], false);
    assert!(
        plain(&flow).contains("      ⊞ Leave early   1 condition · breaks → after the switch\n"),
        "{}",
        plain(&flow)
    );
    assert_routes(&flow, &["Leave early -Break-> second", "Leave early -On-> first"]);
}

/// `poll.ts`: `do … while` loops.
fn do_loops() -> GraphBuilder {
    const CODE: &str = r#"function poll() {
  do {
    step();
  } while (more());
  after();
}
function once() {
  do {
    return first();
  } while (more());
  after();
}
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "poll.ts", Language::TypeScript, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("poll.ts#{name}")));
    for (name, first) in [("poll", 1), ("once", 7)] {
        let function = c.def(
            module,
            Form::Function,
            name,
            c.lines(first, first + 5),
            c.at(first, &format!("function {name}()")),
            Code::span(c.at(first, "{"), c.line(first + 5)),
        );
        let written = Code::span(c.at(first + 1, "do"), c.line(first + 3));
        let header = c.at(first + 3, "while (more())");
        let do_loop = c.block(function, Form::Do, written, Some(header), None);
        let suite = Code::span(c.at(first + 1, "{"), c.at(first + 3, "}"));
        let body = c.block(do_loop, Form::Body, suite, None, Some(suite));
        if name == "poll" {
            c.call(body, "step", first + 2, "step()", to("step"));
        } else {
            let exit = c.exit(body, Form::Return, first + 2, Some("first()"));
            let call = c.call(exit, "first", first + 2, "first()", to("first"));
            c.flag(call, Flag::ExitValue);
        }
        c.call(do_loop, "more", first + 3, "more()", to("more"));
        c.call(function, "after", first + 4, "after()", to("after"));
    }
    g
}

/// A `do` loop enters its body first and tests after it, so done can't skip the body.
#[test]
fn a_do_loop_runs_its_body_before_its_test() {
    let graph = do_loops();
    let flow = drawn(&graph, "poll.ts#poll");
    assert_eq!(
        plain(&flow),
        "( poll )\n  DO\n      ▸ step\n  LOOP while (more()) · ▸ more\n  ▸ after\n( End )\n"
    );
    assert_routes(&flow, &["do -On-> body", "step -Next-> do", "step -Done-> after"]);
    assert!(!routes(&flow).iter().any(|route| route.starts_with("do -Done->")));
    let once = drawn(&graph, "poll.ts#once");
    assert_eq!(
        plain(&once),
        "( once )\n  DO\n      RETURN ▸ first → End\n  LOOP while (more()) · ▸ more\n  ▸ after · never runs\n( End )\n"
    );
}

/// A one-line callback with no blocks draws its gap tick.
#[test]
fn a_one_line_callback_draws_its_code() {
    const CODE: &str = "function paid(orders) {\n  return orders.filter((o) => o.paid);\n}\n";
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "p.ts", Language::TypeScript, CODE);
    let module = c.module;
    let function = c.def(
        module,
        Form::Function,
        "paid",
        c.lines(1, 3),
        c.at(1, "function paid(orders)"),
        Code::span(c.at(1, "{"), c.line(3)),
    );
    let exit = c.exit(function, Form::Return, 2, Some("orders.filter((o) => o.paid)"));
    let filter = c.call(
        exit,
        "orders.filter",
        2,
        "orders.filter((o) => o.paid)",
        unknown(Reason::NotFound),
    );
    c.flag(filter, Flag::ExitValue);
    let callback = c.callback(
        filter,
        0,
        c.at(2, "(o) => o.paid"),
        c.at(2, "(o) =>"),
        c.at(2, "o.paid"),
    );
    let callback = c.g.key(callback);
    let flow = drawn(&g, callback.as_str());
    assert_eq!(
        plain(&flow),
        "( callback to orders.filter )\n  ⋯ 1 line\n( End )\n"
    );
}

/// The boxes drawn on a header or a test carry their keys on its line.
#[test]
fn boxes_on_headers_and_tests_carry_their_keys() {
    let definition = "booking/slots.py#reserve_first_free";
    let text = flow_text(&drawn(&examples::reserve_first_free(), definition));
    assert!(
        text.contains(&format!(
            "LOOP for slot in list_slots(calendar, day) · ▸ list_slots   key={definition}/for[0] {definition}/for[0]/body {definition}/for[0]/call:list_slots\n"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "/if[1]/then {definition}/for[0]/body/if[1]/then/call:is_free\n"
        )),
        "{text}"
    );
    let route = flow_text(&drawn(&examples::route_message(), "mail/route.py#route_message"));
    assert!(
        route.contains("WHEN is_relevant(part) · ▸ is_relevant   key=mail/route.py#route_message/comprehension[0]/body/call:is_relevant\n"),
        "{route}"
    );
}

/// The drawn flows of the nine examples and the shaped `place_order`, as the JSON the IPC sends:
/// the canvas's layout tests read them from `testdata/goldens/drawn/`.
#[test]
fn the_examples_serialize_as_their_drawn_goldens() {
    let cli_opens: &[&[&str]] = &[
        &["shop/cli.py#/if[0]/then/call:main"],
        &[
            "shop/cli.py#/if[0]/then/call:main",
            "shop/cli.py#main/call:place_order",
        ],
    ];
    let none = Overlay::default();
    let flows = [
        ("01-place-order", drawn(&sample::shop(), PLACE_ORDER)),
        (
            "02-place-order-ts",
            drawn(&examples::place_order_ts(), "src/service.ts#placeOrder"),
        ),
        (
            "03-sync-inbox",
            drawn(&examples::sync_inbox(), "inbox/sync.py#sync_inbox"),
        ),
        (
            "04-reserve-first-free",
            drawn(
                &examples::reserve_first_free(),
                "booking/slots.py#reserve_first_free",
            ),
        ),
        (
            "05-route-message",
            drawn(&examples::route_message(), "mail/route.py#route_message"),
        ),
        (
            "06-checkout",
            drawn(&examples::checkout(), "shop/checkout.py#checkout"),
        ),
        (
            "07-cli",
            drawn_with(&examples::cli(), "shop/cli.py#", &none, cli_opens, false),
        ),
        ("08-api", drawn(&examples::api(), "shop/api.py#")),
        (
            "09-orders-route",
            drawn(&examples::orders_route(), "src/routes/orders.ts#"),
        ),
        (
            "place-order-shaped",
            drawn_with(&sample::shop(), PLACE_ORDER, &shaped(), &[], false),
        ),
    ];
    let shaped = serde_json::to_value(&flows[9].1).expect("the flow serializes");
    assert_eq!(
        (
            &shaped["steps"]["steps"][1]["id"],
            &shaped["steps"]["steps"][1]["item"]
        ),
        (&serde_json::json!(2), &serde_json::json!("stop")),
        "a combined step keeps its id in the flow beside its page item's id"
    );
    for (name, flow) in flows {
        let mut json = serde_json::to_string_pretty(&flow).expect("the flow serializes");
        json.push('\n');
        assert_golden(format!("goldens/drawn/{name}.json"), &json);
    }
}

/// A `raise` (Python) or `throw` (TypeScript) on line `line` of `c`, whose value is the call
/// `text` of `callee`.
fn raise(
    c: &mut Code<'_>,
    parent: unfold_core::testkit::NodeRef,
    form: Form,
    line: u32,
    callee: &str,
    text: &str,
    to: LinkTarget,
) {
    let exit = c.exit(parent, form, line, Some(text));
    let call = c.call(exit, callee, line, text, to);
    if form == Form::Throw {
        c.set(call).form = Form::New;
    }
    c.flag(call, Flag::ExitValue);
}

/// A Python `except` part of `attempt` on lines `first` and `first + 1` (a bare `return`),
/// whose clause names `class`, or is bare without one.
fn handler(
    c: &mut Code<'_>,
    attempt: unfold_core::testkit::NodeRef,
    first: u32,
    class: Option<(&str, LinkTarget)>,
) {
    let clause = class
        .as_ref()
        .map_or_else(|| "except".to_owned(), |(name, _)| format!("except {name}"));
    let part = c.part(attempt, Form::Except, first, first + 1, Some(&clause));
    if let Some((name, to)) = class {
        let named = c.last(first, name);
        c.g.link(part, LinkKind::Types, named, name, to);
    }
    c.exit(part, Form::Return, first + 1, None);
}

/// `r.py`: raises in try bodies, and handlers that name a builtin class, a project class
/// deriving from a builtin one, or nothing.
fn raises() -> GraphBuilder {
    const CODE: &str = r#"class AppError(Exception):
    pass


class NotFound(AppError):
    pass


def guard():
    try:
        raise ValueError("bad")
    except KeyError:
        return
    except Exception:
        return


def escape():
    try:
        raise ValueError("bad")
    except KeyError:
        return
    finally:
        cleanup()


def nested():
    try:
        try:
            raise NotFound()
        except KeyError:
            return
        finally:
            cleanup()
    except AppError:
        return


def anything():
    try:
        raise make_error()
    except Exception:
        return
    except:
        return
"#;
    let python = || builtin("python:builtins");
    let to = |name: &str| project(&key(&format!("r.py#{name}")));
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "r.py", Language::Python, CODE);
    let module = c.module;
    for (name, line, base, from) in [
        ("AppError", 1, "Exception", python()),
        ("NotFound", 5, "AppError", to("AppError")),
    ] {
        let head = c.at(line, &format!("class {name}({base})"));
        let class = c.def(
            module,
            Form::Class,
            name,
            c.lines(line, line + 1),
            head,
            c.line(line + 1),
        );
        let named = c.last(line, base);
        c.g.link(class, LinkKind::Extends, named, base, from);
    }

    let guard = c.function(module, "guard", 9, 15, "def guard()");
    let attempt = c.block(guard, Form::Try, c.lines(10, 15), None, None);
    let body = c.suite(attempt, Form::Body, 11, 11);
    raise(
        &mut c,
        body,
        Form::Raise,
        11,
        "ValueError",
        "ValueError(\"bad\")",
        python(),
    );
    handler(&mut c, attempt, 12, Some(("KeyError", python())));
    handler(&mut c, attempt, 14, Some(("Exception", python())));

    let escape = c.function(module, "escape", 18, 24, "def escape()");
    let attempt = c.block(escape, Form::Try, c.lines(19, 24), None, None);
    let body = c.suite(attempt, Form::Body, 20, 20);
    raise(
        &mut c,
        body,
        Form::Raise,
        20,
        "ValueError",
        "ValueError(\"bad\")",
        python(),
    );
    handler(&mut c, attempt, 21, Some(("KeyError", python())));
    let finally = c.part(attempt, Form::Finally, 23, 24, None);
    c.call(finally, "cleanup", 24, "cleanup()", to("cleanup"));

    let nested = c.function(module, "nested", 27, 36, "def nested()");
    let outer = c.block(nested, Form::Try, c.lines(28, 36), None, None);
    let outer_body = c.suite(outer, Form::Body, 29, 34);
    let inner = c.block(outer_body, Form::Try, c.lines(29, 34), None, None);
    let body = c.suite(inner, Form::Body, 30, 30);
    raise(
        &mut c,
        body,
        Form::Raise,
        30,
        "NotFound",
        "NotFound()",
        to("NotFound"),
    );
    handler(&mut c, inner, 31, Some(("KeyError", python())));
    let finally = c.part(inner, Form::Finally, 33, 34, None);
    c.call(finally, "cleanup", 34, "cleanup()", to("cleanup"));
    handler(&mut c, outer, 35, Some(("AppError", to("AppError"))));

    let anything = c.function(module, "anything", 39, 45, "def anything()");
    let attempt = c.block(anything, Form::Try, c.lines(40, 45), None, None);
    let body = c.suite(attempt, Form::Body, 41, 41);
    raise(
        &mut c,
        body,
        Form::Raise,
        41,
        "make_error",
        "make_error()",
        unknown(Reason::NotFound),
    );
    handler(&mut c, attempt, 42, Some(("Exception", python())));
    handler(&mut c, attempt, 44, None);
    g
}

/// A raise goes to the first handler proven to catch it: one naming a class the raised class
/// derives from (through project classes and Python's builtin exceptions), or a bare `except`.
/// Otherwise it goes to the caller through `finally`, and the try's dashed on-error routes say
/// that a handler may catch it first.
#[test]
fn a_raise_goes_to_the_handler_that_catches_it_or_to_the_caller() {
    let graph = raises();
    let guard = drawn(&graph, "r.py#guard");
    assert_routes(
        &guard,
        &[
            "raise ValueError -Raise-> except except Exception",
            "body -OnError-> except except KeyError",
            "body -OnError-> except except Exception",
        ],
    );
    assert!(!routes(&guard).contains(&"raise ValueError -Raise-> except except KeyError".to_owned()));
    assert!(
        plain(&guard).contains("      RAISE ValueError → except Exception\n"),
        "{}",
        plain(&guard)
    );

    let escape = drawn(&graph, "r.py#escape");
    assert_routes(
        &escape,
        &[
            "raise ValueError -Raise-> caller via finally",
            "body -OnError-> except except KeyError",
        ],
    );
    assert!(
        plain(&escape).contains("      RAISE ValueError → finally → the caller\n"),
        "{}",
        plain(&escape)
    );

    let nested = drawn(&graph, "r.py#nested");
    assert_routes(
        &nested,
        &["raise NotFound -Raise-> except except AppError via finally"],
    );
    assert!(
        plain(&nested).contains("RAISE ▸ NotFound → finally → except AppError\n"),
        "{}",
        plain(&nested)
    );

    let anything = drawn(&graph, "r.py#anything");
    assert_routes(&anything, &["raise make_error -Raise-> except except"]);
}

/// `done.ts`: the design's labeled plain block, `done: { if (x) break done; a(); } b();`, and
/// one whose `break` is written in the block itself, which truncates the function.
fn labeled_blocks() -> GraphBuilder {
    const CODE: &str = r#"function run(x) {
  done: {
    if (x) break done;
    a();
  }
  b();
}
function stop() {
  done: {
    a();
    break done;
  }
  b();
}
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "done.ts", Language::TypeScript, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("done.ts#{name}")));
    let run = c.def(
        module,
        Form::Function,
        "run",
        c.lines(1, 7),
        c.at(1, "function run(x)"),
        Code::span(c.at(1, "{"), c.line(7)),
    );
    let decision = c.block(run, Form::If, c.line(3), None, None);
    let then = c.block(
        decision,
        Form::Then,
        c.line(3),
        Some(c.at(3, "(x)")),
        Some(c.at(3, "break done;")),
    );
    c.flag(then, Flag::Truncated);
    c.call(run, "a", 4, "a()", to("a"));
    c.call(run, "b", 6, "b()", to("b"));
    let stop = c.def(
        module,
        Form::Function,
        "stop",
        c.lines(8, 14),
        c.at(8, "function stop()"),
        Code::span(c.at(8, "{"), c.line(14)),
    );
    c.flag(stop, Flag::Truncated);
    c.call(stop, "a", 10, "a()", to("a"));
    c.call(stop, "b", 13, "b()", to("b"));
    g
}

/// A truncated part, or a truncated definition, may continue: a dashed route from it to where
/// its end goes says the path is uncertain there, never that it ends.
#[test]
fn a_truncated_part_or_definition_may_continue() {
    let graph = labeled_blocks();
    let run = drawn(&graph, "done.ts#run");
    assert_routes(&run, &["then (x) -Yes-> a", "then (x) -MayContinue-> a"]);
    let Shape::Decision(frame) = &run.steps.steps[0].shape else {
        panic!("the `if` comes first: {:#?}", run.steps.steps[0]);
    };
    assert_eq!(frame.lanes[0].state, RouteState::Truncated);
    assert!(plain(&run).contains("  IF (x) · truncated\n"), "{}", plain(&run));

    let stop = drawn(&graph, "done.ts#stop");
    assert_routes(&stop, &["start -MayContinue-> End"]);
    assert!(
        plain(&stop).starts_with("( stop ) · truncated\n"),
        "{}",
        plain(&stop)
    );
}

/// `finals.ts`: a `break`, `continue`, `return`, and `throw` whose `finally` always exits
/// itself, and a `throw` a `catch` catches.
fn finals_ts() -> GraphBuilder {
    const CODE: &str = r#"function brk() {
  do {
    try {
      step();
      break;
    } finally {
      return;
    }
  } while (true);
  after();
}
function cont(xs) {
  for (const x of xs) {
    try {
      continue;
    } finally {
      return;
    }
  }
  after();
}
function ret() {
  try {
    return work();
  } finally {
    throw new Error("x");
  }
  after();
}
function thr(xs) {
  for (const x of xs) {
    try {
      throw new Error("x");
    } finally {
      break;
    }
  }
  after();
}
function caught() {
  try {
    throw new Error("x");
  } catch (error) {
    recover();
  }
}
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "finals.ts", Language::TypeScript, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("finals.ts#{name}")));
    let error = || builtin("ecmascript");
    let function = |c: &mut Code<'_>, name: &str, first: u32, last: u32, head: &str| {
        let head = c.at(first, head);
        let body = Code::span(c.at(first, "{"), c.line(last));
        c.def(module, Form::Function, name, c.lines(first, last), head, body)
    };
    // A loop on lines `first` to `last` (`do … while (true)` or `for (const x of xs)`), and its body.
    let looping = |c: &mut Code<'_>, parent, form: Form, first: u32, last: u32| {
        let (written, header) = if form == Form::Do {
            (
                Code::span(c.at(first, "do"), c.line(last)),
                c.at(last, "while (true)"),
            )
        } else {
            (
                Code::span(c.at(first, "for"), c.line(last)),
                c.at(first, "for (const x of xs)"),
            )
        };
        let block = c.block(parent, form, written, Some(header), None);
        let suite = Code::span(c.at(first, "{"), c.at(last, "}"));
        let body = c.block(block, Form::Body, suite, None, Some(suite));
        (block, body)
    };
    // A try on lines `first` to `last` whose last part, from line `part`, is `form`.
    let attempt = |c: &mut Code<'_>, parent, first: u32, part: u32, last: u32, form: Form| {
        let block = c.block(
            parent,
            Form::Try,
            Code::span(c.at(first, "try"), c.at(last, "}")),
            None,
            None,
        );
        let suite = Code::span(c.at(first, "{"), c.at(part, "}"));
        let body = c.block(block, Form::Body, suite, None, Some(suite));
        let keyword = if form == Form::Catch {
            "catch (error)"
        } else {
            "finally"
        };
        let written = Code::span(c.at(part, keyword), c.at(last, "}"));
        let suite = Code::span(c.at(part, "{"), c.at(last, "}"));
        let clause = (form == Form::Catch).then(|| c.at(part, keyword));
        let tail = c.block(block, form, written, clause, Some(suite));
        (body, tail)
    };

    let brk = function(&mut c, "brk", 1, 11, "function brk()");
    let (do_loop, body) = looping(&mut c, brk, Form::Do, 2, 9);
    let (body, finally) = attempt(&mut c, body, 3, 6, 8, Form::Finally);
    c.call(body, "step", 4, "step()", to("step"));
    let leave = c.exit(body, Form::Break, 5, None);
    c.set(leave).target = Some(do_loop.local_id());
    c.exit(finally, Form::Return, 7, None);
    c.call(brk, "after", 10, "after()", to("after"));

    let cont = function(&mut c, "cont", 12, 21, "function cont(xs)");
    let (for_loop, body) = looping(&mut c, cont, Form::For, 13, 19);
    let (body, finally) = attempt(&mut c, body, 14, 16, 18, Form::Finally);
    let next = c.exit(body, Form::Continue, 15, None);
    c.set(next).target = Some(for_loop.local_id());
    c.exit(finally, Form::Return, 17, None);
    c.call(cont, "after", 20, "after()", to("after"));

    let ret = function(&mut c, "ret", 22, 29, "function ret()");
    let (body, finally) = attempt(&mut c, ret, 23, 25, 27, Form::Finally);
    let exit = c.exit(body, Form::Return, 24, Some("work()"));
    let work = c.call(exit, "work", 24, "work()", to("work"));
    c.flag(work, Flag::ExitValue);
    raise(&mut c, finally, Form::Throw, 26, "Error", "Error(\"x\")", error());
    c.call(ret, "after", 28, "after()", to("after"));

    let thr = function(&mut c, "thr", 30, 39, "function thr(xs)");
    let (for_loop, body) = looping(&mut c, thr, Form::For, 31, 37);
    let (body, finally) = attempt(&mut c, body, 32, 34, 36, Form::Finally);
    raise(&mut c, body, Form::Throw, 33, "Error", "Error(\"x\")", error());
    let leave = c.exit(finally, Form::Break, 35, None);
    c.set(leave).target = Some(for_loop.local_id());
    c.call(thr, "after", 38, "after()", to("after"));

    let caught = function(&mut c, "caught", 40, 46, "function caught()");
    let (body, handler) = attempt(&mut c, caught, 41, 43, 45, Form::Catch);
    raise(&mut c, body, Form::Throw, 42, "Error", "Error(\"x\")", error());
    c.call(handler, "recover", 44, "recover()", to("recover"));
    g
}

/// `finals.py`: the same transfers through a `finally` that always exits, and a `break` and
/// `continue` through one that ends.
fn finals_py() -> GraphBuilder {
    const CODE: &str = r#"def brk(xs):
    for x in xs:
        try:
            step()
            break
        finally:
            return


def cont(xs):
    for x in xs:
        try:
            continue
        finally:
            return
    after()


def ret():
    try:
        return work()
    finally:
        raise Stop()
    after()


def thr(xs):
    for x in xs:
        try:
            raise Stop()
        finally:
            break
    after()


def loop(xs):
    for x in xs:
        try:
            if x:
                break
            continue
        finally:
            cleanup()
    after()
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "finals.py", Language::Python, CODE);
    let module = c.module;
    let to = |name: &str| project(&key(&format!("finals.py#{name}")));
    // A `for x in xs` loop on lines `first` to `last`, and its body.
    let looping = |c: &mut Code<'_>, parent, first: u32, last: u32| {
        let header = c.at(first, "for x in xs");
        let block = c.block(parent, Form::For, c.lines(first, last), Some(header), None);
        let body = c.suite(block, Form::Body, first + 1, last);
        (block, body)
    };
    // A try on lines `first` to `last` whose `finally` starts on line `part`.
    let attempt = |c: &mut Code<'_>, parent, first: u32, part: u32, last: u32| {
        let block = c.block(parent, Form::Try, c.lines(first, last), None, None);
        let body = c.suite(block, Form::Body, first + 1, part - 1);
        let finally = c.part(block, Form::Finally, part, last, None);
        (body, finally)
    };

    let brk = c.function(module, "brk", 1, 7, "def brk(xs)");
    let (for_loop, body) = looping(&mut c, brk, 2, 7);
    let (body, finally) = attempt(&mut c, body, 3, 6, 7);
    c.call(body, "step", 4, "step()", to("step"));
    let leave = c.exit(body, Form::Break, 5, None);
    c.set(leave).target = Some(for_loop.local_id());
    c.exit(finally, Form::Return, 7, None);

    let cont = c.function(module, "cont", 10, 16, "def cont(xs)");
    let (for_loop, body) = looping(&mut c, cont, 11, 15);
    let (body, finally) = attempt(&mut c, body, 12, 14, 15);
    let next = c.exit(body, Form::Continue, 13, None);
    c.set(next).target = Some(for_loop.local_id());
    c.exit(finally, Form::Return, 15, None);
    c.call(cont, "after", 16, "after()", to("after"));

    let ret = c.function(module, "ret", 19, 24, "def ret()");
    let (body, finally) = attempt(&mut c, ret, 20, 22, 23);
    let exit = c.exit(body, Form::Return, 21, Some("work()"));
    let work = c.call(exit, "work", 21, "work()", to("work"));
    c.flag(work, Flag::ExitValue);
    raise(&mut c, finally, Form::Raise, 23, "Stop", "Stop()", to("Stop"));
    c.call(ret, "after", 24, "after()", to("after"));

    let thr = c.function(module, "thr", 27, 33, "def thr(xs)");
    let (for_loop, body) = looping(&mut c, thr, 28, 32);
    let (body, finally) = attempt(&mut c, body, 29, 31, 32);
    raise(&mut c, body, Form::Raise, 30, "Stop", "Stop()", to("Stop"));
    let leave = c.exit(finally, Form::Break, 32, None);
    c.set(leave).target = Some(for_loop.local_id());
    c.call(thr, "after", 33, "after()", to("after"));

    let looped = c.function(module, "loop", 36, 44, "def loop(xs)");
    let (for_loop, body) = looping(&mut c, looped, 37, 43);
    let (body, finally) = attempt(&mut c, body, 38, 42, 43);
    let decision = c.block(body, Form::If, c.lines(39, 40), None, None);
    let then = c.part(decision, Form::Then, 39, 40, Some("x"));
    let leave = c.exit(then, Form::Break, 40, None);
    c.set(leave).target = Some(for_loop.local_id());
    let next = c.exit(body, Form::Continue, 41, None);
    c.set(next).target = Some(for_loop.local_id());
    c.call(finally, "cleanup", 43, "cleanup()", to("cleanup"));
    c.call(looped, "after", 44, "after()", to("after"));
    g
}

/// The overlay that combines `members` (keys) into one collapsed step titled `title`.
fn combining(title: &str, members: &[&str]) -> Overlay {
    Overlay {
        combined: vec![Combine {
            id: "combined".to_owned(),
            title: title.to_owned(),
            members: members.iter().map(|member| key(member)).collect(),
            collapsed: true,
        }],
        ..Overlay::default()
    }
}

/// A `finally` that always exits replaces the transfer that entered it: a `break`, `continue`,
/// `return`, or `throw` through it ends in the lane, whose own exit goes on, so code only the
/// replaced transfer reached never runs. A collapsed combined step's transfer ends there too.
#[test]
fn a_finally_that_always_exits_replaces_the_transfer_through_it() {
    let ts = finals_ts();
    let brk = drawn(&ts, "finals.ts#brk");
    assert_routes(&brk, &["break -Break-> finally", "return -Return-> End"]);
    assert!(
        !routes(&brk).iter().any(|route| route.ends_with("-> after")),
        "{:#?}",
        routes(&brk)
    );
    let text = plain(&brk);
    assert!(text.contains("          BREAK → finally\n"), "{text}");
    assert!(text.contains("  ▸ after · never runs\n"), "{text}");
    let members = [
        "finals.ts#brk/do[0]/body/try[0]/body/call:step",
        "finals.ts#brk/do[0]/body/try[0]/body/break[0]",
    ];
    let combined = drawn_with(&ts, "finals.ts#brk", &combining("Leave", &members), &[], false);
    assert_routes(&combined, &["Leave -Break-> finally"]);
    assert!(
        plain(&combined).contains("⊞ Leave   step · breaks → finally\n"),
        "{}",
        plain(&combined)
    );

    let cont = drawn(&ts, "finals.ts#cont");
    assert_routes(&cont, &["continue -Continue-> finally", "for -Done-> after"]);
    assert!(!routes(&cont).contains(&"continue -Continue-> for".to_owned()));
    assert!(
        plain(&cont).contains("          CONTINUE → finally\n"),
        "{}",
        plain(&cont)
    );

    let ret = drawn(&ts, "finals.ts#ret");
    assert_routes(
        &ret,
        &["return work -Return-> finally", "throw Error -Raise-> caller"],
    );
    assert!(
        plain(&ret).contains("      RETURN ▸ work → finally\n"),
        "{}",
        plain(&ret)
    );

    let thr = drawn(&ts, "finals.ts#thr");
    assert_routes(&thr, &["throw Error -Raise-> finally", "break -Break-> after"]);
    assert!(
        plain(&thr).contains("          THROW Error → finally\n"),
        "{}",
        plain(&thr)
    );

    let caught = drawn(&ts, "finals.ts#caught");
    assert_routes(&caught, &["throw Error -Raise-> catch catch (error)"]);

    let py = finals_py();
    let brk = drawn(&py, "finals.py#brk");
    assert_routes(&brk, &["break -Break-> finally"]);
    assert!(
        plain(&brk).contains("          BREAK → finally\n"),
        "{}",
        plain(&brk)
    );
    let members = [
        "finals.py#brk/for[0]/body/try[0]/body/call:step",
        "finals.py#brk/for[0]/body/try[0]/body/break[0]",
    ];
    let combined = drawn_with(&py, "finals.py#brk", &combining("Leave", &members), &[], false);
    assert_routes(&combined, &["Leave -Break-> finally"]);
    assert!(
        plain(&combined).contains("⊞ Leave   step · breaks → finally\n"),
        "{}",
        plain(&combined)
    );
    let cont = drawn(&py, "finals.py#cont");
    assert_routes(&cont, &["continue -Continue-> finally"]);
    let ret = drawn(&py, "finals.py#ret");
    assert_routes(&ret, &["return work -Return-> finally"]);
    assert!(
        plain(&ret).contains("  ▸ after · never runs\n"),
        "{}",
        plain(&ret)
    );
    let thr = drawn(&py, "finals.py#thr");
    assert_routes(&thr, &["raise Stop -Raise-> finally", "break -Break-> after"]);
}

/// A `break` or `continue` through a `finally` that ends runs the lane and goes on to its own
/// target, and so does one leaving a collapsed combined step.
#[test]
fn a_break_or_continue_goes_through_finally_to_its_loop() {
    let py = finals_py();
    let flow = drawn(&py, "finals.py#loop");
    assert_routes(
        &flow,
        &[
            "break -Break-> after via finally",
            "continue -Continue-> for via finally",
        ],
    );
    let text = plain(&flow);
    assert!(
        text.contains("              BREAK → finally → after the loop\n"),
        "{text}"
    );
    assert!(text.contains("          CONTINUE → finally → next\n"), "{text}");

    let members = [
        "finals.py#loop/for[0]/body/try[0]/body/if[0]",
        "finals.py#loop/for[0]/body/try[0]/body/continue[0]",
    ];
    let combined = drawn_with(&py, "finals.py#loop", &combining("Skip", &members), &[], false);
    assert_routes(
        &combined,
        &[
            "Skip -Break-> after via finally",
            "Skip -Continue-> for via finally",
        ],
    );
    assert!(
        plain(&combined).contains(
            "⊞ Skip   1 condition · breaks → finally → after the loop · continues → finally → next\n"
        ),
        "{}",
        plain(&combined)
    );
}
