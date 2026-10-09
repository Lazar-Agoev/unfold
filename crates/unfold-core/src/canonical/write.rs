//! The canonical export writer.

use std::io;

use serde::Serialize;

use super::{FORMAT, VERSION};
use crate::model::{Edge, Entrance, FileGraph, Graph, Slot};
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::vocab::{
    BlockKind, DetectorId, EdgeKind, EntranceKind, FileState, HttpMethod, InputRole, Language, Reason,
    Resolution, Role, SlotName, TargetRole, TriggerStatus,
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
/// range, slot, index), and edges (by source, sequence); then inputs in path order; then
/// entrances in canonical order, each followed by its targets in position order. The output
/// depends only on the graph's contents, not on the order its vectors happen to be in.
///
/// Each record is several small writes, so pass a buffered writer (such as a `BufWriter` or a
/// `Vec`).
pub fn write(graph: &Graph, out: &mut impl io::Write) -> io::Result<()> {
    let mut writer = LineWriter { out };
    writer.record(&HeaderOut {
        t: "header",
        format: FORMAT,
        version: VERSION,
    })?;

    let mut files: Vec<&FileGraph> = graph.files.iter().collect();
    files.sort_by(|a, b| a.file.path.cmp(&b.file.path));
    for file in files {
        write_file(&mut writer, file)?;
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

fn write_file(writer: &mut LineWriter<'_, impl io::Write>, file: &FileGraph) -> io::Result<()> {
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
