//! Read-only access for every thread other than the engine's.
//!
//! [`ReadPool`] is cheap to clone and shareable across threads. [`ReadPool::get`] hands out a
//! [`Reader`], a `Send` handle over one read-only connection that returns to the pool when
//! dropped. The pool never blocks: when every pooled connection is in use it opens another, and
//! it keeps at most a few idle ones.
//!
//! Under write-ahead logging a reader always sees the last committed graph, even while the
//! writer is in the middle of a commit. Each query sees one snapshot; queries that combine
//! several statements (a file with its blocks, slots, and edges; entrances with their targets)
//! run them in one read transaction. [`Reader::snapshot`] does the same for a caller's own
//! sequence of queries.
//!
//! Queries are grouped by subject: [`graph`] (files, blocks, edges), [`catalog`] (entrances,
//! definitions, applications), [`search`], and [`info`] (inputs, metadata, statistics). They
//! return core types or the small row types in [`rows`]; no SQL leaves the crate.

pub mod catalog;
pub mod graph;
pub mod info;
pub mod rows;
pub mod search;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;
use crate::schema::{READER_PRAGMAS, STATEMENT_CACHE_CAPACITY};

/// A pool of read-only connections to one database.
#[derive(Debug, Clone)]
pub struct ReadPool {
    inner: Arc<PoolInner>,
}

#[derive(Debug)]
struct PoolInner {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
    max_idle: usize,
}

impl ReadPool {
    pub(crate) fn new(path: PathBuf, max_idle: usize) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                path,
                idle: Mutex::new(Vec::new()),
                max_idle,
            }),
        }
    }

    /// A reader: a pooled connection, or a new one when none is idle.
    pub fn get(&self) -> Result<Reader> {
        // The vector stays consistent even if a holder panicked, so a poisoned lock is usable.
        let pooled = self
            .inner
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        let conn = match pooled {
            Some(conn) => conn,
            None => connect_reader(&self.inner.path)?,
        };
        Ok(Reader {
            conn: Some(conn),
            pool: Arc::clone(&self.inner),
        })
    }
}

fn connect_reader(path: &std::path::Path) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);
    conn.execute_batch(READER_PRAGMAS)?;
    Ok(conn)
}

/// One read-only connection, returned to its pool on drop. It is `Send`, not `Sync`: use it
/// from one thread at a time.
#[derive(Debug)]
pub struct Reader {
    conn: Option<Connection>,
    pool: Arc<PoolInner>,
}

impl Reader {
    fn conn(&self) -> &Connection {
        self.conn
            .as_ref()
            .expect("a reader holds its connection until it is dropped")
    }

    /// Runs `queries` against one snapshot of the graph, so they all see the same commit.
    /// Nested calls share the outer snapshot.
    pub fn snapshot<T>(&self, queries: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        let conn = self.conn();
        if !conn.is_autocommit() {
            return queries(self);
        }
        let tx = conn.unchecked_transaction()?;
        let result = queries(self)?;
        // Ending a read transaction cannot lose data; commit just releases the snapshot.
        tx.commit()?;
        Ok(result)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let Some(conn) = self.conn.take() else {
            return;
        };
        // A connection left inside a transaction (a panic mid-snapshot) is closed, not reused.
        if !conn.is_autocommit() {
            return;
        }
        let mut idle = self.pool.idle.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.len() < self.pool.max_idle {
            idle.push(conn);
        }
    }
}
