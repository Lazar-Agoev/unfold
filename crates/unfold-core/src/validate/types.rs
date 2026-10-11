//! Typed records: signatures, parameters, members, arguments, call results, and type links.

use rustc_hash::{FxHashMap, FxHashSet};

use super::files::slice;
use super::{Census, Checker, Subject};
use crate::key::Key;
use crate::model::{Block, FileGraph};
use crate::text::{ByteRange, LABEL_MAX_UTF16, utf16_len};
use crate::typed::{Pairing, Parameter, TypeLink, WrittenType};
use crate::vocab::{
    BlockKind, EdgeKind, Heritage, Language, MemberKind, ParameterKind, Resolution, ResultBinding, SlotName,
};

/// The slots of one file that typed records are checked against, by block.
#[derive(Default)]
struct Slots {
    /// `(block, index)` → the parameter slot's range.
    parameters: FxHashMap<(u32, u32), ByteRange>,
    /// `(block, index)` → the argument slot's range.
    arguments: FxHashMap<(u32, u32), ByteRange>,
    /// Block → its `return_type` slot's range.
    returns: FxHashMap<u32, ByteRange>,
    /// Block → how many argument slots it has.
    argument_counts: FxHashMap<u32, u32>,
    /// Block → its `heritage` slot's range.
    heritage: FxHashMap<u32, ByteRange>,
}

impl Slots {
    fn of(file: &FileGraph) -> Self {
        let mut slots = Self::default();
        for slot in &file.slots {
            match (slot.slot, slot.index) {
                (SlotName::Parameter, Some(index)) => {
                    slots.parameters.insert((slot.block, index), slot.range);
                }
                (SlotName::Argument, Some(index)) => {
                    slots.arguments.insert((slot.block, index), slot.range);
                    *slots.argument_counts.entry(slot.block).or_insert(0) += 1;
                }
                (SlotName::ReturnType, _) => {
                    slots.returns.insert(slot.block, slot.range);
                }
                (SlotName::Heritage, _) => {
                    slots.heritage.insert(slot.block, slot.range);
                }
                _ => {}
            }
        }
        slots
    }
}

/// Checks one file's typed records. With `complete`, every function-like block needs a
/// signature, every parameter and argument slot its record, and every call its result.
pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph, complete: bool) {
    let slots = Slots::of(file);
    check_signatures(checker, file, &slots, complete);
    check_parameters(checker, file, &slots, complete);
    check_members(checker, file);
    check_arguments(checker, file, &slots, complete);
    check_results(checker, file, &slots, complete);
    check_type_links(checker, file, &slots);
}

/// The block a record names, or a violation on the file when it names none.
fn block<'f>(checker: &mut Checker<'_>, file: &'f FileGraph, local: u32, record: &str) -> Option<&'f Block> {
    let found = file.blocks.get(local as usize);
    if found.is_none() {
        checker.report(
            Subject::File(file.file.path.clone()),
            format!("a {record} names missing block {local}"),
        );
    }
    found
}

fn check_text(checker: &mut Checker<'_>, subject: &dyn Fn() -> Subject, field: &str, text: &str) {
    if text.is_empty() {
        checker.report(
            subject(),
            format!("{field} is empty; an absent {field} is omitted"),
        );
    }
    if utf16_len(text) > LABEL_MAX_UTF16 {
        checker.report(
            subject(),
            format!("{field} exceeds {LABEL_MAX_UTF16} UTF-16 code units"),
        );
    }
}

fn check_written(
    checker: &mut Checker<'_>,
    subject: &dyn Fn() -> Subject,
    field: &str,
    written: &WrittenType,
    within: Option<ByteRange>,
) {
    check_text(checker, subject, field, &written.text);
    if written.range.is_empty() {
        checker.report(subject(), format!("{field} range {} is empty", written.range));
    }
    match within {
        Some(within) if within.contains(written.range) => {}
        Some(within) => checker.report(
            subject(),
            format!("{field} range {} is outside {within}", written.range),
        ),
        None => checker.report(subject(), format!("{field} has no written part to lie in")),
    }
}

fn check_signatures(checker: &mut Checker<'_>, file: &FileGraph, slots: &Slots, complete: bool) {
    let mut previous: Option<u32> = None;
    for signature in &file.types.signatures {
        let Some(found) = block(checker, file, signature.block, "signature") else {
            continue;
        };
        let subject = || Subject::Signature(found.key.clone());
        if previous.is_some_and(|previous| previous >= signature.block) {
            checker.report(subject(), "signatures are not in block order, one per block");
        }
        previous = Some(signature.block);
        if !found.kind.is_function_like() {
            checker.report(subject(), format!("a {} has no signature", found.kind));
        }
        let slot = slots.returns.get(&signature.block).copied();
        match (&signature.returns, slot) {
            (Some(returns), slot) => check_written(checker, &subject, "returns", returns, slot),
            (None, Some(_)) => checker.report(subject(), "the return_type slot's type is not recorded"),
            (None, None) => {}
        }
    }
    if !complete {
        return;
    }
    let mut signatures = file
        .types
        .signatures
        .iter()
        .map(|signature| signature.block)
        .peekable();
    for (local, found) in file.blocks.iter().enumerate() {
        let local = local as u32;
        let has = signatures
            .next_if(|&block| block <= local)
            .is_some_and(|block| block == local);
        while signatures.next_if(|&block| block <= local).is_some() {}
        if found.kind.is_function_like() && !has {
            checker.report(
                Subject::Block(found.key.clone()),
                "a function-like block needs exactly one signature",
            );
        }
    }
}

fn check_parameters(checker: &mut Checker<'_>, file: &FileGraph, slots: &Slots, complete: bool) {
    let language = file.file.language;
    let mut previous: Option<(u32, u32)> = None;
    // Per block: the last kind's Python order, and whether a rest or keywords was seen.
    let mut order: FxHashMap<u32, (u8, bool, bool)> = FxHashMap::default();
    for parameter in &file.types.parameters {
        let Some(found) = block(checker, file, parameter.block, "parameter") else {
            continue;
        };
        let subject = || Subject::Parameter {
            block: found.key.clone(),
            index: parameter.index,
        };
        let position = (parameter.block, parameter.index);
        if previous.is_some_and(|previous| previous >= position) {
            checker.report(subject(), "parameters are not in block, then index, order");
        }
        previous = Some(position);
        let Some(&slot) = slots.parameters.get(&position) else {
            checker.report(subject(), "no parameter slot has this index");
            continue;
        };
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
            check_text(checker, &subject, "name", name);
        }
        if let Some(ty) = &parameter.ty {
            check_written(checker, &subject, "type", ty, Some(slot));
        }
        if language == Language::TypeScript && parameter.optional && parameter.kind == ParameterKind::Rest {
            checker.report(subject(), "a rest parameter is never optional");
        }
        if language == Language::Python && parameter.optional {
            checker.report(subject(), "only TypeScript parameters are optional");
        }
        let entry = order.entry(parameter.block).or_insert((0, false, false));
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
    if !complete {
        return;
    }
    let recorded: FxHashSet<(u32, u32)> = file
        .types
        .parameters
        .iter()
        .map(|parameter| (parameter.block, parameter.index))
        .collect();
    let mut missing: Vec<(u32, u32)> = slots
        .parameters
        .keys()
        .filter(|position| !recorded.contains(position))
        .copied()
        .collect();
    missing.sort_unstable();
    for (local, index) in missing {
        if let Some(found) = file.blocks.get(local as usize) {
            checker.report(
                Subject::Slot {
                    block: found.key.clone(),
                    slot: SlotName::Parameter,
                    index: Some(index),
                },
                "the parameter slot has no parameter record",
            );
        }
    }
}

fn check_members(checker: &mut Checker<'_>, file: &FileGraph) {
    let mut previous: Option<(u32, u32)> = None;
    for member in &file.types.members {
        let Some(found) = block(checker, file, member.block, "member") else {
            continue;
        };
        let subject = || Subject::Member {
            block: found.key.clone(),
            index: member.index,
        };
        let expected = match previous {
            Some((block, index)) if block == member.block => index + 1,
            Some((block, _)) if block > member.block => {
                checker.report(subject(), "members are not in block, then index, order");
                member.index
            }
            _ => 0,
        };
        if member.index != expected {
            checker.report(
                subject(),
                format!("member indices must be contiguous from zero; expected {expected}"),
            );
        }
        previous = Some((member.block, member.index));
        if !found.kind.is_type_declaration() {
            checker.report(subject(), format!("a {} has no members", found.kind));
        }
        if (member.kind == MemberKind::Value) != (found.kind == BlockKind::Enum) {
            checker.report(subject(), "value members are exactly the members of an enum");
        }
        if member.value.is_some() && member.kind != MemberKind::Value {
            checker.report(subject(), "only a value member has a value");
        }
        if let Some(name) = &member.name {
            check_text(checker, &subject, "name", name);
        }
        if let Some(value) = &member.value {
            check_text(checker, &subject, "value", value);
        }
        if let Some(ty) = &member.ty {
            check_written(checker, &subject, "type", ty, Some(found.range));
        }
        if file.file.language == Language::Python && (member.optional || member.readonly) {
            checker.report(subject(), "only TypeScript members are optional or readonly");
        }
    }
}

fn check_arguments(checker: &mut Checker<'_>, file: &FileGraph, slots: &Slots, complete: bool) {
    let language = file.file.language;
    let occurrences: FxHashMap<u32, &crate::model::Edge> = file
        .edges
        .iter()
        .filter(|edge| edge.seq == 0)
        .map(|edge| (edge.source, edge))
        .collect();
    // Resolved references by (call, argument position) → target.
    let references: FxHashMap<(u32, u32), &Key> = file
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::References)
        .filter_map(|edge| Some(((edge.source, edge.position?), edge.target.as_ref()?)))
        .collect();
    let mut previous: Option<(u32, u32)> = None;
    for argument in &file.types.arguments {
        let Some(found) = block(checker, file, argument.call, "argument") else {
            continue;
        };
        let subject = || Subject::Argument {
            call: found.key.clone(),
            index: argument.index,
        };
        let position = (argument.call, argument.index);
        if previous.is_some_and(|previous| previous >= position) {
            checker.report(subject(), "arguments are not in call, then index, order");
        }
        previous = Some(position);
        if !matches!(found.kind, BlockKind::Call | BlockKind::New) || !slots.arguments.contains_key(&position)
        {
            checker.report(subject(), "no argument slot of a call or new has this index");
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
            check_text(checker, &subject, "keyword", keyword);
        }
        let occurrence = occurrences.get(&argument.call);
        let resolved = occurrence.and_then(|edge| match edge.resolution {
            Resolution::Resolved => edge.target.as_ref(),
            _ => None,
        });
        if argument.bound.is_some() {
            let passed = references
                .get(&(argument.call, argument.index))
                .is_some_and(|target| function_like(checker, target));
            if !passed {
                checker.report(
                    subject(),
                    "only an argument that passes a function-like callable has a bound count",
                );
            }
        }
        match (argument.pairing, resolved) {
            (None, None) => {}
            (Some(_), None) => {
                checker.report(subject(), "only an argument of a resolved occurrence is paired");
            }
            (None, Some(_)) => checker.report(
                subject(),
                "an argument of a resolved occurrence needs a parameter or an unpaired reason",
            ),
            (Some(pairing), Some(target)) => check_pairing(checker, &subject, pairing, target),
        }
    }
    if !complete {
        return;
    }
    let recorded: FxHashSet<(u32, u32)> = file
        .types
        .arguments
        .iter()
        .map(|argument| (argument.call, argument.index))
        .collect();
    let mut missing: Vec<(u32, u32)> = slots
        .arguments
        .keys()
        .filter(|position| !recorded.contains(position))
        .copied()
        .collect();
    missing.sort_unstable();
    for (local, index) in missing {
        if let Some(found) = file.blocks.get(local as usize) {
            checker.report(
                Subject::Slot {
                    block: found.key.clone(),
                    slot: SlotName::Argument,
                    index: Some(index),
                },
                "the argument slot has no argument record",
            );
        }
    }
}

/// Checks a pairing against the call's resolved target. A target that names no block is the
/// edge checks' to report.
fn check_pairing(checker: &mut Checker<'_>, subject: &dyn Fn() -> Subject, pairing: Pairing, target: &Key) {
    let Some((target_file, target_block)) = checker.resolve(target) else {
        return;
    };
    match pairing {
        Pairing::Unpaired(_) => {}
        Pairing::Parameter(_) if target_block.kind == BlockKind::Class => {
            checker.report(subject(), "an argument of a class's construction is unpaired");
        }
        Pairing::Parameter(index) => {
            let parameter = parameter_of(&target_file.types.parameters, target_block.local_id, index);
            match parameter {
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
    }
}

/// Whether `target` names a function-like block (a missing one is the edge checks' to report,
/// so it counts as one here).
fn function_like(checker: &Checker<'_>, target: &Key) -> bool {
    checker
        .resolve(target)
        .is_none_or(|(_, block)| block.kind.is_function_like())
}

/// The parameter record `index` of block `block`, in records sorted by block, then index.
fn parameter_of(parameters: &[Parameter], block: u32, index: u32) -> Option<&Parameter> {
    parameters
        .binary_search_by_key(&(block, index), |parameter| (parameter.block, parameter.index))
        .ok()
        .map(|found| &parameters[found])
}

/// Whether `ancestor` is a proper ancestor of the block at `local`.
fn is_ancestor(file: &FileGraph, ancestor: u32, local: u32) -> bool {
    let mut current = local;
    while let Some(parent) = file.blocks.get(current as usize).and_then(|block| block.parent) {
        if parent >= current {
            return false;
        }
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

fn check_results(checker: &mut Checker<'_>, file: &FileGraph, slots: &Slots, complete: bool) {
    let resolved: FxHashMap<u32, &Key> = file
        .edges
        .iter()
        .filter(|edge| edge.seq == 0 && edge.resolution == Resolution::Resolved)
        .filter_map(|edge| Some((edge.source, edge.target.as_ref()?)))
        .collect();
    let mut previous: Option<u32> = None;
    for result in &file.types.results {
        let Some(found) = block(checker, file, result.call, "result") else {
            continue;
        };
        let subject = || Subject::Result(found.key.clone());
        if previous.is_some_and(|previous| previous >= result.call) {
            checker.report(subject(), "results are not in call order, one per call");
        }
        previous = Some(result.call);
        if !matches!(found.kind, BlockKind::Call | BlockKind::New) {
            checker.report(subject(), format!("a {} has no result", found.kind));
        }
        let wants_target = matches!(result.binding, ResultBinding::Variable | ResultBinding::Argument);
        if result.target.is_some() != wants_target {
            checker.report(
                subject(),
                "a target is present exactly for a variable or argument binding",
            );
        }
        if result.position.is_some() != (result.binding == ResultBinding::Argument) {
            checker.report(subject(), "a position is present exactly for an argument binding");
        }
        if result.bound.is_some()
            && !resolved
                .get(&result.call)
                .is_some_and(|target| function_like(checker, target))
        {
            checker.report(
                subject(),
                "only a call resolved to a function-like block has a bound count",
            );
        }
        let Some(target) = result.target else {
            continue;
        };
        let Some(outer) = file.blocks.get(target as usize) else {
            checker.report(subject(), format!("target {target} is not a block of the file"));
            continue;
        };
        let kind_fits = match result.binding {
            ResultBinding::Variable => outer.kind == BlockKind::Variable,
            _ => matches!(outer.kind, BlockKind::Call | BlockKind::New),
        };
        if !kind_fits || !is_ancestor(file, target, result.call) {
            checker.report(
                subject(),
                format!(
                    "target {} is not an enclosing block of the binding's kind",
                    outer.key
                ),
            );
        }
        if let Some(position) = result.position {
            let count = slots.argument_counts.get(&target).copied().unwrap_or(0);
            let contains = slots
                .arguments
                .get(&(target, position))
                .is_some_and(|slot| slot.contains(found.range));
            if position >= count || !contains {
                checker.report(
                    subject(),
                    format!("argument {position} of the outer call does not contain the call"),
                );
            }
        }
    }
    if !complete {
        return;
    }
    let mut results = file.types.results.iter().map(|result| result.call).peekable();
    for (local, found) in file.blocks.iter().enumerate() {
        let local = local as u32;
        let has = results
            .next_if(|&call| call <= local)
            .is_some_and(|call| call == local);
        while results.next_if(|&call| call <= local).is_some() {}
        if matches!(found.kind, BlockKind::Call | BlockKind::New) && !has {
            checker.report(
                Subject::Block(found.key.clone()),
                "a call or new needs exactly one result",
            );
        }
    }
}

fn check_type_links(checker: &mut Checker<'_>, file: &FileGraph, slots: &Slots) {
    let mut previous: Option<(u32, ByteRange)> = None;
    let mut ranges: Vec<ByteRange> = Vec::with_capacity(file.types.type_links.len());
    for link in &file.types.type_links {
        let Some(found) = block(checker, file, link.block, "type link") else {
            continue;
        };
        let subject = || Subject::TypeLink {
            block: found.key.clone(),
            range: link.range,
        };
        if previous.is_some_and(|previous| previous >= (link.block, link.range)) {
            checker.report(subject(), "type links are not in block, then range, order");
        }
        previous = Some((link.block, link.range));
        ranges.push(link.range);
        if link.range.is_empty() || !found.range.contains(link.range) {
            checker.report(
                subject(),
                format!(
                    "range {} is empty or outside the block's {}",
                    link.range, found.range
                ),
            );
        } else if let Some(inner) = inner_block(file, link) {
            checker.report(
                subject(),
                format!("block {} is a nearer block containing the name", inner.key),
            );
        }
        if link.text.is_empty() || utf16_len(&link.text) > LABEL_MAX_UTF16 {
            checker.report(
                subject(),
                format!("text is empty or exceeds {LABEL_MAX_UTF16} UTF-16 code units"),
            );
        }
        check_link_outcome(checker, file, link, &subject);
        if let Some(heritage) = link.heritage {
            check_heritage(checker, file, slots, link, found, heritage, &subject);
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
        checker.report(Subject::File(file.file.path.clone()), "type links overlap");
    }
}

/// A heritage link names a base of a `class` or `interface`: `implements` only on a TypeScript
/// class, and an `extends` name inside the block's `heritage` slot (a TypeScript class's
/// `implements` clause is no slot).
fn check_heritage(
    checker: &mut Checker<'_>,
    file: &FileGraph,
    slots: &Slots,
    link: &TypeLink,
    block: &Block,
    heritage: Heritage,
    subject: &dyn Fn() -> Subject,
) {
    let permitted = match heritage {
        Heritage::Extends => matches!(block.kind, BlockKind::Class | BlockKind::Interface),
        Heritage::Implements => block.kind == BlockKind::Class && file.file.language == Language::TypeScript,
    };
    if !permitted {
        checker.report(
            subject(),
            format!("`{heritage}` is not a heritage of a {}", block.kind),
        );
    } else if heritage == Heritage::Extends
        && !slots
            .heritage
            .get(&link.block)
            .is_some_and(|slot| slot.contains(link.range))
    {
        checker.report(
            subject(),
            "an `extends` link lies outside its block's heritage slot",
        );
    }
}

/// A block nested in the link's block whose range also contains the link's range.
fn inner_block<'f>(file: &'f FileGraph, link: &TypeLink) -> Option<&'f Block> {
    let after = file.blocks.get(link.block as usize + 1..)?;
    after
        .iter()
        .take_while(|block| {
            block.range.start <= link.range.start && is_ancestor(file, link.block, block.local_id)
        })
        .find(|block| block.range.contains(link.range))
}

fn check_link_outcome(
    checker: &mut Checker<'_>,
    file: &FileGraph,
    link: &TypeLink,
    subject: &dyn Fn() -> Subject,
) {
    if (link.resolution == Resolution::Unresolved) != link.reason.is_some() {
        checker.report(subject(), "a reason is present exactly when unresolved");
    }
    match (link.resolution, &link.module) {
        (Resolution::External, Some(module)) if !module.is_valid() => checker.report(
            subject(),
            format!(
                "module `{}` is not a label or a sorted set of labels",
                module.as_str()
            ),
        ),
        (Resolution::External, Some(_)) | (Resolution::Resolved | Resolution::Unresolved, None) => {}
        _ => checker.report(subject(), "a module is present exactly when external"),
    }
    match (link.resolution, &link.target) {
        (Resolution::Resolved, Some(target)) => match checker.resolve(target) {
            None => checker.report(
                subject(),
                format!("target `{target}` is not a block in the graph"),
            ),
            Some((target_file, target_block)) => {
                if !target_block.kind.is_type_declaration() {
                    checker.report(
                        subject(),
                        format!("a type link cannot target a {}", target_block.kind),
                    );
                }
                if target_file.file.language != file.file.language {
                    checker.report(subject(), "a resolved type link cannot cross languages");
                }
            }
        },
        (Resolution::Resolved, None) | (Resolution::External | Resolution::Unresolved, Some(_)) => {
            checker.report(subject(), "a target is present exactly when resolved");
        }
        _ => {}
    }
}

/// Every typed record's range checked against the file's snapshot (character boundaries) and
/// census (token boundaries). Type links are checked against characters only: a Python string
/// annotation's name lies inside its quotes. Violations name the file, since only files read in
/// an update are checked against bytes.
pub(super) fn check_text_ranges(checker: &mut Checker<'_>, file: &FileGraph, census: Option<&dyn Census>) {
    let snapshot = checker
        .snapshot(&file.file.path)
        .filter(|snapshot| Some(snapshot.hash()) == file.file.hash);
    let types = &file.types;
    let written = types
        .signatures
        .iter()
        .filter_map(|signature| signature.returns.as_ref())
        .chain(
            types
                .parameters
                .iter()
                .filter_map(|parameter| parameter.ty.as_ref()),
        )
        .chain(types.members.iter().filter_map(|member| member.ty.as_ref()));
    for ty in written {
        let on_characters = snapshot.is_none_or(|snapshot| slice(snapshot, ty.range).is_some());
        let on_tokens = census.is_none_or(|census| census.is_token_range(&file.file.path, ty.range));
        if !on_characters || !on_tokens {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!(
                    "written type `{}` at {} is not on character and token boundaries",
                    ty.text, ty.range
                ),
            );
        }
    }
    if let Some(snapshot) = snapshot {
        for link in &types.type_links {
            if slice(snapshot, link.range).is_none() {
                checker.report(
                    Subject::File(file.file.path.clone()),
                    format!(
                        "type link `{}` at {} is not on character boundaries",
                        link.text, link.range
                    ),
                );
            }
        }
    }
}
