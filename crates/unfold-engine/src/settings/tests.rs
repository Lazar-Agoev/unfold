use super::*;
use crate::launch::LaunchTarget;
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
id = "import"
kind = "command"
trigger = "import"
handler = "ts-playground/src/jobs/import-orders.ts#importOrders"

[[entrance]]
id = "create-order"
kind = "http"
method = "POST"
label = ""
handler = ["src/auth.ts#requireUser", "src/routes.ts#Orders.create"]

[[entrance]]
id = "cli"
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
    let ManualTargets::Handlers(handlers) = &http.targets else {
        panic!("handlers")
    };
    let texts: Vec<&str> = handlers.iter().map(Locator::text).collect();
    assert_eq!(texts, ["src/auth.ts#requireUser", "src/routes.ts#Orders.create"]);
    assert!(matches!(
        handlers[1].selector(),
        Selector::Member { owner, member } if &**owner == "Orders" && &**member == "create"
    ));
    let ids: Vec<&str> = settings.entrances.iter().map(|entrance| &*entrance.id).collect();
    assert_eq!(ids, ["import", "create-order", "cli"]);
    let ManualTargets::Process(process) = &process.targets else {
        panic!("a process")
    };
    assert!(matches!(&process.start, Some(ProcessStart::Startup(locator)) if locator.text() == "src/cli.ts"));
    assert_eq!(process.cwd, FolderPath::ROOT);
    assert_eq!(process.command, None);
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
    assert_eq!(lines, [16, 22, 29]);
    let pointers: Vec<String> = settings.entrances.iter().map(ManualEntrance::pointer).collect();
    assert_eq!(pointers, ["/entrance/0", "/entrance/1", "/entrance/2"]);
    let http = &EXAMPLE[settings.entrances[1].span.as_usize()];
    assert!(http.ends_with("\"src/routes.ts#Orders.create\"]"), "{http:?}");
    let last = &EXAMPLE[settings.entrances[2].span.as_usize()];
    assert!(last.trim_end().ends_with("startup = \"src/cli.ts\""), "{last:?}");
}

#[test]
fn entrance_ranges_are_token_exact() {
    let text = "[[entrance]] # header comment\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\" # trailing\n\n# next\n[[entrance]]\nhandler = [\n  \"a.ts#g\",\n]\nkind = \"worker\"   \nid = \"b\"\n";
    let settings = Settings::parse(text).unwrap();
    assert_eq!(
        &text[settings.entrances[0].span.as_usize()],
        "[[entrance]] # header comment\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\""
    );
    assert_eq!(
        &text[settings.entrances[1].span.as_usize()],
        "[[entrance]]\nhandler = [\n  \"a.ts#g\",\n]\nkind = \"worker\"   \nid = \"b\""
    );
    assert_eq!(settings.entrances[1].line, 7);
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
    std::fs::write(dir.path().join(SETTINGS_PATH), "languages = [\"python\"]\n").unwrap();
    let settings = Settings::load(dir.path()).unwrap();
    assert!(!settings.language_enabled(Language::TypeScript));
    std::fs::write(dir.path().join(SETTINGS_PATH), b"[scope]\n\xff").unwrap();
    assert!(matches!(
        Settings::load(dir.path()),
        Err(SettingsError::Encoding { offset: 8 })
    ));
}

#[test]
fn a_byte_order_mark_is_skipped_but_counted_in_spans() {
    let text = "\u{FEFF}[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\n";
    let settings = Settings::parse(text).unwrap();
    assert!(text[settings.entrances[0].span.as_usize()].starts_with("[[entrance]]"));
}

#[test]
fn unknown_keys_are_rejected_with_their_position() {
    assert_error("surprise = true\n", 1, 1, "surprise");
    assert_error("version = 1\n", 1, 1, "version");
    assert_error("[project]\nname = \"api\"\n", 1, 2, "project");
    assert_error("languages = [\"python\"]\n[scope]\ninclde = []\n", 3, 1, "inclde");
    assert_error("[detectors]\nenabled = []\n", 2, 1, "enabled");
    assert_error(
        "[[python.project]]\nroot = \".\"\nsourceRoots = [\"src\"]\n",
        3,
        1,
        "sourceRoots",
    );
    assert_error("[python]\nversion = \"3.12\"\n", 2, 1, "version");
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\nextra = 0\n",
        5,
        1,
        "extra",
    );
    assert_error(
        "[[application]]\nid = \"a\"\nname = \"A\"\ntitle = \"A\"\n",
        4,
        1,
        "title",
    );
    assert_error(
        "[[entrance_override]]\nmatch = { detector = \"package.script\", path = \"package.json\", pointer = \"/scripts/start\", line = 1 }\n",
        2,
        91,
        "line",
    );
}

#[test]
fn malformed_toml_and_wrong_types_are_rejected() {
    let (message, line, _) = error("[scope]\nlanguages = [\"python\"\n");
    assert!(!message.is_empty());
    assert!(line >= 2);
    assert_error("[scope]\ninclude = \"src\"\n", 2, 11, "sequence");
    assert_error("[scope]\nexclude = [1]\n", 2, 12, "string");
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\nlabel = 1\n",
        5,
        9,
        "string",
    );
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = 5\n",
        4,
        11,
        "locator",
    );
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = [5]\n",
        4,
        12,
        "string",
    );
    assert_error("[[entrance]]\nid = \"a\"\nhandler = \"a.ts#f\"\n", 1, 1, "kind");
    assert_error(
        "[[entrance]]\nkind = \"event\"\nhandler = \"a.ts#f\"\n",
        1,
        1,
        "id",
    );
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"process\"\nenv = { PORT = 80 }\n",
        4,
        16,
        "string",
    );
    assert_error(
        "[[entrance]]\nid = \"a\"\nkind = \"process\"\ncommand = \"x\"\npython_paths = \"src\"\n",
        5,
        16,
        "sequence",
    );
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
    let entrance = |body: &str| format!("[[entrance]]\nid = \"a\"\n{body}\n");
    assert_error(
        &entrance("kind = \"alien\"\nhandler = \"a.ts#f\""),
        3,
        8,
        "unknown entrance kind `alien`",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = []"),
        4,
        11,
        "at least one locator",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#f\"\nmethod = \"GET\""),
        5,
        10,
        "only `http`",
    );
    assert_error(
        &entrance("kind = \"http\"\nhandler = \"a.ts#f\"\nmethod = \"get\""),
        5,
        10,
        "unknown HTTP method `get`",
    );
    assert_error(
        &entrance("kind = \"process\"\nhandler = \"a.ts#f\"\nstartup = \"a.ts\""),
        4,
        11,
        "not `handler`",
    );
    assert_error(
        &entrance("kind = \"process\"\ntrigger = \"x\""),
        1,
        1,
        "needs a `command`, `startup`, or `target`",
    );
    assert_error(
        &entrance("kind = \"event\"\nstartup = \"a.ts\"\nhandler = \"a.ts#f\""),
        4,
        11,
        "only `process` entrances have `startup`",
    );
    assert_error(
        &entrance("kind = \"event\"\ntrigger = \"SIGTERM\""),
        1,
        1,
        "needs a `handler`",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#f\"\ntrigger = \"\""),
        5,
        11,
        "must not be empty",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"../escape.ts#f\""),
        4,
        11,
        "invalid path",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts:0\""),
        4,
        11,
        "invalid line",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = \"a.ts#a.b.c\""),
        4,
        11,
        "invalid name selector",
    );
    assert_error(
        &entrance("kind = \"event\"\nhandler = [\"a.ts#f\", \"b.ts#\"]"),
        4,
        22,
        "invalid name selector",
    );
    assert_error(
        &entrance("kind = \"command\"\nhandler = [\"ts/main.ts#run\", \"python/src/main.py#run\"]"),
        4,
        11,
        "mix TypeScript and Python",
    );
    let settings = Settings::parse(&entrance(
        "kind = \"http\"\nmethod = \"GET\"\nhandler = [\"a.ts#f\", \"b.ts#g\"]",
    ))
    .unwrap();
    assert!(
        matches!(&settings.entrances[0].targets, ManualTargets::Handlers(locators) if locators.len() == 2)
    );
    let positions = "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\n\n[[entrance]]\nid = \"b\"\nkind = \"worker\"\n";
    assert_error(positions, 6, 1, "needs a `handler`");
}

#[test]
fn ids_are_validated_and_distinct() {
    for id in ["-a", "", "a b", "café", &"a".repeat(65)] {
        let text = format!("[[entrance]]\nid = \"{id}\"\nkind = \"event\"\nhandler = \"a.ts#f\"\n");
        assert_error(&text, 2, 6, "invalid id");
    }
    let twice = "[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\n\n[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#g\"\n";
    assert_error(twice, 7, 6, "two `[[entrance]]` tables have the id `a`");
    let applications =
        "[[application]]\nid = \"api\"\nname = \"API\"\n\n[[application]]\nid = \"api\"\nname = \"Other\"\n";
    assert_error(
        applications,
        6,
        6,
        "two `[[application]]` tables have the id `api`",
    );
    let shared = "[[application]]\nid = \"a\"\nname = \"A\"\n\n[[entrance]]\nid = \"a\"\nkind = \"event\"\nhandler = \"a.ts#f\"\n";
    assert!(
        Settings::parse(shared).is_ok(),
        "applications and entrances have their own IDs"
    );
}

#[test]
fn applications_are_validated_and_referenced() {
    let text = "[[entrance]]\nid = \"api-start\"\nkind = \"process\"\napplication = \"api\"\ncommand = \"node dist/server.js\"\n\n[[application]]\nid = \"api\"\nname = \"Orders API\"\ndescription = \"Takes orders.\\nAnd refunds.\"\n";
    let settings = Settings::parse(text).unwrap();
    assert_eq!(
        settings.applications,
        [Application {
            id: "api".into(),
            name: "Orders API".into(),
            description: Some("Takes orders.\nAnd refunds.".into()),
        }]
    );
    assert_eq!(settings.entrances[0].application.as_deref(), Some("api"));

    assert_error(
        &text.replace("application = \"api\"", "application = \"web\""),
        4,
        15,
        "no `[[application]]` has the id `web`",
    );
    assert_error("[[application]]\nid = \"a\"\n", 1, 1, "name");
    assert_error(
        "[[application]]\nid = \"a\"\nname = \" \"\n",
        3,
        8,
        "`name` must not be blank",
    );
    let long = format!("[[application]]\nid = \"a\"\nname = \"{}\"\n", "n".repeat(121));
    assert_error(&long, 3, 8, "at most 120 characters");
    let description = format!(
        "[[application]]\nid = \"a\"\nname = \"A\"\ndescription = \"{}\"\n",
        "d".repeat(2001)
    );
    assert_error(&description, 4, 15, "at most 2000 characters");
}

#[test]
fn process_fields_are_parsed() {
    let text = r#"[[entrance]]
id = "recipe-cost-start"
kind = "process"
label = "Start API"
description = "Serves recipe costs."
cwd = "./data-pipelines/"
command = "python3 -m uvicorn api:app --app-dir food-cost --port $PORT"
target = { type = "python_asgi", module = "dashboard_builder.app", attribute = "app" }
python_paths = ["data-pipelines", "data-pipelines/food-cost"]
env = { PYTHONPATH = ".", _DEBUG = "" }
schedule = "30 7 * * *"
"#;
    let settings = Settings::parse(text).unwrap();
    let entrance = &settings.entrances[0];
    assert_eq!(entrance.description.as_deref(), Some("Serves recipe costs."));
    let ManualTargets::Process(process) = &entrance.targets else {
        panic!("a process")
    };
    assert_eq!(process.cwd.as_str(), "data-pipelines");
    assert_eq!(
        process.command.as_deref(),
        Some("python3 -m uvicorn api:app --app-dir food-cost --port $PORT")
    );
    assert_eq!(
        process.start,
        Some(ProcessStart::Target(LaunchTarget::PythonAsgi {
            module: "dashboard_builder.app".into(),
            attribute: "app".into(),
            factory: false,
        }))
    );
    let paths: Vec<&str> = process
        .python_paths
        .iter()
        .flatten()
        .map(FolderPath::as_str)
        .collect();
    assert_eq!(paths, ["data-pipelines", "data-pipelines/food-cost"]);
    let env: Vec<(&str, &str)> = process
        .env
        .iter()
        .map(|(name, value)| (&**name, &**value))
        .collect();
    assert_eq!(env, [("PYTHONPATH", "."), ("_DEBUG", "")]);
    assert_eq!(process.schedule.as_deref(), Some("30 7 * * *"));
    assert!(text[entrance.span.as_usize()].ends_with("schedule = \"30 7 * * *\""));

    let target = |target: &str| {
        let text = format!("[[entrance]]\nid = \"a\"\nkind = \"process\"\ntarget = {target}\n");
        match Settings::parse(&text).unwrap().entrances[0].targets.clone() {
            ManualTargets::Process(Process {
                start: Some(ProcessStart::Target(target)),
                ..
            }) => target,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        target("{ type = \"file\", path = \"bin/run.py\" }"),
        LaunchTarget::File {
            path: RepoPath::new("bin/run.py").unwrap()
        }
    );
    assert_eq!(
        target("{ type = \"python_module\", module = \"token_refresh.run\" }"),
        LaunchTarget::PythonModule {
            module: "token_refresh.run".into()
        }
    );
    assert_eq!(
        target("{ type = \"python_callable\", module = \"app.cli\", attribute = \"main\" }"),
        LaunchTarget::PythonCallable {
            module: "app.cli".into(),
            attribute: "main".into()
        }
    );
    assert_eq!(
        target("{ type = \"python_asgi\", module = \"app\", attribute = \"create\", factory = true }"),
        LaunchTarget::PythonAsgi {
            module: "app".into(),
            attribute: "create".into(),
            factory: true
        }
    );
}

#[test]
fn process_fields_are_validated() {
    let process = |body: &str| format!("[[entrance]]\nid = \"a\"\nkind = \"process\"\n{body}\n");
    assert_error(
        &process("startup = \"a.ts\"\ntarget = { type = \"file\", path = \"a.ts\" }"),
        5,
        10,
        "`startup` or `target`, not both",
    );
    assert_error(&process("command = \"  \""), 4, 11, "`command` must not be blank");
    assert_error(
        &process("command = \"x\"\ncwd = \"../up\""),
        5,
        7,
        "inside the folder",
    );
    assert_error(
        &process("command = \"x\"\npython_paths = [\"src\", \"./src\"]"),
        5,
        24,
        "duplicate Python path `src`",
    );
    assert_error(
        &process("command = \"x\"\nenv = { \"MY-VAR\" = \"1\" }"),
        5,
        9,
        "invalid environment variable name `MY-VAR`",
    );
    assert_error(&process("command = \"x\"\nenv = { 1A = \"1\" }"), 5, 9, "`1A`");
    assert_error(
        &process("command = \"x\"\nenv = { A = \"a\\u0000b\" }"),
        5,
        13,
        "NUL",
    );
    assert_error(
        &process("command = \"x\"\nschedule = \"\""),
        5,
        12,
        "`schedule` must not be blank",
    );
    let long = process(&format!("command = \"x\"\nschedule = \"{}\"", "s".repeat(201)));
    assert_error(&long, 5, 12, "at most 200 characters");
    assert_error(
        &process("target = { type = \"script\", path = \"a.py\" }"),
        4,
        19,
        "unknown target type `script`",
    );
    assert_error(
        &process("target = { type = \"file\", module = \"a\" }"),
        4,
        36,
        "a `file` target doesn't take `module`",
    );
    assert_error(
        &process("target = { type = \"python_callable\", module = \"a\" }"),
        4,
        10,
        "a `python_callable` target needs `attribute`",
    );
    assert_error(
        &process("target = { type = \"python_module\", module = \"a.b-c\" }"),
        4,
        45,
        "not a dotted Python name",
    );
    assert_error(
        &process("target = { type = \"python_asgi\", module = \"a\", attribute = \"app()\" }"),
        4,
        60,
        "not a dotted Python name",
    );
    assert_error(
        &process("target = { type = \"file\", path = \"../a.py\" }"),
        4,
        34,
        "invalid path",
    );
    assert_error(&process("target = { path = \"a.py\" }"), 4, 10, "type");
    assert_error(
        &process("command = \"x\"\ntarget = { type = \"file\", path = \"a.py\", extra = 1 }"),
        5,
        42,
        "extra",
    );
    for (key, value) in [
        ("cwd", "\".\""),
        ("command", "\"x\""),
        ("target", "{ type = \"file\", path = \"a.py\" }"),
        ("python_paths", "[]"),
        ("env", "{}"),
        ("schedule", "\"daily\""),
    ] {
        let text =
            format!("[[entrance]]\nid = \"a\"\nkind = \"worker\"\nhandler = \"a.ts#f\"\n{key} = {value}\n");
        assert_error(
            &text,
            5,
            u32::try_from(key.len()).unwrap() + 4,
            &format!("only `process` entrances have `{key}`"),
        );
    }
}

#[test]
fn overrides_match_detected_launches() {
    let text = r#"[[application]]
id = "orders-api"
name = "Orders API"

[[entrance_override]]
match = { detector = "package.script", path = "apps/api/package.json", pointer = "/scripts/start" }
application = "orders-api"
label = "Start API"
description = "Production launch."
schedule = "0 5 * * *"

[[entrance_override]]
match = { detector = "pyproject.script", path = "pyproject.toml", pointer = "/project/scripts/serve" }
"#;
    let settings = Settings::parse(text).unwrap();
    let [start, serve] = settings.overrides.as_slice() else {
        panic!("two overrides")
    };
    assert_eq!(
        start.selector,
        OverrideMatch {
            detector: DetectorId::PackageScript,
            path: RepoPath::new("apps/api/package.json").unwrap(),
            pointer: "/scripts/start".into(),
        }
    );
    assert_eq!(start.application.as_deref(), Some("orders-api"));
    assert_eq!(start.label.as_deref(), Some("Start API"));
    assert_eq!(start.description.as_deref(), Some("Production launch."));
    assert_eq!(start.schedule.as_deref(), Some("0 5 * * *"));
    assert_eq!((start.line, serve.line), (5, 12));
    assert_eq!(serve.selector.detector, DetectorId::PyprojectScript);
    assert_eq!(serve.application, None);

    let selector = |detector: &str, path: &str, pointer: &str| {
        format!(
            "[[entrance_override]]\nmatch = {{ detector = \"{detector}\", path = \"{path}\", pointer = \"{pointer}\" }}\n"
        )
    };
    assert_error(
        &selector("express.route", "a.ts", "/x"),
        2,
        22,
        "package.script, package.bin, or pyproject.script, not `express.route`",
    );
    assert_error(
        &selector("manual", "unfold.toml", "/entrance/0"),
        2,
        22,
        "not `manual`",
    );
    assert_error(
        &selector("package.bin", "../package.json", "/bin"),
        2,
        44,
        "invalid path",
    );
    assert_error(
        &selector("package.bin", "package.json", "bin"),
        2,
        70,
        "not a JSON pointer",
    );
    assert_error(
        &selector("package.bin", "package.json", "/a~2"),
        2,
        70,
        "not a JSON pointer",
    );
    let twice = format!(
        "{}\n{}",
        selector("package.bin", "package.json", "/bin"),
        selector("package.bin", "package.json", "/bin")
    );
    assert_error(
        &twice,
        5,
        9,
        "two `[[entrance_override]]` tables have the same `match`",
    );
    assert_error(
        &format!(
            "{}application = \"web\"\n",
            selector("package.bin", "package.json", "/bin")
        ),
        3,
        15,
        "no `[[application]]` has the id `web`",
    );
    assert_error("[[entrance_override]]\nlabel = \"x\"\n", 1, 1, "match");
    assert_error(
        "[[entrance_override]]\nmatch = { detector = \"package.bin\", path = \"package.json\" }\n",
        2,
        9,
        "pointer",
    );
}

#[test]
fn errors_display_the_file_line_and_column() {
    let error = Settings::parse("\n\n  [detectors]\n  disabled = [\"manual\"]\n").unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("unfold.toml:4:15: "), "{text}");
}

#[test]
fn the_settings_hash_ignores_formatting_but_not_values() {
    let base = Settings::parse(EXAMPLE).unwrap().hash();
    let reformatted = EXAMPLE
        .replace("# Unfold settings", "# Changed comment")
        .replace("[scope]\n", "\n\n[scope]\n");
    assert_eq!(Settings::parse(&reformatted).unwrap().hash(), base);
    let changed = EXAMPLE.replace("trigger = \"import\"", "trigger = \"import-all\"");
    assert_ne!(Settings::parse(&changed).unwrap().hash(), base);
    let renamed = EXAMPLE.replace("id = \"import\"", "id = \"import-orders\"");
    assert_ne!(Settings::parse(&renamed).unwrap().hash(), base);
    let scheduled = EXAMPLE.replace(
        "startup = \"src/cli.ts\"",
        "startup = \"src/cli.ts\"\nschedule = \"@daily\"",
    );
    assert_ne!(Settings::parse(&scheduled).unwrap().hash(), base);
    assert_ne!(Settings::default().hash(), base);
    assert_eq!(
        Settings::default().hash(),
        Settings::parse("[scope]").unwrap().hash()
    );
    assert_eq!(base.to_string().len(), 64);
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
    let with_entrance =
        format!("{text}\n[[entrance]]\nid = \"f\"\nkind = \"event\"\nhandler = \"a/x.py#f\"\n");
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
        Settings::load(&testdata.join(folder)).unwrap_or_else(|error| panic!("{folder}: {error}"));
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
