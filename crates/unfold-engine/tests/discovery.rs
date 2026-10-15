//! Discovery over the test playgrounds and fixtures, and over temporary folders for ignore files,
//! symlinks, scope, and determinism.

// `allow-unwrap-in-tests` covers `#[test]` functions but not this file's helpers.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};
use unfold_engine::discovery::{
    DiscoverOptions, Discovery, DiscoveryError, IgnoreRules, Unindexed, discover, exists_in_root,
};
use unfold_engine::settings::{Settings, SettingsFile};

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
    let settings = SettingsFile::load(root).unwrap().settings;
    discover(root, &settings, &options()).unwrap()
}

/// Discovers `root` with the settings `text`: discovery reads only the scope, so a playground's
/// scope is restated here and the rest of its `unfold.toml` doesn't matter.
fn run_with(root: &Path, text: &str) -> Discovery {
    discover(root, &SettingsFile::parse(text).unwrap().settings, &options()).unwrap()
}

fn sources(discovery: &Discovery) -> Vec<&str> {
    discovery.sources.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn playgrounds_know_their_unindexed_files() {
    let ts = run_with(&testdata().join("playgrounds/ts-orders"), "");
    let unindexed: Vec<(&str, Unindexed)> =
        ts.unindexed.iter().map(|f| (f.path.as_str(), f.reason)).collect();
    assert_eq!(
        unindexed,
        [
            ("report-processor.js", Unindexed::JavaScript),
            ("scripts/export.js", Unindexed::JavaScript)
        ]
    );
    let py = run_with(
        &testdata().join("playgrounds/py-orders"),
        "[scope]\nexclude = [\"legacy/**\", \"scripts/**\", \"tests/**\"]\n",
    );
    let legacy = py
        .unindexed
        .iter()
        .find(|file| file.path.as_str() == "legacy/orders_legacy/export.py")
        .map(|file| file.reason);
    assert_eq!(legacy, Some(Unindexed::Excluded));
    assert!(sources(&py).contains(&"src/orders_playground/server.py"));
}

#[test]
fn python_fixture_file_counts() {
    for (folder, count) in [("fixtures/python", 23), ("fixtures/python-ranges", 1)] {
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
    // Configuration inputs are fingerprinted whatever the scope.
    let inputs: Vec<(&str, InputRole)> = discovery
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), input.role))
        .collect();
    assert_eq!(
        inputs,
        [
            ("app/pyproject.toml", InputRole::Pyproject),
            ("pyproject.toml", InputRole::Pyproject),
            ("unfold.toml", InputRole::UnfoldConfig),
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
    let python = discovery
        .sources
        .iter()
        .filter(|file| file.language == Language::Python);
    assert_eq!(python.count(), 1);
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
    assert!(exists_in_root(root, &RepoPath::new("linked.ts").unwrap()));
    assert!(exists_in_root(root, &RepoPath::new("real").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("escape.ts").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("dangling.ts").unwrap()));
    assert!(!exists_in_root(root, &RepoPath::new("missing.ts").unwrap()));
}

/// Discovery's problems as (path, message) pairs.
fn problems(discovery: &Discovery) -> Vec<(&str, &str)> {
    discovery
        .problems
        .iter()
        .map(|problem| (&*problem.path, &*problem.message))
        .collect()
}

#[test]
fn names_that_cannot_be_paths_are_left_out_and_listed() {
    let dir = folder(&[
        ("ok.ts", ""),
        ("bad\\name.ts", ""),
        ("bad\\dir/inner.ts", ""),
        ("c:drive.ts", ""),
        ("node_modules/odd\\name.ts", ""),
    ]);
    let discovery = run(dir.path());
    assert_eq!(sources(&discovery), ["ok.ts"]);
    assert!(discovery.unindexed.is_empty());
    assert_eq!(
        problems(&discovery),
        [(
            "",
            "names that can't be project paths, left out: `bad\\dir`, `bad\\name.ts`, `c:drive.ts`"
        )]
    );

    // One problem, listing the first few.
    let many: Vec<(String, &str)> = (0..15).map(|i| (format!("x\\{i:02}.ts"), "")).collect();
    let borrowed: Vec<(&str, &str)> = many.iter().map(|(p, c)| (p.as_str(), *c)).collect();
    let discovery = run(folder(&borrowed).path());
    let listed: Vec<String> = (0..10).map(|i| format!("`x\\{i:02}.ts`")).collect();
    assert_eq!(
        problems(&discovery),
        [(
            "",
            &*format!(
                "names that can't be project paths, left out: {}, and 5 more",
                listed.join(", ")
            )
        )]
    );
}

#[test]
fn ignore_rules_that_do_not_parse_are_listed() {
    let dir = folder(&[
        (".gitignore", "dist/\n[z-a]\n*.gen.ts\n"),
        ("sub/.ignore", "ok/\nbad{x\n"),
        (".git/info/exclude", "[z-a].ts\n"),
        ("node_modules/.gitignore", "[z-a]\n"),
        ("dist/.gitignore", "[z-a]\n"),
        ("dist/out.ts", ""),
        ("src/a.ts", ""),
        ("src/b.gen.ts", ""),
    ]);
    let discovery = run(dir.path());
    assert_eq!(sources(&discovery), ["src/a.ts"], "the rules that parse apply");
    assert_eq!(
        problems(&discovery),
        [
            (
                ".git/info/exclude",
                "line 1: error parsing glob '[z-a].ts': invalid range; 'z' > 'a'"
            ),
            (
                ".gitignore",
                "line 2: error parsing glob '[z-a]': invalid range; 'z' > 'a'"
            ),
            (
                "sub/.ignore",
                "line 2: error parsing glob 'bad{x': unclosed alternate group; missing '}' \
                 (maybe escape '{' with '[{]'?)"
            ),
        ],
        "ignore files in folders discovery never walks don't count"
    );
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
