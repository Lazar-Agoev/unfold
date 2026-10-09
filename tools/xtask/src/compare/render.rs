//! Human and machine-readable output for differences.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{ABSENT, Category, Change, Difference, FieldChange, RecordType};

/// Difference counts per record type and category.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    counts: BTreeMap<(RecordType, Category), usize>,
}

impl Summary {
    /// Counts `items`.
    pub(crate) fn of(items: impl IntoIterator<Item = (RecordType, Category)>) -> Self {
        let mut counts = BTreeMap::new();
        for item in items {
            *counts.entry(item).or_insert(0) += 1;
        }
        Self { counts }
    }

    /// Counts the differences themselves.
    pub(crate) fn of_differences(differences: &[Difference]) -> Self {
        Self::of(differences.iter().map(|d| (d.id.record, d.change.category())))
    }

    /// The number of items counted.
    pub(crate) fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// `{"block":{"missing":0,"extra":2,"changed":1},...,"total":3}`, with every record type and
    /// category present so the shape never changes.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let mut object = serde_json::Map::new();
        for record in RecordType::ALL {
            let mut categories = serde_json::Map::new();
            for category in Category::ALL {
                let count = self.counts.get(&(record, category)).copied().unwrap_or(0);
                categories.insert(category.as_str().to_owned(), count.into());
            }
            object.insert(record.as_str().to_owned(), categories.into());
        }
        object.insert("total".to_owned(), self.total().into());
        object.into()
    }

    /// `block missing 2, edge changed 1`, or `none`.
    pub(crate) fn line(&self) -> String {
        if self.counts.is_empty() {
            return "none".to_owned();
        }
        let parts: Vec<String> = self
            .counts
            .iter()
            .map(|((record, category), count)| format!("{record} {} {count}", category.as_str()))
            .collect();
        parts.join(", ")
    }
}

/// Lists differences grouped by record type and category, at most `limit` per group (0 for no
/// limit).
pub(crate) fn render_differences(differences: &[&Difference], limit: usize) -> String {
    let mut groups: BTreeMap<(RecordType, Category), Vec<&Difference>> = BTreeMap::new();
    for &difference in differences {
        groups
            .entry((difference.id.record, difference.change.category()))
            .or_default()
            .push(difference);
    }
    let mut out = String::new();
    for ((record, category), members) in groups {
        // Writing to a String cannot fail.
        let _ = writeln!(out, "{record} {} ({}):", category.as_str(), members.len());
        let shown = if limit == 0 {
            members.len()
        } else {
            limit.min(members.len())
        };
        for difference in &members[..shown] {
            let _ = write!(out, "  {}", difference.id);
            if let Change::Changed(fields) = &difference.change {
                let _ = write!(out, ": {}", render_fields(fields));
            }
            out.push('\n');
        }
        if shown < members.len() {
            let _ = writeln!(out, "  ... and {} more", members.len() - shown);
        }
    }
    out
}

/// `resolution: unresolved -> resolved; reason: unknown_receiver -> (absent)`.
pub(crate) fn render_fields(fields: &[FieldChange]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|change| {
            format!(
                "{}: {} -> {}",
                change.field,
                change.expected.as_deref().unwrap_or(ABSENT),
                change.actual.as_deref().unwrap_or(ABSENT)
            )
        })
        .collect();
    parts.join("; ")
}
