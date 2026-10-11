//! Determinism and incremental-equals-full: the graph depends only on the effective inputs,
//! not on input order, thread count, or the sequence of updates that led to them
//! (docs/graph-model.md#order-and-canonical-export).

use std::sync::Arc;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use unfold_engine::UpdateRequest;
use unfold_engine::update::MemoryFactsCache;

use super::{Counting, Folder, background, canonical, compile, engine_with, is_broken, template};

const PATHS: &[(&str, usize)] = &[
    ("main.ts", 0),
    ("lib/util.ts", 1),
    ("lib/base.ts", 1),
    ("jobs/run.ts", 1),
    ("jobs/deep/task.ts", 2),
];

const CONFIGS: &[Option<&str>] = &[
    None,
    Some("[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"main.ts#go\"\n"),
    Some("[scope]\nexclude = [\"jobs/deep/**\"]\n"),
];

fn files() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = PATHS
        .iter()
        .enumerate()
        .map(|(index, (path, depth))| ((*path).to_owned(), template(index % 6, *depth)))
        .collect();
    files.push(("broken.ts".to_owned(), template(6, 0)));
    for index in 0..30 {
        files.push((format!("gen/f{index:02}.ts"), template(8 + index, 1)));
    }
    files
}

fn compile_folder(files: &[(String, String)], threads: usize) -> String {
    let folder = Folder::new();
    for (path, text) in files {
        folder.write(path, text);
    }
    let mut engine = engine_with(
        &folder,
        Counting::new(),
        Arc::new(MemoryFactsCache::new()),
        threads,
    );
    canonical(&compile(&mut engine))
}

#[test]
fn shuffled_files_and_thread_counts_give_identical_exports() {
    let files = files();
    let expected = compile_folder(&files, 1);
    let mut reversed = files;
    reversed.reverse();
    for threads in [1, 3, 8] {
        assert_eq!(compile_folder(&reversed, threads), expected, "{threads} threads");
    }
    assert!(expected.contains("\"state\":\"failed\""));
    assert!(expected.contains("express.route"));
}

/// One step of an edit sequence.
#[derive(Debug, Clone)]
enum Edit {
    /// Write template `.1` to path `.0`.
    Write(usize, usize),
    Delete(usize),
    DeleteFolder(&'static str),
    Config(usize),
    /// An explicit Compile on the incremental engine.
    Compile,
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        6 => (0..PATHS.len(), 0..10usize).prop_map(|(path, text)| Edit::Write(path, text)),
        2 => (0..PATHS.len()).prop_map(Edit::Delete),
        1 => prop::sample::select(vec!["lib", "jobs"]).prop_map(Edit::DeleteFolder),
        1 => (0..CONFIGS.len()).prop_map(Edit::Config),
        1 => Just(Edit::Compile),
    ]
}

/// Applies `edit` to `folder`, returning the paths a watcher would report.
fn apply(folder: &Folder, edit: &Edit) -> Vec<String> {
    match *edit {
        Edit::Write(path, text) => {
            let (path, depth) = PATHS[path];
            folder.write(path, &template(text, depth));
            vec![path.to_owned()]
        }
        Edit::Delete(path) => {
            folder.remove(PATHS[path].0);
            vec![PATHS[path].0.to_owned()]
        }
        Edit::DeleteFolder(name) => {
            folder.remove(name);
            vec![name.to_owned()]
        }
        Edit::Config(index) => {
            match CONFIGS[index] {
                Some(text) => folder.write(".unfold/config.toml", text),
                None => folder.remove(".unfold/config.toml"),
            }
            vec![".unfold/config.toml".to_owned()]
        }
        Edit::Compile => Vec::new(),
    }
}

/// Runs `steps` (each a batch of edits) as updates, then compares the incremental graph with
/// fresh compiles of the same folder.
fn check_sequence(steps: &[Vec<Edit>]) -> Result<(), TestCaseError> {
    let folder = Folder::new();
    for (index, (path, depth)) in PATHS.iter().enumerate() {
        folder.write(path, &template(index, *depth));
    }
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let mut last = compile(&mut engine);
    for step in steps {
        let mut paths = Vec::new();
        let mut full = false;
        for edit in step {
            full |= matches!(edit, Edit::Compile);
            paths.extend(apply(&folder, edit));
        }
        last = if full {
            compile(&mut engine)
        } else {
            let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
            background(&mut engine, &paths)
        };
    }
    let incremental = canonical(&last);
    // A fresh engine over the same cache sees the same last-good facts the incremental one kept.
    let mut fresh = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let full = fresh.update(UpdateRequest::compile()).expect("fresh compile");
    prop_assert_eq!(&incremental, &canonical(&full));
    prop_assert_eq!(last.complete, full.complete);
    // Without broken files, last-good facts play no part: a cold compile agrees too.
    let broken = steps
        .iter()
        .flatten()
        .any(|edit| matches!(edit, Edit::Write(_, text) if is_broken(*text)));
    if !broken {
        let mut cold = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 1);
        prop_assert_eq!(&incremental, &canonical(&compile(&mut cold)));
    }
    Ok(())
}

#[test]
fn incremental_updates_equal_a_full_compile() {
    let config = Config {
        cases: 48,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner = TestRunner::new(config);
    let steps = prop::collection::vec(prop::collection::vec(edit(), 1..4), 1..10);
    runner
        .run(&steps, |steps| check_sequence(&steps))
        .expect("incremental equals full");
}

#[test]
fn breaking_and_fixing_a_file_matches_a_fresh_compile() {
    let steps = vec![
        vec![Edit::Write(1, 6)],
        vec![Edit::Delete(1)],
        vec![Edit::Write(1, 7)],
        vec![Edit::Write(0, 9), Edit::Config(1)],
        vec![Edit::Write(1, 0)],
    ];
    check_sequence(&steps).expect("matches");
}

/// A case-only rename through a background update leaves only the new name in the graph, as a
/// fresh compile would. On a case-insensitive file system the old name still finds the file,
/// so the scan must compare names as spelled on disk.
#[test]
fn a_case_only_rename_matches_a_fresh_compile() {
    let folder = Folder::new();
    for (path, text) in files() {
        folder.write(&path, &text);
    }
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    compile(&mut engine);

    for (from, to) in [("lib/util.ts", "lib/Util.ts"), ("lib/Util.ts", "lib/util.ts")] {
        std::fs::rename(folder.path().join(from), folder.path().join(to)).expect("rename");
        let outcome = background(&mut engine, &[from, to]);
        let paths: Vec<&str> = outcome
            .graph
            .files
            .iter()
            .map(|file| file.file.path.as_str())
            .filter(|path| path.eq_ignore_ascii_case(to))
            .collect();
        assert_eq!(paths, [to], "only the new name is in the graph");
        let mut fresh = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2);
        assert_eq!(canonical(&outcome), canonical(&compile(&mut fresh)));
    }
}
