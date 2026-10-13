//! Relations of one definition (docs/reader.md#derived-views): its callers, its outgoing occurrences, and
//! the entrances it is reached from.
//!
//! - **Callers** reverse resolved edges and map each occurrence to its owner.
//! - **Outgoing** occurrences are the edges the definition owns, in source order, grouped as
//!   already on the board, resolved and openable, or external or unresolved. Ownership never
//!   crosses files, so this needs only the definition's own file.
//! - **Reached from** lists the non-process entrances whose activation closures contain the
//!   definition; [`crate::ReaderIndex`] computes it with cached closures.

use serde::Serialize;
use unfold_core::vocab::{EdgeKind, EntranceKind, Reason, Resolution};
use unfold_core::{Edge, FileGraph, Key, ModuleLabel, RepoPath};

use crate::index::{BlockId, GraphIndex, file_owners};
use crate::notes::note;

/// One resolved edge into a definition, seen from the definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    /// The definition (or file) that owns the occurrence.
    pub owner: Key,
    /// The occurrence block.
    pub source: Key,
    /// The occurrence's file.
    pub path: RepoPath,
    /// The edge's position among the occurrence's edges.
    pub seq: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// The argument position of a passed reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub position: Option<u32>,
}

/// The callers of `id`, in graph order (path, then occurrence, then sequence).
pub(crate) fn callers(index: &GraphIndex, id: BlockId) -> Vec<Caller> {
    index
        .incoming(id)
        .map(|edge| Caller {
            owner: index.key(index.owner(edge.source)).clone(),
            source: index.key(edge.source).clone(),
            path: index.path(edge.source).clone(),
            seq: edge.seq,
            kind: edge.kind,
            position: edge.position,
        })
        .collect()
}

/// One edge a definition owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingCall {
    /// The occurrence block.
    pub source: Key,
    /// The edge's position among the occurrence's edges (0 is the occurrence itself).
    pub seq: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// How it resolved.
    pub resolution: Resolution,
    /// The target, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub target: Option<Key>,
    /// The written callee, argument, or specifier.
    pub text: Box<str>,
    /// Why it is unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reason: Option<Reason>,
    /// Where an external target comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub module: Option<ModuleLabel>,
    /// The module or reason as display text ([`crate::notes`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// The argument position of a passed reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub position: Option<u32>,
    /// The line the occurrence starts on.
    pub line: u32,
}

/// A definition's outgoing edges, grouped for the inspector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Outgoing {
    /// Resolved edges whose target is already on the board.
    pub on_board: Vec<OutgoingCall>,
    /// Resolved edges whose target can be opened.
    pub openable: Vec<OutgoingCall>,
    /// External and unresolved edges, with their notes.
    pub external_or_unresolved: Vec<OutgoingCall>,
}

/// The edges owned by block `definition` of `file`, in source order. Empty when the block does
/// not exist.
fn owned_edges(file: &FileGraph, definition: u32) -> Vec<&Edge> {
    if definition as usize >= file.blocks.len() {
        return Vec::new();
    }
    let owners = file_owners(&file.blocks);
    file.edges
        .iter()
        .filter(|edge| owners.get(edge.source as usize) == Some(&definition))
        .collect()
}

/// The outgoing edges of block `definition` of `file`, grouped; `on_board` tells whether a
/// target key is already on the board. `None` when the block does not exist.
pub fn outgoing(file: &FileGraph, definition: u32, on_board: impl Fn(&str) -> bool) -> Option<Outgoing> {
    if definition as usize >= file.blocks.len() {
        return None;
    }
    let mut grouped = Outgoing::default();
    for edge in owned_edges(file, definition) {
        let call = OutgoingCall {
            source: file.blocks[edge.source as usize].key.clone(),
            seq: edge.seq,
            kind: edge.kind,
            resolution: edge.resolution,
            target: edge.target.clone(),
            text: edge.text.clone(),
            reason: edge.reason,
            module: edge.module.clone(),
            note: note(edge.resolution, edge.reason, edge.module.as_ref()),
            position: edge.position,
            line: file.blocks[edge.source as usize].lines.start,
        };
        match &call.target {
            Some(target) if edge.resolution == Resolution::Resolved && on_board(target.as_str()) => {
                grouped.on_board.push(call);
            }
            Some(_) if edge.resolution == Resolution::Resolved => grouped.openable.push(call),
            _ => grouped.external_or_unresolved.push(call),
        }
    }
    Some(grouped)
}

/// The non-process entrances whose closure (given per entrance by `reaches`) contains `id`, as
/// entrance keys in canonical order.
pub(crate) fn reached_from(
    index: &GraphIndex,
    id: BlockId,
    mut reaches: impl FnMut(usize, BlockId) -> bool,
) -> Vec<Key> {
    index
        .entrances()
        .iter()
        .enumerate()
        .filter(|(_, entrance)| entrance.kind != EntranceKind::Process)
        .filter(|(position, _)| reaches(*position, id))
        .map(|(_, entrance)| entrance.key.clone())
        .collect()
}
