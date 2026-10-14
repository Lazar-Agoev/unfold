use unfold_core::{ByteRange, Key};

use super::*;

/// The compile settings of a valid file.
fn compile_settings(text: &str) -> Settings {
    SettingsFile::parse(text).unwrap().settings
}

fn error(text: &str) -> (String, u32, u32) {
    match SettingsFile::parse(text) {
        Err(SettingsError::Invalid {
            message,
            line,
            column,
            ..
        }) => (message.into(), line, column),
        other => panic!("expected an invalid-settings error for {text:?}, got {other:?}"),
    }
}

fn assert_error(text: &str, line: u32, column: u32, needle: &str) {
    let (message, at_line, at_column) = error(text);
    assert!(message.contains(needle), "{message:?} should mention {needle:?}");
    assert_eq!((at_line, at_column), (line, column), "{message}");
}

const EXAMPLE: &str = r#"# Unfold settings for this repository. Commit this file to share them.
languages = ["typescript", "python"]

[scope]
include = []
exclude = ["**/generated/**"]

[[python.project]]
root = "py-playground"
source_roots = ["src"]
version = "3.12"

"#;

#[test]
fn the_documented_example_parses() {
    let settings = compile_settings(EXAMPLE);
    assert_eq!(
        settings.languages,
        Some(vec![Language::TypeScript, Language::Python])
    );
    assert_eq!(settings.scope.exclude(), ["**/generated/**".into()]);
    assert!(settings.scope.include().is_empty());

    let [project] = settings.python_projects.as_slice() else {
        panic!("one project")
    };
    assert_eq!(project.root.as_str(), "py-playground");
    let roots: Vec<&str> = project
        .source_roots
        .iter()
        .flatten()
        .map(FolderPath::as_str)
        .collect();
    assert_eq!(roots, ["py-playground/src"]);
    assert_eq!(project.version, PythonVersion::V3_12);
}

#[test]
fn an_absent_or_empty_file_means_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(SettingsFile::load(dir.path()).unwrap(), SettingsFile::default());
    assert_eq!(SettingsFile::parse("").unwrap(), SettingsFile::default());
    assert_eq!(
        SettingsFile::parse("# only a comment\n").unwrap(),
        SettingsFile::default()
    );
    let defaults = Settings::default();
    assert!(defaults.language_enabled(Language::Python));
    assert!(defaults.language_enabled(Language::TypeScript));
    assert_eq!(
        defaults.python_version_for(&RepoPath::new("a.py").unwrap()),
        PythonVersion::V3_14
    );
}

#[test]
fn load_reads_the_settings_file_and_reports_encoding_errors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(SETTINGS_PATH), "languages = [\"python\"]\n").unwrap();
    let settings = SettingsFile::load(dir.path()).unwrap().settings;
    assert!(!settings.language_enabled(Language::TypeScript));
    std::fs::write(dir.path().join(SETTINGS_PATH), b"[scope]\n\xff").unwrap();
    assert!(matches!(
        SettingsFile::load(dir.path()),
        Err(SettingsError::Encoding { offset: 8 })
    ));
}

#[test]
fn unknown_keys_are_rejected_with_their_position() {
    assert_error("surprise = true\n", 1, 1, "surprise");
    assert_error("version = 1\n", 1, 1, "version");
    assert_error("languages = [\"python\"]\n[scope]\ninclde = []\n", 3, 1, "inclde");
    assert_error(
        "[[python.project]]\nroot = \".\"\nsourceRoots = [\"src\"]\n",
        3,
        1,
        "sourceRoots",
    );
    assert_error("[python]\nversion = \"3.12\"\n", 2, 1, "version");
}

#[test]
fn malformed_toml_and_wrong_types_are_rejected() {
    let (message, line, _) = error("[scope]\nlanguages = [\"python\"\n");
    assert!(!message.is_empty());
    assert!(line >= 2);
    assert_error("[scope]\ninclude = \"src\"\n", 2, 11, "sequence");
    assert_error("[scope]\nexclude = [1]\n", 2, 12, "string");
}

#[test]
fn invalid_values_are_reported_precisely() {
    assert_error("languages = []\n", 1, 13, "at least one language");
    assert_error(
        "languages = [\"python\", \"rust\"]\n",
        1,
        24,
        "unknown language `rust`",
    );
    assert_error("[scope]\nexclude = [\"src/[a\"]\n", 2, 12, "not a valid glob");
    assert_error("[scope]\ninclude = [\"/abs\"]\n", 2, 12, "relative");
    assert_error("[scope]\ninclude = [\"\"]\n", 2, 12, "empty");
}

#[test]
fn evidence_files_are_confined_patterns() {
    let settings =
        compile_settings("[evidence]\nfiles = [\"prisma/schema.prisma\", \"db/migrations/*.sql\"]\n");
    assert_eq!(
        settings.evidence.patterns(),
        ["prisma/schema.prisma".into(), "db/migrations/*.sql".into()]
    );
    assert!(
        settings
            .evidence
            .contains(&RepoPath::new("db/migrations/001.sql").unwrap())
    );
    assert!(compile_settings("[evidence]\n").evidence.patterns().is_empty());
    assert_ne!(
        settings.hash(),
        Settings::default().hash(),
        "evidence patterns are settings"
    );
    assert_error("[evidence]\nfiles = [\"../secrets.sql\"]\n", 2, 10, "`..`");
    assert_error("[evidence]\nfiles = [\"/etc/hosts\"]\n", 2, 10, "relative");
    assert_error("[evidence]\nfiles = [\"db/[a\"]\n", 2, 10, "not a valid glob");
    assert_error("[evidence]\nfiles = [\"\"]\n", 2, 10, "empty");
    assert_error("[evidence]\nfiles = \"db/*.sql\"\n", 2, 9, "sequence");
    assert_error("[evidence]\npatterns = []\n", 2, 1, "patterns");
}

#[test]
fn python_projects_are_validated() {
    assert_error(
        "[[python.project]]\nversion = \"3.15\"\n",
        2,
        11,
        "unsupported Python grammar version \"3.15\"",
    );
    assert_error("[[python.project]]\nversion = 3.12\n", 2, 11, "string");
    assert_error(
        "[[python.project]]\nsource_roots = []\n",
        2,
        16,
        "at least one folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"../outside\"]\n",
        2,
        17,
        "inside the folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"/outside\"]\n",
        2,
        17,
        "relative",
    );
    assert_error(
        "[[python.project]]\nroot = \"../up\"\n",
        2,
        8,
        "inside the folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"src\", \"./src/\"]\n",
        2,
        24,
        "duplicate Python source root `src`",
    );
    assert_error(
        "[[python.project]]\nroot = \"a\"\n[[python.project]]\nroot = \"a/\"\n",
        4,
        8,
        "two `[[python.project]]` tables",
    );
    let settings = compile_settings("[[python.project]]\nversion = \"3.14\"\nsource_roots = [\".\"]\n");
    assert_eq!(settings.python_projects[0].root, FolderPath::ROOT);
    assert_eq!(
        settings.python_projects[0].source_roots,
        Some(vec![FolderPath::ROOT])
    );
}

#[test]
fn errors_display_the_file_line_and_column() {
    let error = SettingsFile::parse("\n\n  [scope]\n  include = [\"/abs\"]\n").unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("unfold.toml:4:14: "), "{text}");
}

#[test]
fn the_settings_hash_ignores_formatting_but_not_values() {
    let base = compile_settings(EXAMPLE).hash();
    let reformatted = EXAMPLE
        .replace("# Unfold settings", "# Changed comment")
        .replace("[scope]\n", "\n\n[scope]\n");
    assert_eq!(compile_settings(&reformatted).hash(), base);
    let changed = EXAMPLE.replace("version = \"3.12\"", "version = \"3.13\"");
    assert_ne!(compile_settings(&changed).hash(), base);
    assert_ne!(Settings::default().hash(), base);
    assert_eq!(Settings::default().hash(), compile_settings("[scope]").hash());
    assert_eq!(base.to_string().len(), 64);
}

#[test]
fn python_hashes_follow_the_owning_project_only() {
    let text =
        "[[python.project]]\nroot = \"a\"\nversion = \"3.12\"\n\n[[python.project]]\nroot = \"a/inner\"\n";
    let settings = compile_settings(text);
    let path = |p: &str| RepoPath::new(p).unwrap();
    assert_eq!(
        settings
            .python_project_for(&path("a/x.py"))
            .unwrap()
            .root
            .as_str(),
        "a"
    );
    assert_eq!(
        settings
            .python_project_for(&path("a/inner/x.py"))
            .unwrap()
            .root
            .as_str(),
        "a/inner"
    );
    assert!(settings.python_project_for(&path("b/x.py")).is_none());
    assert_eq!(settings.python_version_for(&path("a/x.py")), PythonVersion::V3_12);
    assert_eq!(
        settings.python_version_for(&path("a/inner/x.py")),
        PythonVersion::V3_14
    );

    let changed = compile_settings(&text.replace("3.12", "3.13"));
    assert_ne!(
        changed.python_hash(&path("a/x.py")),
        settings.python_hash(&path("a/x.py"))
    );
    assert_eq!(
        changed.python_hash(&path("a/inner/x.py")),
        settings.python_hash(&path("a/inner/x.py"))
    );
    assert_eq!(
        changed.python_hash(&path("b/x.py")),
        Settings::default().python_hash(&path("b/x.py"))
    );
}

#[test]
fn unknown_tables_are_rejected() {
    for (text, table) in [
        ("[project]\nname = \"api\"\n", "project"),
        ("[[entrance]]\nname = \"a\"\n", "entrance"),
        ("[apps]\n", "apps"),
    ] {
        let (message, line, _) = error(text);
        assert!(message.contains(table), "{message:?} should mention {table:?}");
        assert!(
            message.contains("`app`"),
            "{message:?} should name `app` among the known keys"
        );
        assert_eq!(line, 1, "{message}");
    }
}

#[test]
fn testdata_settings_parse() {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    for folder in [
        "fixtures/python/policy/single-root",
        "fixtures/python/policy/duplicate-roots",
    ] {
        let settings =
            SettingsFile::load(&testdata.join(folder)).unwrap_or_else(|error| panic!("{folder}: {error}"));
        assert_eq!(settings.settings.python_projects.len(), 1, "{folder}");
    }
}

/// `docs/designs/unfold.example.toml`, trimmed.
const APPS: &str = r#"[scope]
exclude = ["legacy/**"]

[[app]]
name = "Shop API"
run  = "uvicorn shop.api:app --port 8000"
code = "shop/api.py#"
entrances = [
  { name = "Place an order",  on = "POST /orders/{order_id}", code = "shop/api.py#create_order" },
  { name = "Refund an order", on = "POST /orders/{order_id}/refund", code = "shop/api.py#refund_order" },
]

[[app]]
name = "Background jobs"
about = "Runs on the worker host, not the API."
entrances = [
  { name = "1. Collect incoming email", on = "every 5 minutes", code = "inbox/sync.py#sync_inbox", about = "Polls." },
]

[[app]]
name = "Orders API"
entrances = [
  { name = "Place an order",  code = 'src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]' },
  { name = "Cancel an order", code = ["src/middleware/auth.ts#requireUser", "src/routes/orders.ts#cancelOrder"] },
]

# An app can be only a start.
[[app]]
name = "Command line"
run  = "python -m shop.cli <order-id>"
code = "shop/cli.py#"
"#;

/// The text a span covers.
fn spanned(text: &str, span: ByteRange) -> &str {
    &text[span.start as usize..span.end as usize]
}

fn keys(codes: &[Key]) -> Vec<&str> {
    codes.iter().map(Key::as_str).collect()
}

#[test]
fn apps_and_entrances_read_every_field() {
    let file = SettingsFile::parse(APPS).unwrap();
    assert_eq!(file.settings.scope.exclude(), ["legacy/**".into()]);
    let names: Vec<_> = file.apps.iter().map(|app| app.name.as_deref()).collect();
    assert_eq!(
        names,
        [
            Some("Shop API"),
            Some("Background jobs"),
            Some("Orders API"),
            Some("Command line")
        ]
    );
    assert!(file.apps.iter().all(AppEntry::is_valid));
    let [shop, jobs, orders, cli] = file.apps.as_slice() else {
        panic!("four apps")
    };
    assert_eq!(shop.key.as_ref().map(Key::as_str), Some("entrance:shop-api"));
    assert_eq!(shop.run.as_deref(), Some("uvicorn shop.api:app --port 8000"));
    assert_eq!(shop.code.as_ref().map(Key::as_str), Some("shop/api.py#"));
    assert_eq!(shop.about, None);
    let refund = &shop.entrances[1];
    assert_eq!(
        refund.key.as_ref().map(Key::as_str),
        Some("entrance:shop-api/refund-an-order")
    );
    assert_eq!(refund.on.as_deref(), Some("POST /orders/{order_id}/refund"));
    assert_eq!(keys(&refund.code), ["shop/api.py#refund_order"]);

    assert_eq!(
        jobs.about.as_deref(),
        Some("Runs on the worker host, not the API.")
    );
    assert_eq!((jobs.run.as_ref(), jobs.code.as_ref()), (None, None));
    let collect = &jobs.entrances[0];
    assert_eq!(
        collect.key.as_ref().map(Key::as_str),
        Some("entrance:background-jobs/1-collect-incoming-email")
    );
    assert_eq!(collect.about.as_deref(), Some("Polls."));

    assert_eq!(
        keys(&orders.entrances[0].code),
        ["src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]"]
    );
    assert_eq!(
        keys(&orders.entrances[1].code),
        [
            "src/middleware/auth.ts#requireUser",
            "src/routes/orders.ts#cancelOrder"
        ]
    );
    assert_eq!(orders.entrances[1].on, None);
    assert!(cli.entrances.is_empty());
}

#[test]
fn every_app_and_entrance_has_its_span() {
    let file = SettingsFile::parse(APPS).unwrap();
    let shop = &file.apps[0];
    let table = spanned(APPS, shop.span);
    assert!(table.starts_with("[[app]]\nname = \"Shop API\""), "{table}");
    assert!(table.ends_with("refund_order\" },\n]"), "{table}");
    assert_eq!(
        spanned(APPS, shop.entrances[0].span),
        "{ name = \"Place an order\",  on = \"POST /orders/{order_id}\", code = \"shop/api.py#create_order\" }"
    );
    let cli = spanned(APPS, file.apps[3].span);
    assert_eq!(
        cli,
        "[[app]]\nname = \"Command line\"\nrun  = \"python -m shop.cli <order-id>\"\ncode = \"shop/cli.py#\""
    );
}

#[test]
fn a_byte_order_mark_is_skipped_and_spans_count_it() {
    let text = format!("\u{FEFF}{APPS}");
    let file = SettingsFile::parse(&text).unwrap();
    assert_eq!(file.apps.len(), 4);
    let entrance = &file.apps[0].entrances[0];
    assert!(spanned(&text, entrance.span).starts_with("{ name = \"Place an order\""));
    assert!(spanned(&text, file.apps[0].span).starts_with("[[app]]"));
    let (_, line, column) = error("\u{FEFF}[scope]\nbogus = 1\n");
    assert_eq!((line, column), (2, 1));
}

/// The one app of `text`, which must parse.
fn only_app(text: &str) -> AppEntry {
    let mut file = SettingsFile::parse(text).unwrap();
    assert_eq!(file.apps.len(), 1, "{text}");
    file.apps.remove(0)
}

fn problem_of(text: &str, entry: &Option<EntryProblem>) -> (EntryProblemKind, String) {
    let problem = entry.as_ref().expect("a problem");
    (problem.kind, spanned(text, problem.span).to_owned())
}

#[test]
fn a_bad_app_is_listed_with_its_problem_and_span() {
    use EntryProblemKind::*;
    for (text, kind, at) in [
        ("[[app]]\nname = \"A\"\nkind = \"worker\"\n", UnknownField, "kind"),
        ("[[app]]\nrun = \"go\"\n", MissingName, "[[app]]\nrun = \"go\""),
        ("[[app]]\nname = \"--\"\n", EmptyKey, "[[app]]\nname = \"--\""),
        ("[[app]]\nname = 3\n", WrongType, "3"),
        ("[[app]]\nname = \"A\"\ncode = \"\"\n", EmptyKey, "\"\""),
        (
            "[[app]]\nname = \"A\"\ncode = [\"a.py#\"]\n",
            WrongType,
            "[\"a.py#\"]",
        ),
        (
            "[[app]]\nname = \"A\"\nentrances = \"none\"\n",
            WrongType,
            "\"none\"",
        ),
    ] {
        let app = only_app(text);
        assert!(!app.is_valid(), "{text}");
        assert_eq!(problem_of(text, &app.problem), (kind, at.to_owned()), "{text}");
    }
    let app = only_app("[[app]]\nname = \"A\"\nkind = \"worker\"\n");
    let message = &app.problem.expect("a problem").message;
    assert!(message.contains("unknown field `kind`"), "{message}");
    assert_eq!(app.name.as_deref(), Some("A"), "the entry is still listed");
}

#[test]
fn a_bad_entrance_marks_only_itself() {
    use EntryProblemKind::*;
    let text = r#"[[app]]
name = "A"
entrances = [
  { name = "Fine", code = "a.py#f" },
  { name = "No code" },
  { code = "a.py#g" },
  { name = "!!", code = "a.py#h" },
  { name = "Odd", code = "a.py#i", kind = "http" },
  { name = "Empty", code = [] },
  { name = "Numbers", code = [1] },
  "text",
  { name = "fine", code = "a.py#j" },
]
"#;
    let app = only_app(text);
    assert!(app.is_valid(), "an entrance's problem is its own");
    let problems: Vec<_> = app
        .entrances
        .iter()
        .map(|entrance| entrance.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(
        problems,
        [
            None,
            Some(MissingCode),
            Some(MissingName),
            Some(EmptyKey),
            Some(UnknownField),
            Some(MissingCode),
            Some(WrongType),
            Some(WrongType),
            Some(DuplicateKey)
        ]
    );
    assert_eq!(
        problem_of(text, &app.entrances[1].problem).1,
        "{ name = \"No code\" }"
    );
    assert_eq!(problem_of(text, &app.entrances[4].problem).1, "kind");
    assert_eq!(problem_of(text, &app.entrances[5].problem).1, "[]");
    assert_eq!(problem_of(text, &app.entrances[7].problem).1, "\"text\"");
    assert_eq!(
        problem_of(text, &app.entrances[8].problem).1,
        "{ name = \"fine\", code = \"a.py#j\" }"
    );
    assert_eq!(app.entrances[2].name, None, "listed without a name");
    assert_eq!(keys(&app.entrances[4].code), ["a.py#i"], "listed as written");
}

#[test]
fn an_empty_code_key_marks_its_entry() {
    let text = r#"[[app]]
name = "A"
code = ""
entrances = [
  { name = "Blank", code = "" },
  { name = "Blank in a list", code = ["a.py#f", ""] },
  { name = "Fine", code = ["a.py#f", "a.py#g"] },
]
"#;
    let app = only_app(text);
    let (kind, at) = problem_of(text, &app.problem);
    assert_eq!((kind, at.as_str()), (EntryProblemKind::EmptyKey, "\"\""));
    let message = &app.problem.as_ref().expect("a problem").message;
    assert!(message.contains("empty key"), "{message}");
    assert_eq!(app.code.as_ref().map(Key::as_str), Some(""), "listed as written");
    let problems: Vec<_> = app
        .entrances
        .iter()
        .map(|entrance| entrance.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(
        problems,
        [
            Some(EntryProblemKind::EmptyKey),
            Some(EntryProblemKind::EmptyKey),
            None
        ]
    );
    let blank_in_a_list = &app.entrances[1];
    assert_eq!(problem_of(text, &blank_in_a_list.problem).1, "\"\"");
    assert_eq!(keys(&blank_in_a_list.code), ["a.py#f", ""], "listed as written");
}

#[test]
fn duplicate_app_keys_mark_the_later_apps() {
    let text =
        "[[app]]\nname = \"Shop API\"\n\n[[app]]\nname = \"shop  api!\"\n\n[[app]]\nname = \"Other\"\n";
    let file = SettingsFile::parse(text).unwrap();
    let kinds: Vec<_> = file
        .apps
        .iter()
        .map(|app| app.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(kinds, [None, Some(EntryProblemKind::DuplicateKey), None]);
    let problem = file.apps[1].problem.as_ref().expect("a problem");
    assert!(
        problem.message.contains("entrance:shop-api"),
        "{}",
        problem.message
    );
    assert_eq!(spanned(text, problem.span), "[[app]]\nname = \"shop  api!\"");
}

#[test]
fn a_broken_entry_claims_no_key() {
    let text = r#"[[app]]
name = "Broken"
nope = 1

[[app]]
name = "broken"
entrances = [
  { name = "Go", code = "a.py#f", nope = 1 },
  { name = "go", code = "a.py#g" },
]
"#;
    let file = SettingsFile::parse(text).unwrap();
    let kinds: Vec<_> = file
        .apps
        .iter()
        .map(|app| app.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(kinds, [Some(EntryProblemKind::UnknownField), None]);
    let kinds: Vec<_> = file.apps[1]
        .entrances
        .iter()
        .map(|entrance| entrance.problem.as_ref().map(|problem| problem.kind))
        .collect();
    assert_eq!(kinds, [Some(EntryProblemKind::UnknownField), None]);
}

#[test]
fn an_app_without_a_key_gives_its_entrances_none() {
    let app = only_app("[[app]]\nname = \"...\"\nentrances = [{ name = \"Go\", code = \"a.py#\" }]\n");
    assert_eq!(
        app.problem.map(|problem| problem.kind),
        Some(EntryProblemKind::EmptyKey)
    );
    assert_eq!(app.entrances[0].key, None);
    assert!(app.entrances[0].is_valid(), "the entrance itself is fine");
}

#[test]
fn the_app_table_itself_must_be_a_list_of_tables() {
    assert_error("app = 3\n", 1, 7, "`app` must be a list");
    let file = SettingsFile::parse("app = [1]\n").unwrap();
    assert_eq!(
        file.apps[0].problem.as_ref().map(|problem| problem.kind),
        Some(EntryProblemKind::WrongType)
    );
}

#[test]
fn apps_never_change_the_settings_hash() {
    let base = SettingsFile::parse(APPS).unwrap();
    let edited = APPS
        .replace("uvicorn shop.api:app", "gunicorn shop.api:app")
        .replace("[[app]]\nname = \"Command line\"", "[[app]]\nname = \"CLI\"");
    let edited = SettingsFile::parse(&edited).unwrap();
    assert_ne!(base.apps, edited.apps);
    assert_eq!(base.settings, edited.settings);
    assert_eq!(base.settings.hash(), edited.settings.hash());
    let without = APPS[..APPS.find("[[app]]").unwrap()].to_owned();
    assert_eq!(compile_settings(&without).hash(), base.settings.hash());
}
