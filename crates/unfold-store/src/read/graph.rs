//! Files, nodes, links, typed records, and the reverse indexes of links: callers and type uses.

use rusqlite::{Connection, Row};
use rustc_hash::FxHashMap;
use unfold_core::vocab::{LinkKind, Outcome};
use unfold_core::{
    Argument, FileGraph, FileTypes, Graph, Link, LinkTarget, Member, Node, Pairing, Parameter, Signature,
    WrittenType,
};

use super::Reader;
use super::rows::{Cursor, FileRow, IncomingLink, LocatedNode, Page};
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

/// Node columns, from `nodes n` joined with its key `k`.
const NODE_COLUMNS: &str = "n.local_id, n.parent, n.form, n.name, n.start_byte, n.end_byte, \
     n.start_line, n.end_line, n.head_start, n.head_end, n.body_start, n.body_end, n.doc_start, \
     n.doc_end, n.stmt_start, n.stmt_end, n.value_start, n.value_end, n.binds, n.role, n.for_node, \
     n.flags, n.target, k.key";

/// Decodes [`NODE_COLUMNS`] starting at column `at`: the node's local ID and the node.
fn decode_node(row: &Row<'_>, at: usize) -> Result<(u32, Node)> {
    let node = Node {
        key: codec::key(row, at + 23)?,
        parent: codec::opt_u32(row, at + 1)?,
        form: codec::vocab(row, at + 2, "node form")?,
        name: codec::opt_boxed(row, at + 3)?,
        range: codec::range(row, at + 4)?,
        lines: codec::lines(row, at + 6)?,
        head: codec::opt_range(row, at + 8)?,
        body: codec::opt_range(row, at + 10)?,
        doc: codec::opt_range(row, at + 12)?,
        stmt: codec::opt_range(row, at + 14)?,
        value: codec::opt_range(row, at + 16)?,
        binds: codec::opt_boxed(row, at + 18)?,
        role: codec::opt_vocab(row, at + 19, "call role")?,
        r#for: codec::opt_u32(row, at + 20)?,
        flags: codec::flags(row, at + 21)?,
        target: codec::opt_u32(row, at + 22)?,
    };
    Ok((codec::u32_at(row, at)?, node))
}

/// A [`LocatedNode`] from the file's path and language followed by [`NODE_COLUMNS`].
fn decode_located(row: &Row<'_>) -> Result<LocatedNode> {
    let (id, node) = decode_node(row, 2)?;
    Ok(LocatedNode {
        path: codec::path(row, 0)?,
        language: codec::vocab(row, 1, "language")?,
        id,
        node,
    })
}

/// Link columns, from `links l` left-joined with its target's key `tk`.
const LINK_COLUMNS: &str = "l.from_id, l.seq, l.kind, l.start_byte, l.end_byte, l.text, l.outcome, \
     tk.key, l.module, l.reason, l.implements, l.bound";

/// Decodes [`LINK_COLUMNS`] starting at column `at`. The target must be in the one column its
/// outcome names.
fn decode_link(row: &Row<'_>, at: usize) -> Result<Link> {
    let outcome: Outcome = codec::vocab(row, at + 6, "outcome")?;
    let key = codec::opt_key(row, at + 7)?;
    let module = codec::opt_module(row, at + 8)?;
    let reason = codec::opt_vocab(row, at + 9, "reason")?;
    let to = match (outcome, key, module, reason) {
        (Outcome::Project, Some(key), None, None) => LinkTarget::Project(key),
        (Outcome::Library, None, Some(module), None) => LinkTarget::Library(module),
        (Outcome::Builtin, None, Some(module), None) => LinkTarget::Builtin(module),
        (Outcome::Unknown, None, None, Some(reason)) => LinkTarget::Unknown(reason),
        (outcome, ..) => return Err(codec::invalid("link target", outcome.as_str())),
    };
    Ok(Link {
        from: codec::u32_at(row, at)?,
        seq: codec::u32_at(row, at + 1)?,
        kind: codec::vocab(row, at + 2, "link kind")?,
        range: codec::range(row, at + 3)?,
        text: codec::text(row, at + 5)?.into(),
        to,
        implements: row.get(at + 10)?,
        bound: codec::opt_u32(row, at + 11)?,
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

/// Which typed records to read.
#[derive(Debug, Clone, Copy)]
enum Scope {
    /// Every file's.
    All,
    /// One file's, by file ID.
    File(i64),
    /// One node's, by file ID and local ID.
    Node(i64, u32),
}

impl Scope {
    /// The `WHERE` clause for a typed-record table whose node column is `node`.
    fn filter(self, node: &str) -> String {
        match self {
            Self::All => String::new(),
            Self::File(_) => "WHERE file_id = ?1".to_owned(),
            Self::Node(..) => format!("WHERE file_id = ?1 AND {node} = ?2"),
        }
    }

    /// The parameters [`Scope::filter`] binds.
    fn params(self) -> Vec<i64> {
        match self {
            Self::All => Vec::new(),
            Self::File(file) => vec![file],
            Self::Node(file, node) => vec![file, i64::from(node)],
        }
    }
}

/// Reads the typed records in `scope` by file ID, each file's records in canonical order. Each
/// query ends with the file ID column.
fn read_types(conn: &Connection, scope: Scope) -> Result<FxHashMap<i64, FileTypes>> {
    let mut files: FxHashMap<i64, FileTypes> = FxHashMap::default();
    let params = || rusqlite::params_from_iter(scope.params());
    let mut statement = conn.prepare_cached(&format!(
        "SELECT node, returns, returns_start, returns_end, file_id FROM signatures {} \
         ORDER BY file_id, node",
        scope.filter("node")
    ))?;
    let mut rows = statement.query(params())?;
    while let Some(row) = rows.next()? {
        let signature = Signature {
            node: codec::u32_at(row, 0)?,
            returns: decode_written(row, 1)?,
        };
        files.entry(row.get(4)?).or_default().signatures.push(signature);
    }
    let mut statement = conn.prepare_cached(&format!(
        "SELECT node, idx, start_byte, end_byte, kind, name, flags, type, type_start, type_end, file_id \
         FROM parameters {} ORDER BY file_id, node, idx",
        scope.filter("node")
    ))?;
    let mut rows = statement.query(params())?;
    while let Some(row) = rows.next()? {
        let parameter = Parameter {
            node: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            range: codec::range(row, 2)?,
            kind: codec::vocab(row, 4, "parameter kind")?,
            name: codec::opt_boxed(row, 5)?,
            optional: flag(row, 6, 0)?,
            default: flag(row, 6, 1)?,
            ty: decode_written(row, 7)?,
        };
        files.entry(row.get(10)?).or_default().parameters.push(parameter);
    }
    let mut statement = conn.prepare_cached(&format!(
        "SELECT node, idx, start_byte, end_byte, kind, name, flags, type, type_start, type_end, value, \
         file_id FROM members {} ORDER BY file_id, node, idx",
        scope.filter("node")
    ))?;
    let mut rows = statement.query(params())?;
    while let Some(row) = rows.next()? {
        let member = Member {
            node: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            range: codec::range(row, 2)?,
            kind: codec::vocab(row, 4, "member kind")?,
            name: codec::opt_boxed(row, 5)?,
            optional: flag(row, 6, 0)?,
            readonly: flag(row, 6, 1)?,
            default: flag(row, 6, 2)?,
            ty: decode_written(row, 7)?,
            value: codec::opt_boxed(row, 10)?,
        };
        files.entry(row.get(11)?).or_default().members.push(member);
    }
    let mut statement = conn.prepare_cached(&format!(
        "SELECT call, idx, start_byte, end_byte, expand, keyword, parameter, unpaired, bound, file_id \
         FROM arguments {} ORDER BY file_id, call, idx",
        scope.filter("call")
    ))?;
    let mut rows = statement.query(params())?;
    while let Some(row) = rows.next()? {
        let pairing = match (
            codec::opt_u32(row, 6)?,
            codec::opt_vocab(row, 7, "pairing reason")?,
        ) {
            (Some(index), None) => Some(Pairing::Parameter(index)),
            (None, Some(reason)) => Some(Pairing::Unpaired(reason)),
            (None, None) => None,
            (Some(_), Some(_)) => return Err(codec::invalid("pairing", "a parameter and a reason")),
        };
        let argument = Argument {
            call: codec::u32_at(row, 0)?,
            index: codec::u32_at(row, 1)?,
            range: codec::range(row, 2)?,
            expand: codec::opt_vocab(row, 4, "expansion")?,
            keyword: codec::opt_boxed(row, 5)?,
            pairing,
            bound: codec::opt_u32(row, 8)?,
        };
        files.entry(row.get(9)?).or_default().arguments.push(argument);
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

/// Fetches one more item than a page holds; ends the page at `limit` items and, when more
/// remain, points the cursor at the last one. Items come with their cursor positions.
pub(crate) fn page<T>(mut items: Vec<(Cursor, T)>, limit: usize) -> Page<T> {
    let next = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|(cursor, _)| cursor.clone())
    } else {
        None
    };
    Page {
        items: items.into_iter().map(|(_, item)| item).collect(),
        next,
    }
}

/// The row limit that fetches one item past a page of `limit`.
pub(crate) fn fetch(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1)
}

/// Where a page starts: after `after`, or before every item.
pub(crate) fn start(after: Option<&Cursor>) -> (&str, i64, i64) {
    after.map_or(("", i64::MIN, i64::MIN), |cursor| {
        (&cursor.path, cursor.at[0], cursor.at[1])
    })
}

/// The link kinds of the callers index.
const CALLER_KINDS: [LinkKind; 2] = [LinkKind::Calls, LinkKind::Passes];
/// The link kinds of the type-uses index.
const TYPE_USE_KINDS: [LinkKind; 2] = [LinkKind::Types, LinkKind::Extends];

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

    /// One file's record, nodes (preorder), links (by `from`, then `seq`), and typed records
    /// (in canonical order), from one snapshot.
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
            let mut nodes = conn.prepare_cached(&format!(
                "SELECT {NODE_COLUMNS} FROM nodes n JOIN keys k ON k.id = n.key_id \
                 WHERE n.file_id = ?1 ORDER BY n.local_id"
            ))?;
            let nodes = collect(&mut nodes, [file_id], |row| Ok(decode_node(row, 0)?.1))?;
            let mut links = conn.prepare_cached(&format!(
                "SELECT {LINK_COLUMNS} FROM links l LEFT JOIN keys tk ON tk.id = l.target_key_id \
                 WHERE l.file_id = ?1 ORDER BY l.from_id, l.seq"
            ))?;
            let links = collect(&mut links, [file_id], |row| decode_link(row, 0))?;
            let types = read_types(conn, Scope::File(file_id))?
                .remove(&file_id)
                .unwrap_or_default();
            Ok(Some(FileGraph {
                file: row.record(),
                nodes,
                links,
                types,
            }))
        })
    }

    /// The whole committed graph from one snapshot: every file with its nodes, links, and typed
    /// records (in the orders [`Reader::file_graph`] returns), the inputs, and the apps. Bulk
    /// queries instead of one round per file: the workspace uses it to restore the engine's
    /// in-memory program when a folder reopens.
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
                    nodes: Vec::new(),
                    links: Vec::new(),
                    types: FileTypes::default(),
                });
            }
            let owner = |id: i64| -> Result<usize> {
                index
                    .get(&id)
                    .copied()
                    .ok_or_else(|| codec::invalid("file id", id.to_string()))
            };
            let mut nodes = conn.prepare_cached(&format!(
                "SELECT n.file_id, {NODE_COLUMNS} FROM nodes n JOIN keys k ON k.id = n.key_id \
                 ORDER BY n.file_id, n.local_id"
            ))?;
            let mut rows = nodes.query([])?;
            while let Some(row) = rows.next()? {
                let (_, node) = decode_node(row, 1)?;
                files[owner(row.get(0)?)?].nodes.push(node);
            }
            let mut links = conn.prepare_cached(&format!(
                "SELECT l.file_id, {LINK_COLUMNS} FROM links l LEFT JOIN keys tk ON tk.id = l.target_key_id \
                 ORDER BY l.file_id, l.from_id, l.seq"
            ))?;
            let mut rows = links.query([])?;
            while let Some(row) = rows.next()? {
                let link = decode_link(row, 1)?;
                files[owner(row.get(0)?)?].links.push(link);
            }
            for (id, types) in read_types(conn, Scope::All)? {
                files[owner(id)?].types = types;
            }
            Ok(Graph {
                files,
                inputs: reader.inputs()?,
                apps: reader.apps()?,
            })
        })
    }

    /// The node with `key`, with its file and local ID.
    pub fn node(&self, key: &str) -> Result<Option<LocatedNode>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, f.language, {NODE_COLUMNS} FROM keys k \
             JOIN nodes n ON n.key_id = k.id JOIN files f ON f.id = n.file_id WHERE k.key = ?1"
        ))?;
        Ok(collect(&mut statement, [key], decode_located)?.into_iter().next())
    }

    /// The direct children of the node with `key`, in preorder. Empty when the node does not
    /// exist or has no children.
    pub fn children(&self, key: &str) -> Result<Vec<LocatedNode>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, f.language, {NODE_COLUMNS} FROM keys pk \
             JOIN nodes p ON p.key_id = pk.id \
             JOIN nodes n ON n.file_id = p.file_id AND n.parent = p.local_id \
             JOIN keys k ON k.id = n.key_id \
             JOIN files f ON f.id = p.file_id \
             WHERE pk.key = ?1 ORDER BY n.local_id"
        ))?;
        collect(&mut statement, [key], decode_located)
    }

    /// The typed records of the node with `key`: a function's signature and parameters, a type's
    /// or class's members, a call's arguments. Empty when the node does not exist or has none.
    pub fn typed_records(&self, key: &str) -> Result<FileTypes> {
        self.snapshot(|reader| {
            let conn = reader.conn();
            let mut statement = conn.prepare_cached(
                "SELECT n.file_id, n.local_id FROM keys k JOIN nodes n ON n.key_id = k.id WHERE k.key = ?1",
            )?;
            let Some((file_id, node)) = collect(&mut statement, [key], |row| {
                Ok((row.get::<_, i64>(0)?, codec::u32_at(row, 1)?))
            })?
            .into_iter()
            .next() else {
                return Ok(FileTypes::default());
            };
            Ok(read_types(conn, Scope::Node(file_id, node))?
                .remove(&file_id)
                .unwrap_or_default())
        })
    }

    /// The `calls` and `passes` links whose project target is `key`, each with the node it
    /// starts at, one page at a time (pass the previous page's `next` as `after`), in path
    /// order and then source order.
    pub fn callers(&self, key: &str, after: Option<&Cursor>, limit: usize) -> Result<Page<IncomingLink>> {
        self.incoming(key, CALLER_KINDS, after, limit)
    }

    /// How many `calls` and `passes` links reach `key`.
    pub fn caller_count(&self, key: &str) -> Result<u64> {
        self.incoming_count(key, CALLER_KINDS)
    }

    /// The `types` and `extends` links whose project target is `key`, paged like
    /// [`Reader::callers`]: where a type or class is named.
    pub fn type_uses(&self, key: &str, after: Option<&Cursor>, limit: usize) -> Result<Page<IncomingLink>> {
        self.incoming(key, TYPE_USE_KINDS, after, limit)
    }

    /// How many `types` and `extends` links reach `key`: "used in N places".
    pub fn type_use_count(&self, key: &str) -> Result<u64> {
        self.incoming_count(key, TYPE_USE_KINDS)
    }

    fn incoming(
        &self,
        key: &str,
        kinds: [LinkKind; 2],
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<Page<IncomingLink>> {
        let (path, from, seq) = start(after);
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, f.language, sk.key, {LINK_COLUMNS} FROM keys tk \
             JOIN links l ON l.target_key_id = tk.id \
             JOIN files f ON f.id = l.file_id \
             JOIN nodes n ON n.file_id = l.file_id AND n.local_id = l.from_id \
             JOIN keys sk ON sk.id = n.key_id \
             WHERE tk.key = ?1 AND l.kind IN (?2, ?3) AND (f.path, l.from_id, l.seq) > (?4, ?5, ?6) \
             ORDER BY f.path, l.from_id, l.seq LIMIT ?7"
        ))?;
        let params = (
            key,
            kinds[0].as_str(),
            kinds[1].as_str(),
            path,
            from,
            seq,
            fetch(limit),
        );
        let items = collect(&mut statement, params, |row| {
            let incoming = IncomingLink {
                path: codec::path(row, 0)?,
                language: codec::vocab(row, 1, "language")?,
                from: codec::key(row, 2)?,
                link: decode_link(row, 3)?,
            };
            let cursor = Cursor {
                path: incoming.path.as_str().into(),
                at: [i64::from(incoming.link.from), i64::from(incoming.link.seq)],
            };
            Ok((cursor, incoming))
        })?;
        Ok(page(items, limit))
    }

    fn incoming_count(&self, key: &str, kinds: [LinkKind; 2]) -> Result<u64> {
        let mut statement = self.conn().prepare_cached(
            "SELECT count(*) FROM keys k JOIN links l ON l.target_key_id = k.id \
             WHERE k.key = ?1 AND l.kind IN (?2, ?3)",
        )?;
        let counts = collect(
            &mut statement,
            (key, kinds[0].as_str(), kinds[1].as_str()),
            |row| codec::count(row, 0),
        )?;
        Ok(counts.into_iter().next().unwrap_or(0))
    }
}
