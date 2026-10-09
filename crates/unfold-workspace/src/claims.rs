//! Board claims of one project (contract §3.5, decision U-37): a board is open in one window at
//! a time, so two windows never edit the same board and no merge logic exists.
//!
//! [`BoardClaims`] maps each open board of a project (by `BoardId`) to the window holding it.
//! Opening a board claims it; a board another window holds is refused with that window's id, so
//! the UI can focus it instead. **Open in New Window** transfers the claim, a board id change
//! (first Publish, Move back to drafts) renames it, and a window that closes releases everything
//! it held. Board commands check [`BoardClaims::check`] before touching a board.
//!
//! Each claim also records the other projects the board's cards resolve to
//! ([`BoardClaims::set_references`]): an open board keeps those projects live too (contract
//! §3.3), so the cards it shows from them stay current.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::{Result, WorkspaceError};
use crate::events::WindowId;
use crate::projects::ProjectId;

/// One open board.
#[derive(Debug, Clone)]
struct Claim {
    window: WindowId,
    references: BTreeSet<ProjectId>,
}

/// Which window holds each open board of one project. See the [module documentation](self).
#[derive(Debug, Default)]
pub struct BoardClaims {
    held: Mutex<BTreeMap<String, Claim>>,
}

impl BoardClaims {
    /// No board is open.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Claim>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Claims `board` for `window`. Claiming a board the window already holds succeeds;
    /// a board another window holds fails with [`WorkspaceError::BoardHeld`].
    pub fn claim(&self, board: &str, window: WindowId) -> Result<()> {
        let mut held = self.lock();
        match held.get(board) {
            Some(claim) if claim.window != window => Err(WorkspaceError::BoardHeld {
                board: board.to_owned(),
                window: claim.window,
            }),
            Some(_) => Ok(()),
            None => {
                held.insert(
                    board.to_owned(),
                    Claim {
                        window,
                        references: BTreeSet::new(),
                    },
                );
                Ok(())
            }
        }
    }

    /// Releases `board` if `window` holds it; whether it did.
    pub fn release(&self, board: &str, window: WindowId) -> bool {
        let mut held = self.lock();
        if held.get(board).is_some_and(|claim| claim.window == window) {
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
            Some(claim) if claim.window == from => {
                claim.window = to;
                Ok(())
            }
            Some(claim) => Err(WorkspaceError::BoardHeld {
                board: board.to_owned(),
                window: claim.window,
            }),
            None => Err(WorkspaceError::BoardNotHeld(board.to_owned())),
        }
    }

    /// Renames a claimed board (its id changed), keeping its holder and references.
    pub fn rename(&self, from: &str, to: &str) {
        let mut held = self.lock();
        if let Some(claim) = held.remove(from) {
            held.insert(to.to_owned(), claim);
        }
    }

    /// Records the other projects `board`'s cards resolve to (an open board keeps them live).
    /// Does nothing when the board isn't claimed.
    pub fn set_references(&self, board: &str, references: BTreeSet<ProjectId>) {
        if let Some(claim) = self.lock().get_mut(board) {
            claim.references = references;
        }
    }

    /// The window holding `board`.
    pub fn holder(&self, board: &str) -> Option<WindowId> {
        self.lock().get(board).map(|claim| claim.window)
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

    /// Whether no board of the project is open.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Whether an open board has cards from `project`.
    pub fn references(&self, project: &ProjectId) -> bool {
        self.lock()
            .values()
            .any(|claim| claim.references.contains(project))
    }

    /// Releases every board `window` holds and returns them, sorted.
    pub fn release_window(&self, window: WindowId) -> Vec<String> {
        let mut held = self.lock();
        let boards: Vec<String> = held
            .iter()
            .filter(|(_, claim)| claim.window == window)
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
        assert!(claims.is_empty());
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
        assert!(claims.transfer("nothing", 1, 2).is_err());
        let web = ProjectId::new_unchecked("p-webwebwebw");
        claims.set_references("orders", BTreeSet::from([web.clone()]));
        claims.rename("orders", ".unfold/boards/orders.json");
        assert!(claims.references(&web), "references move with the claim");
        claims.claim("b", 3).expect("claim");
        assert_eq!(claims.release_window(3).len(), 2);
        assert_eq!(claims.holder("b"), None);
        assert!(!claims.references(&web));
        assert!(!claims.release("x", 1));
        assert!(claims.is_empty());
    }
}
