//! Discovery and scope: which files in the open folder Unfold indexes, reads as context, knows
//! to exist, or fingerprints as configuration (docs/engine.md#updates, file discovery).
//!
//! [`discover`] walks the folder with the `ignore` crate, so `.gitignore`, `.ignore`,
//! `.git/info/exclude`, and the global Git excludes file apply exactly as in ripgrep, without
//! running Git and whether or not the folder is a repository. Folders named in
//! [`DEFAULT_EXCLUDED`] are never entered, symlinked folders are never followed, and a symlinked
//! file counts only when its target is a file inside the folder. `[scope]` and `languages` from
//! the settings then decide what is indexed; see [`classify`]. What the walk can't read or use
//! (an unreadable folder, an ignore rule that doesn't parse, a name that can't be a project path)
//! is left out and reported as a [`Problem`].
//!
//! The result is deterministic: every list is sorted by path.

mod classify;
mod walk;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};

pub use classify::{
    DEFAULT_EXCLUDED, FileClass, Unindexed, classify, is_default_excluded, is_default_excluded_name,
};
pub use walk::IgnoreRules;

use crate::settings::{ScopeStatus, Settings};

/// How to walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverOptions {
    /// Walker threads; `0` lets the walker choose from the number of CPUs.
    pub threads: usize,
    /// Whether the global Git excludes file (`core.excludesFile`, read from Git's config files)
    /// applies. Tests turn it off so a developer's own excludes can't change their results.
    pub global_ignore: bool,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            threads: 0,
            global_ignore: true,
        }
    }
}

/// Size and modification time: the cheap check before hashing a file again (source verification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileStamp {
    /// The size in bytes.
    pub size: u64,
    /// The modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
}

impl FileStamp {
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

/// An indexed source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Its path.
    pub path: RepoPath,
    /// Its language.
    pub language: Language,
    /// Its size and modification time.
    pub stamp: FileStamp,
}

/// A file read for context only, such as a TypeScript declaration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    /// Its path.
    pub path: RepoPath,
    /// The language that reads it.
    pub language: Language,
    /// Where it stands with respect to `[scope]`; the language decides what that means.
    pub scope: ScopeStatus,
    /// Its size and modification time.
    pub stamp: FileStamp,
}

/// An existing source-like file that isn't indexed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnindexedFile {
    /// Its path.
    pub path: RepoPath,
    /// Why it isn't indexed.
    pub reason: Unindexed,
    /// Its size and modification time.
    pub stamp: FileStamp,
}

/// A configuration input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    /// Its path.
    pub path: RepoPath,
    /// What it configures.
    pub role: InputRole,
    /// Its size and modification time.
    pub stamp: FileStamp,
}

/// Something the walk couldn't read or use: a folder without permission, an ignore rule that
/// doesn't parse (its ignore file is the path), or the files and folders whose names can't be
/// project paths (one problem lists the first few, with no path). Discovery carries on without
/// it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Problem {
    /// The folder-relative path involved, or empty when unknown.
    pub path: Box<str>,
    /// What went wrong.
    pub message: Box<str>,
}

/// Everything one walk found. Every list is sorted by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Indexed sources.
    pub sources: Vec<SourceFile>,
    /// Context-only files.
    pub context: Vec<ContextFile>,
    /// Existing source-like files that aren't indexed: out of scope, a language that is off, or
    /// JavaScript. Files under ignored or always-excluded folders aren't walked and so aren't
    /// listed; [`exists_in_root`] answers for those.
    pub unindexed: Vec<UnindexedFile>,
    /// Configuration inputs: `unfold.toml`, `tsconfig*.json`, `package.json`, and
    /// `pyproject.toml`.
    pub inputs: Vec<InputFile>,
    /// What couldn't be read or used.
    pub problems: Vec<Problem>,
}

/// Why discovery couldn't start.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The folder can't be opened.
    #[error("cannot open folder {}: {source}", path.display())]
    Root {
        /// The folder.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The path isn't a folder.
    #[error("{} is not a folder", path.display())]
    NotAFolder {
        /// The path.
        path: PathBuf,
    },
}

/// Walks `root` and classifies every file under the settings. Unreadable entries become
/// [`Problem`]s rather than errors; only an unusable `root` fails.
pub fn discover(
    root: &Path,
    settings: &Settings,
    options: &DiscoverOptions,
) -> Result<Discovery, DiscoveryError> {
    let root = std::fs::canonicalize(root).map_err(|source| DiscoveryError::Root {
        path: root.to_owned(),
        source,
    })?;
    if !root.is_dir() {
        return Err(DiscoveryError::NotAFolder { path: root });
    }
    Ok(walk::walk(&root, settings, options))
}

/// Whether `path` exists inside `root` as a file or folder, without following a symlink out of
/// the folder. Resolution uses this for paths a walk doesn't list (ignored or always-excluded
/// folders) to choose between `not_indexed` and `not_found`.
pub fn exists_in_root(root: &Path, path: &RepoPath) -> bool {
    let full = root.join(path.as_str());
    match std::fs::symlink_metadata(&full) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            match (std::fs::canonicalize(root), std::fs::canonicalize(&full)) {
                (Ok(root), Ok(target)) => target.starts_with(root),
                _ => false,
            }
        }
        Ok(_) => true,
        Err(_) => false,
    }
}
