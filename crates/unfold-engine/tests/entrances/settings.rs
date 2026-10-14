//! `[[app]]` tables and the rest of the engine: changing only them never recompiles, relinks,
//! or invalidates facts; a bad entry stays out of the graph; and a file that doesn't parse keeps
//! the last good apps.

use unfold_core::ContentHash;
use unfold_core::vocab::TargetState;
use unfold_engine::settings::{EntryProblemKind, SettingsFile};
use unfold_engine::{EntrancesError, ProblemKind};

use super::{Folder, background, compile, states};

const API: &str =
    "def create_order(order):\n    return order\n\ndef refund_order(order):\n    return order\n";
const SETTINGS: &str = "# Shared settings.\n[scope]\nexclude = [\"legacy/**\"]\n\n[[app]]\nname = \"Shop API\"\nrun = \"uvicorn shop.api:app\"\nentrances = [\n  { name = \"Place an order\", code = \"shop/api.py#create_order\" },\n]\n";

fn project() -> Folder {
    Folder::new(&[
        ("shop/api.py", API),
        ("src/index.ts", "export function main() {\n  return 1;\n}\n"),
        ("unfold.toml", SETTINGS),
    ])
}

#[test]
fn changing_only_apps_never_recompiles_or_relinks() {
    let folder = project();
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    let hash = engine.settings().hash();

    let edited = SETTINGS
        .replace("uvicorn shop.api:app", "gunicorn shop.api:app")
        .replace(
            "create_order\" },\n",
            "create_order\" },\n  { name = \"Refund an order\", code = \"shop/api.py#refund_order\" },\n",
        )
        + "\n[[app]]\nname = \"Web\"\ncode = \"src/index.ts#main\"\n";
    folder.write("unfold.toml", &edited);
    let changed = background(&mut engine, &["unfold.toml"]);
    assert!(!changed.stats.relinked, "only `[[app]]` tables changed");
    assert_eq!(changed.stats.files_extracted, 0);
    assert_eq!(changed.stats.files_read, 0);
    assert_eq!(engine.settings().hash(), hash);
    assert_eq!(
        changed.basis, first.basis,
        "the link basis leaves `[[app]]` tables out"
    );
    assert_eq!(changed.graph.files, first.graph.files);
    assert!(changed.summary.apps_changed && changed.summary.inputs_changed);
    assert!(changed.summary.changed_files.is_empty() && changed.summary.changed_keys.is_empty());
    assert_eq!(
        states(&changed.graph.apps),
        [
            ("shop/api.py#create_order".to_owned(), TargetState::Found),
            ("shop/api.py#refund_order".to_owned(), TargetState::Found),
            ("src/index.ts#main".to_owned(), TargetState::Found),
        ]
    );
    assert_eq!(
        changed.graph.apps[0].run.as_deref(),
        Some("gunicorn shop.api:app")
    );
    let record = changed
        .graph
        .inputs
        .iter()
        .find(|input| input.path.as_str() == "unfold.toml")
        .expect("the settings file's record");
    assert_eq!(record.hash, ContentHash::of(edited.as_bytes()));

    folder.write("unfold.toml", &edited.replace("# Shared settings.", "# Ours."));
    let commented = background(&mut engine, &["unfold.toml"]);
    assert!(!commented.stats.relinked, "a comment shapes nothing");
    assert!(commented.summary.inputs_changed && !commented.summary.apps_changed);

    folder.write("unfold.toml", &edited.replace("legacy/**", "old/**"));
    let scoped = background(&mut engine, &["unfold.toml"]);
    assert!(scoped.stats.relinked, "the compile settings changed");
    assert_eq!(
        scoped.stats.files_extracted, 0,
        "facts survive a scope change that keeps the files"
    );
}

#[test]
fn resolve_entrances_reads_the_apps_again_without_compiling() {
    let folder = project();
    let mut engine = folder.engine();
    let first = compile(&mut engine);

    folder.write("unfold.toml", &SETTINGS.replace("create_order", "cancel_order"));
    let resolved = engine.resolve_entrances().expect("only apps changed");
    assert!(resolved.apps_changed && resolved.inputs_changed);
    assert_eq!(resolved.graph.files, first.graph.files);
    assert_eq!(*engine.graph(), resolved.graph);
    assert_eq!(
        states(&resolved.graph.apps),
        [("shop/api.py#cancel_order".to_owned(), TargetState::Missing)]
    );
    let after = background(&mut engine, &["unfold.toml"]);
    assert!(!after.stats.relinked && !after.summary.apps_changed);
    assert_eq!(after.stats.files_extracted, 0);

    folder.write("unfold.toml", &SETTINGS.replace("legacy/**", "old/**"));
    assert!(matches!(
        engine.resolve_entrances(),
        Err(EntrancesError::CompileNeeded)
    ));
    assert_eq!(*engine.graph(), resolved.graph, "nothing changed");
}

#[test]
fn a_settings_file_that_does_not_parse_keeps_the_last_good_apps() {
    let folder = project();
    let mut engine = folder.engine();
    let first = compile(&mut engine);
    let entries = engine.app_entries().to_vec();
    assert_eq!(entries.len(), 1);

    folder.write("unfold.toml", "[[app]\nname = \"Broken\"\n");
    let broken = background(&mut engine, &["unfold.toml"]);
    assert!(
        broken
            .problems
            .iter()
            .any(|problem| problem.kind == ProblemKind::Settings),
        "{:?}",
        broken.problems
    );
    assert!(!broken.complete);
    assert_eq!(engine.app_entries(), entries.as_slice());
    assert_eq!(broken.graph.apps, first.graph.apps);
    let settings = SettingsFile::parse(SETTINGS).expect("valid").settings;
    assert_eq!(
        engine.settings(),
        &settings,
        "the last good compile settings stay"
    );

    let resolved = engine.resolve_entrances().expect("the compile settings stay");
    assert!(!resolved.apps_changed);
    assert!(
        resolved
            .problems
            .iter()
            .any(|problem| problem.kind == ProblemKind::Settings)
    );
}

#[test]
fn an_entry_with_a_problem_stays_out_of_the_graph() {
    let folder = Folder::new(&[
        ("shop/api.py", API),
        (
            "unfold.toml",
            "[[app]]\nname = \"Shop API\"\nentrances = [\n  { name = \"Place an order\", code = \"shop/api.py#create_order\" },\n  { name = \"Refund\", code = \"shop/api.py#refund_order\", kind = \"http\" },\n  { name = \"place an order!\", code = \"shop/api.py#refund_order\" },\n]\n\n[[app]]\nname = \"Jobs\"\nqueue = \"emails\"\n\n[[app]]\nname = \"shop-api\"\n",
        ),
    ]);
    let mut engine = folder.engine();
    let outcome = compile(&mut engine);
    assert!(outcome.complete, "entry problems don't fail the file");
    let apps = &outcome.graph.apps;
    assert_eq!(apps.len(), 1);
    let entrances: Vec<&str> = apps[0]
        .entrances
        .iter()
        .map(|entrance| entrance.key.as_str())
        .collect();
    assert_eq!(entrances, ["entrance:shop-api/place-an-order"]);

    let entries = engine.app_entries();
    let problems: Vec<_> = entries
        .iter()
        .map(|app| app.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(
        problems,
        [
            None,
            Some(EntryProblemKind::UnknownField),
            Some(EntryProblemKind::DuplicateKey)
        ]
    );
    let entrance_problems: Vec<_> = entries[0]
        .entrances
        .iter()
        .map(|entrance| entrance.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(
        entrance_problems,
        [
            None,
            Some(EntryProblemKind::UnknownField),
            Some(EntryProblemKind::DuplicateKey)
        ]
    );
}
