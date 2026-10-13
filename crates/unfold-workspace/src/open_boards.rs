//! The open boards of one project: the board-session registry. The one window keeps a session
//! for every board it opened while the app runs (its undo history lives on in the UI), so
//! nothing here refuses an open.
//!
//! [`OpenBoards`] maps each open board of a project (by board id) to the window that opened it.
//! Opening a board records it ([`crate::projects::ProjectHandle::open_board`], which refuses a
//! window that already closed); `close_board` and `delete_board` remove it, and a window that
//! closes releases every board it opened ([`OpenBoards::close_window`]), so its project can rest.
//! Board commands other than the reads and the folder commands work only on an open board
//! ([`OpenBoards::check`]).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::{Result, WorkspaceError};
use crate::events::WindowId;

/// The open boards of one project. See the [module documentation](self).
#[derive(Debug, Default)]
pub struct OpenBoards {
    open: Mutex<BTreeMap<String, WindowId>>,
}

impl OpenBoards {
    /// No board is open.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, WindowId>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens `board` in `window`; whether it was closed before.
    pub(crate) fn open(&self, board: &str, window: WindowId) -> bool {
        self.lock().insert(board.to_owned(), window).is_none()
    }

    /// Closes `board`; whether it was open.
    pub fn close(&self, board: &str) -> bool {
        self.lock().remove(board).is_some()
    }

    /// The window that opened `board`, while it is open.
    pub fn window(&self, board: &str) -> Option<WindowId> {
        self.lock().get(board).copied()
    }

    /// `Ok` when `board` is open: the check every board command on an open board makes.
    pub fn check(&self, board: &str) -> Result<()> {
        if self.lock().contains_key(board) {
            Ok(())
        } else {
            Err(WorkspaceError::BoardNotOpen(board.to_owned()))
        }
    }

    /// Whether no board of the project is open.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Closes every board `window` opened and returns them, sorted.
    pub fn close_window(&self, window: WindowId) -> Vec<String> {
        let mut open = self.lock();
        let boards: Vec<String> = open
            .iter()
            .filter(|(_, opener)| **opener == window)
            .map(|(board, _)| board.clone())
            .collect();
        for board in &boards {
            open.remove(board);
        }
        boards
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boards_open_and_close_with_their_window() {
        let open = OpenBoards::new();
        assert!(open.is_empty());
        assert!(open.check("orders").is_err());
        assert!(open.open("orders", 1));
        assert!(!open.open("orders", 1), "opening again keeps the session");
        assert!(open.check("orders").is_ok());
        assert_eq!(open.window("orders"), Some(1));
        assert!(open.open("b", 1));
        assert!(open.open("other-window", 2));
        assert_eq!(open.close_window(1), ["b", "orders"]);
        assert_eq!(open.window("b"), None);
        assert!(open.close("other-window"));
        assert!(!open.close("other-window"));
        assert!(open.is_empty());
    }
}
