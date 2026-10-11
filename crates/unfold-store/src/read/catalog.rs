//! Entrances with their targets, definitions, applications, memberships, and launch records.

use rusqlite::{OptionalExtension, Row};
use rustc_hash::FxHashMap;
use unfold_core::{Entrance, EntranceTarget, Evidence, Key, Launches, RepoPath};

use super::Reader;
use super::graph::collect;
use super::rows::{Cursor, DefinitionRow, Page};
use crate::codec;
use crate::error::Result;
use crate::schema::meta_key;
use crate::update::{Application, Membership};

/// Which entrances to load.
enum Which<'a> {
    All,
    EvidencePath(&'a str),
    Key(&'a str),
}

fn decode_entrance(row: &Row<'_>) -> Result<(i64, Entrance)> {
    let entrance = Entrance {
        key: codec::key(row, 1)?,
        kind: codec::vocab(row, 2, "entrance kind")?,
        label: codec::opt_boxed(row, 3)?,
        method: codec::opt_vocab(row, 4, "HTTP method")?,
        trigger: codec::opt_boxed(row, 5)?,
        trigger_status: codec::vocab(row, 6, "trigger status")?,
        detector: codec::vocab(row, 7, "detector")?,
        evidence: Evidence {
            path: codec::path(row, 8)?,
            range: codec::range(row, 9)?,
            block: codec::opt_key(row, 11)?,
            pointer: codec::opt_boxed(row, 12)?,
        },
        binding: codec::opt_key(row, 13)?,
        targets: Vec::new(),
    };
    Ok((row.get(0)?, entrance))
}

fn decode_target(row: &Row<'_>) -> Result<(i64, EntranceTarget)> {
    let target = EntranceTarget {
        position: codec::u32_at(row, 1)?,
        role: codec::vocab(row, 2, "target role")?,
        target: codec::opt_key(row, 3)?,
        text: codec::text(row, 4)?.into(),
        resolution: codec::vocab(row, 5, "resolution")?,
        reason: codec::opt_vocab(row, 6, "reason")?,
        module: codec::opt_module(row, 7)?,
    };
    Ok((row.get(0)?, target))
}

const DEFINITION_COLUMNS: &str = "d.key_id, k.key, f.path, d.local_id, d.kind, d.qualified_name, s.name";

/// Definitions joined with their key, file, and search row (which holds the label).
const DEFINITION_FROM: &str = "definitions d JOIN keys k ON k.id = d.key_id JOIN files f ON f.id = d.file_id \
     LEFT JOIN search s ON s.rowid = d.key_id * 4";

fn decode_definition(row: &Row<'_>) -> Result<DefinitionRow> {
    Ok(DefinitionRow {
        key: codec::key(row, 1)?,
        path: codec::path(row, 2)?,
        local_id: codec::u32_at(row, 3)?,
        kind: codec::vocab(row, 4, "block kind")?,
        qualified_name: codec::text(row, 5)?.into(),
        label: codec::opt_text(row, 6)?.unwrap_or_default().into(),
    })
}

/// `?1, ?2, …, ?count`: numbered, so a statement can repeat the list.
fn placeholders(count: usize) -> String {
    (1..=count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Reader {
    /// Every entrance with its targets, in canonical order.
    pub fn entrances(&self) -> Result<Vec<Entrance>> {
        self.load_entrances(Which::All)
    }

    /// The entrances whose evidence is in the file or input at `path`, in canonical order.
    pub fn entrances_in(&self, path: &str) -> Result<Vec<Entrance>> {
        self.load_entrances(Which::EvidencePath(path))
    }

    /// The entrance with `key`.
    pub fn entrance(&self, key: &str) -> Result<Option<Entrance>> {
        Ok(self.load_entrances(Which::Key(key))?.into_iter().next())
    }

    /// Loads entrances and then all their targets: two queries in one snapshot, never one per
    /// entrance.
    fn load_entrances(&self, which: Which<'_>) -> Result<Vec<Entrance>> {
        let (filter, value) = match which {
            Which::All => ("?1 IS NULL", None),
            Which::EvidencePath(path) => ("e.evidence_path = ?1", Some(path)),
            Which::Key(key) => ("k.key = ?1", Some(key)),
        };
        self.snapshot(|reader| {
            let conn = reader.conn();
            let mut statement = conn.prepare_cached(&format!(
                "SELECT e.id, k.key, e.kind, e.label, e.method, e.trigger, e.trigger_status, e.detector, \
                 e.evidence_path, e.evidence_start_byte, e.evidence_end_byte, bk.key, e.evidence_pointer, \
                 ak.key FROM entrances e JOIN keys k ON k.id = e.key_id \
                 LEFT JOIN keys bk ON bk.id = e.evidence_block_key_id \
                 LEFT JOIN keys ak ON ak.id = e.binding_key_id WHERE {filter}"
            ))?;
            let mut entrances = collect(&mut statement, [value], decode_entrance)?;
            let index: FxHashMap<i64, usize> = entrances
                .iter()
                .enumerate()
                .map(|(position, (id, _))| (*id, position))
                .collect();
            let mut statement = conn.prepare_cached(&format!(
                "SELECT t.entrance_id, t.position, t.role, tk.key, t.target_text, t.resolution, t.reason, \
                 t.module FROM entrance_targets t LEFT JOIN keys tk ON tk.id = t.target_key_id \
                 WHERE t.entrance_id IN (SELECT e.id FROM entrances e JOIN keys k ON k.id = e.key_id \
                 WHERE {filter}) ORDER BY t.entrance_id, t.position"
            ))?;
            for (id, target) in collect(&mut statement, [value], decode_target)? {
                if let Some(&position) = index.get(&id) {
                    entrances[position].1.targets.push(target);
                }
            }
            let mut entrances: Vec<Entrance> = entrances.into_iter().map(|(_, entrance)| entrance).collect();
            // Canonical order; the key (which carries the duplicate ordinal) breaks full ties.
            entrances.sort_by(|a, b| a.canonical_cmp(b).then_with(|| a.key.cmp(&b.key)));
            Ok(entrances)
        })
    }

    /// The definitions in the file at `path`, in source (preorder) order.
    pub fn definitions_in(&self, path: &str) -> Result<Vec<DefinitionRow>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT {DEFINITION_COLUMNS} FROM {DEFINITION_FROM} WHERE f.path = ?1 ORDER BY d.local_id"
        ))?;
        collect(&mut statement, [path], decode_definition)
    }

    /// The definition with `key`.
    pub fn definition(&self, key: &str) -> Result<Option<DefinitionRow>> {
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT {DEFINITION_COLUMNS} FROM {DEFINITION_FROM} WHERE k.key = ?1"
        ))?;
        Ok(collect(&mut statement, [key], decode_definition)?
            .into_iter()
            .next())
    }

    /// Every definition, one page at a time, in a stable order.
    pub fn definitions(&self, after: Option<Cursor>, limit: usize) -> Result<Page<DefinitionRow>> {
        let after = after.map_or(i64::MIN, |cursor| cursor.0[0]);
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT {DEFINITION_COLUMNS} FROM {DEFINITION_FROM} WHERE d.key_id > ?1 ORDER BY d.key_id LIMIT ?2"
        ))?;
        let fetch = i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1);
        let mut items = collect(&mut statement, (after, fetch), |row| {
            Ok((row.get::<_, i64>(0)?, decode_definition(row)?))
        })?;
        let next = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|(id, _)| Cursor([*id, 0, 0]))
        } else {
            None
        };
        Ok(Page {
            items: items.into_iter().map(|(_, definition)| definition).collect(),
            next,
        })
    }

    /// How many definitions each file of `paths` holds, in one grouped query. A path that isn't
    /// an indexed file is left out; an indexed file without definitions counts 0.
    pub fn definition_counts(&self, paths: &[&str]) -> Result<Vec<(RepoPath, u32)>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT f.path, count(d.key_id) FROM files f LEFT JOIN definitions d ON d.file_id = f.id \
             WHERE f.path IN ({}) GROUP BY f.id ORDER BY f.path",
            placeholders(paths.len())
        ))?;
        collect(&mut statement, rusqlite::params_from_iter(paths), |row| {
            Ok((codec::path(row, 0)?, codec::u32_at(row, 1)?))
        })
    }

    /// The name of the first application (by name) each entrance of `keys` belongs to. One
    /// query; an entrance without an application is left out. Sorted by key.
    pub fn entrance_applications(&self, keys: &[&str]) -> Result<Vec<(Key, Box<str>)>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT ek.key, a.name FROM memberships m JOIN keys ek ON ek.id = m.entrance_id \
             JOIN applications a ON a.id = m.application WHERE ek.key IN ({}) ORDER BY 1, 2",
            placeholders(keys.len())
        ))?;
        let mut rows = collect(&mut statement, rusqlite::params_from_iter(keys), |row| {
            Ok((codec::key(row, 0)?, codec::text(row, 1)?.into()))
        })?;
        rows.dedup_by(|next, first| next.0 == first.0);
        Ok(rows)
    }

    /// Every application, ordered by ID.
    pub fn applications(&self) -> Result<Vec<Application>> {
        let mut statement = self.conn().prepare_cached(
            "SELECT id, name, description, declared, category FROM applications ORDER BY id",
        )?;
        collect(&mut statement, [], |row| {
            Ok(Application {
                id: codec::text(row, 0)?.into(),
                name: codec::text(row, 1)?.into(),
                description: codec::opt_boxed(row, 2)?,
                declared: row.get(3)?,
                category: codec::vocab(row, 4, "category")?,
            })
        })
    }

    /// Every membership, ordered by entrance key, then application.
    pub fn memberships(&self) -> Result<Vec<Membership>> {
        let mut statement = self.conn().prepare_cached(
            "SELECT ek.key, m.application, m.basis FROM memberships m \
             JOIN keys ek ON ek.id = m.entrance_id ORDER BY ek.key, m.application",
        )?;
        collect(&mut statement, [], |row| {
            Ok(Membership {
                entrance: codec::key(row, 0)?,
                application: codec::text(row, 1)?.into(),
                basis: codec::vocab(row, 2, "membership basis")?,
            })
        })
    }

    /// The launch records (empty before the first commit).
    pub fn launches(&self) -> Result<Launches> {
        let value: Option<String> = self
            .conn()
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?
            .query_row([meta_key::LAUNCHES], |row| row.get(0))
            .optional()?;
        match value {
            Some(text) => serde_json::from_str(&text).map_err(|_| codec::invalid("launch records", text)),
            None => Ok(Launches::default()),
        }
    }
}
