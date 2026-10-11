//! The single writer of one folder's database.
//!
//! The folder's engine thread owns the [`Writer`]. [`Writer::commit`] applies one
//! [`Update`] in one `BEGIN IMMEDIATE` transaction, in this order:
//!
//! 1. removed files;
//! 2. changed files (row, then blocks and slots, edges, and definitions as the change says);
//! 3. entrance removals and upserts;
//! 4. inputs, applications, and memberships, when supplied;
//! 5. metadata.
//!
//! [`Writer::replace`] applies an update that describes the whole graph the same way, and also
//! removes every stored file and entrance the update does not name: the full rewrite for a
//! database whose contents the caller no longer knows (after a failed commit, say).
//!
//! Any error rolls the transaction back, and the in-memory mirror of keys and file
//! IDs forgets what the transaction added, so a failed commit leaves the database and the
//! writer exactly as they were. Readers never observe a partial commit (write-ahead logging).
//! A transaction a panic left open is rolled back by the writer's next operation.
//!
//! The writer also owns the per-file facts cache ([`facts`]).

mod catalog;
pub mod facts;
mod graph;
mod mirror;
mod statements;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use self::graph::Apply;
use self::mirror::Mirror;
use self::statements::Statements;
use crate::error::{Result, StoreError};
use crate::schema::meta_key;
use crate::update::Update;

/// What a commit wrote: for the Index log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommitStats {
    /// File rows inserted or updated.
    pub files_written: u64,
    /// Files removed.
    pub files_removed: u64,
    /// Blocks written.
    pub blocks_written: u64,
    /// Slots written.
    pub slots_written: u64,
    /// Edges written.
    pub edges_written: u64,
    /// Typed records written (signatures, parameters, members, arguments, results, and type
    /// links).
    pub typed_records_written: u64,
    /// Definitions written.
    pub definitions_written: u64,
    /// Entrances inserted or replaced.
    pub entrances_written: u64,
    /// Entrances removed.
    pub entrances_removed: u64,
    /// Wall time of the transaction.
    pub elapsed: Duration,
}

/// Where a test-only hook runs during a commit.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// After removed files are deleted.
    AfterRemovals,
    /// After this many changed files are written.
    AfterFiles(u64),
    /// After entrances are written.
    AfterEntrances,
    /// Just before `COMMIT`.
    BeforeCommit,
}

/// A test-only hook called at each [`FaultPoint`]; returning an error aborts the commit.
#[cfg(test)]
pub(crate) type FaultHook = Box<dyn FnMut(FaultPoint) -> Result<()> + Send>;

/// Runs the test-only hook at a fault point; expands to nothing outside tests.
macro_rules! fault {
    ($hook:ident, $point:expr) => {
        #[cfg(test)]
        if let Some(hook) = $hook.as_mut() {
            hook($point)?;
        }
    };
}

/// Whether an update describes changes or the whole graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// [`Writer::commit`].
    Delta,
    /// [`Writer::replace`].
    Replace,
}

impl Mode {
    /// The set to write for a replaceable part: as given, or empty when replacing.
    fn set<T>(self, part: Option<&[T]>) -> Option<&[T]> {
        match self {
            Self::Delta => part,
            Self::Replace => Some(part.unwrap_or_default()),
        }
    }
}

/// The database's only writer. It is `Send`, so the engine thread can own it.
pub struct Writer {
    conn: Connection,
    path: PathBuf,
    engine_version: Box<str>,
    /// Loaded on first use, so opening a folder does not wait for every key to load. `None`
    /// also after an interrupted transaction, to force a reload.
    mirror: Option<Mirror>,
    #[cfg(test)]
    pub(crate) hook: Option<FaultHook>,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer")
            .field("path", &self.path)
            .field("engine_version", &self.engine_version)
            .finish_non_exhaustive()
    }
}

impl Writer {
    pub(crate) fn new(conn: Connection, path: PathBuf, engine_version: &str) -> Self {
        Self {
            conn,
            path,
            engine_version: engine_version.into(),
            mirror: None,
            #[cfg(test)]
            hook: None,
        }
    }

    /// The database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The engine version the database belongs to.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// Applies `update` in one transaction. On error nothing is written.
    pub fn commit(&mut self, update: &Update<'_>) -> Result<CommitStats> {
        self.run(update, Mode::Delta, &|| false)
    }

    /// [`Writer::commit`], abandoned with [`StoreError::Cancelled`] (writing nothing) when
    /// `cancelled` answers `true`; it is asked between files and before the transaction
    /// commits.
    pub fn commit_unless(
        &mut self,
        update: &Update<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<CommitStats> {
        self.run(update, Mode::Delta, cancelled)
    }

    /// Makes the stored graph exactly `update`, in one transaction: like [`Writer::commit`],
    /// and every stored file not in [`Update::files`] and every stored entrance not in
    /// `entrances.upserts` is removed; `None` inputs, applications, or memberships mean empty
    /// sets. For a full rewrite of a database whose contents the caller does not track, such as
    /// after a failed commit: readers keep the previous graph until this one commits.
    pub fn replace(&mut self, update: &Update<'_>) -> Result<CommitStats> {
        self.run(update, Mode::Replace, &|| false)
    }

    /// [`Writer::replace`], abandoned like [`Writer::commit_unless`].
    pub fn replace_unless(
        &mut self,
        update: &Update<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<CommitStats> {
        self.run(update, Mode::Replace, cancelled)
    }

    fn run(&mut self, update: &Update<'_>, mode: Mode, cancelled: &dyn Fn() -> bool) -> Result<CommitStats> {
        let started = Instant::now();
        self.recover()?;
        if self.mirror.is_none() {
            self.mirror = Some(Mirror::load(&self.conn)?);
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.apply(update, mode, cancelled).and_then(|stats| {
            if cancelled() {
                return Err(StoreError::Cancelled);
            }
            self.conn.execute_batch("COMMIT")?;
            Ok(stats)
        });
        match result {
            Ok(mut stats) => {
                if let Some(mirror) = &mut self.mirror {
                    mirror.commit();
                }
                stats.elapsed = started.elapsed();
                Ok(stats)
            }
            Err(error) => {
                if !self.conn.is_autocommit() && self.conn.execute_batch("ROLLBACK").is_err() {
                    // The connection is in an unknown state; reload everything next time.
                    self.mirror = None;
                }
                if let Some(mirror) = &mut self.mirror {
                    mirror.rollback();
                }
                Err(error)
            }
        }
    }

    /// Rolls back a transaction a panic left open (the engine thread's writer mutex tolerates
    /// poisoning) and forgets the mirror, which may hold that transaction's additions. Every
    /// writing operation starts here, so one interrupted commit cannot wedge the writer.
    fn recover(&mut self) -> Result<()> {
        if !self.conn.is_autocommit() {
            self.conn.execute_batch("ROLLBACK")?;
            self.mirror = None;
        }
        Ok(())
    }

    /// Writes every part of `update` inside the open transaction.
    fn apply(
        &mut self,
        update: &Update<'_>,
        mode: Mode,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<CommitStats> {
        let Self {
            conn,
            mirror,
            #[cfg(test)]
            hook,
            ..
        } = self;
        let mirror = mirror.as_mut().expect("commit loads the mirror before applying");
        mirror.begin();
        let mut apply = Apply {
            st: Statements::new(conn)?,
            mirror,
            stats: CommitStats::default(),
        };
        for path in &update.removed_files {
            apply.remove_file(path)?;
        }
        if mode == Mode::Replace {
            apply.remove_files_except(&update.files)?;
        }
        fault!(hook, FaultPoint::AfterRemovals);
        for change in &update.files {
            if cancelled() {
                return Err(StoreError::Cancelled);
            }
            apply.write_file(change)?;
            fault!(hook, FaultPoint::AfterFiles(apply.stats.files_written));
        }
        if mode == Mode::Replace {
            apply.remove_entrances_except(conn, &update.entrances.upserts)?;
        }
        apply.write_entrances(conn, &update.entrances)?;
        fault!(hook, FaultPoint::AfterEntrances);
        if let Some(inputs) = mode.set(update.inputs) {
            apply.write_inputs(conn, inputs)?;
        }
        if let Some(applications) = mode.set(update.applications) {
            apply.write_applications(conn, applications)?;
        }
        if let Some(memberships) = mode.set(update.memberships) {
            apply.write_memberships(conn, memberships)?;
        }
        if let Some(meta) = &update.meta {
            catalog::write_meta(conn, meta)?;
        }
        fault!(hook, FaultPoint::BeforeCommit);
        Ok(apply.stats)
    }

    /// Deletes the graph, the facts cache, and the search index, keeping the schema and
    /// versions: Rebuild from scratch (docs/engine.md#updates). Key IDs start again from 1.
    /// Readers keep working and see an empty graph once this commits.
    pub fn clear(&mut self) -> Result<()> {
        self.recover()?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "DELETE FROM files; DELETE FROM inputs; DELETE FROM keys; DELETE FROM blocks;
             DELETE FROM slots; DELETE FROM edges; DELETE FROM entrances;
             DELETE FROM entrance_targets; DELETE FROM definitions; DELETE FROM applications;
             DELETE FROM memberships; DELETE FROM file_facts; DELETE FROM search;",
        )?;
        tx.execute(
            "DELETE FROM meta WHERE key NOT IN (?1, ?2)",
            (meta_key::SCHEMA_VERSION, meta_key::ENGINE_VERSION),
        )?;
        tx.commit()?;
        // Reloaded by the next commit; never left holding keys the tables no longer have.
        self.mirror = None;
        Ok(())
    }

    /// The writer's connection, for tests that inspect rows directly.
    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The writer's mirror, for tests that check it against the tables.
    #[cfg(test)]
    pub(crate) fn mirror(&self) -> Option<&Mirror> {
        self.mirror.as_ref()
    }
}
