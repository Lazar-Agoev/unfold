//! Search candidates from the FTS5 index (crates/unfold-store/src/read/search.rs).

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
    /// whitespace-separated term of `query`, ignoring ASCII case; with `item_kinds`, only those
    /// whose item kind (a definition's block kind, an entrance's kind) is one of them. The order
    /// is a rough relevance order (BM25, or shortest name first for very short terms) that the
    /// caller re-ranks; it is deterministic for a given database.
    pub fn search(
        &self,
        query: &str,
        kind: CandidateKind,
        item_kinds: &[&str],
        limit: usize,
    ) -> Result<Vec<SearchCandidate>> {
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
        let mut sql = String::from("SELECT key, name, qualified_name, path, kind FROM search WHERE ");
        let mut values = Vec::new();
        let order = match plan(query) {
            Plan::Empty => return Ok(Vec::new()),
            Plan::Match(expression) => {
                let at = bind(&mut values, Value::Text(expression));
                let _ = write!(sql, "search MATCH ?{at}");
                format!("bm25(search, {WEIGHTS}), rowid")
            }
            Plan::Like(patterns) => {
                for (index, pattern) in patterns.into_iter().enumerate() {
                    let at = bind(&mut values, Value::Text(pattern));
                    let _ = write!(
                        sql,
                        "{}(name LIKE ?{at} ESCAPE '\\' OR qualified_name LIKE ?{at} ESCAPE '\\' \
                         OR path LIKE ?{at} ESCAPE '\\')",
                        if index > 0 { " AND " } else { "" }
                    );
                }
                "length(name), rowid".to_owned()
            }
        };
        let at = bind(&mut values, Value::Integer(kind.tag()));
        let _ = write!(sql, " AND (rowid & 3) = ?{at}");
        if !item_kinds.is_empty() {
            sql.push_str(" AND kind IN (");
            for (index, item_kind) in item_kinds.iter().enumerate() {
                let at = bind(&mut values, Value::Text((*item_kind).to_owned()));
                let _ = write!(sql, "{}?{at}", if index > 0 { ", " } else { "" });
            }
            sql.push(')');
        }
        let at = bind(
            &mut values,
            Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)),
        );
        let _ = write!(sql, " ORDER BY {order} LIMIT ?{at}");
        let mut statement = self.conn().prepare_cached(&sql)?;
        collect(&mut statement, rusqlite::params_from_iter(values), decode)
    }
}

/// Appends `value` to `values` and returns its parameter number.
fn bind(values: &mut Vec<Value>, value: Value) -> usize {
    values.push(value);
    values.len()
}
