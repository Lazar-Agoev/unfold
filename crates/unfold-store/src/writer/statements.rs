//! The prepared statements one commit uses, taken from the connection's statement cache once
//! per commit and reused for every row.

use rusqlite::{CachedStatement, Connection};

use crate::error::Result;

/// Statements for the graph tables and the search index.
pub(crate) struct Statements<'c> {
    pub insert_key: CachedStatement<'c>,
    pub insert_file: CachedStatement<'c>,
    pub update_file: CachedStatement<'c>,
    pub delete_file: CachedStatement<'c>,
    pub delete_blocks: CachedStatement<'c>,
    pub insert_block: CachedStatement<'c>,
    pub delete_slots: CachedStatement<'c>,
    pub insert_slot: CachedStatement<'c>,
    pub delete_edges: CachedStatement<'c>,
    pub insert_edge: CachedStatement<'c>,
    pub delete_definition_search: CachedStatement<'c>,
    pub delete_definitions: CachedStatement<'c>,
    pub insert_definition: CachedStatement<'c>,
    pub insert_search: CachedStatement<'c>,
    pub delete_search: CachedStatement<'c>,
    pub delete_facts: CachedStatement<'c>,
}

impl<'c> Statements<'c> {
    pub(crate) fn new(conn: &'c Connection) -> Result<Self> {
        Ok(Self {
            insert_key: conn.prepare_cached("INSERT INTO keys (id, key) VALUES (?1, ?2)")?,
            insert_file: conn.prepare_cached(
                "INSERT INTO files (id, path, language, state, facts_hash, disk_hash, disk_mtime, \
                 disk_size, failure) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?,
            update_file: conn.prepare_cached(
                "UPDATE files SET language = ?2, state = ?3, facts_hash = ?4, disk_hash = ?5, \
                 disk_mtime = ?6, disk_size = ?7, failure = ?8 WHERE id = ?1",
            )?,
            delete_file: conn.prepare_cached("DELETE FROM files WHERE id = ?1")?,
            delete_blocks: conn.prepare_cached("DELETE FROM blocks WHERE file_id = ?1")?,
            insert_block: conn.prepare_cached(
                "INSERT INTO blocks (file_id, local_id, key_id, parent_local_id, kind, role, name, \
                 start_byte, end_byte, start_line, end_line) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?,
            delete_slots: conn.prepare_cached("DELETE FROM slots WHERE file_id = ?1")?,
            insert_slot: conn.prepare_cached(
                "INSERT INTO slots (file_id, block_local_id, slot, idx, start_byte, end_byte) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?,
            delete_edges: conn.prepare_cached("DELETE FROM edges WHERE file_id = ?1")?,
            insert_edge: conn.prepare_cached(
                "INSERT INTO edges (file_id, source_local_id, seq, kind, target_key_id, target_text, \
                 resolution, reason, module, position) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?,
            // A definition's search row ID is its key ID times 4 (`crate::search::rowid`).
            delete_definition_search: conn.prepare_cached(
                "DELETE FROM search WHERE rowid IN (SELECT key_id * 4 FROM definitions WHERE file_id = ?1)",
            )?,
            delete_definitions: conn.prepare_cached("DELETE FROM definitions WHERE file_id = ?1")?,
            insert_definition: conn.prepare_cached(
                "INSERT INTO definitions (key_id, file_id, local_id, kind, qualified_name) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?,
            insert_search: conn.prepare_cached(
                "INSERT INTO search (rowid, name, qualified_name, path, kind, key) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?,
            delete_search: conn.prepare_cached("DELETE FROM search WHERE rowid = ?1")?,
            delete_facts: conn.prepare_cached("DELETE FROM file_facts WHERE path = ?1")?,
        })
    }
}
