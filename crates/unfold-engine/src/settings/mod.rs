//! Project settings: the validated contents of `unfold.toml` (docs/engine.md#shared-config).
//!
//! [`Settings::load`] reads the file (defaults when it is absent) and [`Settings::parse`] checks
//! it strictly: unknown keys, wrong types, and invalid values are errors with a line and column.
//! The model is plain data plus the compiled scope matcher. Editing the file while preserving
//! comments is the workspace's job, not this module's.
//!
//! [`Settings::hash`] fingerprints everything that can change a compile; languages hash only the
//! part that affects one file, such as [`Settings::python_hash`], so a settings edit re-extracts
//! only the files it touches.

mod evidence;
mod folder;
mod hash;
mod parse;
mod scope;

use std::path::Path;

use serde::Serialize;
use unfold_core::RepoPath;
use unfold_core::vocab::Language;

pub use evidence::Evidence;
pub use folder::{FolderPath, FolderPathError};
pub use hash::{SettingsHash, SettingsHasher};
pub use parse::SettingsError;
pub use scope::{Scope, ScopePatternError, ScopeStatus};

/// Where the settings file lives, relative to the open folder.
pub const SETTINGS_PATH: &str = "unfold.toml";

/// The validated settings of one open folder. [`Settings::default`] is what an absent file means.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Settings {
    /// The languages to graph, sorted and without duplicates. `None` means every supported
    /// language found in the folder.
    pub languages: Option<Vec<Language>>,
    /// Which files are graphed, on top of the default exclusions.
    pub scope: Scope,
    /// `[[python.project]]` tables, sorted by root (roots are distinct).
    pub python_projects: Vec<PythonProject>,
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
