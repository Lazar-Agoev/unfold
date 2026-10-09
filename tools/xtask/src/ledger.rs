//! The divergence ledger, `testdata/reference/divergences.toml`: every allowed difference between
//! the PoC reference and Unfold's output, with its reason
//! ([U-33](../../../docs/decisions-and-tradeoffs.md)).
//!
//! The ledger is checked against the differences of one corpus at a time. A difference is split
//! into units: one for a missing or extra record, one per changed field. Every unit must be
//! covered by an entry (otherwise it is *unlisted*), and every entry must cover at least one
//! unit (otherwise it is *stale*: the divergence no longer occurs). Entries are explicit: one
//! record, optionally one field; there are no pattern selectors.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail, ensure};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use unfold_core::vocab::SlotName;

use crate::compare::{ABSENT, Category, Change, Difference, FieldChange, RecordId, RecordType, Scope};
use crate::repo;

/// One allowed difference.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    /// The corpus it applies to.
    pub corpus: String,
    /// The record type.
    pub record: RecordType,
    /// The record's path (file, input), key (block, entrance), owning block key (slot), source
    /// key (edge), or entrance key (target).
    pub key: String,
    /// The slot name; slots only.
    pub slot: Option<SlotName>,
    /// The slot index; repeated slots only.
    pub index: Option<u32>,
    /// The edge sequence number; edges only.
    pub seq: Option<u32>,
    /// The target position; targets only.
    pub position: Option<u32>,
    /// `missing`, `extra`, or `changed`.
    pub change: Category,
    /// The one changed field this entry covers; without it, a `changed` entry covers every
    /// changed field of the record.
    pub field: Option<String>,
    /// The expected value as the diff prints it (`(absent)` for no value); requires `field`.
    pub expected: Option<String>,
    /// The actual value as the diff prints it; requires `field`.
    pub actual: Option<String>,
    /// Why the difference is allowed, such as "C-2 fixed: logger.info now resolves".
    pub reason: String,
}

impl Entry {
    /// The identity of the record this entry selects.
    pub(crate) fn record_id(&self) -> RecordId {
        RecordId {
            record: self.record,
            key: self.key.as_str().into(),
            slot: self.slot,
            index: self.index,
            seq: self.seq,
            position: self.position,
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        ensure!(!self.key.is_empty(), "`key` is empty");
        ensure!(!self.reason.trim().is_empty(), "`reason` is empty");
        let is = |record| self.record == record;
        ensure!(
            self.slot.is_some() == is(RecordType::Slot),
            "`slot` is required for slots and only for them"
        );
        ensure!(
            self.index.is_none() || is(RecordType::Slot),
            "`index` belongs to slots"
        );
        ensure!(
            self.seq.is_some() == is(RecordType::Edge),
            "`seq` is required for edges and only for them"
        );
        ensure!(
            self.position.is_some() == is(RecordType::Target),
            "`position` is required for targets and only for them"
        );
        match &self.field {
            Some(field) => {
                ensure!(
                    self.change == Category::Changed,
                    "`field` requires change = \"changed\""
                );
                ensure!(
                    self.record.fields().contains(&field.as_str()),
                    "`{field}` is not a field of a {} (fields: {})",
                    self.record,
                    self.record.fields().join(", ")
                );
            }
            None => ensure!(
                self.expected.is_none() && self.actual.is_none(),
                "`expected` and `actual` require `field`"
            ),
        }
        Ok(())
    }

    /// Whether this entry covers one unit of a difference.
    fn covers(&self, change: &Change, field: Option<&FieldChange>) -> bool {
        if self.change != change.category() {
            return false;
        }
        let (Some(wanted), Some(field)) = (self.field.as_deref(), field) else {
            return true;
        };
        fn value(value: &Option<String>) -> &str {
            value.as_deref().unwrap_or(ABSENT)
        }
        wanted == field.field
            && self
                .expected
                .as_deref()
                .is_none_or(|expected| expected == value(&field.expected))
            && self
                .actual
                .as_deref()
                .is_none_or(|actual| actual == value(&field.actual))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerFile {
    #[serde(default)]
    divergence: Vec<Entry>,
}

/// The parsed, checked ledger.
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    entries: Vec<Entry>,
}

/// One unit of a difference: a missing or extra record, or one changed field.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Unit<'d> {
    /// The difference it belongs to.
    pub difference: &'d Difference,
    /// The changed field, for a `changed` difference.
    pub field: Option<&'d FieldChange>,
}

/// The result of checking one corpus's differences against the ledger.
#[derive(Debug)]
pub(crate) struct Outcome<'l, 'd> {
    /// Units covered by an entry.
    pub listed: Vec<Unit<'d>>,
    /// Units no entry covers.
    pub unlisted: Vec<Unit<'d>>,
    /// Entries (with their one-based number in the file) that cover nothing.
    pub stale: Vec<(usize, &'l Entry)>,
}

impl Outcome<'_, '_> {
    /// Whether the check passes.
    pub(crate) fn passed(&self) -> bool {
        self.unlisted.is_empty() && self.stale.is_empty()
    }
}

impl Ledger {
    /// The default location, `testdata/reference/divergences.toml`.
    pub(crate) fn default_path() -> PathBuf {
        repo::reference_dir().join("divergences.toml")
    }

    /// Reads and checks the ledger at `path`; every entry must name one of `corpora`.
    pub(crate) fn load(path: &Path, corpora: &BTreeSet<&str>) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text, corpora).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parses and checks ledger text.
    pub(crate) fn parse(text: &str, corpora: &BTreeSet<&str>) -> anyhow::Result<Self> {
        let file: LedgerFile = toml::from_str(text)?;
        let mut seen = BTreeSet::new();
        for (number, entry) in (1..).zip(&file.divergence) {
            entry.check().with_context(|| format!("divergence {number}"))?;
            if !corpora.contains(entry.corpus.as_str()) {
                bail!("divergence {number}: unknown corpus `{}`", entry.corpus);
            }
            let selector = (
                entry.corpus.as_str(),
                entry.record_id().to_string(),
                entry.change,
                entry.field.as_deref(),
            );
            if !seen.insert(selector) {
                bail!("divergence {number} repeats an earlier entry's selector");
            }
        }
        Ok(Self {
            entries: file.divergence,
        })
    }

    /// Checks `differences` of `corpus`, computed over `scopes`, against the ledger. Entries for
    /// record types outside `scopes` are ignored.
    pub(crate) fn check<'l, 'd>(
        &'l self,
        corpus: &str,
        scopes: &[Scope],
        differences: &'d [Difference],
    ) -> Outcome<'l, 'd> {
        let mut by_record: FxHashMap<RecordId, Vec<usize>> = FxHashMap::default();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.corpus == corpus && scopes.contains(&entry.record.scope()) {
                by_record.entry(entry.record_id()).or_default().push(index);
            }
        }
        let mut used = vec![false; self.entries.len()];
        let mut outcome = Outcome {
            listed: Vec::new(),
            unlisted: Vec::new(),
            stale: Vec::new(),
        };
        for difference in differences {
            let candidates = by_record.get(&difference.id).map_or(&[][..], Vec::as_slice);
            let fields: Vec<Option<&FieldChange>> = match &difference.change {
                Change::Changed(fields) => fields.iter().map(Some).collect(),
                Change::Missing | Change::Extra => vec![None],
            };
            for field in fields {
                let mut covered = false;
                for &index in candidates {
                    if self.entries[index].covers(&difference.change, field) {
                        used[index] = true;
                        covered = true;
                    }
                }
                let unit = Unit { difference, field };
                if covered {
                    outcome.listed.push(unit);
                } else {
                    outcome.unlisted.push(unit);
                }
            }
        }
        for indexes in by_record.values() {
            for &index in indexes {
                if !used[index] {
                    outcome.stale.push((index + 1, &self.entries[index]));
                }
            }
        }
        outcome.stale.sort_by_key(|(number, _)| *number);
        outcome
    }
}

/// One line describing a unit: `edge src/a.ts#call[0] seq 0: resolution: unresolved -> resolved`.
pub(crate) fn describe_unit(unit: &Unit<'_>) -> String {
    match unit.field {
        Some(field) => format!(
            "{}: {}: {} -> {}",
            unit.difference.id,
            field.field,
            field.expected.as_deref().unwrap_or(ABSENT),
            field.actual.as_deref().unwrap_or(ABSENT)
        ),
        None => format!(
            "{}: {}",
            unit.difference.id,
            unit.difference.change.category().as_str()
        ),
    }
}

/// One line describing an entry: `divergence 3 (edge src/a.ts#call[0] seq 0 changed resolution)`.
pub(crate) fn describe_entry(number: usize, entry: &Entry) -> String {
    let field = entry
        .field
        .as_deref()
        .map_or(String::new(), |field| format!(" {field}"));
    format!(
        "divergence {number} ({} {}{field}): {}",
        entry.record_id(),
        entry.change.as_str(),
        entry.reason
    )
}

#[cfg(test)]
mod tests;
