//! What a commit or a failure tells the windows (docs/engine.md#updates step 5, §7.9, §12).

use unfold_core::vocab::FileState;
use unfold_core::{Key, RepoPath};
use unfold_engine::UpdateOutcome;
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
/// periodic scan, `files_changed`.
pub(crate) fn committed(
    shared: &Shared,
    outcome: &UpdateOutcome,
    first: bool,
    rescanned: bool,
) -> CompileOutcome {
    let graph = &outcome.graph;
    shared
        .reader_cache
        .install(ReaderIndex::from_graph(graph, &outcome.launches));
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
        let listed = summary.changed_files.len()
            + summary.removed_files.len()
            + summary.changed_keys.len()
            + summary.changed_entrances.len();
        let everything = first || listed > MAX_LISTED;
        let list = |items: Vec<String>| if everything { Vec::new() } else { items };
        shared.emit(&WorkspaceEvent::GraphUpdated {
            project: shared.project.clone(),
            changed_files: list(strings(&summary.changed_files)),
            removed_files: list(strings(&summary.removed_files)),
            changed_keys: list(keys(&summary.changed_keys)),
            changed_entrances: list(keys(&summary.changed_entrances)),
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
        changed_entrances: u32::try_from(summary.changed_entrances.len()).unwrap_or(u32::MAX),
        complete: outcome.complete,
        problems: problem_count,
        duration_ms: u64::try_from(outcome.stats.total.as_millis()).unwrap_or(u64::MAX),
    }
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
