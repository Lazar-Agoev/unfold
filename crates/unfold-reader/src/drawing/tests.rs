//! The drawing function and agent text: the route rules, the presentation rules, the nine
//! examples of diagram-blocks-examples.html, `place_order` drawn and shaped, and the
//! playgrounds' drawn-flow goldens.

use rustc_hash::FxHashMap;
use unfold_core::testkit::{GraphBuilder, assert_golden, project, sample, unknown};
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
                    let lane_name = match test.or(lane.clause.as_deref()) {
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
      · logger.warning ?
      RETURN ▸ mark_failed → End
  ⋯ 1 line
  ▸ save_order
  LOOP for item in order["items"]
      ▸ reserve_stock
  RETURN order → End
( End )
"#;

/// Example 01 and diagram-blocks' "Drawn": every step connected, builtins hidden, the unknown
/// `logger.warning` in a thin row, and the two gap ticks for `total = sum(…)` and
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

/// Example 08: decorators and defaults are calls of the enclosing body, folded into one thin
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

/// The overlay of mcp-capabilities' `shape_card` example: a title, a named handler, and three
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

/// mcp-capabilities' `shape_card` result, line for line (keys aside).
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
      ⊞ Mark the order failed   mark_failed · 1 unknown ? · returns → End
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
    assert!(plain(&named).contains("      □ Log the decline · logger.warning ?\n"));
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
    assert!(text.contains("  ON ERROR → except PaymentDeclined as error\n      + PROPOSED Retry once\n      · logger.warning ?\n"));
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

/// A TypeScript `break` that leaves a `switch` early goes past the switch.
#[test]
fn a_break_may_leave_a_switch() {
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
            "L21  try[0]/except[0]/call:logger.warning         ? unknown_receiver",
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
fn playground_flows(name: &str) -> String {
    let playground = Playground::load(name);
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
        assert_golden(format!("goldens/{name}.flows.txt"), &playground_flows(name));
    }
}
