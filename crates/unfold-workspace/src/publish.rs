//! What a commit or a failure tells the windows (docs/engine.md#updates step 5, §7.9, §12).

use unfold_core::vocab::FileState;
use unfold_core::{Key, RepoPath};
use unfold_engine::{EntrancesOutcome, UpdateOutcome};
use unfold_reader::ReaderIndex;

use crate::compile::CompileOutcome;
use crate::error::WorkspaceError;
use crate::events::{MAX_LISTED, WorkspaceEvent};
use crate::problems::{ProblemCategory, ProblemItem, graph_problems, project_problem_count};
use crate::shared::{Shared, lock};
use crate::status::{Activity, GraphCounts, WatchMode};
use crate::watcher;

/// Installs the new reader index and tells the windows what changed: `graph_updated` (with
/// everything changed after a full commit), `problems_changed`, `project_state`, and, for a
/// periodic scan, `files_changed`. (The engine thread publishes the entrances, which sends
/// `entrances_changed` when they changed.)
pub(crate) fn committed(
    shared: &Shared,
    outcome: &UpdateOutcome,
    first: bool,
    rescanned: bool,
) -> CompileOutcome {
    let graph = &outcome.graph;
    shared.reader_cache.install(ReaderIndex::from_graph(graph));
    let mut languages: Vec<_> = graph.files.iter().map(|file| file.file.language).collect();
    languages.sort();
    languages.dedup();
    let counts = GraphCounts {
        files: u32::try_from(graph.files.len()).unwrap_or(u32::MAX),
        not_current: u32::try_from(
            graph
                .files
                .iter()
                .filter(|file| file.file.state != FileState::Current)
                .count(),
        )
        .unwrap_or(u32::MAX),
        languages,
    };
    let watching = {
        let mut sched = lock(&shared.sched);
        sched.compiled = true;
        sched.watch
    };
    let problems = graph_problems(&shared.root, graph, &outcome.problems);
    shared.update_problems(|all| {
        all.graph = problems;
        all.compile = None;
    });
    let summary = &outcome.summary;
    if first || !summary.is_empty() {
        let strings = |paths: &[RepoPath]| {
            paths
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        let keys = |keys: &[Key]| keys.iter().map(|key| key.as_str().to_owned()).collect::<Vec<_>>();
        let listed = summary.changed_files.len() + summary.removed_files.len() + summary.changed_keys.len();
        let everything = first || listed > MAX_LISTED;
        let list = |items: Vec<String>| if everything { Vec::new() } else { items };
        shared.emit(&WorkspaceEvent::GraphUpdated {
            project: shared.project.clone(),
            changed_files: list(strings(&summary.changed_files)),
            removed_files: list(strings(&summary.removed_files)),
            changed_keys: list(keys(&summary.changed_keys)),
            inputs_changed: summary.inputs_changed,
            everything,
            complete: outcome.complete,
        });
        if rescanned && watching == WatchMode::Scanning {
            let mut paths = strings(&summary.changed_files);
            paths.extend(strings(&summary.removed_files));
            paths.sort();
            watcher::emit_files_changed(shared, paths);
        }
    }
    let problem_count = project_problem_count(&shared.problems());
    shared.update_status(|model| {
        model.compiled = true;
        model.failure = None;
        model.complete = outcome.complete;
        model.counts = counts.clone();
        model.activity = Activity::Idle;
    });
    CompileOutcome {
        files: counts.files,
        changed_files: u32::try_from(summary.changed_files.len()).unwrap_or(u32::MAX),
        removed_files: u32::try_from(summary.removed_files.len()).unwrap_or(u32::MAX),
        entrances_changed: summary.apps_changed,
        complete: outcome.complete,
        problems: problem_count,
        duration_ms: u64::try_from(outcome.stats.total.as_millis()).unwrap_or(u64::MAX),
    }
}

/// Tells the windows what re-resolving the entrances changed: a new reader index when the apps
/// changed (Reached from starts at their code), and the problems (`unfold.toml` may have stopped
/// parsing). `complete` is the engine's completeness now.
pub(crate) fn entrances_resolved(shared: &Shared, outcome: &EntrancesOutcome, complete: bool) {
    if outcome.apps_changed {
        shared
            .reader_cache
            .install(ReaderIndex::from_graph(&outcome.graph));
    }
    let problems = graph_problems(&shared.root, &outcome.graph, &outcome.problems);
    shared.update_problems(|all| all.graph = problems);
    shared.update_status(|model| model.complete = complete);
}

/// Records a failed compile: the status says so and Problems lists it.
pub(crate) fn failed(shared: &Shared, error: &WorkspaceError) {
    let message = error.to_string();
    shared.update_problems(|problems| {
        problems.compile = Some(ProblemItem::new(
            ProblemCategory::CompileFailed,
            "",
            message.clone(),
        ));
    });
    shared.update_status(|model| {
        model.failure = Some(message);
        model.activity = Activity::Idle;
    });
}
