//! The graph's records: files, blocks, slots, edges, inputs, entrances, and entrance targets;
//! each file also carries its typed records ([`crate::typed`]).
//!
//! Within a file, blocks are identified by a file-scoped `local_id` numbered in preorder;
//! across files and in everything persisted outside the store, by their structural [`Key`].
//! Records are compact: offsets are `u32`, names and texts are boxed strings, and keys share
//! their text through reference counts.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::key::{EntranceShape, Key, entrance_keys};
use crate::path::RepoPath;
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::FileTypes;
use crate::vocab::{
    BlockKind, DetectorId, EdgeKind, EntranceKind, FileState, HttpMethod, InputRole, Language, Reason,
    Resolution, Role, SlotName, TargetRole, TriggerStatus,
};

/// An indexed source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// The folder-relative path.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// Whether its facts come from the bytes on disk.
    pub state: FileState,
    /// The SHA-256 of the bytes the facts came from; absent for a failed file, which has none.
    pub hash: Option<ContentHash>,
    /// The syntax or read error, with its position, for a `last_good` or `failed` file.
    pub failure: Option<Box<str>>,
}

/// A fingerprinted configuration file that shaped the graph, such as `package.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputRecord {
    /// The folder-relative path.
    pub path: RepoPath,
    /// What the input is.
    pub role: InputRole,
    /// The SHA-256 of its bytes.
    pub hash: ContentHash,
}

/// A selected syntax entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// The block's position in its file's preorder; the root is 0.
    pub local_id: u32,
    /// The parent's local ID; `None` only for the root.
    pub parent: Option<u32>,
    /// The block's kind.
    pub kind: BlockKind,
    /// Its relationship to the parent; `None` for the root or an unmapped relationship.
    pub role: Option<Role>,
    /// The declared or inferred name; never empty, and never identity.
    pub name: Option<Box<str>>,
    /// The token-exact byte range; the root spans the whole file.
    pub range: ByteRange,
    /// The lines the range starts and ends on.
    pub lines: LineSpan,
    /// The structural key.
    pub key: Key,
}

/// The range of a written part of a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// The owning block's local ID.
    pub block: u32,
    /// Which part.
    pub slot: SlotName,
    /// The zero-based index of a repeated part (parameters, arguments, Python handlers).
    pub index: Option<u32>,
    /// The token-exact byte range, inside the block's range.
    pub range: ByteRange,
}

/// A relationship sourced at one occurrence (a `call`, `new`, or `import` block).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// The occurrence block's local ID.
    pub source: u32,
    /// The edge's position among its source's edges: the occurrence is 0, then references in
    /// argument order.
    pub seq: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// The target's key, exactly when resolved.
    pub target: Option<Key>,
    /// The short written callee, argument, or specifier (at most 120 UTF-16 units).
    pub text: Box<str>,
    /// How the edge resolved.
    pub resolution: Resolution,
    /// Why it is unresolved, exactly when unresolved.
    pub reason: Option<Reason>,
    /// The module label, exactly when external.
    pub module: Option<ModuleLabel>,
    /// The argument position of a reference, or the owner's parameter position of an
    /// unresolved `parameter` call.
    pub position: Option<u32>,
}

/// One file's blocks, slots, edges, and typed records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileGraph {
    /// The file.
    pub file: FileRecord,
    /// Blocks in preorder; `blocks[i].local_id == i`.
    pub blocks: Vec<Block>,
    /// Slots ordered by block, then start.
    pub slots: Vec<Slot>,
    /// Edges ordered by source, then sequence.
    pub edges: Vec<Edge>,
    /// Signatures, parameters, members, arguments, call results, and type links.
    pub types: FileTypes,
}

/// Where an entrance is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The source file or configuration input.
    pub path: RepoPath,
    /// The byte range of the declaration in that file.
    pub range: ByteRange,
    /// For source evidence, the block with exactly that range.
    pub block: Option<Key>,
    /// For configuration evidence, the RFC 6901 pointer into the parsed JSON or TOML value.
    pub pointer: Option<Box<str>>,
}

/// One independently resolved handler or startup target of an entrance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntranceTarget {
    /// The zero-based declaration order within the entrance.
    pub position: u32,
    /// `startup` for a process, `handler` otherwise.
    pub role: TargetRole,
    /// The target's key, exactly when resolved.
    pub target: Option<Key>,
    /// The handler label, locator, or launch path as written (at most 120 UTF-16 units).
    pub text: Box<str>,
    /// How the target resolved.
    pub resolution: Resolution,
    /// Why it is unresolved, exactly when unresolved.
    pub reason: Option<Reason>,
    /// The module label, exactly when external.
    pub module: Option<ModuleLabel>,
}

/// A declared way that a process or unit of work starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrance {
    /// The structural key, derived from the fields below and the entrance's order.
    pub key: Key,
    /// The entrance's kind.
    pub kind: EntranceKind,
    /// A display label; manual declarations only.
    pub label: Option<Box<str>>,
    /// The HTTP method; `http` entrances only.
    pub method: Option<HttpMethod>,
    /// The activating value; absent exactly when its status is `unknown`.
    pub trigger: Option<Box<str>>,
    /// How much of the trigger is known.
    pub trigger_status: TriggerStatus,
    /// The detector that produced it.
    pub detector: DetectorId,
    /// Where it is declared.
    pub evidence: Evidence,
    /// Its targets in declaration order (not execution order).
    pub targets: Vec<EntranceTarget>,
}

impl Entrance {
    /// What the entrance contributes to its key.
    pub fn key_shape(&self) -> EntranceShape<'_> {
        EntranceShape {
            detector: self.detector,
            kind: self.kind,
            method: self.method,
            trigger: self.trigger.as_deref(),
            first_target: self.targets.first().map(|target| &*target.text),
        }
    }

    /// The canonical entrance order: evidence path, start, end, then detector, kind, method,
    /// and trigger compared as their vocabulary strings (absent values as empty strings).
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.evidence
            .path
            .cmp(&other.evidence.path)
            .then(self.evidence.range.start.cmp(&other.evidence.range.start))
            .then(self.evidence.range.end.cmp(&other.evidence.range.end))
            .then_with(|| self.detector.as_str().cmp(other.detector.as_str()))
            .then_with(|| self.kind.as_str().cmp(other.kind.as_str()))
            .then_with(|| {
                self.method
                    .map_or("", HttpMethod::as_str)
                    .cmp(other.method.map_or("", HttpMethod::as_str))
            })
            .then_with(|| {
                self.trigger
                    .as_deref()
                    .unwrap_or("")
                    .cmp(other.trigger.as_deref().unwrap_or(""))
            })
    }
}

/// The whole graph: files in path order, the inputs that shaped it, and entrances in canonical
/// order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Graph {
    /// Files sorted by path.
    pub files: Vec<FileGraph>,
    /// Inputs sorted by path.
    pub inputs: Vec<InputRecord>,
    /// Entrances in canonical order, each with its key.
    pub entrances: Vec<Entrance>,
}

impl Graph {
    /// Puts files, inputs, and entrances in canonical order and derives every entrance key.
    ///
    /// Entrances that tie on every ordering field keep their relative input order (detectors
    /// run in a fixed order), so the result is deterministic for deterministic producers.
    /// Entrance keys are recomputed, so any keys already present are replaced.
    pub fn normalize(&mut self) {
        self.files.sort_by(|a, b| a.file.path.cmp(&b.file.path));
        self.inputs.sort_by(|a, b| a.path.cmp(&b.path));
        self.entrances.sort_by(Entrance::canonical_cmp);
        let keys = entrance_keys(self.entrances.iter().map(Entrance::key_shape));
        for (entrance, key) in self.entrances.iter_mut().zip(keys) {
            entrance.key = key;
        }
    }

    /// The file graph at `path`, if indexed.
    pub fn file(&self, path: &str) -> Option<&FileGraph> {
        self.files
            .binary_search_by(|file| file.file.path.as_str().cmp(path))
            .ok()
            .map(|index| &self.files[index])
    }
}

/// An external origin: one plain label (`express`, `node:fs`, `global`, `python:builtins`, a
/// repository-relative path), or, when several origins are possible, a compact JSON array of
/// two or more distinct nonempty labels sorted by their UTF-16 code units, stored as its text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct ModuleLabel(Box<str>);

impl ModuleLabel {
    /// One plain label. It must be nonempty and must not start with `[`.
    pub fn single(label: &str) -> Option<Self> {
        (!label.is_empty() && !label.starts_with('[')).then(|| Self(label.into()))
    }

    /// The label for a set of possible origins: the plain label when there is one, otherwise a
    /// sorted, deduplicated JSON array. Returns `None` for an empty set or an empty label.
    pub fn from_set<'a>(labels: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut labels: Vec<&str> = labels.into_iter().collect();
        if labels.iter().any(|label| label.is_empty()) {
            return None;
        }
        labels.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        labels.dedup();
        match labels.as_slice() {
            [] => None,
            [one] => Self::single(one),
            many => serde_json::to_string(many).ok().map(|text| Self(text.into())),
        }
    }

    /// Wraps stored label text without checking it; see [`ModuleLabel::is_valid`].
    pub fn from_text(text: &str) -> Self {
        Self(text.into())
    }

    /// The stored text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the text is a nonempty plain label, or a compact JSON array of at least two
    /// distinct nonempty labels in UTF-16 order.
    pub fn is_valid(&self) -> bool {
        if self.0.is_empty() {
            return false;
        }
        if !self.0.starts_with('[') {
            return true;
        }
        let Ok(labels) = serde_json::from_str::<Vec<String>>(&self.0) else {
            return false;
        };
        labels.len() >= 2
            && labels.iter().all(|label| !label.is_empty())
            && labels
                .windows(2)
                .all(|pair| pair[0].encode_utf16().cmp(pair[1].encode_utf16()) == Ordering::Less)
            && serde_json::to_string(&labels).is_ok_and(|compact| compact == *self.0)
    }

    /// The individual labels, or `None` if the text is not valid.
    pub fn labels(&self) -> Option<Vec<String>> {
        if !self.is_valid() {
            return None;
        }
        if self.0.starts_with('[') {
            serde_json::from_str(&self.0).ok()
        } else {
            Some(vec![self.0.to_string()])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphs_round_trip_through_postcard_and_json() {
        let graph = crate::testkit::sample::sample().build();
        let packed = postcard::to_allocvec(&graph).unwrap();
        assert_eq!(postcard::from_bytes::<Graph>(&packed).unwrap(), graph);
        let json = serde_json::to_string(&graph).unwrap();
        assert_eq!(serde_json::from_str::<Graph>(&json).unwrap(), graph);
        assert!(graph.file("src/orders.ts").is_some());
        assert!(graph.file("src/absent.ts").is_none());
    }

    #[test]
    fn module_labels() {
        assert_eq!(ModuleLabel::single("express").unwrap().as_str(), "express");
        assert!(ModuleLabel::single("").is_none());
        let set = ModuleLabel::from_set(["client-b", "client-a", "client-b"]).unwrap();
        assert_eq!(set.as_str(), r#"["client-a","client-b"]"#);
        assert!(set.is_valid());
        assert_eq!(set.labels().unwrap(), ["client-a", "client-b"]);
        assert_eq!(ModuleLabel::from_set(["node:fs"]).unwrap().as_str(), "node:fs");
        assert!(ModuleLabel::from_set([]).is_none());
        for bad in [
            "",
            r#"["a"]"#,
            r#"["b","a"]"#,
            r#"["a","a"]"#,
            r#"[ "a","b"]"#,
            r#"["a",""]"#,
            "[oops",
        ] {
            assert!(!ModuleLabel::from_text(bad).is_valid(), "{bad}");
        }
    }

    #[test]
    fn module_sets_sort_by_utf16_code_units() {
        // U+1F600 sorts before U+FF61 in UTF-16 (its high surrogate 0xD83D < 0xFF61), but after
        // it by scalar value.
        let set = ModuleLabel::from_set(["｡", "😀"]).unwrap();
        assert_eq!(set.as_str(), "[\"😀\",\"｡\"]");
        assert!(set.is_valid());
    }
}
