//! File records, blocks, and slots.

use rustc_hash::FxHashMap;

use super::{Checker, Subject};
use crate::key::KeyBuilder;
use crate::model::{Block, FileGraph};
use crate::text::{ByteRange, LineIndex, Snapshot};
use crate::vocab::{BlockKind, FileState, Language, Role, SlotName};

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph) {
    let snapshot = check_record(checker, file);
    let lines = snapshot.map(Snapshot::line_index);
    let context = FileContext {
        file,
        snapshot,
        lines: lines.as_ref(),
    };
    check_blocks(checker, &context);
    check_keys(checker, file);
    check_slots(checker, &context);
}

struct FileContext<'a> {
    file: &'a FileGraph,
    snapshot: Option<&'a Snapshot>,
    lines: Option<&'a LineIndex>,
}

/// Checks the file record and returns the snapshot to check ranges against, if it matches.
fn check_record<'g>(checker: &mut Checker<'g>, file: &FileGraph) -> Option<&'g Snapshot> {
    let record = &file.file;
    let subject = || Subject::File(record.path.clone());
    let has_blocks = !file.blocks.is_empty();
    match record.state {
        FileState::Current | FileState::LastGood => {
            if record.hash.is_none() {
                checker.report(
                    subject(),
                    format!("a {} file needs the hash its facts came from", record.state),
                );
            }
            if !has_blocks {
                checker.report(subject(), format!("a {} file needs its blocks", record.state));
            }
            match (record.state, &record.failure) {
                (FileState::Current, Some(_)) => checker.report(subject(), "a current file has no failure"),
                (FileState::LastGood, None) => {
                    checker.report(subject(), "a last_good file needs its failure");
                }
                _ => {}
            }
        }
        FileState::Failed => {
            if record.hash.is_some() || has_blocks || !file.slots.is_empty() || !file.edges.is_empty() {
                checker.report(
                    subject(),
                    "a failed file has no facts: no hash, blocks, slots, or edges",
                );
            }
            if record.failure.is_none() {
                checker.report(subject(), "a failed file needs its failure");
            }
        }
    }
    if record.failure.as_deref() == Some("") {
        checker.report(subject(), "failure is empty");
    }
    // Last-good facts come from earlier bytes, so only current facts are checked against disk.
    if checker.sources.is_none() || record.state != FileState::Current {
        return None;
    }
    let Some(snapshot) = checker.snapshot(&record.path) else {
        checker.report(subject(), "no snapshot was supplied");
        return None;
    };
    if record.hash != Some(snapshot.hash()) {
        checker.report(
            subject(),
            format!("fingerprint does not match the snapshot's {}", snapshot.hash()),
        );
        return None;
    }
    Some(snapshot)
}

fn check_blocks(checker: &mut Checker<'_>, context: &FileContext<'_>) {
    let file = context.file;
    let language = file.file.language;
    // The chain of open ancestors of the next block in preorder.
    let mut ancestors: Vec<u32> = Vec::new();
    // The previous child seen under each parent.
    let mut last_child: FxHashMap<u32, u32> = FxHashMap::default();
    for (index, block) in file.blocks.iter().enumerate() {
        let subject = || Subject::Block(block.key.clone());
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        if block.local_id != index {
            checker.report(
                subject(),
                format!("local ID {} is not its preorder position {index}", block.local_id),
            );
        }
        if !block.kind.allowed_in(language) {
            checker.report(
                subject(),
                format!("kind {} does not occur in {language} files", block.kind),
            );
        }
        if block.name.as_deref() == Some("") {
            checker.report(subject(), "name is empty; an absent name is omitted");
        }
        if block.kind == BlockKind::Import && block.name.is_some() {
            checker.report(subject(), "an import has no name");
        }
        if block.range.start > block.range.end {
            checker.report(subject(), format!("range {} is reversed", block.range));
        }
        if block.lines.start == 0 || block.lines.end < block.lines.start {
            checker.report(
                subject(),
                format!("{:?} are not one-based and ordered", block.lines),
            );
        }
        if index == 0 {
            check_root(checker, context, block);
            ancestors.clear();
            ancestors.push(0);
            continue;
        }
        check_child(checker, file, index, block, &mut ancestors, &mut last_child);
        check_text(checker, context, block);
        ancestors.push(index);
    }
}

fn check_root(checker: &mut Checker<'_>, context: &FileContext<'_>, root: &Block) {
    let subject = || Subject::Block(root.key.clone());
    if root.kind != BlockKind::File || root.parent.is_some() || root.role.is_some() || root.name.is_some() {
        checker.report(
            subject(),
            "the first block must be a file root with no parent, role, or name",
        );
    }
    if root.range.start != 0 || root.lines.start != 1 {
        checker.report(subject(), "a file root starts at byte 0 on line 1");
    }
    if let (Some(snapshot), Some(lines)) = (context.snapshot, context.lines) {
        if root.range.end != snapshot.len() {
            checker.report(
                subject(),
                format!("a file root spans the whole file ({} bytes)", snapshot.len()),
            );
        } else if root.lines != lines.span(root.range) {
            checker.report(
                subject(),
                format!(
                    "{:?} disagree with the text ({:?})",
                    root.lines,
                    lines.span(root.range)
                ),
            );
        }
    }
}

fn check_child(
    checker: &mut Checker<'_>,
    file: &FileGraph,
    index: u32,
    block: &Block,
    ancestors: &mut Vec<u32>,
    last_child: &mut FxHashMap<u32, u32>,
) {
    let subject = || Subject::Block(block.key.clone());
    if block.kind == BlockKind::File {
        checker.report(subject(), "only the first block is a file root");
    }
    let Some(parent_id) = block.parent else {
        checker.report(subject(), "a non-root block needs a parent");
        return;
    };
    match ancestors.iter().rposition(|&ancestor| ancestor == parent_id) {
        Some(position) => ancestors.truncate(position + 1),
        None => {
            checker.report(
                subject(),
                format!("parent {parent_id} is not an open ancestor, so blocks are not in preorder"),
            );
            ancestors.truncate(1);
        }
    }
    let Some(parent) = file.blocks.get(parent_id as usize).filter(|_| parent_id < index) else {
        checker.report(
            subject(),
            format!("parent {parent_id} does not precede the block"),
        );
        return;
    };
    if !parent.range.contains(block.range) || !block.lines.within(parent.lines) {
        checker.report(
            subject(),
            format!("parent range {} does not contain {}", parent.range, block.range),
        );
    }
    if block.range.is_empty() {
        checker.report(subject(), "a non-root range is never empty");
    }
    if let Some(role) = block.role
        && !role.allowed_in(parent.kind, file.file.language)
    {
        checker.report(
            subject(),
            format!(
                "role {role} under {} does not occur in {} files",
                parent.kind, file.file.language
            ),
        );
    }
    if let Some(previous) = last_child.insert(parent_id, index)
        && let Some(previous) = file.blocks.get(previous as usize)
        && previous.range.start > block.range.start
    {
        checker.report(subject(), "siblings are not in start order");
    }
}

/// Checks a non-root block's range against its text: boundaries, lines, and trivia.
fn check_text(checker: &mut Checker<'_>, context: &FileContext<'_>, block: &Block) {
    let (Some(snapshot), Some(lines)) = (context.snapshot, context.lines) else {
        return;
    };
    let subject = || Subject::Block(block.key.clone());
    let Some(slice) = slice(snapshot, block.range) else {
        checker.report(
            subject(),
            format!(
                "range {} is outside the file or not on character boundaries",
                block.range
            ),
        );
        return;
    };
    if block.lines != lines.span(block.range) {
        checker.report(
            subject(),
            format!(
                "{:?} disagree with the text ({:?})",
                block.lines,
                lines.span(block.range)
            ),
        );
    }
    if let Some(problem) = trivia(slice, context.file.file.language) {
        checker.report(subject(), problem);
    }
}

/// The text of `range`, if it lies inside the snapshot on character boundaries.
pub(super) fn slice(snapshot: &Snapshot, range: ByteRange) -> Option<&str> {
    snapshot.text().get(range.as_usize())
}

/// Why a token-exact range is not, judging by its text alone.
fn trivia(text: &str, language: Language) -> Option<&'static str> {
    let is_space = |character: char| character.is_whitespace() || character == '\u{feff}';
    if text.starts_with(is_space) {
        return Some("range starts with whitespace");
    }
    let comment = match language {
        Language::TypeScript => text.starts_with("//") || text.starts_with("/*"),
        Language::Python => text.starts_with('#'),
    };
    if comment {
        return Some("range starts with a comment");
    }
    if text.ends_with(is_space) {
        return Some("range ends with whitespace");
    }
    None
}

fn check_keys(checker: &mut Checker<'_>, file: &FileGraph) {
    let mut builder = KeyBuilder::new(&file.file.path);
    for block in &file.blocks {
        match builder.push(block.parent, block.kind, block.name.as_deref()) {
            Ok(key) if key != block.key => {
                checker.report(
                    Subject::Block(block.key.clone()),
                    format!("key differs from its derivation `{key}`"),
                );
            }
            Ok(_) => {}
            Err(error) => {
                // The structural problem is reported by the block checks; later keys cannot be
                // derived without a consistent tree.
                checker.report(
                    Subject::Block(block.key.clone()),
                    format!("cannot derive the key: {error}"),
                );
                return;
            }
        }
    }
}

fn check_slots(checker: &mut Checker<'_>, context: &FileContext<'_>) {
    let file = context.file;
    let language = file.file.language;
    // How many slots of each name each block has had so far.
    let mut counts: FxHashMap<(u32, SlotName), u32> = FxHashMap::default();
    let mut previous: Option<(u32, u32)> = None;
    for slot in &file.slots {
        let Some(block) = file.blocks.get(slot.block as usize) else {
            let subject = Subject::File(file.file.path.clone());
            checker.report(
                subject,
                format!("slot {} names missing block {}", slot.slot, slot.block),
            );
            continue;
        };
        let subject = || Subject::Slot {
            block: block.key.clone(),
            slot: slot.slot,
            index: slot.index,
        };
        if !block.kind.has_slot(slot.slot, language) {
            checker.report(
                subject(),
                format!("a {language} {} has no {} slot", block.kind, slot.slot),
            );
        }
        let count = counts.entry((slot.block, slot.slot)).or_insert(0);
        if block.kind.slot_is_indexed(slot.slot, language) {
            if slot.index != Some(*count) {
                checker.report(
                    subject(),
                    format!("indices must be contiguous from zero; expected {count}"),
                );
            }
        } else if slot.index.is_some() {
            checker.report(subject(), "a single slot has no index");
        } else if *count > 0 {
            checker.report(subject(), "duplicate slot");
        }
        *count += 1;
        let position = (slot.block, slot.range.start);
        if previous.is_some_and(|previous| previous > position) {
            checker.report(subject(), "slots are not in block, then start, order");
        }
        previous = Some(position);
        if slot.range.is_empty() || !block.range.contains(slot.range) {
            checker.report(
                subject(),
                format!(
                    "range {} is empty or outside the block's {}",
                    slot.range, block.range
                ),
            );
        } else if let Some(snapshot) = context.snapshot {
            match slice(snapshot, slot.range) {
                None => checker.report(
                    subject(),
                    format!("range {} is not on character boundaries", slot.range),
                ),
                Some(text) => {
                    if let Some(problem) = trivia(text, language) {
                        checker.report(subject(), problem);
                    }
                }
            }
        }
    }
}

/// The position in `file.blocks` of the block that owns the block at `index` for graph
/// purposes: the nearest file, class (unless reached through a Python header), or
/// function-like ancestor reached through its body or a parameter. Used to bound parameter
/// positions.
///
/// The walk follows positions, not the blocks' own `local_id` fields, so a malformed graph
/// (reported elsewhere) can never send it out of bounds.
pub(super) fn owner(file: &FileGraph, index: usize) -> Option<usize> {
    let mut current = index;
    loop {
        let block = file.blocks.get(current)?;
        // Parents strictly precede their children, so the walk terminates; a parent that does
        // not precede is malformed and ends it.
        let parent_index = usize::try_from(block.parent?)
            .ok()
            .filter(|&parent| parent < current)?;
        let parent = &file.blocks[parent_index];
        let through_body = matches!(block.role, Some(Role::Body | Role::Parameter));
        if parent.kind == BlockKind::File
            || (parent.kind == BlockKind::Class && block.role != Some(Role::Header))
            || (parent.kind.is_function_like() && through_body)
        {
            return Some(parent_index);
        }
        current = parent_index;
    }
}
