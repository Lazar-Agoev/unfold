//! Manual entrances: the `[[entrance]]` tables of `unfold.toml` as candidates.
//!
//! Each table becomes one `manual` candidate whose evidence is its span in the settings file
//! (pointer `/entrance/<index>`) and whose key derives from its `id`. Its targets are its
//! handler locators in declared order, or a process's start: its `startup` locator, or what its
//! plan starts, resolved after linking ([`super::launches::starts`]). Manual candidates come
//! first in detection order, before any language's detectors.

use unfold_core::vocab::DetectorId;

use super::launches::Start;
use super::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use crate::settings::{ManualTargets, Settings};

/// The candidates of every `[[entrance]]` table, in file order. `starts` is aligned with the
/// tables ([`super::launches::starts`]).
pub fn manual_candidates(settings: &Settings, starts: &[Option<Start>]) -> Vec<EntranceCandidate> {
    settings
        .entrances
        .iter()
        .zip(starts)
        .map(|(entrance, start)| EntranceCandidate {
            detector: DetectorId::Manual,
            kind: entrance.kind,
            method: entrance.method,
            trigger: entrance.trigger.clone().map_or(Trigger::Unknown, Trigger::Exact),
            label: entrance.label.clone(),
            evidence: CandidateEvidence::Configured {
                id: entrance.id.clone(),
                range: entrance.span,
                pointer: entrance.pointer().into(),
            },
            binding: None,
            targets: match (&entrance.targets, start) {
                (ManualTargets::Handlers(locators), _) => {
                    locators.iter().cloned().map(TargetCandidate::Locator).collect()
                }
                (ManualTargets::Process(_), Some(start)) => vec![start.target.clone()],
                (ManualTargets::Process(_), None) => {
                    unreachable!("a parsed process has a command, startup, or target")
                }
            },
        })
        .collect()
}
