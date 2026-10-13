//! Claims on boards (docs/reader.md › Boards): resource cards and connections grounded in
//! `ts-orders` (the connections of design §4.7 and an `orders` table backed by `interface
//! Order`), the example board saved to itself with real signatures, and evidence files: anchors
//! into them are signed, go outdated when the file changes, and are repaired.

use serde_json::{Value, json};
use unfold_core::testkit::{assert_golden, testdata_dir};

use crate::boards::{card, edit_of, id_of};
use crate::common::Harness;

const CREATE_ORDER: &str = "src/routes/orders.ts#function:createOrder";
const PROCESS_ORDER: &str = "src/jobs/process-order.ts#function:processOrder";
const PLACE: &str = "src/services/order-service.ts#class:OrderService/method:place";
const NOTIFY: &str = "src/services/notifier.ts#function:notifyCustomer";
const STRIPE_CHARGE: &str = "src/services/payment.ts#class:StripeProvider/method:charge";
const ORDERS_QUEUE: &str = "src/queues/queues.ts#variable:ordersQueue";
const EMAIL_QUEUE: &str = "src/queues/queues.ts#variable:emailQueue";
const ORDER: &str = "src/types.ts#interface:Order";

/// The key of the worker entrance on queue `queue`.
fn worker(harness: &Harness, queue: &str) -> String {
    harness
        .entrance_rows(Some(queue))
        .into_iter()
        .find(|row| row["kind"] == "worker" && row["trigger"] == queue)
        .and_then(|row| row["key"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("no worker on {queue}"))
}

/// An agent's connection.
fn connection(id: &str, kind: &str, from: &str, to: &str, step: Option<&str>, anchors: &[&str]) -> Value {
    let mut link = json!({
        "id": id, "kind": kind, "from": from, "to": to, "author": "agent:claude",
        "anchors": anchors.iter().map(|key| json!({ "key": key })).collect::<Vec<_>>(),
    });
    if let Some(step) = step {
        link["step"] = json!(step);
    }
    link
}

/// The `orders` table backed by `interface Order`, marking `marks` as its primary key.
fn orders_table(marks: &[&str]) -> Value {
    json!({
        "id": "orders-table", "kind": "table", "name": "orders", "type": ORDER,
        "columns": marks.iter().map(|name| json!({ "name": name, "key": "primary" })).collect::<Vec<_>>(),
        "anchors": [{ "key": "src/db/repository.ts#class:OrderRepository" }],
        "at": [800, 400], "author": "agent:claude"
    })
}

impl Harness {
    /// Saves page `page` (its items) on board `id`, returning the result or the error message.
    fn save_page(&self, id: &str, page: Value) -> Result<Value, String> {
        let mut edit = json!({ "title": "Orders", "pages": [page] });
        edit["pages"][0]["id"] = json!("page-1");
        self.call(
            1,
            "save_board",
            json!({ "project": self.project, "id": id, "board": edit }),
        )
        .map_err(|error| {
            assert_eq!(error.code, "board_problem", "{}", error.message);
            error.message
        })
    }
}

#[test]
fn the_example_board_saves_to_itself() {
    let harness = Harness::compiled("ts-orders");
    let example =
        std::fs::read_to_string(testdata_dir().join("boards/order-system.json")).expect("read the example");
    let parsed: Value = serde_json::from_str(&example).expect("the example is JSON");
    let id = id_of(&harness.new_board(1, "Order system"));
    let saved = harness.save(
        1,
        &id,
        json!({ "title": parsed["title"], "pages": parsed["pages"] }),
    );
    assert_eq!(
        saved["health"]["items"],
        json!([]),
        "every claim is grounded and signed"
    );
    // Saving signs every claim from verified source: the example holds its real signatures.
    assert_golden("boards/order-system.json", &harness.read(&id));
}

#[test]
fn the_design_connections_are_grounded_in_ts_orders() {
    let harness = Harness::compiled("ts-orders");
    let id = id_of(&harness.new_board(1, "Orders"));
    let mut page = json!({
        "title": "Placing an order", "defaultView": "diagram", "groups": [],
        "cards": [
            card("create", CREATE_ORDER), card("process", PROCESS_ORDER), card("place", PLACE),
            card("notify", NOTIFY), card("stripe", STRIPE_CHARGE),
            card("orders-worker", &worker(&harness, "orders")), card("emails-worker", &worker(&harness, "emails")),
        ],
        "resources": [orders_table(&["id", "customerId"])],
        "links": [
            connection("enqueue-order", "queue", "create", "orders-worker",
                Some("src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]"), &[ORDERS_QUEUE]),
            connection("enqueue-email", "queue", "process", "emails-worker",
                Some("src/jobs/process-order.ts#function:processOrder/expression_statement[0]/call[0]"), &[EMAIL_QUEUE]),
            connection("order-placed", "event", "place", "notify",
                Some("src/services/order-service.ts#class:OrderService/method:place/expression_statement[2]/call[0]"),
                &["src/services/notifier.ts#expression_statement[0]"]),
            connection("charge", "calls", "place", "stripe",
                Some("src/services/order-service.ts#class:OrderService/method:place/expression_statement[0]/call[0]"),
                &["src/services/payment.ts#function:createProvider", "src/services/order-service.ts#variable:orderService"]),
            connection("insert", "writes", "place", "orders-table",
                Some("src/services/order-service.ts#class:OrderService/method:place/expression_statement[1]/call[0]"), &[]),
        ],
    });
    page["links"][0]["carries"] = json!("src/types.ts#interface:OrderJob");
    page["links"][1]["carries"] = json!("src/types.ts#interface:EmailJob");
    page["links"][2]["channel"] = json!("order.placed");
    let saved = harness
        .save_page(&id, page.clone())
        .expect("every connection is grounded");
    assert_eq!(saved["health"]["items"], json!([]));
    let links = &saved["document"]["pages"][0]["links"];
    let channel = |id: &str| {
        links
            .as_array()
            .expect("links")
            .iter()
            .find(|link| link["id"] == id)
            .map(|link| (link["channel"].clone(), link["signature"].is_string()))
    };
    // The workers give their queues as the channels; every connection that cites code is signed.
    assert_eq!(channel("enqueue-order"), Some((json!("orders"), true)));
    assert_eq!(channel("enqueue-email"), Some((json!("emails"), true)));
    assert_eq!(channel("order-placed"), Some((json!("order.placed"), true)));
    assert_eq!(channel("charge"), Some((Value::Null, true)));
    assert!(saved["document"]["pages"][0]["resources"][0]["signature"].is_string());

    let refused = |edit: &dyn Fn(&mut Value)| {
        let mut changed = page.clone();
        edit(&mut changed);
        harness.save_page(&id, changed).expect_err("refused")
    };
    // A column no anchor mentions.
    assert_eq!(
        refused(&|page| {
            page["resources"][0] = json!({
                "id": "orders-table", "kind": "table", "name": "orders",
                "columns": [{ "name": "id" }, { "name": "customer_id" }],
                "anchors": [{ "key": "src/db/repository.ts#class:OrderRepository" }],
                "at": [800, 400], "author": "agent:claude"
            });
        }),
        "page `page-1`, resource `orders-table`: Can't add the table \"orders\": the column \"customer_id\" doesn't appear in its anchors (OrderRepository in src/db/repository.ts). Cite where the column is defined, or leave it out"
    );
    // A mark on a field `Order` doesn't have (it inherits `customerId` from `OrderDraft`).
    assert_eq!(
        refused(&|page| page["resources"][0] = orders_table(&["customer_id"])),
        "page `page-1`, resource `orders-table`: Can't add the table \"orders\": \"customer_id\" isn't a field of Order (src/types.ts). A table backed by a type marks its fields; mark one of: id, status, createdAt, customerId, email, items"
    );
    // A runtime call from a call Unfold follows.
    assert!(
        refused(&|page| {
            page["links"][3]["step"] =
                json!("src/routes/orders.ts#function:createOrder/variable:order/call[0]");
            page["links"][3]["from"] = json!("create");
        })
        .contains(
            "the call orderService.place (line 32) resolves to OrderService.place, so Unfold already draws it"
        ),
    );
    // A carried type the sending code and the anchors never write.
    assert!(
        refused(&|page| page["links"][0]["carries"] = json!("src/types.ts#interface:EmailJob"))
            .contains("EmailJob (src/types.ts) isn't used in createOrder or in its anchors"),
    );
    // A queue connection without its receiving side.
    assert!(
        refused(&|page| page["links"][1]["anchors"] = json!([]))
            .contains("it has no anchors. A queue connection cites its receiving side"),
    );
}

const SCHEMA: &str = "create table customers (\n  id text primary key,\n  email text unique\n);\n";

#[test]
fn evidence_files_anchor_claims_that_go_outdated_when_the_file_changes() {
    let harness = Harness::new("ts-orders", false);
    let config = harness.read("unfold.toml");
    harness.write(
        "unfold.toml",
        &format!("{config}\n[evidence]\nfiles = [\"db/*.sql\"]\n"),
    );
    harness.write("db/schema.sql", SCHEMA);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.new_board(1, "Customers");
    let id = id_of(&board);
    let customers = |columns: &[&str]| {
        json!({
            "id": "customers", "kind": "table", "name": "customers",
            "columns": columns.iter().map(|name| json!({ "name": name })).collect::<Vec<_>>(),
            "anchors": [{ "file": "db/schema.sql", "lines": [1, 4] }],
            "at": [0, 0], "author": "agent:claude"
        })
    };
    let mut edit = edit_of(&board);
    edit["pages"][0]["resources"] = json!([customers(&["id", "email", "customer_id"])]);
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": edit })
        ),
        "board_problem",
        "`customer_id` isn't in the cited lines"
    );
    edit["pages"][0]["resources"] = json!([customers(&["id", "email"])]);
    let saved = harness.save(1, &id, edit);
    let signature = saved["document"]["pages"][0]["resources"][0]["signature"].clone();
    assert!(signature.is_string(), "signed over the file's lines");

    // The file changes and the project compiles: the table is outdated until re-signed.
    harness.write(
        "db/schema.sql",
        &SCHEMA.replace("email text unique", "email citext unique"),
    );
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let health = harness.ok(1, "board_health", json!({ "project": harness.project, "id": id }));
    assert_eq!(health["label"], "1 outdated");
    let item = &health["health"]["items"][0];
    assert_eq!(
        (&item["kind"], &item["target"], &item["repairs"]),
        (
            &json!("outdated"),
            &json!({ "resource": "customers" }),
            &json!(["resign", "removeResource"])
        )
    );
    let repair = |repair: Value| {
        harness.ok(
            1,
            "repair_board",
            json!({ "project": harness.project, "id": id, "repair": repair }),
        )
    };
    let resigned =
        repair(json!({ "page": "page-1", "kind": "resign", "target": { "resource": "customers" } }));
    assert_eq!(resigned["health"]["items"], json!([]));
    assert_ne!(
        resigned["document"]["pages"][0]["resources"][0]["signature"],
        signature
    );

    // An anchor is reconnected by its position, and a resource card removed.
    let reconnected = repair(json!({
        "page": "page-1", "kind": "reconnect_anchor", "item": "customers", "anchor": 0,
        "to": { "file": "db/schema.sql", "lines": [1, 3] }
    }));
    assert_eq!(
        reconnected["document"]["pages"][0]["resources"][0]["anchors"],
        json!([{ "file": "db/schema.sql", "lines": [1, 3] }])
    );
    let removed = repair(json!({ "page": "page-1", "kind": "remove_resource", "resource": "customers" }));
    assert!(removed["document"]["pages"][0].get("resources").is_none());
}
