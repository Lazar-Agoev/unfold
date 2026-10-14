//! The routes of a built flow (`docs/designs/diagram-blocks.md`, the routes table), the combined
//! steps that take over the routes leaving them, and each step's route state.
//!
//! | Block | Routes |
//! | --- | --- |
//! | A body or a lane | each step goes on to the next; the last goes wherever the body goes |
//! | `if`, ternary | the first test; yes into its lane, no to the next test, the `else` lane, or past the decision; each lane's end past the decision |
//! | `switch`, `match` | a case route to each case lane; default to the default lane, or past the decision; a lane's end past the decision, or into the next lane when it falls through |
//! | loop | each into the body (through its test: no goes back to the header); the body's end back to the header (next); done to the `else` lane, then past the loop, or straight past it |
//! | try | into the body, with an on-error route to each handler; the body's end to `else`, then `finally`, then past the try; a handler's end to `finally`, then past the try |
//! | with | into the body, then past the frame |
//! | `return` | End, through every enclosing `finally` |
//! | `raise`, `throw` | the handlers of the nearest try whose body holds it, or the caller, through the `finally` lanes between |
//! | `break` | past its loop or `switch`; `continue`: its loop's header; through the `finally` lanes between |
//! | `yield` | on to the next step |
//! | truncated | also "may continue" to wherever its end goes |
//!
//! A collapsed combined step keeps every route that leaves its blocks: ordinary flow to where it
//! goes becomes its own "on" route, and exits keep their kind and target. A search from the
//! start then marks what no route reaches as unreachable.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::Key;
use unfold_core::vocab::{Flag, Form};

use super::{
    DrawnFlow, ExitStep, Frame, Lane, Route, RouteEnd, RouteKind, RouteState, Sequence, Shape, Step,
};
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
    Loop { key: Key, header: u32, after: RouteEnd },
    /// A `switch` or `match`, which a TypeScript `break` may leave.
    Switch { key: Key, after: RouteEnd },
    /// A try's body, whose handlers may catch.
    Handlers(Vec<u32>),
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
    };
    let entry = router.sequence(&flow.steps, Next::on(RouteEnd::End));
    router.push(0, RouteKind::On, entry, Vec::new());
    let Router {
        mut routes,
        groups,
        truncated,
        ..
    } = router;
    collapse(&mut flow.steps, &mut routes, &groups);
    routes.sort_by_key(|route| route.from);
    let reached = reached(&routes);
    mark(&mut flow.steps, &reached, &truncated);
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
}

impl Router<'_> {
    fn push(&mut self, from: u32, kind: RouteKind, to: RouteEnd, through: Vec<u32>) {
        self.routes.push(Route {
            from,
            kind,
            to,
            through,
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

    /// Enters `lane` with a `kind` route, its end going to `next`.
    fn lane(&mut self, lane: &Lane, kind: RouteKind, next: Next) {
        if self.is_truncated(&lane.key) {
            self.truncated.insert(lane.id);
        }
        let entry = self.sequence(&lane.steps, next);
        self.push(lane.id, kind, entry, Vec::new());
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
        if let Some(body) = frame.lanes.iter().find(|lane| lane.form == Form::Body) {
            self.push(id, RouteKind::Each, RouteEnd::Step(body.id), Vec::new());
            self.scopes.push(Scope::Loop {
                key: frame.key.clone(),
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
                    handlers.iter().map(|handler| handler.id).collect(),
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
            self.lane(finally, RouteKind::On, next);
        }
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
                let mut handlers = None;
                for scope in self.scopes.iter().rev() {
                    match scope {
                        Scope::Finally(lane) => through.push(*lane),
                        Scope::Handlers(lanes) => {
                            handlers = Some(lanes.clone());
                            break;
                        }
                        Scope::Loop { .. } | Scope::Switch { .. } => {}
                    }
                }
                match handlers {
                    Some(lanes) => {
                        for lane in lanes {
                            self.push(id, RouteKind::Raise, RouteEnd::Step(lane), through.clone());
                        }
                    }
                    None => self.push(id, RouteKind::Raise, RouteEnd::Caller, through),
                }
            }
            Form::Break | Form::Continue => {
                let leaving = exit.form == Form::Break;
                let mut found = None;
                if let Some(target) = &exit.target {
                    for scope in self.scopes.iter().rev() {
                        match scope {
                            Scope::Finally(lane) => through.push(*lane),
                            Scope::Loop { key, header, after } if *key == target.key => {
                                found = Some(if leaving {
                                    (RouteKind::Break, *after)
                                } else {
                                    (RouteKind::Continue, RouteEnd::Step(*header))
                                });
                                break;
                            }
                            Scope::Switch { key, after } if *key == target.key && leaving => {
                                found = Some((RouteKind::Break, *after));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                match found {
                    Some((kind, to)) => self.push(id, kind, to, through),
                    None => self.push(id, RouteKind::MayContinue, next.to, Vec::new()),
                }
            }
            _ => self.push(id, next.kind, next.to, Vec::new()),
        }
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
/// the routes that leave them.
fn collapse(sequence: &mut Sequence, routes: &mut Vec<Route>, groups: &FxHashMap<u32, Next>) {
    for step in &mut sequence.steps {
        match &mut step.shape {
            Shape::Combined(combined) if combined.collapsed => {
                let (Some(inner), Some(next)) = (combined.steps.take(), groups.get(&step.id)) else {
                    continue;
                };
                let mut inside = FxHashSet::default();
                ids(&inner, &mut inside);
                take_over(step.id, &inside, *next, routes);
            }
            Shape::Combined(combined) => {
                if let Some(steps) = &mut combined.steps {
                    collapse(steps, routes, groups);
                }
            }
            Shape::Decision(frame) | Shape::Loop(frame) | Shape::Try(frame) | Shape::With(frame) => {
                for lane in &mut frame.lanes {
                    collapse(&mut lane.steps, routes, groups);
                }
            }
            Shape::Call(_) | Shape::Thin(_) | Shape::Exit(_) | Shape::Proposed(_) => {}
        }
    }
}

/// Moves the routes that leave `inside` to `group`, whose end goes to `next`.
fn take_over(group: u32, inside: &FxHashSet<u32>, next: Next, routes: &mut Vec<Route>) {
    let is_inside = |end: RouteEnd| matches!(end, RouteEnd::Step(id) if inside.contains(&id));
    let mut kept: Vec<Route> = Vec::with_capacity(routes.len());
    for mut route in routes.drain(..) {
        let from_inside = inside.contains(&route.from);
        if (from_inside || route.from == group) && is_inside(route.to) {
            continue;
        }
        route.through.retain(|lane| !inside.contains(lane));
        if from_inside {
            route.from = group;
            if route.kind.is_flow() && route.to == next.to {
                route.kind = next.kind;
            }
            if kept.contains(&route) {
                continue;
            }
        }
        kept.push(route);
    }
    *routes = kept;
}

/// The ids a route reaches from the start, through `finally` lanes too.
fn reached(routes: &[Route]) -> FxHashSet<u32> {
    let mut from: FxHashMap<u32, Vec<&Route>> = FxHashMap::default();
    for route in routes {
        from.entry(route.from).or_default().push(route);
    }
    let mut reached = FxHashSet::default();
    reached.insert(0);
    let mut queue = vec![0];
    while let Some(id) = queue.pop() {
        for route in from.get(&id).into_iter().flatten() {
            let to = match route.to {
                RouteEnd::Step(to) => Some(to),
                RouteEnd::End | RouteEnd::Caller => None,
            };
            for next in route.through.iter().copied().chain(to) {
                if reached.insert(next) {
                    queue.push(next);
                }
            }
        }
    }
    reached
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
