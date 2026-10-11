//! Entrances: detectors find evidence-backed candidates; shared assembly resolves their targets,
//! orders them, and derives their keys.
//!
//! Language crates implement [`EntranceDetector`] and return their detectors from
//! [`Linked::detectors`](crate::Linked::detectors); [`detect`] runs the enabled ones in their
//! fixed order. Manual declarations and package launches produce candidates the same way, from
//! outside the languages. [`assemble`] then:
//!
//! 1. resolves every target independently and keeps every one, in declaration order, even when
//!    it fails: handler expressions through the value policy's handler context (a call result is
//!    `wrapper`, a class is `unsupported`), known blocks by the detector's permitted kinds,
//!    manual locators by [`locate::resolve_locator`], and launch paths through a
//!    [`TargetResolver`];
//! 2. sorts entrances by evidence path, start, end, detector, kind, method, and trigger, keeping
//!    detection order for ties;
//! 3. derives entrance keys, numbering duplicates `#n` in that order; a configured entrance's key
//!    is `entrance:configured:<id>`.

pub mod launches;
pub mod locate;
mod manual;

use unfold_core::key::entrance_keys;
use unfold_core::text::short_label;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Reason, TargetRole, TriggerStatus};
use unfold_core::{ByteRange, Entrance, EntranceTarget, Evidence, Key, RepoPath};

use crate::error::{BackendError, path_of};
use crate::evidence::ValueContext;
use crate::language::Linked;
use crate::locator::Locator;
use crate::program::{BlockRef, FactsSet, FileId, ValueRef};
use crate::provenance::{self, Provenance};
use crate::resolve::{Outcome, Resolver};
use crate::settings::{SETTINGS_PATH, Settings};

pub use manual::manual_candidates;

/// A language's recognizer for one framework's entrances.
pub trait EntranceDetector {
    /// The detector's identity; it must belong to the language that returns it.
    fn id(&self) -> DetectorId;

    /// The candidates this detector finds, in a deterministic order. Detectors recognize only
    /// what provenance proves; lookalikes, wrappers, spreads, and dynamic values become explicit
    /// failed targets or unknown triggers, never guesses.
    fn detect(&self, context: &DetectContext<'_>) -> Vec<EntranceCandidate>;
}

/// What a detector can consult: the program's facts, its language's answers, and provenance.
#[derive(Debug, Clone, Copy)]
pub struct DetectContext<'a> {
    resolver: &'a Resolver<'a>,
}

impl<'a> DetectContext<'a> {
    /// The context over one resolver's program and language.
    pub fn new(resolver: &'a Resolver<'a>) -> Self {
        Self { resolver }
    }

    /// The program's facts.
    pub fn program(&self) -> &'a FactsSet {
        self.resolver.program()
    }

    /// The language's answers.
    pub fn linked(&self) -> &'a dyn Linked {
        self.resolver.linked()
    }

    /// The shared policy, for detectors that need an outcome before deciding on a candidate.
    pub fn resolver(&self) -> &'a Resolver<'a> {
        self.resolver
    }

    /// The bounded import provenance of a value.
    pub fn provenance(&self, value: ValueRef) -> Option<Provenance> {
        provenance::provenance(self.program(), self.linked(), value)
    }

    /// The provenance of a callee's receiver (or of the callee itself when it is not an access).
    pub fn callee_provenance(&self, value: ValueRef) -> Option<Provenance> {
        provenance::callee_provenance(self.program(), self.linked(), value)
    }
}

/// How much of an entrance's trigger is known.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Trigger {
    /// The whole trigger is known.
    Exact(Box<str>),
    /// Only part of it is known.
    Partial(Box<str>),
    /// Nothing is known.
    Unknown,
}

/// Where a candidate entrance is declared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CandidateEvidence {
    /// A registration call, construction, or decorated declaration block.
    Block(BlockRef),
    /// A configuration member: an input's path, the member's range, and its RFC 6901 pointer.
    Input {
        /// The input file.
        path: RepoPath,
        /// The member's byte range.
        range: ByteRange,
        /// The pointer into the parsed value.
        pointer: Box<str>,
    },
    /// An `[[entrance]]` table of `unfold.toml`: its ID, its range, and its pointer.
    Configured {
        /// The table's `id`, which the entrance's key derives from.
        id: Box<str>,
        /// The table's byte range.
        range: ByteRange,
        /// The pointer, `/entrance/<index>`.
        pointer: Box<str>,
    },
}

/// One declared target, before resolution. Its role follows from the entrance kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetCandidate {
    /// A handler expression, resolved in the handler context.
    Value(ValueRef),
    /// A known block, such as a decorated function. Its text defaults to the block's name.
    Block {
        /// The block.
        block: BlockRef,
        /// The target's text, if not the block's name.
        text: Option<Box<str>>,
    },
    /// A manual locator.
    Locator(Locator),
    /// A launch path with its working directory, resolved through a [`TargetResolver`].
    Launch {
        /// The path as written.
        path: Box<str>,
        /// The folder-relative working directory.
        cwd: Box<str>,
    },
    /// A recognized target that cannot resolve, such as a handler after a spread
    /// (`unsupported`) or a wrapper registration (`wrapper`).
    Failure {
        /// Why it cannot resolve.
        reason: Reason,
        /// Its text as written.
        text: Box<str>,
    },
}

/// One detected entrance, before assembly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntranceCandidate {
    /// The detector that found it.
    pub detector: DetectorId,
    /// The entrance kind.
    pub kind: EntranceKind,
    /// The HTTP method of an `http` entrance.
    pub method: Option<HttpMethod>,
    /// The trigger and how much of it is known.
    pub trigger: Trigger,
    /// A display label; manual declarations only.
    pub label: Option<Box<str>>,
    /// Where it is declared.
    pub evidence: CandidateEvidence,
    /// The block binding the application a route's mount chain reaches (see
    /// [`Entrance::binding`]).
    pub binding: Option<BlockRef>,
    /// Its targets in declaration order; at least one.
    pub targets: Vec<TargetCandidate>,
}

/// Resolves the targets the languages don't: launch paths, and whether a missing file exists.
pub trait TargetResolver {
    /// Whether a repository file exists at `path` outside the indexed files, so a locator or
    /// launch naming it is `not_indexed` rather than `not_found`.
    fn is_unindexed_file(&self, path: &RepoPath) -> bool;

    /// Resolves a launch path from a working directory, through the emit map.
    fn launch(&self, path: &str, cwd: &str) -> Outcome;
}

/// Runs the enabled detectors of the resolver's language in their fixed order.
pub fn detect(resolver: &Resolver<'_>, settings: &Settings) -> Vec<EntranceCandidate> {
    let context = DetectContext::new(resolver);
    let language = resolver.language();
    resolver
        .linked()
        .detectors()
        .iter()
        .filter(|detector| {
            detector.id().language() == Some(language) && settings.detector_enabled(detector.id())
        })
        .flat_map(|detector| detector.detect(&context))
        .collect()
}

/// Resolves, orders, and keys entrance candidates given in detection order.
pub fn assemble(
    program: &FactsSet,
    resolvers: &[&Resolver<'_>],
    targets: &dyn TargetResolver,
    candidates: Vec<EntranceCandidate>,
) -> Result<Vec<Entrance>, BackendError> {
    let mut entrances = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let detector = candidate.detector;
        if candidate.targets.is_empty() {
            return Err(BackendError::EmptyEntrance { detector });
        }
        let evidence = evidence(program, &candidate)?;
        let (trigger, trigger_status) = match candidate.trigger {
            Trigger::Exact(text) if !text.is_empty() => (Some(text), TriggerStatus::Exact),
            Trigger::Partial(text) if !text.is_empty() => (Some(text), TriggerStatus::Partial),
            _ => (None, TriggerStatus::Unknown),
        };
        let role = candidate.kind.target_role();
        let mut resolved = Vec::with_capacity(candidate.targets.len());
        for (position, target) in candidate.targets.iter().enumerate() {
            let (outcome, text) = resolve_target(program, resolvers, targets, detector, role, target)?;
            let fields = outcome.into_fields(program);
            resolved.push(EntranceTarget {
                position: position as u32,
                role,
                target: fields.target,
                text: short_label(&text).into(),
                resolution: fields.resolution,
                reason: fields.reason,
                module: fields.module,
            });
        }
        let key = match &candidate.evidence {
            CandidateEvidence::Configured { id, .. } => Key::configured_entrance(id),
            _ => Key::from_text(""),
        };
        entrances.push(Entrance {
            key,
            kind: candidate.kind,
            label: candidate.label.filter(|label| !label.is_empty()),
            method: candidate.method,
            trigger,
            trigger_status,
            detector,
            evidence,
            binding: candidate
                .binding
                .and_then(|block| program.block(block))
                .map(|block| block.key.clone()),
            targets: resolved,
        });
    }
    // A stable sort keeps detection order for entrances that tie on every field.
    entrances.sort_by(Entrance::canonical_cmp);
    let keys = entrance_keys(entrances.iter().map(Entrance::key_shape));
    for (entrance, key) in entrances.iter_mut().zip(keys) {
        entrance.key = key;
    }
    Ok(entrances)
}

fn evidence(program: &FactsSet, candidate: &EntranceCandidate) -> Result<Evidence, BackendError> {
    match &candidate.evidence {
        CandidateEvidence::Block(reference) => {
            let block = program
                .block(*reference)
                .ok_or(BackendError::MissingCandidateFact {
                    detector: candidate.detector,
                })?;
            Ok(Evidence {
                path: path_of(program, reference.file),
                range: block.range,
                block: Some(block.key.clone()),
                pointer: None,
            })
        }
        CandidateEvidence::Input { path, range, pointer } => Ok(Evidence {
            path: path.clone(),
            range: *range,
            block: None,
            pointer: Some(pointer.clone()),
        }),
        CandidateEvidence::Configured { range, pointer, .. } => Ok(Evidence {
            path: RepoPath::new(SETTINGS_PATH).expect("the settings path is a valid folder-relative path"),
            range: *range,
            block: None,
            pointer: Some(pointer.clone()),
        }),
    }
}

fn resolve_target(
    program: &FactsSet,
    resolvers: &[&Resolver<'_>],
    targets: &dyn TargetResolver,
    detector: DetectorId,
    role: TargetRole,
    target: &TargetCandidate,
) -> Result<(Outcome, Box<str>), BackendError> {
    let missing = || BackendError::MissingCandidateFact { detector };
    match target {
        TargetCandidate::Value(value) => {
            let fact = program.value(*value).ok_or_else(missing)?;
            let resolver = resolver_for(program, resolvers, value.file)?;
            let outcome = resolver.resolve_value(*value, ValueContext::Handler)?;
            Ok((outcome, fact.label.clone()))
        }
        TargetCandidate::Block { block, text } => {
            let found = program.block(*block).ok_or_else(missing)?;
            let outcome = if role.target_kinds(detector).contains(&found.kind) {
                Outcome::Resolved(*block)
            } else {
                Outcome::unresolved(Reason::Unsupported)
            };
            let text = text.clone().or_else(|| found.name.clone()).unwrap_or_default();
            Ok((outcome, text))
        }
        TargetCandidate::Locator(locator) => {
            let outcome = locate::resolve_locator(program, resolvers, targets, locator, role)?;
            Ok((outcome, locator.text().into()))
        }
        TargetCandidate::Launch { path, cwd } => Ok((targets.launch(path, cwd), path.clone())),
        TargetCandidate::Failure { reason, text } => Ok((Outcome::unresolved(*reason), text.clone())),
    }
}

/// The resolver for the language that owns `file`.
fn resolver_for<'r, 'a>(
    program: &FactsSet,
    resolvers: &'r [&'r Resolver<'a>],
    file: FileId,
) -> Result<&'r Resolver<'a>, BackendError> {
    resolvers
        .iter()
        .copied()
        .find(|resolver| resolver.owns(file))
        .ok_or_else(|| BackendError::NoLanguage {
            path: path_of(program, file),
        })
}
