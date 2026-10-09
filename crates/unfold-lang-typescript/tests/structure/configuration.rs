//! Project configuration: the nearest `tsconfig.json`, `extends` chains (relative, package,
//! arrays, missing, circular), the inputs a chain reads and requests, and problems reported
//! instead of failures.

use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Resolution};
use unfold_engine::Language;
use unfold_engine::settings::Settings;

use crate::support::Project;

const VERBATIM: &str = r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#;
const EMPTY_IMPORT: &str = "import {} from './b';";

fn verbatim_imports(project: &Project, path: &str) -> usize {
    project.compile().import_count(path)
}

#[test]
fn each_file_uses_its_nearest_configuration() {
    let project = Project::new()
        .input("tsconfig.json", VERBATIM)
        .input("plain/tsconfig.json", "{}")
        .source("a.ts", EMPTY_IMPORT)
        .source("b.ts", "")
        .source("deep/er/c.ts", "import {} from '../../b';")
        .source("plain/d.ts", "import {} from '../b';");
    assert_eq!(verbatim_imports(&project, "a.ts"), 1);
    assert_eq!(verbatim_imports(&project, "deep/er/c.ts"), 1);
    assert_eq!(verbatim_imports(&project, "plain/d.ts"), 0);
}

#[test]
fn extends_chains_merge_in_order() {
    let project = Project::new()
        .input("configs/verbatim.json", VERBATIM)
        .input(
            "configs/off.json",
            r#"{"compilerOptions": {"verbatimModuleSyntax": false}}"#,
        )
        .input("a/tsconfig.json", r#"{"extends": "../configs/verbatim"}"#)
        .input(
            "b/tsconfig.json",
            r#"{"extends": ["../configs/verbatim.json", "../configs/off.json"]}"#,
        )
        .input(
            "c/tsconfig.json",
            r#"{"extends": "../configs/off.json", "compilerOptions": {"verbatimModuleSyntax": true}}"#,
        )
        .source("a/x.ts", "import {} from '../b';")
        .source("b/x.ts", "import {} from '../b';")
        .source("c/x.ts", "import {} from '../b';")
        .source("b.ts", "");
    // `.json` is added to a relative base that doesn't exist as written.
    assert_eq!(verbatim_imports(&project, "a/x.ts"), 1);
    // Later bases override earlier ones; the file's own options override them all.
    assert_eq!(verbatim_imports(&project, "b/x.ts"), 0);
    assert_eq!(verbatim_imports(&project, "c/x.ts"), 1);
    let inputs = project.link(|_, linked| linked.inputs_read());
    let paths: Vec<&str> = inputs.iter().map(RepoPath::as_str).collect();
    assert_eq!(
        paths,
        [
            "a/tsconfig.json",
            "b/tsconfig.json",
            "c/tsconfig.json",
            "configs/off.json",
            "configs/verbatim.json"
        ]
    );
}

#[test]
fn package_bases_are_requested_from_node_modules_and_used_once_read() {
    let base = Project::new()
        .input(
            "app/tsconfig.json",
            r#"{"extends": "@tsconfig/strictest/tsconfig.json"}"#,
        )
        .source("app/x.ts", EMPTY_IMPORT)
        .source("app/b.ts", "");
    let requested: Vec<(String, InputRole)> = base
        .language()
        .requested_inputs(base.inputs())
        .into_iter()
        .map(|(path, role)| (path.as_str().to_owned(), role))
        .collect();
    assert_eq!(
        requested,
        [
            (
                "app/node_modules/@tsconfig/strictest/tsconfig.json".to_owned(),
                InputRole::Tsconfig
            ),
            (
                "app/node_modules/@tsconfig/strictest/tsconfig.json/package.json".to_owned(),
                InputRole::Tsconfig
            ),
            (
                "app/node_modules/@tsconfig/strictest/tsconfig.json/tsconfig.json".to_owned(),
                InputRole::Tsconfig
            ),
            (
                "node_modules/@tsconfig/strictest/tsconfig.json".to_owned(),
                InputRole::Tsconfig
            ),
            (
                "node_modules/@tsconfig/strictest/tsconfig.json/package.json".to_owned(),
                InputRole::Tsconfig
            ),
            (
                "node_modules/@tsconfig/strictest/tsconfig.json/tsconfig.json".to_owned(),
                InputRole::Tsconfig
            ),
        ]
    );
    // Not installed: a reported problem, not a failure; the configuration carries on.
    let problems = base.language().input_problems(base.inputs());
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path.as_str(), "app/tsconfig.json");
    assert!(
        problems[0].message.contains("is the package installed?"),
        "{}",
        problems[0].message
    );
    assert_eq!(verbatim_imports(&base, "app/x.ts"), 0);

    // Installed (the engine read the requested file): its options apply, and nothing more is
    // requested.
    let installed = base.input("node_modules/@tsconfig/strictest/tsconfig.json", VERBATIM);
    assert!(installed.language().input_problems(installed.inputs()).is_empty());
    assert!(
        installed
            .language()
            .requested_inputs(installed.inputs())
            .is_empty()
    );
    assert_eq!(verbatim_imports(&installed, "app/x.ts"), 1);
    let inputs = installed.link(|_, linked| linked.inputs_read());
    assert_eq!(inputs.len(), 2, "{inputs:?}");
}

#[test]
fn a_package_base_can_be_named_by_its_tsconfig_field() {
    let project = Project::new()
        .input("tsconfig.json", r#"{"extends": "shared-config"}"#)
        .input(
            "node_modules/shared-config/package.json",
            r#"{"tsconfig": "./base.json"}"#,
        )
        .input("node_modules/shared-config/base.json", VERBATIM)
        .source("x.ts", EMPTY_IMPORT)
        .source("b.ts", "");
    assert!(project.language().input_problems(project.inputs()).is_empty());
    assert_eq!(verbatim_imports(&project, "x.ts"), 1);
}

#[test]
fn broken_and_circular_configurations_are_problems() {
    let project = Project::new()
        .input(
            "a/tsconfig.json",
            r#"{"extends": "./loop.json", "compilerOptions": {"module": 7}}"#,
        )
        .input("a/loop.json", r#"{"extends": "./tsconfig.json"}"#)
        .input("b/tsconfig.json", "{ not json")
        .input("c/tsconfig.json", r#"{"extends": "../../outside.json"}"#)
        .source("a/x.ts", "")
        .source("b/x.ts", "")
        .source("c/x.ts", "");
    let problems: Vec<(String, String)> = project
        .language()
        .input_problems(project.inputs())
        .into_iter()
        .map(|problem| (problem.path.as_str().to_owned(), problem.message.to_string()))
        .collect();
    let paths: Vec<&str> = problems.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "a/loop.json",
            "a/tsconfig.json",
            "b/tsconfig.json",
            "c/tsconfig.json"
        ],
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|(_, message)| message.contains("extends this file again"))
    );
    assert!(
        problems
            .iter()
            .any(|(_, message)| message.contains("`compilerOptions.module`"))
    );
    assert!(
        problems
            .iter()
            .any(|(_, message)| message.contains("not valid JSON"))
    );
    assert!(
        problems
            .iter()
            .any(|(_, message)| message.contains("outside the folder"))
    );
    // Everything still compiles.
    let compiled = project.compile();
    assert_eq!(compiled.graph.files.len(), 3);
}

#[test]
fn a_configuration_change_changes_exactly_the_options_of_its_files() {
    let settings = Settings::default();
    let before = Project::new()
        .input("tsconfig.json", "{}")
        .input("strict/tsconfig.json", "{}")
        .source("a.ts", "")
        .source("strict/b.ts", "");
    let after = before.clone().input("strict/tsconfig.json", VERBATIM);
    let paths = [
        RepoPath::new("a.ts").expect("path"),
        RepoPath::new("strict/b.ts").expect("path"),
    ];
    let refs: Vec<&RepoPath> = paths.iter().collect();
    let hashes = |project: &Project| -> Vec<_> {
        project
            .language()
            .extraction_options(&refs, &settings, project.inputs())
            .into_iter()
            .map(|options| options.hash)
            .collect()
    };
    let (old, new) = (hashes(&before), hashes(&after));
    assert_eq!(old[0], new[0], "a.ts keeps its options");
    assert_ne!(old[1], new[1], "strict/b.ts changes");
    // An unrelated edit (a comment) changes no options.
    let commented = before.input("strict/tsconfig.json", "{ /* still empty */ }");
    assert_eq!(hashes(&commented), old);
}

#[test]
fn the_default_configuration_is_nodenext() {
    // Without a tsconfig.json, `import './b'` resolves in CommonJS mode and an ESM package
    // needs the extension.
    let commonjs = Project::new().source("a.ts", "import './b';").source("b.ts", "");
    assert_eq!(
        commonjs.compile().import("a.ts", "./b").resolution,
        Resolution::Resolved
    );
    let esm = commonjs.input("package.json", r#"{"type": "module"}"#);
    assert_eq!(
        esm.compile().import("a.ts", "./b").resolution,
        Resolution::Unresolved
    );
}
