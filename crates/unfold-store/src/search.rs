//! The FTS5 search index behind Add's name search: what each indexed item contributes, how rows
//! are identified, and how a typed query becomes SQL.
//!
//! Every row's `rowid` is `id * 4 + tag`, where `id` is the item's key ID (for a file, its
//! module's key ID) and `tag` its [`CandidateKind`]. That makes deleting or replacing an item's
//! row a rowid lookup (FTS5 has no index on its unindexed `key` column), and tells candidate
//! kinds apart without an extra column.
//!
//! | Kind | `name` | `qualified_name` | `path` | `kind` |
//! | --- | --- | --- | --- | --- |
//! | Definition | Its label | Its qualified name | Its file | Node kind |
//! | App | Its name | Its `run` command | Empty | `app` |
//! | Entrance | Its name | Its app's name and its `on` | Empty | `entrance` |
//! | File | File name | Empty | Path | `module` |
//!
//! Queries never match `kind`, so typing `function` does not match every function's kind. A
//! file matches by its name and path; anything else by its name and qualified name only, so a
//! folder's name finds the folder's files, not every definition in them.

use std::fmt::Write as _;

/// What a search candidate is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CandidateKind {
    /// A definition from the reader's catalog.
    Definition,
    /// An app of `unfold.toml`.
    App,
    /// An entrance of an app.
    Entrance,
    /// An indexed file, found through its module.
    File,
}

impl CandidateKind {
    pub(crate) const fn tag(self) -> i64 {
        match self {
            Self::Definition => 0,
            Self::App => 1,
            Self::Entrance => 2,
            Self::File => 3,
        }
    }

    /// The columns a query matches for this kind.
    pub(crate) const fn columns(self) -> &'static [&'static str] {
        match self {
            Self::File => &["name", "path"],
            Self::Definition | Self::App | Self::Entrance => &["name", "qualified_name"],
        }
    }
}

/// The search row ID of the item of `kind` whose key has ID `key_id`.
pub(crate) const fn rowid(kind: CandidateKind, key_id: i64) -> i64 {
    key_id * 4 + kind.tag()
}

/// How a typed query is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Nothing to search for.
    Empty,
    /// An FTS5 `MATCH` expression: every term is at least three characters, so the trigram
    /// index answers it.
    Match(String),
    /// `LIKE` patterns, all of which must match one of the kind's columns: some term is too
    /// short for trigrams.
    Like(Vec<String>),
}

/// The fewest characters a trigram index can look up.
const TRIGRAM: usize = 3;

/// Plans `query` for candidates of `kind`: whitespace separates terms, and every term must match
/// (as a substring, ignoring ASCII case) one of the kind's [columns](CandidateKind::columns).
pub(crate) fn plan(query: &str, kind: CandidateKind) -> Plan {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Plan::Empty;
    }
    if terms.iter().all(|term| term.chars().count() >= TRIGRAM) {
        let mut expression = format!("{{{}}} : (", kind.columns().join(" "));
        for (index, term) in terms.iter().enumerate() {
            if index > 0 {
                expression.push_str(" AND ");
            }
            // An FTS5 string: double quotes inside are doubled.
            let _ = write!(expression, "\"{}\"", term.replace('"', "\"\""));
        }
        expression.push(')');
        return Plan::Match(expression);
    }
    Plan::Like(terms.iter().map(|term| like_pattern(term)).collect())
}

/// `%term%` with `LIKE`'s wildcards escaped by `\`.
fn like_pattern(term: &str) -> String {
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for character in term.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_match_for_long_terms_and_like_for_short_ones() {
        assert_eq!(plan("  ", CandidateKind::Definition), Plan::Empty);
        assert_eq!(
            plan("order pla\"ce", CandidateKind::Definition),
            Plan::Match(r#"{name qualified_name} : ("order" AND "pla""ce")"#.into())
        );
        assert_eq!(
            plan("orders.ts", CandidateKind::File),
            Plan::Match(r#"{name path} : ("orders.ts")"#.into())
        );
        assert_eq!(
            plan("or 50%_x", CandidateKind::Entrance),
            Plan::Like(vec!["%or%".into(), r"%50\%\_x%".into()])
        );
    }

    #[test]
    fn row_ids_separate_kinds() {
        assert_eq!(rowid(CandidateKind::Definition, 7), 28);
        assert_eq!(rowid(CandidateKind::App, 7), 29);
        assert_eq!(rowid(CandidateKind::Entrance, 7), 30);
        assert_eq!(rowid(CandidateKind::File, 7), 31);
    }
}
