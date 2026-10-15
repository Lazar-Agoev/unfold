//! Building a drawn flow's steps from a definition's tree, with the presentation rules
//! (docs/reader.md#what-is-drawn):
//!
//! - **Calls.** A project call, a named call, and a call that takes a callback are boxes. The
//!   boxes and expression control blocks written inside a call come first, in the order they
//!   run, each marked with the call it feeds. The other calls inside a drawn call are listed on
//!   it. Library and unknown calls are thin items, and consecutive thin items of one body fold
//!   into one row unless a gap tick sits between them. Builtins are hidden unless Show all calls
//!   is on; the calls inside a hidden call surface on their own.
//! - **Control blocks** are frames whose parts are lanes. One with nothing drawn inside is
//!   hidden and named in its gap tick, unless the overlay puts something in it. The boxes
//!   written in a header, a test, or a handler's clause are drawn on it, and the unknown calls
//!   outside them are listed after it with their `?` (its library and builtin calls stay its
//!   written text).
//! - **Exits** are labeled by the call that is their value, or by their value as written.
//! - **Gap ticks** count the code lines of a body that no drawn step covers.
//! - **The overlay:** names; combined steps (one step when collapsed, their steps under a title
//!   when open); proposed steps after a step or at the start of a body; calls and callbacks
//!   opened in place, drawn as insets; and what each block allows ([`overlay::allows`]): a
//!   block a combined step holds can't be combined again, and a call opens in place only within
//!   [`MAX_OPEN_DEPTH`] levels and [`MAX_OPEN_PATHS`] paths (closing an open one always can).

use std::rc::Rc;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{CallRole, Flag, Form, LinkKind, NodeKind, Outcome, Reason};
use unfold_core::{ByteRange, Key, LineSpan, LinkTarget, Node, RepoPath};

use super::lines::{FileText, candidates, doc_text, file_text, one_line};
use super::{
    Actions, CallBox, CallRef, Callback, Combine, CombineWith, CombinedStep, DrawnFlow, ExitStep, ExitTarget,
    ExitValue, FlowState, Frame, GapTick, Header, HeldCall, HiddenBlock, Import, Lane, OtherCall, Overlay,
    ProposedStep, RouteState, Sequence, Shape, Step, Test, TextSources, ThinRow, routes,
};
use crate::code_view;
use crate::notes::LinkEnd;
use crate::overlay::{self, Combinable, MAX_OPEN_DEPTH, MAX_OPEN_PATHS, Openable, ProposalSpot};
use crate::tree::{Blocks, FileTree, Trees};

/// What every flow of one drawing shares: the graph, the overlay, and the files' texts.
pub(super) struct Drawer<'a> {
    trees: &'a Trees,
    sources: &'a TextSources<'a>,
    overlay: &'a Overlay,
    show_all: bool,
    /// How many paths the card opens.
    paths: usize,
    /// Each file's state and text, read once.
    texts: FxHashMap<RepoPath, (FlowState, Option<Rc<FileText>>)>,
    /// The overlay's combined steps, by their first block.
    groups: FxHashMap<&'a Key, &'a Combine>,
}

impl<'a> Drawer<'a> {
    pub(super) fn new(
        trees: &'a Trees,
        sources: &'a TextSources<'a>,
        overlay: &'a Overlay,
        show_all: bool,
        paths: usize,
    ) -> Self {
        let groups = overlay
            .combined
            .iter()
            .filter_map(|combine| combine.members.first().map(|first| (first, combine)))
            .collect();
        Self {
            trees,
            sources,
            overlay,
            show_all,
            paths,
            texts: FxHashMap::default(),
            groups,
        }
    }

    /// The state of `tree`'s file and the facts' text ([`file_text`]), read once.
    fn text(&mut self, tree: &FileTree) -> (FlowState, Option<Rc<FileText>>) {
        let sources = self.sources;
        self.texts
            .entry(tree.path().clone())
            .or_insert_with(|| {
                let (state, text) = file_text(sources, tree);
                (state, text.map(Rc::new))
            })
            .clone()
    }

    /// The flow of definition `definition`, `depth` insets deep (the card's own is `0`),
    /// opening `open` (paths below this flow) in place.
    pub(super) fn flow(
        &mut self,
        tree: &'a FileTree,
        definition: u32,
        depth: usize,
        open: &[&'a [Key]],
    ) -> DrawnFlow {
        let (state, text) = self.text(tree);
        let touched = self.touched(tree, definition);
        let mut builder = Builder {
            drawer: self,
            tree,
            definition,
            depth,
            state,
            text,
            open,
            touched,
            next: 1,
            deferred: None,
            combining: false,
        };
        let header = builder.header();
        // A type has no body: its members are typed records, shown by its card.
        let (combinable, steps) = if tree.kind(definition) == NodeKind::Type {
            (Vec::new(), Sequence::default())
        } else {
            (builder.combinable(definition), builder.body(definition))
        };
        let mut flow = DrawnFlow {
            header,
            combinable,
            steps,
            routes: Vec::new(),
        };
        routes::route(&mut flow, tree);
        flow
    }

    /// The blocks of `definition` (in `tree`) that the overlay puts something on or in: those it
    /// names, those its proposals follow or start, and the first blocks of its combined steps.
    fn touched(&self, tree: &FileTree, definition: u32) -> Vec<u32> {
        let overlay = self.overlay;
        let proposals = overlay.proposed.iter().map(|proposal| match &proposal.spot {
            ProposalSpot::After(key) | ProposalSpot::Start(key) => key,
        });
        let combined = overlay
            .combined
            .iter()
            .filter_map(|combine| combine.members.first());
        overlay
            .names
            .keys()
            .chain(proposals)
            .chain(combined)
            .filter_map(|key| tree.id(key.as_str()))
            .filter(|&id| tree.owner(id) == Some(definition))
            .collect()
    }
}

/// A step waiting for its gap tick, with the lines it covers.
struct Item {
    step: Step,
    span: Option<LineSpan>,
    /// A proposed step's place, which covers no lines: the last line of the block it goes after.
    follows: Option<u32>,
}

impl Item {
    /// The last line this item comes after or covers: the code no item covers up to it is the
    /// gap tick on the route into it (when no item before reaches that far).
    fn reach(&self) -> Option<u32> {
        self.span.map(|span| span.end).or(self.follows)
    }
}

/// A hidden control block and its first line.
struct Hidden {
    line: u32,
    block: HiddenBlock,
}

/// Builds one flow.
struct Builder<'d, 'a> {
    drawer: &'d mut Drawer<'a>,
    tree: &'a FileTree,
    definition: u32,
    /// How many insets deep the flow is: the card's own is `0`.
    depth: usize,
    /// Whether the file is the one the facts came from.
    state: FlowState,
    /// The facts' text.
    text: Option<Rc<FileText>>,
    open: &'d [&'a [Key]],
    /// The blocks the overlay puts something on or in ([`Drawer::touched`]).
    touched: Vec<u32>,
    /// The next step or lane id; `0` is the start.
    next: u32,
    /// While a collapsed combined step is built: the proposals inside it, drawn after it.
    deferred: Option<Vec<Item>>,
    /// While a combined step's blocks are built: they can't be combined again.
    combining: bool,
}

impl<'a> Builder<'_, 'a> {
    fn id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn step(&mut self, shape: Shape) -> Step {
        Step {
            id: self.id(),
            state: RouteState::Reached,
            gap: None,
            shape,
        }
    }

    fn node(&self, id: u32) -> &'a Node {
        self.tree.node(id)
    }

    fn key(&self, id: u32) -> Key {
        self.tree.key(id).clone()
    }

    fn kind(&self, id: u32) -> NodeKind {
        self.tree.kind(id)
    }

    fn children(&self, id: u32) -> &'a [u32] {
        self.tree.children(id)
    }

    fn flag(&self, id: u32, flag: Flag) -> bool {
        self.node(id).flags.contains(flag)
    }

    fn name(&self, id: u32) -> Option<String> {
        self.drawer.overlay.names.get(self.tree.key(id)).cloned()
    }

    fn one_line(&self, range: Option<ByteRange>) -> Option<String> {
        self.text.as_ref()?.one_line(range?)
    }

    /// Definition `id`'s signature row: its head after the name the card's title says, on one
    /// line (`def place_order(order_id)` → `(order_id)`, `class Labels(Named)` → `(Named)`); the
    /// whole head when it doesn't write the name (an inline function's); none when nothing
    /// follows the name (`class Order`).
    fn signature(&self, id: u32) -> Option<String> {
        let node = self.node(id);
        let head = self.text.as_ref()?.slice(node.head?)?;
        let rest = node
            .name
            .as_deref()
            .and_then(|name| after_name(head, name))
            .unwrap_or(head);
        let line = one_line(rest);
        (!line.is_empty()).then_some(line)
    }

    /// The lines node `id`'s statement (or, when it begins none, the node) covers.
    fn span(&self, id: u32) -> Option<LineSpan> {
        let node = self.node(id);
        Some(self.text.as_ref()?.lines(node.stmt.unwrap_or(node.range)))
    }

    /// The lines of the statement `call` is written in ([`FileTree::statement`]); the call's
    /// own lines when the file's text can't be read.
    fn statement_lines(&self, call: u32) -> LineSpan {
        match &self.text {
            Some(text) => text.lines(self.tree.statement(call)),
            None => self.node(call).lines,
        }
    }

    fn actions(&self, id: u32) -> Actions {
        let allows = overlay::allows(self.tree, id);
        let room = self.depth < MAX_OPEN_DEPTH && self.drawer.paths < MAX_OPEN_PATHS;
        Actions {
            combine: allows
                .combine
                .filter(|_| !self.combining)
                .map(|combine| match combine {
                    Combinable::Steps { container } => CombineWith::Steps {
                        body: self.key(container),
                    },
                    Combinable::Alone => CombineWith::Alone,
                }),
            after: allows.after,
            start: allows.start,
            open: allows.open.is_some() && (room || self.opened(self.tree.key(id)).is_some()),
        }
    }

    /// Where call `id` goes. Validation gives every call a `calls` link.
    fn target(&self, id: u32) -> LinkTarget {
        self.tree
            .calls_link(id)
            .map_or(LinkTarget::Unknown(Reason::NotFound), |link| link.to.clone())
    }

    fn outcome(&self, id: u32) -> Outcome {
        self.tree
            .calls_link(id)
            .map_or(Outcome::Unknown, |link| link.to.outcome())
    }

    fn is_test(&self, id: u32) -> bool {
        self.kind(id) == NodeKind::Call && self.node(id).role == Some(CallRole::Test)
    }

    /// Whether block `id` belongs to its part's test: a test call, or a ternary or
    /// comprehension written in the part's head.
    fn belongs_to_test(&self, id: u32) -> bool {
        self.is_test(id) || overlay::in_test(self.tree, id)
    }

    fn callbacks_of(&self, call: u32) -> impl Iterator<Item = u32> + 'a {
        let tree = self.tree;
        tree.children(call)
            .iter()
            .copied()
            .filter(move |&child| overlay::openable(tree, child) == Some(Openable::Callback))
    }

    /// Whether call `id` is drawn as a box: a project call, a named call, or a call that takes a
    /// callback.
    fn is_box(&self, id: u32) -> bool {
        self.outcome(id) == Outcome::Project
            || self.drawer.overlay.names.contains_key(self.tree.key(id))
            || self.callbacks_of(id).next().is_some()
    }

    /// Whether call `id` is drawn when it isn't a box: anything but a builtin, unless Show all
    /// calls is on.
    fn shown(&self, id: u32) -> bool {
        self.outcome(id) != Outcome::Builtin || self.drawer.show_all
    }

    /// Whether block `id`, written in a body, draws anything.
    fn draws(&self, id: u32) -> bool {
        if self.flag(id, Flag::Truncated) {
            return true;
        }
        match self.kind(id) {
            NodeKind::Exit => true,
            NodeKind::Call if self.is_test(id) => self.has_chip(id),
            NodeKind::Call => {
                self.is_box(id)
                    || self.shown(id)
                    || self.children(id).iter().any(|&child| {
                        let kind = self.kind(child);
                        (kind == NodeKind::Call || kind.is_control()) && self.draws(child)
                    })
            }
            NodeKind::Part => self.children(id).iter().any(|&child| {
                if self.belongs_to_test(child) {
                    self.has_chip(child)
                } else {
                    self.draws(child)
                }
            }),
            kind if kind.is_control() => {
                self.touched(id)
                    || self.children(id).iter().any(|&child| match self.kind(child) {
                        NodeKind::Part => self.draws(child),
                        _ => self.has_chip(child),
                    })
            }
            _ => false,
        }
    }

    /// Whether the overlay puts something inside control block `id` (a proposal, a name, or a
    /// combined step on a block written in it), which then draws it as a frame. A name on the
    /// block itself is its gap tick's.
    fn touched(&self, id: u32) -> bool {
        self.touched
            .iter()
            .any(|&touched| touched != id && self.tree.contains(id, touched))
    }

    /// Whether block `id`, written in a test or a header, draws there: whether it is or holds a
    /// box or an unknown call (outside the definitions written in it).
    fn has_chip(&self, id: u32) -> bool {
        match self.kind(id) {
            kind if kind.is_definition() => false,
            NodeKind::Call if self.is_box(id) || self.outcome(id) == Outcome::Unknown => true,
            _ => self.children(id).iter().any(|&child| self.has_chip(child)),
        }
    }

    fn header(&mut self) -> Header {
        let id = self.definition;
        let node = self.node(id);
        let defined_with = match node.parent {
            Some(parent) => self
                .children(parent)
                .iter()
                .filter(|&&child| self.node(child).r#for == Some(id))
                .map(|&child| self.other_call(child, None))
                .collect(),
            None => Vec::new(),
        };
        let imports = self
            .tree
            .links(id)
            .iter()
            .filter(|link| link.kind == LinkKind::Imports)
            .map(|link| Import {
                text: link.text.to_string(),
                to: LinkEnd::of(&link.to),
            })
            .collect();
        let description = self
            .text
            .as_ref()
            .zip(node.doc)
            .and_then(|(text, doc)| doc_text(text.slice(doc)?));
        Header {
            key: self.key(id),
            kind: node.kind(),
            form: node.form,
            label: self.tree.label(id),
            name: self.name(id),
            signature: (node.kind() != NodeKind::Module)
                .then(|| self.signature(id))
                .flatten(),
            description,
            path: self.tree.path().clone(),
            state: self.state,
            line: node.lines.start,
            is_async: node.flags.contains(Flag::Async),
            generator: node.flags.contains(Flag::Generator),
            defined_with,
            imports,
            actions: self.actions(id),
            language: self.tree.language(),
        }
    }

    fn combinable(&self, container: u32) -> Vec<Key> {
        overlay::steps(self.tree, container)
            .map(|id| self.key(id))
            .collect()
    }

    /// The body of a definition or part.
    fn body(&mut self, container: u32) -> Sequence {
        let steps: Vec<u32> = overlay::steps(self.tree, container).collect();
        let lines = match &self.text {
            Some(text) => candidates(self.tree, text, self.definition, container),
            None => Vec::new(),
        };
        let start = ProposalSpot::Start(self.key(container));
        self.sequence(Some(&start), &steps, &lines, true)
    }

    /// The steps of `steps` (consecutive steps of one body), with the proposals at `start`
    /// first; `lines` are the code lines its gap ticks count. With `groups`, the combined steps
    /// among them are drawn as such (a combined step's own steps are drawn without).
    fn sequence(
        &mut self,
        start: Option<&ProposalSpot>,
        steps: &[u32],
        lines: &[u32],
        groups: bool,
    ) -> Sequence {
        let mut items = Vec::new();
        let mut hidden = Vec::new();
        if let Some(start) = start {
            self.proposals(start, None, &mut items);
        }
        let mut index = 0;
        while index < steps.len() {
            if let Some((group, covers)) = groups.then(|| self.group_at(&steps[index..])).flatten() {
                let members = &steps[index..index + covers];
                let (item, after) = self.group(group, members, lines);
                items.push(item);
                items.extend(after);
                index += members.len();
                continue;
            }
            let id = steps[index];
            self.step_items(id, &mut items, &mut hidden);
            let follows = self.span(id).map(|span| span.end);
            self.proposals(&ProposalSpot::After(self.key(id)), follows, &mut items);
            index += 1;
        }
        finish(items, &hidden, lines)
    }

    /// The overlay's proposals at `spot`, as steps after line `follows` (none at the start of a
    /// body).
    fn proposals(&mut self, spot: &ProposalSpot, follows: Option<u32>, items: &mut Vec<Item>) {
        let overlay = self.drawer.overlay;
        for proposal in overlay.proposed.iter().filter(|proposal| &proposal.spot == spot) {
            let step = self.step(Shape::Proposed(ProposedStep {
                item: proposal.id.clone(),
                title: proposal.title.clone(),
            }));
            let item = Item {
                step,
                span: None,
                follows,
            };
            match self.deferred.as_mut() {
                Some(deferred) => deferred.push(item),
                None => items.push(item),
            }
        }
    }

    /// The combined step whose blocks cover the first steps of `steps`, if there is one, and how
    /// many it covers: steps written between its blocks since it was combined join it
    /// ([`overlay::covered`]).
    fn group_at(&self, steps: &[u32]) -> Option<(&'a Combine, usize)> {
        let first = self.tree.key(*steps.first()?);
        let group = *self.drawer.groups.get(first)?;
        let covered = overlay::covered(self.tree, &group.members).ok()?;
        let matches = covered.len() <= steps.len()
            && covered
                .iter()
                .zip(steps)
                .all(|(block, &id)| block == self.tree.key(id));
        matches.then_some((group, covered.len()))
    }

    /// Combined step `group` over `members`, and the proposals a collapsed one draws after it.
    fn group(&mut self, group: &Combine, members: &[u32], lines: &[u32]) -> (Item, Vec<Item>) {
        let id = self.id();
        let first = members.first().copied().unwrap_or(self.definition);
        let last = members.last().copied().unwrap_or(first);
        let span = match (self.span(first), self.span(last)) {
            (Some(first), Some(last)) => LineSpan {
                start: first.start,
                end: last.end,
            },
            _ => LineSpan {
                start: self.node(first).lines.start,
                end: self.node(last).lines.end,
            },
        };
        let inside: Vec<u32> = lines
            .iter()
            .copied()
            .filter(|line| (span.start..=span.end).contains(line))
            .collect();
        let outer = group.collapsed.then(|| self.deferred.replace(Vec::new()));
        let combining = std::mem::replace(&mut self.combining, true);
        let steps = self.sequence(None, members, &inside, false);
        self.combining = combining;
        let after = match outer {
            Some(outer) => std::mem::replace(&mut self.deferred, outer).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut summary = Summary::default();
        summary.sequence(&steps);
        let (library, unknown) = self.uncertain(members);
        let holds = if group.collapsed {
            self.held(members)
        } else {
            Vec::new()
        };
        let step = Step {
            id,
            state: RouteState::Reached,
            gap: None,
            shape: Shape::Combined(CombinedStep {
                item: group.id.clone(),
                title: group.title.clone(),
                members: group.members.clone(),
                collapsed: group.collapsed,
                holds,
                lines: span,
                conditions: summary.conditions,
                loops: summary.loops,
                calls: summary.calls,
                library,
                unknown,
                steps: Some(steps),
            }),
        };
        (
            Item {
                step,
                span: Some(span),
                follows: None,
            },
            after,
        )
    }

    /// Every block `members` are or hold, outside the definitions written in them.
    fn held(&self, members: &[u32]) -> Vec<Key> {
        members
            .iter()
            .flat_map(|&member| self.tree.subtree(member))
            .filter(|&id| self.tree.owner(id) == Some(self.definition))
            .map(|id| self.key(id))
            .collect()
    }

    /// How many library and unknown calls `members` hold (not counting callbacks' own).
    fn uncertain(&self, members: &[u32]) -> (u32, u32) {
        let mut counts = (0, 0);
        for &member in members {
            for id in self.tree.subtree(member) {
                if self.kind(id) != NodeKind::Call || self.tree.owner(id) != Some(self.definition) {
                    continue;
                }
                match self.outcome(id) {
                    Outcome::Library => counts.0 += 1,
                    Outcome::Unknown => counts.1 += 1,
                    Outcome::Project | Outcome::Builtin => {}
                }
            }
        }
        counts
    }

    /// The steps body step `id` draws.
    fn step_items(&mut self, id: u32, items: &mut Vec<Item>, hidden: &mut Vec<Hidden>) {
        let span = self.span(id);
        let steps = match self.kind(id) {
            NodeKind::Call => self.flatten(id, None, true),
            NodeKind::Exit => self.exit(id),
            kind if kind.is_control() && self.draws(id) => vec![self.frame(id)],
            kind if kind.is_control() => {
                if let Some(span) = span {
                    let form = self.node(id).form;
                    let label = match self.name(id) {
                        Some(name) => format!("{name} · {}", hidden_label(form)),
                        None => hidden_label(form).to_owned(),
                    };
                    hidden.push(Hidden {
                        line: span.start,
                        block: HiddenBlock {
                            key: self.key(id),
                            form,
                            label,
                        },
                    });
                }
                Vec::new()
            }
            _ => Vec::new(),
        };
        items.extend(steps.into_iter().map(|step| Item {
            step,
            span,
            follows: None,
        }));
    }

    /// Call `call` and what is written inside it, in the order they run: the boxes and
    /// expression control blocks inside, then the call itself when `emit` (a box, or a thin item
    /// when shown). The other calls inside a drawn call are listed on it; inside a hidden one,
    /// they surface on their own.
    fn flatten(&mut self, call: u32, feeds: Option<CallRef>, emit: bool) -> Vec<Step> {
        let boxed = self.is_box(call);
        let lists = !emit || boxed || self.shown(call);
        let mut steps = Vec::new();
        for &child in self.children(call) {
            match self.kind(child) {
                NodeKind::Call => {
                    let surface = self.is_box(child) || !lists;
                    let fed = self.call_ref(call);
                    steps.extend(self.flatten(child, Some(fed), surface));
                }
                kind if kind.is_control() && self.draws(child) => steps.extend(self.expression(child)),
                _ => {}
            }
        }
        if emit {
            if boxed {
                let call_box = self.call_box(call, feeds);
                steps.push(self.step(Shape::Call(call_box)));
            } else if self.shown(call) {
                let other = self.other_call(call, feeds);
                let unknown = unknowns(&other);
                let lines = self.statement_lines(call);
                steps.push(self.step(Shape::Thin(ThinRow {
                    calls: vec![other],
                    lines,
                    unknown,
                })));
            }
        }
        steps
    }

    /// An expression control block (a ternary or comprehension in a call or exit), or the
    /// combined step that holds it alone.
    fn expression(&mut self, control: u32) -> Vec<Step> {
        match self.group_at(&[control]) {
            Some((group, _)) => {
                let (item, after) = self.group(group, &[control], &[]);
                std::iter::once(item).chain(after).map(|item| item.step).collect()
            }
            None => vec![self.frame(control)],
        }
    }

    /// Exit `exit`, after the boxes and expression control blocks written in its value.
    fn exit(&mut self, exit: u32) -> Vec<Step> {
        let mut steps = Vec::new();
        let mut value = None;
        for &child in self.children(exit) {
            match self.kind(child) {
                NodeKind::Call if self.flag(child, Flag::ExitValue) => {
                    steps.extend(self.flatten(child, None, false));
                    value = Some(ExitValue::Call(Box::new(self.call_box(child, None))));
                }
                NodeKind::Call => steps.extend(self.flatten(child, None, true)),
                kind if kind.is_control() && self.draws(child) => steps.extend(self.expression(child)),
                _ => {}
            }
        }
        let node = self.node(exit);
        let value = value.or_else(|| self.one_line(node.value).map(|text| ExitValue::Text { text }));
        let target = node.target.map(|target| ExitTarget {
            key: self.key(target),
            form: self.node(target).form,
        });
        let step = self.step(Shape::Exit(ExitStep {
            key: self.key(exit),
            form: node.form,
            name: self.name(exit),
            value,
            target,
            actions: self.actions(exit),
        }));
        steps.push(step);
        steps
    }

    /// Control block `control` as a frame.
    fn frame(&mut self, control: u32) -> Step {
        let id = self.id();
        let node = self.node(control);
        let mut header_calls = Vec::new();
        let mut header_unknown = Vec::new();
        let mut lanes = Vec::new();
        for &child in self.children(control) {
            match self.kind(child) {
                NodeKind::Part => lanes.push(self.lane(child)),
                kind if kind.is_definition() => {}
                _ => self.chips(child, false, &mut header_calls, &mut header_unknown),
            }
        }
        let frame = Frame {
            key: self.key(control),
            form: node.form,
            name: self.name(control),
            header: self.one_line(node.head),
            header_calls,
            header_unknown,
            binds: node.binds.as_deref().map(str::to_owned),
            lanes,
            actions: self.actions(control),
        };
        let shape = match node.kind() {
            NodeKind::Decision => Shape::Decision(frame),
            NodeKind::Loop => Shape::Loop(frame),
            NodeKind::Try => Shape::Try(frame),
            _ => Shape::With(frame),
        };
        Step {
            id,
            state: RouteState::Reached,
            gap: None,
            shape,
        }
    }

    /// Part `part` as a lane.
    fn lane(&mut self, part: u32) -> Lane {
        let id = self.id();
        let node = self.node(part);
        let test = node.head.map(|head| {
            let mut calls = Vec::new();
            let mut unknown = Vec::new();
            for &child in self.children(part) {
                if self.belongs_to_test(child) {
                    self.chips(child, false, &mut calls, &mut unknown);
                }
            }
            Test {
                text: self.one_line(Some(head)),
                calls,
                unknown,
            }
        });
        let types = self
            .tree
            .links(part)
            .iter()
            .filter(|link| link.kind == LinkKind::Types)
            .map(|link| LinkEnd::of(&link.to))
            .collect();
        let text = self.text.as_ref().map(|text| text.text());
        let lines = match node.parent.filter(|_| node.form == Form::Body) {
            Some(control) => LineSpan {
                start: self.node(control).lines.start,
                end: node.lines.end,
            },
            None => node.lines,
        };
        Lane {
            id,
            key: self.key(part),
            form: node.form,
            label: code_view::label(self.tree, part, text),
            lines,
            state: RouteState::Reached,
            name: self.name(part),
            test,
            types,
            falls_through: node.flags.contains(Flag::FallsThrough),
            group: node.flags.contains(Flag::Group),
            combinable: self.combinable(part),
            steps: self.body(part),
            actions: self.actions(part),
        }
    }

    /// What block `id`, written in a test or a header, draws there, in the order it runs
    /// (outside definitions): the boxes it is or holds, and the unknown calls outside them, which
    /// a box lists (`listed`: `id` is inside one).
    fn chips(&mut self, id: u32, listed: bool, boxes: &mut Vec<CallBox>, unknown: &mut Vec<OtherCall>) {
        let call = self.kind(id) == NodeKind::Call;
        let boxed = call && self.is_box(id);
        for &child in self.children(id) {
            if !self.kind(child).is_definition() {
                self.chips(child, listed || boxed, boxes, unknown);
            }
        }
        if boxed {
            boxes.push(self.call_box(id, None));
        } else if call && !listed && self.outcome(id) == Outcome::Unknown {
            unknown.push(OtherCall {
                inside: Vec::new(),
                ..self.other_call(id, None)
            });
        }
    }

    fn call_ref(&self, call: u32) -> CallRef {
        CallRef {
            key: self.key(call),
            callee: self.callee(call),
            name: self.name(call),
        }
    }

    fn callee(&self, call: u32) -> String {
        self.node(call).name.as_deref().unwrap_or_default().to_owned()
    }

    fn call_box(&mut self, call: u32, feeds: Option<CallRef>) -> CallBox {
        let node = self.node(call);
        let calls = self.listed(call);
        let callbacks = self.callbacks(call);
        CallBox {
            key: self.key(call),
            callee: self.callee(call),
            name: self.name(call),
            form: node.form,
            to: LinkEnd::of(&self.target(call)),
            binds: node.binds.as_deref().map(str::to_owned),
            awaited: node.flags.contains(Flag::Awaited),
            sometimes: node.flags.contains(Flag::Conditional),
            role: node.role,
            feeds,
            calls,
            callbacks,
            inset: self.call_inset(call),
            actions: self.actions(call),
        }
    }

    fn other_call(&self, call: u32, feeds: Option<CallRef>) -> OtherCall {
        let node = self.node(call);
        OtherCall {
            key: self.key(call),
            callee: self.callee(call),
            form: node.form,
            to: LinkEnd::of(&self.target(call)),
            binds: node.binds.as_deref().map(str::to_owned),
            awaited: node.flags.contains(Flag::Awaited),
            sometimes: node.flags.contains(Flag::Conditional),
            role: node.role,
            definition: node.r#for.map(|definition| self.key(definition)),
            feeds,
            inside: self.listed(call),
            actions: self.actions(call),
        }
    }

    /// The non-box calls written in call `call`'s arguments, listed on it. A receiver is part of
    /// the callee, and a hidden call isn't listed, so the calls inside both are listed instead.
    fn listed(&self, call: u32) -> Vec<OtherCall> {
        let head = self.node(call).head;
        let mut listed = Vec::new();
        for &child in self.children(call) {
            if self.kind(child) != NodeKind::Call || self.is_box(child) {
                continue;
            }
            let receiver = head.is_some_and(|head| head.contains(self.node(child).range));
            if !receiver && self.shown(child) {
                listed.push(self.other_call(child, None));
            } else {
                listed.extend(self.listed(child));
            }
        }
        listed
    }

    fn callbacks(&mut self, call: u32) -> Vec<Callback> {
        let callbacks: Vec<u32> = self.callbacks_of(call).collect();
        callbacks
            .into_iter()
            .map(|callback| Callback {
                key: self.key(callback),
                label: self.tree.label(callback),
                property: self.node(callback).name.as_deref().map(str::to_owned),
                name: self.name(callback),
                signature: self.one_line(self.node(callback).head),
                inset: self.callback_inset(callback),
                actions: self.actions(callback),
            })
            .collect()
    }

    /// The paths below block `key` when one of this flow's open paths starts at it.
    fn opened(&self, key: &Key) -> Option<Vec<&'a [Key]>> {
        let mut found = false;
        let mut below = Vec::new();
        for path in self.open {
            if let Some((first, rest)) = path.split_first()
                && first == key
            {
                found = true;
                if !rest.is_empty() {
                    below.push(rest);
                }
            }
        }
        found.then_some(below)
    }

    fn call_inset(&mut self, call: u32) -> Option<Box<DrawnFlow>> {
        let below = self.opened(self.tree.key(call))?;
        let Some(Openable::Call { callee }) = overlay::openable(self.tree, call) else {
            return None;
        };
        let trees = self.drawer.trees;
        let block = trees.block(callee.as_str())?;
        block
            .kind()
            .is_definition()
            .then(|| Box::new(self.drawer.flow(block.tree, block.id, self.depth + 1, &below)))
    }

    fn callback_inset(&mut self, callback: u32) -> Option<Box<DrawnFlow>> {
        let below = self.opened(self.tree.key(callback))?;
        Some(Box::new(self.drawer.flow(
            self.tree,
            callback,
            self.depth + 1,
            &below,
        )))
    }
}

/// How many of `call` and the calls listed inside it, at any depth, are unknown.
fn unknowns(call: &OtherCall) -> u32 {
    u32::from(call.to.outcome == Outcome::Unknown) + call.inside.iter().map(unknowns).sum::<u32>()
}

/// A hidden control block's code name in its gap tick.
fn hidden_label(form: Form) -> &'static str {
    match form {
        Form::For => "for loop",
        Form::While => "while loop",
        Form::Do => "do loop",
        form => form.as_str(),
    }
}

/// The sequence of `items`, with gap ticks for the code `lines` no item covers and thin items
/// gathered into rows.
fn finish(items: Vec<Item>, hidden: &[Hidden], lines: &[u32]) -> Sequence {
    // Gaps by the item they lead into: the first one whose code reaches past them;
    // `items.len()` is the end.
    let mut gaps: Vec<(usize, GapTick)> = Vec::new();
    for &line in lines {
        if items
            .iter()
            .filter_map(|item| item.span)
            .any(|span| (span.start..=span.end).contains(&line))
        {
            continue;
        }
        let position = items
            .iter()
            .position(|item| item.reach().is_some_and(|reach| reach >= line))
            .unwrap_or(items.len());
        match gaps.last_mut() {
            Some((at, gap)) if *at == position => {
                gap.count += 1;
                gap.lines.end = line;
            }
            _ => gaps.push((
                position,
                GapTick {
                    count: 1,
                    lines: LineSpan {
                        start: line,
                        end: line,
                    },
                    hidden: Vec::new(),
                },
            )),
        }
    }
    let mut before: FxHashMap<usize, GapTick> = FxHashMap::default();
    let mut end_gap = None;
    for (position, mut gap) in gaps {
        gap.hidden = hidden
            .iter()
            .filter(|hidden| (gap.lines.start..=gap.lines.end).contains(&hidden.line))
            .map(|hidden| hidden.block.clone())
            .collect();
        if position < items.len() {
            before.insert(position, gap);
        } else {
            end_gap = Some(gap);
        }
    }
    let mut steps: Vec<Step> = Vec::new();
    for (index, mut item) in items.into_iter().enumerate() {
        item.step.gap = before.remove(&index);
        if item.step.gap.is_none()
            && let (
                Some(Step {
                    shape: Shape::Thin(row),
                    ..
                }),
                Shape::Thin(next),
            ) = (steps.last_mut(), &mut item.step.shape)
        {
            row.unknown += next.unknown;
            row.calls.append(&mut next.calls);
            row.lines.end = row.lines.end.max(next.lines.end);
            continue;
        }
        steps.push(item.step);
    }
    Sequence { steps, end_gap }
}

/// What a combined step holds: its decisions' tests, its loops, and its project calls.
#[derive(Default)]
struct Summary {
    conditions: u32,
    loops: u32,
    calls: Vec<HeldCall>,
}

impl Summary {
    fn sequence(&mut self, sequence: &Sequence) {
        for step in &sequence.steps {
            match &step.shape {
                Shape::Call(call) => self.call(call),
                Shape::Exit(exit) => {
                    if let Some(ExitValue::Call(call)) = &exit.value {
                        self.call(call);
                    }
                }
                Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                    if matches!(step.shape, Shape::Loop(_)) {
                        self.loops += 1;
                    }
                    frame.header_calls.iter().for_each(|call| self.call(call));
                    for lane in &frame.lanes {
                        if let Some(test) = &lane.test {
                            if matches!(step.shape, Shape::Decision(_)) {
                                self.conditions += 1;
                            }
                            test.calls.iter().for_each(|call| self.call(call));
                        }
                        self.sequence(&lane.steps);
                    }
                }
                Shape::Combined(combined) => {
                    if let Some(steps) = &combined.steps {
                        self.sequence(steps);
                    }
                }
                Shape::Thin(_) | Shape::Proposed(_) => {}
            }
        }
    }

    fn call(&mut self, call: &CallBox) {
        let (Outcome::Project, Some(to)) = (call.to.outcome, &call.to.key) else {
            return;
        };
        self.calls.push(HeldCall {
            key: call.key.clone(),
            callee: call.callee.clone(),
            name: call.name.clone(),
            to: to.clone(),
        });
    }
}

/// What `head` says after the first `name` written in it as a whole word, if it writes it.
fn after_name<'h>(head: &'h str, name: &str) -> Option<&'h str> {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    head.match_indices(name).find_map(|(at, _)| {
        let end = at + name.len();
        (!head[..at].ends_with(word) && !head[end..].starts_with(word)).then(|| &head[end..])
    })
}
