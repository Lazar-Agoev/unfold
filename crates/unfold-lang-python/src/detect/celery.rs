//! `celery.task`: parameterized task decorators (`@app.task(name=..., queue=...)`) and
//! immediately applied decorator factories (`app.task(...)(function)`) on a proven `Celery`
//! application.
//!
//! The trigger is the written, nonempty literal `queue`; anything else is `unknown` (the task
//! name is never a fallback). The original body is kept only when the application's
//! constructor uses just `main`, `broker`, and `backend`, the registration just a unique,
//! nonempty literal `name` (not `celery.`) and `queue`, the decorator stack is proven, and
//! nothing else could register, configure, or replace tasks in the program's one shared task
//! registry; otherwise targets stay `unsupported`. Bare decorators, `task(function)`,
//! `shared_task`, and task invocations add no entrances.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{DetectorId, EntranceKind, Reason};
use unfold_engine::BlockRef;
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use unfold_engine::evidence::Refusal;

use super::fastapi::TRANSPARENT;
use super::origins::{Flow, Identity, Origins, Receiver, SafeUpdate, Spec};
use super::{Source, failure};
use crate::bind::ir::{ExprKind, NodeKind};
use crate::link::binder::Ctx;
use crate::link::children::Raw;
use crate::link::program::{ExprRef, NodeRef};

const APPLICATIONS: &[&str] = &["celery.Celery", "celery.app.Celery", "celery.app.base.Celery"];

const SPECS: &[Spec] = &[
    Spec {
        family: "celery",
        kind: "application",
        imports: APPLICATIONS,
        safe_updates: &[SafeUpdate {
            path: &["conf", "update"],
            keywords: &[
                "task_always_eager",
                "task_eager_propagates",
                "task_serializer",
                "accept_content",
            ],
        }],
    },
    Spec {
        family: "celery",
        kind: "task-base",
        imports: &["celery.Task", "celery.app.task.Task"],
        safe_updates: &[],
    },
    // Only negative registry evidence: it never grants `shared_task` an entrance.
    Spec {
        family: "celery-shared-task",
        kind: "shared-registration",
        imports: &["celery.shared_task", "celery.app.shared_task"],
        safe_updates: &[],
    },
];

const SHARED: &str = "celery-shared-task";
const LOADERS: &[&str] = &[
    "config_from_object",
    "config_from_envvar",
    "config_from_cmdline",
    "add_defaults",
];
const REGISTRY_METHODS: &[&str] = &["task", "register_task", "_task_from_fun", "autodiscover_tasks"];

/// The deepest chain of values the effect walk follows.
const MAX_DEPTH: u32 = 64;

/// What one effect walk has visited, and how deep it is.
#[derive(Default)]
struct Walked {
    raws: FxHashSet<Raw>,
    depth: u32,
}

/// A task registration effect a callee may have.
#[derive(Debug, Clone, Copy)]
struct TaskEffect {
    receiver: Option<Receiver>,
    registry: bool,
}

#[derive(Clone)]
enum Target {
    Value(ExprRef),
    Block(BlockRef, Box<str>),
    Failure(Reason, Box<str>),
}

struct Registration {
    receiver: Receiver,
    evidence: BlockRef,
    targets: Vec<Target>,
    name: Option<String>,
    queue: Option<String>,
    dynamic_name: bool,
    safe_options: bool,
}

/// The Celery tasks of the program.
pub(super) fn detect(flow: &Flow<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let tasks = Tasks {
        origins: Origins::new(flow, SPECS),
    };
    tasks.candidates()
}

struct Tasks<'f, 's, 'b, 'a> {
    origins: Origins<'f, 's, 'b, 'a>,
}

impl Tasks<'_, '_, '_, '_> {
    fn source(&self) -> &Source<'_, '_> {
        self.origins.source()
    }

    /// `app.task(...)` with no positional argument (a written function would select
    /// `task(function)`, whose result is an ordinary Task).
    fn registration(&self, call: Option<ExprRef>) -> Option<(Receiver, ExprRef)> {
        let call = call?;
        let source = self.source();
        if !matches!(source.kind(call), Some(ExprKind::Call { .. })) {
            return None;
        }
        let (base, "task") = source.member(source.callee(call)?)? else {
            return None;
        };
        if source.args(call).positional().next().is_some() {
            return None;
        }
        let receiver = self.origins.receiver(Some(base))?;
        (receiver.spec.kind == "application").then_some((receiver, call))
    }

    fn constructor_proven(&self, receiver: &Receiver) -> bool {
        let Some(factory) = receiver.factory.filter(|_| !receiver.annotation_only) else {
            return false;
        };
        let args = self.source().args(factory);
        args.get("main", Some(0));
        !args.uncertain()
            && args.positional().count() <= 1
            && args.all().iter().all(|argument| {
                !argument.expansion
                    && argument
                        .keyword
                        .is_none_or(|keyword| ["main", "broker", "backend"].contains(&keyword))
            })
    }

    fn declaration_target(&self, node: NodeRef, factory: ExprRef, display: Box<str>) -> Target {
        let source = self.source();
        let binder = source.binder;
        let mut ctx = Ctx::new();
        let classes = source.enclosing_classes(node);
        let mut unsupported_stack = false;
        for container in std::iter::once(node).chain(classes.iter().copied()) {
            for decorator in source.decorators(container) {
                if container == node && decorator == factory {
                    continue;
                }
                if container == node && self.registration(Some(decorator)).is_some() {
                    unsupported_stack = true;
                    continue;
                }
                let target = match source.kind(decorator) {
                    Some(ExprKind::Call { .. }) => source.callee(decorator),
                    _ => Some(decorator),
                };
                let transparent = self
                    .origins
                    .imported(target)
                    .is_some_and(|identity| TRANSPARENT.contains(&&*identity));
                if !transparent {
                    return Target::Failure(Reason::Wrapper, display);
                }
            }
        }
        let stable = binder
            .program
            .binding_of(node)
            .is_some_and(|binding| binder.stable(binding, &mut ctx));
        if unsupported_stack
            || classes
                .iter()
                .any(|&class| !binder.class_concrete(class, &mut ctx))
            || !stable
            || binder.is_abstract(node, &mut ctx)
            || binder.overload(node, &mut ctx)
            || binder.accessor(node, &mut ctx)
        {
            return Target::Failure(Reason::Unsupported, display);
        }
        match source.block_of(node) {
            Some(block) => Target::Block(block, display),
            None => Target::Failure(Reason::Unsupported, display),
        }
    }

    /// An applied handler. An existing Task is a framework wrapper: it is never unwrapped to
    /// grant a second registration the original body.
    fn expression_target(&self, expr: ExprRef) -> Target {
        let source = self.source();
        let value = source.binder.resolve(expr, &mut Ctx::new());
        if value.refusal == Some(Refusal::Wrapper) {
            return Target::Failure(Reason::Wrapper, source.text(expr));
        }
        Target::Value(expr)
    }

    /// The registration and configuration effects a callee may have, through stored values,
    /// aliases, and every write. They may withhold a body, never recognize a new API.
    fn effects(&self, raw: Raw, seen: &mut Walked, stored: bool, found: &mut Vec<TaskEffect>) {
        if seen.depth >= MAX_DEPTH {
            // Giving up must withhold: an unexplored path may register anything.
            found.push(TaskEffect {
                receiver: None,
                registry: true,
            });
            return;
        }
        if !seen.raws.insert(raw) {
            return;
        }
        seen.depth += 1;
        self.effects_inner(raw, seen, stored, found);
        seen.depth -= 1;
    }

    fn effects_inner(&self, raw: Raw, seen: &mut Walked, stored: bool, found: &mut Vec<TaskEffect>) {
        let source = self.source();
        let binder = source.binder;
        if let Raw::Expr(expr) = raw {
            for value in self.origins.stored_values(expr) {
                self.effects(Raw::Expr(value), seen, true, found);
            }
        }
        if self.origins.may_be(raw, SHARED) {
            found.push(TaskEffect {
                receiver: None,
                registry: true,
            });
        }
        match raw {
            Raw::Expr(expr) => match source.kind(expr) {
                Some(ExprKind::Call { .. }) if stored => match self.registration(Some(expr)) {
                    Some((receiver, _)) => found.push(TaskEffect {
                        receiver: Some(receiver),
                        registry: true,
                    }),
                    None => {
                        if let Some(callee) = source.callee(expr) {
                            self.effects(Raw::Expr(callee), seen, true, found);
                        }
                    }
                },
                Some(ExprKind::Member { .. }) => {
                    let Some((base, method)) = source.member(expr) else {
                        return;
                    };
                    let tasks = source
                        .member(base)
                        .filter(|(_, name)| *name == "tasks")
                        .map(|(owner, _)| owner);
                    let registry_member = ["register", "unregister"].contains(&method) && tasks.is_some();
                    if LOADERS.contains(&method) || REGISTRY_METHODS.contains(&method) || registry_member {
                        let owner = if registry_member { tasks } else { Some(base) };
                        let registry = !LOADERS.contains(&method);
                        let receiver = self
                            .origins
                            .receiver(owner)
                            .filter(|receiver| receiver.spec.kind == "application");
                        if let Some(receiver) = receiver {
                            found.push(TaskEffect {
                                receiver: Some(receiver),
                                registry,
                            });
                            return;
                        }
                        if owner.is_some_and(|owner| self.origins.may_be(Raw::Expr(owner), "celery")) {
                            found.push(TaskEffect {
                                receiver: None,
                                registry: true,
                            });
                            return;
                        }
                    }
                    for &declaration in &binder.resolve(expr, &mut Ctx::new()).declarations {
                        self.effects(Raw::Node(declaration), seen, true, found);
                    }
                }
                Some(ExprKind::Name(_)) => self.write_effects(raw, seen, found),
                _ => {}
            },
            Raw::Node(node) => match binder.program.node(node).map(|fact| &fact.kind) {
                Some(&NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    let value = ExprRef {
                        file: node.file,
                        expr: value,
                    };
                    self.effects(Raw::Expr(value), seen, true, found);
                }
                Some(NodeKind::Import) => self.write_effects(raw, seen, found),
                _ => {}
            },
        }
    }

    fn write_effects(&self, raw: Raw, seen: &mut Walked, found: &mut Vec<TaskEffect>) {
        let binder = self.source().binder;
        for write in binder.raw_writes(raw) {
            let written = binder.program.write(write);
            if let Some(value) = written.value {
                self.effects(Raw::Expr(value), seen, true, found);
            }
            let Some(imported) = &written.import else {
                continue;
            };
            let Some(member) = imported.member.filter(|_| imported.module.external.is_none()) else {
                continue;
            };
            for declaration in binder.module_declarations(&imported.module, member) {
                self.effects(Raw::Node(declaration), seen, true, found);
            }
        }
    }

    fn effects_of(&self, raw: Raw) -> Vec<TaskEffect> {
        let mut found = Vec::new();
        self.effects(raw, &mut Walked::default(), false, &mut found);
        found
    }

    fn candidates(&self) -> Vec<EntranceCandidate> {
        let source = self.source();
        let binder = source.binder;
        let mut registrations: Vec<Registration> = Vec::new();
        let mut configuration_unknown: FxHashSet<Identity> = FxHashSet::default();
        let mut registry_unknown = false;
        // Bare decorators (`@app.task`, `@shared_task`) register without an entrance.
        for (index, bind) in binder.program.files.iter().enumerate() {
            let Some(bind) = bind else {
                continue;
            };
            for (node, fact) in bind.nodes.iter().enumerate() {
                if !matches!(fact.kind, NodeKind::Function(_) | NodeKind::Class(_)) {
                    continue;
                }
                let node = NodeRef {
                    file: unfold_engine::FileId(index as u32),
                    node: node as u32,
                };
                for decorator in source.decorators(node) {
                    if matches!(source.kind(decorator), Some(ExprKind::Call { .. })) {
                        continue;
                    }
                    let raw = Raw::Expr(decorator);
                    if self.effects_of(raw).iter().any(|effect| effect.registry)
                        || self.origins.may_be(raw, SHARED)
                    {
                        registry_unknown = true;
                    }
                }
            }
        }
        for site in source.sites.all() {
            let call = site.call;
            let Some(callee) = source.callee(call) else {
                continue;
            };
            let imported = self.origins.imported(Some(callee));
            let method_of = |methods: &[&str]| {
                imported.as_deref().is_some_and(|imported| {
                    APPLICATIONS.iter().any(|application| {
                        imported
                            .strip_prefix(application)
                            .and_then(|rest| rest.strip_prefix('.'))
                            .is_some_and(|method| methods.contains(&method))
                    })
                })
            };
            if self.origins.may_be(Raw::Expr(callee), SHARED) || method_of(REGISTRY_METHODS) {
                registry_unknown = true;
            }
            if method_of(LOADERS) {
                let args = source.args(call);
                let receiver = self.origins.receiver(args.get("self", Some(0)));
                match receiver {
                    Some(receiver) if !args.uncertain() => {
                        configuration_unknown.insert(receiver.identity);
                    }
                    _ => registry_unknown = true,
                }
            }
            let written_factory = source.member(callee).is_some_and(|(_, name)| name == "task")
                && source.args(call).positional().next().is_none();
            for effect in self.effects_of(Raw::Expr(callee)) {
                if effect.registry {
                    if !written_factory || effect.receiver.is_none() {
                        registry_unknown = true;
                    }
                } else if let Some(receiver) = effect.receiver {
                    configuration_unknown.insert(receiver.identity);
                }
            }
            let (receiver, factory, targets) = match source.sites.decorated(call) {
                Some(node) => {
                    let Some((receiver, factory)) = self.registration(Some(call)) else {
                        continue;
                    };
                    let display = source
                        .block_of(node)
                        .and_then(|block| binder.program.facts.block(block))
                        .and_then(|block| block.name.clone())
                        .unwrap_or_default();
                    (
                        receiver,
                        factory,
                        vec![self.declaration_target(node, call, display)],
                    )
                }
                None => {
                    let Some((receiver, factory)) = self.registration(Some(callee)) else {
                        continue;
                    };
                    let args = source.args(call);
                    let handler = args.get("fun", Some(0));
                    let targets = match handler {
                        Some(handler) if !args.uncertain() && args.len() == 1 => {
                            vec![self.expression_target(handler)]
                        }
                        _ if args.len() > 0 => args
                            .all()
                            .iter()
                            .map(|argument| Target::Failure(Reason::Unsupported, source.text(argument.expr)))
                            .collect(),
                        _ => vec![Target::Failure(Reason::Unsupported, source.call_text(call))],
                    };
                    (receiver, factory, targets)
                }
            };
            let args = source.args(factory);
            let name = source
                .string(args.get("name", None))
                .filter(|name| !name.is_empty());
            let queue = source
                .string(args.get("queue", None))
                .filter(|queue| !queue.is_empty());
            let uncertain = args.uncertain();
            registrations.push(Registration {
                receiver,
                evidence: site.block,
                targets,
                name: name.map(str::to_owned),
                queue: queue.filter(|_| !uncertain).map(str::to_owned),
                dynamic_name: uncertain || name.is_none(),
                safe_options: !uncertain
                    && args.all().iter().all(|argument| {
                        !argument.expansion
                            && argument
                                .keyword
                                .is_some_and(|keyword| ["name", "queue"].contains(&keyword))
                    }),
            });
        }
        // Celery's default `shared=True` publishes task constructors to every application, so
        // the indexed program is one registration domain.
        let mut names: FxHashMap<&str, u32> = FxHashMap::default();
        for registration in &registrations {
            if registration.dynamic_name {
                registry_unknown = true;
            }
            if let Some(name) = &registration.name {
                *names.entry(name).or_default() += 1;
            }
        }
        registrations
            .iter()
            .map(|registration| {
                let proven = !registry_unknown
                    && self.constructor_proven(&registration.receiver)
                    && registration.safe_options
                    && !configuration_unknown.contains(&registration.receiver.identity)
                    && registration.name.as_deref().is_some_and(|name| {
                        !name.starts_with("celery.") && names.get(name).copied() == Some(1)
                    });
                let targets = registration
                    .targets
                    .iter()
                    .map(|target| self.candidate(target, proven))
                    .collect();
                EntranceCandidate {
                    detector: DetectorId::CeleryTask,
                    kind: EntranceKind::Worker,
                    method: None,
                    trigger: registration
                        .queue
                        .as_deref()
                        .map_or(Trigger::Unknown, |queue| Trigger::Exact(queue.into())),
                    label: None,
                    evidence: CandidateEvidence::Block(registration.evidence),
                    binding: None,
                    targets,
                }
            })
            .collect()
    }

    /// A candidate target; an unproven registration keeps only wrappers as they are.
    fn candidate(&self, target: &Target, proven: bool) -> TargetCandidate {
        let source = self.source();
        match target {
            Target::Failure(Reason::Wrapper, text) => failure(Reason::Wrapper, text.clone()),
            Target::Value(expr) if proven => source.handler(*expr),
            Target::Value(expr) => source.unsupported(*expr),
            Target::Block(block, text) if proven => TargetCandidate::Block {
                block: *block,
                text: Some(text.clone()),
            },
            Target::Block(_, text) => failure(Reason::Unsupported, text.clone()),
            Target::Failure(reason, text) if proven => failure(*reason, text.clone()),
            Target::Failure(_, text) => failure(Reason::Unsupported, text.clone()),
        }
    }
}
