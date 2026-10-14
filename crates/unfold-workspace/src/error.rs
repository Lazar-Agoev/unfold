//! The workspace's error type, with the stable codes the addon puts in rejected promises
//! (`{"code", "message"}`, brief §6).

use std::path::PathBuf;

use unfold_engine::settings::edit::EditError;

use crate::layout::Unwritable;
use crate::projects::ProjectId;
use crate::workspace::settings_error;

/// Why a workspace operation failed.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// No project with that id is in the workspace.
    #[error("no project `{0}` in the workspace")]
    UnknownProject(ProjectId),
    /// A board of the project is open, so it can't be removed.
    #[error("a board of this project is open")]
    BoardsOpen,
    /// The workspace has as many projects as it can hold.
    #[error("the workspace already has {0} projects")]
    TooManyProjects(usize),
    /// The path to open is not a folder, or can't be resolved.
    #[error("{} is not a folder Unfold can open: {reason}", path.display())]
    NotAFolder {
        /// The path as given.
        path: PathBuf,
        /// What went wrong.
        reason: String,
    },
    /// Unfold can't write `.unfold/` in the folder.
    #[error("Unfold needs to write `.unfold/` in {}, but {reason}", path.display())]
    FolderNotWritable {
        /// The folder.
        path: PathBuf,
        /// Why it can't be written.
        reason: Unwritable,
    },
    /// Another copy of Unfold holds the folder's lock.
    #[error("{} is open in another copy of Unfold", path.display())]
    OpenElsewhere {
        /// The folder.
        path: PathBuf,
    },
    /// A file or folder could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The graph database failed.
    #[error(transparent)]
    Store(#[from] unfold_store::StoreError),
    /// A reader query failed.
    #[error(transparent)]
    Reader(#[from] unfold_reader::ReaderError),
    /// The compile or update failed; the previous graph is kept.
    #[error("compile failed: {0}")]
    Update(String),
    /// The compile was cancelled.
    #[error("the compile was cancelled")]
    Cancelled,
    /// A command named a path that is not inside the project's folder.
    #[error("`{path}` is not a path inside the project's folder: {reason}")]
    InvalidPath {
        /// The path as given.
        path: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A settings edit would make `unfold.toml` invalid, or the file is invalid now.
    #[error("{message}")]
    InvalidSettings {
        /// What is wrong, with the file and position.
        message: String,
        /// The one-based line, when known.
        line: Option<u32>,
        /// The one-based column, when known.
        column: Option<u32>,
    },
    /// A settings edit can't be expressed without changing a broader rule.
    #[error("{0}")]
    SettingsConflict(String),
    /// The board isn't open.
    #[error("board `{0}` is not open")]
    BoardNotOpen(String),
    /// The window that sent the command has closed.
    #[error("the window has closed")]
    WindowClosed,
    /// The project has no graph yet.
    #[error("the project has not been compiled yet")]
    NotCompiled,
    /// The project's workspace is closing or closed.
    #[error("the project is closed")]
    Closed,
    /// A JSON state file could not be written.
    #[error("cannot encode {what}: {message}")]
    Encode {
        /// What was being written.
        what: &'static str,
        /// The serializer's message.
        message: String,
    },
    /// An invariant broke, such as a panic on the engine thread.
    #[error("internal error: {0}")]
    Internal(String),
}

impl WorkspaceError {
    /// The stable snake_case code the addon reports with the message.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownProject(_) => "unknown_project",
            Self::BoardsOpen => "boards_open",
            Self::TooManyProjects(_) => "too_many_projects",
            Self::NotAFolder { .. } => "not_a_folder",
            Self::FolderNotWritable { .. } => "folder_not_writable",
            Self::OpenElsewhere { .. } => "open_elsewhere",
            Self::Io { .. } => "io",
            Self::Store(_) => "store",
            Self::Reader(_) => "reader",
            Self::Update(_) => "compile_failed",
            Self::Cancelled => "cancelled",
            Self::InvalidPath { .. } => "invalid_path",
            Self::InvalidSettings { .. } => "invalid_settings",
            Self::SettingsConflict(_) => "settings_conflict",
            Self::BoardNotOpen(_) => "board_not_open",
            Self::WindowClosed => "window_closed",
            Self::NotCompiled => "not_compiled",
            Self::Closed => "closed",
            Self::Encode { .. } | Self::Internal(_) => "internal",
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl From<EditError> for WorkspaceError {
    fn from(error: EditError) -> Self {
        match error {
            EditError::Unparsable(error) => match settings_error(error) {
                Self::InvalidSettings {
                    message,
                    line,
                    column,
                } => Self::InvalidSettings {
                    message: format!("fix unfold.toml first: {message}"),
                    line,
                    column,
                },
                other => other,
            },
            EditError::Invalid(error) => settings_error(error),
            EditError::Conflict(message) => Self::SettingsConflict(message.into()),
        }
    }
}

/// The workspace's result type.
pub type Result<T, E = WorkspaceError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case() {
        let errors = [
            WorkspaceError::Cancelled,
            WorkspaceError::NotCompiled,
            WorkspaceError::OpenElsewhere { path: "/x".into() },
            WorkspaceError::BoardNotOpen("b".into()),
            WorkspaceError::UnknownProject(ProjectId::new_unchecked("p-aaaaaaaaaa")),
            WorkspaceError::BoardsOpen,
            WorkspaceError::WindowClosed,
            WorkspaceError::FolderNotWritable {
                path: "/x".into(),
                reason: Unwritable::ReadOnly,
            },
            WorkspaceError::TooManyProjects(64),
        ];
        for error in errors {
            assert!(error.code().chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }
}
