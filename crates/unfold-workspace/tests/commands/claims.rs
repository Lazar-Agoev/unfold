//! Claims on boards: resource cards and connections grounded in `ts-orders` (queues, a call
//! Unfold can't follow, and an `orders` table backed by `interface Order`), the example board
//! saved to itself with real signatures, and evidence files: anchors into them are signed, go
//! outdated when the file changes, and are repaired. The reader's tests cover every grounding
//! rule; these cover the save path.

use serde_json::{Value, json};
use unfold_core::testkit::{assert_golden, testdata_dir};

use crate::boards::{card, edit_of, entrance_card, id_of};
use crate::common::Harness;

const CREATE_ORDER: &str = "src/routes/orders.ts#createOrder";
const PROCESS_ORDER: &str = "src/jobs/process-order.ts#processOrder";
const PLACE: &str = "src/services/order-service.ts#OrderService.place";
const STRIPE_CHARGE: &str = "src/services/payment.ts#StripeProvider.charge";
const ORDERS_QUEUE: &str = "src/queues/queues.ts#/call:Queue";
const EMAIL_QUEUE: &str = "src/queues/queues.ts#/call:Queue[1]";
const ORDER: &str = "src/types.ts#Order";

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
        "anchors": [{ "key": "src/db/repository.ts#OrderRepository" }],
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
            json!({ "project": self.project, "id": id, "base": self.base(1, id), "board": edit }),
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
fn connections_are_grounded_in_ts_orders() {
    let harness = Harness::compiled("ts-orders");
    let id = id_of(&harness.new_board(1, "Orders"));
    let page = json!({
        "title": "Placing an order", "defaultView": "expanded",
        "cards": [
            card("create", CREATE_ORDER), card("process", PROCESS_ORDER), card("place", PLACE),
            card("stripe", STRIPE_CHARGE),
            entrance_card("orders-worker", "entrance:worker/process-an-order"),
            entrance_card("emails-worker", "entrance:worker/send-an-email"),
        ],
        "resources": [orders_table(&["id", "customerId"])],
        "links": [
            connection("enqueue-order", "queue", "create", "orders-worker",
                Some("src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add"),
                &[ORDERS_QUEUE, "entrance:worker/process-an-order"]),
            connection("enqueue-email", "queue", "process", "emails-worker",
                Some("src/jobs/process-order.ts#processOrder/call:emailQueue.add"),
                &[EMAIL_QUEUE, "entrance:worker/send-an-email"]),
            connection("charge", "calls", "place", "stripe",
                Some("src/services/order-service.ts#OrderService.place/call:this.payments.charge"),
                &["src/services/payment.ts#createProvider"]),
            connection("insert", "writes", "place", "orders-table",
                Some("src/services/order-service.ts#OrderService.place/call:this.repo.insert"), &[]),
        ],
    });
    let saved = harness
        .save_page(&id, page.clone())
        .expect("every connection is grounded");
    assert_eq!(saved["health"]["items"], json!([]));
    let links = saved["document"]["pages"][0]["links"].as_array().expect("links");
    assert!(
        links.iter().all(|link| link["signature"].is_string()),
        "every connection that cites code is signed: {links:?}"
    );
    assert!(saved["document"]["pages"][0]["resources"][0]["signature"].is_string());

    let refused = |edit: &dyn Fn(&mut Value)| {
        let mut changed = page.clone();
        edit(&mut changed);
        harness.save_page(&id, changed).expect_err("refused")
    };
    // A column no anchor mentions.
    assert!(
        refused(&|page| {
            page["resources"][0] = json!({
                "id": "orders-table", "kind": "table", "name": "orders",
                "columns": [{ "name": "id" }, { "name": "customer_id" }],
                "anchors": [{ "key": "src/db/repository.ts#OrderRepository" }],
                "at": [800, 400], "author": "agent:claude"
            });
        })
        .contains("customer_id"),
    );
    // A mark on a field `Order` doesn't have.
    assert!(refused(&|page| page["resources"][0] = orders_table(&["customer_id"])).contains("customer_id"));
    // A `calls` connection starts at an unknown call, never one Unfold follows.
    assert!(
        refused(&|page| page["links"][2]["step"] =
            json!("src/routes/orders.ts#createOrder/call:orderService.place"))
        .contains("orderService.place"),
    );
    // A connection's step is a call.
    assert!(
        !refused(&|page| page["links"][3]["step"] = json!("src/routes/orders.ts#createOrder/if[0]"))
            .is_empty()
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
            json!({ "project": harness.project, "id": id, "base": board["revision"], "board": edit })
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
            &json!(["resign", "remove"])
        )
    );
    let repair = |repair: Value| {
        harness.ok(
            1,
            "repair_board",
            json!({ "project": harness.project, "id": id, "base": harness.base(1, &id), "repair": repair }),
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
    let removed =
        repair(json!({ "page": "page-1", "kind": "remove", "target": { "resource": "customers" } }));
    assert!(removed["document"]["pages"][0].get("resources").is_none());
}
