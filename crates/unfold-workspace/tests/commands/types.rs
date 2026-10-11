//! Typed lines (docs/ipc.md › Typed lines): `line_labels`, `call_contract`, `type_definition`,
//! and `type_uses` on a copy of `testdata/fixtures/typed-lines`, with their limits and error
//! codes.

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

/// The labels of `lines` (`(source, seq)`), each as JSON.
fn labels(harness: &Harness, lines: &[(&str, u32)]) -> Vec<Value> {
    let lines: Vec<Value> = lines
        .iter()
        .map(|(source, seq)| json!({ "source": source, "seq": seq }))
        .collect();
    let result = harness.ok(
        1,
        "line_labels",
        json!({ "project": harness.project, "lines": lines }),
    );
    let result = result.as_array().expect("one result per line").clone();
    assert_eq!(result.len(), lines.len());
    for (asked, answered) in lines.iter().zip(&result) {
        assert_eq!(answered["source"], asked["source"]);
        assert_eq!(answered["seq"], asked["seq"]);
    }
    result.into_iter().map(|line| line["label"].clone()).collect()
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
fn typescript_lines_carry_the_types_they_send_and_return() {
    let harness = Harness::compiled_fixture("typed-lines");
    let reserve = link(&harness, CREATE, "reserveInventory");
    let charge = link(&harness, CREATE, "chargePayment");
    let construction = link(&harness, CREATE, "PaymentError");
    let notify = link(&harness, DESCRIBE, "notify");
    let found = labels(
        &harness,
        &[
            (&reserve, 0),
            (&charge, 0),
            (&construction, 0),
            (&notify, 1),
            (&notify, 7),
            ("ts/src/orders/create.ts#nothing", 0),
        ],
    );
    let cart_item = "ts/src/cart.ts#interface:CartItem";
    let hold = "ts/src/inventory.ts#interface:Hold";
    let order = "ts/src/orders/create.ts#interface:Order";
    // `reserveInventory(cart.items)` → `(CartItem[]) → Promise<Hold>`.
    assert_eq!(
        found[0],
        json!({
            "inputs": [{ "type": text("CartItem[]", vec![span(0, 8, "declared", cart_item)]) }],
            "inputsUnknown": false,
            "output": text("Promise<Hold>", vec![span(0, 7, "builtin", "global"), span(8, 12, "declared", hold)]),
        })
    );
    // `chargePayment(cart, req.user)` → `(Cart, User) → Promise<Charge>`.
    assert_eq!(
        found[1],
        json!({
            "inputs": [
                { "type": text("Cart", vec![span(0, 4, "declared", CART)]) },
                { "type": text("User", vec![span(0, 4, "declared", "ts/src/user.ts#interface:User")]) },
            ],
            "inputsUnknown": false,
            "output": text("Promise<Charge>", vec![
                span(0, 7, "builtin", "global"),
                span(8, 14, "declared", "ts/src/payments.ts#interface:Charge"),
            ]),
        })
    );
    // `new PaymentError(String(error))` → `(string) → PaymentError`.
    assert_eq!(
        found[2],
        json!({
            "inputs": [{ "type": text("string", vec![]) }],
            "inputsUnknown": false,
            "output": text("PaymentError", vec![span(0, 12, "declared", "ts/src/errors.ts#class:PaymentError")]),
        })
    );
    // `notify(order, summary)` passes `summary`: its own signature, `(Order) → string`.
    assert_eq!(
        found[3],
        json!({
            "inputs": [{ "type": text("Order", vec![span(0, 5, "declared", order)]) }],
            "inputsUnknown": false,
            "output": text("string", vec![]),
        })
    );
    assert_eq!(found[4], Value::Null, "no such edge");
    assert_eq!(found[5], Value::Null, "no such occurrence");
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
fn python_lines_keep_keywords_and_leave_out_the_bound_receiver() {
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
    let (construct, through_instance, through_class) = (by_line(15), by_line(17), by_line(20));
    let found = labels(
        &harness,
        &[(&construct, 0), (&through_instance, 0), (&through_class, 0)],
    );
    let builtin_str = text("str", vec![span(0, 3, "builtin", "python:builtins")]);
    let receipt = text(
        "Receipt",
        vec![span(0, 7, "declared", "py/src/shop/models.py#class:Receipt")],
    );
    // `Gateway("sk_test", retries=2)` → `(str, retries=int) → Gateway`.
    assert_eq!(
        found[0],
        json!({
            "inputs": [
                { "type": builtin_str },
                { "keyword": "retries", "type": text("int", vec![span(0, 3, "builtin", "python:builtins")]) },
            ],
            "inputsUnknown": false,
            "output": text("Gateway", vec![span(0, 7, "declared", "py/src/shop/payments.py#class:Gateway")]),
        })
    );
    // `gateway.charge(cart, customer, currency="eur")`: `self` is bound by the instance.
    assert_eq!(
        found[1],
        json!({
            "inputs": [
                { "type": text("Cart", vec![span(0, 4, "declared", "py/src/shop/models.py#class:Cart")]) },
                { "type": text("Customer", vec![span(0, 8, "declared", "py/src/shop/models.py#class:Customer")]) },
                { "keyword": "currency", "type": builtin_str },
            ],
            "inputsUnknown": false,
            "output": receipt,
        })
    );
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
    let too_many: Vec<Value> = (0..501).map(|_| json!({ "source": charge, "seq": 0 })).collect();
    assert_eq!(
        harness.code(1, "line_labels", json!({ "project": project, "lines": too_many })),
        "invalid_args"
    );
    let enough: Vec<Value> = (0..500).map(|_| json!({ "source": charge, "seq": 0 })).collect();
    let labeled = harness.ok(1, "line_labels", json!({ "project": project, "lines": enough }));
    assert_eq!(labeled.as_array().map(Vec::len), Some(500));
    assert_eq!(
        harness.code(
            1,
            "line_labels",
            json!({ "project": project, "lines": [{ "source": charge }] })
        ),
        "invalid_args"
    );
    assert_eq!(
        harness.code(
            1,
            "line_labels",
            json!({ "project": project, "lines": [], "extra": 1 })
        ),
        "invalid_args"
    );

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
    assert_eq!(
        harness.code(1, "line_labels", json!({ "project": project, "lines": [] })),
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

    @classmethod
    def build(cls, key: str) -> "Gateway":
        return cls()

    hook = print(build)


def run(callback) -> None:
    callback()


def main() -> None:
    gateway = Gateway()
    gateway.ping()
    Gateway.ping(gateway)
    Gateway.ping()
    run(gateway.ping)
    run(Gateway.ping)
    run(Gateway.build)
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

#[test]
fn a_passed_callable_shows_what_it_takes_after_what_it_binds() {
    let harness = project(&[("app.py", GATEWAY_PY)]);
    let main = "app.py#function:main";
    let int = text("int", vec![span(0, 3, "builtin", "python:builtins")]);
    let str_type = text("str", vec![span(0, 3, "builtin", "python:builtins")]);
    let found = labels(
        &harness,
        &[
            (&nth_link(&harness, main, "run", 0), 1),
            (&nth_link(&harness, main, "run", 1), 1),
            (&nth_link(&harness, main, "run", 2), 1),
        ],
    );
    let bool_type = text("bool", vec![span(0, 4, "builtin", "python:builtins")]);
    // The written return type is a string annotation: its link lies inside the quotes.
    let gateway = text(
        "\"Gateway\"",
        vec![span(1, 8, "declared", "app.py#class:Gateway")],
    );
    // `gateway.ping` binds the instance; `Gateway.ping` binds nothing, so `self` shows.
    assert_eq!(
        found[0],
        json!({ "inputs": [{ "type": int }], "inputsUnknown": false, "output": bool_type })
    );
    assert_eq!(
        found[1],
        json!({ "inputs": [{ "name": "self" }, { "type": int }], "inputsUnknown": false, "output": bool_type })
    );
    // `Gateway.build` is a classmethod: the class is bound.
    assert_eq!(
        found[2],
        json!({ "inputs": [{ "type": str_type }], "inputsUnknown": false, "output": gateway })
    );
    // A classmethod passed by its bare name in the class body: what it binds isn't proven.
    let hook = nth_link(&harness, "app.py#class:Gateway", "print", 0);
    let found = labels(&harness, &[(&hook, 1)]);
    assert_eq!(found[0]["inputs"], json!([]));
    assert_eq!(found[0]["inputsUnknown"], true);
}
