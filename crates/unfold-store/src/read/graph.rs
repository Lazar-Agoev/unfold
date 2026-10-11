//! Files, blocks, slots, edges, typed records, and the reverse index of type links.

use rusqlite::{Connection, Row};
use rustc_hash::FxHashMap;
use unfold_core::{
    Argument, Block, CallResult, Edge, FileGraph, FileTypes, Graph, Member, Pairing, Parameter, Signature,
    Slot, TypeLink, WrittenType,
};

use super::Reader;
use super::rows::{Cursor, FileRow, IncomingEdge, LocatedBlock, Page, TypeUse};
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

/// A written type from three columns (text, start, end), present when the text is.
fn decode_written(row: &Row<'_>, at: usize) -> Result<Option<WrittenType>> {
    let Some(text) = codec::opt_boxed(row, at)? else {
        return Ok(None);
    };
    Ok(Some(WrittenType {
        text,
        range: codec::range(row, at + 1)?,
    }))
}

/// A flag bit of an integer column.
fn flag(row: &Row<'_>, at: usize, bit: u32) -> Result<bool> {
    Ok(codec::u32_at(row, at)? & (1 << bit) != 0)
}

/// The typed record queries, each filtered by file (`?1`) or, with `-1`, over every file, and
/// ending with the file ID column.
const SIGNATURES: &str = "SELECT block_local_id, returns, returns_start, returns_end, file_id \
     FROM signatures WHERE ?1 IN (file_id, -1) ORDER BY file_id, block_local_id";
const PARAMETERS: &str = "SELECT block_local_id, idx, kind, name, flags, type, type_start, type_end, \
     file_id FROM parameters WHERE ?1 IN (file_id, -1) ORDER BY file_id, block_local_id, idx";
const MEMBERS: &str = "SELECT block_local_id, idx, kind, name, flags, type, type_start, type_end, value, \
     file_id FROM members WHERE ?1 IN (file_id, -1) ORDER BY file_id, block_local_id, idx";
const ARGUMENTS: &str = "SELECT call_local_id, idx, expand, keyword, parameter, unpaired, file_id, \
     bound FROM arguments WHERE ?1 IN (file_id, -1) ORDER BY file_id, call_local_id, idx";
const RESULTS: &str = "SELECT call_local_id, binding, awaited, target_local_id, position, file_id, \
     bound FROM results WHERE ?1 IN (file_id, -1) ORDER BY file_id, call_local_id";
const TYPE_LINKS: &str = "SELECT t.block_local_id, t.start_byte, t.end_byte, t.text, t.resolution, \
     k.key, t.reason, t.module, t.file_id, t.heritage FROM type_links t \
     LEFT JOIN keys k ON k.id = t.target_key_id \
     WHERE ?1 IN (t.file_id, -1) ORDER BY t.file_id, t.block_local_id, t.seq";

/// Reads the typed records of file `file_id` (or of every file, with `-1`) by file ID, each
/// file's records in canonical order.
fn read_types(conn: &Connection, file_id: i64) -> Result<FxHashMap<i64, FileTypes>> {
    let mut files: FxHashMap<i64, FileTypes> = FxHashMap::default();
    let mut statement = conn.prepare_cached(SIGNATURES)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let signature = Signature {
            block: codec::u32_at(row, 0)?,
            returns: decode_written(row, 1)?,
        };
        files.entry(row.get(4)?).or_default().signatures.push(signature);
    }
    let mut statement = conn.prepare_cached(PARAMETERS)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let parameter = Parameter {
            block: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            kind: codec::vocab(row, 2, "parameter kind")?,
            name: codec::opt_boxed(row, 3)?,
            optional: flag(row, 4, 0)?,
            default: flag(row, 4, 1)?,
            ty: decode_written(row, 5)?,
        };
        files.entry(row.get(8)?).or_default().parameters.push(parameter);
    }
    let mut statement = conn.prepare_cached(MEMBERS)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let member = Member {
            block: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            kind: codec::vocab(row, 2, "member kind")?,
            name: codec::opt_boxed(row, 3)?,
            optional: flag(row, 4, 0)?,
            readonly: flag(row, 4, 1)?,
            default: flag(row, 4, 2)?,
            ty: decode_written(row, 5)?,
            value: codec::opt_boxed(row, 8)?,
        };
        files.entry(row.get(9)?).or_default().members.push(member);
    }
    let mut statement = conn.prepare_cached(ARGUMENTS)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let pairing = match (
            codec::opt_u32(row, 4)?,
            codec::opt_vocab(row, 5, "pairing reason")?,
        ) {
            (Some(index), None) => Some(Pairing::Parameter(index)),
            (None, Some(reason)) => Some(Pairing::Unpaired(reason)),
            (None, None) => None,
            (Some(_), Some(_)) => return Err(codec::invalid("pairing", "a parameter and a reason")),
        };
        let argument = Argument {
            call: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            expand: codec::opt_vocab(row, 2, "expansion")?,
            keyword: codec::opt_boxed(row, 3)?,
            pairing,
            bound: codec::opt_u32(row, 7)?,
        };
        files.entry(row.get(6)?).or_default().arguments.push(argument);
    }
    let mut statement = conn.prepare_cached(RESULTS)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let result = CallResult {
            call: codec::u32_at(row, 0)?,
            binding: codec::vocab(row, 1, "result binding")?,
            awaited: row.get(2)?,
            target: codec::opt_u32(row, 3)?,
            position: codec::opt_u32(row, 4)?,
            bound: codec::opt_u32(row, 6)?,
        };
        files.entry(row.get(5)?).or_default().results.push(result);
    }
    let mut statement = conn.prepare_cached(TYPE_LINKS)?;
    let mut rows = statement.query([file_id])?;
    while let Some(row) = rows.next()? {
        let link = TypeLink {
            block: codec::u32_at(row, 0)?,
            range: codec::range(row, 1)?,
            text: codec::text(row, 3)?.into(),
            resolution: codec::vocab(row, 4, "resolution")?,
            target: codec::opt_key(row, 5)?,
            reason: codec::opt_vocab(row, 6, "reason")?,
            module: codec::opt_module(row, 7)?,
            heritage: codec::opt_vocab(row, 9, "heritage")?,
        };
        files.entry(row.get(8)?).or_default().type_links.push(link);
    }
    Ok(files)
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

    /// One file's record, blocks (preorder), slots (by block, then start), edges (by source,
    /// then sequence), and typed records (in canonical order), from one snapshot.
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
            let types = read_types(conn, file_id)?.remove(&file_id).unwrap_or_default();
            Ok(Some(FileGraph {
                file: row.record(),
                blocks,
                slots,
                edges,
                types,
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
                    types: FileTypes::default(),
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
            for (id, types) in read_types(conn, -1)? {
                files[owner(id)?].types = types;
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

    /// How many type links resolve to the declaration with `key`: "used in N places".
    pub fn type_use_count(&self, key: &str) -> Result<u64> {
        let mut statement = self.conn().prepare_cached(
            "SELECT count(*) FROM keys k JOIN type_links t ON t.target_key_id = k.id WHERE k.key = ?1",
        )?;
        let counts = collect(&mut statement, [key], |row| codec::count(row, 0))?;
        Ok(counts.into_iter().next().unwrap_or(0))
    }

    /// The type links that resolve to the declaration with `key`, in path, then block, then
    /// range order, skipping `offset` and returning at most `limit`, with each link's block.
    pub fn type_uses(&self, key: &str, offset: usize, limit: usize) -> Result<Vec<TypeUse>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, f.language, t.start_byte, t.end_byte, t.text, t.heritage, {BLOCK_COLUMNS} \
             FROM keys tk \
             JOIN type_links t ON t.target_key_id = tk.id \
             JOIN files f ON f.id = t.file_id \
             JOIN blocks b ON b.file_id = t.file_id AND b.local_id = t.block_local_id \
             JOIN keys k ON k.id = b.key_id \
             WHERE tk.key = ?1 ORDER BY f.path, t.block_local_id, t.seq LIMIT ?2 OFFSET ?3"
        ))?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        collect(&mut statement, (key, limit, offset), |row| {
            Ok(TypeUse {
                path: codec::path(row, 0)?,
                language: codec::vocab(row, 1, "language")?,
                range: codec::range(row, 2)?,
                text: codec::text(row, 4)?.into(),
                heritage: codec::opt_vocab(row, 5, "heritage")?,
                block: decode_block(row, 6)?,
            })
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
