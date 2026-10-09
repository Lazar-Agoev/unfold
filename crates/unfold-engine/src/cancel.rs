//! Cooperative cancellation for compiles and updates.
//!
//! A [`CancelToken`] is shared between whoever may cancel (a newer change, the user, a closing
//! window) and the running update, which checks it between stages and inside every parallel
//! loop, so a cancellation is acknowledged within one file's work. A cancelled update changes
//! nothing: the engine builds the new state beside the old one and swaps only on success.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared cancellation flag. Clones observe the same flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)` once cancellation was requested.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// The work stopped because its [`CancelToken`] was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the update was cancelled")]
pub struct Cancelled;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_flag() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert_eq!(token.check(), Ok(()));
        clone.cancel();
        assert!(token.is_cancelled());
        assert_eq!(token.check(), Err(Cancelled));
    }
}
