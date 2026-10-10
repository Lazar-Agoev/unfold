//! The facts cache and settings changes: unchanged bytes are never extracted again, and a
//! settings edit re-extracts exactly the files whose settings hash changed.

use std::sync::Arc;

use unfold_core::Graph;
use unfold_core::vocab::FileState;
use unfold_engine::update::{DiskState, MemoryFactsCache};
use unfold_engine::{Engine, LinkBasis, UpdateOutcome, UpdateRequest};

use super::{Counting, Folder, background, canonical, compile, engine_with, template};

fn project(folder: &Folder) {
    folder.write("main.ts", &template(1, 0));
    folder.write("lib/util.ts", &template(0, 1));
    for index in 0..4 {
        folder.write(&format!("pkg/f{index}.ts"), &template(8 + index, 1));
    }
}

#[test]
fn unchanged_files_come_from_the_cache() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let language = Counting::new();
    let mut engine = engine_with(&folder, Arc::clone(&language), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    assert_eq!(language.count(), 6);
    assert_eq!((first.stats.files_read, first.stats.files_extracted), (6, 6));

    // An explicit Compile re-reads every file but extracts none whose bytes are unchanged.
    let again = compile(&mut engine);
    assert_eq!(language.count(), 0);
    assert_eq!((again.stats.files_read, again.stats.files_extracted), (6, 0));
    assert!(again.summary.is_empty());

    // A new engine over the same cache (a new session) parses nothing.
    let mut second = engine_with(&folder, Arc::clone(&language), Arc::clone(&cache), 2);
    let cached = compile(&mut second);
    assert_eq!(language.count(), 0);
    assert_eq!(cached.stats.files_from_cache, 6);
    assert_eq!(canonical(&cached), canonical(&first));

    // Editing one file extracts that file only.
    folder.write("lib/util.ts", &template(2, 1));
    let edited = background(&mut engine, &["lib/util.ts"]);
    assert_eq!(language.count(), 1);
    assert_eq!(edited.stats.files_extracted, 1);
    // The cache keeps each path's current entry (and its last good one), so editing it back
    // extracts again.
    folder.write("lib/util.ts", &template(0, 1));
    let reverted = background(&mut engine, &["lib/util.ts"]);
    assert_eq!(language.count(), 1);
    assert_eq!(canonical(&reverted), canonical(&first));

    // Broken bytes seen before are not parsed again either.
    folder.write("lib/util.ts", "(fn");
    background(&mut engine, &["lib/util.ts"]);
    folder.write("lib/util.ts", &template(0, 1));
    background(&mut engine, &["lib/util.ts"]);
    folder.write("lib/util.ts", "(fn");
    let broken = background(&mut engine, &["lib/util.ts"]);
    assert_eq!(language.count(), 1, "only the first broken version was parsed");
    assert_eq!(
        broken.graph.file("lib/util.ts").map(|file| file.file.state),
        Some(FileState::LastGood)
    );

    // Rebuild from scratch ignores the cache.
    engine.update(UpdateRequest::rebuild()).expect("rebuild");
    assert_eq!(language.count(), 6);
}

#[test]
fn a_settings_change_re_extracts_exactly_the_files_it_touches() {
    let folder = Folder::new();
    project(&folder);
    let language = Arc::new(Counting {
        settings_aware: true,
        ..Counting::default()
    });
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Arc::clone(&language), cache, 2);
    let first = compile(&mut engine);
    assert_eq!(language.count(), 6);

    // The four files under `pkg/` get a new settings hash; the others keep theirs.
    folder.write(".unfold/config.toml", "[[python.project]]\nroot = \"pkg\"\n");
    let changed = background(&mut engine, &[".unfold/config.toml"]);
    assert!(changed.stats.full_discovery);
    assert_eq!(language.count(), 4);
    assert_eq!(changed.stats.files_read, 4);
    assert_eq!(changed.graph.inputs.len(), 1);
    assert!(changed.summary.inputs_changed);

    // A manual entrance changes no file's settings hash.
    folder.write(
        ".unfold/config.toml",
        "[[python.project]]\nroot = \"pkg\"\n\n[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"main.ts#go\"\n",
    );
    let entrance = background(&mut engine, &[".unfold/config.toml"]);
    assert_eq!(language.count(), 0);
    assert_eq!(entrance.graph.entrances.len(), 1);
    assert_eq!(entrance.summary.changed_entrances.len(), 1);

    // Removing the settings brings back the first hashes. The cache keeps only each path's
    // current entry, so those four files are extracted again.
    folder.remove(".unfold/config.toml");
    let removed = background(&mut engine, &[".unfold/config.toml"]);
    assert_eq!(language.count(), 4);
    assert_eq!(canonical(&removed), canonical(&first));
}

#[test]
fn reopening_restores_facts_and_checks_stamps() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let language = Counting::new();
    let mut engine = engine_with(&folder, Arc::clone(&language), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    language.count();
    let graph: Graph = (*first.graph).clone();
    let disks: Vec<(unfold_core::RepoPath, DiskState)> = graph
        .files
        .iter()
        .filter_map(|file| Some((file.file.path.clone(), engine.disk_state(&file.file.path)?)))
        .collect();
    folder.write("lib/util.ts", &template(2, 1));

    let mut reopened = engine_with(&folder, Arc::clone(&language), cache, 2);
    let restored = reopened.restore(graph, disks, None).expect("restore");
    assert_eq!((restored.files, restored.restored, restored.missing), (6, 6, 0));
    assert_eq!(reopened.graph().files.len(), 6);
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(
        outcome.stats.files_read, 1,
        "only the file whose stamp changed is read"
    );
    assert_eq!(language.count(), 1);
    assert_eq!(
        outcome.summary.changed_files.first().map(|path| path.as_str()),
        Some("lib/util.ts")
    );
}

/// A new session over `cache`, restored from `outcome` (an update of `engine`) with `basis`.
fn restored(
    folder: &Folder,
    cache: &Arc<MemoryFactsCache>,
    engine: &Engine,
    outcome: &UpdateOutcome,
    basis: Option<LinkBasis>,
) -> Engine {
    let graph: Graph = (*outcome.graph).clone();
    let disks: Vec<(unfold_core::RepoPath, DiskState)> = graph
        .files
        .iter()
        .filter_map(|file| Some((file.file.path.clone(), engine.disk_state(&file.file.path)?)))
        .collect();
    let mut reopened = engine_with(folder, Counting::new(), Arc::clone(cache), 2);
    reopened.restore(graph, disks, basis).expect("restore");
    reopened
}

/// When nothing the graph was linked from changed while the folder was closed, the reopen check
/// keeps the restored graph and the problems linking found, without linking again.
#[test]
fn a_restored_graph_is_kept_when_nothing_it_was_linked_from_changed() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    let mut reopened = restored(&folder, &cache, &engine, &first, Some(first.basis.clone()));
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(outcome.stats.files_read, 0);
    assert!(!outcome.stats.relinked, "the restored graph is kept");
    assert!(outcome.summary.is_empty());
    assert_eq!(canonical(&outcome), canonical(&first));
    assert_eq!(outcome.problems, first.problems);
    assert_eq!(outcome.complete, first.complete);
    assert_eq!(outcome.basis, first.basis);
}

/// Without a basis (the store didn't keep one), the first update after a restore links again:
/// the stored graph's link problems are unknown.
#[test]
fn a_restored_graph_without_its_basis_is_linked_again() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    let mut reopened = restored(&folder, &cache, &engine, &first, None);
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(outcome.stats.files_read, 0);
    assert!(outcome.stats.relinked, "a restored graph is relinked once");
    assert_eq!(canonical(&outcome), canonical(&first));
    let again = reopened.update(UpdateRequest::reopen()).expect("second check");
    assert!(!again.stats.relinked, "later unchanged updates keep the graph");
}

/// A change only linking sees, made while the folder was closed (a manual entrance changes no
/// file's facts), links the restored graph again, which then equals a full compile.
#[test]
fn a_restored_graph_is_linked_again_when_what_it_was_linked_from_changed() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    folder.write(
        ".unfold/config.toml",
        "[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"main.ts#go\"\n",
    );
    let mut reopened = restored(&folder, &cache, &engine, &first, Some(first.basis.clone()));
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert_eq!(outcome.stats.files_read, 0, "no file's facts changed");
    assert!(outcome.stats.relinked);
    assert_eq!(outcome.graph.entrances.len(), 1);
    assert_ne!(outcome.basis, first.basis);
    let mut fresh = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2);
    assert_eq!(canonical(&outcome), canonical(&compile(&mut fresh)));

    // An unindexed file appearing changes what linking sees too.
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let second = compile(&mut engine);
    folder.write("lib/notes.py", "x = 1\n");
    let mut reopened = restored(&folder, &cache, &engine, &second, Some(second.basis.clone()));
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert!(outcome.stats.relinked);
    let mut fresh = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2);
    assert_eq!(canonical(&outcome), canonical(&compile(&mut fresh)));
}
