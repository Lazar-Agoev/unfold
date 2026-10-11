//! Configured launches, including every pathway-agents command the design works through.

use super::*;
use crate::launch::{ScriptLookup, ScriptOutcome, interpret_script};

/// Packages for tests: `(manifest path, [(script name, text)])`.
struct Packages(Vec<(&'static str, Vec<(&'static str, &'static str)>)>);

impl PackageScripts for Packages {
    fn script(&self, package: &str, name: &str) -> ScriptLookup<'_> {
        let Some((_, scripts)) = self.0.iter().find(|(path, _)| *path == package) else {
            return ScriptLookup::NoPackage;
        };
        match scripts.iter().find(|(script, _)| *script == name) {
            None => ScriptLookup::Missing,
            Some((_, text)) => ScriptLookup::Script(text),
        }
    }
}

const ROOT: &str = "/work/pathway-agents";

fn folder(text: &str) -> FolderPath {
    FolderPath::parse(text).unwrap()
}

/// A configured launch, with nothing set by hand unless a test sets it.
struct Draft {
    command: Option<&'static str>,
    cwd: FolderPath,
    target: Option<LaunchTarget>,
    python_paths: Option<Vec<FolderPath>>,
    env: BTreeMap<String, String>,
    packages: Packages,
}

impl Draft {
    fn new(cwd: &str, command: &'static str) -> Self {
        Self {
            command: Some(command),
            cwd: folder(cwd),
            target: None,
            python_paths: None,
            env: BTreeMap::new(),
            packages: Packages(Vec::new()),
        }
    }

    fn target(mut self, target: LaunchTarget) -> Self {
        self.target = Some(target);
        self
    }

    fn python_paths(mut self, paths: &[&str]) -> Self {
        self.python_paths = Some(paths.iter().map(|path| folder(path)).collect());
        self
    }

    fn pythonpath(mut self, value: &str) -> Self {
        self.env.insert("PYTHONPATH".to_owned(), value.to_owned());
        self
    }

    fn package(mut self, path: &'static str, scripts: &[(&'static str, &'static str)]) -> Self {
        self.packages.0.push((path, scripts.to_vec()));
        self
    }

    fn plan(&self) -> LaunchPlan {
        let draft = LaunchDraft {
            command: self.command,
            cwd: &self.cwd,
            target: self.target.as_ref(),
            python_paths: self.python_paths.as_deref(),
            env: &self.env,
        };
        plan_launch(&draft, &self.packages, ROOT)
    }
}

fn read(cwd: &str, command: &'static str) -> LaunchPlan {
    Draft::new(cwd, command).plan()
}

fn file(path: &str) -> LaunchTarget {
    LaunchTarget::File {
        path: RepoPath::new(path).unwrap(),
    }
}

fn module(name: &str) -> LaunchTarget {
    LaunchTarget::PythonModule { module: name.into() }
}

fn asgi(module: &str, attribute: &str) -> LaunchTarget {
    LaunchTarget::PythonAsgi {
        module: module.into(),
        attribute: attribute.into(),
        factory: false,
    }
}

/// The import path as `(folder, origin label)` rows, as the editor lists them.
fn paths(plan: &LaunchPlan) -> Vec<(&str, &'static str)> {
    plan.import_paths
        .iter()
        .map(|path| (path.path.as_str(), path.origin.label()))
        .collect()
}

/// The parts as `(text, meaning, irrelevant)`.
fn parts(plan: &LaunchPlan) -> Vec<(&str, &str, bool)> {
    plan.parts
        .iter()
        .map(|part| (&*part.text, &*part.meaning, part.irrelevant))
        .collect()
}

/// The problem as `(label, message)`.
fn problem(plan: &LaunchPlan) -> (&str, &str) {
    let problem = plan.problem.as_ref().expect("a problem");
    (&problem.label, &problem.message)
}

#[track_caller]
fn refused(cwd: &str, command: &'static str, reason: &str) {
    let plan = read(cwd, command);
    let (label, message) = problem(&plan);
    assert_eq!(label, "Can't read this command", "{command:?}: {message}");
    assert!(
        message.contains(reason),
        "{command:?}: {message:?} should mention {reason:?}"
    );
    assert_eq!(plan.target, None, "{command:?}");
    assert!(plan.parts.is_empty(), "{command:?}");
}

#[test]
fn uvicorn_serves_an_app_from_the_working_folder() {
    let plan = read(
        "data-pipelines",
        "uvicorn api.main:app --host 0.0.0.0 --port 8000",
    );
    assert_eq!(plan.problem, None);
    assert_eq!(plan.target, Some(asgi("api.main", "app")));
    assert!(!plan.target_set_by_hand);
    assert_eq!(paths(&plan), [("data-pipelines", "the working folder")]);
    assert_eq!(
        parts(&plan),
        [
            ("uvicorn", "Runs Uvicorn", false),
            ("api.main:app", "Serves app from the module api.main", false),
            ("--host 0.0.0.0 --port 8000", SERVER_OPTIONS, true),
        ]
    );
    assert_eq!(
        plan.python_context(),
        Some(PythonContext {
            entry: PythonEntry::Module("api.main".into()),
            roots: vec![folder("data-pipelines")],
        })
    );
}

#[test]
fn python_m_uvicorn_puts_the_app_dir_before_the_working_folder() {
    let plan = read(
        "data-pipelines",
        "python3 -m uvicorn api:app --app-dir food-cost --host 0.0.0.0 --port $PORT",
    );
    assert_eq!(plan.problem, None);
    assert_eq!(plan.target, Some(asgi("api", "app")));
    assert_eq!(
        paths(&plan),
        [
            ("data-pipelines/food-cost", "from --app-dir"),
            ("data-pipelines", "the working folder, for python -m"),
        ]
    );
    assert_eq!(
        parts(&plan),
        [
            ("python3 -m uvicorn", "Runs Uvicorn through Python", false),
            ("api:app", "Serves app from the module api", false),
            (
                "--app-dir food-cost",
                "Puts food-cost first on the import path",
                false
            ),
            ("--host 0.0.0.0 --port $PORT", SERVER_OPTIONS, true),
        ]
    );
    // Without --app-dir, Uvicorn's default and `python -m` both name the working folder.
    let plan = Draft::new("data-pipelines", "python3 -m uvicorn api:app").plan();
    assert_eq!(paths(&plan), [("data-pipelines", "the working folder")]);
}

#[test]
fn a_script_runs_from_its_own_folder() {
    let plan = read("data-pipelines", "python3 review-workers/run_worker.py");
    assert_eq!(plan.problem, None);
    assert_eq!(
        plan.target,
        Some(file("data-pipelines/review-workers/run_worker.py"))
    );
    assert_eq!(paths(&plan), [("data-pipelines/review-workers", "script folder")]);
    assert_eq!(
        parts(&plan),
        [
            ("python3", "Runs Python", false),
            (
                "review-workers/run_worker.py",
                "Runs the script review-workers/run_worker.py",
                false
            ),
        ]
    );
    assert_eq!(
        plan.python_context(),
        Some(PythonContext {
            entry: PythonEntry::Script(RepoPath::new("data-pipelines/review-workers/run_worker.py").unwrap()),
            roots: vec![folder("data-pipelines/review-workers")],
        })
    );

    let plan = read(".", "python data-pipelines/demo-sync-2/sync.py");
    assert_eq!(plan.target, Some(file("data-pipelines/demo-sync-2/sync.py")));
    assert_eq!(paths(&plan), [("data-pipelines/demo-sync-2", "script folder")]);

    let plan = read(
        "data-pipelines",
        "python3 waste-costing/compute_waste_costs.py --mode worker --batch-size 50 --sleep-sec 2",
    );
    assert_eq!(
        plan.target,
        Some(file("data-pipelines/waste-costing/compute_waste_costs.py"))
    );
    assert_eq!(
        parts(&plan)[2],
        (
            "--mode worker --batch-size 50 --sleep-sec 2",
            PROGRAM_ARGUMENTS,
            true
        )
    );

    let plan = read(".", "python3 main.py");
    assert_eq!(paths(&plan), [(".", "script folder")]);
}

#[test]
fn a_module_runs_from_the_working_folder() {
    let plan = read("data-pipelines", "python3 -m token_refresh.run");
    assert_eq!(plan.problem, None);
    assert_eq!(plan.target, Some(module("token_refresh.run")));
    assert_eq!(
        paths(&plan),
        [("data-pipelines", "the working folder, for python -m")]
    );
    assert_eq!(
        parts(&plan),
        [
            ("python3", "Runs Python", false),
            ("-m token_refresh.run", "Runs the module token_refresh.run", false),
        ]
    );
    assert_eq!(
        plan.python_context().unwrap().entry,
        PythonEntry::Module("token_refresh.run".into())
    );

    let plan = read("data-pipelines", "python -m dashboard_builder.export --from-env");
    assert_eq!(plan.target, Some(module("dashboard_builder.export")));
    assert_eq!(
        parts(&plan),
        [
            ("python", "Runs Python", false),
            (
                "-m dashboard_builder.export",
                "Runs the module dashboard_builder.export",
                false
            ),
            ("--from-env", PROGRAM_ARGUMENTS, true),
        ]
    );
}

#[test]
fn sh_c_is_unreadable_but_suggests_its_quoted_command() {
    let command = "sh -c \"uvicorn dashboard_builder.app:app --host 0.0.0.0 --port ${PORT:-8010}\"";
    let plan = read("data-pipelines", command);
    assert_eq!(
        problem(&plan),
        (
            "Can't read sh -c",
            "Unfold doesn't look inside a quoted shell command, so nothing is read from it."
        )
    );
    assert_eq!(plan.target, None);
    assert!(plan.import_paths.is_empty());
    assert_eq!(
        parts(&plan),
        [(
            command,
            "Runs a quoted shell command. Unfold doesn't look inside it",
            false
        )]
    );
    let suggestion = plan.suggestion.as_ref().expect("a suggestion");
    assert_eq!(&*suggestion.quoted, "uvicorn dashboard_builder.app:app");
    assert_eq!(suggestion.plan.problem, None);
    assert_eq!(suggestion.plan.target, Some(asgi("dashboard_builder.app", "app")));
    assert_eq!(
        paths(&suggestion.plan),
        [("data-pipelines", "the working folder")]
    );
    assert_eq!(
        parts(&suggestion.plan)[2],
        ("--host 0.0.0.0 --port ${PORT:-8010}", SERVER_OPTIONS, true)
    );

    // After the editor fills the target and import path in, the launch is set by hand.
    let plan = Draft::new("data-pipelines", command)
        .target(asgi("dashboard_builder.app", "app"))
        .python_paths(&["data-pipelines"])
        .plan();
    assert_eq!(problem(&plan).0, "Can't read sh -c");
    assert!(plan.target_set_by_hand);
    assert_eq!(plan.target, Some(asgi("dashboard_builder.app", "app")));
    assert_eq!(paths(&plan), [("data-pipelines", "set by hand")]);
    assert!(plan.suggestion.is_some());

    let plan = read(".", "bash -c 'python3 -m jobs.nightly'");
    assert_eq!(problem(&plan).0, "Can't read bash -c");
    let suggestion = plan.suggestion.unwrap();
    assert_eq!(&*suggestion.quoted, "python3 -m jobs.nightly");
    assert_eq!(suggestion.plan.target, Some(module("jobs.nightly")));

    // Only a lone `sh -c` with a readable quoted command suggests one.
    for command in [
        "cd api && sh -c 'uvicorn app:app'",
        "PORT=1 sh -c 'uvicorn app:app'",
        "sh -c 'uvicorn app:app' name",
        "sh -c 'uv run app.py'",
    ] {
        let plan = read(".", command);
        assert_eq!(problem(&plan).0, "Can't read sh -c", "{command}");
        assert_eq!(plan.suggestion, None, "{command}");
    }
}

#[test]
fn pythonpath_adds_folders_after_the_launchers_own() {
    let plan = read(
        "data-pipelines",
        "PYTHONPATH=. python3 review-workers/run_worker.py",
    );
    assert_eq!(
        paths(&plan),
        [
            ("data-pipelines/review-workers", "script folder"),
            ("data-pipelines", "from PYTHONPATH"),
        ]
    );
    assert_eq!(
        parts(&plan)[0],
        ("PYTHONPATH=.", "Adds . to the import path", false)
    );

    // A folder already on the path isn't repeated.
    let plan = read(".", "PYTHONPATH=. python3 x.py");
    assert_eq!(plan.target, Some(file("x.py")));
    assert_eq!(paths(&plan), [(".", "script folder")]);

    // Entries resolve from the working folder; an empty entry is the working folder.
    let plan = read("api", "PYTHONPATH=lib::../shared python3 -m app");
    assert_eq!(
        paths(&plan),
        [
            ("api", "the working folder, for python -m"),
            ("api/lib", "from PYTHONPATH"),
            ("shared", "from PYTHONPATH"),
        ]
    );

    // The declared table counts unless the command assigns its own.
    let declared = Draft::new("api", "python3 -m app").pythonpath("src:../shared");
    assert_eq!(
        paths(&declared.plan()),
        [
            ("api", "the working folder, for python -m"),
            ("api/src", "from PYTHONPATH"),
            ("shared", "from PYTHONPATH"),
        ]
    );
    let assigned = Draft::new("api", "env FOO=1 PYTHONPATH=lib python3 -m app").pythonpath("src");
    let plan = assigned.plan();
    assert_eq!(
        paths(&plan),
        [
            ("api", "the working folder, for python -m"),
            ("api/lib", "from PYTHONPATH"),
        ]
    );
    assert_eq!(
        parts(&plan)[..2],
        [
            ("env FOO=1", ENVIRONMENT, true),
            ("PYTHONPATH=lib", "Adds lib to the import path", false),
        ]
    );
    let emptied = Draft::new("api", "PYTHONPATH= python3 -m app").pythonpath("src");
    assert_eq!(
        paths(&emptied.plan()),
        [("api", "the working folder, for python -m")]
    );

    refused(
        ".",
        "PYTHONPATH=$HOME/lib python3 -m app",
        "Shell expansion in PYTHONPATH",
    );
    refused(".", "PYTHONPATH=../outside python3 -m app", "outside the project");
    let plan = Draft::new(".", "python3 -m app").pythonpath("$SRC").plan();
    assert_eq!(problem(&plan).1, "Shell expansion in declared PYTHONPATH");
}

#[test]
fn node_launches_and_package_scripts_share_the_package_grammar() {
    let draft = Draft::new(".", "npm run build && node dist/server.js")
        .package("package.json", &[("build", "tsc -p .")]);
    let plan = draft.plan();
    assert_eq!(plan.problem, None);
    assert_eq!(plan.target, Some(file("dist/server.js")));
    assert!(plan.import_paths.is_empty());
    assert_eq!(plan.python_context(), None);
    assert_eq!(
        parts(&plan),
        [
            ("npm run build", RUNS_BEFORE, true),
            ("node", "Runs Node", false),
            ("dist/server.js", "Runs the file dist/server.js", false),
        ]
    );

    let plan = read(
        "apps/web",
        "NODE_ENV=production tsx --tsconfig t.json src/main.ts --port 3000",
    );
    assert_eq!(plan.target, Some(file("apps/web/src/main.ts")));
    assert_eq!(
        parts(&plan),
        [
            ("NODE_ENV=production", ENVIRONMENT, true),
            ("tsx --tsconfig t.json", "Runs tsx", false),
            ("src/main.ts", "Runs the file src/main.ts", false),
            ("--port 3000", PROGRAM_ARGUMENTS, true),
        ]
    );

    // A configured package-manager command selects the working folder's package.json.
    let draft = Draft::new("apps/api", "npm run start")
        .package("apps/api/package.json", &[("start", "cd dist && node main.js")]);
    let plan = draft.plan();
    assert_eq!(plan.target, Some(file("apps/api/dist/main.js")));
    assert_eq!(
        parts(&plan),
        [(
            "npm run start",
            "Runs the package script start, which starts main.js",
            false
        )]
    );
    refused(
        "apps/api",
        "npm run start",
        "Package input is not selected: apps/api/package.json",
    );

    // Package scripts keep their own grammar: Python isn't a package-script launcher.
    let packages = Packages(vec![("package.json", vec![("serve", "python3 app.py")])]);
    assert_eq!(
        interpret_script(&packages, ROOT, "package.json", "serve"),
        ScriptOutcome::NoLaunch
    );
}

#[test]
fn only_the_last_step_launches() {
    for command in [
        "node a.js && node b.js",
        "node a.js && vite",
        "python3 manage.py migrate && uvicorn app:app",
        "uvicorn app:app; python3 worker.py",
    ] {
        let plan = read(".", command);
        assert_eq!(problem(&plan).0, "More than one launch", "{command}");
        assert_eq!(plan.target, None, "{command}");
    }
    assert_eq!(
        problem(&read(".", "node a.js && node b.js")).1,
        "Only the last step may start code, but node a.js starts code before it."
    );
    let draft =
        Draft::new(".", "npm run serve && node b.js").package("package.json", &[("serve", "node a.js")]);
    assert_eq!(problem(&draft.plan()).0, "More than one launch");

    // Preparatory steps that start nothing are fine, and `cd` moves the working folder.
    let plan = read(
        "data-pipelines",
        "pip install -r requirements.txt && cd food-cost && python3 -m uvicorn api:app",
    );
    assert_eq!(plan.problem, None);
    assert_eq!(paths(&plan), [("data-pipelines/food-cost", "the working folder")]);
    assert_eq!(
        parts(&plan)[..2],
        [
            ("pip install -r requirements.txt", RUNS_BEFORE, true),
            ("cd food-cost", "Moves into food-cost before the launch", false),
        ]
    );
    let plan = read(".", "cd /work/pathway-agents/jobs && python3 -m nightly");
    assert_eq!(paths(&plan), [("jobs", "the working folder, for python -m")]);
    refused(".", "cd .. && python3 -m nightly", "outside the project");
}

#[test]
fn commands_that_start_nothing_or_something_unknown() {
    for command in ["node --test", "cd src", "FOO=1", "bun test"] {
        assert_eq!(problem(&read(".", command)).0, "Starts no code", "{command}");
    }
    let draft = Draft::new(".", "npm run build").package("package.json", &[("build", "tsc -p .")]);
    assert_eq!(problem(&draft.plan()).0, "Starts no code");

    let plan = read("data-pipelines", "uv run python review-workers/run_worker.py");
    assert_eq!(
        problem(&plan),
        (
            "Can't read uv",
            "Unfold doesn't know what uv starts. Set the target by hand to say what it runs."
        )
    );
    for (command, label) in [
        ("python2 app.py", "Can't read python2"),
        ("python3.x app.py", "Can't read python3.x"),
        ("gunicorn app:app", "Can't read gunicorn"),
        ("sh start.sh", "Can't read sh"),
    ] {
        assert_eq!(problem(&read(".", command)).0, label, "{command}");
    }
    for command in [None, Some(""), Some("  ")] {
        let mut draft = Draft::new(".", "");
        draft.command = command;
        let plan = draft.plan();
        assert_eq!(
            problem(&plan),
            (
                "Nothing to start",
                "Write the command that starts this process, or set its target by hand."
            )
        );
        assert_eq!(plan.suggestion, None);
    }
}

#[test]
fn python_options_are_pinned() {
    let plan = read(".", "python3.12 -u -B -O -OO app.py");
    assert_eq!(plan.target, Some(file("app.py")));
    assert_eq!(parts(&plan)[0], ("python3.12 -u -B -O -OO", "Runs Python", false));
    let plan = read(".", "python3 -u -m jobs.run --since $SINCE \"*.csv\"");
    assert_eq!(plan.target, Some(module("jobs.run")));
    assert_eq!(
        parts(&plan)[2],
        ("--since $SINCE \"*.csv\"", PROGRAM_ARGUMENTS, true)
    );

    refused(".", "python3 -X dev app.py", "Unsupported python option -X");
    refused(".", "python3 -I app.py", "Unsupported python option -I");
    refused(".", "python3 -E -m app", "Unsupported python option -E");
    refused(".", "python3 -uB app.py", "Unsupported python option -uB");
    refused(".", "python3 -c 'print(1)'", "Unsupported python option -c");
    refused(".", "python3 -mapp", "Unsupported python option -mapp");
    refused(".", "python3 - < app.py", "Unsupported shell operator <");
    refused(".", "python3", "python3 needs a script or -m module");
    refused(".", "python3 -u", "python3 needs a script or -m module");
    refused(".", "python3 -m", "Missing module for -m");
    refused(".", "python3 -m 1jobs", "Unsupported module name 1jobs");
    refused(".", "python3 -m jobs..run", "Unsupported module name jobs..run");
    refused(".", "python3 ''", "Empty script path");
    refused(".", "python3 $SCRIPT", "Shell expansion");
    refused(".", "python3 -m $MODULE", "Shell expansion");
    refused(".", "python3 ../outside.py", "outside the project");
    refused(".", "python3 /elsewhere/app.py", "outside the project");
    let plan = read(".", "python3 /work/pathway-agents/jobs/app.py");
    assert_eq!(plan.target, Some(file("jobs/app.py")));
}

#[test]
fn uvicorn_options_are_pinned() {
    let plan = read(
        ".",
        "uvicorn --reload --proxy-headers --workers 4 --log-level info --forwarded-allow-ips '*' \
         --root-path /api --timeout-keep-alive 5 --host=$HOST --port=${PORT:-8010} app.main:create \
         --factory --app-dir=src",
    );
    assert_eq!(plan.problem, None);
    assert_eq!(
        plan.target,
        Some(LaunchTarget::PythonAsgi {
            module: "app.main".into(),
            attribute: "create".into(),
            factory: true,
        })
    );
    assert_eq!(paths(&plan), [("src", "from --app-dir")]);
    assert_eq!(
        parts(&plan),
        [
            ("uvicorn", "Runs Uvicorn", false),
            (
                "--reload --proxy-headers --workers 4 --log-level info --forwarded-allow-ips '*' \
                 --root-path /api --timeout-keep-alive 5 --host=$HOST --port=${PORT:-8010}",
                SERVER_OPTIONS,
                true
            ),
            ("app.main:create", "Serves create from the module app.main", false),
            (
                "--factory",
                "Treats the application as a factory that builds it",
                false
            ),
            ("--app-dir=src", "Puts src first on the import path", false),
        ]
    );

    refused(
        ".",
        "uvicorn app:app --bogus 1",
        "Unsupported uvicorn option --bogus",
    );
    refused(
        ".",
        "uvicorn app:app --reload=yes",
        "Unsupported uvicorn option --reload",
    );
    refused(
        ".",
        "uvicorn app:app --factory=1",
        "Unsupported uvicorn option --factory",
    );
    refused(".", "uvicorn app:app -- other", "Unsupported uvicorn option --");
    refused(".", "uvicorn app:app --port", "Missing value for --port");
    refused(".", "uvicorn", "Missing Uvicorn application");
    refused(".", "uvicorn --port 8000", "Missing Uvicorn application");
    refused(".", "uvicorn app", "Unsupported Uvicorn application app");
    refused(
        ".",
        "uvicorn app:app.inner",
        "Unsupported Uvicorn application app:app.inner",
    );
    refused(".", "uvicorn a:app b:app", "Uvicorn takes one application");
    refused(".", "uvicorn app:$APP", "Shell expansion");
    refused(".", "uvicorn app:app --app-dir $DIR", "Shell expansion");
    refused(
        ".",
        "uvicorn app:app --app-dir ../elsewhere",
        "outside the project",
    );
}

#[test]
fn a_target_set_by_hand_must_agree_with_a_readable_command() {
    let agreeing =
        Draft::new("data-pipelines", "uvicorn api.main:app --port 8000").target(asgi("api.main", "app"));
    let plan = agreeing.plan();
    assert_eq!(plan.problem, None);
    assert!(plan.target_set_by_hand);
    assert_eq!(plan.target, Some(asgi("api.main", "app")));
    assert_eq!(paths(&plan), [("data-pipelines", "the working folder")]);

    let disagreeing = Draft::new("data-pipelines", "python3 -m uvicorn api:app --app-dir food-cost")
        .target(asgi("api.main", "app"));
    let plan = disagreeing.plan();
    assert_eq!(
        problem(&plan),
        (
            "Command and target disagree",
            "The command starts api:app, but the target says api.main:app."
        )
    );
    assert!(plan.target_set_by_hand);
    assert_eq!(plan.target, None);
    assert!(plan.import_paths.is_empty());
    assert_eq!(plan.python_context(), None);
    assert_eq!(parts(&plan).len(), 3);

    let factory = Draft::new(".", "uvicorn app:create --factory").target(asgi("app", "create"));
    assert_eq!(
        problem(&factory.plan()).1,
        "The command starts app:create as a factory, but the target says app:create."
    );
    let script = Draft::new("jobs", "python3 run.py").target(module("jobs.run"));
    assert_eq!(
        problem(&script.plan()).1,
        "The command starts jobs/run.py, but the target says the module jobs.run."
    );
}

#[test]
fn a_target_set_by_hand_stands_in_for_an_unreadable_or_missing_command() {
    let wrapped = Draft::new("data-pipelines", "uv run python review-workers/run_worker.py")
        .target(file("data-pipelines/review-workers/run_worker.py"));
    let plan = wrapped.plan();
    assert_eq!(problem(&plan).0, "Can't read uv");
    assert_eq!(
        plan.target,
        Some(file("data-pipelines/review-workers/run_worker.py"))
    );
    assert_eq!(paths(&plan), [("data-pipelines/review-workers", "script folder")]);

    let mut targeted = Draft::new("data-pipelines", "")
        .target(module("token_refresh.run"))
        .pythonpath("shared");
    targeted.command = None;
    let plan = targeted.plan();
    assert_eq!(plan.problem, None);
    assert!(plan.parts.is_empty());
    assert_eq!(
        paths(&plan),
        [
            ("data-pipelines", "the working folder, for python -m"),
            ("data-pipelines/shared", "from PYTHONPATH"),
        ]
    );
    targeted.target = Some(LaunchTarget::PythonCallable {
        module: "cli".into(),
        attribute: "main".into(),
    });
    assert_eq!(
        paths(&targeted.plan())[0],
        ("data-pipelines", "the working folder")
    );
    targeted.target = Some(asgi("api", "app"));
    assert_eq!(
        paths(&targeted.plan())[0],
        ("data-pipelines", "the working folder")
    );

    // A file that isn't Python has no import path or Python context.
    targeted.target = Some(file("web/src/server.ts"));
    let plan = targeted.plan();
    assert!(plan.import_paths.is_empty());
    assert_eq!(plan.python_context(), None);
}

#[test]
fn python_paths_set_by_hand_replace_the_inferred_ones() {
    let draft = Draft::new(
        "data-pipelines",
        "PYTHONPATH=shared python3 -m uvicorn api:app --app-dir food-cost",
    )
    .python_paths(&[
        "data-pipelines/review-workers",
        "data-pipelines",
        "data-pipelines/review-workers",
    ]);
    let plan = draft.plan();
    assert_eq!(plan.problem, None);
    assert!(!plan.target_set_by_hand);
    assert_eq!(
        paths(&plan),
        [
            ("data-pipelines/review-workers", "set by hand"),
            ("data-pipelines", "set by hand"),
        ]
    );

    // An empty list stays empty.
    let plan = Draft::new("data-pipelines", "python3 -m token_refresh.run")
        .python_paths(&[])
        .plan();
    assert!(plan.import_paths.is_empty());
    assert_eq!(
        plan.python_context(),
        Some(PythonContext {
            entry: PythonEntry::Module("token_refresh.run".into()),
            roots: Vec::new(),
        })
    );

    // A declared PYTHONPATH that can't be read still makes the command unreadable.
    let plan = Draft::new(".", "python3 -m app")
        .pythonpath("$SRC")
        .python_paths(&["src"])
        .plan();
    assert_eq!(problem(&plan).1, "Shell expansion in declared PYTHONPATH");
}

#[test]
fn commands_outside_the_grammar_read_nothing() {
    refused(".", "node a.js | cat", "Unsupported shell operator |");
    refused(".", "exec uvicorn app:app", "Unsupported shell builtin exec");
    refused(".", "uvicorn app:app &", "Unsupported shell operator &");
    refused(".", "python3 \"oops", "Unterminated shell quote");
    refused(".", "node ../outside.js", "outside the project");
}
