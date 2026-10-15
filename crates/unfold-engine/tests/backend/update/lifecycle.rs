//! File states across updates (last-good facts), changed paths, cancellation, and failed updates.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use unfold_core::LinkTarget;
use unfold_core::vocab::{FileState, Reason};
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{CancelToken, ProblemKind, Stage, UpdateError, UpdateRequest};

use super::{Counting, Folder, background, canonical, compile, engine, engine_with, file, template};

fn paths(paths: &[unfold_core::RepoPath]) -> Vec<&str> {
    paths.iter().map(|path| path.as_str()).collect()
}

fn helper_call(outcome: &unfold_engine::UpdateOutcome) -> LinkTarget {
    file(outcome, "main.ts")
        .links
        .iter()
        .find(|link| &*link.text == "helper")
        .expect("a call of helper")
        .to
        .clone()
}

fn resolved(target: &LinkTarget) -> bool {
    matches!(target, LinkTarget::Project(_))
}

#[test]
fn a_file_that_stops_parsing_keeps_its_last_good_facts() {
    let folder = Folder::new();
    folder.write("lib/util.ts", &template(0, 1));
    folder.write("main.ts", &template(1, 0));
    let mut engine = engine(&folder);
    let first = compile(&mut engine);
    assert!(first.complete);
    assert!(resolved(&helper_call(&first)));

    folder.write("lib/util.ts", "(fn");
    let broken = background(&mut engine, &["lib/util.ts"]);
    let util = file(&broken, "lib/util.ts");
    assert_eq!(util.file.state, FileState::LastGood);
    assert!(util.file.failure.is_some());
    assert_eq!(util.nodes, file(&first, "lib/util.ts").nodes);
    assert_eq!(util.file.hash, file(&first, "lib/util.ts").file.hash);
    assert!(resolved(&helper_call(&broken)));
    assert!(broken.complete, "last-good files are indexed");
    assert_eq!(paths(&broken.summary.changed_files), ["lib/util.ts"]);
    assert!(broken.summary.changed_keys.is_empty(), "the nodes didn't change");

    folder.write("lib/util.ts", &template(0, 1));
    let fixed = background(&mut engine, &["lib/util.ts"]);
    assert_eq!(file(&fixed, "lib/util.ts").file.state, FileState::Current);
    assert_eq!(fixed.graph, first.graph);
}

#[test]
fn a_file_that_never_parsed_is_failed_and_the_first_compile_goes_on() {
    let folder = Folder::new();
    folder.write("lib/util.ts", "(call (ref x)");
    folder.write("main.ts", &template(1, 0));
    std::fs::write(folder.path().join("latin.ts"), b"(fn caf\xe9)").expect("write");
    let mut engine = engine(&folder);
    let outcome = compile(&mut engine);
    assert!(!outcome.complete);
    assert!(!engine.is_complete());
    let util = file(&outcome, "lib/util.ts");
    assert_eq!(util.file.state, FileState::Failed);
    assert_eq!(util.file.hash, None);
    assert!(util.nodes.is_empty() && util.links.is_empty());
    assert_eq!(helper_call(&outcome), LinkTarget::Unknown(Reason::NotIndexed));
    let latin = file(&outcome, "latin.ts");
    assert_eq!(latin.file.state, FileState::Failed);
    assert_eq!(latin.file.failure.as_deref(), Some("invalid UTF-8 at byte 7"));
    assert_eq!(file(&outcome, "main.ts").file.state, FileState::Current);
}

#[test]
fn changed_paths_add_remove_and_delete_folders() {
    let folder = Folder::new();
    folder.write("main.ts", &template(1, 0));
    let mut engine = engine(&folder);
    let first = compile(&mut engine);
    assert_eq!(helper_call(&first), LinkTarget::Unknown(Reason::NotFound));

    folder.write("lib/util.ts", &template(0, 1));
    folder.write("lib/base.ts", &template(2, 1));
    let added = background(&mut engine, &["lib/util.ts", "lib/base.ts"]);
    assert!(!added.stats.full_discovery);
    assert_eq!(added.stats.files_read, 2);
    assert!(resolved(&helper_call(&added)));
    assert_eq!(
        paths(&added.summary.changed_files),
        ["lib/base.ts", "lib/util.ts", "main.ts"]
    );

    folder.remove("lib");
    let removed = background(&mut engine, &["lib"]);
    assert_eq!(
        paths(&removed.summary.removed_files),
        ["lib/base.ts", "lib/util.ts"]
    );
    assert_eq!(removed.stats.files_removed, 2);
    assert_eq!(removed.graph, first.graph);

    // A path that changed nothing that shapes the graph keeps the graph as it is.
    folder.touch("main.ts");
    let touched = background(&mut engine, &["main.ts"]);
    assert!(!touched.stats.relinked);
    assert_eq!(touched.stats.files_read, 1);
    assert_eq!(touched.stats.files_extracted, 0);
    assert!(touched.summary.is_empty());
    assert!(Arc::ptr_eq(&touched.graph, &touched.previous));

    // Boards under `.unfold/` never affect the graph or force a full discovery.
    folder.write(".unfold/boards/b.json", "{}");
    let board = background(&mut engine, &[".unfold/boards/b.json", ".unfold/boards"]);
    assert!(!board.stats.full_discovery && !board.stats.relinked);
}

#[test]
fn ignored_and_unindexed_paths_follow_discovery() {
    let folder = Folder::new();
    folder.write("main.ts", &template(1, 0));
    folder.write("lib/util.ts", &template(0, 1));
    let mut engine = engine(&folder);
    compile(&mut engine);
    // An ignore file changes the scope of other files, so the update discovers everything.
    folder.write(".gitignore", "lib/\n");
    let ignored = background(&mut engine, &[".gitignore"]);
    assert!(ignored.stats.full_discovery);
    assert_eq!(paths(&ignored.summary.removed_files), ["lib/util.ts"]);
    assert_eq!(helper_call(&ignored), LinkTarget::Unknown(Reason::NotFound));
    // Settings that exclude a file make it unindexed: references to it are `not_indexed`.
    folder.write(".gitignore", "");
    folder.write("unfold.toml", "[scope]\nexclude = [\"lib/**\"]\n");
    let excluded = background(&mut engine, &[".gitignore", "unfold.toml"]);
    assert_eq!(helper_call(&excluded), LinkTarget::Unknown(Reason::NotIndexed));
    assert_eq!(
        excluded.graph.inputs.len(),
        1,
        "the settings file shaped the graph"
    );
}

#[test]
fn a_git_exclude_change_discovers_everything() {
    let folder = Folder::new();
    folder.write("main.ts", &template(1, 0));
    folder.write("lib/util.ts", &template(0, 1));
    folder.write(".git/info/exclude", "");
    let mut engine = engine(&folder);
    compile(&mut engine);
    // Nothing else under `.git` is ever in scope, but its exclude file is an ignore file.
    folder.write(".git/info/exclude", "lib/\n");
    let excluded = background(&mut engine, &[".git/info/exclude"]);
    assert!(excluded.stats.full_discovery);
    assert_eq!(paths(&excluded.summary.removed_files), ["lib/util.ts"]);
    assert_eq!(helper_call(&excluded), LinkTarget::Unknown(Reason::NotFound));
    // The rest of `.git` changes nothing.
    folder.write(".git/HEAD", "ref: refs/heads/main\n");
    let other = background(&mut engine, &[".git/HEAD"]);
    assert!(!other.stats.full_discovery && other.summary.is_empty());
}

#[test]
fn what_discovery_leaves_out_is_listed_and_makes_the_graph_incomplete() {
    let folder = Folder::new();
    folder.write("main.ts", &template(1, 0));
    let mut engine = engine(&folder);
    assert!(compile(&mut engine).complete);
    folder.write(".gitignore", "dist/\n[z-a]\n");
    folder.write("odd\\name.ts", "");
    let outcome = background(&mut engine, &[".gitignore"]);
    assert!(!outcome.complete);
    let problems: Vec<(ProblemKind, &str)> = outcome
        .problems
        .iter()
        .map(|problem| (problem.kind, &*problem.path))
        .collect();
    assert_eq!(
        problems,
        [
            (ProblemKind::Discovery, ""),
            (ProblemKind::Discovery, ".gitignore")
        ],
        "{:?}",
        outcome.problems
    );
    folder.write(".gitignore", "dist/\n");
    folder.remove("odd\\name.ts");
    let fixed = background(&mut engine, &[".gitignore"]);
    assert!(fixed.complete && fixed.problems.is_empty());
}

#[test]
fn invalid_settings_keep_the_previous_ones() {
    let folder = Folder::new();
    folder.write("main.ts", &template(1, 0));
    folder.write("lib/util.ts", &template(0, 1));
    folder.write("unfold.toml", "[scope]\nexclude = [\"lib/**\"]\n");
    let mut engine = engine(&folder);
    let first = compile(&mut engine);
    assert!(first.complete && first.problems.is_empty());
    folder.write("unfold.toml", "[scope\n");
    let invalid = background(&mut engine, &["unfold.toml"]);
    assert!(!invalid.complete);
    assert_eq!(invalid.problems.len(), 1, "{:?}", invalid.problems);
    assert_eq!(invalid.graph, first.graph);
    // A first compile with invalid settings uses the defaults.
    let mut fresh = super::engine(&folder);
    let defaults = compile(&mut fresh);
    assert!(defaults.graph.file("lib/util.ts").is_some());
    assert!(defaults.graph.inputs.is_empty());
}

/// Cancelling in each late stage (link, resolve, validate) stops the update promptly
/// and leaves the previous graph; a prepared update the engine never adopts changes nothing
/// either, and one prepared against an older state can't be adopted.
#[test]
fn cancelling_late_stages_is_prompt_and_keeps_the_previous_graph() {
    let folder = Folder::new();
    for index in 0..40 {
        folder.write(&format!("gen/f{index}.ts"), &template(8 + index, 1));
    }
    folder.write("main.ts", &template(1, 0));
    let language = Counting::new();
    let mut engine = engine_with(
        &folder,
        Arc::clone(&language),
        Arc::new(MemoryFactsCache::new()),
        2,
    );
    compile(&mut engine);
    folder.write("main.ts", &template(2, 0));
    let before = Arc::clone(engine.graph());

    for stage in [Stage::Link, Stage::Resolve, Stage::Validate] {
        let cancel = CancelToken::new();
        language.trip.arm(stage, cancel.clone());
        let result = engine.update(UpdateRequest::compile().with_cancel(cancel));
        let fired = language
            .trip
            .fired()
            .unwrap_or_else(|| panic!("{stage:?} was reached"));
        assert!(
            matches!(result, Err(UpdateError::Cancelled)),
            "{stage:?}: {result:?}"
        );
        let acknowledged = fired.elapsed();
        assert!(
            acknowledged < Duration::from_millis(250),
            "{stage:?}: {acknowledged:?}"
        );
        assert!(
            Arc::ptr_eq(engine.graph(), &before),
            "{stage:?} kept the previous graph"
        );
    }

    // Prepared but not adopted: nothing changes.
    let prepared = engine.prepare(UpdateRequest::compile()).expect("prepare");
    assert!(!prepared.outcome().summary.changed_files.is_empty());
    drop(prepared);
    assert!(Arc::ptr_eq(engine.graph(), &before));

    // Adopting an update prepared before another one was adopted is refused.
    let stale = engine.prepare(UpdateRequest::compile()).expect("prepare");
    let fresh = engine.prepare(UpdateRequest::compile()).expect("prepare");
    let outcome = engine.adopt(fresh).expect("adopt");
    assert!(Arc::ptr_eq(engine.graph(), &outcome.graph));
    assert!(matches!(engine.adopt(stale), Err(UpdateError::Superseded)));
    assert!(Arc::ptr_eq(engine.graph(), &outcome.graph));
}

#[test]
fn a_cancelled_update_changes_nothing() {
    let folder = Folder::new();
    for index in 0..40 {
        folder.write(&format!("gen/f{index}.ts"), &template(8 + index, 1));
    }
    folder.write("lib/util.ts", &template(0, 1));
    let language = Arc::new(Counting {
        delay: Duration::from_millis(5),
        ..Counting::default()
    });
    let mut engine = engine_with(
        &folder,
        Arc::clone(&language),
        Arc::new(MemoryFactsCache::new()),
        2,
    );
    let before = Arc::clone(engine.graph());

    let cancel = CancelToken::new();
    let cancelled_at = std::sync::Mutex::new(None);
    let progress = |progress: unfold_engine::Progress| {
        if progress.stage == Stage::Extract && progress.done >= 2 && !cancel.is_cancelled() {
            *cancelled_at.lock().expect("lock") = Some(Instant::now());
            cancel.cancel();
        }
    };
    let request = UpdateRequest::compile()
        .with_cancel(cancel.clone())
        .with_progress(&progress);
    let result = engine.update(request);
    let acknowledged = cancelled_at
        .lock()
        .expect("lock")
        .map(|at| at.elapsed())
        .expect("cancelled during extraction");
    assert!(matches!(result, Err(UpdateError::Cancelled)), "{result:?}");
    assert!(acknowledged < Duration::from_millis(250), "{acknowledged:?}");
    assert!(language.count() < 41, "extraction stopped early");
    assert!(Arc::ptr_eq(engine.graph(), &before));

    // A token cancelled before the update starts stops it at once.
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = engine.update(UpdateRequest::compile().with_cancel(cancel));
    assert!(matches!(result, Err(UpdateError::Cancelled)));

    // The next update is a full compile as if nothing happened.
    let outcome = compile(&mut engine);
    assert_eq!(outcome.graph.files.len(), 41);
    assert!(outcome.previous.files.is_empty());
}

#[test]
fn an_engine_error_fails_the_update_and_keeps_the_previous_state() {
    let folder = Folder::new();
    folder.write("lib/util.ts", &template(0, 1));
    folder.write("main.ts", &template(1, 0));
    let language = Counting::new();
    let mut engine = engine_with(
        &folder,
        Arc::clone(&language),
        Arc::new(MemoryFactsCache::new()),
        2,
    );
    let first = compile(&mut engine);
    language.break_link.store(true, Ordering::SeqCst);
    folder.write("lib/util.ts", &template(2, 1));
    let result = engine.update(UpdateRequest::background(vec![
        unfold_core::RepoPath::new("lib/util.ts").expect("path"),
    ]));
    assert!(matches!(result, Err(UpdateError::Backend(_))), "{result:?}");
    assert!(Arc::ptr_eq(engine.graph(), &first.graph));
    language.break_link.store(false, Ordering::SeqCst);
    // The failed update committed nothing, so the change is still pending and gets applied.
    let retried = background(&mut engine, &["lib/util.ts"]);
    assert_eq!(
        retried.summary.changed_files.first().map(|path| path.as_str()),
        Some("lib/util.ts")
    );
}

/// The engine validates every graph with each language's census: a census that finds a call no
/// node is fails the update, and the previous graph stays.
#[test]
fn the_census_runs_on_every_update() {
    let folder = Folder::new();
    folder.write("lib/util.ts", &template(0, 1));
    folder.write("main.ts", &template(1, 0));
    let language = Counting::new();
    let mut engine = engine_with(
        &folder,
        Arc::clone(&language),
        Arc::new(MemoryFactsCache::new()),
        2,
    );
    let first = compile(&mut engine);
    language.skew_census.store(true, Ordering::SeqCst);
    let result = engine.update(UpdateRequest::compile());
    let Err(UpdateError::Validation(violations)) = result else {
        panic!("{result:?}");
    };
    assert!(
        violations
            .iter()
            .any(|violation| violation.message.contains("needs exactly one call node")),
        "{violations:?}"
    );
    assert!(Arc::ptr_eq(engine.graph(), &first.graph));
    language.skew_census.store(false, Ordering::SeqCst);
    assert_eq!(canonical(&compile(&mut engine)), canonical(&first));
}

#[test]
fn progress_counts_files() {
    let folder = Folder::new();
    for index in 0..12 {
        folder.write(&format!("f{index}.ts"), &template(8 + index, 0));
    }
    let mut engine = engine(&folder);
    let reports = std::sync::Mutex::new(Vec::new());
    let progress = |progress: unfold_engine::Progress| reports.lock().expect("lock").push(progress);
    engine
        .update(UpdateRequest::compile().with_progress(&progress))
        .expect("compile");
    let reports = reports.into_inner().expect("lock");
    for stage in [Stage::Snapshot, Stage::Extract] {
        let last = reports
            .iter()
            .rfind(|progress| progress.stage == stage)
            .expect("reports");
        assert_eq!((last.done, last.total), (12, 12), "{stage}");
    }
}

/// A changed path under a symlinked folder is out of scope, as a full discovery (which never
/// follows linked folders) decides; a background update reporting it agrees with a compile.
#[test]
fn changed_paths_under_a_symlinked_folder_stay_out_of_scope() {
    let folder = Folder::new();
    folder.write("real/a.ts", &template(0, 1));
    folder.write("main.ts", &template(1, 0));
    std::os::unix::fs::symlink("real", folder.path().join("link")).expect("a symlink");
    let mut engine = engine(&folder);
    let compiled = compile(&mut engine);
    assert!(compiled.graph.file("link/a.ts").is_none());
    let updated = background(&mut engine, &["link/a.ts"]);
    assert!(updated.graph.file("link/a.ts").is_none());
    assert_eq!(canonical(&updated), canonical(&compiled));
}
