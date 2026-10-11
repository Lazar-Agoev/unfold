//! The database schema (architecture §8.3) and the connection settings.
//!
//! The tables match the architecture exactly. Beyond the indexes it lists (`edges_by_target`
//! and the `UNIQUE` constraints), the store adds only the indexes its queries need:
//!
//! | Index | Query |
//! | --- | --- |
//! | `definitions_by_file` | A file's definitions (outline), and replacing them when the file changes |
//! | `entrances_by_evidence` | The entrances declared in one file or input |
//! | `type_links_by_target` | The reverse index of type links: "used in N places" and the list of those places |
//!
//! The typed records (`signatures`, `parameters`, `members`, `arguments`, `results`,
//! `type_links`) are written and replaced per file like slots and edges. Flags are bit sets:
//! a parameter's `optional` (1) and `default` (2); a member's `optional` (1), `readonly` (2),
//! and `default` (4).
//!
//! The search table uses FTS5's `trigram` tokenizer, so a query matches anywhere inside an
//! identifier (`order` finds `createOrder` and `OrderService`), which word tokenizers cannot do
//! for camel-case names. Ranking is the workspace's job; the index only supplies candidates.
//!
//! The database is a cache: when [`SCHEMA_VERSION`] or the engine version differs from what a
//! database records, the store discards it and creates a new one. There are no migrations.

use rusqlite::Connection;

use crate::error::Result;

/// The version of the schema below. Bump it whenever a table, column, index, or the meaning of
/// a stored value changes; existing databases are then rebuilt.
pub const SCHEMA_VERSION: u32 = 3;

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

CREATE TABLE blocks (
  file_id INTEGER NOT NULL, local_id INTEGER NOT NULL,
  key_id INTEGER NOT NULL UNIQUE,
  parent_local_id INTEGER,
  kind TEXT NOT NULL, role TEXT, name TEXT,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
  PRIMARY KEY (file_id, local_id)
) WITHOUT ROWID;

CREATE TABLE slots (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL,
  slot TEXT NOT NULL, idx INTEGER NOT NULL DEFAULT -1,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  PRIMARY KEY (file_id, block_local_id, slot, idx)
) WITHOUT ROWID;

CREATE TABLE edges (
  file_id INTEGER NOT NULL, source_local_id INTEGER NOT NULL, seq INTEGER NOT NULL,
  kind TEXT NOT NULL,
  target_key_id INTEGER,
  target_text TEXT NOT NULL,
  resolution TEXT NOT NULL,
  reason TEXT, module TEXT, position INTEGER,
  PRIMARY KEY (file_id, source_local_id, seq)
) WITHOUT ROWID;
CREATE INDEX edges_by_target ON edges (target_key_id);

CREATE TABLE signatures (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL,
  returns TEXT, returns_start INTEGER, returns_end INTEGER,
  PRIMARY KEY (file_id, block_local_id)
) WITHOUT ROWID;

CREATE TABLE parameters (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,
  type TEXT, type_start INTEGER, type_end INTEGER,
  PRIMARY KEY (file_id, block_local_id, idx)
) WITHOUT ROWID;

CREATE TABLE members (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,
  type TEXT, type_start INTEGER, type_end INTEGER, value TEXT,
  PRIMARY KEY (file_id, block_local_id, idx)
) WITHOUT ROWID;

CREATE TABLE arguments (
  file_id INTEGER NOT NULL, call_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  expand TEXT, keyword TEXT, parameter INTEGER, unpaired TEXT, bound INTEGER,
  PRIMARY KEY (file_id, call_local_id, idx)
) WITHOUT ROWID;

CREATE TABLE results (
  file_id INTEGER NOT NULL, call_local_id INTEGER NOT NULL,
  binding TEXT NOT NULL, awaited INTEGER NOT NULL, target_local_id INTEGER, position INTEGER,
  bound INTEGER,
  PRIMARY KEY (file_id, call_local_id)
) WITHOUT ROWID;

CREATE TABLE type_links (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, seq INTEGER NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL, text TEXT NOT NULL,
  resolution TEXT NOT NULL, target_key_id INTEGER, reason TEXT, module TEXT, heritage TEXT,
  PRIMARY KEY (file_id, block_local_id, seq)
) WITHOUT ROWID;
CREATE INDEX type_links_by_target ON type_links (target_key_id);

CREATE TABLE entrances (
  id INTEGER PRIMARY KEY,
  key_id INTEGER NOT NULL UNIQUE,
  kind TEXT NOT NULL, label TEXT, method TEXT, trigger TEXT, trigger_status TEXT NOT NULL,
  detector TEXT NOT NULL,
  evidence_path TEXT NOT NULL,
  evidence_start_byte INTEGER NOT NULL, evidence_end_byte INTEGER NOT NULL,
  evidence_block_key_id INTEGER,
  evidence_pointer TEXT
);
CREATE INDEX entrances_by_evidence ON entrances (evidence_path);

CREATE TABLE entrance_targets (
  entrance_id INTEGER NOT NULL, position INTEGER NOT NULL,
  role TEXT NOT NULL,
  target_key_id INTEGER, target_text TEXT NOT NULL,
  resolution TEXT NOT NULL, reason TEXT, module TEXT,
  PRIMARY KEY (entrance_id, position)
);

CREATE TABLE definitions (key_id INTEGER PRIMARY KEY, file_id INTEGER, local_id INTEGER,
                          kind TEXT, qualified_name TEXT);
CREATE INDEX definitions_by_file ON definitions (file_id, local_id);
CREATE TABLE applications (id INTEGER PRIMARY KEY, startup_key_id INTEGER, name TEXT, language TEXT);
CREATE TABLE memberships (entrance_id INTEGER, application_id INTEGER,
                          PRIMARY KEY (entrance_id, application_id));

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
