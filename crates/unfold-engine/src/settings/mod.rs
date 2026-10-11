//! Project settings: the validated contents of `unfold.toml` (docs/engine.md#shared-config).
//!
//! [`Settings::load`] reads the file (defaults when it is absent) and [`Settings::parse`] checks
//! it strictly: unknown keys, wrong types, and invalid values are errors with a line and column.
//! The model is plain data plus the compiled scope matcher. Editing the file while preserving
//! comments is the workspace's job, not this module's.
//!
//! [`Settings::hash`] fingerprints everything that can change a compile; languages hash only the
//! part that affects one file, such as [`Settings::python_hash`], so a settings edit re-extracts
//! only the files it touches. The project's shared name ([`Settings::project_name`]) changes no
//! compile, so neither hash includes it.

mod evidence;
mod folder;
mod hash;
mod parse;
mod project;
mod scope;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Language};
use unfold_core::{ByteRange, RepoPath};

pub use evidence::Evidence;
pub use folder::{FolderPath, FolderPathError};
pub use hash::{SettingsHash, SettingsHasher};
pub use parse::SettingsError;
pub use project::{MAX_PROJECT_NAME, default_project_name, is_project_name};
pub use scope::{Scope, ScopePatternError, ScopeStatus};

use crate::launch::LaunchTarget;
use crate::locator::Locator;

/// Where the settings file lives, relative to the open folder.
pub const SETTINGS_PATH: &str = "unfold.toml";

/// The validated settings of one open folder. [`Settings::default`] is what an absent file means.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Settings {
    /// `[project] name`: the shared name other projects' boards use for this one, always a
    /// valid name ([`is_project_name`]). `None` means the default the workspace makes from the
    /// folder ([`default_project_name`]). Not part of [`Settings::hash`]: a rename re-extracts
    /// nothing.
    #[serde(skip)]
    pub project_name: Option<String>,
    /// The languages to graph, sorted and without duplicates. `None` means every supported
    /// language found in the folder.
    pub languages: Option<Vec<Language>>,
    /// Which files are graphed, on top of the default exclusions.
    pub scope: Scope,
    /// Detectors turned off, sorted. Never contains [`DetectorId::Manual`].
    pub disabled_detectors: Vec<DetectorId>,
    /// `[[python.project]]` tables, sorted by root (roots are distinct).
    pub python_projects: Vec<PythonProject>,
    /// `[[application]]` tables, in file order (IDs are distinct).
    pub applications: Vec<Application>,
    /// `[[entrance]]` tables, in file order (IDs are distinct).
    pub entrances: Vec<ManualEntrance>,
    /// `[[entrance_override]]` tables, in file order (matches are distinct).
    pub overrides: Vec<EntranceOverride>,
    /// `[evidence] files`: the files boards may cite, hashed on each update but never compiled.
    pub evidence: Evidence,
}

/// One `[[python.project]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonProject {
    /// The project folder, relative to the open folder (`.` is the folder itself).
    pub root: FolderPath,
    /// Import roots, already joined onto `root` and so relative to the open folder. `None` when
    /// the table doesn't set them: the Python front end then uses `src/` for a src layout, or
    /// the project folder. A source root may lie outside `[scope]`: its modules resolve as
    /// `not_indexed` instead of `not_found`.
    pub source_roots: Option<Vec<FolderPath>>,
    /// The Python grammar the project's files are parsed with.
    pub version: PythonVersion,
}

/// A Python grammar version Unfold can parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
pub enum PythonVersion {
    /// Python 3.12.
    #[serde(rename = "3.12")]
    V3_12,
    /// Python 3.13.
    #[serde(rename = "3.13")]
    V3_13,
    /// Python 3.14, the default.
    #[default]
    #[serde(rename = "3.14")]
    V3_14,
}

impl PythonVersion {
    /// Every supported version, oldest first.
    pub const ALL: &'static [PythonVersion] = &[Self::V3_12, Self::V3_13, Self::V3_14];

    /// The version as written in settings, such as `3.14`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V3_12 => "3.12",
            Self::V3_13 => "3.13",
            Self::V3_14 => "3.14",
        }
    }

    /// Parses `3.12`, `3.13`, or `3.14`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|version| version.as_str() == text)
    }
}

/// One `[[application]]` table: a declared group of launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Application {
    /// The ID entrances and overrides name it by.
    pub id: Box<str>,
    /// The display name. Never blank.
    pub name: Box<str>,
    /// A description, with its line breaks. Never blank.
    pub description: Option<Box<str>>,
}

/// One `[[entrance]]` table: an entrance the detectors can't see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManualEntrance {
    /// The ID its key derives from, so the key survives edits to everything else.
    pub id: Box<str>,
    /// The entrance kind.
    pub kind: EntranceKind,
    /// The declared `[[application]]` it belongs to.
    pub application: Option<Box<str>>,
    /// The trigger, if known. Never empty.
    pub trigger: Option<Box<str>>,
    /// The HTTP method; only `http` entrances have one.
    pub method: Option<HttpMethod>,
    /// A display label, which may be empty.
    pub label: Option<Box<str>>,
    /// A description, with its line breaks. Never blank.
    pub description: Option<Box<str>>,
    /// The handlers, or what the process runs.
    pub targets: ManualTargets,
    /// The zero-based position of this `[[entrance]]` table in the file; the evidence pointer is
    /// `/entrance/<index>` (see [`ManualEntrance::pointer`]).
    pub index: usize,
    /// The evidence range: from the `[[` of the table's header through the end of its last value,
    /// token-exact (no trailing comment or whitespace). Byte offsets into the settings file.
    /// Not part of the settings hash.
    #[serde(skip)]
    pub span: ByteRange,
    /// The one-based line of the table's header, for diagnostics. Not part of the settings hash.
    #[serde(skip)]
    pub line: u32,
}

/// What a manual entrance points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ManualTargets {
    /// `handler`: one locator or a nonempty ordered list (middleware first) for every kind
    /// except `process`.
    Handlers(Vec<Locator>),
    /// What a `process` entrance runs.
    Process(Process),
}

/// The process fields of a `process` entrance. At least one of `command` and `start` is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Process {
    /// `cwd`: the folder the command runs in; `.` when omitted.
    pub cwd: FolderPath,
    /// `command`: the command line as written. Never blank, and never run.
    pub command: Option<Box<str>>,
    /// `startup` or `target`: what the process starts, declared by hand.
    pub start: Option<ProcessStart>,
    /// `python_paths`: the import path, in search order, that replaces the inferred one.
    pub python_paths: Option<Vec<FolderPath>>,
    /// `env`: the declared environment, by name. The host's environment is never read.
    pub env: BTreeMap<Box<str>, Box<str>>,
    /// `schedule`, as written. Never blank.
    pub schedule: Option<Box<str>>,
}

/// What a `process` entrance declares it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ProcessStart {
    /// `startup`: a locator.
    Startup(Locator),
    /// `target`: a symbolic launch target.
    Target(LaunchTarget),
}

/// One `[[entrance_override]]` table: metadata for a detected launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntranceOverride {
    /// `match`: the detected launch's evidence.
    pub selector: OverrideMatch,
    /// The declared `[[application]]` it belongs to.
    pub application: Option<Box<str>>,
    /// A display label.
    pub label: Option<Box<str>>,
    /// A description, with its line breaks. Never blank.
    pub description: Option<Box<str>>,
    /// A schedule, as written. Never blank.
    pub schedule: Option<Box<str>>,
    /// The one-based line of the table's header, for diagnostics. Not part of the settings hash.
    #[serde(skip)]
    pub line: u32,
}

/// The evidence an override matches: a launch detector, its input file, and the pointer into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OverrideMatch {
    /// `package.script`, `package.bin`, or `pyproject.script`.
    pub detector: DetectorId,
    /// The input file, such as `apps/api/package.json`.
    pub path: RepoPath,
    /// The RFC 6901 pointer to the launch's member, such as `/scripts/start`.
    pub pointer: Box<str>,
}

impl ManualEntrance {
    /// The evidence pointer, `/entrance/<index>`.
    pub fn pointer(&self) -> String {
        format!("/entrance/{}", self.index)
    }
}

impl Settings {
    /// Reads `unfold.toml` under `root`. An absent file means the defaults.
    pub fn load(root: &Path) -> Result<Self, SettingsError> {
        let path = root.join(SETTINGS_PATH);
        match std::fs::read(&path) {
            Ok(bytes) => Self::parse_bytes(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(SettingsError::Read { path, source }),
        }
    }

    /// Parses the settings file's bytes, which must be UTF-8. Spans and error positions are
    /// offsets into these bytes.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, SettingsError> {
        let text = std::str::from_utf8(bytes).map_err(|error| SettingsError::Encoding {
            offset: error.valid_up_to(),
        })?;
        Self::parse(text)
    }

    /// Parses and validates the settings file's text.
    pub fn parse(text: &str) -> Result<Self, SettingsError> {
        parse::parse(text)
    }

    /// Whether `language` is graphed.
    pub fn language_enabled(&self, language: Language) -> bool {
        self.languages
            .as_ref()
            .is_none_or(|languages| languages.contains(&language))
    }

    /// Whether `detector` runs. `manual` always does.
    pub fn detector_enabled(&self, detector: DetectorId) -> bool {
        detector == DetectorId::Manual || !self.disabled_detectors.contains(&detector)
    }

    /// The configured Python project that owns `path`: the one with the deepest root containing
    /// it. `None` when no configured project does.
    pub fn python_project_for(&self, path: &RepoPath) -> Option<&PythonProject> {
        self.python_projects
            .iter()
            .filter(|project| project.root.contains(path))
            .max_by_key(|project| project.root.depth())
    }

    /// The grammar version for a Python file: its project's, or the default.
    pub fn python_version_for(&self, path: &RepoPath) -> PythonVersion {
        self.python_project_for(path)
            .map(|project| project.version)
            .unwrap_or_default()
    }

    /// The hash of everything in the settings that can change a compile. Spans and the project
    /// name are left out, so editing comments, reordering whitespace, or renaming the project
    /// doesn't change it.
    pub fn hash(&self) -> SettingsHash {
        let mut hasher = SettingsHasher::new("settings");
        hasher.add(self);
        hasher.finish()
    }

    /// The hash of the settings that affect extracting one Python file: its owning project
    /// (root, source roots, and grammar version), or the absence of one.
    pub fn python_hash(&self, path: &RepoPath) -> SettingsHash {
        let mut hasher = SettingsHasher::new("python.project");
        hasher.add(&self.python_project_for(path));
        hasher.finish()
    }
}

#[cfg(test)]
mod tests;
