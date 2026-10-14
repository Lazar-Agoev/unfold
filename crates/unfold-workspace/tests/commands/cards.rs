//! Cards and what the Selected section shows, each naming its project: `card` (a definition's
//! drawn flow with the page's overlay, open paths, and Show all calls; an app's or entrance's
//! card), the agent text (`code_text`, `flow_text`), `relations`, and `callers`.

use serde_json::{Value, json};

use crate::common::Harness;

const CREATE_ORDER: &str = "src/routes/orders.ts#createOrder";
const PLACE: &str = "src/routes/orders.ts#createOrder/call:orderService.place";
const QUEUE_IT: &str = "src/routes/orders.ts#createOrder/if[0]";

/// A TypeScript file of `functions` small functions that call each other.
fn generated(functions: usize) -> String {
    let mut text = String::new();
    for index in 0..functions {
        let next = (index + 1) % functions;
        text.push_str(&format!(
            "export function step{index}(value: number): number {{\n  if (value > {index}) {{\n    return step{next}(value - 1);\n  }}\n  // A comment with é and 😀.\n  return value + {index};\n}}\n\n"
        ));
    }
    text
}

/// The page's overlay the card tests draw with: a name, a combined step, and a proposal.
fn overlay() -> Value {
    json!({
        "names": [{ "key": PLACE, "text": "Place the order" }],
        "combined": [{ "id": "queue", "title": "Queue it when asked", "members": [QUEUE_IT], "collapsed": true }],
        "proposed": [{ "id": "audit", "after": PLACE, "title": "Write an audit record" }],
    })
}

/// Every object in `value`, depth first.
fn objects(value: &Value) -> Vec<&Value> {
    let mut found = Vec::new();
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                found.push(value);
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    found
}

#[test]
fn a_card_draws_its_flow_with_the_overlay_and_open_paths() {
    let harness = Harness::compiled("ts-orders");
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": CREATE_ORDER, "overlay": overlay(), "open": [[PLACE]] }),
    );
    assert_eq!(card["type"], "definition");
    let flow = &card["flow"];
    assert_eq!(flow["header"]["key"], CREATE_ORDER);
    assert_eq!(flow["header"]["label"], "createOrder");
    assert_eq!(flow["header"]["path"], "src/routes/orders.ts");
    let found = objects(flow);
    let place = found
        .iter()
        .find(|item| item["key"] == PLACE && item["callee"].is_string())
        .expect("the call box");
    assert_eq!(place["name"], "Place the order");
    assert_eq!(place["actions"]["open"], true);
    assert_eq!(
        place["inset"]["header"]["key"], "src/services/order-service.ts#OrderService.place",
        "the open call's callee is drawn inside it"
    );
    let combined = found
        .iter()
        .find(|item| item["kind"] == "combined")
        .expect("the combined step");
    assert_eq!(combined["title"], "Queue it when asked");
    assert_eq!(combined["members"], json!([QUEUE_IT]));
    assert!(
        found
            .iter()
            .any(|item| item["kind"] == "proposed" && item["title"] == "Write an audit record"),
        "{flow}"
    );

    // Without the overlay and open paths, the same card is drawn plain.
    let plain = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": CREATE_ORDER }),
    );
    let found = objects(&plain["flow"]);
    let place = found
        .iter()
        .find(|item| item["key"] == PLACE && item["callee"].is_string())
        .expect("the call box");
    assert!(
        place.get("inset").is_none() && place.get("name").is_none(),
        "{place}"
    );
    assert!(found.iter().all(|item| item["kind"] != "combined"));
    // Show all calls draws builtins too; nothing else changes shape.
    let all = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": CREATE_ORDER, "showAllCalls": true }),
    );
    assert_eq!(all["flow"]["header"], plain["flow"]["header"]);

    // What a card refuses.
    let card = |extra: Value| {
        let mut args = json!({ "project": harness.project, "key": CREATE_ORDER });
        args.as_object_mut()
            .expect("args")
            .extend(extra.as_object().expect("extra").clone());
        harness.call(1, "card", args).map_err(|error| error.code)
    };
    let deep = json!([[PLACE, PLACE, PLACE, PLACE]]);
    let many: Vec<Value> = (0..65).map(|_| json!([PLACE])).collect();
    for bad in [
        json!({ "open": deep }),
        json!({ "open": many }),
        json!({ "open": [[]] }),
        json!({ "overlay": { "proposed": [{ "id": "p", "title": "x" }] } }),
        json!({ "overlay": { "proposed": [{ "id": "p", "after": PLACE, "start": CREATE_ORDER, "title": "x" }] } }),
        json!({ "overlay": { "combined": [{ "id": "c", "title": "x", "members": [], "collapsed": true }] } }),
        json!({ "overlay": { "names": [{ "key": "", "text": "x" }] } }),
        json!({ "overlay": { "labels": [] } }),
        json!({ "lens": "code" }),
    ] {
        assert_eq!(card(bad.clone()), Err("invalid_args".to_owned()), "{bad}");
    }
    assert_eq!(
        harness.code(1, "card", json!({ "project": harness.project, "key": PLACE })),
        "not_found",
        "a call isn't a card"
    );
}

#[test]
fn an_entrance_card_lists_the_code_it_starts_at() {
    let harness = Harness::compiled("ts-orders");
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": "entrance:server/place-an-order" }),
    );
    assert_eq!(card["type"], "start");
    assert_eq!(card["kind"], "entrance");
    assert_eq!(card["app"], "Server");
    assert_eq!(card["name"], "Place an order");
    assert_eq!(card["on"], "POST /orders");
    let code = card["code"].as_array().expect("code");
    assert_eq!(code.len(), 3);
    assert_eq!(
        code[2],
        json!({
            "key": CREATE_ORDER, "state": "found", "label": "createOrder",
            "path": "src/routes/orders.ts", "line": code[2]["line"],
        })
    );
    let app = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": "entrance:server" }),
    );
    assert_eq!(app["kind"], "app");
    assert_eq!(app["on"], "npm run start");
    let admin = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": "entrance:server/admin-stats" }),
    );
    assert_eq!(
        admin["code"],
        json!([{ "key": "src/server.ts#adminStats", "state": "missing" }])
    );
    assert_eq!(
        harness.code(
            1,
            "card",
            json!({ "project": harness.project, "key": "entrance:nope" })
        ),
        "not_found"
    );
}

#[test]
fn code_text_prints_the_compiled_tree_with_its_keys() {
    let harness = Harness::compiled("ts-orders");
    let text = harness.ok(
        1,
        "code_text",
        json!({ "project": harness.project, "key": CREATE_ORDER }),
    );
    let printed = text["text"].as_str().expect("text");
    assert!(printed.starts_with(&format!("{CREATE_ORDER}\n")), "{printed}");
    assert!(printed.contains(&format!("key={PLACE}")), "{printed}");
    let labels = text["labels"].as_array().expect("labels");
    assert!(
        labels.contains(&json!({ "label": "if[0]", "key": QUEUE_IT })),
        "{labels:?}"
    );
    assert!(labels.contains(&json!({ "label": "call:orderService.place", "key": PLACE })));
    assert_eq!(
        harness.code(
            1,
            "code_text",
            json!({ "project": harness.project, "key": PLACE })
        ),
        "not_found",
        "a call has no compiled tree of its own"
    );
}

#[test]
fn flow_text_prints_the_shaped_flow_with_its_labels() {
    let harness = Harness::compiled("ts-orders");
    let text = harness.ok(
        1,
        "flow_text",
        json!({ "project": harness.project, "key": CREATE_ORDER, "overlay": overlay(), "open": [[PLACE]] }),
    );
    let printed = text["text"].as_str().expect("text");
    for expected in [
        "Place the order",
        "Queue it when asked",
        "Write an audit record",
        "key=",
    ] {
        assert!(printed.contains(expected), "{expected} in {printed}");
    }
    assert!(
        printed.contains("src/services/order-service.ts#OrderService.place"),
        "the inset is printed: {printed}"
    );
    let labels = text["labels"].as_array().expect("labels");
    assert!(labels.contains(&json!({ "label": "if[0]", "key": QUEUE_IT })));
    assert_eq!(
        harness.code(
            1,
            "flow_text",
            json!({ "project": harness.project, "key": "src/nope.ts#x" })
        ),
        "not_found"
    );
}

#[test]
fn relations_and_callers_name_their_project() {
    let harness = Harness::compiled("ts-orders");
    harness.write("src/generated.ts", &generated(6));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let key = "src/generated.ts#step1";
    let relations = harness.ok(1, "relations", json!({ "project": harness.project, "key": key }));
    assert_eq!(relations["key"], key);
    assert_eq!(relations["callersTotal"], 1);
    assert_eq!(relations["callersNext"], Value::Null);
    let caller = &relations["callers"][0];
    assert_eq!(caller["owner"], "src/generated.ts#step0");
    assert_eq!(caller["ownerName"], "step0");
    assert_eq!(caller["kind"], "calls");
    let outgoing = &relations["outgoing"][0];
    assert_eq!(outgoing["to"]["key"], "src/generated.ts#step2");
    assert_eq!(outgoing["targetName"], "step2");
    // Any block stands for the definition it belongs to.
    let inner = harness.ok(
        1,
        "relations",
        json!({ "project": harness.project, "key": "src/generated.ts#step1/if[0]" }),
    );
    assert_eq!(inner["key"], key);

    // Callers page through an offset.
    let first = harness.ok(
        1,
        "relations",
        json!({ "project": harness.project, "key": "src/routes/middleware.ts#requireUser", "callersLimit": 1 }),
    );
    let reached = &first["reachedFrom"];
    assert!(
        reached
            .as_array()
            .expect("reached from")
            .iter()
            .any(
                |entrance| entrance["key"] == "entrance:server/place-an-order" && entrance["app"] == "Server"
            ),
        "{reached}"
    );
    if first["callersTotal"].as_u64() > Some(1) {
        let next = harness.ok(
            1,
            "relations",
            json!({ "project": harness.project, "key": "src/routes/middleware.ts#requireUser", "callersOffset": 1, "callersLimit": 1 }),
        );
        assert_ne!(next["callers"][0], first["callers"][0]);
    }

    let callers = harness.ok(1, "callers", json!({ "project": harness.project, "key": key }));
    assert_eq!(callers["project"], harness.project);
    assert_eq!(callers["total"], 1);
    assert_eq!(
        callers["callers"][0],
        json!({
            "key": "src/generated.ts#step0", "name": "step0", "qualifiedName": "step0",
            "path": "src/generated.ts", "line": 1, "sites": 1,
        })
    );
    assert_eq!(
        harness.code(
            1,
            "callers",
            json!({ "project": harness.project, "key": "src/generated.ts#nope" })
        ),
        "not_found"
    );
    assert_eq!(
        harness.code(
            1,
            "relations",
            json!({ "project": harness.project, "key": "src/generated.ts#nope" })
        ),
        "not_found"
    );
}
