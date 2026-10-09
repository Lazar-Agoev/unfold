//! Board claims per window (decision U-37, architecture §8.7, §11.2): a board is open in one
//! window at a time, so two windows never edit the same board and no merge logic exists.
//!
//! [`BoardClaims`] maps each open board (by ID: a draft ID or a published path) to the window
//! holding it. Opening a board claims it; a board another window holds is refused with that
//! window's ID, so the UI can focus it instead. **Move to New Window** transfers the claim, and a
//! window that closes releases everything it held. Board commands check
//! [`BoardClaims::check`] before touching a board. The board logic itself lives in
//! `unfold-reader`.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::{Result, WorkspaceError};
use crate::events::WindowId;

/// Which window holds each open board of one folder. See the [module documentation](self).
#[derive(Debug, Default)]
pub struct BoardClaims {
    held: Mutex<BTreeMap<String, WindowId>>,
}

impl BoardClaims {
    /// No board is open.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, WindowId>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Claims `board` for `window`. Claiming a board the window already holds succeeds;
    /// a board another window holds fails with [`WorkspaceError::BoardHeld`].
    pub fn claim(&self, board: &str, window: WindowId) -> Result<()> {
        let mut held = self.lock();
        match held.get(board) {
            Some(&holder) if holder != window => Err(WorkspaceError::BoardHeld {
                board: board.to_owned(),
                window: holder,
            }),
            Some(_) => Ok(()),
            None => {
                held.insert(board.to_owned(), window);
                Ok(())
            }
        }
    }

    /// Releases `board` if `window` holds it; whether it did.
    pub fn release(&self, board: &str, window: WindowId) -> bool {
        let mut held = self.lock();
        if held.get(board) == Some(&window) {
            held.remove(board);
            true
        } else {
            false
        }
    }

    /// Moves `board` from window `from`, which must hold it, to window `to`.
    pub fn transfer(&self, board: &str, from: WindowId, to: WindowId) -> Result<()> {
        let mut held = self.lock();
        match held.get_mut(board) {
            Some(holder) if *holder == from => {
                *holder = to;
                Ok(())
            }
            Some(&mut holder) => Err(WorkspaceError::BoardHeld {
                board: board.to_owned(),
                window: holder,
            }),
            None => Err(WorkspaceError::BoardNotHeld(board.to_owned())),
        }
    }

    /// Renames a claimed board, such as a draft that was published, keeping its holder.
    pub fn rename(&self, from: &str, to: &str) {
        let mut held = self.lock();
        if let Some(window) = held.remove(from) {
            held.insert(to.to_owned(), window);
        }
    }

    /// The window holding `board`.
    pub fn holder(&self, board: &str) -> Option<WindowId> {
        self.lock().get(board).copied()
    }

    /// `Ok` when `window` holds `board`: the check every board command makes.
    pub fn check(&self, board: &str, window: WindowId) -> Result<()> {
        match self.holder(board) {
            Some(holder) if holder == window => Ok(()),
            Some(holder) => Err(WorkspaceError::BoardHeld {
                board: board.to_owned(),
                window: holder,
            }),
            None => Err(WorkspaceError::BoardNotHeld(board.to_owned())),
        }
    }

    /// The boards `window` holds, sorted.
    pub fn boards_of(&self, window: WindowId) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(_, holder)| **holder == window)
            .map(|(board, _)| board.clone())
            .collect()
    }

    /// Releases every board `window` holds and returns them, sorted.
    pub fn release_window(&self, window: WindowId) -> Vec<String> {
        let mut held = self.lock();
        let boards: Vec<String> = held
            .iter()
            .filter(|(_, holder)| **holder == window)
            .map(|(board, _)| board.clone())
            .collect();
        for board in &boards {
            held.remove(board);
        }
        boards
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_window_per_board() {
        let claims = BoardClaims::new();
        claims.claim("orders", 1).expect("claim");
        claims.claim("orders", 1).expect("again");
        let held = claims.claim("orders", 2).expect_err("held");
        assert!(matches!(held, WorkspaceError::BoardHeld { window: 1, .. }));
        assert!(claims.check("orders", 1).is_ok());
        assert!(claims.check("orders", 2).is_err());
        assert!(claims.check("other", 1).is_err());
        assert!(!claims.release("orders", 2));
        claims.transfer("orders", 1, 3).expect("transfer");
        assert_eq!(claims.holder("orders"), Some(3));
        assert!(claims.transfer("orders", 1, 2).is_err());
        claims.rename("orders", ".unfold/boards/orders.json");
        assert_eq!(claims.boards_of(3), [".unfold/boards/orders.json"]);
        claims.claim("b", 3).expect("claim");
        assert_eq!(claims.release_window(3).len(), 2);
        assert_eq!(claims.holder("b"), None);
        assert!(!claims.release("x", 1));
    }
}
