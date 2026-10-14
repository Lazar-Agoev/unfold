//! What an update did, and progress reports for streaming.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use unfold_core::vocab::{FileState, Language as LanguageId, Outcome, Reason};
use unfold_core::{Graph, LinkTarget};

use super::UpdateKind;

/// One stage of the pipeline (docs/engine.md#updates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Settings, then the files in scope: a full discovery or the changed paths.
    Discover,
    /// Reading and hashing changed files and inputs.
    Snapshot,
    /// Per-file facts, from the cache or the language front ends.
    Extract,
    /// Each language's cross-file indexes over the whole program.
    Link,
    /// Every link's outcome, and every argument's pairing.
    Resolve,
    /// The new graph against the core invariants and each language's census.
    Validate,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: &'static [Stage] = &[
        Self::Discover,
        Self::Snapshot,
        Self::Extract,
        Self::Link,
        Self::Resolve,
        Self::Validate,
    ];

    /// The stage's name in progress reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Snapshot => "snapshot",
            Self::Extract => "extract",
            Self::Link => "link",
            Self::Resolve => "resolve",
            Self::Validate => "validate",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A progress report: `done` of `total` items of `stage` (files for snapshot and extract).
/// Home's state line renders it as "Compiling · 12 of 84 files".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// The running stage.
    pub stage: Stage,
    /// Items finished.
    pub done: usize,
    /// Items in the stage.
    pub total: usize,
}

/// What one update did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateStats {
    /// Which action ran.
    pub kind: UpdateKind,
    /// Worker threads available to the update.
    pub threads: usize,
    /// Whether a full discovery ran (rather than applying changed paths).
    pub full_discovery: bool,
    /// Indexed source files after the update, by language.
    pub files: BTreeMap<LanguageId, usize>,
    /// Source files read and hashed.
    pub files_read: usize,
    /// Changed files whose facts came from the cache.
    pub files_from_cache: usize,
    /// Files the language front ends extracted.
    pub files_extracted: usize,
    /// Files in each state after the update.
    pub states: BTreeMap<FileState, usize>,
    /// Configuration inputs read and hashed.
    pub inputs_read: usize,
    /// Files no longer indexed.
    pub files_removed: usize,
    /// Time per stage; stages that didn't run are absent.
    pub stages: BTreeMap<Stage, Duration>,
    /// The whole update.
    pub total: Duration,
    /// Whether the graph was rebuilt (an update that changed no input keeps the graph).
    pub relinked: bool,
    /// Links by outcome.
    pub outcomes: BTreeMap<Outcome, usize>,
    /// Unknown links by reason.
    pub reasons: BTreeMap<Reason, usize>,
}

impl UpdateStats {
    /// Fills the graph-derived counts.
    pub(crate) fn count_graph(&mut self, graph: &Graph) {
        self.files.clear();
        self.states.clear();
        self.outcomes.clear();
        self.reasons.clear();
        for file in &graph.files {
            *self.files.entry(file.file.language).or_default() += 1;
            *self.states.entry(file.file.state).or_default() += 1;
            for link in &file.links {
                *self.outcomes.entry(link.to.outcome()).or_default() += 1;
                if let LinkTarget::Unknown(reason) = link.to {
                    *self.reasons.entry(reason).or_default() += 1;
                }
            }
        }
    }

    /// The number of links.
    pub fn links(&self) -> usize {
        self.outcomes.values().sum()
    }
}

fn counts<K: fmt::Display>(map: &BTreeMap<K, usize>) -> String {
    if map.is_empty() {
        return "none".to_owned();
    }
    map.iter()
        .map(|(key, count)| format!("{key} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for UpdateStats {
    /// The update, one fact per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let files: usize = self.files.values().sum();
        writeln!(
            f,
            "{} · {files} files ({}) · {} threads · {:.0?}",
            self.kind,
            counts(&self.files),
            self.threads,
            self.total
        )?;
        writeln!(
            f,
            "files: read {}, from cache {}, extracted {}, removed {}; inputs read {}; discovery {}",
            self.files_read,
            self.files_from_cache,
            self.files_extracted,
            self.files_removed,
            self.inputs_read,
            if self.full_discovery {
                "full"
            } else {
                "changed paths"
            }
        )?;
        writeln!(f, "states: {}", counts(&self.states))?;
        let stages = self
            .stages
            .iter()
            .map(|(stage, time)| format!("{stage} {time:.1?}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(f, "stages: {}", if stages.is_empty() { "none" } else { &stages })?;
        if !self.relinked {
            return writeln!(f, "graph unchanged: nothing that shapes it changed");
        }
        writeln!(f, "links: {} ({})", self.links(), counts(&self.outcomes))?;
        writeln!(f, "reasons: {}", counts(&self.reasons))
    }
}
