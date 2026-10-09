//! The Python entrance detectors (architecture §6.6): `pyproject.script`, `fastapi.route`,
//! `celery.task`, and `apscheduler.job`, carried over from the proof of concept's
//! `frontend/python/detectors/` with their exact bounds.
//!
//! Every detector recognizes only what the binder proves. Framework objects are recognized by
//! proven imported identity ([`origins`]): a stable constructor call, an alias or re-export of
//! one, or a written annotation, and never by spelling. A broader negative census over every
//! indexed write withholds that proof when anything could replace the framework API.
//! Lookalikes, wrappers, expansions, and dynamic values become explicit failed targets or
//! unknown triggers, never guesses. Detectors only add entrances: blocks, slots, and edges are
//! the same with them on or off.
//!
//! See `docs/entrances.md` for each detector's forms, bounds, evidence, and refusals.

mod apscheduler;
mod celery;
mod fastapi;
mod origins;
pub(crate) mod pyproject;
pub(crate) mod scripts;
mod sites;

use std::cell::Cell;

use unfold_core::vocab::{DetectorId, Reason};
use unfold_engine::entrances::{DetectContext, EntranceCandidate, EntranceDetector, TargetCandidate};
use unfold_engine::settings::Settings;

use crate::bind::ir::{Argument, ExprKind, NodeKind};
use crate::link::PythonLinked;
use crate::link::binder::Binder;
use crate::link::program::{ExprRef, NodeRef};
use origins::Flow;

pub(crate) use sites::Sites;

/// The detectors, in the fixed order they run.
pub(crate) fn detectors<'l>(linked: &'l PythonLinked<'_>) -> Vec<Box<dyn EntranceDetector + 'l>> {
    vec![
        Box::new(Detector {
            id: DetectorId::PyprojectScript,
            linked,
        }),
        Box::new(Detector {
            id: DetectorId::FastapiRoute,
            linked,
        }),
        Box::new(Detector {
            id: DetectorId::CeleryTask,
            linked,
        }),
        Box::new(Detector {
            id: DetectorId::ApschedulerJob,
            linked,
        }),
    ]
}

struct Detector<'l, 'a> {
    id: DetectorId,
    linked: &'l PythonLinked<'a>,
}

impl EntranceDetector for Detector<'_, '_> {
    fn id(&self) -> DetectorId {
        self.id
    }

    fn detect(&self, _context: &DetectContext<'_>) -> Vec<EntranceCandidate> {
        let linked = self.linked;
        match self.id {
            DetectorId::PyprojectScript => linked.scripts().candidates.clone(),
            DetectorId::FastapiRoute => linked.source_candidates().fastapi.clone(),
            DetectorId::CeleryTask => linked.source_candidates().celery.clone(),
            DetectorId::ApschedulerJob => linked.source_candidates().apscheduler.clone(),
            _ => Vec::new(),
        }
    }
}

/// The source detectors' candidates, found together over one shared [`Flow`].
#[derive(Debug, Default)]
pub(crate) struct SourceCandidates {
    pub(crate) fastapi: Vec<EntranceCandidate>,
    pub(crate) celery: Vec<EntranceCandidate>,
    pub(crate) apscheduler: Vec<EntranceCandidate>,
}

/// Runs the enabled source detectors. The value flow they share is computed only when an
/// enabled detector's framework is imported somewhere.
pub(crate) fn source_candidates(source: &Source<'_, '_>, settings: &Settings) -> SourceCandidates {
    let wanted =
        |detector: DetectorId, package: &str| settings.detector_enabled(detector) && source.imports(package);
    let fastapi = wanted(DetectorId::FastapiRoute, "fastapi");
    let celery = wanted(DetectorId::CeleryTask, "celery");
    let apscheduler = wanted(DetectorId::ApschedulerJob, "apscheduler");
    if !fastapi && !celery && !apscheduler {
        return SourceCandidates::default();
    }
    let flow = Flow::new(source);
    let run = |enabled: bool, detect: fn(&Flow<'_, '_, '_>) -> Vec<EntranceCandidate>| {
        if enabled { detect(&flow) } else { Vec::new() }
    };
    SourceCandidates {
        fastapi: run(fastapi, fastapi::detect),
        celery: run(celery, celery::detect),
        apscheduler: run(apscheduler, apscheduler::detect),
    }
}

/// What the source detectors read: the frozen binder and the program's call sites.
pub(crate) struct Source<'b, 'a> {
    pub(crate) binder: &'b Binder<'a>,
    pub(crate) sites: &'b Sites,
}

impl<'b> Source<'b, '_> {
    /// An expression's kind.
    pub(crate) fn kind(&self, expr: ExprRef) -> Option<&'b ExprKind> {
        self.binder.program.expr(expr).map(|fact| &fact.kind)
    }

    /// The text of a name of `file`'s table.
    pub(crate) fn name(&self, file: unfold_engine::FileId, name: u32) -> &'b str {
        let program = &self.binder.program;
        program.text(program.sym(file, name))
    }

    /// Whether any file imports a module of the package `root` (`fastapi`, `celery`), which
    /// every framework identity of that package starts from.
    pub(crate) fn imports(&self, root: &str) -> bool {
        self.binder.program.writes.iter().any(|write| {
            write
                .import
                .as_ref()
                .and_then(|imported| imported.module.external.as_deref())
                == Some(root)
        })
    }

    /// The decoded text of a plain string literal: never bytes or an f-string.
    pub(crate) fn string(&self, expr: Option<ExprRef>) -> Option<&'b str> {
        let expr = expr?;
        match self.kind(expr)? {
            &ExprKind::Str(text) => Some(self.name(expr.file, text)),
            _ => None,
        }
    }

    /// Whether an expression is a string literal of any kind (plain, bytes, or f-string).
    pub(crate) fn string_like(&self, expr: ExprRef) -> bool {
        if matches!(self.kind(expr), Some(ExprKind::Str(_))) {
            return true;
        }
        let text = self.text(expr);
        let body = text.trim_start_matches(['r', 'R', 'b', 'B', 'u', 'U', 'f', 'F', 't', 'T']);
        matches!(self.kind(expr), Some(ExprKind::Other)) && (body.starts_with('"') || body.starts_with('\''))
    }

    /// An expression's written text, from its value-table label.
    pub(crate) fn text(&self, expr: ExprRef) -> Box<str> {
        self.sites
            .value(expr)
            .and_then(|value| self.binder.program.facts.value(value))
            .map_or_else(|| "…".into(), |fact| fact.label.clone())
    }

    /// A call's written text. Calls are recorded by their callee, so the text is rebuilt from
    /// it: exact for a call without arguments, abbreviated otherwise.
    pub(crate) fn call_text(&self, call: ExprRef) -> Box<str> {
        match self.kind(call) {
            Some(ExprKind::Call {
                callee, arguments, ..
            }) => {
                let callee = self.text(ExprRef {
                    file: call.file,
                    expr: *callee,
                });
                let arguments = if arguments.is_empty() { "" } else { "…" };
                format!("{callee}({arguments})").into()
            }
            _ => self.text(call),
        }
    }

    /// The written arguments of a call expression.
    pub(crate) fn args(&self, call: ExprRef) -> Args<'b> {
        match self.kind(call) {
            Some(ExprKind::Call { arguments, .. }) => self.written(call.file, arguments),
            _ => Args::default(),
        }
    }

    /// Written arguments (of a call or a class header).
    fn written(&self, file: unfold_engine::FileId, arguments: &[Argument]) -> Args<'b> {
        let written = arguments
            .iter()
            .map(|argument| Written {
                keyword: argument.keyword.map(|keyword| self.name(file, keyword)),
                expr: ExprRef {
                    file,
                    expr: argument.value,
                },
                expansion: argument.expansion,
            })
            .collect();
        Args::new(written)
    }

    /// A handler expression, resolved by the shared handler policy.
    pub(crate) fn handler(&self, expr: ExprRef) -> TargetCandidate {
        match self.sites.value(expr) {
            Some(value) => TargetCandidate::Value(value),
            None => self.unsupported(expr),
        }
    }

    /// A recognized target that stays `unsupported`, with its written text.
    pub(crate) fn unsupported(&self, expr: ExprRef) -> TargetCandidate {
        failure(Reason::Unsupported, self.text(expr))
    }

    /// The callee of a call expression.
    pub(crate) fn callee(&self, call: ExprRef) -> Option<ExprRef> {
        match self.kind(call)? {
            &ExprKind::Call { callee, .. } => Some(ExprRef {
                file: call.file,
                expr: callee,
            }),
            _ => None,
        }
    }

    /// `receiver.name` as `(receiver, name)`.
    pub(crate) fn member(&self, expr: ExprRef) -> Option<(ExprRef, &'b str)> {
        match self.kind(expr)? {
            &ExprKind::Member { base, name } => Some((
                ExprRef {
                    file: expr.file,
                    expr: base,
                },
                self.name(expr.file, name),
            )),
            _ => None,
        }
    }

    /// The decorator expressions of a `def` or `class`, in written order.
    pub(crate) fn decorators(&self, node: NodeRef) -> Vec<ExprRef> {
        let decorators = match self.binder.program.node(node).map(|fact| &fact.kind) {
            Some(NodeKind::Function(function)) => &function.decorators,
            Some(NodeKind::Class(class)) => &class.decorators,
            _ => return Vec::new(),
        };
        decorators
            .iter()
            .map(|&expr| ExprRef {
                file: node.file,
                expr,
            })
            .collect()
    }

    /// The block of a `def`.
    pub(crate) fn block_of(&self, node: NodeRef) -> Option<unfold_engine::BlockRef> {
        match &self.binder.program.node(node)?.kind {
            NodeKind::Function(function) => Some(unfold_engine::BlockRef {
                file: node.file,
                local: function.block,
            }),
            _ => None,
        }
    }

    /// The classes around a declaration, innermost first (the class stack whose decorators
    /// apply to a method).
    pub(crate) fn enclosing_classes(&self, node: NodeRef) -> Vec<NodeRef> {
        let program = &self.binder.program;
        let mut classes = Vec::new();
        let mut current = program.enclosing_definition(node);
        while let Some(owner) = current {
            if program.is_class(owner) {
                classes.push(owner);
            }
            current = program.enclosing_definition(owner);
        }
        classes
    }
}

/// A failed target.
pub(crate) fn failure(reason: Reason, text: Box<str>) -> TargetCandidate {
    TargetCandidate::Failure { reason, text }
}

/// One written argument.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Written<'b> {
    /// The keyword of `name=value`.
    pub(crate) keyword: Option<&'b str>,
    /// The value (for `*x` and `**x`, the expanded expression).
    pub(crate) expr: ExprRef,
    /// Whether it is `*x` or `**x`.
    pub(crate) expansion: bool,
}

/// A call's written arguments, bound by name and position the way the proof of concept did:
/// a parameter written both by keyword and by position, a repeated keyword, or any expansion
/// makes the binding uncertain.
#[derive(Debug, Default)]
pub(crate) struct Args<'b> {
    written: Vec<Written<'b>>,
    duplicate: Cell<bool>,
}

impl<'b> Args<'b> {
    fn new(written: Vec<Written<'b>>) -> Self {
        let keywords: Vec<&str> = written.iter().filter_map(|argument| argument.keyword).collect();
        let repeated = keywords
            .iter()
            .enumerate()
            .any(|(index, keyword)| keywords[..index].contains(keyword));
        Self {
            written,
            duplicate: Cell::new(repeated),
        }
    }

    /// Every written argument, in order.
    pub(crate) fn all(&self) -> &[Written<'b>] {
        &self.written
    }

    /// The positional arguments, in order.
    pub(crate) fn positional(&self) -> impl Iterator<Item = ExprRef> + '_ {
        self.written
            .iter()
            .filter(|argument| argument.keyword.is_none() && !argument.expansion)
            .map(|argument| argument.expr)
    }

    /// The expanded arguments (`*x`, `**x`).
    pub(crate) fn expansions(&self) -> impl Iterator<Item = ExprRef> + '_ {
        self.written
            .iter()
            .filter(|argument| argument.expansion)
            .map(|argument| argument.expr)
    }

    /// The value of the keyword `name`, if written.
    pub(crate) fn keyword(&self, name: &str) -> Option<ExprRef> {
        self.written
            .iter()
            .rev()
            .find(|argument| argument.keyword == Some(name))
            .map(|argument| argument.expr)
    }

    /// The argument for the parameter `name` (at `position`, if it can be positional). One
    /// written both ways is a duplicate: no value, and the binding becomes uncertain.
    pub(crate) fn get(&self, name: &str, position: Option<usize>) -> Option<ExprRef> {
        let keyword = self.keyword(name);
        let positional = position.and_then(|position| self.positional().nth(position));
        if keyword.is_some() && positional.is_some() {
            self.duplicate.set(true);
            return None;
        }
        keyword.or(positional)
    }

    /// Whether the binding is uncertain: an expansion or a duplicate.
    pub(crate) fn uncertain(&self) -> bool {
        self.duplicate.get() || self.written.iter().any(|argument| argument.expansion)
    }

    /// How many arguments are written.
    pub(crate) fn len(&self) -> usize {
        self.written.len()
    }
}
