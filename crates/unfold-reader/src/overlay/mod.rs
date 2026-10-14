//! The overlay rules (plan decision 2): what a page's overlay may do to a definition's drawing.
//! The drawing function calls them to tell the canvas what each block allows ([`allows`]), and
//! saving a board calls the checks to refuse what breaks them. There is no other copy of these
//! rules.
//!
//! | Overlay | Rule |
//! | --- | --- |
//! | Combine | Consecutive steps of one body, or exactly one whole control block ([`check_combined`]); a page's combined steps never overlap or nest ([`check_disjoint`]) |
//! | Propose | After a step of a body, or at the start of a definition's or a part's body ([`check_proposal`]) |
//! | Open in place | A project call or a callback; a path is 1 to [`MAX_OPEN_DEPTH`] such blocks, outermost first, each inside what the previous one opens; at most [`MAX_OPEN_PATHS`] paths per card ([`check_open_paths`]) |
//! | Name | Any block |
//!
//! A **body** is the code of a module, class, or function, or of one part (a branch, a loop
//! body, an `except`). Its **steps** are its children that run in it: calls, exits, control
//! blocks, and definitions, except what is written in a part's test (its test calls, and the
//! ternaries and comprehensions inside its head), which belongs to the test.
//! Decorators, class bases, and Python defaults are steps of the body they are written in.
//!
//! Every rule reads the records only (kinds, parents, links, argument ranges), never key text.

#[cfg(test)]
mod tests;

use unfold_core::vocab::{CallRole, NodeKind};
use unfold_core::{Key, LinkTarget};

use crate::tree::{Block, Blocks, FileTree};

/// The most blocks an open path holds.
pub const MAX_OPEN_DEPTH: usize = 3;

/// The most paths one card opens.
pub const MAX_OPEN_PATHS: usize = 64;

/// Why an overlay breaks the rules, naming the block at fault.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OverlayError {
    /// A key names no block in the graph.
    #[error("`{0}` is not a block in the graph")]
    Missing(Key),
    /// A combined step lists no blocks.
    #[error("a combined step needs at least one block")]
    NoMembers,
    /// A combined step lists a block twice.
    #[error("`{0}` is listed twice in one combined step")]
    Repeated(Key),
    /// A block is neither a step of a body nor a whole control block.
    #[error(
        "`{0}` can't be combined: combine steps of one body (calls, exits, control blocks, and definitions written directly in a definition's or a part's body) or one whole control block"
    )]
    NotCombinable(Key),
    /// A block is not in the same body as the first one.
    #[error("`{key}` is not in the body of `{first}`: combined steps are consecutive steps of one body")]
    OtherBody {
        /// The block at fault.
        key: Key,
        /// The combined step's first block.
        first: Key,
    },
    /// A block doesn't come right after the one before it.
    #[error(
        "`{key}` doesn't come right after `{previous}` in their body: combined steps are consecutive, in written order"
    )]
    NotConsecutive {
        /// The block at fault.
        key: Key,
        /// The block listed before it.
        previous: Key,
    },
    /// A block is in two combined steps.
    #[error("`{0}` is in two combined steps: combined steps never overlap")]
    Overlaps(Key),
    /// A block lies inside a block of another combined step.
    #[error("`{key}` lies inside `{outer}`, which another combined step holds: combined steps never nest")]
    Nests {
        /// The inner block.
        key: Key,
        /// The block of the other combined step it lies in.
        outer: Key,
    },
    /// A proposal names a block that is not a step of a body.
    #[error(
        "a proposal can't go after `{0}`: it goes after a step of a body (a call, exit, control block, or definition written directly in a definition's or a part's body)"
    )]
    NotAfter(Key),
    /// A proposal names a start that is no definition with a body or part.
    #[error(
        "a proposal can't go at the start of `{0}`: only a module, class, function, or part has a body to start"
    )]
    NoStart(Key),
    /// A block in an open path is neither a project call nor a callback.
    #[error("`{0}` can't open in place: only a call to a project function or class, or a callback, opens")]
    NotOpenable(Key),
    /// A block in an open path is not drawn in what the previous block opens (or in the card).
    #[error("`{key}` is not drawn in `{inside}`, so it can't open there")]
    NotInside {
        /// The block at fault.
        key: Key,
        /// The card, or what the previous block of the path opens.
        inside: Key,
    },
    /// An open path is empty or too deep.
    #[error("an open path holds 1 to 3 blocks; this one holds {0}")]
    PathLength(usize),
    /// A card opens too many paths.
    #[error("a card opens at most 64 paths; this one opens {0}")]
    TooManyPaths(usize),
    /// A card opens one path twice.
    #[error("the path ending at `{0}` is opened twice")]
    RepeatedPath(Key),
}

/// Whether node `id` holds a body: a module, class, function, or part. A type has no body.
pub fn is_body(tree: &FileTree, id: u32) -> bool {
    matches!(
        tree.kind(id),
        NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Part
    )
}

/// Whether node `id` is a step of the body it is written in: a call (not one in a part's
/// test), exit, control block (not one in a part's test), or definition whose parent holds a
/// body.
pub fn is_step(tree: &FileTree, id: u32) -> bool {
    let node = tree.node(id);
    let runs = match node.kind() {
        NodeKind::Call => node.role != Some(CallRole::Test),
        NodeKind::Exit => true,
        kind if kind.is_control() => !in_test(tree, id),
        kind => kind.is_definition(),
    };
    runs && tree.parent(id).is_some_and(|parent| is_body(tree, parent))
}

/// Whether node `id` is a control block written in its part's test: a ternary or comprehension
/// whose range lies inside the head of the part it is a child of.
pub fn in_test(tree: &FileTree, id: u32) -> bool {
    let range = tree.node(id).range;
    tree.kind(id).is_control()
        && tree
            .parent(id)
            .filter(|&parent| tree.kind(parent) == NodeKind::Part)
            .and_then(|part| tree.node(part).head)
            .is_some_and(|head| head.contains(range))
}

/// The steps of body `container`, in written order. Empty when `container` holds no body.
pub fn steps(tree: &FileTree, container: u32) -> impl Iterator<Item = u32> + '_ {
    let children = if is_body(tree, container) {
        tree.children(container)
    } else {
        &[]
    };
    children.iter().copied().filter(move |&id| is_step(tree, id))
}

/// What a block can be combined with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Combinable {
    /// With any consecutive run of the steps of body `container` ([`steps`]) that holds it.
    Steps {
        /// The definition or part whose body it is a step of.
        container: u32,
    },
    /// Only by itself: a whole control block written in an expression that the drawing draws
    /// as a step of its own (a ternary or comprehension in a call's arguments or an exit's
    /// value).
    Alone,
}

/// What block `id` can be combined with; `None` when it can't be combined. A ternary or
/// comprehension in a test or a header is part of that test or header, so it can't.
pub fn combinable(tree: &FileTree, id: u32) -> Option<Combinable> {
    if is_step(tree, id) {
        return tree.parent(id).map(|container| Combinable::Steps { container });
    }
    let in_expression = tree
        .parent(id)
        .is_some_and(|parent| matches!(tree.kind(parent), NodeKind::Call | NodeKind::Exit));
    (tree.kind(id).is_control() && in_expression).then_some(Combinable::Alone)
}

/// Where a proposed step goes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProposalSpot {
    /// After this step of a body.
    After(Key),
    /// At the start of the body of this definition or part.
    Start(Key),
}

/// Whether a proposed step can go after block `id`: whether it is a step of a body.
pub fn can_follow(tree: &FileTree, id: u32) -> bool {
    is_step(tree, id)
}

/// Whether a proposed step can go at the start of block `id`'s body.
pub fn can_start(tree: &FileTree, id: u32) -> bool {
    is_body(tree, id)
}

/// What opening a block in place shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Openable {
    /// A call to a project function or class: its callee's flow.
    Call {
        /// The callee.
        callee: Key,
    },
    /// A function written in a call's arguments: its own flow.
    Callback,
}

/// What block `id` opens in place; `None` when it can't open.
pub fn openable(tree: &FileTree, id: u32) -> Option<Openable> {
    match tree.kind(id) {
        NodeKind::Call => match &tree.calls_link(id)?.to {
            LinkTarget::Project(callee) => Some(Openable::Call {
                callee: callee.clone(),
            }),
            _ => None,
        },
        NodeKind::Function if is_callback(tree, id) => Some(Openable::Callback),
        _ => None,
    }
}

/// Whether function `id` is written in one of its parent call's arguments.
fn is_callback(tree: &FileTree, id: u32) -> bool {
    let range = tree.node(id).range;
    tree.parent(id)
        .filter(|&parent| tree.kind(parent) == NodeKind::Call)
        .is_some_and(|call| {
            tree.arguments(call)
                .iter()
                .any(|argument| argument.range.contains(range))
        })
}

/// Everything the overlay may do to one block, for the drawn flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allows {
    /// What it can be combined with.
    pub combine: Option<Combinable>,
    /// A proposed step can go after it.
    pub after: bool,
    /// A proposed step can go at the start of its body.
    pub start: bool,
    /// What it opens in place.
    pub open: Option<Openable>,
}

/// What the overlay may do to block `id`. Any block can be named, so naming isn't listed.
pub fn allows(tree: &FileTree, id: u32) -> Allows {
    Allows {
        combine: combinable(tree, id),
        after: can_follow(tree, id),
        start: can_start(tree, id),
        open: openable(tree, id),
    }
}

fn find<'a>(blocks: &'a impl Blocks, key: &Key) -> Result<Block<'a>, OverlayError> {
    blocks
        .block(key.as_str())
        .ok_or_else(|| OverlayError::Missing(key.clone()))
}

/// Checks one combined step's blocks: consecutive steps of one body in written order, or
/// exactly one whole control block.
pub fn check_combined(blocks: &impl Blocks, members: &[Key]) -> Result<(), OverlayError> {
    let Some(first_key) = members.first() else {
        return Err(OverlayError::NoMembers);
    };
    let first = find(blocks, first_key)?;
    let Some(combine) = combinable(first.tree, first.id) else {
        return Err(OverlayError::NotCombinable(first_key.clone()));
    };
    let Combinable::Steps { container } = combine else {
        return match members.get(1) {
            Some(second) => Err(OverlayError::NotCombinable(second.clone())),
            None => Ok(()),
        };
    };
    let steps: Vec<u32> = steps(first.tree, container).collect();
    let mut position = steps.iter().position(|&id| id == first.id);
    for (index, key) in members.iter().enumerate().skip(1) {
        let block = find(blocks, key)?;
        if members[..index].contains(key) {
            return Err(OverlayError::Repeated(key.clone()));
        }
        let same_body = std::ptr::eq(block.tree, first.tree)
            && is_step(block.tree, block.id)
            && block.tree.parent(block.id) == Some(container);
        if !same_body {
            return Err(OverlayError::OtherBody {
                key: key.clone(),
                first: first_key.clone(),
            });
        }
        let next = position.map(|position| position + 1);
        if steps.get(next.unwrap_or(usize::MAX)) != Some(&block.id) {
            return Err(OverlayError::NotConsecutive {
                key: key.clone(),
                previous: members[index - 1].clone(),
            });
        }
        position = next;
    }
    Ok(())
}

/// Checks that a page's combined steps never share a block and never hold one another's
/// blocks. Keys that name no block are left to [`check_combined`].
pub fn check_disjoint(blocks: &impl Blocks, combined: &[&[Key]]) -> Result<(), OverlayError> {
    let found: Vec<Vec<Block<'_>>> = combined
        .iter()
        .map(|members| {
            members
                .iter()
                .filter_map(|key| blocks.block(key.as_str()))
                .collect()
        })
        .collect();
    for (index, step) in found.iter().enumerate() {
        for other in &found[index + 1..] {
            for a in step {
                for b in other {
                    if a.contains(b) && b.contains(a) {
                        return Err(OverlayError::Overlaps(a.key().clone()));
                    }
                    if a.contains(b) {
                        return Err(OverlayError::Nests {
                            key: b.key().clone(),
                            outer: a.key().clone(),
                        });
                    }
                    if b.contains(a) {
                        return Err(OverlayError::Nests {
                            key: a.key().clone(),
                            outer: b.key().clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Checks where a proposed step goes: after a step of a body, or at the start of a
/// definition's or a part's body.
pub fn check_proposal(blocks: &impl Blocks, spot: &ProposalSpot) -> Result<(), OverlayError> {
    match spot {
        ProposalSpot::After(key) => {
            let block = find(blocks, key)?;
            can_follow(block.tree, block.id)
                .then_some(())
                .ok_or_else(|| OverlayError::NotAfter(key.clone()))
        }
        ProposalSpot::Start(key) => {
            let block = find(blocks, key)?;
            can_start(block.tree, block.id)
                .then_some(())
                .ok_or_else(|| OverlayError::NoStart(key.clone()))
        }
    }
}

/// Checks one open path of the card that draws definition `card`: 1 to [`MAX_OPEN_DEPTH`]
/// project calls or callbacks, outermost first. The first is drawn in the card (it belongs to
/// `card`), and each next one is drawn in what the one before it opens (it belongs to that
/// callee, or to that callback). The callees' files must be loaded.
pub fn check_open_path(blocks: &impl Blocks, card: &Key, path: &[Key]) -> Result<(), OverlayError> {
    if path.is_empty() || path.len() > MAX_OPEN_DEPTH {
        return Err(OverlayError::PathLength(path.len()));
    }
    let mut inside = card.clone();
    for key in path {
        let block = find(blocks, key)?;
        if block.owner().map(|owner| owner.key()) != Some(&inside) {
            return Err(OverlayError::NotInside {
                key: key.clone(),
                inside,
            });
        }
        inside = match openable(block.tree, block.id) {
            Some(Openable::Call { callee }) => callee,
            Some(Openable::Callback) => key.clone(),
            None => return Err(OverlayError::NotOpenable(key.clone())),
        };
    }
    Ok(())
}

/// Checks every open path of the card that draws definition `card`: at most
/// [`MAX_OPEN_PATHS`], each once, each valid ([`check_open_path`]).
pub fn check_open_paths<'a>(
    blocks: &impl Blocks,
    card: &Key,
    paths: impl IntoIterator<Item = &'a [Key]>,
) -> Result<(), OverlayError> {
    let paths: Vec<&[Key]> = paths.into_iter().collect();
    if paths.len() > MAX_OPEN_PATHS {
        return Err(OverlayError::TooManyPaths(paths.len()));
    }
    for (index, path) in paths.iter().enumerate() {
        check_open_path(blocks, card, path)?;
        if paths[..index].contains(path) {
            let last = path.last().cloned().unwrap_or_else(|| card.clone());
            return Err(OverlayError::RepeatedPath(last));
        }
    }
    Ok(())
}
