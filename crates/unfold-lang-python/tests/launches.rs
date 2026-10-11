//! Launch contexts (`testdata/fixtures/python-launches/`): each Python launch's ordered import
//! path, the files it reaches, the per-file decision when launches agree or disagree, script
//! versus `-m` packages, launch targets after linking, a draft context's preview, and FastAPI
//! application bindings.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use unfold_core::vocab::{EdgeKind, Language as LanguageId, Resolution};
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::launch::{LaunchTarget, PythonContext, PythonEntry};
use unfold_engine::settings::{FolderPath, Settings};
use unfold_engine::{FactsSet, Language, LaunchResolution, Linked, Outcome, ProgramFile, ProjectInputs};
use unfold_lang_python::Python;

use common::{Compiled, compile_folder, compile_with_inputs, testdata};

fn case(name: &str) -> std::path::PathBuf {
    testdata().join("fixtures/python-launches").join(name)
}

/// Every Python file under `root`, by its path relative to `root`.
fn snapshots(root: &Path) -> BTreeMap<RepoPath, Snapshot> {
    fn walk(root: &Path, folder: &Path, out: &mut BTreeMap<RepoPath, Snapshot>) {
        for entry in std::fs::read_dir(folder).expect("read a folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|extension| extension == "py") {
                let relative = path.strip_prefix(root).expect("under the root");
                let relative = RepoPath::new(&relative.to_string_lossy()).expect("a valid path");
                let bytes = std::fs::read(&path).expect("read a file");
                out.insert(
                    relative.clone(),
                    Snapshot::new(relative, bytes).expect("a snapshot"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn context(entry: PythonEntry, roots: &[&str]) -> PythonContext {
    PythonContext {
        entry,
        roots: roots
            .iter()
            .map(|root| FolderPath::parse(root).expect("a folder"))
            .collect(),
    }
}

fn script(path: &str, roots: &[&str]) -> PythonContext {
    context(PythonEntry::Script(RepoPath::new(path).expect("a path")), roots)
}

fn module(name: &str, roots: &[&str]) -> PythonContext {
    context(PythonEntry::Module(name.into()), roots)
}

fn inputs(contexts: Vec<PythonContext>) -> ProjectInputs {
    ProjectInputs {
        settings: Settings::default(),
        python_contexts: contexts,
        ..ProjectInputs::default()
    }
}

/// Compiles a case with launch contexts (validated).
fn compile_case(name: &str, contexts: Vec<PythonContext>) -> Compiled {
    compile_with_inputs(snapshots(&case(name)), &inputs(contexts)).valid()
}

/// Links a case with `inputs` and hands the program and its answers to `answer`.
fn linked<R>(name: &str, inputs: &ProjectInputs, answer: impl FnOnce(&FactsSet, &dyn Linked) -> R) -> R {
    let files: Vec<ProgramFile> = snapshots(&case(name))
        .into_iter()
        .map(|(path, snapshot)| {
            let facts = Python
                .extract(&snapshot, &inputs.settings)
                .expect("the fixture extracts");
            ProgramFile {
                path,
                language: LanguageId::Python,
                facts: Some(Arc::new(facts)),
            }
        })
        .collect();
    let program = FactsSet::new(files).expect("distinct paths");
    let linked = Python.link(&program, inputs);
    answer(&program, linked.as_ref())
}

/// Each import edge of `file`: `text=target path`, `text=external:module`, or `text=reason`.
fn imports(compiled: &Compiled, file: &str) -> Vec<String> {
    compiled
        .file(file)
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Imports)
        .map(|edge| {
            let outcome = match edge.resolution {
                Resolution::Resolved => edge
                    .target
                    .as_ref()
                    .map(|key| key.as_str().trim_end_matches('#').to_owned())
                    .unwrap_or_default(),
                Resolution::External => format!(
                    "external:{}",
                    edge.module
                        .as_ref()
                        .map(|module| module.as_str())
                        .unwrap_or_default()
                ),
                Resolution::Unresolved => edge.reason.map(|reason| reason.to_string()).unwrap_or_default(),
            };
            format!("{}={outcome}", edge.text)
        })
        .collect()
}

/// The target key (or reason) of the call labeled `label` in `file`.
fn call(compiled: &Compiled, file: &str, label: &str) -> String {
    let edge = compiled
        .file(file)
        .edges
        .iter()
        .find(|edge| edge.kind == EdgeKind::Calls && &*edge.text == label)
        .unwrap_or_else(|| panic!("no call `{label}` in {file}"));
    match (&edge.target, edge.reason) {
        (Some(key), _) => key.to_string(),
        (None, Some(reason)) => reason.to_string(),
        (None, None) => edge.resolution.to_string(),
    }
}

/// An outcome as a key, a reason, or `external`.
fn outcome(program: &FactsSet, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Resolved(block) => program
            .block(*block)
            .map(|block| block.key.to_string())
            .unwrap_or_default(),
        Outcome::External(module) => format!("external:{}", module.as_str()),
        Outcome::Unresolved { reason, .. } => reason.to_string(),
    }
}

fn seeds(program: &FactsSet, resolution: &LaunchResolution) -> Vec<String> {
    resolution
        .seeds
        .iter()
        .map(|seed| outcome(program, &Outcome::Resolved(*seed)))
        .collect()
}

#[test]
fn a_package_comes_before_a_same_named_module() {
    // Packages before modules apply without launches too.
    let compiled = compile_case("package-beside-module", Vec::new());
    assert_eq!(imports(&compiled, "main.py"), ["tools=tools/__init__.py"]);
    assert_eq!(
        call(&compiled, "main.py", "tools.run"),
        "tools/__init__.py#function:run"
    );
    let compiled = compiled_with_script("package-beside-module", "main.py", &["."]);
    assert_eq!(imports(&compiled, "main.py"), ["tools=tools/__init__.py"]);
}

fn compiled_with_script(name: &str, entry: &str, roots: &[&str]) -> Compiled {
    compile_case(name, vec![script(entry, roots)])
}

/// The problems linking a case with `contexts` finds, as `path: message`.
fn problems(name: &str, contexts: Vec<PythonContext>) -> Vec<String> {
    linked(name, &inputs(contexts), |_, linked| {
        linked
            .problems()
            .iter()
            .map(|problem| format!("{}: {}", problem.path, problem.message))
            .collect()
    })
}

#[test]
fn ordered_roots_settle_modules_and_accumulate_namespace_portions() {
    let compiled = compiled_with_script("ordered-roots", "app/main.py", &["app", "first", "second"]);
    assert_eq!(
        imports(&compiled, "app/main.py"),
        [
            // The first root that has the module settles it.
            "shared=first/shared.py",
            // A namespace portion in `first` gives way to the regular package in `second`.
            "pkg=second/pkg/__init__.py",
            // Portions of one namespace package accumulate across roots.
            "ns.a=first/ns/a.py",
            "ns.b=second/ns/b.py",
        ]
    );
    assert_eq!(
        call(&compiled, "app/main.py", "shared.where"),
        "first/shared.py#function:where"
    );
    // Without a launch, the open folder is the only root.
    let unlaunched = compile_case("ordered-roots", Vec::new());
    assert_eq!(
        imports(&unlaunched, "app/main.py"),
        [
            "shared=external:shared",
            "pkg=external:pkg",
            "ns.a=external:ns",
            "ns.b=external:ns"
        ]
    );
}

#[test]
fn launches_that_disagree_about_a_shared_import_make_it_ambiguous() {
    let api = script("api/main.py", &["api", "."]);
    let worker = script("worker/main.py", &["worker", "."]);
    let compiled = compile_case("shared-utils", vec![api.clone(), worker.clone()]);
    assert_eq!(imports(&compiled, "shared/task.py"), ["utils=ambiguous"]);
    // Calls through the import follow the same decision.
    assert_eq!(call(&compiled, "shared/task.py", "utils.helper"), "ambiguous");
    // Each launch's own files resolve with its own path.
    assert_eq!(
        imports(&compiled, "api/main.py"),
        // `shared` is a namespace package: no file of its own.
        ["shared=not_indexed", "shared.task=shared/task.py"]
    );
    assert_eq!(
        problems("shared-utils", vec![api, worker]),
        [
            "shared/task.py: import `utils` is ambiguous: more than one source root (or launches that reach this file) supply different modules"
        ]
    );
    // A file no launch reaches keeps the open folder as its root.
    assert_eq!(imports(&compiled, "shared/other.py"), ["utils=external:utils"]);
}

#[test]
fn launches_that_agree_about_a_shared_import_write_the_normal_edge() {
    let script = script("api/main.py", &["api", "."]);
    let module = module("main", &["api", "."]);
    let compiled = compile_case("shared-utils", vec![script.clone(), module.clone()]);
    assert_eq!(imports(&compiled, "shared/task.py"), ["utils=api/utils.py"]);
    assert_eq!(
        call(&compiled, "shared/task.py", "utils.helper"),
        "api/utils.py#function:helper"
    );
    assert!(problems("shared-utils", vec![script, module]).is_empty());
    assert_eq!(imports(&compiled, "shared/other.py"), ["utils=external:utils"]);
}

#[test]
fn a_script_has_no_package_but_a_module_keeps_its_own() {
    let as_script = compiled_with_script("script-and-module", "pkg/run.py", &["pkg"]);
    assert_eq!(
        imports(&as_script, "pkg/run.py"),
        [".=not_found", ".helper=not_found"]
    );
    let as_module = compile_case("script-and-module", vec![module("pkg.run", &["."])]);
    assert_eq!(
        imports(&as_module, "pkg/run.py"),
        [
            ".=pkg/__init__.py",
            ".helper=pkg/helper.py",
            ".helper=pkg/helper.py"
        ]
    );
    assert_eq!(call(&as_module, "pkg/run.py", "go"), "pkg/helper.py#function:go");
}

#[test]
fn launch_targets_resolve_with_their_context() {
    let inputs = inputs(Vec::new());
    linked("script-and-module", &inputs, |program, linked| {
        let root = module("pkg", &["."]);
        let target = |target: LaunchTarget| linked.launch_target(&target, &root).expect("Python resolves it");
        let run = target(LaunchTarget::PythonModule {
            module: "pkg.run".into(),
        });
        assert_eq!(outcome(program, &run.target), "pkg/run.py#");
        assert_eq!(seeds(program, &run), ["pkg/__init__.py#"]);
        // `-m` on a package runs its `__main__.py` after the package.
        let package = target(LaunchTarget::PythonModule { module: "pkg".into() });
        assert_eq!(outcome(program, &package.target), "pkg/__main__.py#");
        assert_eq!(seeds(program, &package), ["pkg/__init__.py#"]);
        let file = target(LaunchTarget::File {
            path: RepoPath::new("pkg/run.py").expect("a path"),
        });
        assert_eq!(outcome(program, &file.target), "pkg/run.py#");
        assert!(file.seeds.is_empty());
        let missing = target(LaunchTarget::File {
            path: RepoPath::new("pkg/missing.py").expect("a path"),
        });
        assert_eq!(outcome(program, &missing.target), "not_found");
        // A module outside the roots is not found.
        let elsewhere = linked
            .launch_target(
                &LaunchTarget::PythonModule {
                    module: "pkg.run".into(),
                },
                &module("pkg", &["pkg"]),
            )
            .expect("Python resolves it");
        assert_eq!(outcome(program, &elsewhere.target), "not_found");
    });
}

#[test]
fn asgi_targets_resolve_to_the_bindings_their_routes_record() {
    let inputs = ProjectInputs {
        unindexed: vec![RepoPath::new("service/legacy.py").expect("a path")],
        ..inputs(Vec::new())
    };
    let launched = linked("applications", &inputs, |program, linked| {
        let root = module("service.apps", &["."]);
        let resolve = |module: &str, attribute: &str| {
            let target = LaunchTarget::PythonAsgi {
                module: module.into(),
                attribute: attribute.into(),
                factory: false,
            };
            let resolution = linked.launch_target(&target, &root).expect("Python resolves it");
            (outcome(program, &resolution.target), seeds(program, &resolution))
        };
        let public = resolve("service.apps", "public_app");
        assert_eq!(
            public,
            (
                "service/apps.py#variable:public_app".to_owned(),
                vec!["service/__init__.py#".to_owned(), "service/apps.py#".to_owned()]
            )
        );
        let admin = resolve("service.apps", "admin_app");
        assert_eq!(admin.0, "service/apps.py#variable:admin_app");
        // A missing attribute is not found even though the module exists.
        assert_eq!(resolve("service.apps", "missing_app").0, "not_found");
        assert_eq!(resolve("service.nothing", "app").0, "not_found");
        assert_eq!(resolve("service.legacy", "app").0, "not_indexed");
        let main = LaunchTarget::PythonCallable {
            module: "service.apps".into(),
            attribute: "main".into(),
        };
        let main = linked.launch_target(&main, &root).expect("Python resolves it");
        assert_eq!(outcome(program, &main.target), "service/apps.py#function:main");
        [public.0, admin.0]
    });
    // The router mounted into both applications gives one entrance per mount branch, each bound
    // to the application its branch reaches.
    let compiled = compile_folder(&case("applications")).valid();
    let routes: Vec<(String, String)> = compiled
        .graph
        .entrances
        .iter()
        .map(|entrance| {
            (
                entrance.trigger.as_deref().unwrap_or_default().to_owned(),
                entrance
                    .binding
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        routes,
        [
            ("/admin/items".to_owned(), launched[1].clone()),
            ("/public/items".to_owned(), launched[0].clone()),
        ]
    );
}

#[test]
fn a_draft_context_previews_what_it_would_resolve() {
    let inputs = inputs(Vec::new());
    linked("shared-utils", &inputs, |_, linked| {
        let preview = linked
            .context_preview(&script("api/main.py", &["api", "."]))
            .expect("Python previews it");
        let reached: Vec<&str> = preview.reached.iter().map(RepoPath::as_str).collect();
        assert_eq!(reached, ["api/main.py", "api/utils.py", "shared/task.py"]);
        // `shared.task` (the module `from shared import task` imports) resolves either way;
        // `import utils` only with the launch's path.
        assert_eq!((preview.resolved, preview.resolved_now), (2, 1));
        assert_eq!(preview.newly_resolved, ["utils".into()] as [Box<str>; 1]);
    });
}

#[test]
fn equal_contexts_reach_and_resolve_the_same() {
    let api = script("api/main.py", &["api", "."]);
    let once = compile_case("shared-utils", vec![api.clone()]);
    let twice = compile_case("shared-utils", vec![api.clone(), api]);
    assert_eq!(
        unfold_core::canonical::to_string(&once.graph),
        unfold_core::canonical::to_string(&twice.graph)
    );
}
