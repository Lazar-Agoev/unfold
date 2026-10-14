//! The canonical export reader.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::{FORMAT, VERSION};
use crate::key::Key;
use crate::model::{
    App, Entrance, EntranceTarget, File, FileGraph, Graph, Input, Link, LinkTarget, ModuleLabel, Node,
};
use crate::path::RepoPath;
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::{Argument, FileTypes, Member, Pairing, Parameter, Signature, WrittenType};
use crate::vocab::{
    CallRole, Expansion, FileState, Flag, Flags, Form, InputRole, Language, LinkKind, MemberKind, NodeKind,
    Outcome, PairingReason, ParameterKind, Reason, TargetState,
};

/// A canonical export that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct CanonicalError {
    /// The one-based line of the offending record.
    pub line: usize,
    /// What is wrong with it.
    pub message: String,
}

/// One line of the export, exactly as [`write`](super::write) produces it: every field a
/// record type doesn't have is rejected.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Record {
    Header(HeaderIn),
    File(FileIn),
    Node(NodeIn),
    Link(LinkIn),
    Signature(SignatureIn),
    Parameter(ParameterIn),
    Member(MemberIn),
    Argument(ArgumentIn),
    Input(InputIn),
    App(AppIn),
    Entrance(EntranceIn),
    EntranceTarget(TargetIn),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderIn {
    format: Box<str>,
    version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIn {
    path: RepoPath,
    language: Language,
    state: FileState,
    #[serde(default, deserialize_with = "present")]
    hash: Option<ContentHash>,
    #[serde(default, deserialize_with = "present")]
    failure: Option<Box<str>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeIn {
    key: Key,
    #[serde(default, deserialize_with = "present")]
    parent: Option<Box<str>>,
    kind: NodeKind,
    form: Form,
    #[serde(default, deserialize_with = "present")]
    name: Option<Box<str>>,
    range: ByteRange,
    lines: LineSpan,
    #[serde(default, deserialize_with = "present")]
    head: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    body: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    doc: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    stmt: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    value: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    binds: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    role: Option<CallRole>,
    #[serde(rename = "for", default, deserialize_with = "present")]
    r#for: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    flags: Option<Vec<Flag>>,
    #[serde(default, deserialize_with = "present")]
    target: Option<Box<str>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkIn {
    from: Box<str>,
    seq: u32,
    kind: LinkKind,
    range: ByteRange,
    text: Box<str>,
    outcome: Outcome,
    to: Box<str>,
    #[serde(default, deserialize_with = "present")]
    implements: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    bound: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureIn {
    node: Box<str>,
    #[serde(default, deserialize_with = "present")]
    returns: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    returns_range: Option<ByteRange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterIn {
    node: Box<str>,
    index: u32,
    range: ByteRange,
    kind: ParameterKind,
    #[serde(default, deserialize_with = "present")]
    name: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    optional: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    default: Option<bool>,
    #[serde(rename = "type", default, deserialize_with = "present")]
    ty: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    type_range: Option<ByteRange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberIn {
    node: Box<str>,
    index: u32,
    range: ByteRange,
    kind: MemberKind,
    #[serde(default, deserialize_with = "present")]
    name: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    optional: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    readonly: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    default: Option<bool>,
    #[serde(rename = "type", default, deserialize_with = "present")]
    ty: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    type_range: Option<ByteRange>,
    #[serde(default, deserialize_with = "present")]
    value: Option<Box<str>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArgumentIn {
    call: Box<str>,
    index: u32,
    range: ByteRange,
    #[serde(default, deserialize_with = "present")]
    expand: Option<Expansion>,
    #[serde(default, deserialize_with = "present")]
    keyword: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    parameter: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    unpaired: Option<PairingReason>,
    #[serde(default, deserialize_with = "present")]
    bound: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputIn {
    path: RepoPath,
    role: InputRole,
    hash: ContentHash,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppIn {
    key: Key,
    name: Box<str>,
    #[serde(default, deserialize_with = "present")]
    run: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    about: Option<Box<str>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntranceIn {
    key: Key,
    name: Box<str>,
    #[serde(default, deserialize_with = "present")]
    on: Option<Box<str>>,
    #[serde(default, deserialize_with = "present")]
    about: Option<Box<str>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetIn {
    of: Box<str>,
    code: Key,
    state: TargetState,
}

impl Record {
    /// The record's `t`.
    fn name(&self) -> &'static str {
        match self {
            Self::Header(_) => "header",
            Self::File(_) => "file",
            Self::Node(_) => "node",
            Self::Link(_) => "link",
            Self::Signature(_) => "signature",
            Self::Parameter(_) => "parameter",
            Self::Member(_) => "member",
            Self::Argument(_) => "argument",
            Self::Input(_) => "input",
            Self::App(_) => "app",
            Self::Entrance(_) => "entrance",
            Self::EntranceTarget(_) => "entrance_target",
        }
    }

    /// The record's place in the export's order: a record never follows one of a higher
    /// place, except that a file starts again at `file`. The records of an app interleave.
    fn place(&self) -> u8 {
        match self {
            Self::Header(_) => 0,
            Self::File(_) => 1,
            Self::Node(_) => 2,
            Self::Link(_) => 3,
            Self::Signature(_) => 4,
            Self::Parameter(_) => 5,
            Self::Member(_) => 6,
            Self::Argument(_) => 7,
            Self::Input(_) => 8,
            Self::App(_) | Self::Entrance(_) | Self::EntranceTarget(_) => 9,
        }
    }
}

/// The place of a file's last records; a `file` record may follow any record up to it.
const LAST_FILE_PLACE: u8 = 7;

type Fail = String;

/// An optional field that is written: the writer leaves an absent field out, so an explicit
/// `null` is rejected (read-then-write stays byte-identical).
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        Some(value) => Ok(Some(value)),
        None => Err(D::Error::custom(
            "an absent field is left out, never written as null",
        )),
    }
}

/// A flag, which the export writes only when true.
fn flag(value: Option<bool>, field: &str) -> Result<bool, Fail> {
    match value {
        Some(false) => Err(format!("`{field}` is written only when true")),
        Some(true) => Ok(true),
        None => Ok(false),
    }
}

/// A written type from its text and range, which appear together or not at all.
fn written(
    text: Option<Box<str>>,
    range: Option<ByteRange>,
    field: &str,
) -> Result<Option<WrittenType>, Fail> {
    match (text, range) {
        (Some(text), Some(range)) => Ok(Some(WrittenType { text, range })),
        (None, None) => Ok(None),
        _ => Err(format!("`{field}` and `{field}_range` appear together")),
    }
}

/// A node's flags, which the export writes only when there are some, once each, in
/// vocabulary order.
fn flags(written: Option<Vec<Flag>>) -> Result<Flags, Fail> {
    let Some(flags) = written else {
        return Ok(Flags::NONE);
    };
    if flags.is_empty() || flags.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("`flags` are written only when there are some, once each, in vocabulary order".into());
    }
    Ok(flags.into_iter().collect())
}

/// A node field that names another node of its file, resolved when the file's nodes are all
/// read, since a decorator's `for` names a definition after it.
#[derive(Debug, Clone, Copy)]
enum Reference {
    For,
    Target,
}

#[derive(Debug)]
struct Pending {
    line: usize,
    node: usize,
    field: Reference,
    key: Box<str>,
}

#[derive(Default)]
struct Reader {
    graph: Graph,
    /// The line being read.
    line: usize,
    /// The name and place of the last record read; `None` before the header.
    last: Option<(&'static str, u8)>,
    /// The current file's node keys, by local ID.
    local_ids: FxHashMap<Key, u32>,
    /// The current file's `for` and `target` fields, waiting for the file's nodes.
    pending: Vec<Pending>,
}

/// Reads a canonical export. It accepts exactly the shapes [`write`](super::write) produces,
/// rebuilds local IDs and parents from the keys, and checks that each node's `kind` is its
/// form's. Graph invariants are left to the validator.
pub fn read(text: &str) -> Result<Graph, CanonicalError> {
    if text.is_empty() {
        return Err(CanonicalError {
            line: 1,
            message: "missing header record".into(),
        });
    }
    let Some(body) = text.strip_suffix('\n') else {
        return Err(CanonicalError {
            line: text.split('\n').count(),
            message: "the export must end with a newline".into(),
        });
    };
    let mut reader = Reader::default();
    for (index, line) in body.split('\n').enumerate() {
        reader.line = index + 1;
        reader.read_line(line)?;
    }
    reader.finish_file()?;
    Ok(reader.graph)
}

impl Reader {
    fn fail(&self, message: String) -> CanonicalError {
        CanonicalError {
            line: self.line,
            message,
        }
    }

    fn read_line(&mut self, line: &str) -> Result<(), CanonicalError> {
        let record: Record =
            serde_json::from_str(line).map_err(|error| self.fail(format!("invalid record: {error}")))?;
        self.check_order(&record).map_err(|message| self.fail(message))?;
        if matches!(record, Record::File(_) | Record::Input(_) | Record::App(_)) {
            self.finish_file()?;
        }
        self.record(record).map_err(|message| self.fail(message))
    }

    /// Checks that `record` may follow the records before it.
    fn check_order(&mut self, record: &Record) -> Result<(), Fail> {
        let (name, place) = (record.name(), record.place());
        match self.last {
            None if place != 0 => return Err("the first record must be the header".into()),
            Some(_) if place == 0 => return Err("repeated header".into()),
            Some((last, last_place)) => {
                // The latest place the record before this one may have.
                let latest = if place == 1 { LAST_FILE_PLACE } else { place };
                if last_place > latest {
                    return Err(format!("`{name}` records cannot follow `{last}` records"));
                }
                if place > 1 && place <= LAST_FILE_PLACE && last_place == 0 {
                    return Err(format!("`{name}` records must follow a `file` record"));
                }
            }
            None => {}
        }
        self.last = Some((name, place));
        Ok(())
    }

    fn record(&mut self, record: Record) -> Result<(), Fail> {
        match record {
            Record::Header(header) => check_header(&header),
            Record::File(file) => {
                self.file(file);
                Ok(())
            }
            Record::Node(node) => self.node(node),
            Record::Link(link) => self.link(link),
            Record::Signature(signature) => self.signature(signature),
            Record::Parameter(parameter) => self.parameter(parameter),
            Record::Member(member) => self.member(member),
            Record::Argument(argument) => self.argument(argument),
            Record::Input(input) => {
                self.graph.inputs.push(Input {
                    path: input.path,
                    role: input.role,
                    hash: input.hash,
                });
                Ok(())
            }
            Record::App(app) => {
                self.graph.apps.push(App {
                    key: app.key,
                    name: app.name,
                    run: app.run,
                    code: None,
                    about: app.about,
                    entrances: Vec::new(),
                });
                Ok(())
            }
            Record::Entrance(entrance) => self.entrance(entrance),
            Record::EntranceTarget(target) => self.target(target),
        }
    }

    fn file(&mut self, file: FileIn) {
        self.local_ids.clear();
        self.graph.files.push(Arc::new(FileGraph {
            file: File {
                path: file.path,
                language: file.language,
                state: file.state,
                hash: file.hash,
                failure: file.failure,
            },
            nodes: Vec::new(),
            links: Vec::new(),
            types: FileTypes::default(),
        }));
    }

    /// Resolves the current file's pending `for` and `target` fields.
    fn finish_file(&mut self) -> Result<(), CanonicalError> {
        let pending = std::mem::take(&mut self.pending);
        // Nothing is pending before the first file.
        let Some(file) = self.graph.files.last_mut().map(Arc::make_mut) else {
            return Ok(());
        };
        for pending in pending {
            let local_id = self
                .local_ids
                .get(&*pending.key)
                .copied()
                .ok_or_else(|| CanonicalError {
                    line: pending.line,
                    message: format!("`{}` is not a node of the current file", pending.key),
                })?;
            let node = &mut file.nodes[pending.node];
            match pending.field {
                Reference::For => node.r#for = Some(local_id),
                Reference::Target => node.target = Some(local_id),
            }
        }
        Ok(())
    }

    /// The current file. The order check has made sure there is one.
    fn current(&mut self) -> Result<&mut FileGraph, Fail> {
        self.graph
            .files
            .last_mut()
            .map(Arc::make_mut)
            .ok_or_else(|| "records of a file must follow a `file` record".into())
    }

    fn local_id(&self, key: &str) -> Result<u32, Fail> {
        self.local_ids
            .get(key)
            .copied()
            .ok_or_else(|| format!("`{key}` is not a node of the current file"))
    }

    fn node(&mut self, node: NodeIn) -> Result<(), Fail> {
        if node.kind != node.form.kind() {
            return Err(format!(
                "kind `{}` does not match form `{}`, which is a `{}`",
                node.kind,
                node.form,
                node.form.kind()
            ));
        }
        let flags = flags(node.flags)?;
        if self.local_ids.contains_key(node.key.as_str()) {
            return Err(format!("duplicate node key `{}`", node.key));
        }
        let parent = match node.parent {
            Some(parent) => Some(
                self.local_id(&parent)
                    .map_err(|_| format!("parent `{parent}` is not an earlier node of the current file"))?,
            ),
            None => None,
        };
        let line = self.line;
        let file = self.current()?;
        let index = file.nodes.len();
        if parent.is_none() && index > 0 {
            return Err("only a file's first node, its module, has no `parent`".into());
        }
        let local_id = u32::try_from(index).map_err(|_| "too many nodes".to_string())?;
        file.nodes.push(Node {
            key: node.key.clone(),
            parent,
            form: node.form,
            name: node.name,
            range: node.range,
            lines: node.lines,
            head: node.head,
            body: node.body,
            doc: node.doc,
            stmt: node.stmt,
            value: node.value,
            binds: node.binds,
            role: node.role,
            r#for: None,
            flags,
            target: None,
        });
        self.local_ids.insert(node.key, local_id);
        for (field, key) in [(Reference::For, node.r#for), (Reference::Target, node.target)] {
            if let Some(key) = key {
                self.pending.push(Pending {
                    line,
                    node: index,
                    field,
                    key,
                });
            }
        }
        Ok(())
    }

    fn link(&mut self, link: LinkIn) -> Result<(), Fail> {
        let from = self.local_id(&link.from)?;
        let to = match link.outcome {
            Outcome::Project => LinkTarget::Project(Key::from_text(&link.to)),
            Outcome::Library => LinkTarget::Library(ModuleLabel::from_text(&link.to)),
            Outcome::Builtin => LinkTarget::Builtin(ModuleLabel::from_text(&link.to)),
            Outcome::Unknown => {
                LinkTarget::Unknown(link.to.parse::<Reason>().map_err(|error| error.to_string())?)
            }
        };
        let link = Link {
            from,
            seq: link.seq,
            kind: link.kind,
            range: link.range,
            text: link.text,
            to,
            implements: flag(link.implements, "implements")?,
            bound: link.bound,
        };
        self.current()?.links.push(link);
        Ok(())
    }

    fn signature(&mut self, signature: SignatureIn) -> Result<(), Fail> {
        let signature = Signature {
            node: self.local_id(&signature.node)?,
            returns: written(signature.returns, signature.returns_range, "returns")?,
        };
        self.current()?.types.signatures.push(signature);
        Ok(())
    }

    fn parameter(&mut self, parameter: ParameterIn) -> Result<(), Fail> {
        let parameter = Parameter {
            node: self.local_id(&parameter.node)?,
            index: parameter.index,
            range: parameter.range,
            kind: parameter.kind,
            name: parameter.name,
            optional: flag(parameter.optional, "optional")?,
            default: flag(parameter.default, "default")?,
            ty: written(parameter.ty, parameter.type_range, "type")?,
        };
        self.current()?.types.parameters.push(parameter);
        Ok(())
    }

    fn member(&mut self, member: MemberIn) -> Result<(), Fail> {
        let member = Member {
            node: self.local_id(&member.node)?,
            index: member.index,
            range: member.range,
            kind: member.kind,
            name: member.name,
            optional: flag(member.optional, "optional")?,
            readonly: flag(member.readonly, "readonly")?,
            default: flag(member.default, "default")?,
            ty: written(member.ty, member.type_range, "type")?,
            value: member.value,
        };
        self.current()?.types.members.push(member);
        Ok(())
    }

    fn argument(&mut self, argument: ArgumentIn) -> Result<(), Fail> {
        let pairing = match (argument.parameter, argument.unpaired) {
            (Some(index), None) => Some(Pairing::Parameter(index)),
            (None, Some(reason)) => Some(Pairing::Unpaired(reason)),
            (None, None) => None,
            (Some(_), Some(_)) => return Err("`parameter` and `unpaired` exclude each other".into()),
        };
        let argument = Argument {
            call: self.local_id(&argument.call)?,
            index: argument.index,
            range: argument.range,
            expand: argument.expand,
            keyword: argument.keyword,
            pairing,
            bound: argument.bound,
        };
        self.current()?.types.arguments.push(argument);
        Ok(())
    }

    fn entrance(&mut self, entrance: EntranceIn) -> Result<(), Fail> {
        let app = self
            .graph
            .apps
            .last_mut()
            .ok_or("`entrance` records must follow an `app` record")?;
        app.entrances.push(Entrance {
            key: entrance.key,
            name: entrance.name,
            on: entrance.on,
            about: entrance.about,
            code: Vec::new(),
        });
        Ok(())
    }

    /// Adds a target to the app or entrance just before it, whichever its `of` names. An
    /// app's own target comes before its entrances.
    fn target(&mut self, target: TargetIn) -> Result<(), Fail> {
        let of = target.of;
        let code = EntranceTarget {
            code: target.code,
            state: target.state,
        };
        let app = self
            .graph
            .apps
            .last_mut()
            .ok_or("`entrance_target` records must follow an `app` or `entrance` record")?;
        match app.entrances.last_mut() {
            Some(entrance) if entrance.key.as_str() == &*of => entrance.code.push(code),
            None if app.key.as_str() == &*of && app.code.is_none() => app.code = Some(code),
            None if app.key.as_str() == &*of => return Err(format!("app `{of}` has a second target")),
            _ => {
                return Err(format!(
                    "the target of `{of}` does not follow that app or entrance"
                ));
            }
        }
        Ok(())
    }
}

fn check_header(header: &HeaderIn) -> Result<(), Fail> {
    if &*header.format != FORMAT {
        return Err(format!("unknown export format `{}`", header.format));
    }
    match header.version {
        VERSION => Ok(()),
        other => Err(format!("unsupported export version {other}")),
    }
}
