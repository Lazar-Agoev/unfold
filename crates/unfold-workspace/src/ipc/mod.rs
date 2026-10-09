//! The typed IPC contract between the UI and the engine (architecture §12, decision U-25).
//!
//! - [`table`]: the command table, one line per command, and the [`table::Request`] the addon's
//!   `dispatch` parses.
//! - [`context`]: what a handler gets ([`context::Ctx`]).
//! - [`error`]: the `{code, message}` error every command rejects with.
//! - [`typescript`]: the generated TypeScript module (`cargo xtask gen-ipc`).
//!
//! The handlers live in [`crate::commands`]; [`crate::host::Host`] is the facade the addon
//! wraps. `docs/ipc.md` catalogues the commands and events.

pub mod context;
pub mod error;
pub mod table;
pub mod typescript;

pub use context::Ctx;
pub use error::{CommandError, CommandResult};
pub use table::Request;
