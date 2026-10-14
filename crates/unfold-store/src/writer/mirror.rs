//! The writer's in-memory mirror of the `keys` table and of file IDs, so that writes never
//! query per key or per file.
//!
//! IDs are assigned by the mirror (one more than the largest ID ever assigned) and inserted
//! with the row. Everything added during a transaction is journaled: if the transaction rolls
//! back, the mirror forgets those entries, so it always equals the committed tables.

use rusqlite::{CachedStatement, Connection};
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::{Key, RepoPath};

use crate::codec;
use crate::error::Result;

/// Interned keys and file IDs, mirroring the committed database.
#[derive(Debug)]
pub(crate) struct Mirror {
    keys: FxHashMap<Key, i64>,
    next_key: i64,
    files: FxHashMap<RepoPath, i64>,
    next_file: i64,
    journal: Journal,
}

/// What the open transaction changed, in order, and the ID counters before it began.
#[derive(Debug, Default)]
struct Journal {
    ops: Vec<Op>,
    next_key: i64,
    next_file: i64,
}

/// One journaled change to the mirror.
#[derive(Debug)]
enum Op {
    AddKey(Key),
    AddFile(RepoPath),
    RemoveFile(RepoPath, i64),
}

impl Mirror {
    /// Loads every key and file ID.
    pub(crate) fn load(conn: &Connection) -> Result<Self> {
        let key_count: i64 = conn.query_row("SELECT count(*) FROM keys", [], |row| row.get(0))?;
        let mut keys = FxHashMap::default();
        keys.reserve(usize::try_from(key_count).unwrap_or(0));
        let mut next_key = 1;
        let mut statement = conn.prepare("SELECT id, key FROM keys")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            next_key = next_key.max(id + 1);
            keys.insert(codec::key(row, 1)?, id);
        }
        let mut files = FxHashMap::default();
        let mut next_file = 1;
        let mut statement = conn.prepare("SELECT id, path FROM files")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            next_file = next_file.max(id + 1);
            files.insert(codec::path(row, 1)?, id);
        }
        let mut mirror = Self {
            keys,
            next_key,
            files,
            next_file,
            journal: Journal::default(),
        };
        mirror.checkpoint();
        Ok(mirror)
    }

    /// Starts journaling a transaction.
    pub(crate) fn begin(&mut self) {
        self.checkpoint();
    }

    /// Keeps everything the transaction added.
    pub(crate) fn commit(&mut self) {
        self.checkpoint();
    }

    /// Makes the current state the one a rollback returns to, so a rollback without any
    /// journaled change is a no-op.
    fn checkpoint(&mut self) {
        self.journal = Journal {
            ops: Vec::new(),
            next_key: self.next_key,
            next_file: self.next_file,
        };
    }

    /// Forgets everything the transaction added and restores what it removed.
    pub(crate) fn rollback(&mut self) {
        let journal = std::mem::take(&mut self.journal);
        for op in journal.ops.into_iter().rev() {
            match op {
                Op::AddKey(key) => {
                    self.keys.remove(key.as_str());
                }
                Op::AddFile(path) => {
                    self.files.remove(path.as_str());
                }
                Op::RemoveFile(path, id) => {
                    self.files.insert(path, id);
                }
            }
        }
        self.next_key = journal.next_key;
        self.next_file = journal.next_file;
        self.checkpoint();
    }

    /// The ID of `key`, if it was ever interned.
    pub(crate) fn key_id(&self, key: &str) -> Option<i64> {
        self.keys.get(key).copied()
    }

    /// The ID of `key`, interning it with `insert` (`INSERT INTO keys (id, key)`) if needed.
    /// The key's text is shared with the caller's graph, not copied.
    pub(crate) fn intern(&mut self, key: &Key, insert: &mut CachedStatement<'_>) -> Result<i64> {
        if let Some(&id) = self.keys.get(key.as_str()) {
            return Ok(id);
        }
        let id = self.next_key;
        insert.execute((id, key.as_str()))?;
        self.next_key += 1;
        self.keys.insert(key.clone(), id);
        self.journal.ops.push(Op::AddKey(key.clone()));
        Ok(id)
    }

    /// The ID of the file at `path`, if stored.
    pub(crate) fn file_id(&self, path: &str) -> Option<i64> {
        self.files.get(path).copied()
    }

    /// Reserves an ID for a new file at `path`; the caller inserts the row.
    pub(crate) fn add_file(&mut self, path: &RepoPath) -> i64 {
        let id = self.next_file;
        self.next_file += 1;
        self.files.insert(path.clone(), id);
        self.journal.ops.push(Op::AddFile(path.clone()));
        id
    }

    /// The stored paths not in `keep`, sorted so removals run in a stable order.
    pub(crate) fn file_paths_except(&self, keep: &FxHashSet<&str>) -> Vec<RepoPath> {
        let mut paths: Vec<RepoPath> = self
            .files
            .keys()
            .filter(|path| !keep.contains(path.as_str()))
            .cloned()
            .collect();
        paths.sort_unstable();
        paths
    }

    /// Forgets the file at `path`, returning its ID.
    pub(crate) fn remove_file(&mut self, path: &str) -> Option<i64> {
        let (path, id) = self.files.remove_entry(path)?;
        self.journal.ops.push(Op::RemoveFile(path, id));
        Some(id)
    }

    /// The number of interned keys.
    #[cfg(test)]
    pub(crate) fn key_count(&self) -> usize {
        self.keys.len()
    }
}
