//! Inputs, metadata, statistics for the Index log, and import counts.

use unfold_core::InputRecord;
use unfold_core::vocab::Language;

use super::Reader;
use super::graph::collect;
use super::rows::{ImportSummary, IndexStats, StoredMeta};
use crate::codec;
use crate::error::Result;
use crate::schema::meta_key;
use crate::update::GraphMeta;

impl Reader {
    /// The fingerprinted inputs, in path order.
    pub fn inputs(&self) -> Result<Vec<InputRecord>> {
        let mut statement = self
            .conn()
            .prepare_cached("SELECT path, role, hash FROM inputs ORDER BY path")?;
        collect(&mut statement, [], |row| {
            Ok(InputRecord {
                path: codec::path(row, 0)?,
                role: codec::vocab(row, 1, "input role")?,
                hash: codec::hash(row, 2)?,
            })
        })
    }

    /// The `meta` table.
    pub fn meta(&self) -> Result<StoredMeta> {
        let mut statement = self.conn().prepare_cached("SELECT key, value FROM meta")?;
        let pairs = collect(&mut statement, [], |row| {
            Ok((codec::text(row, 0)?.to_owned(), codec::text(row, 1)?.to_owned()))
        })?;
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        let graph = match (
            get(meta_key::PRODUCERS),
            get(meta_key::SETTINGS_HASH),
            get(meta_key::COMPILED_AT),
            get(meta_key::COMPLETE),
            get(meta_key::LINK_BASIS),
        ) {
            (Some(producers), Some(settings), Some(compiled_at), Some(complete), Some(link_basis)) => {
                Some(GraphMeta {
                    producers: producers.into(),
                    settings_hash: settings
                        .parse()
                        .map_err(|_| codec::invalid("settings hash", settings))?,
                    compiled_at_ms: compiled_at
                        .parse()
                        .map_err(|_| codec::invalid("compile time", compiled_at))?,
                    complete: match complete {
                        "true" => true,
                        "false" => false,
                        other => return Err(codec::invalid("completeness", other)),
                    },
                    link_basis: link_basis.into(),
                })
            }
            _ => None,
        };
        Ok(StoredMeta {
            schema_version: get(meta_key::SCHEMA_VERSION).unwrap_or_default().to_owned(),
            engine_version: get(meta_key::ENGINE_VERSION).unwrap_or_default().to_owned(),
            graph,
        })
    }

    /// The import edges of `language`'s files, in one snapshot.
    pub fn import_summary(&self, language: Language) -> Result<ImportSummary> {
        self.snapshot(|reader| {
            let mut totals = reader.conn().prepare_cached(
                "SELECT count(*), coalesce(sum(e.resolution = 'resolved'), 0) FROM edges e \
                 JOIN files f ON f.id = e.file_id WHERE e.kind = 'imports' AND f.language = ?1",
            )?;
            let counts = collect(&mut totals, [language.as_str()], |row| {
                Ok((codec::count(row, 0)?, codec::count(row, 1)?))
            })?;
            let (total, resolved) = counts.into_iter().next().unwrap_or_default();
            let mut modules = reader.conn().prepare_cached(
                "SELECT e.module, count(*) FROM edges e JOIN files f ON f.id = e.file_id \
                 WHERE e.kind = 'imports' AND e.resolution = 'external' AND f.language = ?1 \
                 AND e.module IS NOT NULL GROUP BY e.module ORDER BY 2 DESC, 1",
            )?;
            let external = collect(&mut modules, [language.as_str()], |row| {
                Ok((codec::text(row, 0)?.into(), codec::count(row, 1)?))
            })?;
            Ok(ImportSummary {
                resolved,
                total,
                external,
            })
        })
    }

    /// Row counts and the database size, from one snapshot. Counting scans the tables, so this
    /// is for the Index log, not for every frame.
    pub fn stats(&self) -> Result<IndexStats> {
        self.snapshot(|reader| {
            let mut statement = reader.conn().prepare_cached(
                "SELECT \
                   (SELECT count(*) FROM files WHERE state = 'current'), \
                   (SELECT count(*) FROM files WHERE state = 'last_good'), \
                   (SELECT count(*) FROM files WHERE state = 'failed'), \
                   (SELECT count(*) FROM inputs), \
                   (SELECT count(*) FROM blocks), \
                   (SELECT count(*) FROM slots), \
                   (SELECT count(*) FROM edges WHERE resolution = 'resolved'), \
                   (SELECT count(*) FROM edges WHERE resolution = 'external'), \
                   (SELECT count(*) FROM edges WHERE resolution = 'unresolved'), \
                   (SELECT count(*) FROM entrances), \
                   (SELECT count(*) FROM definitions), \
                   (SELECT count(*) FROM applications), \
                   (SELECT count(*) FROM keys), \
                   (SELECT count(*) FROM file_facts), \
                   (SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size())",
            )?;
            let stats = collect(&mut statement, [], |row| {
                Ok(IndexStats {
                    files_current: codec::count(row, 0)?,
                    files_last_good: codec::count(row, 1)?,
                    files_failed: codec::count(row, 2)?,
                    inputs: codec::count(row, 3)?,
                    blocks: codec::count(row, 4)?,
                    slots: codec::count(row, 5)?,
                    edges_resolved: codec::count(row, 6)?,
                    edges_external: codec::count(row, 7)?,
                    edges_unresolved: codec::count(row, 8)?,
                    entrances: codec::count(row, 9)?,
                    definitions: codec::count(row, 10)?,
                    applications: codec::count(row, 11)?,
                    keys: codec::count(row, 12)?,
                    facts_entries: codec::count(row, 13)?,
                    database_bytes: codec::count(row, 14)?,
                })
            })?;
            Ok(stats.into_iter().next().unwrap_or_default())
        })
    }
}
