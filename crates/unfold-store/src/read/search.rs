//! Search candidates from the FTS5 index (architecture §8.5).

use std::fmt::Write as _;

use rusqlite::types::Value;

use super::Reader;
use super::graph::collect;
use super::rows::SearchCandidate;
use crate::codec;
use crate::error::Result;
use crate::search::{CandidateKind, Plan, plan};

/// Column weights for `bm25`: a match in the name counts most, then the qualified name, then
/// the path. `kind` and `key` never match.
const WEIGHTS: &str = "8.0, 4.0, 1.0, 0.0, 0.0";

impl Reader {
    /// Up to `limit` candidates of `kind` whose name, qualified name, or path contains every
    /// whitespace-separated term of `query`, ignoring ASCII case. The order is a rough
    /// relevance order (BM25, or shortest name first for very short terms) that the caller
    /// re-ranks; it is deterministic for a given database.
    pub fn search(&self, query: &str, kind: CandidateKind, limit: usize) -> Result<Vec<SearchCandidate>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let decode = |row: &rusqlite::Row<'_>| {
            Ok(SearchCandidate {
                kind,
                key: codec::key(row, 0)?,
                name: codec::text(row, 1)?.into(),
                qualified_name: codec::text(row, 2)?.into(),
                path: codec::text(row, 3)?.into(),
                item_kind: codec::text(row, 4)?.into(),
            })
        };
        match plan(query) {
            Plan::Empty => Ok(Vec::new()),
            Plan::Match(expression) => {
                let mut statement = self.conn().prepare_cached(&format!(
                    "SELECT key, name, qualified_name, path, kind FROM search \
                     WHERE search MATCH ?1 AND (rowid & 3) = ?2 \
                     ORDER BY bm25(search, {WEIGHTS}), rowid LIMIT ?3"
                ))?;
                collect(&mut statement, (expression, kind.tag(), limit), decode)
            }
            Plan::Like(patterns) => {
                let mut sql = String::from(
                    "SELECT key, name, qualified_name, path, kind FROM search WHERE (rowid & 3) = ?1",
                );
                for index in 0..patterns.len() {
                    let parameter = index + 2;
                    let _ = write!(
                        sql,
                        " AND (name LIKE ?{parameter} ESCAPE '\\' OR qualified_name LIKE ?{parameter} \
                         ESCAPE '\\' OR path LIKE ?{parameter} ESCAPE '\\')"
                    );
                }
                let _ = write!(sql, " ORDER BY length(name), rowid LIMIT ?{}", patterns.len() + 2);
                let mut values = Vec::with_capacity(patterns.len() + 2);
                values.push(Value::Integer(kind.tag()));
                values.extend(patterns.into_iter().map(Value::Text));
                values.push(Value::Integer(limit));
                let mut statement = self.conn().prepare_cached(&sql)?;
                collect(&mut statement, rusqlite::params_from_iter(values), decode)
            }
        }
    }
}
