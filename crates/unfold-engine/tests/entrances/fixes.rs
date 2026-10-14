//! One-candidate fixes for missing keys: offered when exactly one candidate fits, never applied.

use std::sync::Arc;

use unfold_core::vocab::{NodeKind, TargetState};
use unfold_core::{Graph, Key, RepoPath};
use unfold_engine::entrances::{Fix, FixKind};
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, LinkBasis, UpdateRequest};

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

/// The fix to `code`, a block in `path`.
fn fix(kind: FixKind, path: &str, code: &str) -> Option<Fix> {
    Some(Fix {
        kind,
        code: Key::from_text(code),
        path: RepoPath::new(path).expect("a repo path"),
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
        fix(
            FixKind::MovedDefinition,
            "shop/orders.py",
            "shop/orders.py#create_order"
        )
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

const OBJECT: &str =
    "export const slack = {\n  post(channel: string): void {\n    console.log(channel);\n  },\n};\n";
const NAMESPACE: &str = "export namespace Util {\n  export function helper(): void {}\n}\n";
const PLAIN: &str = "export function plain(): void {}\n";
const CLASS: &str = "export class Box {\n  open(): void {}\n}\n";

/// TypeScript qualifies an object literal's methods and a namespace's members in their names
/// (`slack.post`), and a class's members only in their keys (`Box.open`): each is found by its
/// key's names.
#[test]
fn a_moved_typescript_definition_is_fixed_however_its_name_is_qualified() {
    let codes = [
        "src/lib/slack.ts#slack.post",
        "src/lib/slack.ts#Util.helper",
        "src/lib/slack.ts#plain",
        "src/lib/slack.ts#Box.open",
    ];
    let folder = Folder::new(&[
        ("src/lib/slack.ts", &format!("{OBJECT}{NAMESPACE}{PLAIN}{CLASS}")),
        ("unfold.toml", &one_app("", &codes)),
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

    folder.write("src/lib/slack.ts", "export const unrelated = 1;\n");
    folder.write("src/notify.ts", &format!("{OBJECT}{NAMESPACE}{PLAIN}{CLASS}"));
    background(&mut engine, &["src/lib/slack.ts", "src/notify.ts"]);
    let fixes: Vec<_> = codes.iter().map(|code| fix_for(&engine, code)).collect();
    let moved: Vec<_> = codes
        .iter()
        .map(|code| {
            let code = code.replace("src/lib/slack.ts", "src/notify.ts");
            fix(FixKind::MovedDefinition, "src/notify.ts", &code)
        })
        .collect();
    assert_eq!(fixes, moved);
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
        fix(FixKind::EditedRoute, "src/routes.ts", renamed.as_str()),
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
    let moved = fix(FixKind::MovedModule, "tasks/worker.py", "tasks/worker.py#");
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

/// The old module's definitions are stored with the graph's basis, so a session restored after
/// its file left the graph still recognizes the module, and its basis keeps them.
#[test]
fn a_moved_module_is_still_fixed_after_a_restart() {
    let folder = Folder::new(&[
        ("a/cli.py", WORKER),
        ("unfold.toml", "[[app]]\nname = \"Cli\"\ncode = \"a/cli.py#\"\n"),
    ]);
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = folder.engine_with(Arc::clone(&cache));
    compile(&mut engine);
    folder.remove("a/cli.py");
    folder.write("b/cli.py", WORKER);
    let moved = background(&mut engine, &["a/cli.py", "b/cli.py"]);
    let fixed = fix(FixKind::MovedModule, "b/cli.py", "b/cli.py#");
    assert_eq!(fix_for(&engine, "a/cli.py#"), fixed);

    let graph: Graph = (*moved.graph).clone();
    let disks: Vec<_> = graph
        .files
        .iter()
        .filter_map(|file| Some((file.file.path.clone(), engine.disk_state(&file.file.path)?)))
        .collect();
    // As the store keeps it.
    let basis = LinkBasis::decode(&moved.basis.encode());
    let mut reopened = folder.engine_with(cache);
    reopened.restore(graph, disks, basis).expect("restore");
    let checked = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(fix_for(&reopened, "a/cli.py#"), fixed);
    assert_eq!(checked.basis, moved.basis);
}

#[test]
fn a_moved_module_with_no_definitions_has_no_fix() {
    let folder = Folder::new(&[
        ("jobs/a/__main__.py", "print(\"a\")\n"),
        ("tools/b/__main__.py", "print(\"b\")\n"),
        (
            "unfold.toml",
            "[[app]]\nname = \"Job\"\ncode = \"jobs/a/__main__.py#\"\n",
        ),
    ]);
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    assert_eq!(
        states(&first.graph.apps),
        [("jobs/a/__main__.py#".to_owned(), TargetState::Found)]
    );

    folder.remove("jobs/a/__main__.py");
    background(&mut engine, &["jobs/a/__main__.py"]);
    assert_eq!(
        fix_for(&engine, "jobs/a/__main__.py#"),
        None,
        "an unrelated module with the same file name and no definitions isn't offered"
    );
}
