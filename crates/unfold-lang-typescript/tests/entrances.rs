//! The TypeScript entrance detectors end to end, through the engine's `update()` exactly as the
//! app compiles a folder: the `ts-orders` playground's expected entrances, every TypeScript and
//! manual detector case under `testdata/detectors/`, `[detectors] disabled`, evidence ranges and
//! pointers, unreadable packages, and determinism.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use unfold_core::vocab::Resolution;
use unfold_core::{Graph, LineIndex};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::{Engine, EngineOptions, MemoryFactsCache, UpdateError, UpdateOutcome, UpdateRequest};
use unfold_lang_typescript::TypeScript;

/// The detector folders these tests own.
const DETECTORS: &[&str] = &[
    "bullmq.worker",
    "express.route",
    "manual",
    "node-cron.schedule",
    "package.bin",
    "package.script",
];

fn testdata() -> PathBuf {
    unfold_core::testkit::testdata_dir()
}

fn copy_folder(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read a folder") {
        let entry = entry.expect("a folder entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            std::fs::create_dir_all(&target).expect("create a folder");
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

fn engine(root: &Path, threads: usize) -> Engine {
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: threads,
        ..EngineOptions::default()
    };
    Engine::new(
        root,
        vec![Arc::new(TypeScript::new())],
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("an engine for the folder")
}

fn compile(engine: &mut Engine) -> UpdateOutcome {
    match engine.update(UpdateRequest::compile()) {
        Ok(outcome) => outcome,
        Err(UpdateError::Validation(violations)) => panic!(
            "validation failed:\n{}",
            violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Err(error) => panic!("the compile failed: {error}"),
    }
}

/// A field of the expected-entrances format, which can't hold a tab or a line break.
fn field(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Renders a graph's entrances in the `expected-entrances.txt` format (`testdata/README.md`),
/// reading evidence lines from the files under `root`. Backslashes, tabs, and line breaks in
/// triggers and texts are escaped (`\\`, `\t`, `\n`, `\r`).
fn render_entrances(root: &Path, graph: &Graph) -> String {
    let mut out = String::new();
    for entrance in &graph.entrances {
        let evidence = &entrance.evidence;
        let text = std::fs::read_to_string(root.join(evidence.path.as_str())).unwrap_or_default();
        let line = LineIndex::new(&text).line_of(evidence.range.start);
        let method = entrance
            .method
            .map_or_else(|| "-".to_owned(), |method| method.to_string());
        let trigger = entrance.trigger.as_deref().map_or_else(|| "-".to_owned(), field);
        out.push_str(&format!(
            "entrance\t{}\t{}\t{method}\t{trigger}\t{}\t{}:{line}\n",
            entrance.detector, entrance.kind, entrance.trigger_status, evidence.path
        ));
        for target in &entrance.targets {
            let what = match target.resolution {
                Resolution::Resolved => target
                    .target
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                Resolution::External => target
                    .module
                    .as_ref()
                    .map(|module| module.as_str().to_owned())
                    .unwrap_or_default(),
                Resolution::Unresolved => target.reason.map(|reason| reason.to_string()).unwrap_or_default(),
            };
            out.push_str(&format!(
                "\ttarget\t{}\t{}\t{what}\t{}\n",
                target.role,
                target.resolution,
                field(&target.text)
            ));
        }
    }
    out
}

/// The records of an `expected-entrances.txt` file: every line but comments and blank lines.
fn entrance_records(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Compiles a copy of `folder` on `threads` threads and renders its entrances.
fn entrances_of(folder: &Path, threads: usize) -> String {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(folder, temporary.path());
    let outcome = compile(&mut engine(temporary.path(), threads));
    render_entrances(temporary.path(), &outcome.graph)
}

#[test]
fn ts_orders_entrances_match_the_expected_file() {
    let folder = testdata().join("playgrounds/ts-orders");
    let actual = entrances_of(&folder, 1);
    let expected =
        std::fs::read_to_string(folder.join("expected-entrances.txt")).expect("read the expected file");
    let actual = entrance_records(&actual);
    assert_eq!(actual, entrance_records(&expected));
    let count = |prefix: &str| actual.iter().filter(|line| line.starts_with(prefix)).count();
    assert_eq!(count("entrance"), 25);
    assert_eq!(count("\ttarget"), 28);
}

/// Every case folder under the detector folders these tests own, sorted.
fn detector_cases() -> Vec<PathBuf> {
    let mut cases = Vec::new();
    for detector in DETECTORS {
        let folder = testdata().join("detectors").join(detector);
        let mut found: Vec<PathBuf> = std::fs::read_dir(&folder)
            .unwrap_or_else(|error| panic!("read {}: {error}", folder.display()))
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.is_dir())
            .collect();
        found.sort();
        cases.extend(found);
    }
    cases
}

/// Each case compiles with the engine (validation included) and renders exactly its expected
/// entrances. `UNFOLD_BLESS=1` rewrites the records, keeping each file's comment lines.
#[test]
fn detector_cases_match_their_expected_files() {
    let cases = detector_cases();
    assert!(cases.len() > 40, "the detector cases are missing");
    let bless = std::env::var_os("UNFOLD_BLESS").is_some_and(|value| value == "1");
    let results: Vec<(PathBuf, String)> = std::thread::scope(|scope| {
        let handles: Vec<_> = cases
            .chunks(cases.len().div_ceil(8))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|case| (case.clone(), entrances_of(case, 1)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a case thread"))
            .collect()
    });
    let mut failures = Vec::new();
    for (case, actual) in results {
        let path = case.join("expected-entrances.txt");
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if bless {
            let mut text: String = expected
                .lines()
                .filter(|line| line.starts_with('#'))
                .map(|line| format!("{line}\n"))
                .collect();
            text.push_str(&actual);
            std::fs::write(&path, text).expect("write the expected file");
            continue;
        }
        if entrance_records(&expected) != entrance_records(&actual) {
            failures.push(format!("{}:\n{actual}", case.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The canonical records of the given types.
fn records(graph: &Graph, kinds: &[&str]) -> Vec<String> {
    unfold_core::canonical::to_string(graph)
        .lines()
        .filter(|line| {
            kinds
                .iter()
                .any(|kind| line.starts_with(&format!("{{\"t\":\"{kind}\"")))
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn disabling_a_detector_removes_only_its_entrances() {
    let source = testdata().join("playgrounds/ts-orders");
    let baseline_folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&source, baseline_folder.path());
    let baseline = compile(&mut engine(baseline_folder.path(), 1));
    let structure = records(&baseline.graph, &["file", "block", "slot", "edge"]);
    for detectors in [
        &["package.script"][..],
        &["package.bin"],
        &["package.script", "package.bin"],
        &["express.route"],
        &["bullmq.worker"],
        &["node-cron.schedule"],
    ] {
        let folder = tempfile::tempdir().expect("a temporary directory");
        copy_folder(&source, folder.path());
        let settings = folder.path().join("unfold.toml");
        let mut text = std::fs::read_to_string(&settings).expect("read the settings");
        let list = detectors
            .iter()
            .map(|detector| format!("\"{detector}\""))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!("\n[detectors]\ndisabled = [{list}]\n"));
        std::fs::write(&settings, text).expect("write the settings");
        let outcome = compile(&mut engine(folder.path(), 1));
        let kept: Vec<String> = baseline
            .graph
            .entrances
            .iter()
            .filter(|entrance| !detectors.contains(&entrance.detector.as_str()))
            .map(|entrance| entrance.key.to_string())
            .collect();
        let actual: Vec<String> = outcome
            .graph
            .entrances
            .iter()
            .map(|entrance| entrance.key.to_string())
            .collect();
        assert_eq!(actual, kept, "{detectors:?}");
        assert!(
            kept.len() < baseline.graph.entrances.len(),
            "{detectors:?} found nothing"
        );
        assert_eq!(
            records(&outcome.graph, &["file", "block", "slot", "edge"]),
            structure,
            "{detectors:?} changed the structure"
        );
        let reads_package = outcome
            .graph
            .inputs
            .iter()
            .any(|input| input.path.as_str() == "package.json");
        assert_eq!(reads_package, detectors.len() < 2, "{detectors:?}");
    }
}

#[test]
fn entrances_are_deterministic_across_threads_and_runs() {
    let source = testdata().join("playgrounds/ts-orders");
    let renders: Vec<String> = [1, 4, 1]
        .into_iter()
        .map(|threads| entrances_of(&source, threads))
        .collect();
    assert_eq!(renders[0], renders[1]);
    assert_eq!(renders[0], renders[2]);
}

/// The source or input text of each entrance's evidence range, and its pointer, in entrance
/// order.
fn evidence_of(folder: &Path) -> Vec<(String, Option<String>)> {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(folder, temporary.path());
    let outcome = compile(&mut engine(temporary.path(), 1));
    outcome
        .graph
        .entrances
        .iter()
        .map(|entrance| {
            let evidence = &entrance.evidence;
            let bytes =
                std::fs::read(temporary.path().join(evidence.path.as_str())).expect("read the evidence file");
            let text =
                String::from_utf8(bytes[evidence.range.start as usize..evidence.range.end as usize].to_vec())
                    .expect("evidence on character boundaries");
            (text, evidence.pointer.as_deref().map(str::to_owned))
        })
        .collect()
}

#[test]
fn package_evidence_is_the_exact_member_and_its_pointer() {
    let evidence = evidence_of(&testdata().join("detectors/package.script/evidence-pointers"));
    let expected = [
        ("\"run/tilde~😀\": \"node a.js\"", "/scripts/run~1tilde~0😀"),
        ("\"dup\" :  \"node b.js\"", "/scripts/dup"),
        ("\"\": \"node a.js\"", "/scripts/"),
    ];
    let actual: Vec<(&str, Option<&str>)> = evidence
        .iter()
        .map(|(text, pointer)| (text.as_str(), pointer.as_deref()))
        .collect();
    let expected: Vec<(&str, Option<&str>)> = expected
        .iter()
        .map(|(text, pointer)| (*text, Some(*pointer)))
        .collect();
    assert_eq!(actual, expected);
    let bins = evidence_of(&testdata().join("detectors/package.bin/forms"));
    let pointers: Vec<&str> = bins
        .iter()
        .filter_map(|(_, pointer)| pointer.as_deref())
        .collect();
    assert_eq!(pointers, ["/bin", "/bin/a~1b", "/bin/plain"]);
}

#[test]
fn source_evidence_is_the_exact_registration() {
    let texts = |case: &str| -> Vec<String> {
        evidence_of(&testdata().join("detectors").join(case))
            .into_iter()
            .map(|(text, pointer)| {
                assert!(pointer.is_none());
                text
            })
            .collect()
    };
    assert_eq!(
        texts("express.route/methods-annotations-wrappers")[..2],
        ["app.get('/get', handler)", "app.post('/post', handler)"]
    );
    assert_eq!(
        texts("bullmq.worker/aliases-wrappers-literals")[0],
        "new Consumer(\"orders\", handle, { concurrency: 2 })"
    );
    assert_eq!(
        texts("node-cron.schedule/named-default-namespace")[0],
        "every(\"0 * * * *\", handle, { timezone: \"UTC\" })"
    );
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("a parent")).expect("create folders");
    std::fs::write(full, text).expect("write a file");
}

#[test]
fn packages_are_inputs_and_unreadable_ones_are_problems() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    write(root, "package.json", "{\"name\": \"x\"}\n");
    write(root, "src/app.ts", "export function main() {}\n");
    let mut engine = engine(root, 1);
    // Read and fingerprinted even without scripts.
    let first = compile(&mut engine);
    assert!(first.graph.entrances.is_empty());
    assert!(
        first
            .graph
            .inputs
            .iter()
            .any(|input| input.path.as_str() == "package.json")
    );
    assert!(
        first.complete && first.problems.is_empty(),
        "{:?}",
        first.problems
    );
    for (text, message) in [
        ("{\"scripts\": {\"a\": \"node a.js\",}}", "invalid JSON"),
        ("// comment\n{}", "invalid JSON"),
        ("[\"not an object\"]", "package.json must contain a JSON object"),
    ] {
        write(root, "package.json", text);
        let outcome = compile(&mut engine);
        assert!(!outcome.complete, "{text}");
        assert_eq!(outcome.problems.len(), 1, "{text}: {:?}", outcome.problems);
        let problem = &outcome.problems[0];
        assert_eq!(&*problem.path, "package.json");
        assert!(problem.message.starts_with(message), "{}", problem.message);
        assert!(outcome.graph.file("src/app.ts").is_some());
        assert!(
            outcome
                .graph
                .inputs
                .iter()
                .any(|input| input.path.as_str() == "package.json")
        );
    }
    // With both package detectors disabled, nothing reads it.
    write(
        root,
        "unfold.toml",
        "[detectors]\ndisabled = [\"package.script\", \"package.bin\"]\n",
    );
    let disabled = compile(&mut engine);
    assert!(
        disabled.complete && disabled.problems.is_empty(),
        "{:?}",
        disabled.problems
    );
    assert!(
        !disabled
            .graph
            .inputs
            .iter()
            .any(|input| input.path.as_str() == "package.json")
    );
}

/// The `package.json` inputs of a compiled case.
fn package_inputs(case: &str) -> Vec<String> {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&testdata().join("detectors").join(case), temporary.path());
    let outcome = compile(&mut engine(temporary.path(), 1));
    outcome
        .graph
        .inputs
        .iter()
        .filter(|input| input.path.file_name() == "package.json")
        .map(|input| input.path.to_string())
        .collect()
}

#[test]
fn excluded_packages_are_not_read_and_unmatched_ones_are() {
    assert_eq!(
        package_inputs("package.script/excluded-package"),
        ["package.json", "unused/package.json"]
    );
    assert_eq!(
        package_inputs("package.script/malformed-fields"),
        ["bad/package.json", "nameless/package.json", "package.json"]
    );
}

/// C-9: the calls beside the property locators of `manual/c9-property-locators`. A plain constant
/// object's function-valued properties resolve for calls (C-2) as they do for locators; an
/// annotated object and a reassigned member refuse calls, while their locators still name the
/// declaration as written.
#[test]
fn property_locators_and_calls_agree_where_calls_have_evidence() {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(
        &testdata().join("detectors/manual/c9-property-locators"),
        temporary.path(),
    );
    let outcome = compile(&mut engine(temporary.path(), 1));
    let file = outcome.graph.file("main.ts").expect("main.ts is compiled");
    let calls: Vec<(String, String)> = file
        .edges
        .iter()
        .filter(|edge| edge.seq == 0 && (edge.text.ends_with("arrow") || edge.text.ends_with("method")))
        .map(|edge| {
            let what = match edge.resolution {
                Resolution::Resolved => edge.target.as_ref().map(ToString::to_string).unwrap_or_default(),
                _ => edge.reason.map(|reason| reason.to_string()).unwrap_or_default(),
            };
            (edge.text.to_string(), what)
        })
        .collect();
    assert_eq!(
        calls,
        [
            (
                "api.arrow".to_owned(),
                "main.ts#variable:api/function:arrow".to_owned()
            ),
            (
                "api.method".to_owned(),
                "main.ts#variable:api/method:method".to_owned()
            ),
            ("typed.arrow".to_owned(), "unknown_receiver".to_owned()),
            ("written.arrow".to_owned(), "mutable_binding".to_owned()),
        ]
    );
}

/// A route whose mounts fan out past the bound gets one `partial` entrance instead of one per
/// branch: five routers each mounted ten times by the next would make 10^5 branches.
#[test]
fn wide_mount_fan_out_is_bounded() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    let mut source = String::from("import express from \"express\";\nconst app = express();\n");
    for router in 1..=5 {
        source.push_str(&format!("const r{router} = express.Router();\n"));
    }
    for router in 2..=5 {
        for prefix in 0..10 {
            source.push_str(&format!("r{router}.use(\"/p{prefix}\", r{});\n", router - 1));
        }
    }
    for prefix in 0..10 {
        source.push_str(&format!("app.use(\"/p{prefix}\", r5);\n"));
    }
    source.push_str("function handle() {}\nr1.get(\"/x\", handle);\nr2.get(\"/y\", handle);\n");
    write(root, "src/server.ts", &source);
    let outcome = compile(&mut engine(root, 1));
    let triggers: Vec<String> = outcome
        .graph
        .entrances
        .iter()
        .map(|entrance| {
            format!(
                "{} {}",
                entrance.trigger_status,
                entrance.trigger.as_deref().unwrap_or("-")
            )
        })
        .collect();
    // `r1.get` has 10^5 branches and `r2.get` 10^4, both past the bound.
    assert_eq!(triggers, ["partial /x", "partial /y"], "{triggers:?}");
}
