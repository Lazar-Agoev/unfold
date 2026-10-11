//! `apscheduler.job`: APScheduler 3 `BlockingScheduler.add_job` registrations on a proven
//! scheduler, with the function and trigger as the first two positional arguments or named.
//!
//! The trigger is exact only for a proven `CronTrigger.from_crontab(...)` call with a literal
//! string of five whitespace-separated fields (and an optional timezone), kept as written.
//! The handler keeps its body only when the scheduler's constructor sets at most `timezone`, the
//! job uses known options with the default job store and executor, its literal ID is unique
//! in its scheduler, and no configuration change, hidden registration, or scheduler or job
//! mutation reaches that scheduler; otherwise it stays `unsupported`. Other scheduler classes,
//! `scheduled_job`, and APScheduler 4 add no entrances.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{DetectorId, EntranceKind, Reason};
use unfold_engine::BlockRef;
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, Trigger};
use unfold_engine::evidence::Refusal;

use super::origins::{DICT_CHANGES, Flow, Identity, Origins, Receiver, Spec};
use super::{Source, failure};
use crate::bind::ir::{Effect, ExprKind, NodeKind};
use crate::link::binder::Ctx;
use crate::link::children::Raw;
use crate::link::program::ExprRef;

const SCHEDULER: &str = "apscheduler";
const BLOCKING: &str = "apscheduler.schedulers.blocking.BlockingScheduler";
const BASE: &str = "apscheduler.schedulers.base.BaseScheduler";
const JOB: &str = "apscheduler.job.Job";
const CRON_FACTORY: &str = "apscheduler.triggers.cron.CronTrigger.from_crontab";

const SPECS: &[Spec] = &[
    Spec {
        family: SCHEDULER,
        kind: "scheduler",
        imports: &[BLOCKING],
        safe_updates: &[],
    },
    Spec {
        family: SCHEDULER,
        kind: "scheduler-base",
        imports: &[BASE],
        safe_updates: &[],
    },
    Spec {
        family: SCHEDULER,
        kind: "job",
        imports: &[JOB],
        safe_updates: &[],
    },
    Spec {
        family: "apscheduler-cron",
        kind: "trigger",
        imports: &["apscheduler.triggers.cron.CronTrigger"],
        safe_updates: &[],
    },
    Spec {
        family: "apscheduler-cron",
        kind: "trigger-factory",
        imports: &[CRON_FACTORY],
        safe_updates: &[],
    },
    Spec {
        family: "apscheduler-setter",
        kind: "setter",
        imports: &["builtins.setattr", "builtins.delattr"],
        safe_updates: &[],
    },
    Spec {
        family: "apscheduler-dict-mutation",
        kind: "dictionary-mutator",
        imports: &[
            "builtins.dict.update",
            "builtins.dict.__setitem__",
            "builtins.dict.__delitem__",
            "builtins.dict.setdefault",
            "builtins.dict.pop",
            "builtins.dict.popitem",
            "builtins.dict.clear",
            "builtins.dict.__ior__",
        ],
        safe_updates: &[],
    },
];

const CONFIGURATION: &[&str] = &[
    "configure",
    "add_jobstore",
    "add_executor",
    "remove_jobstore",
    "remove_executor",
];
const SCHEDULER_CHANGES: &[&str] = &[
    "modify_job",
    "remove_job",
    "remove_all_jobs",
    "reschedule_job",
    "pause_job",
    "resume_job",
    "scheduled_job",
    "import_jobs",
    "_real_add_job",
];
const JOB_CHANGES: &[&str] = &[
    "modify",
    "_modify",
    "remove",
    "reschedule",
    "pause",
    "resume",
    "__setstate__",
];
const JOB_LOOKUPS: &[&str] = &["add_job", "get_job", "get_jobs"];
const JOB_KEYWORDS: &[&str] = &[
    "func",
    "trigger",
    "args",
    "kwargs",
    "id",
    "name",
    "misfire_grace_time",
    "coalesce",
    "max_instances",
    "next_run_time",
    "jobstore",
    "executor",
    "replace_existing",
];

/// The deepest chain of values the walks follow; a longer one withholds everything.
const MAX_DEPTH: u32 = 64;

fn job_change(method: &str) -> bool {
    JOB_CHANGES.contains(&method) || DICT_CHANGES.contains(&method)
}

fn scheduler_method(method: &str) -> bool {
    CONFIGURATION.contains(&method) || SCHEDULER_CHANGES.contains(&method) || method == "add_job"
}

#[derive(Clone)]
enum Target {
    Value(ExprRef),
    Failure(Reason, Box<str>),
}

struct Registration {
    receiver: Receiver,
    evidence: BlockRef,
    target: Target,
    id: Option<String>,
    uncertain_id: bool,
    arguments_proven: bool,
    schedule: Option<String>,
}

/// Where withholding evidence lands: one scheduler's domain, or every domain.
#[derive(Default)]
struct Uncertainty {
    domains: FxHashSet<Identity>,
    global: bool,
}

/// The APScheduler jobs of the program.
pub(super) fn detect(flow: &Flow<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let jobs = Jobs {
        origins: Origins::new(flow, SPECS),
        exhausted: std::cell::Cell::new(false),
        schedulers: RefCell::default(),
    };
    jobs.candidates()
}

struct Jobs<'f, 's, 'b, 'a> {
    origins: Origins<'f, 's, 'b, 'a>,
    /// Set when a walk ran out of depth; every domain is then uncertain.
    exhausted: std::cell::Cell<bool>,
    /// Kept [`Jobs::job_schedulers`] answers.
    schedulers: RefCell<FxHashMap<Raw, Rc<[Receiver]>>>,
}

impl Jobs<'_, '_, '_, '_> {
    fn source(&self) -> &Source<'_, '_> {
        self.origins.source()
    }

    fn families(&self, raw: Raw) -> bool {
        self.origins.may_be(raw, SCHEDULER)
    }

    /// The values a raw may hold: stored values, an assignment's value, a member's
    /// declarations, a name's writes, and an indexed import's module declarations.
    fn values(&self, raw: Raw) -> Vec<Raw> {
        let source = self.source();
        let binder = source.binder;
        let mut found: Vec<Raw> = Vec::new();
        match raw {
            Raw::Expr(expr) => {
                found.extend(self.origins.stored_values(expr).into_iter().map(Raw::Expr));
                match source.kind(expr) {
                    Some(ExprKind::Member { .. }) => found.extend(
                        binder
                            .resolve(expr, &mut Ctx::new())
                            .declarations
                            .iter()
                            .map(|&node| Raw::Node(node)),
                    ),
                    Some(ExprKind::Name(_)) => self.write_values(raw, &mut found),
                    _ => {}
                }
            }
            Raw::Node(node) => match binder.program.node(node).map(|fact| &fact.kind) {
                Some(&NodeKind::Assignment {
                    value: Some(value), ..
                }) => found.push(Raw::Expr(ExprRef {
                    file: node.file,
                    expr: value,
                })),
                Some(NodeKind::Import) => self.write_values(raw, &mut found),
                _ => {}
            },
        }
        found
    }

    fn write_values(&self, raw: Raw, found: &mut Vec<Raw>) {
        let binder = self.source().binder;
        for write in binder.raw_writes(raw) {
            let written = binder.program.write(write);
            found.extend(written.value.map(Raw::Expr));
            let Some(imported) = &written.import else {
                continue;
            };
            if let Some(member) = imported.member.filter(|_| imported.module.external.is_none()) {
                found.extend(
                    binder
                        .module_declarations(&imported.module, member)
                        .into_iter()
                        .map(Raw::Node),
                );
            }
        }
    }

    /// The proven schedulers whose jobs a value may be (`scheduler.add_job(...)`,
    /// `get_job`, `get_jobs`), through containers, stored values, and aliases.
    fn job_schedulers(&self, raw: Raw) -> Rc<[Receiver]> {
        let raw = self.origins.canonical(raw);
        if let Some(found) = self.schedulers.borrow().get(&raw) {
            return Rc::clone(found);
        }
        let mut found = Vec::new();
        self.collect_schedulers(raw, &mut FxHashSet::default(), 0, &mut found);
        let found: Rc<[Receiver]> = found.into();
        self.schedulers.borrow_mut().insert(raw, Rc::clone(&found));
        found
    }

    fn collect_schedulers(&self, raw: Raw, seen: &mut FxHashSet<Raw>, depth: u32, found: &mut Vec<Receiver>) {
        if depth >= MAX_DEPTH {
            self.exhausted.set(true);
            return;
        }
        if !seen.insert(raw) {
            return;
        }
        let source = self.source();
        let next = depth + 1;
        if let Raw::Expr(expr) = raw {
            let local = |id: u32| {
                Raw::Expr(ExprRef {
                    file: expr.file,
                    expr: id,
                })
            };
            match source.kind(expr) {
                Some(ExprKind::Call { .. }) => {
                    let lookup = source
                        .callee(expr)
                        .and_then(|callee| source.member(callee))
                        .filter(|(_, name)| JOB_LOOKUPS.contains(name));
                    if let Some((base, _)) = lookup
                        && let Some(receiver) = self
                            .origins
                            .receiver(Some(base))
                            .filter(|receiver| receiver.spec.kind == "scheduler")
                    {
                        found.push(receiver);
                        return;
                    }
                }
                Some(&(ExprKind::Member { base, .. } | ExprKind::Index { base, .. })) => {
                    self.collect_schedulers(local(base), seen, next, found);
                }
                Some(ExprKind::Sequence(items) | ExprKind::Dict(items)) => {
                    for &item in items {
                        self.collect_schedulers(local(item), seen, next, found);
                    }
                    return;
                }
                Some(&ExprKind::Starred(inner)) => {
                    self.collect_schedulers(local(inner), seen, next, found);
                    return;
                }
                _ => {}
            }
        }
        for value in self.values(raw) {
            self.collect_schedulers(value, seen, next, found);
        }
    }

    /// Scheduler configuration and job changes a callee may make.
    fn effects(
        &self,
        raw: Raw,
        seen: &mut FxHashSet<Raw>,
        stored: bool,
        depth: u32,
        found: &mut Vec<Option<Receiver>>,
    ) {
        if depth >= MAX_DEPTH {
            self.exhausted.set(true);
            return;
        }
        if !seen.insert(raw) {
            return;
        }
        let source = self.source();
        let next = depth + 1;
        if let Raw::Expr(expr) = raw {
            if stored
                && let Some(callee) = source.callee(expr)
                && source
                    .member(callee)
                    .is_some_and(|(_, name)| name == "scheduled_job")
            {
                self.effects(Raw::Expr(callee), seen, false, next, found);
                return;
            }
            if let Some((base, method)) = source.member(expr) {
                if scheduler_method(method) {
                    match self
                        .origins
                        .receiver(Some(base))
                        .filter(|receiver| receiver.spec.kind == "scheduler")
                    {
                        Some(receiver) => found.push(Some(receiver)),
                        None if self.families(Raw::Expr(base)) => found.push(None),
                        None => {}
                    }
                }
                if job_change(method) {
                    let schedulers = self.job_schedulers(Raw::Expr(base));
                    if !schedulers.is_empty() {
                        found.extend(schedulers.iter().copied().map(Some));
                    } else if self.families(Raw::Expr(base)) {
                        found.push(None);
                    }
                }
            }
        }
        for value in self.values(raw) {
            self.effects(value, seen, true, next, found);
        }
    }

    fn effects_of(&self, raw: Raw) -> Vec<Option<Receiver>> {
        let mut found = Vec::new();
        self.effects(raw, &mut FxHashSet::default(), false, 0, &mut found);
        found
    }

    fn constructor_proven(&self, receiver: &Receiver) -> bool {
        let Some(factory) = receiver.factory.filter(|_| !receiver.annotation_only) else {
            return false;
        };
        let args = self.source().args(factory);
        !args.uncertain()
            && args
                .all()
                .iter()
                .all(|argument| !argument.expansion && argument.keyword == Some("timezone"))
    }

    /// The written crontab of a proven `CronTrigger.from_crontab(...)` call.
    fn schedule(&self, trigger: Option<ExprRef>) -> Option<String> {
        let source = self.source();
        let trigger = trigger?;
        if !matches!(source.kind(trigger), Some(ExprKind::Call { .. })) {
            return None;
        }
        if self.origins.trusted_import(source.callee(trigger)).as_deref() != Some(CRON_FACTORY) {
            return None;
        }
        let args = source.args(trigger);
        let expression = args.get("expr", Some(0));
        args.get("timezone", Some(1));
        let known = args.all().iter().all(|argument| {
            argument
                .keyword
                .is_none_or(|keyword| ["expr", "timezone"].contains(&keyword))
        });
        if args.uncertain() || args.positional().count() > 2 || !known {
            return None;
        }
        let literal = source.string(expression)?;
        (literal.split_whitespace().count() == 5).then(|| literal.to_owned())
    }

    fn handler(&self, expr: ExprRef) -> Target {
        let source = self.source();
        // A string names an import path; indexing never imports it.
        if source.string_like(expr) {
            return Target::Failure(Reason::Unsupported, source.text(expr));
        }
        if source.binder.resolve(expr, &mut Ctx::new()).refusal == Some(Refusal::Wrapper) {
            return Target::Failure(Reason::Wrapper, source.text(expr));
        }
        Target::Value(expr)
    }

    fn withhold_job(&self, raw: Raw, uncertainty: &mut Uncertainty) {
        let schedulers = self.job_schedulers(raw);
        if !schedulers.is_empty() {
            uncertainty
                .domains
                .extend(schedulers.iter().map(|receiver| receiver.identity));
        } else if self.families(raw) {
            uncertainty.global = true;
        }
    }

    fn written_target(&self, expr: ExprRef, uncertainty: &mut Uncertainty, depth: u32) {
        if depth >= MAX_DEPTH {
            self.exhausted.set(true);
            return;
        }
        let source = self.source();
        let local = |id: u32| ExprRef {
            file: expr.file,
            expr: id,
        };
        match source.kind(expr) {
            Some(&(ExprKind::Member { base, .. } | ExprKind::Index { base, .. })) => {
                self.withhold_job(Raw::Expr(local(base)), uncertainty);
            }
            Some(ExprKind::Sequence(items)) => {
                for &item in items {
                    self.written_target(local(item), uncertainty, depth + 1);
                }
            }
            Some(&ExprKind::Starred(inner)) => self.written_target(local(inner), uncertainty, depth + 1),
            _ => {}
        }
    }

    fn candidates(&self) -> Vec<EntranceCandidate> {
        let source = self.source();
        let binder = source.binder;
        let mut uncertainty = Uncertainty::default();
        for (index, bind) in binder.program.files.iter().enumerate() {
            let Some(bind) = bind else {
                continue;
            };
            let file = unfold_engine::FileId(index as u32);
            for effect in &bind.effects {
                if let &Effect::Target {
                    target, augmented, ..
                } = effect
                {
                    let target = ExprRef { file, expr: target };
                    self.written_target(target, &mut uncertainty, 0);
                    if augmented {
                        self.withhold_job(Raw::Expr(target), &mut uncertainty);
                    }
                }
            }
        }
        let mut registrations: Vec<Registration> = Vec::new();
        for site in source.sites.all() {
            let call = site.call;
            let Some(callee) = source.callee(call) else {
                continue;
            };
            let setter = self.origins.may_be(Raw::Expr(callee), "apscheduler-setter");
            if setter
                || self
                    .origins
                    .may_be(Raw::Expr(callee), "apscheduler-dict-mutation")
            {
                let args = source.args(call);
                let object = args.get(if setter { "object" } else { "self" }, Some(0));
                match object {
                    Some(object) if !args.uncertain() => {
                        self.withhold_job(Raw::Expr(object), &mut uncertainty);
                    }
                    _ => {
                        for argument in args.all() {
                            self.withhold_job(Raw::Expr(argument.expr), &mut uncertainty);
                        }
                    }
                }
            }
            let direct = source
                .member(callee)
                .filter(|(_, name)| *name == "add_job")
                .map(|(base, _)| base);
            let receiver = direct
                .and_then(|base| self.origins.receiver(Some(base)))
                .filter(|receiver| receiver.spec.kind == "scheduler");
            let imported = self.origins.imported(Some(callee));
            for identity in [BLOCKING, BASE, JOB] {
                let method = imported
                    .as_deref()
                    .and_then(|imported| imported.strip_prefix(identity))
                    .and_then(|rest| rest.strip_prefix('.'));
                let applies = method.is_some_and(|method| {
                    if identity == JOB {
                        job_change(method)
                    } else {
                        scheduler_method(method)
                    }
                });
                if !applies {
                    continue;
                }
                let args = source.args(call);
                let own = args.get("self", Some(0));
                let related: Vec<Receiver> = match own {
                    Some(own) if identity == JOB => self.job_schedulers(Raw::Expr(own)).to_vec(),
                    Some(own) => self
                        .origins
                        .receiver(Some(own))
                        .filter(|receiver| receiver.spec.kind == "scheduler")
                        .into_iter()
                        .collect(),
                    None => Vec::new(),
                };
                if !args.uncertain() && !related.is_empty() {
                    uncertainty
                        .domains
                        .extend(related.into_iter().map(|receiver| receiver.identity));
                } else {
                    uncertainty.global = true;
                }
            }
            for effect in self.effects_of(Raw::Expr(callee)) {
                match effect {
                    Some(found) if receiver.is_some_and(|receiver| receiver.identity == found.identity) => {}
                    Some(found) => {
                        uncertainty.domains.insert(found.identity);
                    }
                    None => uncertainty.global = true,
                }
            }
            let Some(receiver) = receiver else {
                continue;
            };
            let args = source.args(call);
            let func = args.get("func", Some(0));
            let trigger = args.get("trigger", Some(1));
            let id_value = args.get("id", None);
            let id = source.string(id_value);
            let random_id = id_value.is_none()
                || id == Some("")
                || id_value.is_some_and(|value| {
                    matches!(source.kind(value), Some(ExprKind::Other)) && &*source.text(value) == "None"
                });
            let positional = args.positional().count();
            let keywords_known = args.all().iter().all(|argument| {
                argument
                    .keyword
                    .is_none_or(|keyword| JOB_KEYWORDS.contains(&keyword))
            });
            let defaults = ["jobstore", "executor"].iter().all(|name| {
                let value = args.get(name, None);
                value.is_none() || source.string(value) == Some("default")
            });
            let uncertain = args.uncertain();
            registrations.push(Registration {
                receiver,
                evidence: site.block,
                target: match func {
                    Some(func) => self.handler(func),
                    None => Target::Failure(Reason::Unsupported, source.call_text(call)),
                },
                id: id.filter(|id| !id.is_empty()).map(str::to_owned),
                uncertain_id: uncertain || positional > 2 || !random_id && id.is_none(),
                arguments_proven: !uncertain && positional <= 2 && keywords_known && defaults,
                schedule: if uncertain { None } else { self.schedule(trigger) },
            });
        }
        let mut ids: FxHashMap<(Identity, &str), u32> = FxHashMap::default();
        for registration in &registrations {
            if registration.uncertain_id {
                uncertainty.domains.insert(registration.receiver.identity);
            }
            if let Some(id) = &registration.id {
                *ids.entry((registration.receiver.identity, id)).or_default() += 1;
            }
        }
        let global = uncertainty.global || self.exhausted.get();
        registrations
            .iter()
            .map(|registration| {
                let identity = registration.receiver.identity;
                let proven = !global
                    && self.constructor_proven(&registration.receiver)
                    && registration.arguments_proven
                    && !uncertainty.domains.contains(&identity)
                    && registration
                        .id
                        .as_deref()
                        .is_none_or(|id| ids.get(&(identity, id)).copied() == Some(1));
                let target = match &registration.target {
                    Target::Failure(reason, text) if proven || *reason == Reason::Wrapper => {
                        failure(*reason, text.clone())
                    }
                    Target::Failure(_, text) => failure(Reason::Unsupported, text.clone()),
                    Target::Value(expr) if proven => source.handler(*expr),
                    Target::Value(expr) => source.unsupported(*expr),
                };
                EntranceCandidate {
                    detector: DetectorId::ApschedulerJob,
                    kind: EntranceKind::Schedule,
                    method: None,
                    trigger: registration
                        .schedule
                        .as_deref()
                        .map_or(Trigger::Unknown, |schedule| Trigger::Exact(schedule.into())),
                    label: None,
                    evidence: CandidateEvidence::Block(registration.evidence),
                    binding: None,
                    targets: vec![target],
                }
            })
            .collect()
    }
}
