//! The proof of concept's launch-command cases, ported as tables.

use super::*;

/// One test package: `(manifest path, [(script name, Some(text) or None for a non-string)])`.
type Package = (&'static str, Vec<(String, Option<String>)>);

/// Packages for tests.
struct Table(Vec<Package>);

impl PackageScripts for Table {
    fn script(&self, package: &str, name: &str) -> ScriptLookup<'_> {
        let Some((_, scripts)) = self.0.iter().find(|(path, _)| *path == package) else {
            return ScriptLookup::NoPackage;
        };
        match scripts.iter().find(|(script, _)| script == name) {
            None => ScriptLookup::Missing,
            Some((_, None)) => ScriptLookup::NotString,
            Some((_, Some(text))) => ScriptLookup::Script(text),
        }
    }
}

fn package(path: &'static str, scripts: &[(&str, &str)]) -> Package {
    (
        path,
        scripts
            .iter()
            .map(|(name, text)| ((*name).to_owned(), Some((*text).to_owned())))
            .collect(),
    )
}

const ROOT: &str = "/fixture";

/// Runs `script` as `app/package.json`'s `start`.
fn run(script: &str) -> ScriptOutcome {
    let table = Table(vec![package("app/package.json", &[("start", script)])]);
    interpret_script(&table, ROOT, "app/package.json", "start")
}

fn launch(path: &str, cwd: &str) -> ScriptOutcome {
    ScriptOutcome::Launch(LaunchCommand {
        path: path.into(),
        cwd: cwd.into(),
    })
}

#[track_caller]
fn launches(script: &str, path: &str) {
    launches_from(script, path, "app");
}

#[track_caller]
fn launches_from(script: &str, path: &str, cwd: &str) {
    assert_eq!(run(script), launch(path, cwd), "{script:?}");
}

#[track_caller]
fn no_launch(script: &str) {
    assert_eq!(run(script), ScriptOutcome::NoLaunch, "{script:?}");
}

#[track_caller]
fn unsupported(script: &str, reason: &str) {
    match run(script) {
        ScriptOutcome::Unsupported(refusal) => {
            assert!(
                refusal.message().contains(reason),
                "{script:?}: {:?} should mention {reason:?}",
                refusal.message()
            );
        }
        other => panic!("{script:?} should be unsupported, got {other:?}"),
    }
}

#[test]
fn words_decode_posix_quoting_without_losing_provenance() {
    launches("node \"src/my file.js\"", "src/my file.js");
    launches("node 'src/$literal(1);&.js'", "src/$literal(1);&.js");
    launches("node src/my\\ file.js", "src/my file.js");
    launches("node \"src/a\\\"b\\\\c.js\"", "src/a\"b\\c.js");
    launches("node \"src/a\\qb.js\"", "src/a\\qb.js");
    launches("node 'src/'\"mixed\".js", "src/mixed.js");
    launches("node \"src/\\$literal.js\"", "src/$literal.js");
    launches("node src/\\*.js", "src/*.js");
    launches("node './a#b.js'", "./a#b.js");
    launches("node -- '-entry.js'", "-entry.js");
    launches("node \\\n src/app.ts", "src/app.ts");
    launches("no\\\nde src/app.ts", "src/app.ts");
    launches("FO\\\nO=value node src/app.ts", "src/app.ts");
    for script in ["node \"oops", "node 'oops", "node src/escape\\", "node \"\""] {
        unsupported(script, "");
    }
    unsupported("node \"oops", "Unterminated shell quote");
    unsupported("node src/escape\\", "Unfinished shell escape");
    unsupported("node \"\"", "Empty module path");
}

#[test]
fn assignments_differ_from_literal_command_words_and_env_arguments() {
    launches(
        "NODE_ENV=\"production\" FOO=$VALUE tsx src/main.ts",
        "src/main.ts",
    );
    launches("env \"NODE_ENV=production\" FOO=$VALUE node app.js", "app.js");
    launches("cross-env NODE_ENV=production node app.js", "app.js");
    launches("env node app.js", "app.js");
    no_launch("\"FOO=value\" node app.js");
    no_launch("'FOO'=value node app.js");
    no_launch("FO\\O=value node app.js");
    no_launch("FOO=value");
    no_launch("env FOO=value");
    unsupported("env -i node app.js", "wrapper");
    unsupported("env FOO=bar cd nested; node app.js", "builtin");
    unsupported("FOO=$(pwd) node app.js", "substitution");
}

#[test]
fn only_the_final_launch_counts_and_cd_moves_the_working_directory() {
    launches("tsc -p . && node dist/main.js", "dist/main.js");
    launches_from(
        "node prep.js; cd src && cd nested/..; tsx main.ts;",
        "main.ts",
        "app/src",
    );
    launches_from("cd ..; node main.js", "main.js", ".");
    launches_from("cd ../../outside; node ../main.js", "../main.js", "../outside");
    launches_from("cd /fixture/app/src; node main.js", "main.js", "/fixture/app/src");
    launches_from("cd /outside; node main.js", "main.js", "/outside");
    no_launch("node main.js && vite");
    no_launch("node main.js; cd src");
    no_launch(" ");
    no_launch("");
    for script in [
        "cd; node main.js",
        "cd one two && node main.js",
        "cd - && node main.js",
        "; node main.js",
        "node main.js &&",
        "node a.js;; node b.js",
    ] {
        unsupported(script, "");
    }
    unsupported("node main.js &&", "Missing step after &&");
    unsupported("; node main.js", "Empty shell step");
    unsupported("cd one two && node main.js", "cd requires one literal directory");
    unsupported("cd - && node main.js", "Unsupported cd directory");
}

#[test]
fn control_syntax_is_refused_anywhere_in_a_script() {
    for script in [
        "node a.js || node b.js",
        "node a.js | cat",
        "node a.js &",
        "(node a.js)",
        "node a.js > out",
        "node a.js < in",
        "node a.js `pwd`",
        "node a.js $(pwd)",
        "node a.js \"$(pwd)\"",
        "node a.js # comment",
        "node a.js\nnode b.js",
        "if true; then node a.js; fi",
        "{ node a.js; }",
        "node a.js\0",
        "node bad\\\0path",
        "node $'a.js'",
    ] {
        unsupported(script, "");
    }
    for script in [
        "exec node first.js && node second.js",
        "eval 'cd nested'; node app.js",
        ". ./setup.sh; node app.js",
        "source setup.sh; node app.js",
        "exit 0; node app.js",
        "'exit' 0; node app.js",
        "trap 'cd nested' DEBUG; node app.js",
        "command cd nested; node app.js",
        "pushd nested; node app.js",
    ] {
        unsupported(script, "builtin");
    }
    for script in [
        "$LAUNCHER a.js",
        "node $FILE",
        "node \"${FILE}\"",
        "node --require $PRELOAD a.js",
        "node --flag=$VALUE a.js",
        "node *.js",
        "node ~/app.js",
        "node a?.js",
        "cd $DIR; node app.js",
    ] {
        unsupported(script, "expansion");
    }
    launches("node app.js $IGNORED '*.args' --eval x", "app.js");
    launches("node 'a|b>c`d$(e).js'", "a|b>c`d$(e).js");
    unsupported("node a.js | cat", "Unsupported shell operator |");
    unsupported("if true; then node a.js; fi", "Unsupported shell control word if");
    unsupported("node a.js # comment", "Shell comments are unsupported");
    unsupported("node $'a.js'", "Non-POSIX shell quoting is unsupported");
    unsupported("node a.js\nnode b.js", "control character or newline");
    launches("'if' a.js; node b.js", "b.js");
}

#[test]
fn launchers_options_and_option_arities() {
    for command in LAUNCHERS {
        launches(&format!("{command} --boolean src/main.ts"), "src/main.ts");
    }
    for option in [
        "-r",
        "--require",
        "--import",
        "--loader",
        "--experimental-loader",
        "--env-file",
        "-C",
        "--conditions",
        "--input-type",
        "--inspect-port",
        "--title",
    ] {
        launches(&format!("node {option} value main.js"), "main.js");
        launches(&format!("tsx {option} value main.ts"), "main.ts");
        unsupported(&format!("node {option}"), "Missing value");
    }
    for option in ["--watch", "-w", "--ext", "-e", "--delay", "--signal"] {
        launches(&format!("nodemon {option} value main.js"), "main.js");
    }
    launches(
        "tsx --tsconfig config.json watch --clear-screen=false src/main.ts",
        "src/main.ts",
    );
    launches("tsx watch --tsconfig config.json src/main.ts", "src/main.ts");
    launches("tsx watch watch", "watch");
    launches(
        "node --loader=custom --inspect=9229 --flag=value src/main.js",
        "src/main.js",
    );
    launches("ts-node-esm src/main.ts", "src/main.ts");
    launches("bun src/main.ts", "src/main.ts");
    no_launch("node --test src/example.test.js");
    no_launch("node -");
    no_launch("node");
    no_launch("bun test main.ts");
    no_launch("bun build main.ts");
    no_launch("bun install");
    no_launch("deno run main.ts");
}

#[test]
fn nonlaunch_modes_and_unlisted_value_options_are_refused() {
    for script in [
        "node -e source",
        "node --eval=source",
        "node -p source",
        "node --print=source",
        "node --check main.js",
        "node -c main.js",
        "node --run start",
        "node --watch-path src main.js",
        "node --snapshot-blob blob main.js",
        "node --entry-url main.js",
        "node inspect main.js",
        "node --max_old_space_size 4096 main.js",
        "tsx --max_old_space_size 4096 main.ts",
        "node --inspect_port 9230 main.js",
        "tsx -e source",
        "tsx --print source",
        "tsx --test main.ts",
        "tsx watch --ignore src main.ts",
        "tsx watch --include src main.ts",
        "tsx watch --exclude src main.ts",
        "ts-node -P config.json main.ts",
        "ts-node --compiler typescript main.ts",
        "ts-node -O '{}' main.ts",
        "ts-node --compiler-options '{}' main.ts",
        "ts-node --cwd src main.ts",
        "ts-node --showConfig main.ts",
        "ts-node-esm --project config.json main.ts",
        "ts-node --eval source",
        "ts-node -r preload main.ts",
        "bun --cwd src main.ts",
        "bun --preload setup.ts main.ts",
        "bun --print source",
        "bun --eval=source",
        "bun --config=config.toml main.ts",
        "nodemon --exec node main.js",
        "nodemon --exec=node main.js",
        "nodemon --cwd src main.js",
        "nodemon --config nodemon.json main.js",
        "nodemon --ignore src main.js",
        "nodemon -x node main.js",
        "nodemon -d 2 main.js",
        "nodemon -s SIGTERM main.js",
        "nodemon --polling-interval 200 main.js",
        "nodemon --test main.js",
        "nodemon --require preload main.js",
        "node -rpreload main.js",
        "ts-node -Pconfig.json main.ts",
        "bun --watch run target",
    ] {
        unsupported(script, "");
    }
    unsupported("node -e source", "Unsupported node option -e");
    unsupported("node --eval=source", "Unsupported node option --eval");
    unsupported("node inspect main.js", "Node inspect mode is unsupported");
    unsupported(
        "bun --watch run target",
        "bun run options before the script reference are unsupported",
    );
    unsupported(
        "nodemon --max_old_space_size 1 main.js",
        "Unsupported nodemon option",
    );
    unsupported("nodemon -wsrc main.js", "Unsupported nodemon option -wsrc");
    unsupported("bun -pfoo main.ts", "Unsupported bun option -pfoo");
}

fn scripts_with(extra: &[(&str, &str)]) -> Vec<Package> {
    let mut scripts = vec![
        ("target", "node child.js"),
        ("pretarget", "node wrong.js"),
        ("posttarget", "node wrong.js"),
        ("start", "node start.js"),
        ("stop", "node stop.js"),
        ("restart", "node restart.js"),
        ("test", "node test.js"),
    ];
    scripts.extend_from_slice(extra);
    vec![package("app/package.json", &scripts)]
}

#[test]
fn script_references_cover_each_package_manager_form() {
    for command in [
        "npm run target",
        "npm run-script target",
        "yarn target",
        "yarn run target",
        "pnpm target",
        "pnpm run target",
        "bun run target",
        "npm run target -- --port 3000",
    ] {
        let table = Table(scripts_with(&[("caller", command)]));
        assert_eq!(
            interpret_script(&table, ROOT, "app/package.json", "caller"),
            launch("child.js", "app"),
            "{command}"
        );
    }
    for name in ["start", "stop", "restart", "test"] {
        let command = format!("npm {name}");
        let table = Table(scripts_with(&[("caller", &command)]));
        assert_eq!(
            interpret_script(&table, ROOT, "app/package.json", "caller"),
            launch(&format!("{name}.js"), "app")
        );
    }
}

#[test]
fn references_restart_in_their_own_package_and_keep_child_cd_local() {
    let table = Table(vec![
        package(
            "package.json",
            &[
                ("child", "cd nested; node child.js"),
                ("final", "cd elsewhere; npm run child"),
                ("prep", "cd elsewhere; npm run child; node caller.js"),
                ("prefix", "cd apps; npm --prefix selected run start"),
                (
                    "prefixPrep",
                    "npm --prefix apps/selected run start; node caller.js",
                ),
                ("absolute", "npm --prefix /fixture/apps/selected run start"),
                ("absoluteCwd", "cd /fixture/apps; npm --prefix selected run start"),
            ],
        ),
        package("apps/selected/package.json", &[("start", "cd src; tsx main.ts")]),
    ]);
    let run = |name: &str| interpret_script(&table, ROOT, "package.json", name);
    assert_eq!(run("final"), launch("child.js", "nested"));
    assert_eq!(run("prep"), launch("caller.js", "elsewhere"));
    assert_eq!(run("prefix"), launch("main.ts", "apps/selected/src"));
    assert_eq!(run("prefixPrep"), launch("caller.js", "."));
    assert_eq!(run("absolute"), launch("main.ts", "apps/selected/src"));
    assert_eq!(run("absoluteCwd"), launch("main.ts", "apps/selected/src"));
    let trailing = interpret_script(&table, "/fixture/", "package.json", "absolute");
    assert_eq!(trailing, launch("main.ts", "apps/selected/src"));
}

#[test]
fn references_fail_on_missing_packages_and_scripts_cycles_and_depth() {
    for script in [
        "npm run missing",
        "bun run missing",
        "yarn missing",
        "pnpm missing",
        "npm --prefix ../other run start",
        "npm run",
        "bun run",
        "npm run start",
        "npm run missing; node main.js",
        "npm run start --prefix other",
        "npm --prefix other start",
    ] {
        unsupported(script, "");
    }
    unsupported(
        "npm run missing",
        "Missing or non-string script app/package.json: missing",
    );
    unsupported(
        "npm --prefix ../other run start",
        "Package input is not selected: other/package.json",
    );
    unsupported("npm run", "Missing npm script name");
    unsupported("yarn", "Missing yarn script name");
    unsupported("npm run start --prefix other", "Script arguments require --");
    unsupported("npm --prefix other start", "npm --prefix requires run");
    unsupported("npm --verbose run start", "Unsupported npm option");
    unsupported("npm run -x", "Script reference options are unsupported");
    unsupported("npm run start", "Cyclic script reference app/package.json: start");
    no_launch("npm install");
    no_launch("npm");

    let mut scripts: Vec<(String, String)> = (0..9)
        .map(|i| {
            (
                format!("s{i}"),
                if i == 8 {
                    "node main.js".to_owned()
                } else {
                    format!("npm run s{}", i + 1)
                },
            )
        })
        .collect();
    let build = |scripts: &[(String, String)]| {
        Table(vec![(
            "package.json",
            scripts
                .iter()
                .map(|(n, t)| (n.clone(), Some(t.clone())))
                .collect(),
        )])
    };
    assert_eq!(
        interpret_script(&build(&scripts), ROOT, "package.json", "s0"),
        launch("main.js", ".")
    );
    scripts[8].1 = "npm run s9".to_owned();
    scripts.push(("s9".to_owned(), "node main.js".to_owned()));
    assert_eq!(
        interpret_script(&build(&scripts), ROOT, "package.json", "s0"),
        ScriptOutcome::Unsupported(Refusal::new("Script reference depth exceeds 8"))
    );
    let cycle = Table(vec![package(
        "package.json",
        &[("a", "npm run b"), ("b", "npm run a")],
    )]);
    assert_eq!(
        interpret_script(&cycle, ROOT, "package.json", "a"),
        ScriptOutcome::Unsupported(Refusal::new("Cyclic script reference package.json: a"))
    );
}

#[test]
fn script_lookup_uses_own_members_including_unusual_names() {
    let table = Table(vec![
        package(
            "package.json",
            &[
                ("", "node empty.js"),
                ("__proto__", "node proto.js"),
                ("constructor", "node constructor.js"),
                ("x~/y", "node unusual.js"),
            ],
        ),
        ("bad/package.json", vec![("bad".to_owned(), None)]),
    ]);
    for (name, path) in [
        ("", "empty.js"),
        ("__proto__", "proto.js"),
        ("constructor", "constructor.js"),
        ("x~/y", "unusual.js"),
    ] {
        assert_eq!(
            interpret_script(&table, ROOT, "package.json", name),
            launch(path, ".")
        );
    }
    let missing = interpret_script(&table, ROOT, "package.json", "toString");
    assert!(matches!(missing, ScriptOutcome::Unsupported(r) if r.message().contains("Missing")));
    let bad = interpret_script(&table, ROOT, "bad/package.json", "bad");
    assert!(matches!(bad, ScriptOutcome::Unsupported(r) if r.message().contains("non-string")));
    let none = interpret_script(&table, ROOT, "absent/package.json", "x");
    assert!(
        matches!(none, ScriptOutcome::Unsupported(r) if r.message() == "Package input is not selected: absent/package.json")
    );
}

#[test]
fn the_ts_orders_playground_scripts() {
    // testdata/playgrounds/ts-orders/package.json, verbatim.
    let table = Table(vec![package(
        "package.json",
        &[
            ("build", "tsc -p ."),
            ("prestart", "npm run build"),
            ("start", "node dist/server.js"),
            ("dev", "tsx watch src/server.ts"),
            ("worker", "NODE_ENV=production tsx src/worker.ts"),
            ("scheduler", "cd src && tsx scheduler.ts"),
            ("cli", "tsx src/cli.ts"),
            ("legacy:export", "node scripts/export.js"),
            ("dev:all", "npm run dev & npm run worker"),
            ("test", "node --test"),
        ],
    )]);
    let run = |name: &str| interpret_script(&table, ROOT, "package.json", name);
    assert_eq!(run("build"), ScriptOutcome::NoLaunch);
    assert_eq!(run("prestart"), ScriptOutcome::NoLaunch);
    assert_eq!(run("start"), launch("dist/server.js", "."));
    assert_eq!(run("dev"), launch("src/server.ts", "."));
    assert_eq!(run("worker"), launch("src/worker.ts", "."));
    assert_eq!(run("scheduler"), launch("scheduler.ts", "src"));
    assert_eq!(run("cli"), launch("src/cli.ts", "."));
    assert_eq!(run("legacy:export"), launch("scripts/export.js", "."));
    assert_eq!(
        run("dev:all"),
        ScriptOutcome::Unsupported(Refusal::new("Unsupported shell operator &"))
    );
    assert_eq!(run("test"), ScriptOutcome::NoLaunch);
}

#[test]
fn paths_normalize_lexically() {
    assert_eq!(normalize("a/b/../c/./d/"), "a/c/d");
    assert_eq!(normalize("a/../.."), "..");
    assert_eq!(normalize(""), ".");
    assert_eq!(normalize("/../a"), "/a");
    assert_eq!(normalize("/"), "/");
    assert_eq!(dirname("package.json"), ".");
    assert_eq!(dirname("apps/api/package.json"), "apps/api");
    assert_eq!(resolve("app", "/abs/x"), "/abs/x");
    assert_eq!(resolve(".", "src"), "src");
}

#[test]
fn script_reference_fan_out_is_bounded() {
    // Each of `s0`…`s7` runs the next ten times: 10^8 interpretations without a budget.
    let steps = |next: &str| vec![format!("npm run {next}"); 10].join(" && ");
    let mut scripts: Vec<(String, String)> = (0..8)
        .map(|level| (format!("s{level}"), steps(&format!("s{}", level + 1))))
        .collect();
    scripts.push(("s8".to_owned(), "node main.js".to_owned()));
    let borrowed: Vec<(&str, &str)> = scripts
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let table = Table(vec![package("app/package.json", &borrowed)]);
    let started = std::time::Instant::now();
    match interpret_script(&table, ROOT, "app/package.json", "s0") {
        ScriptOutcome::Unsupported(refusal) => {
            assert!(
                refusal.message().contains("Script references exceed"),
                "{refusal:?}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    // A modest fan-out still launches.
    let table = Table(vec![package(
        "app/package.json",
        &[
            ("start", "npm run a && npm run a && npm run a"),
            ("a", "node main.js"),
        ],
    )]);
    assert_eq!(
        interpret_script(&table, ROOT, "app/package.json", "start"),
        launch("main.js", "app")
    );
}
