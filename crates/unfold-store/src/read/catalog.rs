//! Apps with their entrances and targets, and the definition catalog.

use rusqlite::Row;
use rustc_hash::FxHashMap;
use unfold_core::{App, Entrance, EntranceTarget, RepoPath};

use super::Reader;
use super::graph::{collect, fetch, page, start};
use super::rows::{Cursor, DefinitionRow, Page};
use crate::codec;
use crate::error::Result;

const DEFINITION_COLUMNS: &str = "k.key, f.path, d.local_id, d.kind, d.qualified_name, d.label";

/// Definitions joined with their key and file.
const DEFINITION_FROM: &str = "definitions d JOIN keys k ON k.id = d.key_id JOIN files f ON f.id = d.file_id";

fn decode_definition(row: &Row<'_>) -> Result<DefinitionRow> {
    Ok(DefinitionRow {
        key: codec::key(row, 0)?,
        path: codec::path(row, 1)?,
        node: codec::u32_at(row, 2)?,
        kind: codec::vocab(row, 3, "node kind")?,
        qualified_name: codec::text(row, 4)?.into(),
        label: codec::text(row, 5)?.into(),
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
    /// Every app in file order, each with its `code` target and its entrances in file order,
    /// and each entrance with its targets in order. Three queries in one snapshot.
    pub fn apps(&self) -> Result<Vec<App>> {
        self.snapshot(|reader| {
            let conn = reader.conn();
            let mut statement = conn.prepare_cached(
                "SELECT t.owner_id, k.key, t.state FROM entrance_targets t \
                 JOIN keys k ON k.id = t.code_key_id ORDER BY t.owner_id, t.position",
            )?;
            let mut targets: FxHashMap<i64, Vec<EntranceTarget>> = FxHashMap::default();
            for (owner, target) in collect(&mut statement, [], |row| {
                let target = EntranceTarget {
                    code: codec::key(row, 1)?,
                    state: codec::vocab(row, 2, "target state")?,
                };
                Ok((row.get::<_, i64>(0)?, target))
            })? {
                targets.entry(owner).or_default().push(target);
            }
            let mut statement = conn.prepare_cached(
                "SELECT a.id, k.key, a.name, a.run, a.about FROM apps a JOIN keys k ON k.id = a.id \
                 ORDER BY a.position",
            )?;
            let mut apps = collect(&mut statement, [], |row| {
                let id: i64 = row.get(0)?;
                let app = App {
                    key: codec::key(row, 1)?,
                    name: codec::text(row, 2)?.into(),
                    run: codec::opt_boxed(row, 3)?,
                    code: targets.get(&id).and_then(|code| code.first().cloned()),
                    about: codec::opt_boxed(row, 4)?,
                    entrances: Vec::new(),
                };
                Ok((id, app))
            })?;
            let index: FxHashMap<i64, usize> = apps
                .iter()
                .enumerate()
                .map(|(position, (id, _))| (*id, position))
                .collect();
            let mut statement = conn.prepare_cached(
                "SELECT e.id, e.app_id, k.key, e.name, e.on_text, e.about FROM entrances e \
                 JOIN keys k ON k.id = e.id ORDER BY e.app_id, e.position",
            )?;
            for (app, entrance) in collect(&mut statement, [], |row| {
                let entrance = Entrance {
                    key: codec::key(row, 2)?,
                    name: codec::text(row, 3)?.into(),
                    on: codec::opt_boxed(row, 4)?,
                    about: codec::opt_boxed(row, 5)?,
                    code: targets.remove(&row.get::<_, i64>(0)?).unwrap_or_default(),
                };
                Ok((row.get::<_, i64>(1)?, entrance))
            })? {
                let position = index
                    .get(&app)
                    .copied()
                    .ok_or_else(|| codec::invalid("entrance app", app.to_string()))?;
                apps[position].1.entrances.push(entrance);
            }
            Ok(apps.into_iter().map(|(_, app)| app).collect())
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

    /// Every definition, one page at a time (pass the previous page's `next` as `after`), in
    /// path order and then source order.
    pub fn definitions(&self, after: Option<&Cursor>, limit: usize) -> Result<Page<DefinitionRow>> {
        let (path, local_id, _) = start(after);
        let mut statement = self.conn().prepare_cached(&format!(
            "SELECT {DEFINITION_COLUMNS} FROM {DEFINITION_FROM} WHERE (f.path, d.local_id) > (?1, ?2) \
             ORDER BY f.path, d.local_id LIMIT ?3"
        ))?;
        let items = collect(&mut statement, (path, local_id, fetch(limit)), |row| {
            let definition = decode_definition(row)?;
            let cursor = Cursor {
                path: definition.path.as_str().into(),
                at: [i64::from(definition.node), 0],
            };
            Ok((cursor, definition))
        })?;
        Ok(page(items, limit))
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
}
