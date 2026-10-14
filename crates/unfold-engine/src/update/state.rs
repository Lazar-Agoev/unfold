//! The engine's long-lived state for one folder: every file's facts and disk state, the input
//! fingerprints, the effective settings, and the committed graph.
//!
//! An update clones the state (facts are shared through `Arc`, so this is cheap), changes the
//! clone, and the engine swaps it in only when the whole update succeeds.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use unfold_core::vocab::{FileState, InputRole, Language as LanguageId};
use unfold_core::{ContentHash, File, Graph, RepoPath, Snapshot};

use super::{LinkBasis, Problem};
use crate::discovery::FileStamp;
use crate::facts::FileFacts;
use crate::language::ProjectInputs;
use crate::settings::{Settings, SettingsHash};

/// What the engine last saw on disk for a file: the cheap check (size and modification time)
/// and the hash of the bytes, which differs from the facts' hash for a `last_good` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskState {
    /// Size and modification time at the last check, if the file could be examined.
    pub stamp: Option<FileStamp>,
    /// The SHA-256 of the bytes on disk, if they could be read.
    pub hash: Option<ContentHash>,
}

/// Where a file's facts come from.
#[derive(Debug, Clone)]
pub(crate) enum Facts {
    /// From the bytes on disk.
    Current {
        hash: ContentHash,
        facts: Arc<FileFacts>,
    },
    /// From the last version that parsed; the bytes on disk don't parse.
    LastGood {
        hash: ContentHash,
        facts: Arc<FileFacts>,
        failure: Box<str>,
    },
    /// No version has parsed.
    Failed { failure: Box<str> },
}

impl Facts {
    /// The facts the program uses, if any.
    pub(crate) fn program(&self) -> Option<Arc<FileFacts>> {
        match self {
            Self::Current { facts, .. } | Self::LastGood { facts, .. } => Some(Arc::clone(facts)),
            Self::Failed { .. } => None,
        }
    }

    /// The last parsed facts with the hash they came from, if any.
    pub(crate) fn good(&self) -> Option<(ContentHash, &Arc<FileFacts>)> {
        match self {
            Self::Current { hash, facts } | Self::LastGood { hash, facts, .. } => Some((*hash, facts)),
            Self::Failed { .. } => None,
        }
    }

    pub(crate) fn state(&self) -> FileState {
        match self {
            Self::Current { .. } => FileState::Current,
            Self::LastGood { .. } => FileState::LastGood,
            Self::Failed { .. } => FileState::Failed,
        }
    }

    /// Whether two states give the graph the same file record and facts.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Current { hash: a, facts: x }, Self::Current { hash: b, facts: y }) => {
                a == b && Arc::ptr_eq(x, y)
            }
            (
                Self::LastGood {
                    hash: a,
                    facts: x,
                    failure: f,
                },
                Self::LastGood {
                    hash: b,
                    facts: y,
                    failure: g,
                },
            ) => a == b && Arc::ptr_eq(x, y) && f == g,
            (Self::Failed { failure: f }, Self::Failed { failure: g }) => f == g,
            _ => false,
        }
    }
}

/// One indexed source file.
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) language: LanguageId,
    /// The settings hash its facts were extracted under.
    pub(crate) settings: SettingsHash,
    pub(crate) disk: DiskState,
    pub(crate) facts: Facts,
}

impl FileEntry {
    pub(crate) fn record(&self, path: &RepoPath) -> File {
        let (hash, failure) = match &self.facts {
            Facts::Current { hash, .. } => (Some(*hash), None),
            Facts::LastGood { hash, failure, .. } => (Some(*hash), Some(failure.clone())),
            Facts::Failed { failure } => (None, Some(failure.clone())),
        };
        File {
            path: path.clone(),
            language: self.language,
            state: self.facts.state(),
            hash,
            failure,
        }
    }
}

/// One configuration input other than the settings file.
#[derive(Debug, Clone)]
pub(crate) struct InputEntry {
    pub(crate) role: InputRole,
    pub(crate) disk: DiskState,
    /// Its bytes, or why they couldn't be read.
    pub(crate) content: Result<Snapshot, Box<str>>,
    /// Whether a language asked for it ([`crate::Language::requested_inputs`]) rather than
    /// discovery finding it.
    pub(crate) requested: bool,
}

/// The effective settings and where they came from.
#[derive(Debug, Clone, Default)]
pub(crate) struct SettingsState {
    pub(crate) settings: Settings,
    /// The bytes of `unfold.toml` the settings were parsed from; `None` for defaults.
    pub(crate) source: Option<Snapshot>,
    /// The settings file's stamp at the last check (`None` when absent or unchecked).
    pub(crate) stamp: Option<FileStamp>,
    /// Why the file on disk couldn't be used. The previous valid settings (or the defaults)
    /// stay in effect meanwhile.
    pub(crate) problem: Option<Problem>,
}

/// Everything the engine keeps for one folder between updates.
#[derive(Debug, Clone, Default)]
pub(crate) struct State {
    pub(crate) settings: SettingsState,
    pub(crate) files: BTreeMap<RepoPath, FileEntry>,
    pub(crate) inputs: BTreeMap<RepoPath, InputEntry>,
    /// Existing source-like files that aren't indexed, including sources no registered
    /// language owns.
    pub(crate) unindexed: BTreeSet<RepoPath>,
    /// Context files (declaration files) discovery found.
    pub(crate) context: BTreeSet<RepoPath>,
    /// What the last full discovery couldn't read.
    pub(crate) discovery_problems: Vec<Problem>,
    /// What the languages found wrong with the configuration inputs
    /// ([`crate::Language::input_problems`]).
    pub(crate) input_problems: Vec<Problem>,
    /// What the languages found while linking the current graph ([`crate::Linked::problems`]),
    /// with whether each one makes the graph incomplete. Kept until the next link.
    pub(crate) link_problems: Vec<(Problem, bool)>,
    /// Whether a full discovery has run, so that changed paths can be applied to its result.
    pub(crate) discovered: bool,
    /// Whether `graph` came from an update or a restore (rather than being the initial empty
    /// graph).
    pub(crate) built: bool,
    /// Whether `graph` was linked by this engine. A restored graph wasn't: the first update
    /// after a restore links it again unless it sees exactly what the graph was linked from
    /// (`restored`).
    pub(crate) linked: bool,
    /// For a restored graph, what it was linked from, when the store kept it.
    pub(crate) restored: Option<LinkBasis>,
    pub(crate) graph: Arc<Graph>,
    /// What the languages saw beside the files at the last update or restore.
    pub(crate) project: Arc<ProjectInputs>,
    pub(crate) complete: bool,
}
