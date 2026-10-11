//! Builds a [`FlowModel`] from one file's blocks, slots, and edges.
//!
//! Each written statement becomes a *fragment*: its lane items, its entry node, and the ports
//! that leave it (normal exits, returns, breaks, continues). A sequence connects each fragment's
//! exits to the next fragment's entry; statements after a terminal one stay visible but receive
//! no route. Opaque regions are terminal on purpose: an unknown cleanup or transfer rule must
//! never produce a confident route that skips `finally` or guesses a jump target.
//!
//! Node numbers follow creation order, which the proof of concept's Mermaid output also
//! follows: a sequence renders all of its statements before it connects them, a condition
//! precedes its branches, and a loop header follows its body.

use rustc_hash::FxHashMap;
use unfold_core::vocab::{BlockKind, Language, Resolution, Role, SlotName};
use unfold_core::{Block, FileGraph};

use super::names::written_names;
use super::{
    FlowCallback, FlowChip, FlowContext, FlowItem, FlowModel, FlowNode, FlowNodeKind, FlowRoute, FlowWarning,
    RouteKind, WarningCode,
};
use crate::catalog;
use crate::error::Result;

mod regions;
mod statement;
use crate::index::{file_owners, file_subtree_ends, is_type_only, to_u32};
use crate::notes::note;
use crate::source::SourceState;

/// Statements nested deeper than this become one opaque node, so deeply nested code cannot
/// exhaust the stack.
const MAX_DEPTH: usize = 100;

/// The longest fallback name for a call node, in UTF-16 code units.
const CALL_LABEL_LENGTH: usize = 48;

/// A route waiting for its destination.
#[derive(Debug, Clone, Copy)]
struct Port {
    node: u32,
    kind: RouteKind,
}

/// What one statement (or a run of them) contributes.
#[derive(Debug)]
struct Fragment {
    items: Vec<FlowItem>,
    entry: Option<u32>,
    exits: Vec<Port>,
    returns: Vec<u32>,
    breaks: Vec<Port>,
    continues: Vec<Port>,
    ends: bool,
    lanes: u32,
}

impl Fragment {
    fn empty() -> Self {
        Self {
            items: Vec::new(),
            entry: None,
            exits: Vec::new(),
            returns: Vec::new(),
            breaks: Vec::new(),
            continues: Vec::new(),
            ends: false,
            lanes: 1,
        }
    }

    /// Takes over `part`'s pending returns and transfers.
    fn absorb(&mut self, part: &mut Fragment) {
        self.returns.append(&mut part.returns);
        self.breaks.append(&mut part.breaks);
        self.continues.append(&mut part.continues);
    }
}

/// The optional parts of a node.
#[derive(Debug, Default)]
struct Extra {
    chips: Vec<usize>,
    call: Option<u32>,
    callbacks: Vec<FlowCallback>,
    warning: Option<WarningCode>,
    /// A fixed label that authored labels never replace (Start).
    fixed: bool,
}

struct Builder<'a> {
    file: &'a FileGraph,
    text: &'a str,
    language: Language,
    context: &'a dyn FlowContext,
    ends: Vec<u32>,
    children: FxHashMap<u32, Vec<u32>>,
    keys: FxHashMap<&'a str, u32>,
    /// The edges the card's owner owns inside the card, as indices into `file.edges`.
    chips: Vec<usize>,
    nodes: Vec<FlowNode>,
    routes: Vec<FlowRoute>,
    warnings: Vec<WarningCode>,
    depth: usize,
}

/// Builds the Flow of block `root` of `file`. `text` is the verified source, or `None` when
/// the source is not current (`state` says why).
pub(super) fn build(
    file: &FileGraph,
    root: u32,
    text: Option<&str>,
    state: SourceState,
    context: &dyn FlowContext,
) -> Result<FlowModel> {
    let ends = file_subtree_ends(&file.blocks);
    let mut children: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for local in root + 1..ends[root as usize] {
        let block = &file.blocks[local as usize];
        // Type declarations are not drawn: they are not statements of the path.
        if let Some(parent) = block.parent.filter(|_| !is_type_only(block.kind)) {
            children.entry(parent).or_default().push(local);
        }
    }
    let owners = file_owners(&file.blocks);
    let root_block = &file.blocks[root as usize];
    let owner = if root_block.kind.is_function_like()
        || matches!(root_block.kind, BlockKind::Class | BlockKind::File)
    {
        root
    } else {
        owners[root as usize]
    };
    let subtree = root..ends[root as usize];
    let chips = file
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            subtree.contains(&edge.source) && owners.get(edge.source as usize) == Some(&owner)
        })
        .map(|(index, _)| index)
        .collect();
    let mut builder = Builder {
        file,
        text: text.unwrap_or(""),
        language: file.file.language,
        context,
        ends,
        children,
        keys: file
            .blocks
            .iter()
            .map(|block| (block.key.as_str(), block.local_id))
            .collect(),
        chips,
        nodes: Vec::new(),
        routes: Vec::new(),
        warnings: Vec::new(),
        depth: 0,
    };
    Ok(builder.finish(root, text.is_some(), state))
}

impl<'a> Builder<'a> {
    fn block(&self, local: u32) -> &'a Block {
        &self.file.blocks[local as usize]
    }

    fn children(&self, local: u32) -> &[u32] {
        self.children.get(&local).map_or(&[], Vec::as_slice)
    }

    fn children_with(&self, local: u32, role: Role) -> Vec<u32> {
        self.children(local)
            .iter()
            .copied()
            .filter(|&child| self.block(child).role == Some(role))
            .collect()
    }

    fn slots(&self, local: u32, slot: SlotName) -> impl Iterator<Item = &'a unfold_core::Slot> + 'a {
        let slots = &self.file.slots;
        let start = slots.partition_point(|item| item.block < local);
        slots[start..]
            .iter()
            .take_while(move |item| item.block == local)
            .filter(move |item| item.slot == slot)
    }

    fn has_slot(&self, local: u32, slot: SlotName) -> bool {
        self.slots(local, slot).next().is_some()
    }

    /// The verified text of every `slot` of block `local`, joined with `, `.
    fn slot_text(&self, local: u32, slot: SlotName) -> String {
        let parts: Vec<&str> = self
            .slots(local, slot)
            .map(|item| self.text.get(item.range.as_usize()).unwrap_or(""))
            .collect();
        parts.join(", ")
    }

    fn block_text(&self, local: u32) -> &'a str {
        self.text.get(self.block(local).range.as_usize()).unwrap_or("")
    }

    fn names(&self, text: &str, limit: usize) -> Vec<String> {
        written_names(self.language, text, limit)
    }

    fn warn(&mut self, code: WarningCode) {
        if !self.warnings.contains(&code) {
            self.warnings.push(code);
        }
    }

    fn route(&mut self, from: u32, to: u32, kind: RouteKind) {
        self.routes.push(FlowRoute { from, to, kind });
    }

    /// The qualified name of a resolved target: computed here for the same file, otherwise
    /// asked of the context.
    fn target_name(&self, key: &str) -> Option<String> {
        match self.keys.get(key) {
            Some(&local) => catalog::qualified_name(self.file, local),
            None => self.context.target_name(key),
        }
    }

    fn chip(&self, index: usize) -> FlowChip {
        let edge = &self.file.edges[index];
        let source = self.block(edge.source);
        let name = edge
            .target
            .as_ref()
            .filter(|_| edge.resolution == Resolution::Resolved)
            .and_then(|target| self.target_name(target.as_str()))
            .unwrap_or_else(|| edge.text.to_string());
        FlowChip {
            source: source.key.clone(),
            seq: edge.seq,
            kind: edge.kind,
            resolution: edge.resolution,
            target: edge.target.clone(),
            name,
            note: note(edge.resolution, edge.reason, edge.module.as_ref()),
            line: source.lines.start,
        }
    }

    /// The card's chips inside block `local`; with `header_only`, without those inside its
    /// body, branch, handler, or cleanup slots.
    fn chips_for(&self, local: u32, header_only: bool) -> Vec<usize> {
        let bodies: Vec<_> = if header_only {
            let slots = &self.file.slots;
            let start = slots.partition_point(|slot| slot.block < local);
            slots[start..]
                .iter()
                .take_while(|slot| slot.block == local)
                .filter(|slot| {
                    matches!(
                        slot.slot,
                        SlotName::Body
                            | SlotName::Then
                            | SlotName::Else
                            | SlotName::Catch
                            | SlotName::Finally
                    )
                })
                .map(|slot| slot.range)
                .collect()
        } else {
            Vec::new()
        };
        // The card's chips follow the file's edge order (by source block), so the ones inside
        // `local`'s subtree are one contiguous run.
        let source = |index: &usize| self.file.edges[*index].source;
        let start = self.chips.partition_point(|index| source(index) < local);
        let end = self
            .chips
            .partition_point(|index| source(index) < self.ends[local as usize]);
        self.chips[start..end.max(start)]
            .iter()
            .copied()
            .filter(|index| {
                let range = self.block(source(index)).range;
                !bodies.iter().any(|body| body.contains(range))
            })
            .collect()
    }

    /// Adds a node and returns its one-node fragment.
    fn node(&mut self, local: Option<u32>, kind: FlowNodeKind, fallback: String, extra: Extra) -> Fragment {
        let block = local.map(|local| self.block(local));
        let authored = block
            .filter(|_| !extra.fixed)
            .and_then(|block| self.context.label(block.key.as_str()))
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let (label, is_fallback) = match authored {
            Some(text) => (text.to_owned(), false),
            None => (fallback, !extra.fixed && kind != FlowNodeKind::End),
        };
        let id = to_u32(self.nodes.len());
        let chips = extra.chips.iter().map(|&index| self.chip(index)).collect();
        self.nodes.push(FlowNode {
            id,
            key: block.map(|block| block.key.clone()),
            kind,
            block_kind: block.map(|block| block.kind),
            label,
            fallback: is_fallback,
            start_line: block.map(|block| block.lines.start),
            end_line: block.map(|block| block.lines.end),
            lane: 0,
            chips,
            call: extra.call,
            checks: None,
            outcome: None,
            callbacks: extra.callbacks,
            warning: extra.warning,
        });
        let ends = kind.is_terminal();
        Fragment {
            items: vec![FlowItem::Node {
                node: id,
                ends,
                lanes: 1,
            }],
            entry: Some(id),
            exits: if ends {
                Vec::new()
            } else {
                vec![Port {
                    node: id,
                    kind: RouteKind::Sequence,
                }]
            },
            ends,
            ..Fragment::empty()
        }
    }

    /// Renders every block first, then connects them in written order.
    fn sequence(&mut self, blocks: &[u32]) -> Fragment {
        let parts: Vec<Fragment> = blocks.iter().map(|&local| self.render(local)).collect();
        let mut result = Fragment::empty();
        let mut previous: Vec<Port> = Vec::new();
        for mut part in parts {
            if let Some(entry) = part.entry {
                for port in &previous {
                    self.route(port.node, entry, port.kind);
                }
                result.entry.get_or_insert(entry);
            }
            result.items.append(&mut part.items);
            result.absorb(&mut part);
            result.lanes = result.lanes.max(part.lanes);
            previous = if result.ends { Vec::new() } else { part.exits };
            result.ends |= part.ends;
        }
        result.exits = previous;
        result
    }

    fn render(&mut self, local: u32) -> Fragment {
        if self.depth >= MAX_DEPTH {
            let kind = self.block(local).kind.as_str().to_owned();
            return self.opaque(local, kind, WarningCode::TooDeep);
        }
        self.depth += 1;
        let part = self.render_block(local);
        self.depth -= 1;
        part
    }

    fn render_block(&mut self, local: u32) -> Fragment {
        let kind = self.block(local).kind;
        let python = self.language == Language::Python;
        if python {
            let opaque = match kind {
                BlockKind::With => Some(("with", WarningCode::PythonWith)),
                BlockKind::Match => Some(("match", WarningCode::PythonMatch)),
                BlockKind::Case => Some(("case", WarningCode::PythonCase)),
                BlockKind::Catch => Some(("except", WarningCode::PythonExcept)),
                _ => None,
            };
            if let Some((label, code)) = opaque {
                return self.opaque(local, label.to_owned(), code);
            }
        }
        let code = match kind {
            BlockKind::Switch => Some(WarningCode::Switch),
            BlockKind::Case => Some(WarningCode::Case),
            BlockKind::Default => Some(WarningCode::Default),
            BlockKind::Catch => Some(WarningCode::Catch),
            _ => None,
        };
        if let Some(code) = code {
            let names = self.names(&self.slot_text(local, SlotName::Expression), 4);
            let word = if kind == BlockKind::Catch {
                "catch"
            } else {
                "switch"
            };
            let label = if names.is_empty() {
                word.to_owned()
            } else {
                format!("{word} {}", names.join(", "))
            };
            return self.opaque(local, label, code);
        }
        match kind {
            BlockKind::Try => self.attempt(local),
            BlockKind::If => self.condition(local),
            BlockKind::For | BlockKind::ForIn | BlockKind::ForOf | BlockKind::While | BlockKind::DoWhile => {
                self.loop_region(local)
            }
            BlockKind::Break | BlockKind::Continue => self.transfer(local),
            kind if kind.is_function_like() || kind == BlockKind::Class => {
                let name = catalog::qualified_name(self.file, local).unwrap_or_default();
                self.node(Some(local), FlowNodeKind::Definition, name, Extra::default())
            }
            _ => self.statement(local),
        }
    }

    fn opaque(&mut self, local: u32, fallback: String, code: WarningCode) -> Fragment {
        self.warn(code);
        let chips = self.chips_for(local, false);
        self.node(
            Some(local),
            FlowNodeKind::Opaque,
            fallback,
            Extra {
                chips,
                warning: Some(code),
                ..Extra::default()
            },
        )
    }

    /// Sets every node's lane from the lane structure.
    fn assign(&mut self, items: &[FlowItem], lane: u32) {
        for item in items {
            match item {
                FlowItem::Node { node, .. } => self.nodes[*node as usize].lane = lane,
                FlowItem::Branch {
                    node,
                    yes,
                    no,
                    yes_lane,
                    ..
                } => {
                    self.nodes[*node as usize].lane = lane;
                    self.assign(no, lane);
                    self.assign(yes, lane + yes_lane);
                }
                FlowItem::Loop { node, body, .. } => {
                    self.nodes[*node as usize].lane = lane;
                    self.assign(body, lane);
                }
                FlowItem::Try {
                    body,
                    handler,
                    catch_lane,
                    ..
                } => {
                    self.assign(body, lane);
                    self.assign(handler, lane + catch_lane);
                }
            }
        }
    }

    fn finish(&mut self, root: u32, current: bool, state: SourceState) -> FlowModel {
        let block = self.block(root);
        let name = block
            .name
            .as_deref()
            .map(str::to_owned)
            .or_else(|| catalog::qualified_name(self.file, root))
            .unwrap_or_default();
        let start = self
            .node(
                Some(root),
                FlowNodeKind::Start,
                name,
                Extra {
                    fixed: true,
                    ..Extra::default()
                },
            )
            .entry
            .unwrap_or_default();
        let mut model = FlowModel {
            key: block.key.clone(),
            path: self.file.file.path.clone(),
            language: self.language,
            kind: block.kind,
            source: state,
            nodes: Vec::new(),
            routes: Vec::new(),
            tree: Vec::new(),
            start,
            end: None,
            lanes: 1,
            warnings: Vec::new(),
        };
        let unavailable = if !current {
            Some(if state == SourceState::Changed {
                (
                    WarningCode::SourceChanged,
                    "Source changed. Compile to see paths from written structure.",
                )
            } else {
                (
                    WarningCode::SourceMissing,
                    "Source missing. Compile to see paths from written structure.",
                )
            })
        } else if block.kind == BlockKind::File {
            Some((
                WarningCode::FileDeferred,
                "File initialization is not drawn. Open Code to read this file.",
            ))
        } else {
            None
        };
        if let Some((code, message)) = unavailable {
            // One explanatory node without a block stands in for the body.
            self.warn(code);
            let mut part = self.node(
                None,
                FlowNodeKind::Opaque,
                message.to_owned(),
                Extra {
                    warning: Some(code),
                    fixed: true,
                    ..Extra::default()
                },
            );
            model.tree.append(&mut part.items);
        } else if block.kind == BlockKind::Class {
            let members: Vec<u32> = self
                .children(root)
                .iter()
                .copied()
                .filter(|&member| self.block(member).kind.is_function_like())
                .collect();
            for member in members {
                let name = catalog::qualified_name(self.file, member).unwrap_or_default();
                let mut part = self.node(Some(member), FlowNodeKind::Definition, name, Extra::default());
                model.tree.append(&mut part.items);
            }
            self.warn(WarningCode::ClassMembers);
        } else {
            let mut body = if block.kind.is_function_like() {
                self.sequence(&self.children_with(root, Role::Body))
            } else {
                self.render(root)
            };
            let terminal = match body.entry {
                Some(entry) => {
                    self.route(start, entry, RouteKind::Sequence);
                    std::mem::take(&mut body.exits)
                }
                None => vec![Port {
                    node: start,
                    kind: RouteKind::Sequence,
                }],
            };
            if !terminal.is_empty() || !body.returns.is_empty() {
                let end = self
                    .node(None, FlowNodeKind::End, "End".to_owned(), Extra::default())
                    .entry
                    .unwrap_or_default();
                for port in terminal {
                    self.route(port.node, end, port.kind);
                }
                for returned in std::mem::take(&mut body.returns) {
                    self.route(returned, end, RouteKind::Return);
                }
                model.end = Some(end);
            }
            if !body.breaks.is_empty() || !body.continues.is_empty() {
                self.warn(WarningCode::OrphanTransfer);
            }
            self.assign(&body.items, 0);
            model.tree = body.items;
            model.lanes = body.lanes;
        }
        model.nodes = std::mem::take(&mut self.nodes);
        model.routes = std::mem::take(&mut self.routes);
        model.warnings = self
            .warnings
            .iter()
            .map(|&code| FlowWarning::from(code))
            .collect();
        model
    }
}
