//! Typed lines (docs/ipc.md › Typed lines): `call_contract`, `type_definition`, and
//! `type_uses` on a copy of `testdata/fixtures/typed-lines`, with their limits and error codes.

use serde_json::{Value, json};

use crate::common::{Harness, options};

const CREATE: &str = "ts/src/orders/create.ts#function:createOrder";
const DESCRIBE: &str = "ts/src/orders/create.ts#function:describe";
const CREATE_ORDER_PY: &str = "py/src/shop/orders.py#function:create_order";
const CART: &str = "ts/src/cart.ts#interface:Cart";

/// The occurrence key of the Code link of `definition` whose written callee is `text`.
fn link(harness: &Harness, definition: &str, text: &str) -> String {
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": definition, "lens": "code" }),
    );
    let links = card["code"]["source"]["links"].as_array().expect("links");
    links
        .iter()
        .find(|link| link["text"] == text)
        .and_then(|link| link["source"].as_str())
        .unwrap_or_else(|| panic!("no link `{text}` in {definition}: {links:?}"))
        .to_owned()
}

/// A span over `[start, end)` of `style`, with `target` when declared and `module` otherwise.
fn span(start: u32, end: u32, style: &str, detail: &str) -> Value {
    match style {
        "declared" => json!({ "start": start, "end": end, "style": style, "target": detail }),
        _ => json!({ "start": start, "end": end, "style": style, "module": detail }),
    }
}

/// A type text.
fn text(text: &str, spans: Vec<Value>) -> Value {
    json!({ "text": text, "spans": spans })
}

#[test]
fn a_contract_names_what_is_sent_returned_and_thrown() {
    let harness = Harness::compiled_fixture("typed-lines");
    let charge = link(&harness, CREATE, "chargePayment");
    let contract = harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "source": charge, "seq": 0 }),
    );
    assert_eq!(contract["target"], "ts/src/payments.ts#function:chargePayment");
    assert_eq!(contract["callerLine"], 27);
    assert_eq!(contract["caught"], true, "the call sits in a try with a catch");
    assert_eq!(
        contract["sends"],
        json!([
            { "argument": { "index": 0, "text": "cart" },
              "parameter": { "index": 0, "name": "cart", "kind": "positional", "optional": false, "default": false,
                             "type": text("Cart", vec![span(0, 4, "declared", CART)]) } },
            { "argument": { "index": 1, "text": "req.user" },
              "parameter": { "index": 1, "name": "user", "kind": "positional", "optional": false, "default": false,
                             "type": text("User", vec![span(0, 4, "declared", "ts/src/user.ts#interface:User")]) } },
        ])
    );
    assert_eq!(
        contract["returns"],
        json!({
            "binding": "variable", "text": "charge", "awaited": true,
            "type": text("Promise<Charge>", vec![
                span(0, 7, "builtin", "global"),
                span(8, 14, "declared", "ts/src/payments.ts#interface:Charge"),
            ]),
        })
    );
    assert_eq!(
        contract["throws"],
        json!([{
            "type": text("CardDeclinedError", vec![span(0, 17, "declared", "ts/src/errors.ts#class:CardDeclinedError")]),
            "line": 17,
        }])
    );
    assert_eq!(contract["calleeLines"], json!([14, 17]));

    // `reserveInventory` throws outside any try, and its call isn't caught.
    let reserve = link(&harness, CREATE, "reserveInventory");
    let contract = harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "source": reserve, "seq": 0 }),
    );
    assert_eq!(contract["caught"], false);
    assert_eq!(contract["throws"][0]["type"]["text"], "OutOfStockError");
    assert_eq!(contract["throws"][0]["line"], 12);
    assert_eq!(contract["returns"]["binding"], "variable");
    assert_eq!(contract["returns"]["text"], "hold");
}

#[test]
fn python_contracts_keep_keywords_and_leave_out_the_bound_receiver() {
    let harness = Harness::compiled_fixture("typed-lines");
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": CREATE_ORDER_PY, "lens": "code" }),
    );
    let links = card["code"]["source"]["links"].as_array().expect("links").clone();
    let by_line = |line: u64| -> String {
        links
            .iter()
            .find(|link| link["line"] == line && link["kind"] != "imports")
            .and_then(|link| link["source"].as_str())
            .unwrap_or_else(|| panic!("no link on line {line}: {links:?}"))
            .to_owned()
    };
    let (through_instance, through_class) = (by_line(17), by_line(20));
    let contract = harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "source": through_instance, "seq": 0 }),
    );
    assert_eq!(contract["caught"], true);
    let sends = contract["sends"].as_array().expect("sends");
    let filled: Vec<&Value> = sends.iter().map(|send| &send["parameter"]["name"]).collect();
    assert_eq!(
        filled,
        [
            &json!("cart"),
            &json!("customer"),
            &json!("currency"),
            &json!("idempotency")
        ]
    );
    assert_eq!(sends[2]["argument"]["keyword"], "currency");
    assert_eq!(sends[3]["argument"], Value::Null, "nothing fills idempotency");
    assert_eq!(sends[3]["parameter"]["kind"], "keyword_only");
    assert_eq!(sends[3]["parameter"]["default"], true);
    // `raise CardDeclined(…)`: the binder proves the construction (its bases end in the builtin
    // `Exception`), so it is a direct throw of `charge`.
    assert_eq!(
        contract["throws"],
        json!([{ "type": text("CardDeclined", vec![span(0, 12, "declared", "py/src/shop/errors.py#class:CardDeclined")]), "line": 22 }])
    );
    // Through the class, the gateway fills `self` itself.
    let contract = harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "source": through_class, "seq": 0 }),
    );
    assert_eq!(contract["sends"][0]["parameter"]["name"], "self");
    assert_eq!(contract["caught"], false);
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
        cart["members"][0]["type"]["spans"],
        json!([span(0, 6, "declared", "ts/src/cart.ts#type_alias:CartId")])
    );
    assert_eq!(cart["uses"], 6, "Used in 6 places");

    // Every place, then a page of two.
    let all = harness.ok(1, "type_uses", json!({ "project": harness.project, "key": CART }));
    let place = |owner: &str, label: &str, path: &str, line: u32| json!({ "owner": format!("{path}#{owner}"), "label": label, "path": path, "line": line });
    assert_eq!(
        all,
        json!({ "total": 6, "places": [
            place("type_alias:Discount", "Discount", "ts/src/cart.ts", 31),
            place("interface:Order", "Order", "ts/src/orders/create.ts", 11),
            place("interface:Request", "Request", "ts/src/orders/create.ts", 20),
            place("function:toOrder", "toOrder", "ts/src/orders/create.ts", 34),
            place("function:chargePayment", "chargePayment", "ts/src/payments.ts", 14),
            place("function:total", "total", "ts/src/payments.ts", 22),
        ] })
    );
    let places = all["places"].as_array().expect("places");
    let page = harness.ok(
        1,
        "type_uses",
        json!({ "project": harness.project, "key": CART, "offset": 1, "limit": 2 }),
    );
    assert_eq!(page["total"], 6);
    assert_eq!(page["places"], json!(places[1..3]));

    // An alias that isn't an object type shows what it aliases; an enum its values.
    let discount = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "ts/src/cart.ts#type_alias:Discount" }),
    );
    assert_eq!(discount["kind"], "type_alias");
    assert_eq!(discount["aliased"]["text"], "Partial<Cart> | null");
    assert_eq!(
        discount["aliased"]["spans"],
        json!([span(0, 7, "builtin", "global"), span(8, 12, "declared", CART)])
    );
    let currency = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "ts/src/cart.ts#enum:Currency" }),
    );
    assert_eq!(currency["members"][0]["kind"], "value");
    assert_eq!(currency["members"][0]["value"], "\"usd\"");
    // A Python dataclass is a class with fields.
    let customer = harness.ok(
        1,
        "type_definition",
        json!({ "project": harness.project, "key": "py/src/shop/models.py#class:Cart" }),
    );
    assert_eq!(customer["kind"], "class");
    assert_eq!(customer["language"], "python");
    assert_eq!(customer["members"][2]["name"], "coupon");
    assert_eq!(customer["members"][2]["default"], true);
}

#[test]
fn typed_line_commands_check_their_limits_and_keys() {
    let harness = Harness::compiled_fixture("typed-lines");
    let project = harness.project.clone();
    let charge = link(&harness, CREATE, "chargePayment");
    let notify = link(&harness, DESCRIBE, "notify");
    // A contract exists only for a resolved call or construction.
    let unresolved = link(
        &harness,
        "ts/src/payments.ts#function:chargePayment",
        "stripe.paymentIntents.create",
    );
    for (source, seq) in [(&charge, 1), (&notify, 1), (&unresolved, 0)] {
        assert_eq!(
            harness.code(
                1,
                "call_contract",
                json!({ "project": project, "source": source, "seq": seq })
            ),
            "not_found",
            "{source} {seq}"
        );
    }
    assert_eq!(
        harness.code(
            1,
            "call_contract",
            json!({ "project": project, "source": "ts/src/nowhere.ts#call[0]", "seq": 0 })
        ),
        "not_found"
    );
    assert_eq!(
        harness.code(
            1,
            "call_contract",
            json!({ "project": project, "source": "", "seq": 0 })
        ),
        "invalid_args"
    );

    // Definitions are of type declarations and classes only.
    assert_eq!(
        harness.code(1, "type_definition", json!({ "project": project, "key": CREATE })),
        "not_found"
    );
    assert_eq!(
        harness.code(1, "type_uses", json!({ "project": project, "key": CREATE })),
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
fn typed_line_commands_need_a_graph() {
    let harness = Harness::empty(options(false));
    let (project, _) = harness.add_copy(&crate::common::fixture("typed-lines"), "typed-lines");
    assert_eq!(
        harness.code(1, "type_definition", json!({ "project": project, "key": CART })),
        "not_compiled"
    );
}

/// A host with one compiled project made of `files` (path, text).
fn project(files: &[(&str, &str)]) -> Harness {
    let sources = tempfile::tempdir().expect("temp dir");
    for (path, text) in files {
        let path = sources.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let mut harness = Harness::empty(options(false));
    let (project, root) = harness.add_copy(sources.path(), "project");
    harness.project = project;
    harness.root = root;
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    harness
}

/// The occurrence key of the `nth` Code link of `definition` whose written callee is `text`.
fn nth_link(harness: &Harness, definition: &str, text: &str, nth: usize) -> String {
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": definition, "lens": "code" }),
    );
    let links = card["code"]["source"]["links"].as_array().expect("links");
    links
        .iter()
        .filter(|link| link["text"] == text)
        .nth(nth)
        .and_then(|link| link["source"].as_str())
        .unwrap_or_else(|| panic!("no link `{text}` #{nth} in {definition}: {links:?}"))
        .to_owned()
}

/// The names of the parameters a contract lists that no argument fills.
fn unfilled(harness: &Harness, source: &str) -> Vec<Value> {
    let contract = harness.ok(
        1,
        "call_contract",
        json!({ "project": harness.project, "source": source, "seq": 0 }),
    );
    contract["sends"]
        .as_array()
        .expect("sends")
        .iter()
        .filter(|send| send["argument"].is_null())
        .map(|send| send["parameter"]["name"].clone())
        .collect()
}

const GATEWAY_PY: &str = r#"class Gateway:
    def ping(self, timeout: int = 5) -> bool:
        return True


def main() -> None:
    gateway = Gateway()
    gateway.ping()
    Gateway.ping(gateway)
    Gateway.ping()
"#;

#[test]
fn a_contract_leaves_out_exactly_the_parameters_the_call_binds() {
    let harness = project(&[("app.py", GATEWAY_PY)]);
    let main = "app.py#function:main";
    // Through an instance the call binds `self`: only the unfilled `timeout` is listed.
    assert_eq!(
        unfilled(&harness, &nth_link(&harness, main, "gateway.ping", 0)),
        [json!("timeout")]
    );
    // Through the class nothing is bound: `self` is filled by the argument, or listed missing.
    assert_eq!(
        unfilled(&harness, &nth_link(&harness, main, "Gateway.ping", 0)),
        [json!("timeout")]
    );
    assert_eq!(
        unfilled(&harness, &nth_link(&harness, main, "Gateway.ping", 1)),
        [json!("self"), json!("timeout")]
    );
}

/// Each group of `definition`'s inherited fields as (its base's text, its first span's style,
/// its members' names).
fn inherited(definition: &Value) -> Vec<(String, String, Vec<String>)> {
    definition["inherited"]
        .as_array()
        .expect("inherited")
        .iter()
        .map(|group| {
            let names = group["members"]
                .as_array()
                .expect("members")
                .iter()
                .filter_map(|member| member["name"].as_str().map(str::to_owned))
                .collect();
            (
                group["from"]["text"].as_str().expect("text").to_owned(),
                group["from"]["spans"][0]["style"]
                    .as_str()
                    .expect("style")
                    .to_owned(),
                names,
            )
        })
        .collect()
}

fn group(from: &str, style: &str, members: &[&str]) -> (String, String, Vec<String>) {
    (
        from.to_owned(),
        style.to_owned(),
        members.iter().map(|&member| member.to_owned()).collect(),
    )
}

#[test]
fn a_type_lists_the_fields_it_inherits_by_base() {
    let harness = Harness::compiled_fixture("typed-lines");
    let definition = |key: &str| {
        harness.ok(
            1,
            "type_definition",
            json!({ "project": harness.project, "key": key }),
        )
    };

    // Depth first in written order; `Timestamped` once, though `Tracked` extends it too.
    let order = definition("ts/src/orders/model.ts#interface:Order");
    assert_eq!(
        inherited(&order),
        [
            group("OrderDraft", "declared", &["customerId", "email", "items"]),
            group("Timestamped", "declared", &["createdAt"]),
            group("Tracked", "declared", &["updatedAt"]),
        ]
    );
    assert_eq!(
        order["inherited"][0]["from"]["spans"][0]["target"],
        "ts/src/orders/model.ts#interface:OrderDraft"
    );
    assert_eq!(order["inherited"][0]["members"][2]["type"]["text"], "CartItem[]");
    // A merged base has no one declaration; a library's base has no members here.
    let shipment = definition("ts/src/orders/model.ts#interface:Shipment");
    assert_eq!(
        inherited(&shipment),
        [
            group("Address", "unresolved", &[]),
            group("Stripe.MetadataParam", "external", &[]),
        ]
    );
    assert_eq!(
        shipment["inherited"][0]["from"]["spans"][0]["note"],
        "ambiguous target"
    );
    assert_eq!(shipment["inherited"][1]["from"]["spans"][0]["module"], "stripe");
    // A class inherits what it extends, not what it implements; a cycle stops.
    let record = definition("ts/src/orders/model.ts#class:OrderRecord");
    assert_eq!(inherited(&record), [group("Entity", "declared", &["id"])]);
    let looped = definition("ts/src/orders/cycle.ts#interface:Loop");
    assert_eq!(inherited(&looped), [group("Knot", "declared", &["knot"])]);

    // Python: a dataclass chain, and a pydantic chain ending in its library.
    let order = definition("py/src/shop/accounts.py#class:Order");
    assert_eq!(
        inherited(&order),
        [
            group("OrderDraft", "declared", &["customer_id", "items"]),
            group("Timestamped", "declared", &["created_at"]),
            group("Generic", "builtin", &[]),
        ]
    );
    let gold = definition("py/src/shop/accounts.py#class:GoldMember");
    assert_eq!(
        inherited(&gold),
        [
            group("Member", "declared", &["tier"]),
            group("Profile", "declared", &["email"]),
            group("BaseModel", "external", &[]),
        ]
    );
    // Heritage clauses are places a type is used, and say so.
    let timestamped = "ts/src/orders/model.ts#interface:Timestamped";
    assert_eq!(definition(timestamped)["uses"], 2);
    let uses = harness.ok(
        1,
        "type_uses",
        json!({ "project": harness.project, "key": timestamped }),
    );
    let places: Vec<(&Value, &Value)> = uses["places"]
        .as_array()
        .expect("places")
        .iter()
        .map(|place| (&place["label"], &place["heritage"]))
        .collect();
    assert_eq!(
        places,
        [
            (&json!("Tracked"), &json!("extends")),
            (&json!("Order"), &json!("extends"))
        ]
    );
    let implemented = harness.ok(
        1,
        "type_uses",
        json!({ "project": harness.project, "key": "ts/src/orders/model.ts#interface:Auditable" }),
    );
    assert_eq!(implemented["places"][0]["heritage"], "implements");
}
