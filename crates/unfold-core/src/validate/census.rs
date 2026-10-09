//! Checks against a language front end's independent census.

use rustc_hash::FxHashMap;

use super::{CallSite, Census, Checker, Subject};
use crate::model::{Edge, FileGraph};
use crate::text::ByteRange;
use crate::vocab::{BlockKind, EdgeKind};

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph, census: &dyn Census) {
    let path = &file.file.path;
    for block in file.blocks.iter().skip(1) {
        if !census.is_token_range(path, block.range) {
            checker.report(
                Subject::Block(block.key.clone()),
                format!("range {} is not on token boundaries", block.range),
            );
        }
    }
    for slot in &file.slots {
        if let Some(block) = file.blocks.get(slot.block as usize)
            && !census.is_token_range(path, slot.range)
        {
            let subject = Subject::Slot {
                block: block.key.clone(),
                slot: slot.slot,
                index: slot.index,
            };
            checker.report(
                subject,
                format!("range {} is not on token boundaries", slot.range),
            );
        }
    }
    if let Some(sites) = census.call_sites(path) {
        check_call_sites(checker, file, &sites);
    }
    if let Some(dependencies) = census.module_dependencies(path) {
        check_dependencies(checker, file, &dependencies);
    }
}

fn check_call_sites(checker: &mut Checker<'_>, file: &FileGraph, sites: &[CallSite]) {
    // Blocks by position in `file.blocks` (not by their `local_id` field, which a malformed
    // graph may get wrong), so indexing below never goes out of bounds.
    let mut blocks: FxHashMap<(ByteRange, BlockKind), Vec<usize>> = FxHashMap::default();
    for (index, block) in file.blocks.iter().enumerate() {
        if matches!(block.kind, BlockKind::Call | BlockKind::New) {
            blocks.entry((block.range, block.kind)).or_default().push(index);
        }
    }
    let mut edges_by_source: FxHashMap<u32, Vec<&Edge>> = FxHashMap::default();
    for edge in &file.edges {
        edges_by_source.entry(edge.source).or_default().push(edge);
    }
    let mut matched: FxHashMap<(ByteRange, BlockKind), u32> = FxHashMap::default();
    for site in sites {
        let site_key = (site.range, site.block_kind);
        *matched.entry(site_key).or_insert(0) += 1;
        let &[index] = blocks.get(&site_key).map_or(&[][..], Vec::as_slice) else {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!(
                    "call site {} needs exactly one {} block",
                    site.range, site.block_kind
                ),
            );
            continue;
        };
        let block = &file.blocks[index];
        let mut occurrence: Option<EdgeKind> = None;
        let source = u32::try_from(index).unwrap_or(u32::MAX);
        for edge in edges_by_source.get(&source).map_or(&[][..], Vec::as_slice) {
            if edge.kind.is_occurrence() {
                occurrence.get_or_insert(edge.kind);
            } else if edge
                .position
                .is_some_and(|position| site.ineligible_references.contains(&position))
            {
                checker.report(
                    Subject::Edge {
                        source: block.key.clone(),
                        seq: edge.seq,
                    },
                    "the argument at this position cannot supply a named reference",
                );
            }
        }
        if let (Some(expected), Some(actual)) = (site.edge_kind, occurrence)
            && expected != actual
        {
            checker.report(
                Subject::Block(block.key.clone()),
                format!("occurrence edge is {actual}, but the front end found {expected}"),
            );
        }
    }
    let mut unmatched: Vec<usize> = blocks
        .iter()
        .filter(|(site_key, _)| matched.get(*site_key) != Some(&1))
        .flat_map(|(_, indexes)| indexes.iter().copied())
        .collect();
    unmatched.sort_unstable();
    for index in unmatched {
        checker.report(
            Subject::Block(file.blocks[index].key.clone()),
            "the block does not match exactly one call site",
        );
    }
}

fn check_dependencies(checker: &mut Checker<'_>, file: &FileGraph, dependencies: &[ByteRange]) {
    let mut blocks: FxHashMap<ByteRange, Vec<usize>> = FxHashMap::default();
    for (index, block) in file.blocks.iter().enumerate() {
        if block.kind == BlockKind::Import {
            blocks.entry(block.range).or_default().push(index);
        }
    }
    let mut matched: FxHashMap<ByteRange, u32> = FxHashMap::default();
    for range in dependencies {
        *matched.entry(*range).or_insert(0) += 1;
        if blocks.get(range).map_or(0, Vec::len) != 1 {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!("module dependency {range} needs exactly one import block"),
            );
        }
    }
    let mut unmatched: Vec<usize> = blocks
        .iter()
        .filter(|(range, _)| matched.get(*range) != Some(&1))
        .flat_map(|(_, indexes)| indexes.iter().copied())
        .collect();
    unmatched.sort_unstable();
    for index in unmatched {
        checker.report(
            Subject::Block(file.blocks[index].key.clone()),
            "the import does not match exactly one module dependency",
        );
    }
}
