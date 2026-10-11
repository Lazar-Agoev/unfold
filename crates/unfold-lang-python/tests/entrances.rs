//! The Python entrance detectors end to end: the `py-orders` playground's expected entrances,
//! every detector case under `testdata/detectors/`, `[detectors] disabled`, and determinism.

mod common;

use std::path::Path;

use common::{compile_folder_with_threads, copy_folder, entrance_records, render_entrances, testdata};

/// Compiles a copy of `folder` and renders its entrances.
fn entrances_of(folder: &Path) -> String {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(folder, temporary.path());
    let compiled = compile_folder_with_threads(temporary.path(), 1).valid();
    render_entrances(temporary.path(), &compiled.graph)
}

#[test]
fn py_orders_entrances_match_the_expected_file() {
    let folder = testdata().join("playgrounds/py-orders");
    let actual = entrances_of(&folder);
    let expected =
        std::fs::read_to_string(folder.join("expected-entrances.txt")).expect("read the expected file");
    let expected = entrance_records(&expected);
    let actual: Vec<&str> = entrance_records(&actual);
    assert_eq!(actual, expected);
    assert_eq!(
        actual.iter().filter(|line| line.starts_with("entrance")).count(),
        25
    );
    assert_eq!(
        actual.iter().filter(|line| line.starts_with("\ttarget")).count(),
        28
    );
}

/// Every case folder under `testdata/detectors/<detector>/` of a Python detector, sorted. (The
/// TypeScript and manual cases are compiled by `unfold-lang-typescript`'s tests.)
fn detector_cases() -> Vec<std::path::PathBuf> {
    let mut cases = Vec::new();
    let root = testdata().join("detectors");
    let python = |path: &Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<unfold_core::vocab::DetectorId>().ok())
            .is_some_and(|detector| detector.language() == Some(unfold_core::vocab::Language::Python))
    };
    let mut detectors: Vec<_> = std::fs::read_dir(&root)
        .expect("read testdata/detectors")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.is_dir() && python(path))
        .collect();
    detectors.sort();
    for detector in detectors {
        let mut found: Vec<_> = std::fs::read_dir(&detector)
            .expect("read a detector folder")
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
    assert!(cases.len() > 100, "the detector cases are missing");
    let bless = std::env::var_os("UNFOLD_BLESS").is_some_and(|value| value == "1");
    let results: Vec<(std::path::PathBuf, String)> = std::thread::scope(|scope| {
        let chunks: Vec<_> = cases.chunks(cases.len().div_ceil(8)).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|case| (case.clone(), entrances_of(case)))
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

/// The engine over a folder, for tests that need the update's inputs and problems.
fn engine(root: &Path) -> unfold_engine::Engine {
    let options = unfold_engine::EngineOptions {
        discover: unfold_engine::discovery::DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: 1,
        ..unfold_engine::EngineOptions::default()
    };
    unfold_engine::Engine::new(
        root,
        vec![std::sync::Arc::new(unfold_lang_python::Python)],
        std::sync::Arc::new(unfold_engine::MemoryFactsCache::new()),
        options,
    )
    .expect("an engine for the folder")
}

fn compile(engine: &mut unfold_engine::Engine) -> unfold_engine::UpdateOutcome {
    engine
        .update(unfold_engine::UpdateRequest::compile())
        .expect("the compile succeeds")
}

/// The canonical records of the given types.
fn records(graph: &unfold_core::Graph, kinds: &[&str]) -> Vec<String> {
    common::canonical_records(&unfold_core::canonical::to_string(graph), kinds)
}

#[test]
fn disabling_a_detector_removes_only_its_entrances() {
    let source = testdata().join("playgrounds/py-orders");
    let baseline_folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&source, baseline_folder.path());
    let baseline = compile(&mut engine(baseline_folder.path()));
    let structure = records(&baseline.graph, &["file", "block", "slot", "edge"]);
    for detector in [
        "pyproject.script",
        "fastapi.route",
        "celery.task",
        "apscheduler.job",
    ] {
        let folder = tempfile::tempdir().expect("a temporary directory");
        copy_folder(&source, folder.path());
        let settings = folder.path().join("unfold.toml");
        let mut text = std::fs::read_to_string(&settings).expect("read the settings");
        text.push_str(&format!("\n[detectors]\ndisabled = [\"{detector}\"]\n"));
        std::fs::write(&settings, text).expect("write the settings");
        let outcome = compile(&mut engine(folder.path()));
        let kept: Vec<String> = baseline
            .graph
            .entrances
            .iter()
            .filter(|entrance| entrance.detector.to_string() != detector)
            .map(|entrance| entrance.key.to_string())
            .collect();
        let actual: Vec<String> = outcome
            .graph
            .entrances
            .iter()
            .map(|entrance| entrance.key.to_string())
            .collect();
        assert_eq!(actual, kept, "{detector}");
        assert!(
            kept.len() < baseline.graph.entrances.len(),
            "{detector} found nothing"
        );
        assert_eq!(
            records(&outcome.graph, &["file", "block", "slot", "edge"]),
            structure,
            "{detector} changed the structure"
        );
        let reads_pyproject = outcome
            .graph
            .inputs
            .iter()
            .any(|input| input.path.as_str() == "pyproject.toml");
        assert_eq!(reads_pyproject, detector != "pyproject.script", "{detector}");
    }
}

#[test]
fn entrances_are_deterministic_across_threads_and_runs() {
    let source = testdata().join("playgrounds/py-orders");
    let mut renders = Vec::new();
    for threads in [1, 4, 1] {
        let folder = tempfile::tempdir().expect("a temporary directory");
        copy_folder(&source, folder.path());
        let compiled = compile_folder_with_threads(folder.path(), threads).valid();
        renders.push(render_entrances(folder.path(), &compiled.graph));
    }
    assert_eq!(renders[0], renders[1]);
    assert_eq!(renders[0], renders[2]);
}

/// The source or input text of each entrance's evidence range, in entrance order.
fn evidence_texts(folder: &Path) -> Vec<String> {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(folder, temporary.path());
    let compiled = compile_folder_with_threads(temporary.path(), 1).valid();
    compiled
        .graph
        .entrances
        .iter()
        .map(|entrance| {
            let evidence = &entrance.evidence;
            let bytes =
                std::fs::read(temporary.path().join(evidence.path.as_str())).expect("read the evidence file");
            String::from_utf8(bytes[evidence.range.start as usize..evidence.range.end as usize].to_vec())
                .expect("evidence on character boundaries")
        })
        .collect()
}

#[test]
fn evidence_ranges_are_the_exact_registration() {
    let detectors = testdata().join("detectors");
    assert_eq!(
        evidence_texts(&detectors.join("celery.task/aliases-reexports-forms")),
        [
            "app.task(name='unique.decorated', queue='orders')",
            "app.task(name='unique.alias', queue='emails')(alias)",
            "app.task(name='unique.keyword', queue='reports')(fun=ordinary)",
        ]
    );
    assert_eq!(
        evidence_texts(&detectors.join("apscheduler.job/aliases-keywords")),
        [
            "clock.add_job(first, Cron.from_crontab('0 2 * * *'), id='a')",
            "clock.add_job(trigger=Cron.from_crontab(timezone='UTC', expr='*/15 * * * *'), id='b', func=second)",
        ]
    );
    assert_eq!(
        evidence_texts(&detectors.join("fastapi.route/aliases-reexports-annotations")),
        ["application.head('/health')", "router.get('/x')(handler)"]
    );
    assert_eq!(
        evidence_texts(&detectors.join("pyproject.script/toml-evidence")),
        [
            "project.scripts.\"run/tilde~😀=key\" = \"app:main\"",
            "project.scripts.'second.name' = 'app:main'",
            "project.scripts.number = 1_000",
        ]
    );
    assert_eq!(
        evidence_texts(&detectors.join("pyproject.script/toml-inline")),
        ["\"inside/key\" = \"app:main\""]
    );
}

#[test]
fn console_script_pointers_escape_their_names() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(
        &testdata().join("detectors/pyproject.script/toml-evidence"),
        folder.path(),
    );
    let outcome = compile(&mut engine(folder.path()));
    let pointers: Vec<&str> = outcome
        .graph
        .entrances
        .iter()
        .filter_map(|entrance| entrance.evidence.pointer.as_deref())
        .collect();
    assert_eq!(
        pointers,
        [
            "/project/scripts/run~1tilde~0😀=key",
            "/project/scripts/second.name",
            "/project/scripts/number"
        ]
    );
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("a parent")).expect("create folders");
    std::fs::write(full, text).expect("write a file");
}

#[test]
fn owning_pyprojects_are_inputs_and_unreadable_ones_are_problems() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    write(root, "pyproject.toml", "[project]\nname = \"test\"\n");
    write(root, "src/app.py", "def main(): pass\n");
    let mut engine = engine(root);
    // Read and fingerprinted even without scripts.
    let first = compile(&mut engine);
    assert!(first.graph.entrances.is_empty());
    assert!(
        first
            .graph
            .inputs
            .iter()
            .any(|input| input.path.as_str() == "pyproject.toml")
    );
    assert!(
        first.complete && first.problems.is_empty(),
        "{:?}",
        first.problems
    );
    // Invalid TOML, a scripts value that isn't a table, and duplicate keys are problems that
    // make the graph incomplete; the rest of the graph stays.
    for (text, message) in [
        ("[broken TOML", "invalid TOML"),
        (
            "project.scripts = \"app:main\"\n",
            "project.scripts must be a TOML table",
        ),
        ("project.scripts = []\n", "project.scripts must be a TOML table"),
        (
            "[project.scripts]\nrun=\"app:main\"\nrun=\"app:main\"\n",
            "invalid TOML",
        ),
    ] {
        write(root, "pyproject.toml", text);
        let outcome = compile(&mut engine);
        assert!(!outcome.complete, "{text}");
        assert_eq!(outcome.problems.len(), 1, "{text}: {:?}", outcome.problems);
        let problem = &outcome.problems[0];
        assert_eq!(problem.kind, unfold_engine::ProblemKind::Input);
        assert_eq!(&*problem.path, "pyproject.toml");
        assert!(problem.message.starts_with(message), "{}", problem.message);
        assert!(outcome.graph.file("src/app.py").is_some());
    }
    // A disabled detector never reads it.
    write(
        root,
        "unfold.toml",
        "[detectors]\ndisabled = [\"pyproject.script\"]\n",
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
            .any(|input| input.path.as_str() == "pyproject.toml")
    );
}

#[test]
fn ambiguous_module_roots_are_listed_without_making_the_graph_incomplete() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    let root = folder.path();
    write(
        root,
        "unfold.toml",
        "[[python.project]]\nroot = \".\"\nsource_roots = [\"left\", \"right\"]\n",
    );
    write(root, "left/service.py", "def main(): pass\n");
    write(root, "right/service.py", "def main(): pass\n");
    write(root, "left/app.py", "import service\n");
    let mut engine = engine(root);
    let outcome = compile(&mut engine);
    assert!(outcome.complete);
    let messages: Vec<(String, String)> = outcome
        .problems
        .iter()
        .map(|problem| (problem.path.to_string(), problem.message.to_string()))
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].0, "left/app.py");
    assert!(messages[0].1.contains("`service` is ambiguous"), "{messages:?}");
    // An update that doesn't relink keeps listing it.
    let again = engine
        .update(unfold_engine::UpdateRequest::reopen())
        .expect("the check succeeds");
    assert!(!again.stats.relinked);
    assert_eq!(again.problems.len(), 1);
    // Removing the competing module clears it.
    std::fs::remove_file(root.join("right/service.py")).expect("remove a file");
    let fixed = compile(&mut engine);
    assert!(fixed.problems.is_empty(), "{:?}", fixed.problems);
}

/// The entrance blocks (an entrance line and its target lines) of a rendering whose entrance
/// is Python's: a Python detector's, or a manual one targeting `py-orders/`.
fn python_blocks(text: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    for line in entrance_records(text) {
        if line.starts_with("entrance") {
            blocks.push(format!("{line}\n"));
        } else if let Some(block) = blocks.last_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks.retain(|block| {
        let detector = block.split('\t').nth(1).unwrap_or_default();
        match detector {
            "manual" => block.contains("\tpy-orders/"),
            "pyproject.script" | "fastapi.route" | "celery.task" | "apscheduler.job" => true,
            _ => false,
        }
    });
    blocks
}

/// In the mixed folder (both playgrounds), Python's entrances are the PoC's, with the
/// `py-orders/` prefix: the nested project owns its `pyproject.toml` and source roots.
#[test]
fn mixed_folder_python_entrances_match_the_expected_file() {
    let folder = testdata().join("playgrounds");
    let actual = entrances_of(&folder);
    let expected =
        std::fs::read_to_string(folder.join("expected-entrances.txt")).expect("read the expected file");
    let expected = python_blocks(&expected);
    assert_eq!(python_blocks(&actual), expected);
    assert_eq!(expected.len(), 25);
}

/// Mount branches multiply level by level (`include_router` ten times at each of five levels is
/// 100,000 branches); past the cap a route gets one `partial` entrance with its local path, as a
/// router mounted nowhere does, and the compile stays quick.
#[test]
fn fastapi_mount_fan_out_is_capped_per_route() {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    let mut source =
        String::from("from fastapi import FastAPI, APIRouter\napp = FastAPI()\ndef endpoint(): pass\n");
    for level in 1..=4 {
        source.push_str(&format!("r{level} = APIRouter(prefix='/r{level}')\n"));
    }
    source.push_str("r4.get('/leaf')(endpoint)\nr4.post('/other')(endpoint)\n");
    for _ in 0..10 {
        source.push_str("app.include_router(r1, prefix='/a')\n");
        for level in 1..4 {
            source.push_str(&format!("r{level}.include_router(r{}, prefix='/m')\n", level + 1));
        }
    }
    write(temporary.path(), "app.py", &source);
    let started = std::time::Instant::now();
    let compiled = compile_folder_with_threads(temporary.path(), 1).valid();
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let triggers: Vec<(Option<&str>, unfold_core::vocab::TriggerStatus)> = compiled
        .graph
        .entrances
        .iter()
        .map(|entrance| (entrance.trigger.as_deref(), entrance.trigger_status))
        .collect();
    assert_eq!(
        triggers,
        [
            (Some("/leaf"), unfold_core::vocab::TriggerStatus::Partial),
            (Some("/other"), unfold_core::vocab::TriggerStatus::Partial),
        ]
    );
}
