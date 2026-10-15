//! Contracts: `call_contract` (a call, by its block key), `type_definition`, and `type_uses` on a
//! copy of `testdata/fixtures/typed-lines`, with their limits and error codes. The reader's own
//! tests cover every rule; these cover the commands.

use serde_json::{Value, json};

use crate::common::{Harness, options};

const CHARGE: &str = "ts/src/orders/create.ts#createOrder/try[0]/body/call:chargePayment";
const RESERVE: &str = "ts/src/orders/create.ts#createOrder/call:reserveInventory";
const THROUGH_INSTANCE: &str = "py/src/shop/orders.py#create_order/try[0]/body/call:gateway.charge";
const THROUGH_CLASS: &str = "py/src/shop/orders.py#create_order/call:Gateway.charge";
const CART: &str = "ts/src/cart.ts#Cart";

fn contract(harness: &Harness, key: &str) -> Value {
    harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "key": key }),
    )
}

/// The keys of the project definitions each span of a type text reaches.
fn targets(text: &Value) -> Vec<Value> {
    text["spans"]
        .as_array()
        .expect("spans")
        .iter()
        .map(|span| span["to"]["key"].clone())
        .collect()
}

#[test]
fn a_contract_names_what_is_sent_returned_and_raised() {
    let harness = Harness::compiled_fixture("typed-lines");
    let charge = contract(&harness, CHARGE);
    assert_eq!(charge["call"], CHARGE);
    assert_eq!(charge["line"], 27);
    assert_eq!(charge["text"], "chargePayment(cart, req.user)");
    assert_eq!(charge["callee"]["key"], "ts/src/payments.ts#chargePayment");
    let sends = charge["sends"].as_array().expect("sends");
    assert_eq!(sends[0]["argument"]["text"], "cart");
    assert_eq!(sends[0]["parameter"]["name"], "cart");
    assert_eq!(sends[0]["parameter"]["type"]["text"], "Cart");
    assert_eq!(targets(&sends[0]["parameter"]["type"]), [json!(CART)]);
    assert_eq!(sends[1]["argument"]["text"], "req.user");
    assert_eq!(sends[1]["parameter"]["name"], "user");
    let returns = &charge["returns"];
    assert_eq!(returns["awaited"], true);
    assert_eq!(returns["lands"], json!({ "kind": "binds", "target": "charge" }));
    assert_eq!(returns["type"]["text"], "Promise<Charge>");
    let raises = charge["raises"].as_array().expect("raises");
    assert_eq!(raises[0]["type"]["text"], "CardDeclinedError");
    assert_eq!(
        raises[0]["caughtBy"], "ts/src/orders/create.ts#createOrder/try[0]/catch",
        "the call sits in a try with a catch"
    );

    // `reserveInventory` raises outside any try, and its call isn't caught.
    let reserve = contract(&harness, RESERVE);
    assert_eq!(reserve["raises"][0]["type"]["text"], "OutOfStockError");
    assert!(reserve["raises"][0].get("caughtBy").is_none(), "{reserve}");
    assert_eq!(
        reserve["returns"]["lands"],
        json!({ "kind": "binds", "target": "hold" })
    );
}

#[test]
fn python_contracts_keep_keywords_and_leave_out_the_bound_receiver() {
    let harness = Harness::compiled_fixture("typed-lines");
    let through_instance = contract(&harness, THROUGH_INSTANCE);
    let sends = through_instance["sends"].as_array().expect("sends");
    let filled: Vec<&Value> = sends.iter().map(|send| &send["parameter"]["name"]).collect();
    assert_eq!(filled, ["cart", "customer", "currency", "idempotency"]);
    assert_eq!(sends[2]["argument"]["keyword"], "currency");
    assert!(sends[3].get("argument").is_none(), "nothing fills idempotency");
    assert_eq!(sends[3]["parameter"]["kind"], "keyword_only");
    assert_eq!(sends[3]["parameter"]["default"], true);
    assert_eq!(through_instance["raises"][0]["type"]["text"], "CardDeclined");
    assert_eq!(
        through_instance["raises"][0]["caughtBy"],
        "py/src/shop/orders.py#create_order/try[0]/except[0]"
    );
    // Through the class, the caller fills `self` itself.
    let through_class = contract(&harness, THROUGH_CLASS);
    assert_eq!(through_class["sends"][0]["parameter"]["name"], "self");
    assert!(through_class["raises"][0].get("caughtBy").is_none());
}

#[test]
fn a_type_opens_with_its_members_and_its_uses_page() {
    let harness = Harness::compiled_fixture("typed-lines");
    let cart = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": CART }),
    );
    assert_eq!(cart["kind"], "interface");
    assert_eq!(cart["name"], "Cart");
    assert_eq!(cart["path"], "ts/src/cart.ts");
    assert_eq!(cart["line"], 12);
    assert_eq!(cart["language"], "typescript");
    let members: Vec<(&Value, &Value)> = cart["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|member| (&member["name"], &member["type"]["text"]))
        .collect();
    assert_eq!(
        members,
        [
            (&json!("id"), &json!("CartId")),
            (&json!("owner"), &json!("User")),
            (&json!("items"), &json!("CartItem[]")),
            (&json!("coupon"), &json!("string")),
            (&json!("createdAt"), &json!("Date")),
        ]
    );
    assert_eq!(cart["members"][3]["optional"], true);
    assert_eq!(cart["members"][4]["readonly"], true);
    assert_eq!(
        targets(&cart["members"][0]["type"]),
        [json!("ts/src/cart.ts#CartId")]
    );
    let uses = cart["uses"].as_u64().expect("uses");
    assert!(uses >= 2, "{cart}");

    // Every place, then a page of two after the first.
    let all = harness.ok(1, "type_uses", json!({ "project": harness.project, "key": CART }));
    assert_eq!(all["total"], uses);
    let places = all["places"].as_array().expect("places");
    assert_eq!(places.len() as u64, uses);
    assert!(
        places.iter().any(
            |place| place["owner"] == "ts/src/payments.ts#chargePayment" && place["kind"] == "annotation"
        ),
        "{all}"
    );
    let page = harness.ok(
        1,
        "type_uses",
        json!({ "project": harness.project, "key": CART, "offset": 1, "limit": 2 }),
    );
    assert_eq!(page["total"], uses);
    assert_eq!(page["places"], json!(places[1..3]));

    // An alias that isn't an object type shows what it aliases; an enum its values; a Python
    // dataclass is a class with fields.
    let discount = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "ts/src/cart.ts#Discount" }),
    );
    assert_eq!(discount["kind"], "type_alias");
    assert_eq!(discount["aliased"]["text"], "Partial<Cart> | null");
    let currency = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "ts/src/cart.ts#Currency" }),
    );
    assert_eq!(currency["kind"], "enum");
    assert_eq!(currency["members"][0]["value"], "\"usd\"");
    let customer = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "py/src/shop/models.py#Cart" }),
    );
    assert_eq!(customer["kind"], "class");
    assert_eq!(customer["language"], "python");
    assert_eq!(customer["members"][2]["name"], "coupon");
    assert_eq!(customer["members"][2]["default"], true);

    // Bases: depth first in written order, each type once.
    let order = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "ts/src/orders/model.ts#Order" }),
    );
    let bases: Vec<&Value> = order["inherited"]
        .as_array()
        .expect("inherited")
        .iter()
        .map(|group| &group["from"]["text"])
        .collect();
    assert_eq!(bases, ["OrderDraft", "Timestamped", "Tracked"]);
    let implemented = harness.ok(
        1,
        "type_uses",
        json!({ "project": harness.project, "key": "ts/src/orders/model.ts#Auditable" }),
    );
    assert_eq!(implemented["places"][0]["kind"], "implements");
}

#[test]
fn contract_commands_check_their_limits_and_keys() {
    let harness = Harness::compiled_fixture("typed-lines");
    let project = harness.project.clone();
    for key in [
        "ts/src/nowhere.ts#f/call:g",
        "ts/src/orders/create.ts#createOrder",
    ] {
        assert_eq!(
            harness.code(1, "call_contract", json!({ "project": project, "key": key })),
            "not_found",
            "{key}: only a call has a contract"
        );
    }
    assert_eq!(
        harness.code(1, "call_contract", json!({ "project": project, "key": "" })),
        "invalid_args"
    );
    assert_eq!(
        harness.code(
            1,
            "call_contract",
            json!({ "project": project, "source": CHARGE, "seq": 0 })
        ),
        "invalid_args",
        "a call is named by its block key alone"
    );

    // Definitions are of classes and types only.
    let function = "ts/src/orders/create.ts#createOrder";
    assert_eq!(
        harness.code(
            1,
            "type_definition",
            json!({ "project": project, "key": function })
        ),
        "not_found"
    );
    assert_eq!(
        harness.code(1, "type_uses", json!({ "project": project, "key": function })),
        "not_found"
    );
    assert_eq!(
        harness.code(
            1,
            "type_uses",
            json!({ "project": project, "key": CART, "limit": 201 })
        ),
        "invalid_args"
    );
    let empty = harness.ok(
        1,
        "type_uses",
        json!({ "project": project, "key": CART, "offset": 1000, "limit": 200 }),
    );
    assert_eq!(empty["places"], json!([]));
    assert_eq!(
        harness.code(
            1,
            "type_definition",
            json!({ "project": "p-aaaaaaaaaa", "key": CART })
        ),
        "unknown_project"
    );
}

#[test]
fn contract_commands_need_a_graph() {
    let harness = Harness::empty(options(false));
    let (project, _) = harness.add_copy(&crate::common::fixture("typed-lines"), "typed-lines");
    assert_eq!(
        harness.code(1, "type_definition", json!({ "project": project, "key": CART })),
        "not_compiled"
    );
}
