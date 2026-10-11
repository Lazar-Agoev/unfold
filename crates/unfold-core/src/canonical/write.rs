//! The canonical export writer.

use std::io;

use serde::Serialize;

use super::{FORMAT, VERSION};
use crate::model::{Edge, Entrance, FileGraph, Graph, Slot};
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::{FileTypes, Pairing};
use crate::vocab::{
    BlockKind, DetectorId, EdgeKind, EntranceKind, Expansion, FileState, Heritage, HttpMethod, InputRole,
    Language, MemberKind, PairingReason, ParameterKind, Reason, Resolution, ResultBinding, Role, SlotName,
    TargetRole, TriggerStatus,
};

#[derive(Serialize)]
struct HeaderOut {
    t: &'static str,
    format: &'static str,
    version: u32,
}

#[derive(Serialize)]
struct FileOut<'a> {
    t: &'static str,
    path: &'a str,
    language: Language,
    state: FileState,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<ContentHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<&'a str>,
}

#[derive(Serialize)]
struct BlockOut<'a> {
    t: &'static str,
    key: &'a str,
    kind: BlockKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    range: ByteRange,
    lines: LineSpan,
}

#[derive(Serialize)]
struct SlotOut<'a> {
    t: &'static str,
    block: &'a str,
    slot: SlotName,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<u32>,
    range: ByteRange,
}

#[derive(Serialize)]
struct EdgeOut<'a> {
    t: &'static str,
    source: &'a str,
    seq: u32,
    kind: EdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    text: &'a str,
    resolution: Resolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u32>,
}

/// Whether a flag is absent from the export (it is written only when true).
#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes fields by reference.
fn is_false(flag: &bool) -> bool {
    !*flag
}

#[derive(Serialize)]
struct SignatureOut<'a> {
    t: &'static str,
    block: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns_range: Option<ByteRange>,
}

#[derive(Serialize)]
struct ParameterOut<'a> {
    t: &'static str,
    block: &'a str,
    index: u32,
    kind: ParameterKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    optional: bool,
    #[serde(skip_serializing_if = "is_false")]
    default: bool,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ty: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    type_range: Option<ByteRange>,
}

#[derive(Serialize)]
struct MemberOut<'a> {
    t: &'static str,
    block: &'a str,
    index: u32,
    kind: MemberKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    optional: bool,
    #[serde(skip_serializing_if = "is_false")]
    readonly: bool,
    #[serde(skip_serializing_if = "is_false")]
    default: bool,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ty: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    type_range: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Serialize)]
struct ArgumentOut<'a> {
    t: &'static str,
    call: &'a str,
    index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    expand: Option<Expansion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keyword: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unpaired: Option<PairingReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bound: Option<u32>,
}

#[derive(Serialize)]
struct ResultOut<'a> {
    t: &'static str,
    call: &'a str,
    binding: ResultBinding,
    #[serde(skip_serializing_if = "is_false")]
    awaited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bound: Option<u32>,
}

#[derive(Serialize)]
struct TypeLinkOut<'a> {
    t: &'static str,
    block: &'a str,
    range: ByteRange,
    text: &'a str,
    resolution: Resolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heritage: Option<Heritage>,
}

#[derive(Serialize)]
struct InputOut<'a> {
    t: &'static str,
    path: &'a str,
    role: InputRole,
    hash: ContentHash,
}

#[derive(Serialize)]
struct EntranceOut<'a> {
    t: &'static str,
    key: &'a str,
    kind: EntranceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<HttpMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger: Option<&'a str>,
    trigger_status: TriggerStatus,
    detector: DetectorId,
    evidence_path: &'a str,
    evidence_range: ByteRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_block: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_pointer: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binding: Option<&'a str>,
}

#[derive(Serialize)]
struct TargetOut<'a> {
    t: &'static str,
    entrance: &'a str,
    position: u32,
    role: TargetRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    text: &'a str,
    resolution: Resolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<&'a str>,
}

/// Writes `graph` as canonical JSON Lines.
///
/// Files are written in path order, each followed by its blocks (preorder), slots (by block,
/// range, slot, index), edges (by source, sequence), and typed records (signatures, parameters,
/// members, arguments, and results by block then index; type links by block then range); then
/// inputs in path order; then
/// entrances in canonical order, each followed by its targets in position order. The output
/// depends only on the graph's contents, not on the order its vectors happen to be in.
///
/// Each record is several small writes, so pass a buffered writer (such as a `BufWriter` or a
/// `Vec`).
pub fn write(graph: &Graph, out: &mut impl io::Write) -> io::Result<()> {
    write_version(graph, VERSION, out)
}

/// [`write`] with the header's `version`. Version 1 predates typed records, so they are not
/// written; it is for writing back an export read at version 1.
pub fn write_version(graph: &Graph, version: u32, out: &mut impl io::Write) -> io::Result<()> {
    let mut writer = LineWriter { out };
    writer.record(&HeaderOut {
        t: "header",
        format: FORMAT,
        version,
    })?;

    let mut files: Vec<&FileGraph> = graph.files.iter().collect();
    files.sort_by(|a, b| a.file.path.cmp(&b.file.path));
    for file in files {
        write_file(&mut writer, file, version >= 2)?;
    }

    let mut inputs: Vec<_> = graph.inputs.iter().collect();
    inputs.sort_by(|a, b| a.path.cmp(&b.path));
    for input in inputs {
        writer.record(&InputOut {
            t: "input",
            path: input.path.as_str(),
            role: input.role,
            hash: input.hash,
        })?;
    }

    let mut entrances: Vec<&Entrance> = graph.entrances.iter().collect();
    entrances.sort_by(|a, b| a.canonical_cmp(b));
    for entrance in entrances {
        write_entrance(&mut writer, entrance)?;
    }
    Ok(())
}

/// Writes `graph` as canonical JSON Lines into a string.
pub fn to_string(graph: &Graph) -> String {
    let mut bytes = Vec::new();
    // Writing to a Vec cannot fail, and serde_json writes only valid UTF-8.
    let _ = write(graph, &mut bytes);
    String::from_utf8(bytes).unwrap_or_default()
}

fn write_file(writer: &mut LineWriter<'_, impl io::Write>, file: &FileGraph, typed: bool) -> io::Result<()> {
    let record = &file.file;
    writer.record(&FileOut {
        t: "file",
        path: record.path.as_str(),
        language: record.language,
        state: record.state,
        hash: record.hash,
        failure: record.failure.as_deref(),
    })?;
    for block in &file.blocks {
        writer.record(&BlockOut {
            t: "block",
            key: block.key.as_str(),
            kind: block.kind,
            role: block.role,
            name: block.name.as_deref(),
            range: block.range,
            lines: block.lines,
        })?;
    }
    let block_key = |local_id: u32| {
        file.blocks
            .get(local_id as usize)
            .map_or("", |block| block.key.as_str())
    };

    let mut slots: Vec<&Slot> = file.slots.iter().collect();
    slots.sort_by(|a, b| {
        (a.block, a.range, a.slot.as_str(), a.index).cmp(&(b.block, b.range, b.slot.as_str(), b.index))
    });
    for slot in slots {
        writer.record(&SlotOut {
            t: "slot",
            block: block_key(slot.block),
            slot: slot.slot,
            index: slot.index,
            range: slot.range,
        })?;
    }

    let mut edges: Vec<&Edge> = file.edges.iter().collect();
    edges.sort_by_key(|edge| (edge.source, edge.seq));
    for edge in edges {
        writer.record(&EdgeOut {
            t: "edge",
            source: block_key(edge.source),
            seq: edge.seq,
            kind: edge.kind,
            target: edge.target.as_ref().map(|key| key.as_str()),
            text: &edge.text,
            resolution: edge.resolution,
            reason: edge.reason,
            module: edge.module.as_ref().map(|module| module.as_str()),
            position: edge.position,
        })?;
    }
    if typed {
        write_types(writer, &file.types, &block_key)?;
    }
    Ok(())
}

/// The records of `items` sorted by `key`, without copying them.
fn sorted<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> Vec<&T> {
    let mut sorted: Vec<&T> = items.iter().collect();
    sorted.sort_by_key(|item| key(item));
    sorted
}

fn write_types<'a>(
    writer: &mut LineWriter<'_, impl io::Write>,
    types: &'a FileTypes,
    block_key: &dyn Fn(u32) -> &'a str,
) -> io::Result<()> {
    for signature in sorted(&types.signatures, |signature| signature.block) {
        writer.record(&SignatureOut {
            t: "signature",
            block: block_key(signature.block),
            returns: signature.returns.as_ref().map(|returns| &*returns.text),
            returns_range: signature.returns.as_ref().map(|returns| returns.range),
        })?;
    }
    for parameter in sorted(&types.parameters, |parameter| (parameter.block, parameter.index)) {
        writer.record(&ParameterOut {
            t: "parameter",
            block: block_key(parameter.block),
            index: parameter.index,
            kind: parameter.kind,
            name: parameter.name.as_deref(),
            optional: parameter.optional,
            default: parameter.default,
            ty: parameter.ty.as_ref().map(|ty| &*ty.text),
            type_range: parameter.ty.as_ref().map(|ty| ty.range),
        })?;
    }
    for member in sorted(&types.members, |member| (member.block, member.index)) {
        writer.record(&MemberOut {
            t: "member",
            block: block_key(member.block),
            index: member.index,
            kind: member.kind,
            name: member.name.as_deref(),
            optional: member.optional,
            readonly: member.readonly,
            default: member.default,
            ty: member.ty.as_ref().map(|ty| &*ty.text),
            type_range: member.ty.as_ref().map(|ty| ty.range),
            value: member.value.as_deref(),
        })?;
    }
    for argument in sorted(&types.arguments, |argument| (argument.call, argument.index)) {
        let (parameter, unpaired) = match argument.pairing {
            Some(Pairing::Parameter(index)) => (Some(index), None),
            Some(Pairing::Unpaired(reason)) => (None, Some(reason)),
            None => (None, None),
        };
        writer.record(&ArgumentOut {
            t: "argument",
            call: block_key(argument.call),
            index: argument.index,
            expand: argument.expand,
            keyword: argument.keyword.as_deref(),
            parameter,
            unpaired,
            bound: argument.bound,
        })?;
    }
    for result in sorted(&types.results, |result| result.call) {
        writer.record(&ResultOut {
            t: "result",
            call: block_key(result.call),
            binding: result.binding,
            awaited: result.awaited,
            target: result.target.map(block_key),
            position: result.position,
            bound: result.bound,
        })?;
    }
    for link in sorted(&types.type_links, |link| (link.block, link.range)) {
        writer.record(&TypeLinkOut {
            t: "type_link",
            block: block_key(link.block),
            range: link.range,
            text: &link.text,
            resolution: link.resolution,
            target: link.target.as_ref().map(|key| key.as_str()),
            reason: link.reason,
            module: link.module.as_ref().map(|module| module.as_str()),
            heritage: link.heritage,
        })?;
    }
    Ok(())
}

fn write_entrance(writer: &mut LineWriter<'_, impl io::Write>, entrance: &Entrance) -> io::Result<()> {
    writer.record(&EntranceOut {
        t: "entrance",
        key: entrance.key.as_str(),
        kind: entrance.kind,
        label: entrance.label.as_deref(),
        method: entrance.method,
        trigger: entrance.trigger.as_deref(),
        trigger_status: entrance.trigger_status,
        detector: entrance.detector,
        evidence_path: entrance.evidence.path.as_str(),
        evidence_range: entrance.evidence.range,
        evidence_block: entrance.evidence.block.as_ref().map(|key| key.as_str()),
        evidence_pointer: entrance.evidence.pointer.as_deref(),
        binding: entrance.binding.as_ref().map(|key| key.as_str()),
    })?;
    let mut targets: Vec<_> = entrance.targets.iter().collect();
    targets.sort_by_key(|target| target.position);
    for target in targets {
        writer.record(&TargetOut {
            t: "target",
            entrance: entrance.key.as_str(),
            position: target.position,
            role: target.role,
            target: target.target.as_ref().map(|key| key.as_str()),
            text: &target.text,
            resolution: target.resolution,
            reason: target.reason,
            module: target.module.as_ref().map(|module| module.as_str()),
        })?;
    }
    Ok(())
}

struct LineWriter<'a, W: io::Write> {
    out: &'a mut W,
}

impl<W: io::Write> LineWriter<'_, W> {
    fn record(&mut self, record: &impl Serialize) -> io::Result<()> {
        serde_json::to_writer(&mut *self.out, record).map_err(io::Error::other)?;
        self.out.write_all(b"\n")
    }
}
