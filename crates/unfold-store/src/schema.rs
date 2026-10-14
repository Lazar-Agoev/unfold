//! The executable database schema and connection settings.
//!
//! The tables persist the graph's records (`docs/designs/diagram-blocks.md`, "Records") and the
//! reader's definition catalog:
//!
//! | Table | One row per |
//! | --- | --- |
//! | `files`, `inputs` | file (with its disk stamp), input |
//! | `keys` | structural key ever written: the intern table every key column points into |
//! | `nodes` | node, by file and local ID |
//! | `links` | link, by file, `from`, and `seq` |
//! | `signatures`, `parameters`, `members`, `arguments` | typed record |
//! | `apps`, `entrances` | app and entrance of `unfold.toml`, in file order; the row ID is the key's ID |
//! | `entrance_targets` | `code` key of an app (position 0) or an entrance (in order), with its state |
//! | `definitions` | entry of the reader's definition catalog |
//! | `file_facts` | cached per-file facts entry |
//! | `search` | FTS5 row: a definition, app, entrance, or file ([`crate::search`]) |
//!
//! Nodes, links, and typed records refer to nodes of their own file by local ID, as the model
//! does. A node's optional ranges are pairs of nullable columns. A link's target is in exactly
//! one of `target_key_id` (`project`), `module` (`library`, `builtin`), or `reason`
//! (`unknown`), as its `outcome` says.
//!
//! Bit sets: a node's `flags` has bit *i* for the *i*-th [`unfold_core::vocab::Flag`]; a
//! parameter's `flags` has `optional` (1) and `default` (2); a member's `optional` (1),
//! `readonly` (2), and `default` (4).
//!
//! Besides the primary keys and `UNIQUE` constraints, these indexes serve specific queries:
//!
//! | Index | Query |
//! | --- | --- |
//! | `nodes_by_parent` | A node's children |
//! | `links_by_target` | The reverse indexes: callers (`calls`, `passes`) and type uses (`types`, `extends`) of a key |
//! | `definitions_by_file` | A file's definitions, and replacing them when the file changes |
//!
//! The search table uses FTS5's `trigram` tokenizer, so a query matches anywhere inside an
//! identifier (`order` finds `place_order` and `OrderService`), which word tokenizers cannot do
//! for camel-case names.
//!
//! The database is a cache: when [`SCHEMA_VERSION`] or the engine version differs from what a
//! database records, the store discards it and creates a new one. There are no migrations.

use rusqlite::Connection;

use crate::error::Result;

/// The version of the schema below. Bump it whenever a table, column, index, or the meaning of
/// a stored value changes; existing databases are then rebuilt.
pub const SCHEMA_VERSION: u32 = 6;

/// Every table and index, created in one transaction.
const DDL: &str = r"
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE files (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL UNIQUE,
  language    TEXT NOT NULL,
  state       TEXT NOT NULL,
  facts_hash  BLOB,
  disk_hash   BLOB,
  disk_mtime  INTEGER,
  disk_size   INTEGER,
  failure     TEXT
);

CREATE TABLE inputs (
  path TEXT PRIMARY KEY,
  role TEXT NOT NULL,
  hash BLOB NOT NULL
);

CREATE TABLE keys (id INTEGER PRIMARY KEY, key TEXT NOT NULL UNIQUE);

CREATE TABLE nodes (
  file_id INTEGER NOT NULL, local_id INTEGER NOT NULL,
  key_id INTEGER NOT NULL UNIQUE,
  parent INTEGER,
  form TEXT NOT NULL, name TEXT,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
  head_start INTEGER, head_end INTEGER,
  body_start INTEGER, body_end INTEGER,
  doc_start INTEGER, doc_end INTEGER,
  stmt_start INTEGER, stmt_end INTEGER,
  value_start INTEGER, value_end INTEGER,
  binds TEXT, role TEXT, for_node INTEGER, flags INTEGER NOT NULL, target INTEGER,
  PRIMARY KEY (file_id, local_id)
) WITHOUT ROWID;
CREATE INDEX nodes_by_parent ON nodes (file_id, parent);

CREATE TABLE links (
  file_id INTEGER NOT NULL, from_id INTEGER NOT NULL, seq INTEGER NOT NULL,
  kind TEXT NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL, text TEXT NOT NULL,
  outcome TEXT NOT NULL, target_key_id INTEGER, module TEXT, reason TEXT,
  implements INTEGER NOT NULL, bound INTEGER,
  PRIMARY KEY (file_id, from_id, seq)
) WITHOUT ROWID;
CREATE INDEX links_by_target ON links (target_key_id) WHERE target_key_id IS NOT NULL;

CREATE TABLE signatures (
  file_id INTEGER NOT NULL, node INTEGER NOT NULL,
  returns TEXT, returns_start INTEGER, returns_end INTEGER,
  PRIMARY KEY (file_id, node)
) WITHOUT ROWID;

CREATE TABLE parameters (
  file_id INTEGER NOT NULL, node INTEGER NOT NULL, idx INTEGER NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,
  type TEXT, type_start INTEGER, type_end INTEGER,
  PRIMARY KEY (file_id, node, idx)
) WITHOUT ROWID;

CREATE TABLE members (
  file_id INTEGER NOT NULL, node INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,
  type TEXT, type_start INTEGER, type_end INTEGER, value TEXT,
  PRIMARY KEY (file_id, node, idx)
) WITHOUT ROWID;

CREATE TABLE arguments (
  file_id INTEGER NOT NULL, call INTEGER NOT NULL, idx INTEGER NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  expand TEXT, keyword TEXT, parameter INTEGER, unpaired TEXT, bound INTEGER,
  PRIMARY KEY (file_id, call, idx)
) WITHOUT ROWID;

CREATE TABLE apps (
  id INTEGER PRIMARY KEY,
  position INTEGER NOT NULL UNIQUE,
  name TEXT NOT NULL, run TEXT, about TEXT
);

CREATE TABLE entrances (
  id INTEGER PRIMARY KEY,
  app_id INTEGER NOT NULL, position INTEGER NOT NULL,
  name TEXT NOT NULL, on_text TEXT, about TEXT,
  UNIQUE (app_id, position)
);

CREATE TABLE entrance_targets (
  owner_id INTEGER NOT NULL, position INTEGER NOT NULL,
  code_key_id INTEGER NOT NULL, state TEXT NOT NULL,
  PRIMARY KEY (owner_id, position)
) WITHOUT ROWID;

CREATE TABLE definitions (
  key_id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL, local_id INTEGER NOT NULL,
  kind TEXT NOT NULL, qualified_name TEXT NOT NULL, label TEXT NOT NULL
);
CREATE INDEX definitions_by_file ON definitions (file_id, local_id);

CREATE TABLE file_facts (
  path TEXT NOT NULL, hash BLOB NOT NULL, engine TEXT NOT NULL, settings_hash BLOB NOT NULL,
  facts BLOB NOT NULL,
  PRIMARY KEY (path, hash, engine, settings_hash)
) WITHOUT ROWID;

CREATE VIRTUAL TABLE search USING fts5(
  name, qualified_name, path, kind, key UNINDEXED,
  tokenize = 'trigram'
);
";

/// Keys of the `meta` table.
pub(crate) mod meta_key {
    pub const SCHEMA_VERSION: &str = "schema_version";
    pub const ENGINE_VERSION: &str = "engine_version";
    pub const PRODUCERS: &str = "producers";
    pub const SETTINGS_HASH: &str = "settings_hash";
    pub const COMPILED_AT: &str = "compiled_at";
    pub const COMPLETE: &str = "complete";
    pub const LINK_BASIS: &str = "link_basis";
}

/// Creates every table in an empty database and records the versions.
pub(crate) fn create(conn: &Connection, engine_version: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(DDL)?;
    {
        let mut insert = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
        insert.execute((meta_key::SCHEMA_VERSION, SCHEMA_VERSION.to_string()))?;
        insert.execute((meta_key::ENGINE_VERSION, engine_version))?;
    }
    tx.commit()?;
    Ok(())
}

/// Settings for the writer connection. `journal_mode` is set separately because its result
/// must be checked.
///
/// - `synchronous = NORMAL`: with write-ahead logging a commit survives an application crash,
///   and only a power loss can drop the last commits, which is fine for a cache.
/// - `temp_store = MEMORY`: sorts and temporary indexes never touch the disk.
/// - `cache_size`: 64 MiB, so a first compile's transaction does not spill pages mid-way.
/// - `journal_size_limit`: shrinks the log after a large first compile is checkpointed.
/// - Foreign keys stay off: the store maintains referential integrity itself.
pub(crate) const WRITER_PRAGMAS: &str = "
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -65536;
PRAGMA journal_size_limit = 67108864;
";

/// Settings for reader connections.
pub(crate) const READER_PRAGMAS: &str = "
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -16384;
PRAGMA query_only = ON;
";

/// How many prepared statements each connection keeps.
pub(crate) const STATEMENT_CACHE_CAPACITY: usize = 64;
