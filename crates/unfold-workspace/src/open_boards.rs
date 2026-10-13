//! The open boards of one project: the board-session registry. The one window keeps a session
//! for every board it opened while the app runs (its undo history lives on in the UI), so
//! nothing here refuses an open.
//!
//! [`OpenBoards`] maps each open board of a project (by board id) to the window that opened it.
//! Opening a board records it; a board id change (first Publish, Move back to drafts) renames it;
//! `close_board` and `delete_board` remove it, and a window that closes releases every board it
//! opened ([`OpenBoards::close_window`]), so its project can rest. Board commands other than the
//! reads and the folder commands work only on an open board ([`OpenBoards::check`]).
//!
//! Each open board also records the other projects its cards resolve to
//! ([`OpenBoards::set_references`]): an open board keeps those projects live too (decision
//! project liveness), so the cards it shows from them stay current.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::{Result, WorkspaceError};
use crate::events::WindowId;
use crate::projects::ProjectId;

/// One open board.
#[derive(Debug, Clone)]
struct OpenBoard {
    window: WindowId,
    references: BTreeSet<ProjectId>,
}

/// The open boards of one project. See the [module documentation](self).
#[derive(Debug, Default)]
pub struct OpenBoards {
    open: Mutex<BTreeMap<String, OpenBoard>>,
}

impl OpenBoards {
    /// No board is open.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, OpenBoard>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens `board` in `window`, keeping its references when it is open already; whether it
    /// was closed before.
    pub fn open(&self, board: &str, window: WindowId) -> bool {
        let mut open = self.lock();
        match open.get_mut(board) {
            Some(existing) => {
                existing.window = window;
                false
            }
            None => {
                open.insert(
                    board.to_owned(),
                    OpenBoard {
                        window,
                        references: BTreeSet::new(),
                    },
                );
                true
            }
        }
    }

    /// Closes `board`; whether it was open.
    pub fn close(&self, board: &str) -> bool {
        self.lock().remove(board).is_some()
    }

    /// Renames an open board (its id changed), keeping its window and references.
    pub fn rename(&self, from: &str, to: &str) {
        let mut open = self.lock();
        if let Some(board) = open.remove(from) {
            open.insert(to.to_owned(), board);
        }
    }

    /// Records the other projects `board`'s cards resolve to (an open board keeps them live).
    /// Does nothing when the board isn't open.
    pub fn set_references(&self, board: &str, references: BTreeSet<ProjectId>) {
        if let Some(open) = self.lock().get_mut(board) {
            open.references = references;
        }
    }

    /// The window that opened `board`, while it is open.
    pub fn window(&self, board: &str) -> Option<WindowId> {
        self.lock().get(board).map(|open| open.window)
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

    /// Whether an open board has cards from `project`.
    pub fn references(&self, project: &ProjectId) -> bool {
        self.lock().values().any(|open| open.references.contains(project))
    }

    /// Closes every board `window` opened and returns them, sorted.
    pub fn close_window(&self, window: WindowId) -> Vec<String> {
        let mut open = self.lock();
        let boards: Vec<String> = open
            .iter()
            .filter(|(_, board)| board.window == window)
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
    fn boards_open_rename_and_close_with_their_window() {
        let open = OpenBoards::new();
        assert!(open.is_empty());
        assert!(open.check("orders").is_err());
        assert!(open.open("orders", 1));
        assert!(!open.open("orders", 1), "opening again keeps the session");
        assert!(open.check("orders").is_ok());
        assert_eq!(open.window("orders"), Some(1));
        let web = ProjectId::new_unchecked("p-webwebwebw");
        open.set_references("orders", BTreeSet::from([web.clone()]));
        open.rename("orders", ".unfold/boards/orders.json");
        assert!(open.references(&web), "references move with the board");
        assert!(open.check("orders").is_err());
        assert!(open.open("b", 1));
        assert!(open.open("other-window", 2));
        assert_eq!(open.close_window(1), [".unfold/boards/orders.json", "b"]);
        assert!(!open.references(&web));
        assert_eq!(open.window("b"), None);
        assert!(open.close("other-window"));
        assert!(!open.close("other-window"));
        assert!(open.is_empty());
    }
}
