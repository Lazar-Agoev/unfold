//! The FTS5 search index: what each indexed item contributes, how rows are identified, and how
//! a typed query becomes SQL.
//!
//! Every row's `rowid` is `id * 4 + tag`, where `id` is the item's key ID (for a file, its root
//! key's ID) and `tag` its [`CandidateKind`]. That makes deleting or replacing an item's row a
//! rowid lookup (FTS5 has no index on its unindexed `key` column), and tells candidate kinds
//! apart without an extra column.
//!
//! | Kind | `name` | `qualified_name` | `path` | `kind` |
//! | --- | --- | --- | --- | --- |
//! | Definition | Its label | Its qualified name | Its file | Block kind |
//! | Entrance | Label, else trigger, else first target text | Method, trigger, and target texts | Evidence path | Entrance kind |
//! | File | File name | Empty | Path | `file` |
//!
//! Queries match `name`, `qualified_name`, and `path` only, so typing `function` does not match
//! every function's kind.

use std::fmt::Write as _;

use unfold_core::Entrance;

/// What a search candidate is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CandidateKind {
    /// A definition from the reader's catalog.
    Definition,
    /// An entrance.
    Entrance,
    /// An indexed file.
    File,
}

impl CandidateKind {
    pub(crate) const fn tag(self) -> i64 {
        match self {
            Self::Definition => 0,
            Self::Entrance => 1,
            Self::File => 2,
        }
    }
}

/// The search row ID of the item of `kind` whose key has ID `key_id`.
pub(crate) const fn rowid(kind: CandidateKind, key_id: i64) -> i64 {
    key_id * 4 + kind.tag()
}

/// The `name` and `qualified_name` an entrance contributes.
pub(crate) fn entrance_text(entrance: &Entrance) -> (String, String) {
    let first_target = entrance.targets.first().map(|target| &*target.text);
    let name = entrance
        .label
        .as_deref()
        .or(entrance.trigger.as_deref())
        .or(first_target)
        .unwrap_or_else(|| entrance.kind.as_str());
    let mut qualified = String::new();
    let parts = entrance
        .method
        .map(|method| method.as_str())
        .into_iter()
        .chain(entrance.trigger.as_deref())
        .chain(entrance.targets.iter().map(|target| &*target.text));
    for part in parts {
        if !qualified.is_empty() {
            qualified.push(' ');
        }
        qualified.push_str(part);
    }
    (name.to_owned(), qualified)
}

/// How a typed query is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Nothing to search for.
    Empty,
    /// An FTS5 `MATCH` expression: every term is at least three characters, so the trigram
    /// index answers it.
    Match(String),
    /// `LIKE` patterns, all of which must match one of the searched columns: some term is too
    /// short for trigrams.
    Like(Vec<String>),
}

/// The fewest characters a trigram index can look up.
const TRIGRAM: usize = 3;

/// Plans `query`: whitespace separates terms, and every term must match (as a substring,
/// ignoring ASCII case) one of the searched columns.
pub(crate) fn plan(query: &str) -> Plan {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Plan::Empty;
    }
    if terms.iter().all(|term| term.chars().count() >= TRIGRAM) {
        let mut expression = String::from("{name qualified_name path} : (");
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
        assert_eq!(plan("  "), Plan::Empty);
        assert_eq!(
            plan("order pla\"ce"),
            Plan::Match(r#"{name qualified_name path} : ("order" AND "pla""ce")"#.into())
        );
        assert_eq!(
            plan("or 50%_x"),
            Plan::Like(vec!["%or%".into(), r"%50\%\_x%".into()])
        );
    }

    #[test]
    fn row_ids_separate_kinds() {
        assert_eq!(rowid(CandidateKind::Definition, 7), 28);
        assert_eq!(rowid(CandidateKind::Entrance, 7), 29);
        assert_eq!(rowid(CandidateKind::File, 7), 30);
    }
}
