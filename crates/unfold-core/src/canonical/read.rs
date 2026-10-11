//! The canonical export reader.

use std::borrow::Cow;
use std::str::FromStr;

use rustc_hash::FxHashMap;
use serde::Deserialize;

use super::{FORMAT, OLDEST_VERSION, VERSION};
use crate::key::Key;
use crate::model::{
    Block, Edge, Entrance, EntranceTarget, Evidence, FileGraph, FileRecord, Graph, InputRecord, ModuleLabel,
    Slot,
};
use crate::path::RepoPath;
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::{
    Argument, CallResult, FileTypes, Member, Pairing, Parameter, Signature, TypeLink, WrittenType,
};
use crate::vocab::UnknownValue;

/// A canonical export that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct CanonicalError {
    /// The one-based line of the offending record.
    pub line: usize,
    /// What is wrong with it.
    pub message: String,
}

/// Every field any record type can have. Unknown fields are rejected; fields that belong to a
/// different record type are rejected per type below.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Raw<'a> {
    #[serde(borrow)]
    t: Option<Cow<'a, str>>,
    #[serde(borrow)]
    format: Option<Cow<'a, str>>,
    version: Option<u32>,
    #[serde(borrow)]
    path: Option<Cow<'a, str>>,
    #[serde(borrow)]
    language: Option<Cow<'a, str>>,
    #[serde(borrow)]
    state: Option<Cow<'a, str>>,
    #[serde(borrow)]
    hash: Option<Cow<'a, str>>,
    #[serde(borrow)]
    failure: Option<Cow<'a, str>>,
    #[serde(borrow)]
    key: Option<Cow<'a, str>>,
    #[serde(borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    role: Option<Cow<'a, str>>,
    #[serde(borrow)]
    name: Option<Cow<'a, str>>,
    range: Option<ByteRange>,
    lines: Option<LineSpan>,
    #[serde(borrow)]
    block: Option<Cow<'a, str>>,
    #[serde(borrow)]
    slot: Option<Cow<'a, str>>,
    index: Option<u32>,
    #[serde(borrow)]
    source: Option<Cow<'a, str>>,
    seq: Option<u32>,
    #[serde(borrow)]
    target: Option<Cow<'a, str>>,
    #[serde(borrow)]
    text: Option<Cow<'a, str>>,
    #[serde(borrow)]
    resolution: Option<Cow<'a, str>>,
    #[serde(borrow)]
    reason: Option<Cow<'a, str>>,
    #[serde(borrow)]
    module: Option<Cow<'a, str>>,
    position: Option<u32>,
    #[serde(borrow)]
    label: Option<Cow<'a, str>>,
    #[serde(borrow)]
    method: Option<Cow<'a, str>>,
    #[serde(borrow)]
    trigger: Option<Cow<'a, str>>,
    #[serde(borrow)]
    trigger_status: Option<Cow<'a, str>>,
    #[serde(borrow)]
    detector: Option<Cow<'a, str>>,
    #[serde(borrow)]
    evidence_path: Option<Cow<'a, str>>,
    evidence_range: Option<ByteRange>,
    #[serde(borrow)]
    evidence_block: Option<Cow<'a, str>>,
    #[serde(borrow)]
    evidence_pointer: Option<Cow<'a, str>>,
    #[serde(borrow)]
    entrance: Option<Cow<'a, str>>,
    #[serde(borrow)]
    returns: Option<Cow<'a, str>>,
    returns_range: Option<ByteRange>,
    optional: Option<bool>,
    default: Option<bool>,
    readonly: Option<bool>,
    #[serde(borrow, rename = "type")]
    ty: Option<Cow<'a, str>>,
    type_range: Option<ByteRange>,
    #[serde(borrow)]
    value: Option<Cow<'a, str>>,
    #[serde(borrow)]
    call: Option<Cow<'a, str>>,
    #[serde(borrow)]
    expand: Option<Cow<'a, str>>,
    #[serde(borrow)]
    keyword: Option<Cow<'a, str>>,
    parameter: Option<u32>,
    #[serde(borrow)]
    unpaired: Option<Cow<'a, str>>,
    #[serde(borrow)]
    binding: Option<Cow<'a, str>>,
    awaited: Option<bool>,
    bound: Option<u32>,
}

impl Raw<'_> {
    /// The names of the fields present, other than `t`.
    fn present(&self) -> impl Iterator<Item = &'static str> {
        [
            ("format", self.format.is_some()),
            ("version", self.version.is_some()),
            ("path", self.path.is_some()),
            ("language", self.language.is_some()),
            ("state", self.state.is_some()),
            ("hash", self.hash.is_some()),
            ("failure", self.failure.is_some()),
            ("key", self.key.is_some()),
            ("kind", self.kind.is_some()),
            ("role", self.role.is_some()),
            ("name", self.name.is_some()),
            ("range", self.range.is_some()),
            ("lines", self.lines.is_some()),
            ("block", self.block.is_some()),
            ("slot", self.slot.is_some()),
            ("index", self.index.is_some()),
            ("source", self.source.is_some()),
            ("seq", self.seq.is_some()),
            ("target", self.target.is_some()),
            ("text", self.text.is_some()),
            ("resolution", self.resolution.is_some()),
            ("reason", self.reason.is_some()),
            ("module", self.module.is_some()),
            ("position", self.position.is_some()),
            ("label", self.label.is_some()),
            ("method", self.method.is_some()),
            ("trigger", self.trigger.is_some()),
            ("trigger_status", self.trigger_status.is_some()),
            ("detector", self.detector.is_some()),
            ("evidence_path", self.evidence_path.is_some()),
            ("evidence_range", self.evidence_range.is_some()),
            ("evidence_block", self.evidence_block.is_some()),
            ("evidence_pointer", self.evidence_pointer.is_some()),
            ("entrance", self.entrance.is_some()),
            ("returns", self.returns.is_some()),
            ("returns_range", self.returns_range.is_some()),
            ("optional", self.optional.is_some()),
            ("default", self.default.is_some()),
            ("readonly", self.readonly.is_some()),
            ("type", self.ty.is_some()),
            ("type_range", self.type_range.is_some()),
            ("value", self.value.is_some()),
            ("call", self.call.is_some()),
            ("expand", self.expand.is_some()),
            ("keyword", self.keyword.is_some()),
            ("parameter", self.parameter.is_some()),
            ("unpaired", self.unpaired.is_some()),
            ("binding", self.binding.is_some()),
            ("awaited", self.awaited.is_some()),
            ("bound", self.bound.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, present)| present.then_some(name))
    }
}

type Fail = String;

fn required<T>(value: Option<T>, field: &str) -> Result<T, Fail> {
    value.ok_or_else(|| format!("missing field `{field}`"))
}

fn vocab<T: FromStr<Err = UnknownValue>>(value: Option<Cow<'_, str>>, field: &str) -> Result<T, Fail> {
    required(value, field)?
        .parse()
        .map_err(|error: UnknownValue| error.to_string())
}

fn optional_vocab<T: FromStr<Err = UnknownValue>>(value: Option<Cow<'_, str>>) -> Result<Option<T>, Fail> {
    value
        .map(|text| text.parse().map_err(|error: UnknownValue| error.to_string()))
        .transpose()
}

fn path(value: Option<Cow<'_, str>>, field: &str) -> Result<RepoPath, Fail> {
    RepoPath::new(&required(value, field)?).map_err(|error| error.to_string())
}

fn hash(value: Option<Cow<'_, str>>, field: &str) -> Result<ContentHash, Fail> {
    ContentHash::from_hex(&required(value, field)?).map_err(|error| error.to_string())
}

fn boxed(value: Option<Cow<'_, str>>) -> Option<Box<str>> {
    value.map(|text| text.into())
}

fn key(value: Option<Cow<'_, str>>) -> Option<Key> {
    value.map(|text| Key::from_text(&text))
}

fn module(value: Option<Cow<'_, str>>) -> Option<ModuleLabel> {
    value.map(|text| ModuleLabel::from_text(&text))
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
    text: Option<Cow<'_, str>>,
    range: Option<ByteRange>,
    field: &str,
) -> Result<Option<WrittenType>, Fail> {
    match (text, range) {
        (Some(text), Some(range)) => Ok(Some(WrittenType {
            text: text.into(),
            range,
        })),
        (None, None) => Ok(None),
        _ => Err(format!("`{field}` and `{field}_range` appear together")),
    }
}

/// Where the reader is in the export's fixed section order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Files,
    Inputs,
    Entrances,
}

struct Reader {
    graph: Graph,
    seen_header: bool,
    /// The header's version.
    version: u32,
    section: Section,
    /// The current file's block keys, by local ID.
    local_ids: FxHashMap<Key, u32>,
    /// The current file's root key (`path#`), which every one of its block keys starts with.
    root: String,
}

/// Reads a canonical export. It accepts exactly the shapes [`write`](super::write) produces
/// (and version 1 exports, which have no typed records) and rebuilds local IDs and parents from
/// the keys; graph invariants are left to the validator.
pub fn read(text: &str) -> Result<Graph, CanonicalError> {
    read_versioned(text).map(|(graph, _)| graph)
}

/// [`read`], also returning the header's `version` (1 or 2), so a version-1 export can be
/// validated as one ([`crate::validate::Validator::without_typed_records`]) and written again
/// at its version ([`write_version`](super::write_version)).
pub fn read_versioned(text: &str) -> Result<(Graph, u32), CanonicalError> {
    let mut reader = Reader {
        graph: Graph::default(),
        seen_header: false,
        version: VERSION,
        section: Section::Files,
        local_ids: FxHashMap::default(),
        root: String::new(),
    };
    if text.is_empty() {
        return Err(CanonicalError {
            line: 1,
            message: "missing header record".into(),
        });
    }
    let Some(body) = text.strip_suffix('\n') else {
        let line = text.split('\n').count();
        return Err(CanonicalError {
            line,
            message: "the export must end with a newline".into(),
        });
    };
    for (index, line) in body.split('\n').enumerate() {
        reader.line(line).map_err(|message| CanonicalError {
            line: index + 1,
            message,
        })?;
    }
    Ok((reader.graph, reader.version))
}

impl Reader {
    fn line(&mut self, line: &str) -> Result<(), Fail> {
        let raw: Raw<'_> = serde_json::from_str(line).map_err(|error| format!("invalid record: {error}"))?;
        let record = raw.t.clone().ok_or("missing field `t`")?;
        if !self.seen_header || record == "header" {
            if self.seen_header {
                return Err("repeated header".into());
            }
            if record != "header" {
                return Err("the first record must be the header".into());
            }
            if let Some(field) = raw.present().find(|field| !["format", "version"].contains(field)) {
                return Err(format!("field `{field}` is not allowed on a `header` record"));
            }
            self.seen_header = true;
            self.version = header(raw)?;
            return Ok(());
        }
        let (section, allowed): (Section, &[&str]) = match &*record {
            "file" => (Section::Files, &["path", "language", "state", "hash", "failure"]),
            "block" => (Section::Files, &["key", "kind", "role", "name", "range", "lines"]),
            "slot" => (Section::Files, &["block", "slot", "index", "range"]),
            "edge" => (
                Section::Files,
                &[
                    "source",
                    "seq",
                    "kind",
                    "target",
                    "text",
                    "resolution",
                    "reason",
                    "module",
                    "position",
                ],
            ),
            "signature" | "parameter" | "member" | "argument" | "result" | "type_link"
                if self.version < 2 =>
            {
                return Err(format!(
                    "a `{record}` record needs export version 2, not {}",
                    self.version
                ));
            }
            "signature" => (Section::Files, &["block", "returns", "returns_range"]),
            "parameter" => (
                Section::Files,
                &[
                    "block",
                    "index",
                    "kind",
                    "name",
                    "optional",
                    "default",
                    "type",
                    "type_range",
                ],
            ),
            "member" => (
                Section::Files,
                &[
                    "block",
                    "index",
                    "kind",
                    "name",
                    "optional",
                    "readonly",
                    "default",
                    "type",
                    "type_range",
                    "value",
                ],
            ),
            "argument" => (
                Section::Files,
                &[
                    "call",
                    "index",
                    "expand",
                    "keyword",
                    "parameter",
                    "unpaired",
                    "bound",
                ],
            ),
            "result" => (
                Section::Files,
                &["call", "binding", "awaited", "target", "position", "bound"],
            ),
            "type_link" => (
                Section::Files,
                &[
                    "block",
                    "range",
                    "text",
                    "resolution",
                    "target",
                    "reason",
                    "module",
                ],
            ),
            "input" => (Section::Inputs, &["path", "role", "hash"]),
            "entrance" => (
                Section::Entrances,
                &[
                    "key",
                    "kind",
                    "label",
                    "method",
                    "trigger",
                    "trigger_status",
                    "detector",
                    "evidence_path",
                    "evidence_range",
                    "evidence_block",
                    "evidence_pointer",
                ],
            ),
            "target" => (
                Section::Entrances,
                &[
                    "entrance",
                    "position",
                    "role",
                    "target",
                    "text",
                    "resolution",
                    "reason",
                    "module",
                ],
            ),
            other => return Err(format!("unknown record type `{other}`")),
        };
        if let Some(field) = raw.present().find(|field| !allowed.contains(field)) {
            return Err(format!("field `{field}` is not allowed on a `{record}` record"));
        }
        if section < self.section {
            return Err(format!("a `{record}` record cannot follow later sections"));
        }
        self.section = section;
        match &*record {
            "file" => self.file(raw),
            "block" => self.block(raw),
            "slot" => self.slot(raw),
            "edge" => self.edge(raw),
            "signature" => self.signature(raw),
            "parameter" => self.parameter(raw),
            "member" => self.member(raw),
            "argument" => self.argument(raw),
            "result" => self.result(raw),
            "type_link" => self.type_link(raw),
            "input" => self.input(raw),
            "entrance" => self.entrance(raw),
            _ => self.target(raw),
        }
    }

    fn file(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let file = FileRecord {
            path: path(raw.path, "path")?,
            language: vocab(raw.language, "language")?,
            state: vocab(raw.state, "state")?,
            hash: raw
                .hash
                .map(|text| ContentHash::from_hex(&text).map_err(|error| error.to_string()))
                .transpose()?,
            failure: boxed(raw.failure),
        };
        self.local_ids.clear();
        self.root.clear();
        self.root.push_str(Key::file_root(&file.path).as_str());
        self.graph.files.push(FileGraph {
            file,
            blocks: Vec::new(),
            slots: Vec::new(),
            edges: Vec::new(),
            types: FileTypes::default(),
        });
        Ok(())
    }

    fn current(&mut self, record: &str) -> Result<&mut FileGraph, Fail> {
        self.graph
            .files
            .last_mut()
            .ok_or_else(|| format!("a `{record}` record must follow a `file` record"))
    }

    fn local_id(&self, key: &str) -> Result<u32, Fail> {
        self.local_ids
            .get(key)
            .copied()
            .ok_or_else(|| format!("`{key}` is not a block of the current file"))
    }

    fn block(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let key = Key::from_text(&required(raw.key, "key")?);
        let file = self.current("block")?;
        let local_id = u32::try_from(file.blocks.len()).map_err(|_| "too many blocks".to_string())?;
        let parent_key = parent_key(&self.root, key.as_str()).ok_or_else(|| {
            let path = self.root.strip_suffix('#').unwrap_or(&self.root);
            format!("block key `{key}` does not belong to file `{path}`")
        })?;
        let parent = match parent_key {
            None if local_id == 0 => None,
            None => return Err(format!("`{key}` repeats the file root")),
            Some(parent) => Some(self.local_id(parent)?),
        };
        let block = Block {
            local_id,
            parent,
            kind: vocab(raw.kind, "kind")?,
            role: optional_vocab(raw.role)?,
            name: boxed(raw.name),
            range: required(raw.range, "range")?,
            lines: required(raw.lines, "lines")?,
            key: key.clone(),
        };
        if self.local_ids.insert(key, local_id).is_some() {
            return Err(format!("duplicate block key `{}`", block.key));
        }
        self.current("block")?.blocks.push(block);
        Ok(())
    }

    fn slot(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let block = self.local_id(&required(raw.block, "block")?)?;
        let slot = Slot {
            block,
            slot: vocab(raw.slot, "slot")?,
            index: raw.index,
            range: required(raw.range, "range")?,
        };
        self.current("slot")?.slots.push(slot);
        Ok(())
    }

    fn edge(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let source = self.local_id(&required(raw.source, "source")?)?;
        let edge = Edge {
            source,
            seq: required(raw.seq, "seq")?,
            kind: vocab(raw.kind, "kind")?,
            target: key(raw.target),
            text: required(raw.text, "text")?.into(),
            resolution: vocab(raw.resolution, "resolution")?,
            reason: optional_vocab(raw.reason)?,
            module: module(raw.module),
            position: raw.position,
        };
        self.current("edge")?.edges.push(edge);
        Ok(())
    }

    fn types(&mut self, record: &str) -> Result<&mut FileTypes, Fail> {
        Ok(&mut self.current(record)?.types)
    }

    fn signature(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let signature = Signature {
            block: self.local_id(&required(raw.block, "block")?)?,
            returns: written(raw.returns, raw.returns_range, "returns")?,
        };
        self.types("signature")?.signatures.push(signature);
        Ok(())
    }

    fn parameter(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let parameter = Parameter {
            block: self.local_id(&required(raw.block, "block")?)?,
            index: required(raw.index, "index")?,
            kind: vocab(raw.kind, "kind")?,
            name: boxed(raw.name),
            optional: flag(raw.optional, "optional")?,
            default: flag(raw.default, "default")?,
            ty: written(raw.ty, raw.type_range, "type")?,
        };
        self.types("parameter")?.parameters.push(parameter);
        Ok(())
    }

    fn member(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let member = Member {
            block: self.local_id(&required(raw.block, "block")?)?,
            index: required(raw.index, "index")?,
            kind: vocab(raw.kind, "kind")?,
            name: boxed(raw.name),
            optional: flag(raw.optional, "optional")?,
            readonly: flag(raw.readonly, "readonly")?,
            default: flag(raw.default, "default")?,
            ty: written(raw.ty, raw.type_range, "type")?,
            value: boxed(raw.value),
        };
        self.types("member")?.members.push(member);
        Ok(())
    }

    fn argument(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let pairing = match (raw.parameter, optional_vocab(raw.unpaired)?) {
            (Some(index), None) => Some(Pairing::Parameter(index)),
            (None, Some(reason)) => Some(Pairing::Unpaired(reason)),
            (None, None) => None,
            (Some(_), Some(_)) => return Err("`parameter` and `unpaired` exclude each other".into()),
        };
        let argument = Argument {
            call: self.local_id(&required(raw.call, "call")?)?,
            index: required(raw.index, "index")?,
            expand: optional_vocab(raw.expand)?,
            keyword: boxed(raw.keyword),
            pairing,
            bound: raw.bound,
        };
        self.types("argument")?.arguments.push(argument);
        Ok(())
    }

    fn result(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let target = match raw.target {
            Some(target) => Some(self.local_id(&target)?),
            None => None,
        };
        let result = CallResult {
            call: self.local_id(&required(raw.call, "call")?)?,
            binding: vocab(raw.binding, "binding")?,
            awaited: flag(raw.awaited, "awaited")?,
            target,
            position: raw.position,
            bound: raw.bound,
        };
        self.types("result")?.results.push(result);
        Ok(())
    }

    fn type_link(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let link = TypeLink {
            block: self.local_id(&required(raw.block, "block")?)?,
            range: required(raw.range, "range")?,
            text: required(raw.text, "text")?.into(),
            resolution: vocab(raw.resolution, "resolution")?,
            target: key(raw.target),
            reason: optional_vocab(raw.reason)?,
            module: module(raw.module),
        };
        self.types("type_link")?.type_links.push(link);
        Ok(())
    }

    fn input(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let input = InputRecord {
            path: path(raw.path, "path")?,
            role: vocab(raw.role, "role")?,
            hash: hash(raw.hash, "hash")?,
        };
        self.graph.inputs.push(input);
        Ok(())
    }

    fn entrance(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let entrance = Entrance {
            key: Key::from_text(&required(raw.key, "key")?),
            kind: vocab(raw.kind, "kind")?,
            label: boxed(raw.label),
            method: optional_vocab(raw.method)?,
            trigger: boxed(raw.trigger),
            trigger_status: vocab(raw.trigger_status, "trigger_status")?,
            detector: vocab(raw.detector, "detector")?,
            evidence: Evidence {
                path: path(raw.evidence_path, "evidence_path")?,
                range: required(raw.evidence_range, "evidence_range")?,
                block: key(raw.evidence_block),
                pointer: boxed(raw.evidence_pointer),
            },
            targets: Vec::new(),
        };
        self.graph.entrances.push(entrance);
        Ok(())
    }

    fn target(&mut self, raw: Raw<'_>) -> Result<(), Fail> {
        let owner = required(raw.entrance, "entrance")?;
        let target = EntranceTarget {
            position: required(raw.position, "position")?,
            role: vocab(raw.role, "role")?,
            target: key(raw.target),
            text: required(raw.text, "text")?.into(),
            resolution: vocab(raw.resolution, "resolution")?,
            reason: optional_vocab(raw.reason)?,
            module: module(raw.module),
        };
        let entrance = self
            .graph
            .entrances
            .last_mut()
            .filter(|entrance| entrance.key.as_str() == owner)
            .ok_or_else(|| format!("target of `{owner}` does not follow that entrance"))?;
        entrance.targets.push(target);
        Ok(())
    }
}

/// Checks the header and returns its version.
fn header(raw: Raw<'_>) -> Result<u32, Fail> {
    let format = required(raw.format, "format")?;
    if format != FORMAT {
        return Err(format!("unknown export format `{format}`"));
    }
    match required(raw.version, "version")? {
        version @ OLDEST_VERSION..=VERSION => Ok(version),
        other => Err(format!("unsupported export version {other}")),
    }
}

/// The parent key of block `key` in the file whose root key is `root`: `Some(None)` for the
/// root itself, `None` if the key is not in that file. Steps never contain `/` (names escape
/// it), so the parent is everything before the last `/` after the root, or the root.
fn parent_key<'k>(root: &str, key: &'k str) -> Option<Option<&'k str>> {
    let steps = key.strip_prefix(root)?;
    if steps.is_empty() {
        return Some(None);
    }
    Some(Some(match steps.rfind('/') {
        Some(slash) => &key[..root.len() + slash],
        None => &key[..root.len()],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_keys_come_from_the_last_step() {
        let root = "src/a#b.ts#";
        assert_eq!(parent_key(root, "src/a#b.ts#"), Some(None));
        assert_eq!(
            parent_key(root, "src/a#b.ts#function:f"),
            Some(Some("src/a#b.ts#"))
        );
        assert_eq!(
            parent_key(root, "src/a#b.ts#function:f/if[0]"),
            Some(Some("src/a#b.ts#function:f"))
        );
        assert_eq!(parent_key(root, "other.ts#function:f"), None);
    }
}
