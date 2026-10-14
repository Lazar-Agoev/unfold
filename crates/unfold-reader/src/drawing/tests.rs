//! The drawing function and agent text: the route rules, the presentation rules, the nine
//! worked examples, `place_order` drawn and shaped, and the
//! playgrounds' drawn-flow goldens.

use rustc_hash::FxHashMap;
use unfold_core::testkit::{GraphBuilder, assert_golden, builtin, project, sample, unknown};
use unfold_core::vocab::{Flag, Form, Language, Outcome, Reason};
use unfold_core::{Key, LineSpan};

use super::examples::{self, Code, ORDERS_HANDLER};
use super::*;
use crate::testing::Playground;

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
    draw(&trees, &playground.root, overlay, &card).expect("the definition draws")
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
    let flow = draw(&trees, &ts.root, &Overlay::default(), &card).expect("the definition draws");
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
        unknown(Reason::NotFound),
    );
    c.flag(error, Flag::ExitValue);
    let first = c.part(try_block, Form::Except, 12, 13, Some("except ValueError"));
    c.exit(first, Form::Return, 13, Some("None"));
    let second = c.part(try_block, Form::Except, 14, 15, Some("except KeyError"));
    c.exit(second, Form::Return, 15, Some("None"));
    g
}

/// Code after a decision whose every lane exits never runs; a raise in a try body goes to its
/// handlers.
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
    assert!(plain(&guard).contains("      RAISE ValueError ? → the handlers\n"));
    assert_routes(
        &guard,
        &[
            "raise ValueError -Raise-> except except ValueError",
            "raise ValueError -Raise-> except except KeyError",
        ],
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
    let flow = draw(&trees, &playground.root, &Overlay::default(), &card).expect("main draws");
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
    let flow = draw(&trees, &playground.root, &Overlay::default(), &card).expect("records draws");
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
    let code = code_text(&tree, &playground.root, id);
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
            "L11  call:load_order                              ▸ shop/store.py#load_order · binds order",
            "L12  if[0]                                        if",
            "L12  if[0]/then                                   test order is None",
            "L13  if[0]/then/raise[0]                          raise · value ValueError(\"unknown order\")",
            "L13  if[0]/then/raise[0]/call:ValueError          builtin · exit value",
            "L14  if[0]/elif[0]                                test order[\"paid\"]",
            "L15  if[0]/elif[0]/return[0]                      return · value order",
            "L17  call:sum                                     builtin · binds total",
            "L18  try[0]                                       try",
            "L19  try[0]/body                                  body",
            "L19  try[0]/body/call:charge_card                 ▸ shop/payments.py#charge_card · binds receipt",
            "L20  try[0]/except[0]                             clause except PaymentDeclined as error · types ▸ shop/payments.py#PaymentDeclined",
            "L21  try[0]/except[0]/call:logger.warning         library python:logging",
            "L22  try[0]/except[0]/return[0]                   return · value mark_failed(order)",
            "L22  try[0]/except[0]/return[0]/call:mark_failed  ▸ shop/store.py#mark_failed · exit value",
            "L24  ⋯ order[\"receipt\"] = receipt                 code",
            "L25  call:save_order                              ▸ shop/store.py#save_order",
            "L26  for[0]                                       for · for item in order[\"items\"]",
            "L27  for[0]/body                                  body",
            "L27  for[0]/body/call:reserve_stock               ▸ shop/store.py#reserve_stock",
            "L28  return[0]                                    return · value order",
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

    let module = Playground::built(&examples::orders_route());
    let (tree, id) = module.block("src/routes/orders.ts#");
    let code = code_text(&tree, &module.root, id);
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
            let flow = draw(&trees, &playground.root, &overlay, &card).expect("a definition draws");
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
