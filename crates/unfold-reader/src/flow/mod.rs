//! The Flow model (docs/reader.md#flow-view): **paths from written structure**.
//!
//! Flow is derived from saved blocks, roles, slots, and source order only, never by re-parsing
//! the source. It is not a runtime trace: short-circuit expressions stay as written, labeled
//! transfers get no guessed destination, and exceptions and asynchronous behavior are not
//! modeled. Verified source is used for two things: the fallback names of undescribed nodes
//! ([`names`]), and each node's [`SourceFocus`]: its recorded range as lines and UTF-16 columns.
//!
//! | Saved structure | Drawn as |
//! | --- | --- |
//! | Callable body | Start, the written statements, and End where a route reaches it |
//! | Statement, variable, call, construction | One node with its call chips; a main call makes it a `call` node |
//! | `if` and `else if` | A condition with yes and no lanes; a chain of three or more checks carries `checks` |
//! | Loop | A header and body with each, next, and done routes; a do-while tests after its body |
//! | `return`, `throw` | A terminal node; a return connects to End |
//! | Unlabeled `break` or `continue` | A transfer to the enclosing loop |
//! | TypeScript `try`/`catch` without `finally` | Separate regions with an on-error route |
//! | Nested callback or class | A separate definition, listed on its statement's node |
//! | TypeScript `switch` and `try`/`finally`; Python `with`, `match`, loops with `else`, and every `try` | An opaque node with a warning and no outgoing route |
//!
//! The model has two faces: `nodes` and `routes` are the semantic graph that tests read; `tree`
//! is the lane structure the UI lays out top to bottom (a main lane and at most one side lane per
//! condition or catch). Rust computes structure; the UI computes geometry.

mod build;
pub(crate) mod names;

use std::path::Path;

use serde::Serialize;
use unfold_core::vocab::{BlockKind, EdgeKind, Language, Resolution};
use unfold_core::{FileGraph, Key, RepoPath};

use crate::error::{ReaderError, Result};
use crate::source::{SourceState, Verified, verify_file};

/// What the Flow builder needs from outside the file: the board's authored labels and the
/// names of resolved targets in other files.
pub trait FlowContext {
    /// The authored label of block `key`, if the board has one.
    fn label(&self, key: &str) -> Option<&str> {
        let _ = key;
        None
    }

    /// The qualified name of the resolved target `key` when it lies in another file.
    fn target_name(&self, key: &str) -> Option<String> {
        let _ = key;
        None
    }
}

/// A context with no labels and no names from other files.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoContext;

impl FlowContext for NoContext {}

/// A node's presentation type, derived from the compiled structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum FlowNodeKind {
    /// The card's entry.
    Start,
    /// Where routes end.
    End,
    /// A variable without a main call.
    Variable,
    /// A statement whose main expression is a call or construction.
    Call,
    /// Any other statement.
    Statement,
    /// An `if` condition.
    Condition,
    /// A loop header.
    Loop,
    /// A `return`.
    Return,
    /// A `throw`.
    Throw,
    /// An unlabeled `break`.
    Break,
    /// An unlabeled `continue`.
    Continue,
    /// A nested definition shown as its own card.
    Definition,
    /// A region whose routing is not modeled.
    Opaque,
}

impl FlowNodeKind {
    /// Whether nothing routes out of a node of this kind to the next statement.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Return | Self::Throw | Self::Break | Self::Continue | Self::Opaque
        )
    }
}

/// What a route means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RouteKind {
    /// Written order, unlabeled.
    Sequence,
    /// A condition holds.
    Yes,
    /// A condition does not hold.
    No,
    /// A condition does not hold and an `else if` follows.
    ElseIf,
    /// Into a loop body.
    Each,
    /// Into a do-while body again.
    Again,
    /// From the end of a loop body back to its header.
    Next,
    /// From the end of a do-while body to its test.
    Test,
    /// A `continue` back to its loop.
    Continue,
    /// Out of a loop.
    Done,
    /// A `break` out of its loop.
    Break,
    /// A `return` to End.
    Return,
    /// From a `try` region into its `catch` (drawn dashed).
    Error,
}

impl RouteKind {
    /// The route's label as drawn (empty for written order).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sequence => "",
            Self::Yes => "yes",
            Self::No => "no",
            Self::ElseIf => "no · else if",
            Self::Each => "each",
            Self::Again => "again",
            Self::Next => "next",
            Self::Test => "test",
            Self::Continue => "continue",
            Self::Done => "done",
            Self::Break => "break",
            Self::Return => "return",
            Self::Error => "on error",
        }
    }

    /// Whether the route is drawn dashed.
    pub const fn dashed(self) -> bool {
        matches!(self, Self::Error)
    }
}

/// Why something in a Flow is not drawn, or drawn only in part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum WarningCode {
    /// The source changed since the facts were taken.
    SourceChanged,
    /// The source is missing or unreadable.
    SourceMissing,
    /// A file root: file initialization is not drawn.
    FileDeferred,
    /// A class root: members are separate definitions.
    ClassMembers,
    /// A TypeScript `switch`.
    Switch,
    /// A TypeScript `case` shown on its own.
    Case,
    /// A TypeScript `default` shown on its own.
    Default,
    /// A TypeScript `catch` shown on its own.
    Catch,
    /// A TypeScript `try` with `finally` (or without a handler).
    TryFinally,
    /// An empty `try` body, which has no route into its `catch`.
    EmptyTry,
    /// A Python loop with `else`.
    PythonLoopElse,
    /// A Python `try`.
    PythonTry,
    /// A Python `with`.
    PythonWith,
    /// A Python `match`.
    PythonMatch,
    /// A Python `case` shown on its own.
    PythonCase,
    /// A Python `except` shown on its own.
    PythonExcept,
    /// A labeled `break`.
    LabeledBreak,
    /// A labeled `continue`.
    LabeledContinue,
    /// A `break` or `continue` with no enclosing loop in the card.
    OrphanTransfer,
    /// Structure nested too deeply to draw.
    TooDeep,
}

impl WarningCode {
    /// The warning as a sentence.
    pub const fn message(self) -> &'static str {
        match self {
            Self::SourceChanged => {
                "Source changed; recorded source ranges are withheld until the next compile."
            }
            Self::SourceMissing => "Source missing; recorded source ranges are withheld.",
            Self::FileDeferred => "File initialization Flow is not drawn. Open Code to read this file.",
            Self::ClassMembers => "Class members are separate definitions; no execution sequence is implied.",
            Self::Switch => {
                "switch routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::Case => {
                "case routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::Default => {
                "default routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::Catch => {
                "catch routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::TryFinally => {
                "try/finally routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::EmptyTry => "An empty try body has no route into its catch.",
            Self::PythonLoopElse => {
                "Python loop/else routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::PythonTry => {
                "Python exception matching, except*, else and finally routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::PythonWith => {
                "Python with routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::PythonMatch => {
                "Python match routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::PythonCase => {
                "Python case routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::PythonExcept => {
                "Python except routing is not modeled. Open Code to read it; no execution route leaves this region."
            }
            Self::LabeledBreak => "Labeled break routing is not modeled.",
            Self::LabeledContinue => "Labeled continue routing is not modeled.",
            Self::OrphanTransfer => {
                "A break or continue has no supported enclosing loop; its route is withheld."
            }
            Self::TooDeep => "Structure nested this deeply is not drawn. Open Code to read it.",
        }
    }
}

/// A warning with its sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowWarning {
    /// What it is about.
    pub code: WarningCode,
    /// The sentence the UI shows.
    pub message: &'static str,
}

impl From<WarningCode> for FlowWarning {
    fn from(code: WarningCode) -> Self {
        Self {
            code,
            message: code.message(),
        }
    }
}

/// The common outcome of every check of a folded `else if` chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum ChainOutcome {
    /// Every check's branch ends in a `throw`.
    Throw,
    /// Every check's branch ends in a `return`.
    Return,
    /// The branches differ.
    Mixed,
}

/// One occurrence a node shows (a call, construction, import, or passed reference).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowChip {
    /// The occurrence block; with `seq` it identifies the edge.
    pub source: Key,
    /// The edge's position among the occurrence's edges.
    pub seq: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// How it resolved.
    pub resolution: Resolution,
    /// The target, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub target: Option<Key>,
    /// The target's qualified name when resolved, otherwise the written callee.
    pub name: String,
    /// The module or reason as display text.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// The line the occurrence starts on.
    pub line: u32,
}

/// A nested definition listed on a node; it opens as its own card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowCallback {
    /// The definition.
    pub key: Key,
    /// Its catalog label (`callback to …`, a name, `class`).
    pub label: String,
    /// How many lines it spans.
    pub lines: u32,
    /// Whether it contains a `throw` outside further nested callables.
    pub throws: bool,
}

/// Where a node's own source is: its condition, its main call expression, or else its statement,
/// from recorded ranges. Lines are one-based; columns are UTF-16 code units from the line's start;
/// the end is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SourceFocus {
    /// The line it starts on.
    pub start_line: u32,
    /// Where it starts on that line.
    pub start_column: u32,
    /// The line it ends on.
    pub end_line: u32,
    /// Where it ends on that line (exclusive).
    pub end_column: u32,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowNode {
    /// The node's index in [`FlowModel::nodes`].
    pub id: u32,
    /// The block it shows; absent for End.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<Key>,
    /// The presentation type.
    pub kind: FlowNodeKind,
    /// The compiled block kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub block_kind: Option<BlockKind>,
    /// The authored label, or a fallback name from the code.
    pub label: String,
    /// Whether `label` is a fallback (drawn with a dotted underline).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub fallback: bool,
    /// The first line of the block.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub start_line: Option<u32>,
    /// The last line of the block.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub end_line: Option<u32>,
    /// Where the node's own source is: a condition's recorded condition; a loop's recorded
    /// condition, else its header (from its start to its body); a `call` node's main call
    /// expression; any other node's whole block. Absent for Start and End, and whenever the
    /// source is not current (recorded ranges are withheld).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub focus: Option<SourceFocus>,
    /// The lane: 0 is the main lane.
    pub lane: u32,
    /// The occurrences this node shows, in source order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub chips: Vec<FlowChip>,
    /// The index in `chips` of the main call that makes this a `call` node.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub call: Option<u32>,
    /// For a condition heading an `else if` chain of three or more checks: their number.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub checks: Option<u32>,
    /// For such a chain: what every check's branch ends in.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub outcome: Option<ChainOutcome>,
    /// Nested definitions written in this statement.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub callbacks: Vec<FlowCallback>,
    /// For an opaque node: why it is not drawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub warning: Option<WarningCode>,
}

/// A route between two nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowRoute {
    /// The node it leaves.
    pub from: u32,
    /// The node it enters.
    pub to: u32,
    /// What it means.
    pub kind: RouteKind,
}

/// The lane structure, top to bottom. Every item knows whether it ends (no route continues
/// below it) and how many lanes it is wide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FlowItem {
    /// One node.
    Node {
        /// The node.
        node: u32,
        /// Whether it is terminal.
        ends: bool,
        /// Always 1.
        lanes: u32,
    },
    /// A condition with its branches: `no` continues in the item's lane; `yes` takes the lane
    /// `yes_lane` to the right (or the main lane, below, when `yes` ends and `no` exists).
    Branch {
        /// The condition node.
        node: u32,
        /// The `then` statements.
        yes: Vec<FlowItem>,
        /// The `else` statements (one nested `Branch` for an `else if`).
        no: Vec<FlowItem>,
        /// Whether the yes lane ends.
        yes_ends: bool,
        /// Whether the no lane ends.
        no_ends: bool,
        /// Whether both end.
        ends: bool,
        /// The width in lanes.
        lanes: u32,
        /// The yes lane's offset from the item's lane.
        yes_lane: u32,
        /// Whether `no` is an `else if`.
        else_if: bool,
    },
    /// A loop region.
    Loop {
        /// The header node.
        node: u32,
        /// The body statements.
        body: Vec<FlowItem>,
        /// Whether the test follows the body (do-while).
        test_after: bool,
        /// Always false: a loop's done route continues.
        ends: bool,
        /// The width in lanes.
        lanes: u32,
    },
    /// A `try` region beside its `catch` region.
    Try {
        /// The `try` block.
        key: Key,
        /// The `catch` block.
        catch_key: Key,
        /// The name the handler binds, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        binding: Option<String>,
        /// The `try` body.
        body: Vec<FlowItem>,
        /// The handler body.
        handler: Vec<FlowItem>,
        /// Whether the body ends.
        body_ends: bool,
        /// Whether the handler ends.
        handler_ends: bool,
        /// Whether both end.
        ends: bool,
        /// The width in lanes.
        lanes: u32,
        /// The handler lane's offset from the item's lane.
        catch_lane: u32,
    },
}

impl FlowItem {
    /// Whether no route continues below the item.
    pub fn ends(&self) -> bool {
        match self {
            Self::Node { ends, .. }
            | Self::Branch { ends, .. }
            | Self::Loop { ends, .. }
            | Self::Try { ends, .. } => *ends,
        }
    }

    /// The item's width in lanes.
    pub fn lanes(&self) -> u32 {
        match self {
            Self::Node { lanes, .. }
            | Self::Branch { lanes, .. }
            | Self::Loop { lanes, .. }
            | Self::Try { lanes, .. } => *lanes,
        }
    }
}

/// The Flow of one block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FlowModel {
    /// The shown block.
    pub key: Key,
    /// Its file.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The block's kind.
    pub kind: BlockKind,
    /// The source's freshness; only current source is drawn beyond Start.
    pub source: SourceState,
    /// Every node; `id` is the index.
    pub nodes: Vec<FlowNode>,
    /// Every route, in the order they were derived.
    pub routes: Vec<FlowRoute>,
    /// The lane structure between Start and End.
    pub tree: Vec<FlowItem>,
    /// The Start node.
    pub start: u32,
    /// The End node, when a route reaches it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub end: Option<u32>,
    /// The diagram's width in lanes.
    pub lanes: u32,
    /// Warnings, without duplicates.
    pub warnings: Vec<FlowWarning>,
}

impl FlowModel {
    /// The nodes still showing a fallback name where an authored label could go (every node
    /// with a block except Start).
    pub fn unlabeled(&self) -> impl Iterator<Item = &FlowNode> {
        self.nodes
            .iter()
            .filter(|node| node.fallback && node.key.is_some() && node.kind != FlowNodeKind::Start)
    }
}

/// The Flow of block `local_id` of `file`, reading and verifying its source under `root`.
pub fn flow_view(
    root: &Path,
    file: &FileGraph,
    local_id: u32,
    context: &dyn FlowContext,
) -> Result<FlowModel> {
    let verified = verify_file(root, &file.file.path, file.file.hash);
    let text = match &verified {
        Verified::Current(snapshot) => Some(snapshot.text()),
        Verified::Changed | Verified::Missing => None,
    };
    build_flow(file, local_id, text, verified.state(), context)
}

/// The Flow of block `local_id` of `file`, given the file's verified text (`None` when it did
/// not verify, with `state` saying why). Callers must have verified `text` against the facts'
/// hash ([`verify_file`]).
pub fn build_flow(
    file: &FileGraph,
    local_id: u32,
    text: Option<&str>,
    state: SourceState,
    context: &dyn FlowContext,
) -> Result<FlowModel> {
    if local_id as usize >= file.blocks.len() {
        return Err(ReaderError::BlockNotFound {
            path: file.file.path.as_str().into(),
            local_id,
        });
    }
    build::build(
        file,
        local_id,
        text.filter(|_| state == SourceState::Current),
        state,
        context,
    )
}

#[cfg(test)]
mod tests;
