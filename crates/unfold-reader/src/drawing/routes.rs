//! The routes of a built flow (docs/reader.md#routes), the combined
//! steps that take over the routes leaving them, and each step's route state.
//!
//! | Block | Routes |
//! | --- | --- |
//! | A body or a lane | each step goes on to the next; the last goes wherever the body goes |
//! | `if`, ternary | the first test; yes into its lane, no to the next test, the `else` lane, or past the decision; each lane's end past the decision |
//! | `switch`, `match` | a case route to each case lane; default to the default lane, or past the decision; a lane's end past the decision, or into the next lane when it falls through |
//! | loop | each into the body (through its test: no goes back to the header); the body's end back to the header (next); done to the `else` lane, then past the loop, or straight past it |
//! | `do` loop | into the body first; the body's end (and `continue`) back to the header (next), and done past the loop from there too, since the test comes after the body |
//! | try | into the body, with an on-error route to each handler; the body's end to `else`, then `finally`, then past the try; a handler's end to `finally`, then past the try |
//! | with | into the body, then past the frame |
//! | `return` | End, through every enclosing `finally` |
//! | `raise`, `throw` | the first handler proven to catch it ([`crate::contracts::catching`]) of the nearest enclosing try whose body holds it and that has one, else the caller; through the `finally` lanes between |
//! | `break` | past its loop or `switch`; `continue`: its loop's header; through the `finally` lanes between |
//! | `yield` | on to the next step |
//! | truncated | a step, a lane, or the definition (from the start) also "may continue" to wherever its end goes |
//!
//! A `finally` lane whose own end no route reaches (it always returns, raises, breaks, or
//! continues) replaces the transfer that entered it: an exit routed through it ends in the lane
//! instead, and the lane's own exits go on.
//!
//! A search from the start marks what no route reaches as unreachable. An exit that passes
//! through a `finally` lane reaches the lane and goes on to its own target: the lane's own end
//! route is taken only when ordinary flow entered the lane, so code after a `try` whose body
//! only returns never runs.
//!
//! A collapsed combined step keeps the routes that leave its blocks: ordinary flow to where it
//! goes becomes its own "on" route, and exits keep their kind and target. A reached one keeps
//! only the routes the search takes, so it draws no "on" route that nothing reaches.

use std::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::Key;
use unfold_core::vocab::{Flag, Form};

use super::{
    DrawnFlow, ExitStep, Frame, Lane, Route, RouteEnd, RouteKind, RouteState, Sequence, Shape, Step,
};
use crate::contracts::catching::{Raised, catches, exit_raises, lineage};
use crate::tree::FileTree;

/// Where a body's end goes, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Next {
    to: RouteEnd,
    kind: RouteKind,
}

impl Next {
    const fn on(to: RouteEnd) -> Self {
        Self {
            to,
            kind: RouteKind::On,
        }
    }
}

/// What encloses a step, for its exits.
enum Scope {
    /// A loop's body.
    Loop {
        key: Key,
        form: Form,
        header: u32,
        after: RouteEnd,
    },
    /// A `switch` or `match`, which a TypeScript `break` may leave.
    Switch { key: Key, form: Form, after: RouteEnd },
    /// A try's body, whose handlers may catch: each handler lane's id, and its part's node.
    Handlers(Vec<(u32, Option<u32>)>),
    /// The body, handlers, or `else` of a try with this `finally` lane.
    Finally(u32),
}

/// Computes `flow`'s routes, collapses its collapsed combined steps, and marks route states.
pub(super) fn route(flow: &mut DrawnFlow, tree: &FileTree) {
    let mut router = Router {
        tree,
        routes: Vec::new(),
        scopes: Vec::new(),
        groups: FxHashMap::default(),
        truncated: FxHashSet::default(),
        gates: FxHashMap::default(),
    };
    let entry = router.sequence(&flow.steps, Next::on(RouteEnd::End));
    router.push(0, RouteKind::On, entry, Vec::new());
    if router.is_truncated(&flow.header.key) {
        router.push(0, RouteKind::MayContinue, RouteEnd::End, Vec::new());
    }
    let Router {
        routes,
        groups,
        truncated,
        gates,
        ..
    } = router;
    let reach = reach(&routes, &gates, 0, 0);
    let mut routes: Vec<(Route, bool)> = routes.into_iter().zip(reach.taken).collect();
    collapse(&mut flow.steps, &mut routes, &groups, &reach.ids);
    let mut routes: Vec<Route> = routes.into_iter().map(|(route, _)| route).collect();
    routes.sort_by_key(|route| route.from);
    mark(&mut flow.steps, &reach.ids, &truncated);
    flow.routes = routes;
}

struct Router<'t> {
    tree: &'t FileTree,
    routes: Vec<Route>,
    scopes: Vec<Scope>,
    /// Each collapsed combined step, and where its end goes.
    groups: FxHashMap<u32, Next>,
    /// The truncated steps and lanes.
    truncated: FxHashSet<u32>,
    /// The `finally` lanes whose own end each route (by index) is, which it takes only when
    /// ordinary flow entered them.
    gates: FxHashMap<usize, Vec<u32>>,
}

impl Router<'_> {
    fn push(&mut self, from: u32, kind: RouteKind, to: RouteEnd, through: Vec<u32>) {
        self.routes.push(Route {
            from,
            kind,
            to,
            through,
            leaves: None,
        });
    }

    fn is_truncated(&self, key: &Key) -> bool {
        self.tree
            .id(key.as_str())
            .is_some_and(|id| self.tree.node(id).flags.contains(Flag::Truncated))
    }

    /// Routes `sequence`'s steps, its end going to `next`; returns where it is entered.
    fn sequence(&mut self, sequence: &Sequence, next: Next) -> RouteEnd {
        for (index, step) in sequence.steps.iter().enumerate() {
            let after = sequence
                .steps
                .get(index + 1)
                .map_or(next, |following| Next::on(RouteEnd::Step(following.id)));
            self.step(step, after);
        }
        sequence
            .steps
            .first()
            .map_or(next.to, |first| RouteEnd::Step(first.id))
    }

    /// Enters `lane` with a `kind` route, its end going to `next`. A truncated lane also may
    /// continue to where its end goes.
    fn lane(&mut self, lane: &Lane, kind: RouteKind, next: Next) {
        let entry = self.sequence(&lane.steps, next);
        self.push(lane.id, kind, entry, Vec::new());
        if self.is_truncated(&lane.key) {
            self.truncated.insert(lane.id);
            self.push(lane.id, RouteKind::MayContinue, next.to, Vec::new());
        }
    }

    fn step(&mut self, step: &Step, next: Next) {
        let id = step.id;
        let truncated = match &step.shape {
            Shape::Call(call) => {
                self.push(id, next.kind, next.to, Vec::new());
                self.is_truncated(&call.key)
            }
            Shape::Thin(row) => {
                self.push(id, next.kind, next.to, Vec::new());
                row.calls.iter().any(|call| self.is_truncated(&call.key))
            }
            Shape::Proposed(_) => {
                self.push(id, next.kind, next.to, Vec::new());
                false
            }
            Shape::Exit(exit) => {
                self.exit(id, exit, next);
                self.is_truncated(&exit.key)
            }
            Shape::Decision(frame) => {
                self.decision(id, frame, next);
                self.is_truncated(&frame.key)
            }
            Shape::Loop(frame) => {
                self.looping(id, frame, next);
                self.is_truncated(&frame.key)
            }
            Shape::Try(frame) => {
                self.trying(id, frame, next);
                self.is_truncated(&frame.key)
            }
            Shape::With(frame) => {
                match frame.lanes.first() {
                    Some(body) => {
                        self.push(id, RouteKind::On, RouteEnd::Step(body.id), Vec::new());
                        self.lane(body, RouteKind::On, next);
                    }
                    None => self.push(id, next.kind, next.to, Vec::new()),
                }
                self.is_truncated(&frame.key)
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &combined.steps {
                    let entry = self.sequence(steps, next);
                    self.push(id, RouteKind::On, entry, Vec::new());
                }
                if combined.collapsed {
                    self.groups.insert(id, next);
                }
                false
            }
        };
        if truncated {
            self.truncated.insert(id);
            self.push(id, RouteKind::MayContinue, next.to, Vec::new());
        }
    }

    fn decision(&mut self, id: u32, frame: &Frame, next: Next) {
        let lanes = &frame.lanes;
        if matches!(frame.form, Form::Switch | Form::Match) {
            self.scopes.push(Scope::Switch {
                key: frame.key.clone(),
                form: frame.form,
                after: next.to,
            });
            for (index, lane) in lanes.iter().enumerate() {
                let kind = if lane.form == Form::Default {
                    RouteKind::Default
                } else {
                    RouteKind::Case
                };
                self.push(id, kind, RouteEnd::Step(lane.id), Vec::new());
                let end = match lanes.get(index + 1) {
                    Some(following) if lane.falls_through => Next::on(RouteEnd::Step(following.id)),
                    _ => next,
                };
                self.lane(lane, RouteKind::On, end);
            }
            if !lanes.iter().any(|lane| lane.form == Form::Default) {
                self.push(id, RouteKind::Default, next.to, Vec::new());
            }
            self.scopes.pop();
            return;
        }
        let first = lanes.first().map_or(next.to, |lane| RouteEnd::Step(lane.id));
        self.push(id, RouteKind::On, first, Vec::new());
        for (index, lane) in lanes.iter().enumerate() {
            if lane.form == Form::Else {
                self.lane(lane, RouteKind::On, next);
                continue;
            }
            self.lane(lane, RouteKind::Yes, next);
            let otherwise = lanes
                .get(index + 1)
                .map_or(next.to, |following| RouteEnd::Step(following.id));
            self.push(lane.id, RouteKind::No, otherwise, Vec::new());
        }
    }

    fn looping(&mut self, id: u32, frame: &Frame, next: Next) {
        if frame.form == Form::Do {
            self.do_loop(id, frame, next);
            return;
        }
        if let Some(body) = frame.lanes.iter().find(|lane| lane.form == Form::Body) {
            self.push(id, RouteKind::Each, RouteEnd::Step(body.id), Vec::new());
            self.scopes.push(Scope::Loop {
                key: frame.key.clone(),
                form: frame.form,
                header: id,
                after: next.to,
            });
            let back = Next {
                to: RouteEnd::Step(id),
                kind: RouteKind::Next,
            };
            if body.test.is_some() {
                self.lane(body, RouteKind::Yes, back);
                self.push(body.id, RouteKind::Next, RouteEnd::Step(id), Vec::new());
            } else {
                self.lane(body, RouteKind::On, back);
            }
            self.scopes.pop();
        }
        match frame.lanes.iter().find(|lane| lane.form == Form::Else) {
            Some(otherwise) => {
                self.push(id, RouteKind::Done, RouteEnd::Step(otherwise.id), Vec::new());
                self.lane(otherwise, RouteKind::On, next);
            }
            None => self.push(id, RouteKind::Done, next.to, Vec::new()),
        }
    }

    /// A `do` loop: it enters its body first, and its test comes after the body, so done leaves
    /// from wherever the body goes back to the header (its end, and each `continue`).
    fn do_loop(&mut self, id: u32, frame: &Frame, next: Next) {
        let Some(body) = frame.lanes.iter().find(|lane| lane.form == Form::Body) else {
            self.push(id, RouteKind::Done, next.to, Vec::new());
            return;
        };
        self.push(id, RouteKind::On, RouteEnd::Step(body.id), Vec::new());
        self.scopes.push(Scope::Loop {
            key: frame.key.clone(),
            form: frame.form,
            header: id,
            after: next.to,
        });
        let start = self.routes.len();
        let back = Next {
            to: RouteEnd::Step(id),
            kind: RouteKind::Next,
        };
        self.lane(body, RouteKind::On, back);
        self.scopes.pop();
        let tests: Vec<(u32, Vec<u32>)> = self.routes[start..]
            .iter()
            .filter(|route| {
                route.to == RouteEnd::Step(id) && matches!(route.kind, RouteKind::Next | RouteKind::Continue)
            })
            .map(|route| (route.from, route.through.clone()))
            .collect();
        for (from, through) in tests {
            self.push(from, RouteKind::Done, next.to, through);
        }
    }

    fn trying(&mut self, id: u32, frame: &Frame, next: Next) {
        let lane = |form: Form| frame.lanes.iter().find(|lane| lane.form == form);
        let handlers: Vec<&Lane> = frame
            .lanes
            .iter()
            .filter(|lane| matches!(lane.form, Form::Except | Form::Catch))
            .collect();
        let otherwise = lane(Form::Else);
        let finally = lane(Form::Finally);
        let to_finally = finally.map_or(next, |finally| Next {
            to: RouteEnd::Step(finally.id),
            kind: RouteKind::Finally,
        });
        let start = self.routes.len();
        if let Some(finally) = finally {
            self.scopes.push(Scope::Finally(finally.id));
        }
        if let Some(body) = lane(Form::Body) {
            self.push(id, RouteKind::On, RouteEnd::Step(body.id), Vec::new());
            for handler in &handlers {
                self.push(
                    body.id,
                    RouteKind::OnError,
                    RouteEnd::Step(handler.id),
                    Vec::new(),
                );
            }
            let after_body = otherwise.map_or(to_finally, |otherwise| Next::on(RouteEnd::Step(otherwise.id)));
            if !handlers.is_empty() {
                self.scopes.push(Scope::Handlers(
                    handlers
                        .iter()
                        .map(|handler| (handler.id, self.tree.id(handler.key.as_str())))
                        .collect(),
                ));
            }
            self.lane(body, RouteKind::On, after_body);
            if !handlers.is_empty() {
                self.scopes.pop();
            }
        }
        for handler in &handlers {
            self.lane(handler, RouteKind::On, to_finally);
        }
        if let Some(otherwise) = otherwise {
            self.lane(otherwise, RouteKind::On, to_finally);
        }
        if let Some(finally) = finally {
            self.scopes.pop();
            let entered = self.routes.len();
            self.lane(finally, RouteKind::On, next);
            // The lane's own end: what goes where its end goes, other than an exit.
            let ends: Vec<usize> = (entered..self.routes.len())
                .filter(|&index| {
                    let route = &self.routes[index];
                    let flows = route.kind.is_flow() || route.kind == RouteKind::MayContinue;
                    flows && route.to == next.to
                })
                .collect();
            for &index in &ends {
                self.gates.entry(index).or_default().push(finally.id);
            }
            // A lane whose own end nothing inside it reaches replaces what entered it.
            let inside = reach(&self.routes, &self.gates, finally.id, entered);
            if !ends.iter().any(|&index| inside.taken[index]) {
                self.end_in(finally.id, start..entered);
            }
        }
    }

    /// Ends in `finally` lane `lane`, which never reaches its own end, every route of `range`
    /// that passes through it: the lane's own exits replace the transfer that entered it.
    fn end_in(&mut self, lane: u32, range: Range<usize>) {
        for route in &mut self.routes[range] {
            if let Some(at) = route.through.iter().position(|&passed| passed == lane) {
                route.through.truncate(at);
                route.to = RouteEnd::Step(lane);
                route.leaves = None;
            }
        }
    }

    /// What `raise` or `throw` exit `exit` raises, and every class it derives from, as far as
    /// this file proves.
    fn raises(&self, exit: &Key) -> Vec<Raised> {
        let Some(node) = self.tree.id(exit.as_str()) else {
            return Vec::new();
        };
        let mut classes = self.tree;
        // Reading one file's tree never fails.
        exit_raises(&mut classes, self.tree, node)
            .and_then(|raised| lineage(&mut classes, raised))
            .unwrap_or_default()
    }

    fn exit(&mut self, id: u32, exit: &ExitStep, next: Next) {
        let mut through = Vec::new();
        match exit.form {
            Form::Return => {
                for scope in self.scopes.iter().rev() {
                    if let Scope::Finally(lane) = scope {
                        through.push(*lane);
                    }
                }
                self.push(id, RouteKind::Return, RouteEnd::End, through);
            }
            Form::Raise | Form::Throw => {
                let lineage = self.raises(&exit.key);
                let mut to = RouteEnd::Caller;
                for scope in self.scopes.iter().rev() {
                    match scope {
                        Scope::Finally(lane) => through.push(*lane),
                        Scope::Handlers(handlers) => {
                            let caught = handlers.iter().find(|(_, part)| {
                                part.is_some_and(|part| catches(self.tree, part, &lineage))
                            });
                            if let Some(&(lane, _)) = caught {
                                to = RouteEnd::Step(lane);
                                break;
                            }
                        }
                        Scope::Loop { .. } | Scope::Switch { .. } => {}
                    }
                }
                self.push(id, RouteKind::Raise, to, through);
            }
            Form::Break | Form::Continue => {
                let leaving = exit.form == Form::Break;
                let mut found = None;
                if let Some(target) = &exit.target {
                    for scope in self.scopes.iter().rev() {
                        match scope {
                            Scope::Finally(lane) => through.push(*lane),
                            Scope::Loop {
                                key,
                                form,
                                header,
                                after,
                            } if *key == target.key => {
                                found = Some(if leaving {
                                    (RouteKind::Break, *after, *form)
                                } else {
                                    (RouteKind::Continue, RouteEnd::Step(*header), *form)
                                });
                                break;
                            }
                            Scope::Switch { key, form, after } if *key == target.key && leaving => {
                                found = Some((RouteKind::Break, *after, *form));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                match found {
                    Some((kind, to, form)) => self.routes.push(Route {
                        from: id,
                        kind,
                        to,
                        through,
                        leaves: Some(form),
                    }),
                    None => self.push(id, RouteKind::MayContinue, next.to, Vec::new()),
                }
            }
            _ => self.push(id, next.kind, next.to, Vec::new()),
        }
    }
}

/// The lane of `flow` with id `id`, for wording a route that goes to it.
pub(super) fn lane(flow: &DrawnFlow, id: u32) -> Option<&Lane> {
    fn find(sequence: &Sequence, id: u32) -> Option<&Lane> {
        sequence.steps.iter().find_map(|step| match &step.shape {
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                frame.lanes.iter().find_map(|lane| {
                    if lane.id == id {
                        Some(lane)
                    } else {
                        find(&lane.steps, id)
                    }
                })
            }
            Shape::Combined(combined) => combined.steps.as_ref().and_then(|steps| find(steps, id)),
            Shape::Call(_) | Shape::Thin(_) | Shape::Exit(_) | Shape::Proposed(_) => None,
        })
    }
    find(&flow.steps, id)
}

/// The route state of `flow`'s start: truncated when its definition is, which may continue to
/// its end.
pub(super) fn start_state(flow: &DrawnFlow) -> RouteState {
    let truncated = flow
        .routes
        .iter()
        .any(|route| route.from == 0 && route.kind == RouteKind::MayContinue);
    if truncated {
        RouteState::Truncated
    } else {
        RouteState::Reached
    }
}

/// Every step and lane id inside `sequence`.
fn ids(sequence: &Sequence, out: &mut FxHashSet<u32>) {
    for step in &sequence.steps {
        out.insert(step.id);
        match &step.shape {
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &frame.lanes {
                    out.insert(lane.id);
                    ids(&lane.steps, out);
                }
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &combined.steps {
                    ids(steps, out);
                }
            }
            Shape::Call(_) | Shape::Thin(_) | Shape::Exit(_) | Shape::Proposed(_) => {}
        }
    }
}

/// Collapses every collapsed combined step in `sequence`: its own steps go, and it takes over
/// the routes that leave them. Each route carries whether the search takes it; `reached` holds
/// the ids it reaches.
fn collapse(
    sequence: &mut Sequence,
    routes: &mut Vec<(Route, bool)>,
    groups: &FxHashMap<u32, Next>,
    reached: &FxHashSet<u32>,
) {
    for step in &mut sequence.steps {
        match &mut step.shape {
            Shape::Combined(combined) if combined.collapsed => {
                let (Some(inner), Some(next)) = (combined.steps.take(), groups.get(&step.id)) else {
                    continue;
                };
                let mut inside = FxHashSet::default();
                ids(&inner, &mut inside);
                take_over(step.id, &inside, *next, reached.contains(&step.id), routes);
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &mut combined.steps {
                    collapse(steps, routes, groups, reached);
                }
            }
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &mut frame.lanes {
                    collapse(&mut lane.steps, routes, groups, reached);
                }
            }
            Shape::Call(_) | Shape::Thin(_) | Shape::Exit(_) | Shape::Proposed(_) => {}
        }
    }
}

/// Moves the routes that leave `inside` to `group`, whose end goes to `next`. A reached group
/// keeps only the routes the search takes: an exit whose code runs, but no "on" route that
/// only a `finally` lane's end (entered by an exit) or code that never runs would take.
fn take_over(
    group: u32,
    inside: &FxHashSet<u32>,
    next: Next,
    reached: bool,
    routes: &mut Vec<(Route, bool)>,
) {
    let is_inside = |end: RouteEnd| matches!(end, RouteEnd::Step(id) if inside.contains(&id));
    let mut kept: Vec<(Route, bool)> = Vec::with_capacity(routes.len());
    for (mut route, taken) in routes.drain(..) {
        let from_inside = inside.contains(&route.from);
        if (from_inside || route.from == group) && is_inside(route.to) {
            continue;
        }
        if from_inside && reached && !taken {
            continue;
        }
        route.through.retain(|lane| !inside.contains(lane));
        if from_inside {
            route.from = group;
            if route.kind.is_flow() && route.to == next.to {
                route.kind = next.kind;
            }
            if kept.iter().any(|(known, _)| *known == route) {
                continue;
            }
        }
        kept.push((route, taken));
    }
    *routes = kept;
}

/// What a search from the start reaches.
struct Reach {
    /// The ids it reaches, through `finally` lanes too.
    ids: FxHashSet<u32>,
    /// Whether it takes each route, by index.
    taken: Vec<bool>,
}

/// Searches `routes` from id `start`, taking only the routes from index `first` on. A route is
/// taken when its start is reached, except that a route gated on `finally` lanes (`gates`, by
/// index: a lane's own end) is taken only once ordinary flow entered each of them, by a route
/// that goes to the lane rather than through it. The start counts as entered.
fn reach(routes: &[Route], gates: &FxHashMap<usize, Vec<u32>>, start: u32, first: usize) -> Reach {
    let mut from: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
    for (index, route) in routes.iter().enumerate().skip(first) {
        from.entry(route.from).or_default().push(index);
    }
    let mut ids = FxHashSet::default();
    ids.insert(start);
    let mut entered = ids.clone();
    let mut taken = vec![false; routes.len()];
    // Gated routes whose start is reached, by the lane they wait for.
    let mut waiting: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
    let mut queue: Vec<usize> = from.get(&start).cloned().unwrap_or_default();
    while let Some(index) = queue.pop() {
        if taken[index] {
            continue;
        }
        let closed = gates
            .get(&index)
            .and_then(|lanes| lanes.iter().find(|lane| !entered.contains(*lane)));
        if let Some(&lane) = closed {
            waiting.entry(lane).or_default().push(index);
            continue;
        }
        taken[index] = true;
        let route = &routes[index];
        let to = match route.to {
            RouteEnd::Step(to) => Some(to),
            RouteEnd::End | RouteEnd::Caller => None,
        };
        if let Some(to) = to
            && entered.insert(to)
        {
            queue.extend(waiting.remove(&to).into_iter().flatten());
        }
        for id in route.through.iter().copied().chain(to) {
            if ids.insert(id) {
                queue.extend(from.get(&id).into_iter().flatten().copied());
            }
        }
    }
    Reach { ids, taken }
}

fn state(id: u32, reached: &FxHashSet<u32>, truncated: &FxHashSet<u32>) -> RouteState {
    if !reached.contains(&id) {
        RouteState::Unreachable
    } else if truncated.contains(&id) {
        RouteState::Truncated
    } else {
        RouteState::Reached
    }
}

fn mark(sequence: &mut Sequence, reached: &FxHashSet<u32>, truncated: &FxHashSet<u32>) {
    for step in &mut sequence.steps {
        step.state = state(step.id, reached, truncated);
        match &mut step.shape {
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &mut frame.lanes {
                    lane.state = state(lane.id, reached, truncated);
                    mark(&mut lane.steps, reached, truncated);
                }
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &mut combined.steps {
                    mark(steps, reached, truncated);
                }
            }
            Shape::Call(_) | Shape::Thin(_) | Shape::Exit(_) | Shape::Proposed(_) => {}
        }
    }
}
