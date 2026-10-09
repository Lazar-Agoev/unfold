//! Files, blocks, slots, and edges.

use rusqlite::Row;
use rustc_hash::FxHashMap;
use unfold_core::{Block, Edge, FileGraph, Graph, Slot};

use super::Reader;
use super::rows::{Cursor, FileRow, IncomingEdge, LocatedBlock, Page};
use crate::codec;
use crate::error::Result;
use crate::update::DiskStamp;

const FILE_COLUMNS: &str =
    "f.path, f.language, f.state, f.facts_hash, f.disk_hash, f.disk_mtime, f.disk_size, f.failure";

/// Decodes [`FILE_COLUMNS`] starting at column `at`.
fn decode_file(row: &Row<'_>, at: usize) -> Result<FileRow> {
    let size: Option<i64> = row.get(at + 6)?;
    let size = size
        .map(|size| u64::try_from(size).map_err(|_| codec::invalid("file size", size.to_string())))
        .transpose()?;
    Ok(FileRow {
        path: codec::path(row, at)?,
        language: codec::vocab(row, at + 1, "language")?,
        state: codec::vocab(row, at + 2, "file state")?,
        facts_hash: codec::opt_hash(row, at + 3)?,
        disk: DiskStamp {
            hash: codec::opt_hash(row, at + 4)?,
            mtime_ns: row.get(at + 5)?,
            size,
        },
        failure: codec::opt_boxed(row, at + 7)?,
    })
}

/// Block columns, from `blocks b` joined with its key `k`.
const BLOCK_COLUMNS: &str = "b.local_id, b.parent_local_id, b.kind, b.role, b.name, \
     b.start_byte, b.end_byte, b.start_line, b.end_line, k.key";

/// Decodes [`BLOCK_COLUMNS`] starting at column `at`.
fn decode_block(row: &Row<'_>, at: usize) -> Result<Block> {
    Ok(Block {
        local_id: codec::u32_at(row, at)?,
        parent: codec::opt_u32(row, at + 1)?,
        kind: codec::vocab(row, at + 2, "block kind")?,
        role: codec::opt_vocab(row, at + 3, "role")?,
        name: codec::opt_boxed(row, at + 4)?,
        range: codec::range(row, at + 5)?,
        lines: codec::lines(row, at + 7)?,
        key: codec::key(row, at + 9)?,
    })
}

fn decode_slot(row: &Row<'_>) -> Result<Slot> {
    let index: i64 = row.get(2)?;
    Ok(Slot {
        block: codec::u32_at(row, 0)?,
        slot: codec::vocab(row, 1, "slot")?,
        index: if index < 0 {
            None
        } else {
            Some(u32::try_from(index).map_err(|_| codec::invalid("slot index", index.to_string()))?)
        },
        range: codec::range(row, 3)?,
    })
}

fn decode_edge(row: &Row<'_>) -> Result<Edge> {
    Ok(Edge {
        source: codec::u32_at(row, 0)?,
        seq: codec::u32_at(row, 1)?,
        kind: codec::vocab(row, 2, "edge kind")?,
        target: codec::opt_key(row, 3)?,
        text: codec::text(row, 4)?.into(),
        resolution: codec::vocab(row, 5, "resolution")?,
        reason: codec::opt_vocab(row, 6, "reason")?,
        module: codec::opt_module(row, 7)?,
        position: codec::opt_u32(row, 8)?,
    })
}

/// Runs `statement` with `params` and decodes every row.
pub(crate) fn collect<T>(
    statement: &mut rusqlite::CachedStatement<'_>,
    params: impl rusqlite::Params,
    mut decode: impl FnMut(&Row<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let mut rows = statement.query(params)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(decode(row)?);
    }
    Ok(items)
}

impl Reader {
    /// Every file, in path order, with its state, hashes, and disk stamp.
    pub fn files(&self) -> Result<Vec<FileRow>> {
        let mut statement = self
            .conn()
            .prepare_cached(&format!("SELECT {FILE_COLUMNS} FROM files f ORDER BY f.path"))?;
        collect(&mut statement, [], |row| decode_file(row, 0))
    }

    /// The file at `path`, if indexed.
    pub fn file(&self, path: &str) -> Result<Option<FileRow>> {
        let mut statement = self
            .conn()
            .prepare_cached(&format!("SELECT {FILE_COLUMNS} FROM files f WHERE f.path = ?1"))?;
        Ok(collect(&mut statement, [path], |row| decode_file(row, 0))?
            .into_iter()
            .next())
    }

    /// One file's record, blocks (preorder), slots (by block, then start), and edges (by
    /// source, then sequence), from one snapshot.
    pub fn file_graph(&self, path: &str) -> Result<Option<FileGraph>> {
        self.snapshot(|reader| {
            let conn = reader.conn();
            let mut statement = conn.prepare_cached(&format!(
                "SELECT f.id, {FILE_COLUMNS} FROM files f WHERE f.path = ?1"
            ))?;
            let Some((file_id, row)) = collect(&mut statement, [path], |row| {
                let id: i64 = row.get(0)?;
                Ok((id, decode_file(row, 1)?))
            })?
            .into_iter()
            .next() else {
                return Ok(None);
            };
            let mut blocks = conn.prepare_cached(&format!(
                "SELECT {BLOCK_COLUMNS} FROM blocks b JOIN keys k ON k.id = b.key_id \
                 WHERE b.file_id = ?1 ORDER BY b.local_id"
            ))?;
            let blocks = collect(&mut blocks, [file_id], |row| decode_block(row, 0))?;
            let mut slots = conn.prepare_cached(
                "SELECT block_local_id, slot, idx, start_byte, end_byte FROM slots WHERE file_id = ?1 \
                 ORDER BY block_local_id, start_byte, end_byte, slot, idx",
            )?;
            let slots = collect(&mut slots, [file_id], decode_slot)?;
            let mut edges = conn.prepare_cached(
                "SELECT e.source_local_id, e.seq, e.kind, k.key, e.target_text, e.resolution, e.reason, \
                 e.module, e.position FROM edges e LEFT JOIN keys k ON k.id = e.target_key_id \
                 WHERE e.file_id = ?1 ORDER BY e.source_local_id, e.seq",
            )?;
            let edges = collect(&mut edges, [file_id], decode_edge)?;
            Ok(Some(FileGraph {
                file: row.record(),
                blocks,
                slots,
                edges,
            }))
        })
    }

    /// The whole committed graph from one snapshot: every file with its blocks, slots, and
    /// edges (in the orders [`Reader::file_graph`] returns), the inputs, and the entrances in
    /// canonical order. Three bulk queries instead of one round per file: the workspace uses it
    /// to restore the engine's in-memory program when a folder reopens.
    pub fn graph(&self) -> Result<Graph> {
        self.snapshot(|reader| {
            let conn = reader.conn();
            let mut statement = conn.prepare_cached(&format!(
                "SELECT f.id, {FILE_COLUMNS} FROM files f ORDER BY f.path"
            ))?;
            let rows = collect(&mut statement, [], |row| {
                let id: i64 = row.get(0)?;
                Ok((id, decode_file(row, 1)?))
            })?;
            let mut index = FxHashMap::with_capacity_and_hasher(rows.len(), Default::default());
            let mut files = Vec::with_capacity(rows.len());
            for (position, (id, row)) in rows.into_iter().enumerate() {
                index.insert(id, position);
                files.push(FileGraph {
                    file: row.record(),
                    blocks: Vec::new(),
                    slots: Vec::new(),
                    edges: Vec::new(),
                });
            }
            let owner = |id: i64| -> Result<usize> {
                index
                    .get(&id)
                    .copied()
                    .ok_or_else(|| codec::invalid("file id", id.to_string()))
            };
            let mut blocks = conn.prepare_cached(&format!(
                "SELECT b.file_id, {BLOCK_COLUMNS} FROM blocks b JOIN keys k ON k.id = b.key_id \
                 ORDER BY b.file_id, b.local_id"
            ))?;
            let mut rows = blocks.query([])?;
            while let Some(row) = rows.next()? {
                let block = decode_block(row, 1)?;
                files[owner(row.get(0)?)?].blocks.push(block);
            }
            let mut slots = conn.prepare_cached(
                "SELECT block_local_id, slot, idx, start_byte, end_byte, file_id FROM slots \
                 ORDER BY file_id, block_local_id, start_byte, end_byte, slot, idx",
            )?;
            let mut rows = slots.query([])?;
            while let Some(row) = rows.next()? {
                let slot = decode_slot(row)?;
                files[owner(row.get(5)?)?].slots.push(slot);
            }
            let mut edges = conn.prepare_cached(
                "SELECT e.source_local_id, e.seq, e.kind, k.key, e.target_text, e.resolution, e.reason, \
                 e.module, e.position, e.file_id FROM edges e LEFT JOIN keys k ON k.id = e.target_key_id \
                 ORDER BY e.file_id, e.source_local_id, e.seq",
            )?;
            let mut rows = edges.query([])?;
            while let Some(row) = rows.next()? {
                let edge = decode_edge(row)?;
                files[owner(row.get(9)?)?].edges.push(edge);
            }
            Ok(Graph {
                files,
                inputs: reader.inputs()?,
                entrances: reader.entrances()?,
            })
        })
    }

    /// The block with `key`, with its file.
    pub fn block(&self, key: &str) -> Result<Option<LocatedBlock>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, f.language, {BLOCK_COLUMNS} FROM keys k \
             JOIN blocks b ON b.key_id = k.id JOIN files f ON f.id = b.file_id WHERE k.key = ?1"
        ))?;
        let found = collect(&mut statement, [key], |row| {
            Ok(LocatedBlock {
                path: codec::path(row, 0)?,
                language: codec::vocab(row, 1, "language")?,
                block: decode_block(row, 2)?,
            })
        })?;
        Ok(found.into_iter().next())
    }

    /// The direct children of the block with `key`, in preorder. Empty when the block does not
    /// exist or has no children.
    pub fn children(&self, key: &str) -> Result<Vec<Block>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT {BLOCK_COLUMNS} FROM keys pk \
             JOIN blocks p ON p.key_id = pk.id \
             JOIN blocks b ON b.file_id = p.file_id AND b.local_id > p.local_id \
                          AND b.parent_local_id = p.local_id \
             JOIN keys k ON k.id = b.key_id \
             WHERE pk.key = ?1 ORDER BY b.local_id"
        ))?;
        collect(&mut statement, [key], |row| decode_block(row, 0))
    }

    /// Resolved edges whose target is `key`, with their source occurrences, one page at a
    /// time (pass the previous page's `next` as `after`). The order is stable but is not path
    /// order; callers sort what they show.
    pub fn incoming(&self, key: &str, after: Option<Cursor>, limit: usize) -> Result<Page<IncomingEdge>> {
        let [file, local, seq] = after.map_or([i64::MIN, i64::MIN, i64::MIN], |cursor| cursor.0);
        let mut statement = self.conn().prepare_cached(
            "SELECT e.file_id, e.source_local_id, e.seq, e.kind, e.target_text, e.position, f.path, sk.key \
             FROM keys tk \
             JOIN edges e ON e.target_key_id = tk.id \
             JOIN files f ON f.id = e.file_id \
             JOIN blocks b ON b.file_id = e.file_id AND b.local_id = e.source_local_id \
             JOIN keys sk ON sk.id = b.key_id \
             WHERE tk.key = ?1 AND (e.file_id, e.source_local_id, e.seq) > (?2, ?3, ?4) \
             ORDER BY e.file_id, e.source_local_id, e.seq LIMIT ?5",
        )?;
        let fetch = i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1);
        let mut items = collect(&mut statement, (key, file, local, seq, fetch), |row| {
            Ok((
                [row.get(0)?, row.get(1)?, row.get(2)?],
                IncomingEdge {
                    source_local_id: codec::u32_at(row, 1)?,
                    seq: codec::u32_at(row, 2)?,
                    kind: codec::vocab(row, 3, "edge kind")?,
                    text: codec::text(row, 4)?.into(),
                    position: codec::opt_u32(row, 5)?,
                    source_path: codec::path(row, 6)?,
                    source: codec::key(row, 7)?,
                },
            ))
        })?;
        let next = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|(position, _)| Cursor(*position))
        } else {
            None
        };
        Ok(Page {
            items: items.into_iter().map(|(_, edge)| edge).collect(),
            next,
        })
    }

    /// How many resolved edges target `key`.
    pub fn incoming_count(&self, key: &str) -> Result<u64> {
        let mut statement = self.conn().prepare_cached(
            "SELECT count(*) FROM keys k JOIN edges e ON e.target_key_id = k.id WHERE k.key = ?1",
        )?;
        let counts = collect(&mut statement, [key], |row| codec::count(row, 0))?;
        Ok(counts.into_iter().next().unwrap_or(0))
    }
}
