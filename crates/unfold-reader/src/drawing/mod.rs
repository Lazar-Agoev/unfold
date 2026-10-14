//! The one drawing function (`docs/designs/diagram-blocks.md`, "One drawing function"): it
//! turns a definition's tree and links, the page's overlay, the card's open paths, and Show all
//! calls into the drawn flow, the same way for every language. The canvas lays the flow out and
//! applies no rules of its own; agent text ([`flow_text`], [`code_text`]) prints the same flow
//! and tree.
//!
//! ```text
//! draw(trees, root, overlay, card)
//!   build:  the definition's body, step by step ─► Sequence of Steps (ids in preorder)
//!           presentation rules: boxes, thin rows, hidden builtins and empty control blocks,
//!           gap ticks, combined steps, proposals, insets, overlay actions
//!   routes: every way out of every step ─► Route list; collapsed combined steps take over the
//!           routes that leave them; a search from the start ─► each step's RouteState
//! ```
//!
//! **Steps** are what the canvas draws, in the order they run: call boxes, thin rows, frames
//! (decisions, loops, tries, and withs, whose parts are [`Lane`]s), exits, combined steps, and
//! proposed steps. A **gap tick** sits on the route into a step ([`Step::gap`]) or after a
//! sequence's last step ([`Sequence::end_gap`]). Every step and lane has an id, unique in its
//! flow; the start of the flow is id `0`. **Routes** go from an id to a step id, End, or the
//! caller, possibly through `finally` lanes. An inset (a call or callback opened in place) is
//! a drawn flow of its own, with its own ids.
//!
//! Without the verified source of a file (it changed since the compile, or is missing), its
//! steps have no texts (tests, headers, values, signatures, docs) and no gap ticks: lines can't
//! be told apart without the text.

mod build;
mod lines;
mod routes;
mod text;

#[cfg(test)]
mod examples;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use unfold_core::vocab::{CallRole, Form, NodeKind};
use unfold_core::{Key, LineSpan, RepoPath};

pub use text::{CodeLabel, CodeText, code_text, flow_text};

use crate::notes::LinkEnd;
use crate::overlay::ProposalSpot;
use crate::tree::{Blocks, Trees};

/// The page's overlay, as the drawing reads it. It applies page-wide, by key: inside insets too.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// Names, by block key: they replace the block's code name, which stays beside them.
    pub names: BTreeMap<Key, String>,
    /// Combined steps. The board's checks keep them from overlapping or nesting; one whose
    /// blocks aren't consecutive steps of a body drawn here is left out.
    pub combined: Vec<Combine>,
    /// Proposed steps.
    pub proposed: Vec<Proposal>,
}

/// One combined step of the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combine {
    /// Its id on the page.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its blocks: consecutive steps of one body, or one whole control block.
    pub members: Vec<Key>,
    /// Drawn as one step (`true`) or as the original steps under its title.
    pub collapsed: bool,
}

/// One proposed step of the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Its id on the page.
    pub id: String,
    /// Where it goes.
    pub spot: ProposalSpot,
    /// What it would do.
    pub title: String,
}

/// The card being drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The definition the card draws.
    pub definition: Key,
    /// The calls and callbacks opened in place: paths of 1 to 3 keys, outermost first.
    pub open: Vec<Vec<Key>>,
    /// Whether builtin calls are drawn.
    pub show_all_calls: bool,
}

/// Draws `card`. `trees` holds the card's file and the files of the callees it opens; source is
/// verified under `root`. `None` when the card's key names no loaded definition.
pub fn draw(trees: &Trees, root: &Path, overlay: &Overlay, card: &Card) -> Option<DrawnFlow> {
    let block = trees.block(card.definition.as_str())?;
    if !block.kind().is_definition() {
        return None;
    }
    let paths: Vec<&[Key]> = card.open.iter().map(Vec::as_slice).collect();
    let mut drawer = build::Drawer::new(trees, root, overlay, card.show_all_calls);
    Some(drawer.flow(block.tree, block.id, &paths))
}

/// A definition's drawn flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DrawnFlow {
    /// The card's header; the start of the flow (id `0`).
    pub header: Header,
    /// The steps of the definition's own body that can be combined, in written order (drawn or
    /// not), for [`CombineWith::Steps`].
    pub combinable: Vec<Key>,
    /// The definition's body.
    pub steps: Sequence,
    /// Every route between the flow's ids.
    pub routes: Vec<Route>,
}

/// A card's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// The definition.
    pub key: Key,
    /// Its kind.
    pub kind: NodeKind,
    /// Its form.
    pub form: Form,
    /// Its code name: a name, a module's path, `callback to <callee>`.
    pub label: String,
    /// The overlay's name for it, which titles the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// The signature as written, on one line.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub signature: Option<String>,
    /// The docstring or JSDoc, as the description from code.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    /// The definition's file.
    pub path: RepoPath,
    /// Its first line.
    pub line: u32,
    /// An `async` function.
    #[serde(rename = "async")]
    pub is_async: bool,
    /// A generator.
    pub generator: bool,
    /// What runs when it is defined, in the enclosing body: decorators, class bases, and
    /// Python defaults.
    pub defined_with: Vec<OtherCall>,
    /// What it imports (a module's imports, or a function's own).
    pub imports: Vec<Import>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// One import of a definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// The module as written.
    pub text: String,
    /// What it reaches.
    pub to: LinkEnd,
}

/// Steps in the order they run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Sequence {
    /// The steps.
    pub steps: Vec<Step>,
    /// The gap tick after the last step: code left out at the end.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub end_gap: Option<GapTick>,
}

/// One drawn step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Its id in the flow.
    pub id: u32,
    /// Whether a route reaches it.
    pub state: RouteState,
    /// The gap tick on the route into it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<GapTick>,
    /// What it is.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub shape: Shape,
}

/// What a step is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    /// A box: a project call, a named call, or a call that takes a callback.
    Call(CallBox),
    /// Consecutive library and unknown calls (and builtins with Show all calls) in one row.
    Thin(ThinRow),
    /// A decision: a diamond per test, its parts as lanes.
    Decision(Frame),
    /// A loop: a frame titled by its header, its body (and Python's `else`) as lanes.
    Loop(Frame),
    /// A try: TRY, its handlers as EXCEPT lanes, ELSE, and FINALLY.
    Try(Frame),
    /// A with block: a frame titled by its header.
    With(Frame),
    /// An exit, labeled by its value.
    Exit(ExitStep),
    /// A combined step.
    Combined(CombinedStep),
    /// A proposed step.
    Proposed(ProposedStep),
}

/// A call drawn as a box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallBox {
    /// The call.
    pub key: Key,
    /// The written callee.
    pub callee: String,
    /// The overlay's name, which replaces the callee (kept beside it).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// `call` or `new`.
    pub form: Form,
    /// Where the call goes.
    pub to: LinkEnd,
    /// The written target its value is assigned to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub binds: Option<String>,
    /// Written under `await` (AWAIT).
    pub awaited: bool,
    /// Runs only sometimes inside its expression (SOMETIMES, drawn dashed).
    pub sometimes: bool,
    /// A decorator, base, or default (in the body it runs in when its definition is defined).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub role: Option<CallRole>,
    /// The call it is written in, which it feeds: it is drawn before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub feeds: Option<CallRef>,
    /// The library and unknown calls written inside it.
    pub calls: Vec<OtherCall>,
    /// The functions passed to it, as rows under it.
    pub callbacks: Vec<Callback>,
    /// Its callee's flow, when it is opened in place.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inset: Option<Box<DrawnFlow>>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// A library, unknown, or (with Show all calls) builtin call: in a thin row, listed on a box,
/// or run when a definition is defined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OtherCall {
    /// The call.
    pub key: Key,
    /// The written callee.
    pub callee: String,
    /// `call` or `new`.
    pub form: Form,
    /// Where the call goes; an unknown one keeps its `?`.
    pub to: LinkEnd,
    /// The written target its value is assigned to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub binds: Option<String>,
    /// Written under `await`.
    pub awaited: bool,
    /// Runs only sometimes inside its expression.
    pub sometimes: bool,
    /// A decorator, base, or default.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub role: Option<CallRole>,
    /// The definition a decorator, base, or Python default belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub definition: Option<Key>,
    /// The call it is written in, which it feeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub feeds: Option<CallRef>,
    /// The library and unknown calls written in its arguments.
    pub inside: Vec<OtherCall>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// Consecutive library and unknown calls of one body, in one row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ThinRow {
    /// The calls, in the order they run.
    pub calls: Vec<OtherCall>,
    /// How many of them (and of the calls inside them) are unknown.
    pub unknown: u32,
}

/// A function passed to a call, drawn as a row under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Callback {
    /// The callback.
    pub key: Key,
    /// Its label: `callback to <callee>`.
    pub label: String,
    /// The overlay's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// Its signature as written, on one line.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub signature: Option<String>,
    /// Its flow, when it is opened in place.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inset: Option<Box<DrawnFlow>>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// A control block drawn as a frame. Its TypeScript name is `FlowFrame`: a board's `Frame` is
/// the other kind of frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FlowFrame"))]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// The control block.
    pub key: Key,
    /// Its form.
    pub form: Form,
    /// The overlay's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// Its header as written: a loop's or with's header, a `switch`'s or `match`'s subject.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub header: Option<String>,
    /// The project calls (and other boxes) written in the header, in the order they run.
    pub header_calls: Vec<CallBox>,
    /// What a ternary's or comprehension's value is assigned to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub binds: Option<String>,
    /// Its parts, in written order.
    pub lanes: Vec<Lane>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// One part of a control block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    /// Its id in the flow.
    pub id: u32,
    /// The part.
    pub key: Key,
    /// Its form: `then`, `elif`, `else`, `case`, `default`, `body`, `except`, `catch`,
    /// `finally`.
    pub form: Form,
    /// Whether a route reaches it.
    pub state: RouteState,
    /// The overlay's name: for a branch, a yes-or-no question.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// Its test, drawn as a diamond: a branch's condition, a case's pattern, a comprehension's
    /// filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub test: Option<Test>,
    /// A handler's clause as written: `except PaymentDeclined as error`, `catch (error)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub clause: Option<String>,
    /// The classes a handler's clause names.
    pub types: Vec<LinkEnd>,
    /// A `switch` case that runs into the next one.
    pub falls_through: bool,
    /// Its body's steps that can be combined, in written order (drawn or not).
    pub combinable: Vec<Key>,
    /// Its body.
    pub steps: Sequence,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// A part's test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Test {
    /// The test as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
    /// The project calls (and other boxes) written in it, in the order they run.
    pub calls: Vec<CallBox>,
}

/// An exit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ExitStep {
    /// The exit.
    pub key: Key,
    /// `return`, `raise`, `throw`, `break`, `continue`, or `yield`.
    pub form: Form,
    /// The overlay's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// What labels it: the call that is its value, or its value as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub value: Option<ExitValue>,
    /// The loop (or `switch`) a `break` or `continue` leaves.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub target: Option<ExitTarget>,
    /// What the overlay may do to it.
    pub actions: Actions,
}

/// What labels an exit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExitValue {
    /// The call that is exactly its value: `RETURN mark_failed ▸`, `RAISE ValueError`.
    Call(Box<CallBox>),
    /// Its value as written: `RETURN order`.
    Text {
        /// The value, on one line.
        text: String,
    },
}

/// The block a `break` or `continue` leaves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ExitTarget {
    /// The loop or `switch`.
    pub key: Key,
    /// Its form.
    pub form: Form,
}

/// A combined step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CombinedStep {
    /// Its id on the page.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its blocks.
    pub members: Vec<Key>,
    /// Drawn as one step.
    pub collapsed: bool,
    /// The lines it covers: its blocks' statements and the code between them.
    #[cfg_attr(feature = "ts", ts(type = "[number, number]"))]
    pub lines: LineSpan,
    /// How many tests its decisions hold.
    pub conditions: u32,
    /// How many loops it holds.
    pub loops: u32,
    /// The project calls inside, in the order they run.
    pub calls: Vec<CallRef>,
    /// How many library calls it holds.
    pub library: u32,
    /// How many unknown calls it holds.
    pub unknown: u32,
    /// The original steps, when it is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub steps: Option<Sequence>,
}

/// A call named by another step: a project call in a combined step, the call a call feeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallRef {
    /// The call.
    pub key: Key,
    /// The written callee.
    pub callee: String,
    /// The overlay's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
}

/// A proposed step, drawn dashed as PROPOSED.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProposedStep {
    /// Its id on the page.
    pub id: String,
    /// What it would do.
    pub title: String,
}

/// A `⋯ N lines` tick: code between two steps that no drawn step covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct GapTick {
    /// How many lines it counts (blank and comment lines aren't counted).
    pub count: u32,
    /// The first and last line it counts, which the code panel opens.
    #[cfg_attr(feature = "ts", ts(type = "[number, number]"))]
    pub lines: LineSpan,
    /// The control blocks with nothing drawn inside that start in it.
    pub hidden: Vec<HiddenBlock>,
}

/// A control block with nothing drawn inside, named in its gap tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct HiddenBlock {
    /// The control block.
    pub key: Key,
    /// Its form.
    pub form: Form,
    /// Its words in the tick: its code name (`for loop`, `if`), after the overlay's name when
    /// it has one.
    pub label: String,
}

/// One way from one step (or lane, or the start) to another.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Where it starts: a step, a lane, or the start (`0`).
    pub from: u32,
    /// What it is.
    pub kind: RouteKind,
    /// Where it goes.
    pub to: RouteEnd,
    /// The `finally` lanes it passes through first, innermost first.
    pub through: Vec<u32>,
    /// What a `break` or `continue` leaves: its loop's form, or `switch`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub leaves: Option<Form>,
}

/// What a route is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RouteKind {
    /// On to the next step.
    On,
    /// A test holds.
    Yes,
    /// A test doesn't hold.
    No,
    /// A `switch` or `match` goes to a case.
    Case,
    /// No case matches: to the default, or past the decision.
    Default,
    /// A loop goes into its body.
    Each,
    /// Back to the loop's header.
    Next,
    /// The loop is done.
    Done,
    /// The try body may raise into a handler (dashed: a possibility).
    OnError,
    /// Into a `finally` lane.
    Finally,
    /// A `return`, to End.
    Return,
    /// A `raise` or `throw`, to a handler or the caller.
    Raise,
    /// A `break`, to after its loop or `switch`.
    Break,
    /// A `continue`, to its loop's header.
    Continue,
    /// A truncated block may continue (dashed: uncertain).
    MayContinue,
}

impl RouteKind {
    /// Whether the route is ordinary flow rather than an exit's or an error's.
    pub(crate) const fn is_flow(self) -> bool {
        !matches!(
            self,
            Self::OnError | Self::Return | Self::Raise | Self::Break | Self::Continue | Self::MayContinue
        )
    }
}

/// Where a route goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RouteEnd {
    /// A step or lane of the flow.
    Step(u32),
    /// The end of the flow: a return.
    End,
    /// Raised out of the flow.
    Caller,
}

/// Whether a route reaches a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RouteState {
    /// A route reaches it.
    Reached,
    /// No route reaches it: it never runs (drawn dimmed).
    Unreachable,
    /// The front end couldn't place it: it may continue (dashed).
    Truncated,
}

/// What the overlay may do to a block. Any block can be named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Actions {
    /// What it can be combined with.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub combine: Option<CombineWith>,
    /// A proposed step can go after it.
    pub after: bool,
    /// A proposed step can go at the start of its body.
    pub start: bool,
    /// It opens in place.
    pub open: bool,
}

/// What a block can be combined with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "with", rename_all = "camelCase")]
pub enum CombineWith {
    /// Consecutive steps of this body (the flow's own, or a lane's `combinable`).
    Steps {
        /// The definition or part whose body it is a step of.
        body: Key,
    },
    /// Only by itself: a whole control block written in a call's arguments or an exit's value.
    Alone,
}
