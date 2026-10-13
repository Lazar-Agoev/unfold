//! Typed records: signatures, parameters, members, and arguments. Each names a node of the
//! right kind, records of one node are indexed contiguously from zero in canonical order, and
//! their ranges lie where `crate::typed` says.

use rustc_hash::{FxHashMap, FxHashSet};

use super::{Checker, Subject};
use crate::key::Key;
use crate::model::{FileGraph, LinkTarget, Node};
use crate::text::ByteRange;
use crate::typed::{Pairing, Parameter, WrittenType};
use crate::vocab::{Form, Language, LinkKind, MemberKind, NodeKind, ParameterKind};

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph) {
    check_signatures(checker, file);
    check_parameters(checker, file);
    check_members(checker, file);
    check_arguments(checker, file);
}

/// Every typed-record range of the file, with its record and field, for the checks against
/// snapshots and censuses. Records that name no node are left out (and reported elsewhere).
pub(super) fn ranges(file: &FileGraph) -> Vec<(Subject, &'static str, ByteRange)> {
    let key = |id: u32| file.nodes.get(id as usize).map(|node| node.key.clone());
    let types = &file.types;
    let mut ranges = Vec::new();
    for signature in &types.signatures {
        if let (Some(node), Some(returns)) = (key(signature.node), &signature.returns) {
            ranges.push((Subject::Signature(node), "returns", returns.range));
        }
    }
    for parameter in &types.parameters {
        let Some(node) = key(parameter.node) else { continue };
        let subject = || Subject::Parameter {
            node: node.clone(),
            index: parameter.index,
        };
        ranges.push((subject(), "range", parameter.range));
        if let Some(ty) = &parameter.ty {
            ranges.push((subject(), "type", ty.range));
        }
    }
    for member in &types.members {
        if let (Some(node), Some(ty)) = (key(member.node), &member.ty) {
            let subject = Subject::Member {
                node,
                index: member.index,
            };
            ranges.push((subject, "type", ty.range));
        }
    }
    for argument in &types.arguments {
        if let Some(call) = key(argument.call) {
            let subject = Subject::Argument {
                call,
                index: argument.index,
            };
            ranges.push((subject, "range", argument.range));
        }
    }
    ranges
}

/// The node a record names, or a violation on the file when it names none.
fn node<'f>(checker: &mut Checker<'_>, file: &'f FileGraph, id: u32, record: &str) -> Option<&'f Node> {
    let found = file.nodes.get(id as usize);
    if found.is_none() {
        checker.report(
            Subject::File(file.file.path.clone()),
            format!("a {record} names missing node {id}"),
        );
    }
    found
}

/// The index a record of node `id` must have, given the previous record's node and index;
/// reports records out of order.
fn expected_index(
    checker: &mut Checker<'_>,
    subject: &dyn Fn() -> Subject,
    previous: Option<(u32, u32)>,
    id: u32,
    record: &str,
) -> u32 {
    match previous {
        Some((node, index)) if node == id => index + 1,
        Some((node, _)) if node > id => {
            checker.report(subject(), format!("{record}s are not in node, then index, order"));
            0
        }
        _ => 0,
    }
}

fn check_index(checker: &mut Checker<'_>, subject: &dyn Fn() -> Subject, index: u32, expected: u32) {
    if index != expected {
        checker.report(
            subject(),
            format!("indexes are contiguous from zero; expected {expected}"),
        );
    }
}

fn check_written(
    checker: &mut Checker<'_>,
    subject: &dyn Fn() -> Subject,
    field: &str,
    written: &WrittenType,
    within: Option<ByteRange>,
    place: &str,
) {
    checker.check_label(subject, field, &written.text);
    if written.range.is_empty() {
        checker.report(subject(), format!("{field} range {} is empty", written.range));
    }
    if !within.is_some_and(|within| within.contains(written.range)) {
        checker.report(
            subject(),
            format!("{field} range {} is outside {place}", written.range),
        );
    }
}

/// Exactly one signature per function, and none on any other node.
fn check_signatures(checker: &mut Checker<'_>, file: &FileGraph) {
    let signed: FxHashSet<u32> = file
        .types
        .signatures
        .iter()
        .map(|signature| signature.node)
        .collect();
    for node in file.nodes.iter().enumerate().filter_map(|(id, node)| {
        let id = u32::try_from(id).ok()?;
        (node.kind() == NodeKind::Function && !signed.contains(&id)).then_some(node)
    }) {
        checker.report(Subject::Node(node.key.clone()), "a function has no signature");
    }
    let mut previous: Option<u32> = None;
    for signature in &file.types.signatures {
        let Some(found) = node(checker, file, signature.node, "signature") else {
            continue;
        };
        let subject = || Subject::Signature(found.key.clone());
        if previous.is_some_and(|previous| previous >= signature.node) {
            checker.report(subject(), "signatures are not in node order, one per function");
        }
        previous = Some(signature.node);
        if found.kind() != NodeKind::Function {
            checker.report(subject(), format!("{} nodes have no signature", found.kind()));
        }
        if let Some(returns) = &signature.returns {
            check_written(
                checker,
                &subject,
                "returns",
                returns,
                found.head,
                "the function's head",
            );
        }
    }
}

fn check_parameters(checker: &mut Checker<'_>, file: &FileGraph) {
    let language = file.file.language;
    let mut previous: Option<(u32, u32)> = None;
    // Per function: the highest Python order seen, and whether a rest or keywords was seen.
    let mut order: FxHashMap<u32, (u8, bool, bool)> = FxHashMap::default();
    for parameter in &file.types.parameters {
        let Some(found) = node(checker, file, parameter.node, "parameter") else {
            continue;
        };
        let subject = || Subject::Parameter {
            node: found.key.clone(),
            index: parameter.index,
        };
        let expected = expected_index(checker, &subject, previous, parameter.node, "parameter");
        check_index(checker, &subject, parameter.index, expected);
        previous = Some((parameter.node, parameter.index));
        if found.kind() != NodeKind::Function {
            checker.report(subject(), format!("{} nodes have no parameters", found.kind()));
            continue;
        }
        if !found.head.is_some_and(|head| head.contains(parameter.range)) {
            checker.report(
                subject(),
                format!("range {} is outside the function's head", parameter.range),
            );
        }
        if !parameter.kind.allowed_in(language) {
            checker.report(
                subject(),
                format!("kind {} does not occur in {language} files", parameter.kind),
            );
        }
        if parameter.kind == ParameterKind::This && parameter.index != 0 {
            checker.report(subject(), "a this parameter is the first parameter");
        }
        if let Some(name) = &parameter.name {
            checker.check_label(&subject, "name", name);
        }
        if let Some(ty) = &parameter.ty {
            check_written(
                checker,
                &subject,
                "type",
                ty,
                Some(parameter.range),
                "the parameter",
            );
        }
        match language {
            Language::TypeScript if parameter.optional && parameter.kind == ParameterKind::Rest => {
                checker.report(subject(), "a rest parameter is never optional");
            }
            Language::Python if parameter.optional => {
                checker.report(subject(), "only TypeScript parameters are optional");
            }
            _ => {}
        }
        let entry = order.entry(parameter.node).or_insert((0, false, false));
        if let Some(rank) = parameter.kind.python_order() {
            if language == Language::Python && rank < entry.0 {
                checker.report(subject(), "Python parameters are not in the language's order");
            }
            entry.0 = entry.0.max(rank);
        }
        let repeated = match parameter.kind {
            ParameterKind::Rest => std::mem::replace(&mut entry.1, true),
            ParameterKind::Keywords => std::mem::replace(&mut entry.2, true),
            _ => false,
        };
        if repeated {
            checker.report(subject(), format!("a second {} parameter", parameter.kind));
        }
    }
}

fn check_members(checker: &mut Checker<'_>, file: &FileGraph) {
    let mut previous: Option<(u32, u32)> = None;
    for member in &file.types.members {
        let Some(found) = node(checker, file, member.node, "member") else {
            continue;
        };
        let subject = || Subject::Member {
            node: found.key.clone(),
            index: member.index,
        };
        let expected = expected_index(checker, &subject, previous, member.node, "member");
        check_index(checker, &subject, member.index, expected);
        previous = Some((member.node, member.index));
        if !matches!(found.kind(), NodeKind::Class | NodeKind::Type) {
            checker.report(subject(), format!("{} nodes have no members", found.kind()));
        }
        if (member.kind == MemberKind::Value) != (found.form == Form::Enum) {
            checker.report(subject(), "value members are exactly the members of an enum");
        }
        if member.value.is_some() && member.kind != MemberKind::Value {
            checker.report(subject(), "only a value member has a value");
        }
        if let Some(name) = &member.name {
            checker.check_label(&subject, "name", name);
        }
        if let Some(value) = &member.value {
            checker.check_label(&subject, "value", value);
        }
        if let Some(ty) = &member.ty {
            check_written(checker, &subject, "type", ty, Some(found.range), "its node");
        }
        if file.file.language == Language::Python && (member.optional || member.readonly) {
            checker.report(subject(), "only TypeScript members are optional or readonly");
        }
    }
}

fn check_arguments(checker: &mut Checker<'_>, file: &FileGraph) {
    let language = file.file.language;
    // Each call's project target, from its `calls` link.
    let mut targets: FxHashMap<u32, &Key> = FxHashMap::default();
    // Each call's project `passes` links, as (range, target).
    let mut passes: FxHashMap<u32, Vec<(ByteRange, &Key)>> = FxHashMap::default();
    for link in &file.links {
        let LinkTarget::Project(key) = &link.to else {
            continue;
        };
        match link.kind {
            LinkKind::Calls => {
                targets.insert(link.from, key);
            }
            LinkKind::Passes => passes.entry(link.from).or_default().push((link.range, key)),
            _ => {}
        }
    }
    let mut previous: Option<(u32, u32)> = None;
    for argument in &file.types.arguments {
        let Some(found) = node(checker, file, argument.call, "argument") else {
            continue;
        };
        let subject = || Subject::Argument {
            call: found.key.clone(),
            index: argument.index,
        };
        let expected = expected_index(checker, &subject, previous, argument.call, "argument");
        check_index(checker, &subject, argument.index, expected);
        previous = Some((argument.call, argument.index));
        if found.kind() != NodeKind::Call {
            checker.report(subject(), format!("{} nodes have no arguments", found.kind()));
            continue;
        }
        if !found.range.contains(argument.range) {
            checker.report(
                subject(),
                format!(
                    "range {} is outside the call's range {}",
                    argument.range, found.range
                ),
            );
        }
        if let Some(expand) = argument.expand
            && !expand.allowed_in(language)
        {
            checker.report(
                subject(),
                format!("expansion {expand} does not occur in {language} files"),
            );
        }
        if let Some(keyword) = &argument.keyword {
            if language != Language::Python {
                checker.report(subject(), "only Python arguments have keywords");
            }
            if argument.expand.is_some() {
                checker.report(subject(), "an expanded argument has no keyword");
            }
            checker.check_label(&subject, "keyword", keyword);
        }
        if argument.bound.is_some() {
            let passed = passes.get(&argument.call).is_some_and(|passed| {
                passed.iter().any(|(range, target)| {
                    argument.range.contains(*range)
                        && checker
                            .resolve(target)
                            .is_some_and(|(_, node, _)| node.kind() == NodeKind::Function)
                })
            });
            if !passed {
                checker.report(
                    subject(),
                    "only an argument that passes a project function has a bound count",
                );
            }
        }
        match (argument.pairing, targets.get(&argument.call)) {
            (None, None) => {}
            (Some(_), None) => checker.report(
                subject(),
                "only an argument of a call that reaches a project definition is paired",
            ),
            (None, Some(_)) => checker.report(
                subject(),
                "an argument of a call that reaches a project definition needs a pairing",
            ),
            (Some(pairing), Some(target)) => check_pairing(checker, &subject, pairing, target),
        }
    }
}

/// A pairing names a parameter the call's target has, and that takes an argument. A target that
/// names no node is the link checks' to report.
fn check_pairing(checker: &mut Checker<'_>, subject: &dyn Fn() -> Subject, pairing: Pairing, target: &Key) {
    let Pairing::Parameter(index) = pairing else {
        return;
    };
    let Some((target_file, _, target_id)) = checker.resolve(target) else {
        return;
    };
    match parameter_of(&target_file.types.parameters, target_id, index) {
        Some(parameter) if parameter.kind.takes_argument() => {}
        Some(parameter) => checker.report(
            subject(),
            format!(
                "parameter {index} of the target is a {} parameter",
                parameter.kind
            ),
        ),
        None => checker.report(subject(), format!("the target has no parameter {index}")),
    }
}

/// The parameter record `index` of node `node`, in records sorted by node, then index.
fn parameter_of(parameters: &[Parameter], node: u32, index: u32) -> Option<&Parameter> {
    parameters
        .binary_search_by_key(&(node, index), |parameter| (parameter.node, parameter.index))
        .ok()
        .map(|found| &parameters[found])
}
