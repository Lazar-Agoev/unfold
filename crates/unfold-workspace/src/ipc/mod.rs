//! The typed IPC contract between the UI and the engine (docs/ipc.md#how-a-command-travels, generated command types).
//!
//! - [`table`]: the command table, one line per command, and the [`table::Request`] the addon's
//!   `dispatch` parses.
//! - [`context`]: what a handler gets ([`context::Ctx`]).
//! - [`error`]: the `{code, message}` error every command rejects with.
//! - `typescript`: the generated TypeScript module (`cargo xtask gen-ipc`), with the `ts`
//!   feature.
//!
//! The handlers live in [`crate::commands`]; [`crate::host::Host`] is the facade the addon
//! wraps. `docs/ipc.md` catalogues the commands and events.

pub mod context;
pub mod error;
pub mod table;
#[cfg(feature = "ts")]
pub mod typescript;

pub use context::Ctx;
pub use error::{CommandError, CommandResult};
pub use table::Request;
