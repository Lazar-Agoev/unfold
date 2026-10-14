//! Each `code` key's state after a compile.

use unfold_core::vocab::{NodeKind, TargetState};
use unfold_engine::validate::validate_graph;

use super::{Folder, background, compile, key_of, one_app, states};

const API: &str = "def create_order(order):\n    if order:\n        print(order)\n    return order\n";
const ROUTES: &str =
    "const router = makeRouter();\nrouter.post(\"/orders/:id\", (req, res) => {\n  res.send(\"ok\");\n});\n";
const SCOPE: &str = "[scope]\nexclude = [\"legacy/**\"]\n\n";

#[test]
fn each_code_key_gets_its_state() {
    let folder = Folder::new(&[
        ("shop/api.py", API),
        ("src/routes.ts", ROUTES),
        ("legacy/old.py", "def run():\n    pass\n"),
        ("README.md", "# Shop\n"),
        ("broken.py", "def ok():\n    pass\n"),
        ("unfold.toml", SCOPE),
    ]);
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    assert!(first.graph.apps.is_empty(), "no `[[app]]` tables, no apps");
    let decision = key_of(&first.graph, "shop/api.py", NodeKind::Decision, |_| true);
    let call = key_of(&first.graph, "shop/api.py", NodeKind::Call, |key| {
        key.contains("print")
    });
    let handler = key_of(&first.graph, "src/routes.ts", NodeKind::Function, |key| {
        key.ends_with("/callback[0]")
    });

    let codes = [
        "shop/api.py#",
        "shop/api.py#create_order",
        handler.as_str(),
        "shop/api.py#gone",
        "nowhere/x.py#f",
        "not a key",
        "legacy/old.py#run",
        "README.md#",
        decision.as_str(),
        call.as_str(),
        "broken.py#ok",
    ];
    folder.write("unfold.toml", &one_app(SCOPE, &codes));
    folder.write("broken.py", "def ok(:\n    pass\n");
    let outcome = background(&mut engine, &["unfold.toml", "broken.py"]);
    use TargetState::*;
    let expected = [
        Found,
        Found,
        Found,
        Missing,
        Missing,
        Missing,
        NotAnalyzed,
        NotAnalyzed,
        NotAStart,
        NotAStart,
        Stale,
    ];
    let expected: Vec<(String, TargetState)> = codes
        .iter()
        .map(|code| (*code).to_owned())
        .zip(expected)
        .collect();
    assert_eq!(states(&outcome.graph.apps), expected);
    assert!(outcome.summary.apps_changed);
    assert_eq!(engine.graph().apps, outcome.graph.apps);
    assert!(
        validate_graph(&outcome.graph, None, &[]).is_empty(),
        "the apps are valid graph records"
    );
    assert!(engine.fixes().is_empty(), "no candidate fits any missing key");
}

#[test]
fn an_app_code_is_resolved_like_an_entrance_code() {
    let folder = Folder::new(&[
        ("shop/api.py", API),
        (
            "unfold.toml",
            "[[app]]\nname = \"Shop\"\nrun = \"python -m shop\"\ncode = \"shop/api.py#\"\n\n[[app]]\nname = \"Gone\"\ncode = \"shop/gone.py#\"\n",
        ),
    ]);
    let mut engine = folder.engine();
    let outcome = compile(&mut engine);
    let apps = &outcome.graph.apps;
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0].key.as_str(), "entrance:shop");
    assert_eq!(apps[0].run.as_deref(), Some("python -m shop"));
    assert_eq!(
        states(apps),
        [
            ("shop/api.py#".to_owned(), TargetState::Found),
            ("shop/gone.py#".to_owned(), TargetState::Missing)
        ]
    );
}
