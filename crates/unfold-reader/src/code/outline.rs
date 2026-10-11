//! The statement outline of a shown block: which blocks the Code view tints, folds, and groups.
//!
//! It is derived from stored blocks only. An outline item is a nested callable, or a statement
//! (`variable`, `expression_statement`, control flow, `return`, `throw`, `import`, `class`, a
//! type declaration, …) that has no role or a `body`, `then`, `else`, `catch`, or `finally` role.
//! Its parent is the nearest enclosing item.
//!
//! Compound items (`if`, loops, `switch`, `try`, `catch`, callables, classes, type declarations,
//! Python `with` and `match`) that span several lines fold. In a card longer than about 40 lines,
//! nested definitions (callables, classes, and type declarations) and `if` statements heading an
//! `else if` chain of three or more checks start folded.

use unfold_core::vocab::{BlockKind, Language, Role, SlotName};
use unfold_core::{ByteRange, FileGraph, Key};

use super::{Fold, Offsets, OutlineItem};
use crate::error::Result;
use crate::index::{file_subtree_ends, to_u32};

const RETURNS: u8 = 1;
const THROWS: u8 = 2;

fn is_statement(kind: BlockKind) -> bool {
    use BlockKind as K;
    matches!(
        kind,
        K::Variable
            | K::ExpressionStatement
            | K::If
            | K::For
            | K::ForIn
            | K::ForOf
            | K::While
            | K::DoWhile
            | K::Switch
            | K::Case
            | K::Default
            | K::Try
            | K::Catch
            | K::Return
            | K::Throw
            | K::Break
            | K::Continue
            | K::Import
            | K::Class
            | K::Interface
            | K::TypeAlias
            | K::Enum
            | K::With
            | K::Match
    )
}

fn is_statement_role(role: Option<Role>) -> bool {
    matches!(
        role,
        None | Some(Role::Body | Role::Then | Role::Else | Role::Catch | Role::Finally)
    )
}

fn is_group_role(role: Option<Role>) -> bool {
    matches!(
        role,
        Some(Role::Body | Role::Then | Role::Else | Role::Catch | Role::Finally)
    )
}

/// A nested callable, class, or type declaration: it starts folded in a long card and is never
/// a group member.
fn is_callback(kind: BlockKind) -> bool {
    kind.is_function_like() || kind.is_type_declaration()
}

fn is_compound(kind: BlockKind) -> bool {
    use BlockKind as K;
    is_callback(kind)
        || matches!(
            kind,
            K::If
                | K::For
                | K::ForIn
                | K::ForOf
                | K::While
                | K::DoWhile
                | K::Switch
                | K::Try
                | K::Catch
                | K::With
                | K::Match
        )
}

/// The slot that starts a block's foldable body.
fn body_slot(kind: BlockKind) -> Option<SlotName> {
    use BlockKind as K;
    match kind {
        kind if kind.is_function_like() => Some(SlotName::Body),
        K::If => Some(SlotName::Then),
        K::Catch
        | K::Try
        | K::For
        | K::ForIn
        | K::ForOf
        | K::While
        | K::DoWhile
        | K::With
        | K::Match
        | K::Interface
        | K::Enum => Some(SlotName::Body),
        _ => None,
    }
}

/// The range of the first `slot` of block `local`.
fn slot_range(file: &FileGraph, local: u32, slot: SlotName) -> Option<ByteRange> {
    let start = file.slots.partition_point(|item| item.block < local);
    file.slots[start..]
        .iter()
        .take_while(|item| item.block == local)
        .find(|item| item.slot == slot)
        .map(|item| item.range)
}

/// The end of the line that `start` is on, within `text`.
fn line_end(text: &str, start: u32) -> u32 {
    let rest = text.get(start as usize..).unwrap_or("");
    let length = rest
        .find(['\n', '\r', '\u{2028}', '\u{2029}'])
        .unwrap_or(rest.len());
    start + to_u32(length)
}

/// The outline of block `shown` of `file` (whose verified text is `text`), in preorder. `long`
/// says whether the card is long enough for default folds.
pub(super) fn outline(
    file: &FileGraph,
    shown: u32,
    text: &str,
    offsets: &Offsets<'_>,
    long: bool,
) -> Result<Vec<OutlineItem>> {
    let blocks = &file.blocks;
    let ends = file_subtree_ends(blocks);
    let first = shown as usize + 1;
    let end = ends[shown as usize] as usize;
    let span = end.saturating_sub(first);
    let parent_in = |local: usize| {
        blocks[local]
            .parent
            .map(|parent| parent as usize)
            .filter(|&parent| parent >= first && parent < local)
    };

    // Bottom-up facts: exits (outside nested callables) and `else if` chain lengths.
    let mut exits = vec![0u8; span];
    let mut else_count = vec![0u8; span];
    let mut else_if = vec![usize::MAX; span];
    let mut chain = vec![0u32; span];
    for local in (first..end).rev() {
        let block = &blocks[local];
        let at = local - first;
        if block.kind == BlockKind::If {
            let next = (else_count[at] == 1 && else_if[at] != usize::MAX).then(|| chain[else_if[at] - first]);
            chain[at] = 1 + next.unwrap_or(0);
        }
        let Some(parent) = parent_in(local) else {
            continue;
        };
        let parent_at = parent - first;
        if !block.kind.is_function_like() {
            let own = match block.kind {
                BlockKind::Return => RETURNS,
                BlockKind::Throw => THROWS,
                _ => 0,
            };
            exits[parent_at] |= own | exits[at];
        }
        if block.role == Some(Role::Else) {
            else_count[parent_at] = else_count[parent_at].saturating_add(1);
            if block.kind == BlockKind::If {
                else_if[parent_at] = local;
            }
        }
    }

    let python = file.file.language == Language::Python;
    let mut items: Vec<OutlineItem> = Vec::new();
    // Enclosing items: (subtree end, item index).
    let mut open: Vec<(usize, u32)> = Vec::new();
    for local in first..end {
        while open.last().is_some_and(|&(subtree_end, _)| subtree_end <= local) {
            open.pop();
        }
        let block = &blocks[local];
        if !(block.kind.is_function_like() || (is_statement(block.kind) && is_statement_role(block.role))) {
            continue;
        }
        if !offsets.contains(block.range) {
            return Err(offsets.invalid(block.range));
        }
        let at = local - first;
        let fold = (is_compound(block.kind) && block.lines.end > block.lines.start)
            .then(|| {
                let body = body_slot(block.kind).and_then(|slot| slot_range(file, to_u32(local), slot));
                let header_end = match body {
                    Some(body) if !python => body.start,
                    _ => line_end(text, block.range.start).min(block.range.end),
                };
                let chain = (block.kind == BlockKind::If).then_some(chain[at]);
                let folded = long
                    && (is_callback(block.kind)
                        || (block.kind == BlockKind::If
                            && block.role != Some(Role::Else)
                            && chain.unwrap_or(1) >= 3));
                let lines = block.lines.end - block.lines.start;
                let mut summary = format!("{lines} {}", if lines == 1 { "line" } else { "lines" });
                if exits[at] & RETURNS != 0 {
                    summary.push_str(" · returns");
                }
                if exits[at] & THROWS != 0 {
                    summary.push_str(" · throws");
                }
                Ok::<_, crate::ReaderError>(Fold {
                    folded,
                    header_end: offsets.at(header_end)?,
                    braces: !python && body.is_some(),
                    summary,
                    returns: exits[at] & RETURNS != 0,
                    throws: exits[at] & THROWS != 0,
                    chain,
                })
            })
            .transpose()?;
        let index = to_u32(items.len());
        items.push(OutlineItem {
            key: block.key.clone(),
            kind: block.kind,
            role: block.role,
            parent: open.last().map(|&(_, item)| item),
            start_line: block.lines.start,
            end_line: block.lines.end,
            start: offsets.at(block.range.start)?,
            end: offsets.at(block.range.end)?,
            groupable: is_group_role(block.role) && !is_callback(block.kind),
            fold,
        });
        open.push((ends[local] as usize, index));
    }
    Ok(items)
}

/// The members of the group a gutter selection of lines `from` through `to` makes: consecutive
/// sibling statements with one parent and one role. The selection descends into a compound item
/// only when it lies strictly inside it. Empty when the selection makes no valid group.
pub fn group_for_lines(outline: &[OutlineItem], from: u32, to: u32) -> Vec<Key> {
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let children = |parent: Option<u32>| {
        outline
            .iter()
            .enumerate()
            .filter(move |(_, item)| item.parent == parent)
            .map(|(index, item)| (to_u32(index), item))
    };
    let mut level: Option<u32> = None;
    loop {
        let inside = children(level).find(|(index, item)| {
            !is_callback(item.kind)
                && children(Some(*index)).next().is_some()
                && from > item.start_line
                && to < item.end_line
        });
        match inside {
            Some((index, _)) => level = Some(index),
            None => break,
        }
    }
    let covered: Vec<&OutlineItem> = children(level)
        .map(|(_, item)| item)
        .filter(|item| !is_callback(item.kind) && item.end_line >= from && item.start_line <= to)
        .collect();
    let Some(role) = covered
        .first()
        .map(|item| item.role)
        .filter(|&role| is_group_role(role))
    else {
        return Vec::new();
    };
    covered
        .into_iter()
        .filter(|item| item.role == role)
        .map(|item| item.key.clone())
        .collect()
}
