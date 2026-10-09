//! Record-by-record comparison of two canonical graphs.
//!
//! Records are matched by identity, never by position in the file:
//!
//! | Record | Identity |
//! | --- | --- |
//! | `file`, `input` | path |
//! | `block` | structural key |
//! | `slot` | block key, slot name, index |
//! | `edge` | source key, `seq` |
//! | `entrance` | entrance key |
//! | `target` | entrance key, position |
//!
//! Each side is indexed once in a hash map, so comparing is linear in the number of records. A
//! record is `missing` (only expected), `extra` (only actual), or `changed` (both, with the
//! fields that differ). Differences come out in a deterministic order: by record type, then in
//! the expected export's order, then extras in the actual export's order.

mod render;

use std::fmt;
use std::hash::Hash;

use anyhow::bail;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use unfold_core::vocab::SlotName;
use unfold_core::{
    ByteRange, ContentHash, Edge, EntranceTarget, Graph, Key, LineSpan, ModuleLabel, RepoPath, Slot,
};

pub(crate) use render::{Summary, render_differences};

/// The text shown for an absent field value, in output and in the divergence ledger.
pub(crate) const ABSENT: &str = "(absent)";

/// A canonical record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RecordType {
    File,
    Block,
    Slot,
    Edge,
    Input,
    Entrance,
    Target,
}

impl RecordType {
    /// Every record type, in canonical section order.
    pub(crate) const ALL: [Self; 7] = [
        Self::File,
        Self::Block,
        Self::Slot,
        Self::Edge,
        Self::Input,
        Self::Entrance,
        Self::Target,
    ];

    /// The record's `t` value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Block => "block",
            Self::Slot => "slot",
            Self::Edge => "edge",
            Self::Input => "input",
            Self::Entrance => "entrance",
            Self::Target => "target",
        }
    }

    /// The fields compared for this record type (its identity fields excluded).
    pub(crate) const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::File => &["language", "state", "hash", "failure"],
            Self::Block => &["kind", "role", "name", "range", "lines"],
            Self::Slot => &["range"],
            Self::Edge => &[
                "kind",
                "target",
                "text",
                "resolution",
                "reason",
                "module",
                "position",
            ],
            Self::Input => &["role", "hash"],
            Self::Entrance => &[
                "kind",
                "label",
                "method",
                "trigger",
                "trigger_status",
                "detector",
                "evidence_path",
                "evidence_range",
                "evidence_block",
                "evidence_pointer",
            ],
            Self::Target => &["role", "target", "text", "resolution", "reason", "module"],
        }
    }

    /// The scope that covers this record type.
    pub(crate) const fn scope(self) -> Scope {
        match self {
            Self::File => Scope::Files,
            Self::Block => Scope::Blocks,
            Self::Slot => Scope::Slots,
            Self::Edge => Scope::Edges,
            Self::Input => Scope::Inputs,
            Self::Entrance | Self::Target => Scope::Entrances,
        }
    }
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which records a comparison covers. `entrances` includes their targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Scope {
    Files,
    Inputs,
    Blocks,
    Slots,
    Edges,
    Entrances,
}

impl Scope {
    /// Every scope.
    pub(crate) const ALL: [Self; 6] = [
        Self::Files,
        Self::Inputs,
        Self::Blocks,
        Self::Slots,
        Self::Edges,
        Self::Entrances,
    ];
}

/// A record's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RecordId {
    /// The record type.
    pub record: RecordType,
    /// The path (files, inputs), block key (blocks, slots), source key (edges), or entrance key
    /// (entrances, targets).
    pub key: Box<str>,
    /// The slot name (slots only).
    pub slot: Option<SlotName>,
    /// The slot index (slots only, when repeated).
    pub index: Option<u32>,
    /// The edge sequence number (edges only).
    pub seq: Option<u32>,
    /// The target position (targets only).
    pub position: Option<u32>,
}

impl RecordId {
    fn new(record: RecordType, key: &str) -> Self {
        Self {
            record,
            key: key.into(),
            slot: None,
            index: None,
            seq: None,
            position: None,
        }
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.record, self.key)?;
        if let Some(slot) = self.slot {
            write!(f, " {slot}")?;
        }
        if let Some(index) = self.index {
            write!(f, "[{index}]")?;
        }
        if let Some(seq) = self.seq {
            write!(f, " seq {seq}")?;
        }
        if let Some(position) = self.position {
            write!(f, " position {position}")?;
        }
        Ok(())
    }
}

/// One field that differs between the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldChange {
    /// The field's canonical name.
    pub field: &'static str,
    /// The expected value as text, `None` when absent.
    pub expected: Option<String>,
    /// The actual value as text, `None` when absent.
    pub actual: Option<String>,
}

/// How a record differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// Only in the expected graph.
    Missing,
    /// Only in the actual graph.
    Extra,
    /// In both, with these fields differing (in the record type's field order).
    Changed(Vec<FieldChange>),
}

/// The three difference categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Category {
    Missing,
    Extra,
    Changed,
}

impl Category {
    /// Every category.
    pub(crate) const ALL: [Self; 3] = [Self::Missing, Self::Extra, Self::Changed];

    /// The category's name.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Extra => "extra",
            Self::Changed => "changed",
        }
    }
}

impl Change {
    /// The change's category.
    pub(crate) const fn category(&self) -> Category {
        match self {
            Self::Missing => Category::Missing,
            Self::Extra => Category::Extra,
            Self::Changed(_) => Category::Changed,
        }
    }
}

/// One differing record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Difference {
    /// Which record.
    pub id: RecordId,
    /// How it differs.
    pub change: Change,
}

/// Compares `actual` with `expected` over `scopes`. Fails if either graph repeats a record
/// identity, which a valid graph never does.
pub(crate) fn diff(expected: &Graph, actual: &Graph, scopes: &[Scope]) -> anyhow::Result<Vec<Difference>> {
    let mut out = Vec::new();
    let has = |scope| scopes.contains(&scope);
    if has(Scope::Files) {
        compare(
            files(expected),
            files(actual),
            |path| RecordId::new(RecordType::File, path),
            |e, a, c| {
                c.field("language", &e.language, &a.language);
                c.field("state", &e.state, &a.state);
                c.field("hash", &e.hash, &a.hash);
                c.field("failure", &e.failure, &a.failure);
            },
            &mut out,
        )?;
    }
    if has(Scope::Blocks) {
        compare(
            blocks(expected),
            blocks(actual),
            |key| RecordId::new(RecordType::Block, key),
            |e, a, c| {
                c.field("kind", &e.kind, &a.kind);
                c.field("role", &e.role, &a.role);
                c.field("name", &e.name, &a.name);
                c.field("range", &e.range, &a.range);
                c.field("lines", &e.lines, &a.lines);
            },
            &mut out,
        )?;
    }
    if has(Scope::Slots) {
        compare(
            slots(expected),
            slots(actual),
            |&(key, slot, index)| RecordId {
                slot: Some(slot),
                index,
                ..RecordId::new(RecordType::Slot, key)
            },
            |e: &&Slot, a: &&Slot, c| c.field("range", &e.range, &a.range),
            &mut out,
        )?;
    }
    if has(Scope::Edges) {
        compare(
            edges(expected),
            edges(actual),
            |&(key, seq)| RecordId {
                seq: Some(seq),
                ..RecordId::new(RecordType::Edge, key)
            },
            |e: &&Edge, a: &&Edge, c| {
                c.field("kind", &e.kind, &a.kind);
                c.field("target", &e.target, &a.target);
                c.field("text", &e.text, &a.text);
                c.field("resolution", &e.resolution, &a.resolution);
                c.field("reason", &e.reason, &a.reason);
                c.field("module", &e.module, &a.module);
                c.field("position", &e.position, &a.position);
            },
            &mut out,
        )?;
    }
    if has(Scope::Inputs) {
        compare(
            expected.inputs.iter().map(|input| (input.path.as_str(), input)),
            actual.inputs.iter().map(|input| (input.path.as_str(), input)),
            |path| RecordId::new(RecordType::Input, path),
            |e, a, c| {
                c.field("role", &e.role, &a.role);
                c.field("hash", &e.hash, &a.hash);
            },
            &mut out,
        )?;
    }
    if has(Scope::Entrances) {
        compare(
            expected
                .entrances
                .iter()
                .map(|entrance| (entrance.key.as_str(), entrance)),
            actual
                .entrances
                .iter()
                .map(|entrance| (entrance.key.as_str(), entrance)),
            |key| RecordId::new(RecordType::Entrance, key),
            |e, a, c| {
                c.field("kind", &e.kind, &a.kind);
                c.field("label", &e.label, &a.label);
                c.field("method", &e.method, &a.method);
                c.field("trigger", &e.trigger, &a.trigger);
                c.field("trigger_status", &e.trigger_status, &a.trigger_status);
                c.field("detector", &e.detector, &a.detector);
                c.field("evidence_path", &e.evidence.path, &a.evidence.path);
                c.field("evidence_range", &e.evidence.range, &a.evidence.range);
                c.field("evidence_block", &e.evidence.block, &a.evidence.block);
                c.field("evidence_pointer", &e.evidence.pointer, &a.evidence.pointer);
            },
            &mut out,
        )?;
        compare(
            targets(expected),
            targets(actual),
            |&(key, position)| RecordId {
                position: Some(position),
                ..RecordId::new(RecordType::Target, key)
            },
            |e: &&EntranceTarget, a: &&EntranceTarget, c| {
                c.field("role", &e.role, &a.role);
                c.field("target", &e.target, &a.target);
                c.field("text", &e.text, &a.text);
                c.field("resolution", &e.resolution, &a.resolution);
                c.field("reason", &e.reason, &a.reason);
                c.field("module", &e.module, &a.module);
            },
            &mut out,
        )?;
    }
    Ok(out)
}

fn files(graph: &Graph) -> impl Iterator<Item = (&str, &unfold_core::FileRecord)> {
    graph
        .files
        .iter()
        .map(|file| (file.file.path.as_str(), &file.file))
}

fn blocks(graph: &Graph) -> impl Iterator<Item = (&str, &unfold_core::Block)> {
    graph
        .files
        .iter()
        .flat_map(|file| file.blocks.iter().map(|block| (block.key.as_str(), block)))
}

/// A block's key by local ID within one file; the empty string for a dangling ID, which then
/// shows up as a difference rather than a panic.
fn local_key(blocks: &[unfold_core::Block], local_id: u32) -> &str {
    blocks
        .get(local_id as usize)
        .map_or("", |block| block.key.as_str())
}

type SlotId<'g> = (&'g str, SlotName, Option<u32>);

fn slots(graph: &Graph) -> impl Iterator<Item = (SlotId<'_>, &Slot)> {
    graph.files.iter().flat_map(|file| {
        file.slots
            .iter()
            .map(|slot| ((local_key(&file.blocks, slot.block), slot.slot, slot.index), slot))
    })
}

fn edges(graph: &Graph) -> impl Iterator<Item = ((&str, u32), &Edge)> {
    graph.files.iter().flat_map(|file| {
        file.edges
            .iter()
            .map(|edge| ((local_key(&file.blocks, edge.source), edge.seq), edge))
    })
}

fn targets(graph: &Graph) -> impl Iterator<Item = ((&str, u32), &EntranceTarget)> {
    graph.entrances.iter().flat_map(|entrance| {
        entrance
            .targets
            .iter()
            .map(|target| ((entrance.key.as_str(), target.position), target))
    })
}

/// Collects the field changes of one matched pair.
struct Changes(Vec<FieldChange>);

impl Changes {
    fn field<T: Render + PartialEq + ?Sized>(&mut self, field: &'static str, expected: &T, actual: &T) {
        if expected != actual {
            self.0.push(FieldChange {
                field,
                expected: expected.render(),
                actual: actual.render(),
            });
        }
    }
}

/// Matches one record type's records by identity and appends their differences.
fn compare<I: Copy + Eq + Hash, R>(
    expected: impl Iterator<Item = (I, R)>,
    actual: impl Iterator<Item = (I, R)>,
    id: impl Fn(&I) -> RecordId,
    fields: impl Fn(&R, &R, &mut Changes),
    out: &mut Vec<Difference>,
) -> anyhow::Result<()> {
    let expected = index("expected", expected, &id)?;
    let actual = index("actual", actual, &id)?;
    for (identity, expected_record) in &expected.records {
        match actual.positions.get(identity) {
            None => out.push(Difference {
                id: id(identity),
                change: Change::Missing,
            }),
            Some(&position) => {
                let mut changes = Changes(Vec::new());
                fields(expected_record, &actual.records[position].1, &mut changes);
                if !changes.0.is_empty() {
                    out.push(Difference {
                        id: id(identity),
                        change: Change::Changed(changes.0),
                    });
                }
            }
        }
    }
    for (identity, _) in &actual.records {
        if !expected.positions.contains_key(identity) {
            out.push(Difference {
                id: id(identity),
                change: Change::Extra,
            });
        }
    }
    Ok(())
}

/// One side's records in export order, with a position index by identity.
struct Indexed<I, R> {
    records: Vec<(I, R)>,
    positions: FxHashMap<I, usize>,
}

fn index<I: Copy + Eq + Hash, R>(
    side: &str,
    records: impl Iterator<Item = (I, R)>,
    id: impl Fn(&I) -> RecordId,
) -> anyhow::Result<Indexed<I, R>> {
    let records: Vec<(I, R)> = records.collect();
    let mut positions = FxHashMap::with_capacity_and_hasher(records.len(), Default::default());
    for (position, (identity, _)) in records.iter().enumerate() {
        if positions.insert(*identity, position).is_some() {
            bail!("the {side} graph repeats {}", id(identity));
        }
    }
    Ok(Indexed { records, positions })
}

/// A field value as text, `None` when absent.
pub(crate) trait Render {
    fn render(&self) -> Option<String>;
}

impl<T: Render> Render for Option<T> {
    fn render(&self) -> Option<String> {
        self.as_ref().and_then(Render::render)
    }
}

macro_rules! render_display {
    ($($type:ty),+ $(,)?) => {
        $(impl Render for $type {
            fn render(&self) -> Option<String> {
                Some(self.to_string())
            }
        })+
    };
}

render_display!(
    Box<str>,
    u32,
    Key,
    RepoPath,
    ContentHash,
    unfold_core::vocab::Language,
    unfold_core::vocab::FileState,
    unfold_core::vocab::InputRole,
    unfold_core::vocab::BlockKind,
    unfold_core::vocab::Role,
    unfold_core::vocab::EdgeKind,
    unfold_core::vocab::Resolution,
    unfold_core::vocab::Reason,
    unfold_core::vocab::EntranceKind,
    unfold_core::vocab::TriggerStatus,
    unfold_core::vocab::TargetRole,
    unfold_core::vocab::HttpMethod,
    unfold_core::vocab::DetectorId,
);

impl Render for ModuleLabel {
    fn render(&self) -> Option<String> {
        Some(self.as_str().to_owned())
    }
}

/// Ranges and lines render as in the canonical export: `[start,end]`.
impl Render for ByteRange {
    fn render(&self) -> Option<String> {
        Some(format!("[{},{}]", self.start, self.end))
    }
}

impl Render for LineSpan {
    fn render(&self) -> Option<String> {
        Some(format!("[{},{}]", self.start, self.end))
    }
}

#[cfg(test)]
mod tests;
