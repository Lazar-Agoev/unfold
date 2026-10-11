//! Entrances, their evidence, and their targets.

use super::files::slice;
use super::{Checker, Subject};
use crate::key::entrance_keys;
use crate::model::{Entrance, EntranceTarget};
use crate::text::{LABEL_MAX_UTF16, utf16_len};
use crate::vocab::{BlockKind, DetectorId, EntranceKind, InputRole, Language, Resolution, TriggerStatus};

pub(super) fn check(checker: &mut Checker<'_>) {
    let graph = checker.graph;
    let keys = entrance_keys(graph.entrances.iter().map(Entrance::key_shape));
    for (index, (entrance, key)) in graph.entrances.iter().zip(keys).enumerate() {
        let subject = || Subject::Entrance(entrance.key.clone());
        if index > 0 && graph.entrances[index - 1].canonical_cmp(entrance).is_gt() {
            checker.report(subject(), "entrances are not in evidence order");
        }
        if entrance.key != key {
            checker.report(subject(), format!("key differs from its derivation `{key}`"));
        }
        check_fields(checker, entrance);
        check_evidence(checker, entrance);
        check_binding(checker, entrance);
        check_targets(checker, entrance);
    }
}

fn check_fields(checker: &mut Checker<'_>, entrance: &Entrance) {
    let subject = || Subject::Entrance(entrance.key.clone());
    if !entrance.detector.entrance_kinds().contains(&entrance.kind) {
        checker.report(
            subject(),
            format!(
                "detector {} does not produce {} entrances",
                entrance.detector, entrance.kind
            ),
        );
    }
    if entrance.method.is_some() && entrance.kind != EntranceKind::Http {
        checker.report(subject(), "only an http entrance has a method");
    }
    if entrance.trigger.as_deref() == Some("") {
        checker.report(subject(), "trigger is empty; an unknown trigger is omitted");
    }
    if (entrance.trigger_status == TriggerStatus::Unknown) != entrance.trigger.is_none() {
        checker.report(
            subject(),
            "trigger status is unknown exactly when the trigger is absent",
        );
    }
    if entrance.detector == DetectorId::Manual && entrance.key.configured_id().is_none() {
        checker.report(subject(), "a manual entrance's key is `entrance:configured:<id>`");
    }
    match entrance.label.as_deref() {
        Some("") => checker.report(subject(), "label is empty"),
        Some(_) if entrance.detector != DetectorId::Manual => {
            checker.report(subject(), "only manual entrances have labels");
        }
        _ => {}
    }
}

/// Whether `pointer` is an RFC 6901 JSON pointer to a member: it starts with `/` and every
/// `~` begins `~0` or `~1`.
fn valid_pointer(pointer: &str) -> bool {
    pointer.starts_with('/')
        && pointer
            .match_indices('~')
            .all(|(at, _)| matches!(pointer.as_bytes().get(at + 1), Some(b'0' | b'1')))
}

/// The input role whose files hold this detector's evidence, or `None` for detectors whose
/// evidence is a source block.
const fn evidence_input_role(detector: DetectorId) -> Option<InputRole> {
    match detector {
        DetectorId::Manual => Some(InputRole::UnfoldConfig),
        DetectorId::PackageScript | DetectorId::PackageBin => Some(InputRole::Package),
        DetectorId::PyprojectScript => Some(InputRole::Pyproject),
        DetectorId::ExpressRoute
        | DetectorId::BullmqWorker
        | DetectorId::NodeCronSchedule
        | DetectorId::FastapiRoute
        | DetectorId::CeleryTask
        | DetectorId::ApschedulerJob => None,
    }
}

fn check_evidence(checker: &mut Checker<'_>, entrance: &Entrance) {
    let subject = || Subject::Entrance(entrance.key.clone());
    let evidence = &entrance.evidence;
    if evidence.range.start > evidence.range.end {
        checker.report(
            subject(),
            format!("evidence range {} is reversed", evidence.range),
        );
    }
    let graph = checker.graph;
    let input = checker
        .inputs
        .get(evidence.path.as_str())
        .map(|&index| &graph.inputs[index]);
    let source = checker
        .files
        .get(evidence.path.as_str())
        .map(|&index| &graph.files[index]);
    match (evidence_input_role(entrance.detector), input, source) {
        (Some(role), Some(input), _) => {
            if input.role != role {
                checker.report(
                    subject(),
                    format!(
                        "{} evidence belongs in a {role} input, not {}",
                        entrance.detector, input.role
                    ),
                );
            }
            match evidence.pointer.as_deref() {
                None => checker.report(subject(), "configuration evidence needs a pointer"),
                Some(pointer) if !valid_pointer(pointer) => {
                    checker.report(
                        subject(),
                        format!("evidence pointer `{pointer}` is not a valid JSON pointer"),
                    );
                }
                Some(pointer) if entrance.detector == DetectorId::PyprojectScript => {
                    let name = pointer.strip_prefix("/project/scripts/");
                    if name.is_none_or(|name| name.is_empty() || name.contains('/')) {
                        checker.report(
                            subject(),
                            "pyproject.script evidence points at /project/scripts/<name>",
                        );
                    }
                }
                Some(_) => {}
            }
            if evidence.block.is_some() {
                checker.report(subject(), "configuration evidence has no block");
            }
        }
        (None, _, Some(file)) => {
            if evidence.pointer.is_some() {
                checker.report(subject(), "source evidence has no pointer");
            }
            if let Some(language) = entrance.detector.language()
                && file.file.language != language
            {
                checker.report(
                    subject(),
                    format!("{} evidence must be in a {language} file", entrance.detector),
                );
            }
            match &evidence.block {
                None => checker.report(subject(), "source evidence needs its block"),
                Some(key) => match checker.resolve(key) {
                    Some((block_file, block))
                        if block_file.file.path == evidence.path && block.range == evidence.range => {}
                    Some(_) => checker.report(
                        subject(),
                        format!("evidence block `{key}` does not have the evidence's file and range"),
                    ),
                    None => checker.report(
                        subject(),
                        format!("evidence block `{key}` is not a block in the graph"),
                    ),
                },
            }
            if let Some(root) = file.blocks.first()
                && !root.range.contains(evidence.range)
            {
                checker.report(
                    subject(),
                    format!("evidence range {} is outside the file", evidence.range),
                );
            }
        }
        (Some(role), None, _) => {
            checker.report(
                subject(),
                format!(
                    "{} evidence must be a {role} input in the graph",
                    entrance.detector
                ),
            );
        }
        (None, _, None) => {
            checker.report(
                subject(),
                format!("{} evidence must be an indexed source file", entrance.detector),
            );
        }
    }
    if checker.sources.is_some() && (input.is_some() || source.is_some()) {
        match checker.snapshot(&evidence.path) {
            None => checker.report(subject(), "no snapshot was supplied for the evidence file"),
            Some(snapshot) if slice(snapshot, evidence.range).is_none() => {
                checker.report(
                    subject(),
                    format!(
                        "evidence range {} is outside the file or not on character boundaries",
                        evidence.range
                    ),
                );
            }
            Some(_) => {}
        }
    }
}

/// An application binding belongs to an http entrance and names a block that can bind an
/// application: a variable, function, or class.
fn check_binding(checker: &mut Checker<'_>, entrance: &Entrance) {
    let Some(key) = &entrance.binding else {
        return;
    };
    let subject = || Subject::Entrance(entrance.key.clone());
    if entrance.kind != EntranceKind::Http {
        checker.report(subject(), "only an http entrance has an application binding");
    }
    match checker.resolve(key) {
        Some((_, block))
            if matches!(
                block.kind,
                BlockKind::Variable | BlockKind::Function | BlockKind::Class
            ) => {}
        Some(_) => checker.report(
            subject(),
            format!("binding `{key}` is not a variable, function, or class block"),
        ),
        None => checker.report(subject(), format!("binding `{key}` is not a block in the graph")),
    }
}

fn check_targets(checker: &mut Checker<'_>, entrance: &Entrance) {
    let entrance_subject = || Subject::Entrance(entrance.key.clone());
    match (entrance.kind, entrance.targets.len()) {
        (_, 0) => checker.report(entrance_subject(), "an entrance needs at least one target"),
        (EntranceKind::Process, count) if count != 1 => {
            checker.report(
                entrance_subject(),
                format!("a process has exactly one startup target, found {count}"),
            );
        }
        _ => {}
    }
    let mut languages: Vec<Language> = Vec::new();
    for (index, target) in entrance.targets.iter().enumerate() {
        if let Some(language) = check_target(checker, entrance, index, target)
            && !languages.contains(&language)
        {
            languages.push(language);
        }
    }
    if languages.len() > 1 {
        checker.report(entrance_subject(), "resolved targets cannot mix languages");
    }
}

/// Checks one target and returns the language of its resolved block, if any.
fn check_target(
    checker: &mut Checker<'_>,
    entrance: &Entrance,
    index: usize,
    target: &EntranceTarget,
) -> Option<Language> {
    let subject = || Subject::Target {
        entrance: entrance.key.clone(),
        position: target.position,
    };
    if usize::try_from(target.position).ok() != Some(index) {
        checker.report(
            subject(),
            format!("positions must be contiguous from zero; expected {index}"),
        );
    }
    let role = entrance.kind.target_role();
    if target.role != role {
        checker.report(
            subject(),
            format!("a {} entrance's targets have role {role}", entrance.kind),
        );
    }
    match (target.resolution, target.reason) {
        (Resolution::Unresolved, None) => checker.report(subject(), "an unresolved target needs a reason"),
        (Resolution::Resolved | Resolution::External, Some(_)) => {
            checker.report(subject(), "only an unresolved target has a reason");
        }
        _ => {}
    }
    match (target.resolution, &target.module) {
        (Resolution::External, None) => checker.report(subject(), "an external target needs a module"),
        (Resolution::External, Some(module)) if !module.is_valid() => {
            checker.report(
                subject(),
                format!(
                    "module `{}` is not a label or a sorted set of labels",
                    module.as_str()
                ),
            );
        }
        (Resolution::Resolved | Resolution::Unresolved, Some(_)) => {
            checker.report(subject(), "only an external target has a module");
        }
        _ => {}
    }
    if utf16_len(&target.text) > LABEL_MAX_UTF16 {
        checker.report(
            subject(),
            format!("text exceeds {LABEL_MAX_UTF16} UTF-16 code units"),
        );
    }
    match (target.resolution, &target.target) {
        (Resolution::Resolved, Some(key)) => {
            let Some((file, block)) = checker.resolve(key) else {
                checker.report(subject(), format!("target `{key}` is not a block in the graph"));
                return None;
            };
            if !target.role.target_kinds(entrance.detector).contains(&block.kind) {
                checker.report(
                    subject(),
                    format!(
                        "a {} target of {} cannot be a {}",
                        target.role, entrance.detector, block.kind
                    ),
                );
            }
            let language = file.file.language;
            if entrance
                .detector
                .language()
                .is_some_and(|expected| expected != language)
            {
                checker.report(
                    subject(),
                    format!(
                        "{} targets must be {}",
                        entrance.detector,
                        entrance.detector.language().map_or("", Language::as_str)
                    ),
                );
            }
            Some(language)
        }
        (Resolution::Resolved, None) => {
            checker.report(subject(), "a resolved target needs its block");
            None
        }
        (_, Some(_)) => {
            checker.report(subject(), "only a resolved target names a block");
            None
        }
        (_, None) => None,
    }
}
