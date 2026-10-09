//! Manual entrances: the `[[entrance]]` tables of `.unfold/config.toml` as candidates.
//!
//! Each table becomes one `manual` candidate whose evidence is its span in the settings file
//! (pointer `/entrance/<index>`) and whose targets are its locators in declared order. Manual
//! candidates come first in detection order, before any language's detectors.

use unfold_core::RepoPath;
use unfold_core::vocab::DetectorId;

use super::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use crate::settings::{SETTINGS_PATH, Settings};

/// The candidates of every `[[entrance]]` table, in file order.
pub fn manual_candidates(settings: &Settings) -> Vec<EntranceCandidate> {
    if settings.entrances.is_empty() {
        return Vec::new();
    }
    let config = RepoPath::new(SETTINGS_PATH).expect("the settings path is a valid folder-relative path");
    settings
        .entrances
        .iter()
        .map(|entrance| EntranceCandidate {
            detector: DetectorId::Manual,
            kind: entrance.kind,
            method: entrance.method,
            trigger: entrance.trigger.clone().map_or(Trigger::Unknown, Trigger::Exact),
            label: entrance.label.clone(),
            evidence: CandidateEvidence::Input {
                path: config.clone(),
                range: entrance.span,
                pointer: entrance.pointer().into(),
            },
            targets: entrance
                .targets
                .locators()
                .iter()
                .cloned()
                .map(TargetCandidate::Locator)
                .collect(),
        })
        .collect()
}
