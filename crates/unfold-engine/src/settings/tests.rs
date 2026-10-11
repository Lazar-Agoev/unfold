use super::*;
use crate::locator::Selector;

fn error(text: &str) -> (String, u32, u32) {
    match Settings::parse(text) {
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
version = 1
languages = ["typescript", "python"]

[scope]
include = []
exclude = ["**/generated/**"]

[detectors]
disabled = ["node-cron.schedule"]

[[python.project]]
root = "py-playground"
source_roots = ["src"]
version = "3.12"

[[entrance]]
kind = "command"
trigger = "import"
handler = "ts-playground/src/jobs/import-orders.ts#importOrders"

[[entrance]]
kind = "http"
method = "POST"
label = ""
handler = ["src/auth.ts#requireUser", "src/routes.ts#Orders.create"]

[[entrance]]
kind = "process"
startup = "src/cli.ts"
"#;

#[test]
fn the_documented_example_parses() {
    let settings = Settings::parse(EXAMPLE).unwrap();
    assert_eq!(
        settings.languages,
        Some(vec![Language::TypeScript, Language::Python])
    );
    assert_eq!(settings.scope.exclude(), ["**/generated/**".into()]);
    assert!(settings.scope.include().is_empty());
    assert_eq!(settings.disabled_detectors, [DetectorId::NodeCronSchedule]);
    assert!(!settings.detector_enabled(DetectorId::NodeCronSchedule));
    assert!(settings.detector_enabled(DetectorId::ExpressRoute));
    assert!(settings.detector_enabled(DetectorId::Manual));

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

    let [import, http, process] = settings.entrances.as_slice() else {
        panic!("three entrances")
    };
    assert_eq!(import.kind, EntranceKind::Command);
    assert_eq!(import.trigger.as_deref(), Some("import"));
    assert_eq!(import.method, None);
    assert_eq!(http.method, Some(HttpMethod::Post));
    assert_eq!(http.label.as_deref(), Some(""));
    assert_eq!(http.trigger, None);
    let texts: Vec<&str> = http.targets.locators().iter().map(Locator::text).collect();
    assert_eq!(texts, ["src/auth.ts#requireUser", "src/routes.ts#Orders.create"]);
    assert!(matches!(
        http.targets.locators()[1].selector(),
        Selector::Member { owner, member } if &**owner == "Orders" && &**member == "create"
    ));
    assert!(matches!(&process.targets, ManualTargets::Startup(locator) if locator.text() == "src/cli.ts"));
}

#[test]
fn entrance_spans_start_at_their_header() {
    let settings = Settings::parse(EXAMPLE).unwrap();
    for entrance in &settings.entrances {
        let text = &EXAMPLE[entrance.span.as_usize()];
        assert!(text.starts_with("[[entrance]]"), "{text:?}");
        assert!(!text.trim_end().ends_with("[[entrance]]"), "{text:?}");
    }
    let lines: Vec<u32> = settings.entrances.iter().map(|entrance| entrance.line).collect();
    assert_eq!(lines, [17, 22, 28]);
    let pointers: Vec<String> = settings.entrances.iter().map(ManualEntrance::pointer).collect();
    assert_eq!(pointers, ["/entrance/0", "/entrance/1", "/entrance/2"]);
    let http = &EXAMPLE[settings.entrances[1].span.as_usize()];
    assert!(http.ends_with("\"src/routes.ts#Orders.create\"]"), "{http:?}");
    let last = &EXAMPLE[settings.entrances[2].span.as_usize()];
    assert!(last.trim_end().ends_with("startup = \"src/cli.ts\""), "{last:?}");
}

#[test]
fn entrance_ranges_are_token_exact() {
    let text = "[[entrance]] # header comment\nkind = \"event\"\nhandler = \"a.ts#f\" # trailing\n\n# next\n[[entrance]]\nhandler = [\n  \"a.ts#g\",\n]\nkind = \"worker\"   \n";
    let settings = Settings::parse(text).unwrap();
    assert_eq!(
        &text[settings.entrances[0].span.as_usize()],
        "[[entrance]] # header comment\nkind = \"event\"\nhandler = \"a.ts#f\""
    );
    assert_eq!(
        &text[settings.entrances[1].span.as_usize()],
        "[[entrance]]\nhandler = [\n  \"a.ts#g\",\n]\nkind = \"worker\""
    );
    assert_eq!(settings.entrances[1].line, 6);
    assert_eq!(settings.entrances[1].index, 1);
}

#[test]
fn an_absent_or_empty_file_means_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Settings::load(dir.path()).unwrap(), Settings::default());
    assert_eq!(Settings::parse("").unwrap(), Settings::default());
    assert_eq!(
        Settings::parse("# only a comment\n").unwrap(),
        Settings::default()
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
    std::fs::create_dir(dir.path().join(".unfold")).unwrap();
    std::fs::write(dir.path().join(SETTINGS_PATH), "languages = [\"python\"]\n").unwrap();
    let settings = Settings::load(dir.path()).unwrap();
    assert!(!settings.language_enabled(Language::TypeScript));
    std::fs::write(dir.path().join(SETTINGS_PATH), b"version = 1\n\xff").unwrap();
    assert!(matches!(
        Settings::load(dir.path()),
        Err(SettingsError::Encoding { offset: 12 })
    ));
}

#[test]
fn a_byte_order_mark_is_skipped_but_counted_in_spans() {
    let text = "\u{FEFF}[[entrance]]\nkind = \"event\"\nhandler = \"a.ts#f\"\n";
    let settings = Settings::parse(text).unwrap();
    assert!(text[settings.entrances[0].span.as_usize()].starts_with("[[entrance]]"));
}

#[test]
fn unknown_keys_are_rejected_with_their_position() {
    assert_error("surprise = true\n", 1, 1, "surprise");
    assert_error("version = 1\n[scope]\ninclde = []\n", 3, 1, "inclde");
    assert_error("[detectors]\nenabled = []\n", 2, 1, "enabled");
    assert_error(
        "[[python.project]]\nroot = \".\"\nsourceRoots = [\"src\"]\n",
        3,
        1,
        "sourceRoots",
    );
    assert_error("[python]\nversion = \"3.12\"\n", 2, 1, "version");
    assert_error(
        "[[entrance]]\nkind = \"event\"\nhandler = \"a.ts#f\"\nextra = 0\n",
        4,
        1,
        "extra",
    );
}

#[test]
fn malformed_toml_and_wrong_types_are_rejected() {
    let (message, line, _) = error("version = 1\nlanguages = [\"python\"\n");
    assert!(!message.is_empty());
    assert!(line >= 2);
    assert_error("version = \"1\"\n", 1, 11, "expected i64");
    assert_error("[scope]\ninclude = \"src\"\n", 2, 11, "sequence");
    assert_error("[scope]\nexclude = [1]\n", 2, 12, "string");
    assert_error(
        "[[entrance]]\nkind = \"event\"\nhandler = \"a.ts#f\"\nlabel = 1\n",
        4,
        9,
        "string",
    );
    assert_error("[[entrance]]\nkind = \"event\"\nhandler = 5\n", 3, 11, "locator");
    assert_error("[[entrance]]\nkind = \"event\"\nhandler = [5]\n", 3, 12, "string");
    assert_error("[[entrance]]\nhandler = \"a.ts#f\"\n", 1, 1, "kind");
}

#[test]
fn invalid_values_are_reported_precisely() {
    assert_error("version = 2\n", 1, 11, "unsupported settings version 2");
    assert_error("languages = []\n", 1, 13, "at least one language");
    assert_error(
        "languages = [\"python\", \"rust\"]\n",
        1,
        24,
        "unknown language `rust`",
    );
    assert_error(
        "[detectors]\ndisabled = [\"nextjs.route\"]\n",
        2,
        13,
        "unknown detector `nextjs.route`",
    );
    assert_error(
        "[detectors]\ndisabled = [\"manual\"]\n",
        2,
        13,
        "can't be disabled",
    );
    assert_error("[scope]\nexclude = [\"src/[a\"]\n", 2, 12, "not a valid glob");
    assert_error("[scope]\ninclude = [\"/abs\"]\n", 2, 12, "relative");
    assert_error("[scope]\ninclude = [\"\"]\n", 2, 12, "empty");
}

#[test]
fn evidence_files_are_confined_patterns() {
    let settings =
        Settings::parse("[evidence]\nfiles = [\"prisma/schema.prisma\", \"db/migrations/*.sql\"]\n").unwrap();
    assert_eq!(
        settings.evidence.patterns(),
        ["prisma/schema.prisma".into(), "db/migrations/*.sql".into()]
    );
    assert!(
        settings
            .evidence
            .contains(&RepoPath::new("db/migrations/001.sql").unwrap())
    );
    assert!(
        Settings::parse("[evidence]\n")
            .unwrap()
            .evidence
            .patterns()
            .is_empty()
    );
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
    let settings =
        Settings::parse("[[python.project]]\nversion = \"3.14\"\nsource_roots = [\".\"]\n").unwrap();
    assert_eq!(settings.python_projects[0].root, FolderPath::ROOT);
    assert_eq!(
        settings.python_projects[0].source_roots,
        Some(vec![FolderPath::ROOT])
    );
}

#[test]
fn entrances_are_validated() {
    let entrance = |body: &str| format!("[[entrance]]\n{body}\n");
    assert_error(
        &entrance("kind = \"alien\"\nhandler = \"a.ts#f\""),
        2,
        8,
        "unknown entrance kind `alien`",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = []"),
        3,
        11,
        "at least one locator",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#f\"\nmethod = \"GET\""),
        4,
        10,
        "only `http`",
    );
    assert_error(
        &entrance("kind = \"http\"\nhandler = \"a.ts#f\"\nmethod = \"get\""),
        4,
        10,
        "unknown HTTP method `get`",
    );
    assert_error(
        &entrance("kind = \"process\"\nhandler = \"a.ts#f\"\nstartup = \"a.ts\""),
        3,
        11,
        "takes `startup`",
    );
    assert_error(
        &entrance("kind = \"process\"\ntrigger = \"x\""),
        1,
        1,
        "needs a `startup`",
    );
    assert_error(
        &entrance("kind = \"event\"\nstartup = \"a.ts\"\nhandler = \"a.ts#f\""),
        3,
        11,
        "only `process`",
    );
    assert_error(
        &entrance("kind = \"event\"\ntrigger = \"SIGTERM\""),
        1,
        1,
        "needs a `handler`",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#f\"\ntrigger = \"\""),
        4,
        11,
        "must not be empty",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"../escape.ts#f\""),
        3,
        11,
        "invalid path",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts:0\""),
        3,
        11,
        "invalid line",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#a.b.c\""),
        3,
        11,
        "invalid name selector",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = [\"a.ts#f\", \"b.ts#\"]"),
        3,
        22,
        "invalid name selector",
    );
    assert_error(
        &entrance("kind = \"command\"\nhandler = [\"ts/main.ts#run\", \"python/src/main.py#run\"]"),
        3,
        11,
        "mix TypeScript and Python",
    );
    let settings = Settings::parse(&entrance(
        "kind = \"http\"\nmethod = \"GET\"\nhandler = [\"a.ts#f\", \"b.ts#g\"]",
    ))
    .unwrap();
    assert_eq!(settings.entrances[0].targets.locators().len(), 2);
    let positions =
        "[[entrance]]\nkind = \"event\"\nhandler = \"a.ts#f\"\n\n[[entrance]]\nkind = \"worker\"\n";
    assert_error(positions, 5, 1, "needs a `handler`");
}

#[test]
fn errors_display_the_file_line_and_column() {
    let error = Settings::parse("\n\n  [detectors]\n  disabled = [\"manual\"]\n").unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with(".unfold/config.toml:4:15: "), "{text}");
}

#[test]
fn the_settings_hash_ignores_formatting_but_not_values() {
    let base = Settings::parse(EXAMPLE).unwrap().hash();
    let reformatted = EXAMPLE
        .replace("# Unfold settings", "# Changed comment")
        .replace("version = 1\n", "\n\nversion = 1\n");
    assert_eq!(Settings::parse(&reformatted).unwrap().hash(), base);
    let changed = EXAMPLE.replace("trigger = \"import\"", "trigger = \"import-all\"");
    assert_ne!(Settings::parse(&changed).unwrap().hash(), base);
    assert_ne!(Settings::default().hash(), base);
    assert_eq!(
        Settings::default().hash(),
        Settings::parse("version = 1").unwrap().hash()
    );
    assert_eq!(base.to_string().len(), 64);
}

#[test]
fn the_project_name_is_optional_strict_and_validated() {
    let named = Settings::parse("version = 1\n\n[project]\nname = \"storefront-api\"\n").unwrap();
    assert_eq!(named.project_name.as_deref(), Some("storefront-api"));
    assert_eq!(Settings::parse("[project]\n").unwrap().project_name, None);
    assert_eq!(Settings::default().project_name, None);
    let longest = "a".repeat(MAX_PROJECT_NAME);
    let parsed = Settings::parse(&format!("[project]\nname = \"{longest}\"\n")).unwrap();
    assert_eq!(parsed.project_name, Some(longest));

    assert_error(
        "[project]\nname = \"-api\"\n",
        2,
        8,
        "invalid project name \"-api\"",
    );
    assert_error("[project]\nname = \"\"\n", 2, 8, "invalid project name");
    assert_error("[project]\nname = \"my api\"\n", 2, 8, "invalid project name");
    assert_error("[project]\nname = \"café\"\n", 2, 8, "invalid project name");
    let too_long = format!("[project]\nname = \"{}\"\n", "a".repeat(MAX_PROJECT_NAME + 1));
    assert_error(&too_long, 2, 8, "1 to 64");
    assert_error("[project]\nname = 1\n", 2, 8, "string");
    assert_error("[project]\ntitle = \"api\"\n", 2, 1, "title");
    assert_error("project = \"api\"\n", 1, 11, "struct");
}

#[test]
fn renaming_the_project_changes_no_hash() {
    let base = Settings::parse(EXAMPLE).unwrap();
    let named = Settings::parse(&format!("{EXAMPLE}\n[project]\nname = \"orders\"\n")).unwrap();
    let renamed = Settings::parse(&format!("{EXAMPLE}\n[project]\nname = \"shop\"\n")).unwrap();
    assert_ne!(named, renamed);
    assert_eq!(named.hash(), base.hash());
    assert_eq!(renamed.hash(), base.hash());
    let path = RepoPath::new("py-playground/src/app.py").unwrap();
    assert_eq!(renamed.python_hash(&path), base.python_hash(&path));
}

#[test]
fn project_names_and_defaults() {
    for valid in ["a", "storefront-api", "Shop_2.0", "9lives", &"x".repeat(64)] {
        assert!(is_project_name(valid), "{valid}");
        assert_eq!(default_project_name(valid), valid);
    }
    for invalid in [
        "",
        ".hidden",
        "-x",
        "_x",
        "my repo",
        "a/b",
        "café",
        &"x".repeat(65),
    ] {
        assert!(!is_project_name(invalid), "{invalid}");
        assert!(is_project_name(&default_project_name(invalid)), "{invalid}");
    }
    assert_eq!(default_project_name("My Repo"), "my-repo");
    assert_eq!(default_project_name("Café Crème"), "cafe-creme");
    assert_eq!(default_project_name(".dotfiles"), "dotfiles");
    assert_eq!(default_project_name("_build output_"), "build-output");
    assert_eq!(default_project_name("日本"), "project");
    assert_eq!(default_project_name(""), "project");
    assert_eq!(
        default_project_name(&format!("{} x", "a".repeat(80))),
        "a".repeat(64)
    );
    assert_eq!(
        default_project_name(&format!("{} x", "a".repeat(63))),
        "a".repeat(63)
    );
}

#[test]
fn python_hashes_follow_the_owning_project_only() {
    let text =
        "[[python.project]]\nroot = \"a\"\nversion = \"3.12\"\n\n[[python.project]]\nroot = \"a/inner\"\n";
    let settings = Settings::parse(text).unwrap();
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

    let changed = Settings::parse(&text.replace("3.12", "3.13")).unwrap();
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
    let with_entrance = format!("{text}\n[[entrance]]\nkind = \"event\"\nhandler = \"a/x.py#f\"\n");
    assert_eq!(
        Settings::parse(&with_entrance)
            .unwrap()
            .python_hash(&path("a/x.py")),
        settings.python_hash(&path("a/x.py"))
    );
}

#[test]
fn testdata_settings_parse() {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    for folder in [
        "playgrounds",
        "playgrounds/ts-orders",
        "playgrounds/py-orders",
        "fixtures/entrances",
        "fixtures/python/policy/single-root",
        "fixtures/python/policy/duplicate-roots",
    ] {
        let settings =
            Settings::load(&testdata.join(folder)).unwrap_or_else(|error| panic!("{folder}: {error}"));
        assert_eq!(settings.version, SETTINGS_VERSION, "{folder}");
    }
    let mixed = Settings::load(&testdata.join("playgrounds")).unwrap();
    assert_eq!(mixed.entrances.len(), 10);
    let roots: Vec<&str> = mixed.python_projects[0]
        .source_roots
        .iter()
        .flatten()
        .map(FolderPath::as_str)
        .collect();
    assert_eq!(roots, ["py-orders/src", "py-orders/legacy"]);
    let legacy = RepoPath::new("py-orders/legacy/orders_legacy/export.py").unwrap();
    assert_eq!(
        mixed.scope.status(&legacy),
        ScopeStatus::Excluded,
        "a source root may lie outside scope"
    );
}
