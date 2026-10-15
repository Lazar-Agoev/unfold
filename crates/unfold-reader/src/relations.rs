//! A definition's relations: who calls it, what it calls, and (through
//! [`crate::ReaderIndex::reached_from`]) which entrances reach it.
//!
//! - **Callers** are the `calls` and `passes` links whose project target is the definition,
//!   read a page at a time from the store's reverse index, each with the call it starts at, the
//!   definition that call belongs to, and the cursor the callers after it start from.
//! - **Outgoing links** are the `calls` and `passes` links of the call nodes that belong to the
//!   definition, in source order, whatever they reach. Ownership never crosses files, so this
//!   needs only the definition's own file.

use serde::Serialize;
use unfold_core::vocab::{LinkKind, NodeKind};
use unfold_core::{Key, RepoPath};
use unfold_store::{Cursor, Reader};

use crate::ReaderIndex;
use crate::error::Result;
use crate::notes::LinkEnd;
use crate::tree::FileTree;

/// One call of a definition, seen from the definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    /// The definition the calling block belongs to.
    pub owner: Key,
    /// The call.
    pub call: Key,
    /// The call's file.
    pub path: RepoPath,
    /// The call's first line.
    pub line: u32,
    /// `calls`, or `passes` when the call passes the definition as a value.
    pub kind: LinkKind,
    /// Where the callers after this one start: the `after` of the next page.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub cursor: Cursor,
}

/// One page of a definition's callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Callers {
    /// How many calls there are in all.
    pub total: u64,
    /// This page, in path order and then source order.
    pub callers: Vec<Caller>,
}

/// One page of the callers of the block with `key`, after `after` (a caller's `cursor`, such
/// as the previous page's last), at most `limit`. `index` must be of the same graph version as
/// `reader`'s snapshot.
pub fn callers(
    reader: &Reader,
    index: &ReaderIndex,
    key: &str,
    after: Option<&Cursor>,
    limit: usize,
) -> Result<Callers> {
    reader
        .snapshot(|reader| {
            let page = reader.callers(key, after, limit)?;
            let total = reader.caller_count(key)?;
            Ok((page, total))
        })
        .map(|(page, total)| Callers {
            total,
            callers: page
                .items
                .into_iter()
                .filter_map(|incoming| {
                    // A call always belongs to a definition; the index and the store hold the same
                    // graph, so every calling block is in both.
                    Some(Caller {
                        owner: index.owner(incoming.from.as_str())?.clone(),
                        line: index.line(incoming.from.as_str())?,
                        cursor: incoming.cursor(),
                        call: incoming.from,
                        path: incoming.path,
                        kind: incoming.link.kind,
                    })
                })
                .collect(),
        })
        .map_err(Into::into)
}

/// One `calls` or `passes` link of a call that belongs to a definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingLink {
    /// The call.
    pub call: Key,
    /// The call's first line.
    pub line: u32,
    /// `calls` (what the call runs) or `passes` (a definition it passes as a value).
    pub kind: LinkKind,
    /// How the callee or passed name is written.
    pub text: Box<str>,
    /// Where it goes.
    pub to: LinkEnd,
}

/// The outgoing links of definition `definition` of `tree`, in source order: each call's
/// `calls` link, then its `passes` links.
pub fn outgoing(tree: &FileTree, definition: u32) -> Vec<OutgoingLink> {
    tree.owned(definition)
        .filter(|&id| tree.kind(id) == NodeKind::Call)
        .flat_map(|id| {
            tree.links(id)
                .iter()
                .filter(|link| matches!(link.kind, LinkKind::Calls | LinkKind::Passes))
                .map(move |link| OutgoingLink {
                    call: tree.key(id).clone(),
                    line: tree.node(id).lines.start,
                    kind: link.kind,
                    text: link.text.clone(),
                    to: LinkEnd::of(&link.to),
                })
        })
        .collect()
}
