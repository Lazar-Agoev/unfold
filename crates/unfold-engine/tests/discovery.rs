//! Discovery over the test playgrounds (checked against the proof of concept's indexed file
//! sets) and over temporary folders for ignore files, symlinks, scope, and determinism.

// `allow-unwrap-in-tests` covers `#[test]` functions but not this file's helpers.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};
use unfold_engine::discovery::{
    DiscoverOptions, Discovery, DiscoveryError, IgnoreRules, SkipReason, Unindexed, discover, exists_in_root,
};
use unfold_engine::settings::{ScopeStatus, Settings};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

fn options() -> DiscoverOptions {
    DiscoverOptions {
        threads: 0,
        global_ignore: false,
    }
}

fn run(root: &Path) -> Discovery {
    let settings = Settings::load(root).unwrap();
    discover(root, &settings, &options()).unwrap()
}

fn sources(discovery: &Discovery) -> Vec<&str> {
    discovery.sources.iter().map(|file| file.path.as_str()).collect()
}

/// The PoC's indexed files and inputs from a reference manifest, with path prefixes mapped to
/// the Unfold folder (`codegraph.json` files become `unfold.toml`).
fn reference(corpus: &str, prefixes: &[(&str, &str)]) -> (BTreeSet<String>, BTreeSet<String>) {
    let text = fs::read_to_string(
        testdata()
            .join("reference/poc")
            .join(corpus)
            .join("manifest.json"),
    )
    .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    let map = |path: &str| -> String {
        if path.ends_with("codegraph.json") {
            return "unfold.toml".to_owned();
        }
        prefixes
            .iter()
            .find_map(|(from, to)| path.strip_prefix(from).map(|rest| format!("{to}{rest}")))
            .unwrap_or_else(|| path.to_owned())
    };
    let keys = |field: &str| -> BTreeSet<String> {
        manifest[field]
            .as_object()
            .unwrap()
            .keys()
            .map(|path| map(path))
            .collect()
    };
    (keys("files"), keys("inputs"))
}

fn assert_matches_reference(folder: &str, corpus: &str, prefixes: &[(&str, &str)], count: usize) {
    let discovery = run(&testdata().join(folder));
    let (files, inputs) = reference(corpus, prefixes);
    let found: BTreeSet<String> = sources(&discovery).into_iter().map(str::to_owned).collect();
    assert_eq!(found.len(), count, "{folder}");
    assert_eq!(found, files, "{folder}: indexed files differ from {corpus}");
    // The PoC's inputs are those its detectors read; discovery also lists every tsconfig.
    let found_inputs: BTreeSet<String> = discovery
        .inputs
        .iter()
        .filter(|input| input.role != InputRole::Tsconfig)
        .map(|input| input.path.to_string())
        .collect();
    assert_eq!(found_inputs, inputs, "{folder}: inputs differ from {corpus}");
    assert!(discovery.problems.is_empty(), "{:?}", discovery.problems);
}

#[test]
fn playground_file_sets_equal_the_proof_of_concept() {
    assert_matches_reference(
        "playgrounds/ts-orders",
        "playground-ts-native",
        &[("ts-playground/", "")],
        34,
    );
    assert_matches_reference(
        "playgrounds/py-orders",
        "python-playground",
        &[("py-playground/", "")],
        50,
    );
    assert_matches_reference(
        "playgrounds",
        "mixed-playground",
        &[("ts-playground/", "ts-orders/"), ("py-playground/", "py-orders/")],
        84,
    );
    assert_matches_reference("fixtures/entrances", "entrances", &[], 5);
}

#[test]
fn playgrounds_know_their_unindexed_files() {
    let ts = run(&testdata().join("playgrounds/ts-orders"));
    let unindexed: Vec<(&str, Unindexed)> =
        ts.unindexed.iter().map(|f| (f.path.as_str(), f.reason)).collect();
    assert_eq!(
        unindexed,
        [
            ("report-processor.js", Unindexed::JavaScript),
            ("scripts/export.js", Unindexed::JavaScript)
        ]
    );
    let py = run(&testdata().join("playgrounds/py-orders"));
    let legacy = RepoPath::new("legacy/orders_legacy/export.py").unwrap();
    assert_eq!(py.unindexed_reason(&legacy), Some(Unindexed::Excluded));
    assert!(py.is_source(&RepoPath::new("src/orders_playground/server.py").unwrap()));
    let report: Vec<(String, SkipReason, u32)> = py
        .skipped
        .iter()
        .map(|line| (line.folder.to_string(), line.reason, line.files))
        .collect();
    assert_eq!(
        report,
        [
            ("legacy".into(), SkipReason::Excluded, 2),
            ("scripts".into(), SkipReason::Excluded, 1),
            ("tests".into(), SkipReason::Excluded, 3),
        ]
    );
}

#[test]
fn fixture_file_counts_equal_the_proof_of_concept() {
    for (folder, count) in [
        ("fixtures/worked", 1),
        ("fixtures/classes", 3),
        ("fixtures/playground-v1", 2),
        ("fixtures/python", 23),
        ("fixtures/python-ranges", 1),
    ] {
        assert_eq!(run(&testdata().join(folder)).sources.len(), count, "{folder}");
    }
}

/// Writes `files` (path, contents) under a new temporary folder.
fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
    dir
}

#[test]
fn default_exclusions_are_never_walked() {
    let dir = folder(&[
        ("a.ts", ""),
        ("node_modules/dep/index.ts", ""),
        ("web/node_modules/dep/index.ts", ""),
        (".venv/lib/x.py", ""),
        ("venv/lib/x.py", ""),
        ("pkg/__pycache__/x.py", ""),
        (".mypy_cache/x.py", ""),
        (".pytest_cache/x.py", ""),
        (".ruff_cache/x.py", ""),
        (".git/hooks/x.ts", ""),
        ("sub/unfold.toml", ""),
        ("sub/.unfold/x.ts", ""),
        ("node_modules.ts", ""),
    ]);
    let discovery = run(dir.path());
    assert_eq!(sources(&discovery), ["a.ts", "node_modules.ts"]);
    assert!(
        discovery.inputs.is_empty(),
        "a nested unfold.toml or .unfold/ is never read: {:?}",
        discovery.inputs
    );
    let pruned: Vec<String> = discovery
        .skipped
        .iter()
        .filter(|line| line.reason == SkipReason::Default && line.whole_folder)
        .map(|line| line.folder.to_string())
        .collect();
    assert_eq!(
        pruned,
        [
            ".mypy_cache",
            ".pytest_cache",
            ".ruff_cache",
            ".venv",
            "node_modules",
            "pkg/__pycache__",
            "venv",
            "web/node_modules"
        ]
    );
}

#[test]
fn ignore_files_apply_at_every_level() {
    let dir = folder(&[
        (".gitignore", "dist/\n*.gen.ts\n!keep.gen.ts\n"),
        ("src/a.ts", ""),
        ("src/b.gen.ts", ""),
        ("src/keep.gen.ts", ""),
        ("dist/out.ts", ""),
        ("pkg/.gitignore", "generated/\nlocal.py\n"),
        ("pkg/generated/x.py", ""),
        ("pkg/local.py", ""),
        ("pkg/kept.py", ""),
        ("other/local.py", ""),
        (".ignore", "scratch/\n"),
        ("scratch/x.ts", ""),
        (".git/info/exclude", "private.ts\n"),
        ("private.ts", ""),
        ("node_modules/dep.ts", ""),
    ]);
    // node_modules is ignored by nothing here, but would be by .gitignore in most projects.
    fs::write(
        dir.path().join(".gitignore"),
        "dist/\n*.gen.ts\n!keep.gen.ts\nnode_modules/\n",
    )
    .unwrap();
    let discovery = run(dir.path());
    assert_eq!(
        sources(&discovery),
        ["other/local.py", "pkg/kept.py", "src/a.ts", "src/keep.gen.ts"]
    );
    let report: Vec<(String, SkipReason, bool, u32)> = discovery
        .skipped
        .iter()
        .map(|line| {
            (
                line.folder.to_string(),
                line.reason,
                line.whole_folder,
                line.files,
            )
        })
        .collect();
    assert_eq!(
        report,
        [
            (".".into(), SkipReason::Ignored, false, 1),
            ("dist".into(), SkipReason::Ignored, true, 0),
            ("node_modules".into(), SkipReason::Default, true, 0),
            ("pkg".into(), SkipReason::Ignored, false, 1),
            ("pkg/generated".into(), SkipReason::Ignored, true, 0),
            ("scratch".into(), SkipReason::Ignored, true, 0),
            ("src".into(), SkipReason::Ignored, false, 1),
        ]
    );

    let mut rules = IgnoreRules::new(dir.path(), &options());
    let path = |p: &str| RepoPath::new(p).unwrap();
    assert!(rules.is_ignored(&path("dist/new.ts"), false));
    assert!(rules.is_ignored(&path("src/c.gen.ts"), false));
    assert!(!rules.is_ignored(&path("src/keep.gen.ts"), false));
    assert!(rules.is_ignored(&path("pkg/generated/y.py"), false));
    assert!(rules.is_ignored(&path("pkg/local.py"), false));
    assert!(!rules.is_ignored(&path("other/local.py"), false));
    assert!(rules.is_ignored(&path("private.ts"), false));
    assert!(rules.is_ignored(&path("a/.venv/x.py"), false));
    assert!(!rules.is_ignored(&path("src/new.ts"), false));
}

#[test]
fn scope_and_languages_decide_what_is_indexed() {
    let dir = folder(&[
        (
            "unfold.toml",
            "languages = [\"python\"]\n[scope]\ninclude = [\"app\"]\nexclude = [\"app/legacy/**\"]\n",
        ),
        ("app/main.py", ""),
        ("app/legacy/old.py", ""),
        ("app/legacy/deep/older.py", ""),
        ("tools/x.py", ""),
        ("app/web.ts", ""),
        ("app/types.d.ts", ""),
        ("app/pyproject.toml", ""),
        ("pyproject.toml", ""),
    ]);
    let discovery = run(dir.path());
    assert_eq!(sources(&discovery), ["app/main.py"]);
    let unindexed: Vec<(&str, Unindexed)> = discovery
        .unindexed
        .iter()
        .map(|f| (f.path.as_str(), f.reason))
        .collect();
    assert_eq!(
        unindexed,
        [
            ("app/legacy/deep/older.py", Unindexed::Excluded),
            ("app/legacy/old.py", Unindexed::Excluded),
            ("app/web.ts", Unindexed::LanguageOff),
            ("tools/x.py", Unindexed::NotIncluded),
        ]
    );
    assert!(
        discovery.context.is_empty(),
        "TypeScript is off, so declaration files don't matter"
    );
    let inputs: Vec<(&str, InputRole, ScopeStatus)> = discovery
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), input.role, input.scope))
        .collect();
    assert_eq!(
        inputs,
        [
            ("app/pyproject.toml", InputRole::Pyproject, ScopeStatus::InScope),
            ("pyproject.toml", InputRole::Pyproject, ScopeStatus::NotIncluded),
            ("unfold.toml", InputRole::UnfoldConfig, ScopeStatus::InScope),
        ]
    );
    let report: Vec<(String, SkipReason, u32)> = discovery
        .skipped
        .iter()
        .map(|line| (line.folder.to_string(), line.reason, line.files))
        .collect();
    assert_eq!(
        report,
        [
            ("app".into(), SkipReason::LanguageOff, 1),
            ("app/legacy".into(), SkipReason::Excluded, 2),
            ("tools".into(), SkipReason::NotIncluded, 1),
        ]
    );
}

#[test]
fn files_are_classified_with_stamps() {
    let dir = folder(&[
        ("src/a.ts", "export {};\n"),
        ("src/b.tsx", ""),
        ("src/c.mts", ""),
        ("src/d.cts", ""),
        ("src/types.d.ts", ""),
        ("src/e.d.mts", ""),
        ("src/f.js", ""),
        ("src/g.py", ""),
        ("src/h.pyi", ""),
        ("tsconfig.json", "{}"),
        ("tsconfig.build.json", "{}"),
        ("package.json", "{}"),
        ("README.md", ""),
    ]);
    let discovery = run(dir.path());
    assert_eq!(
        sources(&discovery),
        ["src/a.ts", "src/b.tsx", "src/c.mts", "src/d.cts", "src/g.py"]
    );
    assert_eq!(discovery.sources_of(Language::Python).count(), 1);
    assert_eq!(discovery.sources[0].stamp.size, 11);
    assert!(discovery.sources[0].stamp.modified.is_some());
    let context: Vec<&str> = discovery.context.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(context, ["src/e.d.mts", "src/types.d.ts"]);
    let unindexed: Vec<&str> = discovery
        .unindexed
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(unindexed, ["src/f.js"]);
    let inputs: Vec<&str> = discovery.inputs.iter().map(|input| input.path.as_str()).collect();
    assert_eq!(inputs, ["package.json", "tsconfig.build.json", "tsconfig.json"]);
}

#[cfg(unix)]
#[test]
fn symlinks_never_lead_out_of_the_folder() {
    use std::os::unix::fs::symlink;
    let outside = folder(&[("secret.ts", ""), ("dir/x.ts", "")]);
    let dir = folder(&[("target.ts", "const value = 1;"), ("real/inner.ts", "")]);
    let root = dir.path();
    symlink("target.ts", root.join("linked.ts")).unwrap();
    symlink(outside.path().join("secret.ts"), root.join("escape.ts")).unwrap();
    symlink(outside.path().join("dir"), root.join("escape-dir")).unwrap();
    symlink("real", root.join("alias")).unwrap();
    symlink("missing.ts", root.join("dangling.ts")).unwrap();
    symlink(".", root.join("real/loop")).unwrap();
    let discovery = run(root);
    assert_eq!(sources(&discovery), ["linked.ts", "real/inner.ts", "target.ts"]);
    assert_eq!(
        discovery.sources[0].stamp.size, 16,
        "a symlinked file's stamp is its target's"
    );
    let report: Vec<(String, SkipReason, bool, u32)> = discovery
        .skipped
        .iter()
        .map(|line| {
            (
                line.folder.to_string(),
                line.reason,
                line.whole_folder,
                line.files,
            )
        })
        .collect();
    assert_eq!(
        report,
        [
            (".".into(), SkipReason::Symlink, false, 2),
            ("alias".into(), SkipReason::Symlink, true, 0),
            ("escape-dir".into(), SkipReason::Symlink, true, 0),
            ("real/loop".into(), SkipReason::Symlink, true, 0),
        ]
    );
    assert!(exists_in_root(root, &RepoPath::new("linked.ts").unwrap()));
    assert!(exists_in_root(root, &RepoPath::new("real").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("escape.ts").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("dangling.ts").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("missing.ts").unwrap()));
}

#[cfg(unix)]
#[test]
fn names_that_cannot_be_paths_are_reported() {
    let dir = folder(&[("ok.ts", ""), ("bad\\name.ts", "")]);
    let discovery = run(dir.path());
    assert_eq!(sources(&discovery), ["ok.ts"]);
    assert_eq!(discovery.skipped.len(), 1);
    assert_eq!(discovery.skipped[0].reason, SkipReason::InvalidPath);
}

#[test]
fn results_do_not_depend_on_thread_count() {
    let files: Vec<(String, String)> = (0..300)
        .map(|i| {
            (
                format!(
                    "d{}/e{}/f{i}.{}",
                    i % 7,
                    i % 5,
                    if i % 3 == 0 { "py" } else { "ts" }
                ),
                String::new(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
    let dir = folder(&borrowed);
    let settings = Settings::default();
    let single = discover(
        dir.path(),
        &settings,
        &DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
    )
    .unwrap();
    for threads in [2, 8] {
        let many = discover(
            dir.path(),
            &settings,
            &DiscoverOptions {
                threads,
                global_ignore: false,
            },
        )
        .unwrap();
        assert_eq!(single, many);
    }
    assert_eq!(single.sources.len(), 300);
    assert!(single.sources.windows(2).all(|pair| pair[0].path < pair[1].path));
}

#[test]
fn an_unusable_root_fails() {
    let dir = folder(&[("file.ts", "")]);
    let settings = Settings::default();
    assert!(matches!(
        discover(&dir.path().join("file.ts"), &settings, &options()),
        Err(DiscoveryError::NotAFolder { .. })
    ));
    assert!(matches!(
        discover(&dir.path().join("missing"), &settings, &options()),
        Err(DiscoveryError::Root { .. })
    ));
}
