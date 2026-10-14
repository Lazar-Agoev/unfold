//! The canonical export writer.

use std::io;

use serde::Serialize;

use super::{FORMAT, VERSION};
use crate::model::{App, EntranceTarget, FileGraph, Graph, Link, LinkTarget};
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::{FileTypes, Pairing};
use crate::vocab::{
    CallRole, Expansion, FileState, Flags, Form, InputRole, Language, LinkKind, MemberKind, NodeKind,
    Outcome, PairingReason, ParameterKind, TargetState,
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
struct NodeOut<'a> {
    t: &'static str,
    key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<&'a str>,
    kind: NodeKind,
    form: Form,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    range: ByteRange,
    lines: LineSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    head: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stmt: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<ByteRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binds: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<CallRole>,
    #[serde(rename = "for", skip_serializing_if = "Option::is_none")]
    r#for: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<Flags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
}

#[derive(Serialize)]
struct LinkOut<'a> {
    t: &'static str,
    from: &'a str,
    seq: u32,
    kind: LinkKind,
    range: ByteRange,
    text: &'a str,
    outcome: Outcome,
    to: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    implements: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bound: Option<u32>,
}

#[derive(Serialize)]
struct SignatureOut<'a> {
    t: &'static str,
    node: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns_range: Option<ByteRange>,
}

#[derive(Serialize)]
struct ParameterOut<'a> {
    t: &'static str,
    node: &'a str,
    index: u32,
    range: ByteRange,
    kind: ParameterKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ty: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    type_range: Option<ByteRange>,
}

#[derive(Serialize)]
struct MemberOut<'a> {
    t: &'static str,
    node: &'a str,
    index: u32,
    range: ByteRange,
    kind: MemberKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
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
    range: ByteRange,
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
struct InputOut<'a> {
    t: &'static str,
    path: &'a str,
    role: InputRole,
    hash: ContentHash,
}

#[derive(Serialize)]
struct AppOut<'a> {
    t: &'static str,
    key: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    run: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<&'a str>,
}

#[derive(Serialize)]
struct EntranceOut<'a> {
    t: &'static str,
    key: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    on: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<&'a str>,
}

#[derive(Serialize)]
struct TargetOut<'a> {
    t: &'static str,
    of: &'a str,
    code: &'a str,
    state: TargetState,
}

/// A flag as the export writes it: present only when true.
fn flag(value: bool) -> Option<bool> {
    value.then_some(true)
}

/// Writes `graph` as canonical JSON Lines.
///
/// The header comes first; then files in path order, each followed by its nodes (in the
/// graph's preorder), its links (by `from`, then `seq`), and its typed records (signatures,
/// parameters, members, and arguments, each by node, then index); then inputs in path order;
/// then apps in file order, each followed by its own `code` target and its entrances, and each
/// entrance by its targets. Apart from the nodes and the apps, whose order is their meaning,
/// the output doesn't depend on the order of the graph's vectors.
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
    for file in sorted(&graph.files, |file| file.file.path.clone()) {
        write_file(&mut writer, file)?;
    }
    for input in sorted(&graph.inputs, |input| input.path.clone()) {
        writer.record(&InputOut {
            t: "input",
            path: input.path.as_str(),
            role: input.role,
            hash: input.hash,
        })?;
    }
    for app in &graph.apps {
        write_app(&mut writer, app)?;
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

/// The items sorted by `key`, without copying them.
fn sorted<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> Vec<&T> {
    let mut sorted: Vec<&T> = items.iter().collect();
    sorted.sort_by_key(|item| key(item));
    sorted
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
    // A local ID with no node is a broken graph; it is written as an empty key, which
    // reading rejects.
    let key = |local_id: u32| {
        file.nodes
            .get(local_id as usize)
            .map_or("", |node| node.key.as_str())
    };
    for node in &file.nodes {
        writer.record(&NodeOut {
            t: "node",
            key: node.key.as_str(),
            parent: node.parent.map(key),
            kind: node.kind(),
            form: node.form,
            name: node.name.as_deref(),
            range: node.range,
            lines: node.lines,
            head: node.head,
            body: node.body,
            doc: node.doc,
            stmt: node.stmt,
            value: node.value,
            binds: node.binds.as_deref(),
            role: node.role,
            r#for: node.r#for.map(key),
            flags: (!node.flags.is_empty()).then_some(node.flags),
            target: node.target.map(key),
        })?;
    }
    for link in sorted(&file.links, |link| (link.from, link.seq)) {
        write_link(writer, link, key(link.from))?;
    }
    write_types(writer, &file.types, &key)
}

fn write_link(writer: &mut LineWriter<'_, impl io::Write>, link: &Link, from: &str) -> io::Result<()> {
    let to = match &link.to {
        LinkTarget::Project(key) => key.as_str(),
        LinkTarget::Library(label) | LinkTarget::Builtin(label) => label.as_str(),
        LinkTarget::Unknown(reason) => reason.as_str(),
    };
    writer.record(&LinkOut {
        t: "link",
        from,
        seq: link.seq,
        kind: link.kind,
        range: link.range,
        text: &link.text,
        outcome: link.to.outcome(),
        to,
        implements: flag(link.implements),
        bound: link.bound,
    })
}

fn write_types<'a>(
    writer: &mut LineWriter<'_, impl io::Write>,
    types: &'a FileTypes,
    key: &dyn Fn(u32) -> &'a str,
) -> io::Result<()> {
    for signature in sorted(&types.signatures, |signature| signature.node) {
        writer.record(&SignatureOut {
            t: "signature",
            node: key(signature.node),
            returns: signature.returns.as_ref().map(|returns| &*returns.text),
            returns_range: signature.returns.as_ref().map(|returns| returns.range),
        })?;
    }
    for parameter in sorted(&types.parameters, |parameter| (parameter.node, parameter.index)) {
        writer.record(&ParameterOut {
            t: "parameter",
            node: key(parameter.node),
            index: parameter.index,
            range: parameter.range,
            kind: parameter.kind,
            name: parameter.name.as_deref(),
            optional: flag(parameter.optional),
            default: flag(parameter.default),
            ty: parameter.ty.as_ref().map(|ty| &*ty.text),
            type_range: parameter.ty.as_ref().map(|ty| ty.range),
        })?;
    }
    for member in sorted(&types.members, |member| (member.node, member.index)) {
        writer.record(&MemberOut {
            t: "member",
            node: key(member.node),
            index: member.index,
            range: member.range,
            kind: member.kind,
            name: member.name.as_deref(),
            optional: flag(member.optional),
            readonly: flag(member.readonly),
            default: flag(member.default),
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
            call: key(argument.call),
            index: argument.index,
            range: argument.range,
            expand: argument.expand,
            keyword: argument.keyword.as_deref(),
            parameter,
            unpaired,
            bound: argument.bound,
        })?;
    }
    Ok(())
}

fn write_app(writer: &mut LineWriter<'_, impl io::Write>, app: &App) -> io::Result<()> {
    writer.record(&AppOut {
        t: "app",
        key: app.key.as_str(),
        name: &app.name,
        run: app.run.as_deref(),
        about: app.about.as_deref(),
    })?;
    if let Some(code) = &app.code {
        write_target(writer, app.key.as_str(), code)?;
    }
    for entrance in &app.entrances {
        writer.record(&EntranceOut {
            t: "entrance",
            key: entrance.key.as_str(),
            name: &entrance.name,
            on: entrance.on.as_deref(),
            about: entrance.about.as_deref(),
        })?;
        for code in &entrance.code {
            write_target(writer, entrance.key.as_str(), code)?;
        }
    }
    Ok(())
}

fn write_target(
    writer: &mut LineWriter<'_, impl io::Write>,
    of: &str,
    target: &EntranceTarget,
) -> io::Result<()> {
    writer.record(&TargetOut {
        t: "entrance_target",
        of,
        code: target.code.as_str(),
        state: target.state,
    })
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
