//! The TypeScript entrance detectors (architecture §6.5, §6.6): `package.script`, `package.bin`,
//! `bullmq.worker`, `express.route`, and `node-cron.schedule`, carried over from the proof of
//! concept's package launches and `frontend/typescript-shared/detectors/` with their exact bounds.
//!
//! Package detectors read `package.json` inputs ([`packages`]). The source detectors recognize
//! framework objects only through the engine's bounded import provenance
//! ([`DetectContext::provenance`]), never by spelling; a lookalike local name, a wrapper, a
//! spread, or a dynamic value becomes an explicit failed target or an unknown trigger, never a
//! guess. Handlers resolve through the shared handler policy. Detectors only add entrances:
//! blocks, slots, and edges are the same with them on or off.
//!
//! See `docs/entrances.md` for each detector's forms, bounds, evidence, and refusals.

mod bullmq;
mod cron;
mod express;
pub(crate) mod json;
pub(crate) mod packages;

use unfold_core::vocab::{BlockKind, DetectorId, Reason};
use unfold_engine::entrances::{
    DetectContext, EntranceCandidate, EntranceDetector, TargetCandidate, Trigger,
};
use unfold_engine::facts::CallFact;
use unfold_engine::provenance::Provenance;
use unfold_engine::{BlockRef, FileId, ValueRef};

use crate::link::TypeScriptLinked;
use crate::payload::{ArgumentSyntax, ImportSource, Site};

/// The detectors, in the fixed order they run.
pub(crate) fn detectors<'l>(linked: &'l TypeScriptLinked<'_>) -> Vec<Box<dyn EntranceDetector + 'l>> {
    [
        DetectorId::PackageScript,
        DetectorId::PackageBin,
        DetectorId::BullmqWorker,
        DetectorId::ExpressRoute,
        DetectorId::NodeCronSchedule,
    ]
    .into_iter()
    .map(|id| Box::new(Detector { id, linked }) as Box<dyn EntranceDetector + 'l>)
    .collect()
}

struct Detector<'l, 'a> {
    id: DetectorId,
    linked: &'l TypeScriptLinked<'a>,
}

impl EntranceDetector for Detector<'_, '_> {
    fn id(&self) -> DetectorId {
        self.id
    }

    fn detect(&self, context: &DetectContext<'_>) -> Vec<EntranceCandidate> {
        let source = Source {
            context: *context,
            linked: self.linked,
        };
        match self.id {
            DetectorId::PackageScript => self.linked.packages().scripts.clone(),
            DetectorId::PackageBin => self.linked.packages().bins.clone(),
            DetectorId::BullmqWorker => bullmq::detect(&source),
            DetectorId::ExpressRoute => express::detect(&source),
            DetectorId::NodeCronSchedule => cron::detect(&source),
            _ => Vec::new(),
        }
    }
}

/// An argument with nothing recorded beyond its value.
static PLAIN: ArgumentSyntax = ArgumentSyntax::Plain;

/// What the source detectors read: the linked program and provenance.
pub(crate) struct Source<'c, 'l, 'a> {
    context: DetectContext<'c>,
    linked: &'l TypeScriptLinked<'a>,
}

/// One call or construction of the program.
#[derive(Clone, Copy)]
pub(crate) struct Call<'s> {
    file: FileId,
    fact: &'s CallFact,
    site: Option<&'s Site>,
}

/// One written argument.
#[derive(Clone, Copy)]
pub(crate) struct Argument<'s> {
    /// Its value.
    pub(crate) value: ValueRef,
    /// Whether it is a spread (`...args`).
    pub(crate) spread: bool,
    /// What extraction recorded about its syntax.
    pub(crate) syntax: &'s ArgumentSyntax,
}

impl<'s> Call<'s> {
    /// The call's block: the entrance's evidence.
    pub(crate) fn block(&self) -> BlockRef {
        BlockRef {
            file: self.file,
            local: self.fact.block,
        }
    }

    /// The callee's value.
    pub(crate) fn callee(&self) -> Option<ValueRef> {
        self.fact.callee.map(|value| ValueRef {
            file: self.file,
            value,
        })
    }

    /// The number of written arguments, spreads included.
    pub(crate) fn len(&self) -> usize {
        self.fact.arguments.len()
    }

    /// The written argument at `position`.
    pub(crate) fn argument(&self, position: usize) -> Option<Argument<'s>> {
        let fact = self.fact.arguments.get(position)?;
        let syntax = self
            .site
            .and_then(|site| site.arguments.get(position))
            .unwrap_or(&PLAIN);
        Some(Argument {
            value: ValueRef {
                file: self.file,
                value: fact.value,
            },
            spread: !fact.reference,
            syntax,
        })
    }

    /// The written arguments from `start` on.
    pub(crate) fn arguments_from(&self, start: usize) -> impl Iterator<Item = Argument<'s>> + '_ {
        (start..self.len()).filter_map(|position| self.argument(position))
    }
}

impl<'s> Argument<'s> {
    /// The decoded text of a string or no-substitution template literal.
    pub(crate) fn literal(&self) -> Option<&'s str> {
        match self.syntax {
            ArgumentSyntax::Literal(text) => Some(text),
            _ => None,
        }
    }
}

impl<'c, 'l, 'a> Source<'c, 'l, 'a> {
    /// Whether each file (by ID) imports from `package`, by module label. Every framework
    /// identity a detector recognizes starts at such an import.
    pub(crate) fn importers(&self, package: &str) -> Vec<bool> {
        let program = self.linked.program();
        program
            .iter()
            .map(|(file, _)| {
                self.linked.module_facts(file).is_some_and(|facts| {
                    facts.imports.iter().any(|binding| match &binding.source {
                        ImportSource::Module { specifier, .. } => {
                            crate::labels::is_package_specifier(specifier)
                                && crate::labels::label_for_specifier(specifier) == package
                        }
                        ImportSource::Qualified(_) => false,
                    })
                })
            })
            .collect()
    }

    /// The calls (or constructions, by `kind`) of the files selected by `files`, in file order
    /// and then written order.
    pub(crate) fn calls(&self, files: &[bool], kind: BlockKind) -> Vec<Call<'l>> {
        let program = self.linked.program();
        let mut calls = Vec::new();
        for (file, _) in program.iter() {
            if !files.get(file.index()).copied().unwrap_or(false) {
                continue;
            }
            let (Some(facts), Some(payload)) = (program.facts(file), self.linked.payload(file)) else {
                continue;
            };
            let blocks = facts.graph.blocks();
            for fact in facts.graph.calls() {
                if blocks.get(fact.block as usize).map(|block| block.kind) != Some(kind) {
                    continue;
                }
                let site = payload
                    .sites
                    .binary_search_by_key(&fact.block, |site| site.block)
                    .ok()
                    .map(|index| &payload.sites[index]);
                calls.push(Call { file, fact, site });
            }
        }
        calls
    }

    /// The bounded import provenance of a value.
    pub(crate) fn provenance(&self, value: ValueRef) -> Option<Provenance> {
        self.context.provenance(value)
    }

    /// A value's member name, for a property access.
    pub(crate) fn member(&self, value: ValueRef) -> Option<&'l str> {
        self.linked.program().value(value)?.member.as_deref()
    }

    /// A property access's receiver.
    pub(crate) fn receiver(&self, value: ValueRef) -> Option<ValueRef> {
        let receiver = self.linked.program().value(value)?.receiver?;
        Some(ValueRef {
            file: value.file,
            value: receiver,
        })
    }

    /// The linked program.
    pub(crate) fn linked(&self) -> &'l TypeScriptLinked<'a> {
        self.linked
    }

    /// A value's written label.
    pub(crate) fn label(&self, value: ValueRef) -> Box<str> {
        self.linked.program().value(value).map_or_else(
            || unfold_engine::edges::text::OTHER.into(),
            |fact| fact.label.clone(),
        )
    }

    /// A target that stays unresolved for `reason`, with the value's written label.
    pub(crate) fn failure(&self, reason: Reason, value: ValueRef) -> TargetCandidate {
        failure(reason, self.label(value))
    }
}

/// A failed target with its text.
pub(crate) fn failure(reason: Reason, text: Box<str>) -> TargetCandidate {
    TargetCandidate::Failure { reason, text }
}

/// A trigger from an optional literal: exact when written, unknown otherwise (the assembler
/// makes an empty literal unknown too).
pub(crate) fn literal_trigger(literal: Option<&str>) -> Trigger {
    literal.map_or(Trigger::Unknown, |text| Trigger::Exact(text.into()))
}

/// Whether `provenance` is exactly `module`'s `export` followed by `path`.
pub(crate) fn proves(provenance: Option<&Provenance>, module: &str, export: &str, path: &[&str]) -> bool {
    provenance.is_some_and(|found| {
        found.module == module
            && &*found.export == export
            && found.path.len() == path.len()
            && found
                .path
                .iter()
                .zip(path)
                .all(|(step, expected)| &**step == *expected)
    })
}
