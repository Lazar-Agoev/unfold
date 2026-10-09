//! What the status bar says (architecture §7.9).
//!
//! ```text
//! ● Graph current · 84 files · TypeScript + Python          current
//! ◌ Compiling · 12 of 84 files                              compiling
//! ◌ Updating · 2 files changed                              updating
//! ● 1 file doesn't parse · showing its last version …       doesnt_parse
//! ● Background updates paused · 3 files changed …           paused
//! ▲ Compile failed · see Problems                           failed
//! ```
//!
//! Besides the state, the status says whether `local/` is in the fallback location and how the
//! folder is watched (events, or a periodic scan). The workspace keeps a status model of the
//! facts behind it and derives the [`GraphStatus`] from them; a `graph_status` event goes out
//! when the derived status changes, ignoring progress counts (those travel in
//! `compile_progress`).

use serde::Serialize;
use ts_rs::TS;
use unfold_core::vocab::Language;

use crate::layout::FallbackReason;

/// The status-bar state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum GraphState {
    /// The folder has never been compiled on this machine.
    NotCompiled,
    /// The graph matches the folder.
    Current {
        /// Indexed files.
        files: u32,
        /// The languages graphed, sorted.
        languages: Vec<Language>,
    },
    /// A compile is running: `done` of `total` files.
    Compiling {
        /// Files finished.
        done: u32,
        /// Files to process (0 until discovery finishes).
        total: u32,
    },
    /// A background update is running.
    Updating {
        /// How many files changed, or `None` while checking the whole folder.
        files: Option<u32>,
    },
    /// Some files don't parse; the graph shows their last version that parsed, or leaves them
    /// out.
    DoesntParse {
        /// Files that don't parse or can't be read.
        files: u32,
        /// Indexed files.
        total: u32,
        /// The languages graphed, sorted.
        languages: Vec<Language>,
    },
    /// Background updates are paused.
    Paused {
        /// Paths changed since the last compile or update.
        changed: u32,
    },
    /// The last compile or update failed; the previous graph is kept.
    Failed {
        /// Why.
        message: String,
    },
}

/// How the folder is watched for changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WatchMode {
    /// File-system events.
    Events,
    /// A periodic scan, because events are unavailable or unreliable here.
    Scanning,
    /// Not watched.
    Off,
}

/// The status of one folder's graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GraphStatus {
    /// The status-bar state.
    pub state: GraphState,
    /// Why `local/` is in the fallback location, if it is ("using fallback location").
    pub fallback: Option<FallbackReason>,
    /// How the folder is watched ("watching by scanning").
    pub watching: WatchMode,
    /// Whether background updates are on.
    pub background_updates: bool,
    /// Whether every discovered file is indexed.
    pub complete: bool,
}

/// What the workspace is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Activity {
    /// Nothing.
    #[default]
    Idle,
    /// A compile, rebuild, or full recompile.
    Compiling { done: u32, total: u32 },
    /// A background update: `None` files while checking the whole folder.
    Updating { files: Option<u32> },
}

/// Counts from the committed graph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GraphCounts {
    pub(crate) files: u32,
    pub(crate) not_current: u32,
    pub(crate) languages: Vec<Language>,
}

/// The facts the status is derived from.
#[derive(Debug, Clone)]
pub(crate) struct StatusModel {
    pub(crate) compiled: bool,
    pub(crate) activity: Activity,
    pub(crate) failure: Option<String>,
    pub(crate) paused: bool,
    pub(crate) changed: u32,
    pub(crate) counts: GraphCounts,
    pub(crate) complete: bool,
    pub(crate) fallback: Option<FallbackReason>,
    pub(crate) watching: WatchMode,
}

impl StatusModel {
    pub(crate) fn new(watching: WatchMode) -> Self {
        Self {
            compiled: false,
            activity: Activity::Idle,
            failure: None,
            paused: false,
            changed: 0,
            counts: GraphCounts::default(),
            complete: false,
            fallback: None,
            watching,
        }
    }

    /// The status this model shows.
    pub(crate) fn status(&self) -> GraphStatus {
        let state = match self.activity {
            Activity::Compiling { done, total } => GraphState::Compiling { done, total },
            Activity::Updating { files } => GraphState::Updating { files },
            Activity::Idle => self.idle_state(),
        };
        GraphStatus {
            state,
            fallback: self.fallback,
            watching: self.watching,
            background_updates: !self.paused,
            complete: self.complete,
        }
    }

    fn idle_state(&self) -> GraphState {
        if let Some(message) = &self.failure {
            return GraphState::Failed {
                message: message.clone(),
            };
        }
        if !self.compiled {
            return GraphState::NotCompiled;
        }
        if self.paused {
            return GraphState::Paused {
                changed: self.changed,
            };
        }
        if self.counts.not_current > 0 {
            return GraphState::DoesntParse {
                files: self.counts.not_current,
                total: self.counts.files,
                languages: self.counts.languages.clone(),
            };
        }
        GraphState::Current {
            files: self.counts.files,
            languages: self.counts.languages.clone(),
        }
    }
}

/// Whether two statuses differ in more than progress counts.
pub(crate) fn differs_in_kind(a: &GraphStatus, b: &GraphStatus) -> bool {
    let progress = |status: &GraphStatus| matches!(status.state, GraphState::Compiling { .. });
    if progress(a) && progress(b) {
        return GraphStatus {
            state: GraphState::NotCompiled,
            ..a.clone()
        } != GraphStatus {
            state: GraphState::NotCompiled,
            ..b.clone()
        };
    }
    a != b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_status_bar_states() {
        let mut model = StatusModel::new(WatchMode::Events);
        assert_eq!(model.status().state, GraphState::NotCompiled);
        model.activity = Activity::Compiling { done: 3, total: 10 };
        assert_eq!(model.status().state, GraphState::Compiling { done: 3, total: 10 });
        model.activity = Activity::Idle;
        model.compiled = true;
        model.counts = GraphCounts {
            files: 10,
            not_current: 1,
            languages: vec![Language::TypeScript],
        };
        assert!(matches!(
            model.status().state,
            GraphState::DoesntParse { files: 1, .. }
        ));
        model.paused = true;
        model.changed = 2;
        assert_eq!(model.status().state, GraphState::Paused { changed: 2 });
        assert!(!model.status().background_updates);
        model.failure = Some("boom".into());
        assert!(matches!(model.status().state, GraphState::Failed { .. }));
        let json = serde_json::to_value(model.status()).expect("json");
        assert_eq!(json["state"]["kind"], "failed");
        assert_eq!(json["watching"], "events");
        assert_eq!(json["backgroundUpdates"], false);
    }

    #[test]
    fn progress_ticks_are_not_a_change_of_kind() {
        let mut model = StatusModel::new(WatchMode::Events);
        model.activity = Activity::Compiling { done: 1, total: 9 };
        let a = model.status();
        model.activity = Activity::Compiling { done: 5, total: 9 };
        assert!(!differs_in_kind(&a, &model.status()));
        model.activity = Activity::Idle;
        assert!(differs_in_kind(&a, &model.status()));
    }
}
