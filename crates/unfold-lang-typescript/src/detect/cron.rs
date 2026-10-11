//! `node-cron.schedule`: `schedule(expression, handler)` where provenance proves the
//! `node-cron` package's named `schedule` export (any local alias), or `schedule` read from its
//! default or namespace import (`cron.schedule`). Transparent wrappers participate; a computed
//! access (`cron["schedule"]`), a construction, and lookalike modules don't.
//!
//! The expression literal is the trigger; the second argument is one handler through the shared
//! handler policy, `unsupported` after a leading spread. Evidence is the call.

use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind, Reason};
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate};
use unfold_engine::facts::ValueShape;

use super::{Source, literal_trigger, proves};

/// The schedules of the program.
pub(super) fn detect(source: &Source<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let importers = source.importers("node-cron");
    if !importers.contains(&true) {
        return Vec::new();
    }
    let program = source.linked().program();
    let mut candidates = Vec::new();
    for call in source.calls(&importers, BlockKind::Call) {
        let Some(callee) = call.callee() else {
            continue;
        };
        // Only a name or a `schedule` member can carry either provenance.
        let shape = program.value(callee).map(|fact| fact.shape);
        if shape != Some(ValueShape::Name) && source.member(callee) != Some("schedule") {
            continue;
        }
        let provenance = source.provenance(callee);
        let provenance = provenance.as_ref();
        let named = proves(provenance, "node-cron", "schedule", &[]);
        let member = proves(provenance, "node-cron", "default", &["schedule"])
            || proves(provenance, "node-cron", "*", &["schedule"]);
        if !named && !member {
            continue;
        }
        let (Some(expression), Some(handler)) = (call.argument(0), call.argument(1)) else {
            continue;
        };
        let target = if expression.spread {
            source.failure(Reason::Unsupported, handler.value)
        } else {
            TargetCandidate::Value(handler.value)
        };
        candidates.push(EntranceCandidate {
            detector: DetectorId::NodeCronSchedule,
            kind: EntranceKind::Schedule,
            method: None,
            trigger: literal_trigger(expression.literal()),
            label: None,
            evidence: CandidateEvidence::Block(call.block()),
            binding: None,
            targets: vec![target],
        });
    }
    candidates
}
