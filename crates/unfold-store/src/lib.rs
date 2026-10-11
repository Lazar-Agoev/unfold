//! SQLite storage for one folder's graph (docs/architecture.md#storage): the schema, delta commits applied
//! in one transaction, the queries the reader and workspace need, the FTS5 search index, and
//! the per-file facts cache.
//!
//! # Shape
//!
//! ```text
//! Store::open(path, config) ──► Store { writer, readers, opened }
//!                                  │        │
//!         engine thread ◄──────────┘        └──► any thread: readers.get()? ─► Reader
//!   writer.commit / replace(&Update)               files(), file_graph(), incoming(),
//!   writer.put_facts / load_facts                  entrances(), definitions_in(), search(), ...
//! ```
//!
//! - **One writer, many readers.** The folder's engine thread owns the [`Writer`]; every other
//!   thread reads through the [`ReadPool`]. Write-ahead logging lets readers see the last
//!   committed graph while a commit runs (docs/engine.md#facts-cache-and-failures).
//! - **Deltas.** The engine describes what changed as an [`Update`]: per-file parts (structure,
//!   edges, definitions), removed files, entrance changes by key, and replaced small sets. A
//!   commit writes rows proportional to the change, in one transaction. See [`update`].
//! - **Stable identity.** Structural keys are interned in the `keys` table and keep their IDs
//!   for the life of the database (structural keys); an in-memory mirror avoids per-key queries.
//!   Entrance row IDs are their keys' IDs.
//! - **A cache.** A database written by another schema or engine version, or a damaged one, is
//!   discarded and recreated on open ([`Opened`]); there are no migrations (cache rebuild policy).
//!   The database is private and may change in any release (shared file contracts).
//! - **Facts cache.** Serialized per-file facts, keyed by path, content hash, engine, and
//!   settings hash, keeping each path's current and last good entries ([`writer::facts`]).
//! - **Search.** FTS5 with trigram tokens returns candidate definitions, entrances, and files;
//!   the workspace ranks them ([`search`]).
//!
//! Byte offsets are stored as the engine produced them; `unfold-reader` converts them to UTF-16.

#![forbid(unsafe_code)]

mod codec;
mod error;
pub mod read;
mod schema;
pub mod search;
mod store;
pub mod update;
pub mod writer;

#[cfg(test)]
mod tests;

pub use error::{Result, StoreError};
pub use read::rows::{
    Cursor, DefinitionRow, FileRow, ImportSummary, IncomingEdge, IndexStats, LocatedBlock, Page,
    SearchCandidate, StoredMeta, TypeUse,
};
pub use read::{ReadPool, Reader};
pub use schema::SCHEMA_VERSION;
pub use search::CandidateKind;
pub use store::{Opened, RecreateReason, Store, StoreConfig};
pub use update::{
    Application, Basis, Category, Definition, DiskStamp, EntranceChanges, FileChange, GraphMeta, Membership,
    Structure, Update,
};
pub use writer::facts::{CachedFacts, FactsStatus, NewFacts};
pub use writer::{CommitStats, Writer};
