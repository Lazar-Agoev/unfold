//! The prepared statements one commit uses, taken from the connection's statement cache once
//! per commit and reused for every row.
//!
//! Each statement that deletes rows holding key IDs returns those IDs, so the commit can delete
//! the keys no row refers to any more ([`Statements::delete_unused_key`]).

use rusqlite::{CachedStatement, Connection};

use crate::error::Result;

/// Statements for the file tables and the search index.
pub(crate) struct Statements<'c> {
    pub insert_key: CachedStatement<'c>,
    pub insert_file: CachedStatement<'c>,
    pub update_file: CachedStatement<'c>,
    pub delete_file: CachedStatement<'c>,
    pub delete_nodes: CachedStatement<'c>,
    pub insert_node: CachedStatement<'c>,
    pub delete_links: CachedStatement<'c>,
    pub insert_link: CachedStatement<'c>,
    pub delete_signatures: CachedStatement<'c>,
    pub insert_signature: CachedStatement<'c>,
    pub delete_parameters: CachedStatement<'c>,
    pub insert_parameter: CachedStatement<'c>,
    pub delete_members: CachedStatement<'c>,
    pub insert_member: CachedStatement<'c>,
    pub delete_arguments: CachedStatement<'c>,
    pub insert_argument: CachedStatement<'c>,
    pub delete_definition_search: CachedStatement<'c>,
    pub delete_definitions: CachedStatement<'c>,
    pub insert_definition: CachedStatement<'c>,
    pub insert_search: CachedStatement<'c>,
    pub delete_search: CachedStatement<'c>,
    pub delete_facts: CachedStatement<'c>,
    pub delete_unused_key: CachedStatement<'c>,
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
            delete_nodes: conn.prepare_cached("DELETE FROM nodes WHERE file_id = ?1 RETURNING key_id")?,
            insert_node: conn.prepare_cached(
                "INSERT INTO nodes (file_id, local_id, key_id, parent, form, name, start_byte, end_byte, \
                 start_line, end_line, head_start, head_end, body_start, body_end, doc_start, doc_end, \
                 stmt_start, stmt_end, value_start, value_end, binds, role, for_node, flags, target) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            )?,
            delete_links: conn
                .prepare_cached("DELETE FROM links WHERE file_id = ?1 RETURNING target_key_id")?,
            insert_link: conn.prepare_cached(
                "INSERT INTO links (file_id, from_id, seq, kind, start_byte, end_byte, text, outcome, \
                 target_key_id, module, reason, implements, bound) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?,
            delete_signatures: conn.prepare_cached("DELETE FROM signatures WHERE file_id = ?1")?,
            insert_signature: conn.prepare_cached(
                "INSERT INTO signatures (file_id, node, returns, returns_start, returns_end) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?,
            delete_parameters: conn.prepare_cached("DELETE FROM parameters WHERE file_id = ?1")?,
            insert_parameter: conn.prepare_cached(
                "INSERT INTO parameters (file_id, node, idx, start_byte, end_byte, kind, name, flags, \
                 type, type_start, type_end) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?,
            delete_members: conn.prepare_cached("DELETE FROM members WHERE file_id = ?1")?,
            insert_member: conn.prepare_cached(
                "INSERT INTO members (file_id, node, idx, start_byte, end_byte, kind, name, flags, type, \
                 type_start, type_end, value) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?,
            delete_arguments: conn.prepare_cached("DELETE FROM arguments WHERE file_id = ?1")?,
            insert_argument: conn.prepare_cached(
                "INSERT INTO arguments (file_id, call, idx, start_byte, end_byte, expand, keyword, \
                 parameter, unpaired, bound) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?,
            // A definition's search row ID is its key ID times 4 (`crate::search::rowid`).
            delete_definition_search: conn.prepare_cached(
                "DELETE FROM search WHERE rowid IN (SELECT key_id * 4 FROM definitions WHERE file_id = ?1)",
            )?,
            delete_definitions: conn
                .prepare_cached("DELETE FROM definitions WHERE file_id = ?1 RETURNING key_id")?,
            insert_definition: conn.prepare_cached(
                "INSERT INTO definitions (key_id, file_id, local_id, kind, qualified_name, label) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?,
            insert_search: conn.prepare_cached(
                "INSERT INTO search (rowid, name, qualified_name, path, kind, key) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?,
            delete_search: conn.prepare_cached("DELETE FROM search WHERE rowid = ?1")?,
            delete_facts: conn.prepare_cached("DELETE FROM file_facts WHERE path = ?1")?,
            // Every column that holds a key ID, and the search rows whose row ID is one
            // (`crate::search::rowid`).
            delete_unused_key: conn.prepare_cached(
                "DELETE FROM keys WHERE id = ?1 \
                 AND NOT EXISTS (SELECT 1 FROM nodes WHERE key_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM links WHERE target_key_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM definitions WHERE key_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM apps WHERE id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM entrances WHERE id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM entrance_targets WHERE code_key_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM search WHERE rowid BETWEEN ?1 * 4 AND ?1 * 4 + 3) \
                 RETURNING key",
            )?,
        })
    }
}
