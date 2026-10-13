//! Test helpers for this and downstream crates (the `testkit` feature): the repository's
//! `testdata/` directory, golden-file comparison, and a builder for hand-written graphs.
//!
//! These helpers panic on misuse; they are for tests only.

pub mod sample;

use std::path::{Path, PathBuf};

use crate::key::{Callee, Child, Key, KeyBuilder};
use crate::model::{
    App, Entrance, EntranceTarget, File, FileGraph, Graph, Input, Link, LinkTarget, ModuleLabel, Node,
};
use crate::path::RepoPath;
use crate::text::{ByteRange, LineIndex, Snapshot};
use crate::typed::{FileTypes, WrittenType};
use crate::vocab::{FileState, Flags, Form, Inline, InputRole, Language, LinkKind, Reason, TargetState};

/// The repository's `testdata/` directory.
pub fn testdata_dir() -> PathBuf {
    // This crate lives at `crates/unfold-core`, two levels below the repository root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

/// Compares `actual` with the file at `relative` under `testdata/`. With `UNFOLD_BLESS=1` in
/// the environment it writes `actual` there instead (creating directories as needed).
pub fn assert_golden(relative: impl AsRef<Path>, actual: &str) {
    let path = testdata_dir().join(relative.as_ref());
    if std::env::var_os("UNFOLD_BLESS").is_some_and(|value| value == "1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create the golden file's directory");
        }
        std::fs::write(&path, actual).expect("write the golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read golden file {}: {error}\nrun with UNFOLD_BLESS=1 to create it",
            path.display()
        )
    });
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        panic!(
            "golden file {} differs at line {}:\n  expected: {}\n  actual:   {}\nrun with UNFOLD_BLESS=1 to update it",
            path.display(),
            line + 1,
            expected.lines().nth(line).unwrap_or("<end of file>"),
            actual.lines().nth(line).unwrap_or("<end of file>"),
        );
    }
}

/// A link target in this project.
pub fn project(key: &Key) -> LinkTarget {
    LinkTarget::Project(key.clone())
}

/// A link target in the outside module `label`.
pub fn library(label: &str) -> LinkTarget {
    LinkTarget::Library(ModuleLabel::single(label).expect("a nonempty plain module label"))
}

/// A link target in the language's builtins, `label` (`python:builtins`, `ecmascript`).
pub fn builtin(label: &str) -> LinkTarget {
    LinkTarget::Builtin(ModuleLabel::single(label).expect("a nonempty plain module label"))
}

/// An unknown link target.
pub fn unknown(reason: Reason) -> LinkTarget {
    LinkTarget::Unknown(reason)
}

/// An app's or entrance's `code` key and its state.
pub fn target(code: &str, state: TargetState) -> EntranceTarget {
    EntranceTarget {
        code: Key::from_text(code),
        state,
    }
}

/// A node added to a [`GraphBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRef {
    file: usize,
    local_id: u32,
}

impl NodeRef {
    /// The node's local ID in its file.
    pub fn local_id(self) -> u32 {
        self.local_id
    }
}

/// An app added to a [`GraphBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppRef(usize);

#[derive(Debug)]
struct FileDraft {
    snapshot: Snapshot,
    lines: LineIndex,
    keys: KeyBuilder,
    graph: FileGraph,
}

/// Builds small graphs by hand, computing hashes, lines, keys, link sequence numbers, and
/// app and entrance keys.
///
/// Add each file's nodes in preorder. The result is normalized but not validated, so tests can
/// build deliberately broken graphs and change them further.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    files: Vec<FileDraft>,
    inputs: Vec<(Input, Snapshot)>,
    apps: Vec<App>,
}

impl GraphBuilder {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a current file with `text` and returns its module, which spans the whole file.
    pub fn file(&mut self, path: &str, language: Language, text: &str) -> NodeRef {
        let path = RepoPath::new(path).expect("a valid test path");
        let snapshot = Snapshot::from_text(path.clone(), text).expect("test text fits in a snapshot");
        let lines = LineIndex::new(text);
        let keys = KeyBuilder::new(&path);
        let range = ByteRange {
            start: 0,
            end: snapshot.len(),
        };
        let module = Node {
            key: keys.keys()[0].clone(),
            parent: None,
            form: Form::Module,
            name: None,
            range,
            lines: lines.span(range),
            head: None,
            body: Some(range),
            doc: None,
            stmt: None,
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags: Flags::NONE,
            target: None,
        };
        let file = File {
            path,
            language,
            state: FileState::Current,
            hash: Some(snapshot.hash()),
            failure: None,
        };
        self.files.push(FileDraft {
            snapshot,
            lines,
            keys,
            graph: FileGraph {
                file,
                nodes: vec![module],
                links: Vec::new(),
                types: FileTypes::default(),
            },
        });
        NodeRef {
            file: self.files.len() - 1,
            local_id: 0,
        }
    }

    /// The byte range of the first occurrence of `needle` at or after byte `from` in the file
    /// that contains `node`.
    pub fn find_from(&self, node: NodeRef, needle: &str, from: u32) -> ByteRange {
        let text = self.files[node.file].snapshot.text();
        let start = text[from as usize..]
            .find(needle)
            .map(|offset| offset + from as usize);
        let start = start.unwrap_or_else(|| panic!("`{needle}` not found after byte {from}"));
        let start = u32::try_from(start).expect("test offsets fit in u32");
        ByteRange {
            start,
            end: start + u32::try_from(needle.len()).expect("test offsets fit in u32"),
        }
    }

    /// The byte range of the first occurrence of `needle` in the file that contains `node`.
    pub fn find(&self, node: NodeRef, needle: &str) -> ByteRange {
        self.find_from(node, needle, 0)
    }

    /// The byte range of the first occurrence of `needle` at or after the start of `node`.
    pub fn find_in(&self, node: NodeRef, needle: &str) -> ByteRange {
        self.find_from(node, needle, self.range(node).start)
    }

    /// The byte range of the first occurrence of `needle` at or after the start of `node`
    /// that is not part of a longer identifier: `order` in `save_order(order)` is the argument.
    pub fn find_word_in(&self, node: NodeRef, needle: &str) -> ByteRange {
        let text = self.files[node.file].snapshot.text();
        let is_word = |character: Option<char>| character.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let mut from = self.range(node).start;
        loop {
            let found = self.find_from(node, needle, from);
            let before = text[..found.start as usize].chars().next_back();
            let after = text[found.end as usize..].chars().next();
            if !is_word(before) && !is_word(after) {
                return found;
            }
            from = found.start + 1;
        }
    }

    /// The text of `range` in the file that contains `node`.
    pub fn source(&self, node: NodeRef, range: ByteRange) -> &str {
        &self.files[node.file].snapshot.text()[range.as_usize()]
    }

    /// Adds the next node in preorder under `parent`, keyed by `child`. A definition's name
    /// and a call's callee become the node's name; every optional field starts empty.
    pub fn node(&mut self, parent: NodeRef, child: Child<'_>, form: Form, range: ByteRange) -> NodeRef {
        let draft = &mut self.files[parent.file];
        let key = draft
            .keys
            .push(parent.local_id, child)
            .expect("nodes are added in preorder");
        let name = match child {
            Child::Definition(name) => Some(name.into()),
            Child::Call { callee, .. } => Some(callee.text().into()),
            Child::Inline(_) | Child::Form(_) => None,
        };
        let local_id = u32::try_from(draft.graph.nodes.len()).expect("test node counts fit in u32");
        draft.graph.nodes.push(Node {
            key,
            parent: Some(parent.local_id),
            form,
            name,
            range,
            lines: draft.lines.span(range),
            head: None,
            body: None,
            doc: None,
            stmt: None,
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags: Flags::NONE,
            target: None,
        });
        NodeRef {
            file: parent.file,
            local_id,
        }
    }

    /// Adds a named definition of `form` (a class, function, or type).
    pub fn def(&mut self, parent: NodeRef, form: Form, name: &str, range: ByteRange) -> NodeRef {
        self.node(parent, Child::Definition(name), form, range)
    }

    /// Adds an unnamed function (`lambda`) written as `inline`.
    pub fn inline(&mut self, parent: NodeRef, inline: Inline, range: ByteRange) -> NodeRef {
        self.node(parent, Child::Inline(inline), Form::Lambda, range)
    }

    /// Adds a control block, part, or exit of `form`.
    pub fn block(&mut self, parent: NodeRef, form: Form, range: ByteRange) -> NodeRef {
        self.node(parent, Child::Form(form), form, range)
    }

    /// Adds a call that takes no callback.
    pub fn call(&mut self, parent: NodeRef, callee: &Callee, range: ByteRange) -> NodeRef {
        let child = Child::Call {
            callee,
            first_literal: None,
            takes_callback: false,
        };
        self.node(parent, child, Form::Call, range)
    }

    /// Adds a construction (form `new`) that takes no callback.
    pub fn construction(&mut self, parent: NodeRef, callee: &Callee, range: ByteRange) -> NodeRef {
        let child = Child::Call {
            callee,
            first_literal: None,
            takes_callback: false,
        };
        self.node(parent, child, Form::New, range)
    }

    /// Adds a call that takes a callback, whose first argument is the string `literal` when
    /// there is one.
    pub fn registration(
        &mut self,
        parent: NodeRef,
        callee: &Callee,
        literal: Option<&str>,
        range: ByteRange,
    ) -> NodeRef {
        let child = Child::Call {
            callee,
            first_literal: literal,
            takes_callback: true,
        };
        self.node(parent, child, Form::Call, range)
    }

    /// The node, to fill in its optional fields.
    pub fn node_mut(&mut self, node: NodeRef) -> &mut Node {
        &mut self.files[node.file].graph.nodes[node.local_id as usize]
    }

    /// Adds the next link from `from`, numbering it after the node's earlier links. Its
    /// `implements` is false and its `bound` absent; set them on the returned link.
    pub fn link(
        &mut self,
        from: NodeRef,
        kind: LinkKind,
        range: ByteRange,
        text: &str,
        to: LinkTarget,
    ) -> &mut Link {
        let links = &mut self.files[from.file].graph.links;
        let seq = links.iter().filter(|link| link.from == from.local_id).count();
        links.push(Link {
            from: from.local_id,
            seq: u32::try_from(seq).expect("test link counts fit in u32"),
            kind,
            range,
            text: text.into(),
            to,
            implements: false,
            bound: None,
        });
        links.last_mut().expect("a link was just pushed")
    }

    /// The typed records of the file that contains `node`, to add records to by hand.
    pub fn types(&mut self, node: NodeRef) -> &mut FileTypes {
        &mut self.files[node.file].graph.types
    }

    /// The written type `needle`, found at or after byte `from` in the file that contains
    /// `node`.
    pub fn written_from(&self, node: NodeRef, needle: &str, from: u32) -> WrittenType {
        WrittenType {
            text: needle.into(),
            range: self.find_from(node, needle, from),
        }
    }

    /// The structural key of `node`.
    pub fn key(&self, node: NodeRef) -> Key {
        self.files[node.file].graph.nodes[node.local_id as usize]
            .key
            .clone()
    }

    /// The byte range of `node`.
    pub fn range(&self, node: NodeRef) -> ByteRange {
        self.files[node.file].graph.nodes[node.local_id as usize].range
    }

    /// The language of the file containing `node`.
    pub fn language(&self, node: NodeRef) -> Language {
        self.files[node.file].graph.file.language
    }

    /// Adds a configuration input with `text`.
    pub fn input(&mut self, path: &str, role: InputRole, text: &str) {
        let path = RepoPath::new(path).expect("a valid test path");
        let snapshot = Snapshot::from_text(path.clone(), text).expect("test text fits in a snapshot");
        self.inputs.push((
            Input {
                path,
                role,
                hash: snapshot.hash(),
            },
            snapshot,
        ));
    }

    /// Adds an app with no entrances, keyed by its name.
    pub fn app(&mut self, name: &str, run: Option<&str>, code: Option<EntranceTarget>) -> AppRef {
        self.apps.push(App {
            key: Key::app(name).expect("an app name with letters or digits"),
            name: name.into(),
            run: run.map(Into::into),
            code,
            about: None,
            entrances: Vec::new(),
        });
        AppRef(self.apps.len() - 1)
    }

    /// Adds the next entrance of `app`, keyed by the two names.
    pub fn entrance(&mut self, app: AppRef, name: &str, on: Option<&str>, code: Vec<EntranceTarget>) {
        let app = &mut self.apps[app.0];
        app.entrances.push(Entrance {
            key: Key::entrance(&app.name, name).expect("names with letters or digits"),
            name: name.into(),
            on: on.map(Into::into),
            about: None,
            code,
        });
    }

    /// The app, to fill in its optional fields.
    pub fn app_mut(&mut self, app: AppRef) -> &mut App {
        &mut self.apps[app.0]
    }

    /// Snapshots of every file and input, for source-aware validation.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let files = self.files.iter().map(|draft| draft.snapshot.clone());
        files
            .chain(self.inputs.iter().map(|(_, snapshot)| snapshot.clone()))
            .collect()
    }

    /// The graph, normalized (files and inputs sorted, typed records in canonical order).
    pub fn build(&self) -> Graph {
        let mut graph = Graph {
            files: self.files.iter().map(|draft| draft.graph.clone()).collect(),
            inputs: self.inputs.iter().map(|(input, _)| input.clone()).collect(),
            apps: self.apps.clone(),
        };
        graph.normalize();
        for file in &mut graph.files {
            file.links.sort_by_key(|link| (link.from, link.seq));
            file.types.sort();
        }
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testdata_is_at_the_repository_root() {
        let dir = testdata_dir();
        assert!(
            dir.join("README.md").is_file(),
            "{} has no README.md",
            dir.display()
        );
    }

    #[test]
    fn golden_files_compare_equal_content() {
        let readme = std::fs::read_to_string(testdata_dir().join("README.md")).unwrap();
        // With UNFOLD_BLESS=1 this rewrites the same bytes, which is harmless.
        assert_golden("README.md", &readme);
    }

    #[test]
    fn the_builder_keys_nodes_and_numbers_links_per_source() {
        let mut g = GraphBuilder::new();
        let file = g.file("a.py", Language::Python, "def f():\n    g()\n    g()\n");
        let f = g.def(
            file,
            Form::Function,
            "f",
            g.find(file, "def f():\n    g()\n    g()"),
        );
        let first = g.call(f, &Callee::name("g"), g.find(file, "g()"));
        let second = g.call(
            f,
            &Callee::name("g"),
            g.find_from(file, "g()", g.range(first).end),
        );
        g.link(
            first,
            LinkKind::Calls,
            g.find(file, "g"),
            "g",
            unknown(Reason::UnknownName),
        );
        g.link(
            second,
            LinkKind::Calls,
            g.find_in(second, "g"),
            "g",
            unknown(Reason::UnknownName),
        );
        g.link(
            second,
            LinkKind::Passes,
            g.find_in(second, "g"),
            "g",
            unknown(Reason::UnknownName),
        )
        .bound = Some(0);
        let graph = g.build();
        let file = graph.file("a.py").unwrap();
        let keys: Vec<&str> = file.nodes.iter().map(|node| node.key.as_str()).collect();
        assert_eq!(keys, ["a.py#", "a.py#f", "a.py#f/call:g", "a.py#f/call:g[1]"]);
        assert_eq!(file.nodes[3].name.as_deref(), Some("g"));
        assert_eq!(file.nodes[3].lines.start, 3);
        let seqs: Vec<(u32, u32)> = file.links.iter().map(|link| (link.from, link.seq)).collect();
        assert_eq!(seqs, [(2, 0), (3, 0), (3, 1)]);
        assert_eq!(file.links[2].bound, Some(0));
    }
}
