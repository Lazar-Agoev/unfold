//! Errors from the shared backend.
//!
//! Every variant is a broken contract between a language and the backend (for example, an
//! external outcome without module evidence). The proof of concept treated these as compiler
//! errors; Unfold fails the update with a message instead of guessing or panicking, and the
//! previous graph stays.

use unfold_core::RepoPath;
use unfold_core::vocab::DetectorId;

use crate::program::{FactsSet, FileId};

/// A broken contract between a language front end and the shared backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// An external outcome has no module evidence to label it with.
    #[error("{path}: the external outcome for `{text}` has no module evidence")]
    MissingModuleEvidence {
        /// The file of the occurrence or target.
        path: RepoPath,
        /// The occurrence's or target's label.
        text: Box<str>,
    },
    /// A language spelled a module label that is empty or starts with `[`.
    #[error("{path}: invalid module label `{label}`")]
    InvalidModuleLabel {
        /// The file the label was computed for.
        path: RepoPath,
        /// The label as the language spelled it.
        label: Box<str>,
    },
    /// A language's module dependencies don't line up with a file's import facts.
    #[error("{path}: {found} module dependencies for {expected} imports")]
    DependencyCount {
        /// The file.
        path: RepoPath,
        /// The number of import facts.
        expected: usize,
        /// The number of dependencies the language reported.
        found: usize,
    },
    /// An import resolved to an indexed file that has no blocks.
    #[error("{path}: an import resolves to {target}, which has no file block")]
    MissingFileBlock {
        /// The importing file.
        path: RepoPath,
        /// The target file.
        target: Box<str>,
    },
    /// A detector produced an entrance with no targets.
    #[error("an entrance candidate from {detector} has no targets")]
    EmptyEntrance {
        /// The detector.
        detector: DetectorId,
    },
    /// An entrance candidate names a block, value, or file that does not exist.
    #[error("an entrance candidate from {detector} names a missing block, value, or file")]
    MissingCandidateFact {
        /// The detector.
        detector: DetectorId,
    },
    /// A file's facts couldn't be renumbered after dropping its unkept conditional imports.
    #[error("{path}: {message}")]
    Facts {
        /// The file.
        path: RepoPath,
        /// What went wrong.
        message: Box<str>,
    },
    /// A value target belongs to a file no linked language owns.
    #[error("{path}: no linked language resolves this file's values")]
    NoLanguage {
        /// The file.
        path: RepoPath,
    },
}

/// The path of `file`, for error messages. File IDs come from the same program, so the fallback
/// only guards against misuse.
pub(crate) fn path_of(program: &FactsSet, file: FileId) -> RepoPath {
    program.file(file).map_or_else(
        || RepoPath::new("unknown").expect("`unknown` is a valid path"),
        |file| file.path.clone(),
    )
}
