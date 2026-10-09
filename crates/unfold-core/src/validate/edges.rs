//! Edges: occurrences, references, outcomes, and targets.

use rustc_hash::FxHashMap;

use super::files::owner;
use super::{Checker, Subject};
use crate::model::{Edge, FileGraph};
use crate::text::{LABEL_MAX_UTF16, utf16_len};
use crate::vocab::{BlockKind, EdgeKind, Reason, Resolution, SlotName};

/// Per-block slot counts that bound edge positions.
#[derive(Default, Clone, Copy)]
struct SlotCounts {
    arguments: u32,
    parameters: u32,
}

/// What has been seen of one source block's edges so far.
#[derive(Default)]
struct SourceState {
    occurrences: u32,
    references: u32,
    last_reference_position: Option<u32>,
}

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph) {
    let mut slot_counts = vec![SlotCounts::default(); file.blocks.len()];
    for slot in &file.slots {
        if let Some(counts) = slot_counts.get_mut(slot.block as usize) {
            match slot.slot {
                SlotName::Argument => counts.arguments += 1,
                SlotName::Parameter => counts.parameters += 1,
                _ => {}
            }
        }
    }
    let mut sources: FxHashMap<u32, SourceState> = FxHashMap::default();
    let mut previous: Option<(u32, u32)> = None;
    for edge in &file.edges {
        let Some(source) = file.blocks.get(edge.source as usize) else {
            let subject = Subject::File(file.file.path.clone());
            checker.report(
                subject,
                format!("edge {} names missing source block {}", edge.seq, edge.source),
            );
            continue;
        };
        let subject = || Subject::Edge {
            source: source.key.clone(),
            seq: edge.seq,
        };
        let state = sources.entry(edge.source).or_default();
        let expected_seq = state.occurrences + state.references;
        if edge.seq != expected_seq {
            checker.report(
                subject(),
                format!("sequence numbers must be contiguous from zero; expected {expected_seq}"),
            );
        }
        if previous.is_some_and(|previous| previous >= (edge.source, edge.seq)) {
            checker.report(subject(), "edges are not in source, then sequence, order");
        }
        previous = Some((edge.source, edge.seq));
        if !source.kind.is_occurrence() {
            checker.report(
                subject(),
                format!(
                    "an edge's source is a call, new, or import, not a {}",
                    source.kind
                ),
            );
        }
        if edge.kind == EdgeKind::References {
            if !matches!(source.kind, BlockKind::Call | BlockKind::New) {
                checker.report(subject(), "a reference's source is a call or new");
            }
            if state.occurrences != 1 {
                checker.report(subject(), "a reference must follow its source's occurrence edge");
            }
            state.references += 1;
            let arguments = slot_counts[edge.source as usize].arguments;
            match edge.position {
                Some(position) if position < arguments => {
                    if state.last_reference_position.is_some_and(|last| last >= position) {
                        checker.report(subject(), "reference positions must be distinct and increasing");
                    }
                    state.last_reference_position = Some(position);
                }
                Some(position) => checker.report(
                    subject(),
                    format!("reference position {position} is outside the {arguments} arguments"),
                ),
                None => checker.report(subject(), "a reference needs its argument position"),
            }
            if edge.resolution != Resolution::Resolved {
                checker.report(subject(), "references are always resolved");
            }
        } else {
            state.occurrences += 1;
            if state.references > 0 {
                checker.report(
                    subject(),
                    "an occurrence edge must precede its source's references",
                );
            }
            if source.kind.is_occurrence()
                && !source
                    .kind
                    .occurrence_edge_kinds(file.file.language)
                    .contains(&edge.kind)
            {
                checker.report(
                    subject(),
                    format!(
                        "a {} {} cannot source a {} edge",
                        file.file.language, source.kind, edge.kind
                    ),
                );
            }
            if let Some(position) = edge.position {
                let parameters =
                    owner(file, edge.source as usize).map_or(0, |owner| slot_counts[owner].parameters);
                let parameter_call = matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs)
                    && edge.resolution == Resolution::Unresolved
                    && edge.reason == Some(Reason::Parameter);
                if !parameter_call || position >= parameters {
                    checker.report(
                        subject(),
                        format!("position {position} is only for an unresolved parameter call within its owner's {parameters} parameters"),
                    );
                }
            }
        }
        check_outcome(checker, file, edge, &subject);
    }
    for (index, block) in file.blocks.iter().enumerate() {
        if block.kind.is_occurrence() {
            // Edges name their source by position, so look the block up by position too.
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let occurrences = sources.get(&index).map_or(0, |state| state.occurrences);
            if occurrences != 1 {
                checker.report(
                    Subject::Block(block.key.clone()),
                    format!(
                        "a {} has exactly one occurrence edge, found {occurrences}",
                        block.kind
                    ),
                );
            }
        }
    }
}

fn check_outcome(checker: &mut Checker<'_>, file: &FileGraph, edge: &Edge, subject: &dyn Fn() -> Subject) {
    match (edge.resolution, edge.reason) {
        (Resolution::Unresolved, None) => checker.report(subject(), "an unresolved edge needs a reason"),
        (Resolution::Resolved | Resolution::External, Some(_)) => {
            checker.report(subject(), "only an unresolved edge has a reason");
        }
        _ => {}
    }
    match (edge.resolution, &edge.module) {
        (Resolution::External, None) => checker.report(subject(), "an external edge needs a module"),
        (Resolution::External, Some(module)) if !module.is_valid() => {
            checker.report(
                subject(),
                format!(
                    "module `{}` is not a label or a sorted set of labels",
                    module.as_str()
                ),
            );
        }
        (Resolution::Resolved | Resolution::Unresolved, Some(_)) => {
            checker.report(subject(), "only an external edge has a module");
        }
        _ => {}
    }
    match (edge.resolution, &edge.target) {
        (Resolution::Resolved, None) => checker.report(subject(), "a resolved edge needs a target"),
        (Resolution::Resolved, Some(target)) => match checker.resolve(target) {
            None => checker.report(
                subject(),
                format!("target `{target}` is not a block in the graph"),
            ),
            Some((target_file, target_block)) => {
                if !edge.kind.target_kinds().contains(&target_block.kind) {
                    checker.report(
                        subject(),
                        format!("a {} edge cannot target a {}", edge.kind, target_block.kind),
                    );
                }
                if target_file.file.language != file.file.language {
                    checker.report(subject(), "a resolved edge cannot cross languages");
                }
                let target_file_index = checker.files.get(target_file.file.path.as_str()).copied();
                if edge.kind == EdgeKind::Constructs
                    && target_block.kind == BlockKind::Class
                    && target_file_index.is_some_and(|index| {
                        checker
                            .constructed_classes
                            .contains(&(index, target_block.local_id))
                    })
                {
                    checker.report(
                        subject(),
                        "the class declares a constructor; a construction targets the constructor",
                    );
                }
            }
        },
        (_, Some(_)) => checker.report(subject(), "only a resolved edge has a target"),
        (_, None) => {}
    }
    if utf16_len(&edge.text) > LABEL_MAX_UTF16 {
        checker.report(
            subject(),
            format!("text exceeds {LABEL_MAX_UTF16} UTF-16 code units"),
        );
    }
}
