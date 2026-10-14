//! One-candidate fixes for missing keys: offered when exactly one candidate fits, never applied.

use unfold_core::Key;
use unfold_core::vocab::{NodeKind, TargetState};
use unfold_engine::Engine;
use unfold_engine::entrances::{Fix, FixKind};

use super::{Folder, background, compile, key_of, one_app, states};

/// The fix offered for `code`, which must be missing.
fn fix_for(engine: &Engine, code: &str) -> Option<Fix> {
    let target = engine
        .graph()
        .apps
        .iter()
        .flat_map(|app| {
            app.code
                .iter()
                .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
        })
        .find(|target| target.code.as_str() == code)
        .unwrap_or_else(|| panic!("a target for {code}"));
    assert_eq!(target.state, TargetState::Missing, "{code}");
    engine.fixes().get(target).cloned()
}

fn fix(kind: FixKind, code: &str) -> Option<Fix> {
    Some(Fix {
        kind,
        code: Key::from_text(code),
    })
}

const CREATE: &str = "def create_order(order):\n    return order\n";

#[test]
fn a_moved_definition_is_fixed_when_one_definition_has_its_name() {
    let folder = Folder::new(&[
        (
            "shop/api.py",
            &format!("{CREATE}\ndef refund_order(order):\n    return order\n"),
        ),
        ("unfold.toml", &one_app("", &["shop/api.py#create_order"])),
    ]);
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    assert_eq!(
        states(&first.graph.apps),
        [("shop/api.py#create_order".to_owned(), TargetState::Found)]
    );

    folder.write("shop/api.py", "def refund_order(order):\n    return order\n");
    folder.write("shop/orders.py", CREATE);
    background(&mut engine, &["shop/api.py", "shop/orders.py"]);
    assert_eq!(
        fix_for(&engine, "shop/api.py#create_order"),
        fix(FixKind::MovedDefinition, "shop/orders.py#create_order")
    );
    let toml = std::fs::read_to_string(folder.path().join("unfold.toml")).expect("read");
    assert!(
        toml.contains("shop/api.py#create_order"),
        "a fix is never applied"
    );

    folder.write("billing/orders.py", CREATE);
    background(&mut engine, &["billing/orders.py"]);
    assert_eq!(
        fix_for(&engine, "shop/api.py#create_order"),
        None,
        "two candidates"
    );
}

const ROUTES: &str = "const router = makeRouter();\nrouter.post(\"/orders/:id\", (req, res) => {\n  res.send(\"order\");\n});\nrouter.post(\"/refunds\", (req, res) => {\n  res.send(\"refund\");\n});\n";

#[test]
fn an_edited_route_literal_is_fixed_when_one_unnamed_registration_fits() {
    let place = "src/routes.ts#/call:router.post(\"/orders/:id\")/callback[0]";
    let refund = "src/routes.ts#/call:router.post(\"/refunds\")/callback[0]";
    let folder = Folder::new(&[
        ("src/routes.ts", ROUTES),
        ("unfold.toml", &one_app("", &[place, refund])),
    ]);
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    assert!(
        states(&first.graph.apps)
            .iter()
            .all(|(_, state)| *state == TargetState::Found),
        "{:?}",
        states(&first.graph.apps)
    );

    folder.write(
        "src/routes.ts",
        &ROUTES.replace("/orders/:id", "/orders/:orderId"),
    );
    let edited = background(&mut engine, &["src/routes.ts"]);
    let renamed = key_of(&edited.graph, "src/routes.ts", NodeKind::Function, |key| {
        key.contains("orderId") && key.ends_with("/callback[0]")
    });
    assert_eq!(
        fix_for(&engine, place),
        fix(FixKind::EditedRoute, renamed.as_str()),
        "the refund route is named by an entrance, so only one registration fits"
    );

    folder.write(
        "src/routes.ts",
        &format!(
            "{}router.post(\"/extra\", (req, res) => {{\n  res.send(\"extra\");\n}});\n",
            ROUTES.replace("/orders/:id", "/orders/:orderId")
        ),
    );
    background(&mut engine, &["src/routes.ts"]);
    assert_eq!(fix_for(&engine, place), None, "two candidates");
}

const WORKER: &str = "def fetch():\n    pass\n\ndef store():\n    pass\n";

#[test]
fn a_moved_module_is_fixed_when_one_module_has_its_name_and_definitions() {
    let folder = Folder::new(&[
        ("jobs/worker.py", WORKER),
        ("misc/worker.py", "def other():\n    pass\n"),
        (
            "unfold.toml",
            "[[app]]\nname = \"Worker\"\ncode = \"jobs/worker.py#\"\n",
        ),
    ]);
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    assert_eq!(
        states(&first.graph.apps),
        [("jobs/worker.py#".to_owned(), TargetState::Found)]
    );

    folder.remove("jobs/worker.py");
    folder.write("tasks/worker.py", WORKER);
    background(&mut engine, &["jobs/worker.py", "tasks/worker.py"]);
    let moved = fix(FixKind::MovedModule, "tasks/worker.py#");
    assert_eq!(fix_for(&engine, "jobs/worker.py#"), moved);

    folder.write("unrelated.py", "x = 1\n");
    background(&mut engine, &["unrelated.py"]);
    assert_eq!(
        fix_for(&engine, "jobs/worker.py#"),
        moved,
        "the old module's definitions are remembered after its file left the graph"
    );

    folder.write("other/worker.py", WORKER);
    background(&mut engine, &["other/worker.py"]);
    assert_eq!(fix_for(&engine, "jobs/worker.py#"), None, "two candidates");
}
