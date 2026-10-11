//! An alien route detector: `(call (get APP get|post) "path" HANDLER...)` where `APP` is proven,
//! by bounded provenance, to be the result of calling the default export of package `express`.

use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Reason};
use unfold_engine::BlockRef;
use unfold_engine::ValueRef;
use unfold_engine::entrances::{
    CandidateEvidence, DetectContext, EntranceCandidate, EntranceDetector, TargetCandidate, Trigger,
};

use super::link::AlienLinked;
use super::payload::ValueInfo;

pub struct Routes<'a> {
    pub linked: &'a AlienLinked<'a>,
}

impl EntranceDetector for Routes<'_> {
    fn id(&self) -> DetectorId {
        DetectorId::ExpressRoute
    }

    fn detect(&self, context: &DetectContext<'_>) -> Vec<EntranceCandidate> {
        let mut candidates = Vec::new();
        for (file, entry) in context.program().iter() {
            let Some(facts) = entry
                .facts
                .as_deref()
                .filter(|_| self.linked.payload(file).is_some())
            else {
                continue;
            };
            for call in facts.graph.calls() {
                let Some(callee) = call.callee.map(|value| ValueRef { file, value }) else {
                    continue;
                };
                let method = match self.linked.value_info(callee) {
                    Some(ValueInfo::Get { name, .. }) if name == "get" => HttpMethod::Get,
                    Some(ValueInfo::Get { name, .. }) if name == "post" => HttpMethod::Post,
                    _ => continue,
                };
                let proven = context.callee_provenance(callee).is_some_and(|origin| {
                    origin.module == "express" && &*origin.export == "default" && origin.path == ["()".into()]
                });
                if !proven {
                    continue;
                }
                let value = |position: u32| {
                    call.arguments
                        .iter()
                        .find(|argument| argument.position == position)
                        .map(|argument| ValueRef {
                            file,
                            value: argument.value,
                        })
                };
                let trigger = match value(0).and_then(|path| self.linked.value_info(path)) {
                    Some(ValueInfo::Literal(text)) => Trigger::Exact(text.as_str().into()),
                    _ => Trigger::Unknown,
                };
                let targets: Vec<TargetCandidate> = call
                    .arguments
                    .iter()
                    .filter(|argument| argument.position > 0)
                    .map(|argument| {
                        let handler = ValueRef {
                            file,
                            value: argument.value,
                        };
                        match self.linked.value_info(handler) {
                            Some(ValueInfo::Other) => TargetCandidate::Failure {
                                reason: Reason::Unsupported,
                                text: "(…)".into(),
                            },
                            _ => TargetCandidate::Value(handler),
                        }
                    })
                    .collect();
                if targets.is_empty() {
                    continue;
                }
                candidates.push(EntranceCandidate {
                    detector: DetectorId::ExpressRoute,
                    kind: EntranceKind::Http,
                    method: Some(method),
                    trigger,
                    label: None,
                    evidence: CandidateEvidence::Block(BlockRef {
                        file,
                        local: call.block,
                    }),
                    binding: None,
                    targets,
                });
            }
        }
        candidates
    }
}
