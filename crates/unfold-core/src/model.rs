//! The graph's records (`docs/designs/diagram-blocks.md`, "Records"): files and their nodes,
//! links, and typed records ([`crate::typed`]); the inputs that shaped them; and the apps and
//! entrances `unfold.toml` declares.
//!
//! Within a file, a node is identified by its local ID, its position in the file's preorder
//! (the module is 0), and links, typed records, and the nodes themselves refer to nodes of their
//! own file that way. Across files and in everything persisted outside the store, a node is
//! identified by its structural [`Key`]. Records are compact: offsets are `u32`, names and texts
//! are boxed strings, and keys share their text through reference counts.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::key::Key;
use crate::path::RepoPath;
use crate::text::{ByteRange, ContentHash, LineSpan};
use crate::typed::FileTypes;
use crate::vocab::{
    CallRole, FileState, Flags, Form, InputRole, Language, LinkKind, NodeKind, Outcome, Reason, TargetState,
};

/// An indexed source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
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
pub struct Input {
    /// The folder-relative path.
    pub path: RepoPath,
    /// What the input is.
    pub role: InputRole,
    /// The SHA-256 of its bytes.
    pub hash: ContentHash,
}

/// One block: a definition, control block, part, call, or exit.
///
/// Ranges are token-exact UTF-8 byte ranges. `head`, `body`, and `value` lie inside `range`;
/// `stmt` contains it; `doc` lies inside it (a docstring) or just before it (a leading JSDoc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// The structural key.
    pub key: Key,
    /// The parent's local ID; `None` only for the module.
    pub parent: Option<u32>,
    /// The written form, which decides the kind ([`Node::kind`]).
    pub form: Form,
    /// A named definition's name, or a call's normalized callee
    /// ([`crate::key::Callee::text`]); never empty.
    pub name: Option<Box<str>>,
    /// The whole block; the module spans the whole file.
    pub range: ByteRange,
    /// The lines `range` starts and ends on.
    pub lines: LineSpan,
    /// A definition's signature, a control block's header, a part's test or clause, or a call's
    /// callee.
    pub head: Option<ByteRange>,
    /// The suite of a definition or a part.
    pub body: Option<ByteRange>,
    /// A definition's docstring or JSDoc: its description from code, never a step.
    pub doc: Option<ByteRange>,
    /// For the first block of a statement, and for every exit: the statement's range.
    pub stmt: Option<ByteRange>,
    /// For an exit with a value: the value's range.
    pub value: Option<ByteRange>,
    /// For a call, ternary, or comprehension whose value is exactly what a statement assigns:
    /// the written target (`receipt`, `order.receipt`, `email_row_id, inserted`).
    pub binds: Option<Box<str>>,
    /// Why a call sits where it does: in a test, or run when a definition is created.
    pub role: Option<CallRole>,
    /// For a decorator, base, or Python default: the local ID of the definition it belongs to.
    pub r#for: Option<u32>,
    /// Yes-or-no facts ([`crate::vocab::Flag`]).
    pub flags: Flags,
    /// For `break` and `continue`: the local ID of their loop.
    pub target: Option<u32>,
}

impl Node {
    /// The node's kind, which its form decides.
    pub const fn kind(&self) -> NodeKind {
        self.form.kind()
    }
}

/// The local ID of the definition `node` belongs to: the nearest definition (module, class,
/// function, or type) above it. The module belongs to nothing.
///
/// This is the only ownership rule. A decorator, base, or Python default sits in the enclosing
/// body, so it belongs to the enclosing definition; a call inside a callback belongs to the
/// callback.
///
/// `nodes` is one file's nodes in preorder, indexed by local ID.
pub fn owner(nodes: &[Node], node: u32) -> Option<u32> {
    let mut parent = nodes.get(node as usize)?.parent;
    while let Some(id) = parent {
        let ancestor = nodes.get(id as usize)?;
        if ancestor.kind().is_definition() {
            return Some(id);
        }
        parent = ancestor.parent;
    }
    None
}

/// What a link reaches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    /// The key of one proven definition (or, for `imports`, module) in this project.
    Project(Key),
    /// A proven outside module.
    Library(ModuleLabel),
    /// The language's builtins: `python:builtins`, `ecmascript`.
    Builtin(ModuleLabel),
    /// Nothing proven, for this reason.
    Unknown(Reason),
}

impl LinkTarget {
    /// How the link resolved.
    pub const fn outcome(&self) -> Outcome {
        match self {
            Self::Project(_) => Outcome::Project,
            Self::Library(_) => Outcome::Library,
            Self::Builtin(_) => Outcome::Builtin,
            Self::Unknown(_) => Outcome::Unknown,
        }
    }
}

/// One relationship from a block to what it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The local ID of the call, definition, or part it starts at.
    pub from: u32,
    /// Its position among the links of `from`, contiguous from 0.
    pub seq: u32,
    /// The relationship.
    pub kind: LinkKind,
    /// Where the target is written: the callee, the passed name, the base, the specifier, or
    /// the type name.
    pub range: ByteRange,
    /// How it is written (at most 120 UTF-16 units).
    pub text: Box<str>,
    /// What it reaches.
    pub to: LinkTarget,
    /// For `extends`: the target is an interface a TypeScript class implements.
    pub implements: bool,
    /// For a `calls` link to a project function: how many of its leading parameters the call
    /// binds itself (a Python method called through an instance binds `self`), when that is
    /// proven.
    pub bound: Option<u32>,
}

/// One file's nodes, links, and typed records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileGraph {
    /// The file.
    pub file: File,
    /// Nodes in preorder, indexed by local ID; the module is 0. Empty for a failed file.
    pub nodes: Vec<Node>,
    /// Links ordered by `from`, then `seq`.
    pub links: Vec<Link>,
    /// Signatures, parameters, members, and arguments.
    pub types: FileTypes,
}

impl FileGraph {
    /// The local ID of the node with `key`, if this file has one.
    pub fn local_id(&self, key: &str) -> Option<u32> {
        let index = self.nodes.iter().position(|node| node.key.as_str() == key)?;
        u32::try_from(index).ok()
    }
}

/// One `code` key of an app or entrance, and what it names after the last compile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntranceTarget {
    /// The block key as written.
    pub code: Key,
    /// What it names.
    pub state: TargetState,
}

/// Something an app handles: one entry of its `entrances`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrance {
    /// `entrance:<app slug>/<entrance slug>` ([`Key::entrance`]).
    pub key: Key,
    /// What a person calls it.
    pub name: Box<str>,
    /// What starts it, as written; never parsed.
    pub on: Option<Box<str>>,
    /// One sentence for its card.
    pub about: Option<Box<str>>,
    /// The blocks it starts at, in the order they run; never empty.
    pub code: Vec<EntranceTarget>,
}

/// Something a person runs: one `[[app]]` table of `unfold.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    /// `entrance:<app slug>` ([`Key::app`]).
    pub key: Key,
    /// Its heading on Home.
    pub name: Box<str>,
    /// The command that starts it, as written; never run or parsed.
    pub run: Option<Box<str>>,
    /// Where that command starts, usually a module.
    pub code: Option<EntranceTarget>,
    /// One sentence for its card.
    pub about: Option<Box<str>>,
    /// What it handles, in file order.
    pub entrances: Vec<Entrance>,
}

/// The whole graph: files in path order, the inputs that shaped it, and the apps of
/// `unfold.toml` in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Graph {
    /// Files sorted by path.
    pub files: Vec<FileGraph>,
    /// Inputs sorted by path.
    pub inputs: Vec<Input>,
    /// Apps in file order, each with its entrances in file order.
    pub apps: Vec<App>,
}

impl Graph {
    /// Puts files and inputs in path order. Apps keep the order `unfold.toml` writes them in.
    pub fn normalize(&mut self) {
        self.files.sort_by(|a, b| a.file.path.cmp(&b.file.path));
        self.inputs.sort_by(|a, b| a.path.cmp(&b.path));
    }

    /// The file graph at `path`, if indexed.
    pub fn file(&self, path: &str) -> Option<&FileGraph> {
        self.files
            .binary_search_by(|file| file.file.path.as_str().cmp(path))
            .ok()
            .map(|index| &self.files[index])
    }
}

/// An external origin: one plain label (`express`, `node:fs`, `python:builtins`, `ecmascript`),
/// or, when several origins are possible, a compact JSON array of two or more distinct nonempty
/// labels sorted by their UTF-16 code units, stored as its text.
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
    use crate::testkit::sample;

    #[test]
    fn graphs_round_trip_through_postcard_and_json() {
        let graph = sample::shop().build();
        let packed = postcard::to_allocvec(&graph).unwrap();
        assert_eq!(postcard::from_bytes::<Graph>(&packed).unwrap(), graph);
        let json = serde_json::to_string(&graph).unwrap();
        assert_eq!(serde_json::from_str::<Graph>(&json).unwrap(), graph);
        assert!(graph.file("shop/service.py").is_some());
        assert!(graph.file("shop/absent.py").is_none());
    }

    #[test]
    fn link_targets_serialize_with_their_outcome() {
        let project = LinkTarget::Project(Key::from_text("shop/payments.py#charge_card"));
        assert_eq!(project.outcome(), Outcome::Project);
        assert_eq!(
            serde_json::to_string(&project).unwrap(),
            r#"{"project":"shop/payments.py#charge_card"}"#
        );
        let unknown = LinkTarget::Unknown(Reason::UnknownReceiver);
        assert_eq!(unknown.outcome(), Outcome::Unknown);
        assert_eq!(
            serde_json::to_string(&unknown).unwrap(),
            r#"{"unknown":"unknown_receiver"}"#
        );
        let builtin = LinkTarget::Builtin(ModuleLabel::single("python:builtins").unwrap());
        assert_eq!(builtin.outcome(), Outcome::Builtin);
        assert_eq!(
            LinkTarget::Library(ModuleLabel::single("fastapi").unwrap()).outcome(),
            Outcome::Library
        );
    }

    /// A block belongs to the nearest definition above it, through control blocks, parts,
    /// calls, and exits, and a callback owns what is inside it.
    #[test]
    fn a_block_belongs_to_the_nearest_definition_above_it() {
        let graph = sample::shop().build();
        let owner_key = |path: &str, key: &str| {
            let file = graph.file(path).unwrap();
            let id = file
                .local_id(key)
                .unwrap_or_else(|| panic!("{key} is in the sample"));
            owner(&file.nodes, id).map(|owner| file.nodes[owner as usize].key.as_str())
        };
        let service = "shop/service.py";
        assert_eq!(owner_key(service, "shop/service.py#"), None);
        assert_eq!(
            owner_key(service, "shop/service.py#place_order"),
            Some("shop/service.py#")
        );
        assert_eq!(
            owner_key(service, "shop/service.py#/call:logging.getLogger"),
            Some("shop/service.py#")
        );
        assert_eq!(
            owner_key(
                service,
                "shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed"
            ),
            Some("shop/service.py#place_order")
        );
        assert_eq!(
            owner_key(service, "shop/service.py#place_order/if[0]/then"),
            Some("shop/service.py#place_order")
        );
        let orders = "src/orders.ts";
        let route = "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")";
        assert_eq!(owner_key(orders, route), Some("src/orders.ts#"));
        let callback = format!("{route}/callback[0]");
        assert_eq!(owner_key(orders, &callback), Some("src/orders.ts#"));
        assert_eq!(
            owner_key(orders, &format!("{callback}/call:loadOrder")),
            Some(callback.as_str())
        );
        assert_eq!(
            owner_key(
                orders,
                &format!("{callback}/call:res.status().json/call:res.status")
            ),
            Some(callback.as_str())
        );
        let payments = "shop/payments.py";
        assert_eq!(
            owner_key(payments, "shop/payments.py#PaymentDeclined"),
            Some("shop/payments.py#")
        );
        // A decorator sits in the enclosing body, so it belongs to the enclosing definition.
        assert_eq!(
            owner_key(payments, "shop/payments.py#/call:retry"),
            Some("shop/payments.py#")
        );
        let file = graph.file(service).unwrap();
        assert_eq!(owner(&file.nodes, u32::MAX), None);
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
