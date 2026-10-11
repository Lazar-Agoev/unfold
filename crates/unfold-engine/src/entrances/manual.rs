//! Manual entrances: the `[[entrance]]` tables of `unfold.toml` as candidates.
//!
//! Each table becomes one `manual` candidate whose evidence is its span in the settings file
//! (pointer `/entrance/<index>`) and whose key derives from its `id`. Its targets are its
//! handler locators in declared order, or a process's `startup`. A process without a `startup`
//! has one failed `unsupported` target until the launch planner reads its command or target.
//! Manual candidates come first in detection order, before any language's detectors.

use unfold_core::vocab::{DetectorId, Reason};

use super::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use crate::launch::LaunchTarget;
use crate::settings::{ManualTargets, ProcessStart, Settings};

/// The candidates of every `[[entrance]]` table, in file order.
pub fn manual_candidates(settings: &Settings) -> Vec<EntranceCandidate> {
    settings
        .entrances
        .iter()
        .map(|entrance| EntranceCandidate {
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
            targets: match &entrance.targets {
                ManualTargets::Handlers(locators) => {
                    locators.iter().cloned().map(TargetCandidate::Locator).collect()
                }
                ManualTargets::Process(process) => vec![match (&process.start, &process.command) {
                    (Some(ProcessStart::Startup(locator)), _) => TargetCandidate::Locator(locator.clone()),
                    (Some(ProcessStart::Target(target)), None) => TargetCandidate::Failure {
                        reason: Reason::Unsupported,
                        text: target_text(target).into(),
                    },
                    (_, Some(command)) => TargetCandidate::Failure {
                        reason: Reason::Unsupported,
                        text: command.clone(),
                    },
                    (None, None) => unreachable!("a parsed process has a command, startup, or target"),
                }],
            },
        })
        .collect()
}

/// A launch target as `path`, `module`, or `module:attribute`.
fn target_text(target: &LaunchTarget) -> String {
    match target {
        LaunchTarget::File { path } => path.to_string(),
        LaunchTarget::PythonModule { module } => module.to_string(),
        LaunchTarget::PythonCallable { module, attribute }
        | LaunchTarget::PythonAsgi {
            module, attribute, ..
        } => format!("{module}:{attribute}"),
    }
}
