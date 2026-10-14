//! Errors from the shared backend.
//!
//! Every variant is a broken contract between a language and the backend (for example, an
//! outside outcome without module evidence). Unfold fails the update with a message instead of
//! guessing or panicking, and the previous graph stays.

use unfold_core::RepoPath;

use crate::program::{FactsSet, FileId};

/// A broken contract between a language front end and the shared backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// An outside outcome has no module evidence to label it with.
    #[error("{path}: the outside outcome for `{text}` has no module evidence")]
    MissingModuleEvidence {
        /// The file of the link.
        path: RepoPath,
        /// The link's text.
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
    /// An import resolved to an indexed file that has no nodes.
    #[error("{path}: an import resolves to {target}, which has no module node")]
    MissingModule {
        /// The importing file.
        path: RepoPath,
        /// The target file.
        target: Box<str>,
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
