//! `bullmq.worker`: `new Worker(queue, processor)` where provenance proves `Worker` is the
//! `bullmq` package's named export (imported aliases and transparent wrappers included; a
//! namespace `bullmq.Worker`, a local `Worker`, `Queue`, `QueueEvents`, and `FlowProducer` are
//! not).
//!
//! The queue literal is the trigger. The processor is one handler: a function or reference
//! through the shared handler policy; a string (a sandboxed processor file) stays `unsupported`
//! with its decoded text; a `new URL(…)` whose constructor provenance proves `node:url`'s `URL`
//! stays `unsupported` with its written text. After a leading spread the processor's position is
//! uncertain, so it is `unsupported`. Evidence is the construction.

use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind, Reason};
use unfold_engine::ValueRef;
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate};

use super::{Argument, Source, failure, literal_trigger, proves};
use crate::payload::{ArgumentSyntax, ValueSyntax};

/// The workers of the program.
pub(super) fn detect(source: &Source<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let importers = source.importers("bullmq");
    if !importers.contains(&true) {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for call in source.calls(&importers, BlockKind::New) {
        let Some(callee) = call.callee() else {
            continue;
        };
        if !proves(source.provenance(callee).as_ref(), "bullmq", "Worker", &[]) {
            continue;
        }
        let (Some(queue), Some(processor)) = (call.argument(0), call.argument(1)) else {
            continue;
        };
        let target = if queue.spread {
            source.failure(Reason::Unsupported, processor.value)
        } else {
            processor_target(source, processor)
        };
        candidates.push(EntranceCandidate {
            detector: DetectorId::BullmqWorker,
            kind: EntranceKind::Worker,
            method: None,
            trigger: literal_trigger(queue.literal()),
            label: None,
            evidence: CandidateEvidence::Block(call.block()),
            binding: None,
            targets: vec![target],
        });
    }
    candidates
}

/// The processor target: a sandbox file path or a proven `node:url` `URL` stays `unsupported`.
fn processor_target(source: &Source<'_, '_, '_>, processor: Argument<'_>) -> TargetCandidate {
    match processor.syntax {
        ArgumentSyntax::Literal(path) => failure(Reason::Unsupported, path.clone()),
        ArgumentSyntax::Construction(written) if constructs_url(source, processor.value) => {
            failure(Reason::Unsupported, written.clone())
        }
        _ => TargetCandidate::Value(processor.value),
    }
}

/// Whether a construction's constructor is proven to be `node:url`'s `URL`.
fn constructs_url(source: &Source<'_, '_, '_>, construction: ValueRef) -> bool {
    let payload = source.linked().payload(construction.file);
    let constructor = match payload.and_then(|payload| payload.values.get(construction.value as usize)) {
        Some(ValueSyntax::Construction { callee: Some(callee) }) => ValueRef {
            file: construction.file,
            value: *callee,
        },
        _ => return false,
    };
    proves(source.provenance(constructor).as_ref(), "node:url", "URL", &[])
}
