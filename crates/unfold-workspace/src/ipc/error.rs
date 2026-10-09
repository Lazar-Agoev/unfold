//! The error every command rejects with: a stable snake_case `code` and a sentence, the
//! `{"code", "message"}` JSON the addon puts in a rejected promise (brief §6).
//!
//! | Code | Meaning |
//! | --- | --- |
//! | `invalid_command` | The command JSON is malformed or names no command |
//! | `invalid_args` | The arguments don't fit the command |
//! | `untrusted` | The command needs the main process (a path chosen in a dialog, window lifecycle) |
//! | `not_found` | The key, path, board, or removed folder doesn't exist |
//! | `unknown_project` | No project with that id is in the workspace |
//! | `boards_open` | A window holds a board of the project (remove, switch worktree) |
//! | `not_a_worktree` | `switch_worktree` to a path that isn't an available worktree of the project |
//! | `too_many_projects` | The workspace has 64 projects |
//! | `invalid_folder` | A board folder path breaks the folder rules |
//! | `folder_exists` / `folder_not_empty` | A board folder rename's target is taken / a deleted board folder has boards |
//! | `board_invalid` | A board file or document breaks the `unfold-board` format |
//! | `board_problem` | A save would introduce a missing reference, broken group, or held reference |
//! | `board_conflict` | The board changed on disk since the revision the edit is based on |
//! | `name_taken` | A published board's file name is taken |
//! | `no_editor` | `open_in_editor` has no editor command and opening the file could run it |
//! | `cancelled` | The request was cancelled |
//! | `internal` | A bug: a panic or a broken invariant |
//! | everything else | [`WorkspaceError::code`] (`not_compiled`, `invalid_path`, `board_held`, `not_a_folder`, `open_elsewhere`, ...) |

use serde::Serialize;
use ts_rs::TS;
use unfold_reader::ReaderError;
use unfold_reader::board::BoardError;

use crate::error::WorkspaceError;

/// A rejected command. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CommandError {
    /// The stable snake_case code.
    pub code: String,
    /// What went wrong, for people.
    pub message: String,
}

impl CommandError {
    /// An error with `code` and `message`.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    /// Malformed arguments.
    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("invalid_args", message)
    }

    /// Something named doesn't exist.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    /// A broken invariant.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }

    /// The request was cancelled.
    pub fn cancelled() -> Self {
        Self::new("cancelled", "the request was cancelled")
    }

    /// The error as `{"code", "message"}` JSON.
    pub fn to_json(&self) -> String {
        // Two strings always serialize.
        serde_json::to_string(self)
            .unwrap_or_else(|_| String::from(r#"{"code":"internal","message":"cannot encode an error"}"#))
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<WorkspaceError> for CommandError {
    fn from(error: WorkspaceError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<ReaderError> for CommandError {
    fn from(error: ReaderError) -> Self {
        WorkspaceError::from(error).into()
    }
}

impl From<unfold_store::StoreError> for CommandError {
    fn from(error: unfold_store::StoreError) -> Self {
        WorkspaceError::from(error).into()
    }
}

impl From<BoardError> for CommandError {
    fn from(error: BoardError) -> Self {
        let code = match &error {
            BoardError::Syntax { .. } | BoardError::Invalid { .. } => "board_invalid",
            BoardError::Problem { .. } | BoardError::Unverified { .. } => "board_problem",
            BoardError::InvalidName { .. } => "invalid_args",
            BoardError::NameTaken { .. } => "name_taken",
            BoardError::Io { .. } => "io",
        };
        Self::new(code, error.to_string())
    }
}

/// A command's result.
pub type CommandResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_stable_codes() {
        let error = CommandError::from(WorkspaceError::NotCompiled);
        assert_eq!(error.code, "not_compiled");
        assert_eq!(
            error.to_json(),
            r#"{"code":"not_compiled","message":"the project has not been compiled yet"}"#
        );
        let board = CommandError::from(BoardError::InvalidName { name: "../x".into() });
        assert_eq!(board.code, "invalid_args");
    }
}
