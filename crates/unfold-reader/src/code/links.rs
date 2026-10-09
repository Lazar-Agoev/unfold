//! Call links: every occurrence (call, construction, import) in the shown slice, at its written
//! range.
//!
//! A link covers the written callee (or an import's specifier; the whole occurrence when neither
//! slot was recorded). A nested call that begins the callee (`res.status(202).json`) keeps its own
//! link, and the outer link starts after it. Shorter links claim characters first, so links never
//! overlap. Leading `.`, `?`, and whitespace are trimmed from the underline.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use unfold_core::vocab::SlotName;
use unfold_core::{ByteRange, Edge, FileGraph};

use super::{CallLink, Offsets};
use crate::error::Result;
use crate::index::{file_owners, file_subtree_ends, to_u32};
use crate::notes::note;

/// The written range a link starts from: the callee or specifier slot, else the occurrence.
fn written_range(file: &FileGraph, source: u32) -> ByteRange {
    let start = file.slots.partition_point(|slot| slot.block < source);
    file.slots[start..]
        .iter()
        .take_while(|slot| slot.block == source)
        .find(|slot| matches!(slot.slot, SlotName::Callee | SlotName::Specifier))
        .map_or(file.blocks[source as usize].range, |slot| slot.range)
}

/// Whether `range` intersects an interval already in `taken` (start → end, non-overlapping).
fn overlaps(taken: &BTreeMap<u32, u32>, range: ByteRange) -> bool {
    taken
        .range(..range.end)
        .next_back()
        .is_some_and(|(_, &end)| end > range.start)
}

/// The links of block `shown` of `file`, whose verified text is `text`, in source order.
pub(super) fn links(
    file: &FileGraph,
    shown: u32,
    text: &str,
    offsets: &Offsets<'_>,
) -> Result<Vec<CallLink>> {
    let ends = file_subtree_ends(&file.blocks);
    let end = ends[shown as usize];
    let edges: Vec<&Edge> = file
        .edges
        .iter()
        .filter(|edge| edge.kind.is_occurrence() && edge.source >= shown && edge.source < end)
        .collect();
    let occurrences: Vec<ByteRange> = edges
        .iter()
        .map(|edge| file.blocks[edge.source as usize].range)
        .collect();
    for &range in &occurrences {
        if !offsets.contains(range) {
            return Err(offsets.invalid(range));
        }
    }
    // Occurrence ends by start, for "a nested call that begins the callee".
    let mut ends_by_start: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for range in &occurrences {
        ends_by_start.entry(range.start).or_default().push(range.end);
    }
    let ranges: Vec<ByteRange> = edges
        .iter()
        .map(|edge| {
            let mut range = written_range(file, edge.source);
            // Step past the longest nested occurrence that begins where the range does, then
            // repeat from the new start (`a.b().c().d` → `.d`).
            while let Some(&nested) = ends_by_start.get(&range.start).and_then(|ends| {
                ends.iter()
                    .filter(|&&nested| nested > range.start && nested < range.end)
                    .max()
            }) {
                range.start = nested;
            }
            range
        })
        .collect();

    let mut order: Vec<usize> = (0..ranges.len()).collect();
    order.sort_by_key(|&index| (ranges[index].len(), index));
    let mut taken: BTreeMap<u32, u32> = BTreeMap::new();
    let mut claimed: Vec<Option<ByteRange>> = vec![None; ranges.len()];
    for index in order {
        let range = ranges[index];
        if range.is_empty() || !offsets.contains(range) || overlaps(&taken, range) {
            continue;
        }
        taken.insert(range.start, range.end);
        claimed[index] = Some(range);
    }

    let owners = file_owners(&file.blocks);
    edges
        .iter()
        .zip(claimed)
        .map(|(edge, claimed)| {
            let source = &file.blocks[edge.source as usize];
            let span = match claimed {
                Some(range) => {
                    let written = text.get(range.as_usize()).unwrap_or("");
                    let skipped = written
                        .char_indices()
                        .find(|&(_, character)| {
                            !(character == '.' || character == '?' || character.is_whitespace())
                        })
                        .map_or(written.len(), |(at, _)| at);
                    let start = range.start + to_u32(skipped);
                    (start < range.end).then_some((start, range.end))
                }
                None => None,
            };
            let span = match span {
                Some((start, end)) => Some([offsets.at(start)?, offsets.at(end)?]),
                None => None,
            };
            Ok(CallLink {
                source: source.key.clone(),
                owner: file.blocks[owners[edge.source as usize] as usize].key.clone(),
                kind: edge.kind,
                resolution: edge.resolution,
                target: edge.target.clone(),
                text: edge.text.clone(),
                note: note(edge.resolution, edge.reason, edge.module.as_ref()),
                reason: edge.reason,
                module: edge.module.clone(),
                span,
                line: source.lines.start,
            })
        })
        .collect()
}
