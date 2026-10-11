//! Test helpers for this and downstream crates (the `testkit` feature): the repository's
//! `testdata/` directory, golden-file comparison, and a builder for hand-written graphs.
//!
//! These helpers panic on misuse; they are for tests only.

pub mod sample;

use std::path::{Path, PathBuf};

use crate::key::{Key, KeyBuilder};
use crate::model::{
    Block, Edge, Entrance, EntranceTarget, Evidence, FileGraph, FileRecord, Graph, InputRecord, ModuleLabel,
    Slot,
};
use crate::path::RepoPath;
use crate::text::{ByteRange, LineIndex, Snapshot};
use crate::typed::{FileTypes, TypeLink, WrittenType};
use crate::vocab::{
    BlockKind, DetectorId, EdgeKind, EntranceKind, FileState, Heritage, HttpMethod, InputRole, Language,
    Reason, Resolution, Role, SlotName, TriggerStatus,
};

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

/// A block added to a [`GraphBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRef {
    file: usize,
    local_id: u32,
}

impl BlockRef {
    /// The block's local ID in its file.
    pub fn local_id(self) -> u32 {
        self.local_id
    }
}

/// An entrance added to a [`GraphBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntranceRef(usize);

/// How an edge or target resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// To this target.
    Resolved(Key),
    /// To this module label.
    External(ModuleLabel),
    /// For this reason.
    Unresolved(Reason),
}

impl Outcome {
    /// An external outcome with a single plain label.
    pub fn external(label: &str) -> Self {
        Self::External(ModuleLabel::single(label).expect("a nonempty plain module label"))
    }

    fn split(self) -> (Option<Key>, Resolution, Option<Reason>, Option<ModuleLabel>) {
        match self {
            Self::Resolved(key) => (Some(key), Resolution::Resolved, None, None),
            Self::External(module) => (None, Resolution::External, None, Some(module)),
            Self::Unresolved(reason) => (None, Resolution::Unresolved, Some(reason), None),
        }
    }
}

#[derive(Debug)]
struct FileDraft {
    snapshot: Snapshot,
    language: Language,
    lines: LineIndex,
    keys: KeyBuilder,
    graph: FileGraph,
}

/// Builds small graphs by hand, computing hashes, lines, keys, and edge sequence numbers.
///
/// Add blocks in preorder. The result is normalized but not validated, so tests can build
/// deliberately broken graphs and mutate them further.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    files: Vec<FileDraft>,
    inputs: Vec<(InputRecord, Snapshot)>,
    entrances: Vec<Entrance>,
}

impl GraphBuilder {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a current file with `text` and returns its root block.
    pub fn file(&mut self, path: &str, language: Language, text: &str) -> BlockRef {
        let path = RepoPath::new(path).expect("a valid test path");
        let snapshot = Snapshot::from_text(path.clone(), text).expect("test text fits in a snapshot");
        let lines = LineIndex::new(text);
        let range = ByteRange {
            start: 0,
            end: snapshot.len(),
        };
        let mut keys = KeyBuilder::new(&path);
        let key = keys
            .push(None, BlockKind::File, None)
            .expect("the first block is the root");
        let root = Block {
            local_id: 0,
            parent: None,
            kind: BlockKind::File,
            role: None,
            name: None,
            range,
            lines: lines.span(range),
            key,
        };
        let file = FileRecord {
            path,
            language,
            state: FileState::Current,
            hash: Some(snapshot.hash()),
            failure: None,
        };
        self.files.push(FileDraft {
            snapshot,
            language,
            lines,
            keys,
            graph: FileGraph {
                file,
                blocks: vec![root],
                slots: Vec::new(),
                edges: Vec::new(),
                types: FileTypes::default(),
            },
        });
        BlockRef {
            file: self.files.len() - 1,
            local_id: 0,
        }
    }

    /// The byte range of the first occurrence of `needle` at or after byte `from` in the file
    /// that contains `block`.
    pub fn find_from(&self, block: BlockRef, needle: &str, from: u32) -> ByteRange {
        let text = self.files[block.file].snapshot.text();
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

    /// The byte range of the first occurrence of `needle` in the file that contains `block`.
    pub fn find(&self, block: BlockRef, needle: &str) -> ByteRange {
        self.find_from(block, needle, 0)
    }

    /// Adds the next block in preorder under `parent`.
    pub fn block(
        &mut self,
        parent: BlockRef,
        kind: BlockKind,
        role: Option<Role>,
        name: Option<&str>,
        range: ByteRange,
    ) -> BlockRef {
        let draft = &mut self.files[parent.file];
        let key = draft
            .keys
            .push(Some(parent.local_id), kind, name)
            .expect("blocks are added in preorder");
        let local_id = u32::try_from(draft.graph.blocks.len()).expect("test block counts fit in u32");
        draft.graph.blocks.push(Block {
            local_id,
            parent: Some(parent.local_id),
            kind,
            role,
            name: name.map(Into::into),
            range,
            lines: draft.lines.span(range),
            key,
        });
        BlockRef {
            file: parent.file,
            local_id,
        }
    }

    /// Adds a slot to `block`.
    pub fn slot(&mut self, block: BlockRef, slot: SlotName, index: Option<u32>, range: ByteRange) {
        self.files[block.file].graph.slots.push(Slot {
            block: block.local_id,
            slot,
            index,
            range,
        });
    }

    /// Adds the next edge sourced at `source`, numbering it after the source's earlier edges.
    pub fn edge(
        &mut self,
        source: BlockRef,
        kind: EdgeKind,
        outcome: Outcome,
        text: &str,
        position: Option<u32>,
    ) {
        let edges = &mut self.files[source.file].graph.edges;
        let seq = u32::try_from(edges.iter().filter(|edge| edge.source == source.local_id).count())
            .expect("test edge counts fit in u32");
        let (target, resolution, reason, module) = outcome.split();
        edges.push(Edge {
            source: source.local_id,
            seq,
            kind,
            target,
            text: text.into(),
            resolution,
            reason,
            module,
            position,
        });
    }

    /// The typed records of the file that contains `block`, to add records to by hand.
    pub fn types(&mut self, block: BlockRef) -> &mut FileTypes {
        &mut self.files[block.file].graph.types
    }

    /// The written type `needle`, found at or after byte `from` in the file that contains
    /// `block`.
    pub fn written_from(&self, block: BlockRef, needle: &str, from: u32) -> WrittenType {
        WrittenType {
            text: needle.into(),
            range: self.find_from(block, needle, from),
        }
    }

    /// Adds a type link sourced at `block` over the first `needle` at or after byte `from`.
    pub fn type_link(&mut self, block: BlockRef, needle: &str, from: u32, outcome: Outcome) {
        self.push_type_link(block, needle, from, outcome, None);
    }

    /// Adds a type link naming a base of `block` (a class or interface) in a heritage clause,
    /// over the first `needle` at or after byte `from`.
    pub fn heritage_link(
        &mut self,
        block: BlockRef,
        needle: &str,
        from: u32,
        outcome: Outcome,
        heritage: Heritage,
    ) {
        self.push_type_link(block, needle, from, outcome, Some(heritage));
    }

    fn push_type_link(
        &mut self,
        block: BlockRef,
        needle: &str,
        from: u32,
        outcome: Outcome,
        heritage: Option<Heritage>,
    ) {
        let range = self.find_from(block, needle, from);
        let (target, resolution, reason, module) = outcome.split();
        self.types(block).type_links.push(TypeLink {
            block: block.local_id,
            range,
            text: needle.into(),
            resolution,
            target,
            reason,
            module,
            heritage,
        });
    }

    /// The structural key of `block`.
    pub fn key(&self, block: BlockRef) -> Key {
        self.files[block.file].graph.blocks[block.local_id as usize]
            .key
            .clone()
    }

    /// The byte range of `block`.
    pub fn range(&self, block: BlockRef) -> ByteRange {
        self.files[block.file].graph.blocks[block.local_id as usize].range
    }

    /// The language of the file containing `block`.
    pub fn language(&self, block: BlockRef) -> Language {
        self.files[block.file].language
    }

    /// Adds a configuration input with `text`.
    pub fn input(&mut self, path: &str, role: InputRole, text: &str) {
        let path = RepoPath::new(path).expect("a valid test path");
        let snapshot = Snapshot::from_text(path.clone(), text).expect("test text fits in a snapshot");
        self.inputs.push((
            InputRecord {
                path,
                role,
                hash: snapshot.hash(),
            },
            snapshot,
        ));
    }

    /// Evidence that is the source block `block`.
    pub fn source_evidence(&self, block: BlockRef) -> Evidence {
        let draft = &self.files[block.file];
        let block = &draft.graph.blocks[block.local_id as usize];
        Evidence {
            path: draft.graph.file.path.clone(),
            range: block.range,
            block: Some(block.key.clone()),
            pointer: None,
        }
    }

    /// Evidence in a configuration input: the range of `needle` in it, and `pointer`.
    pub fn config_evidence(&self, path: &str, needle: &str, pointer: &str) -> Evidence {
        let (input, snapshot) = self
            .inputs
            .iter()
            .find(|(input, _)| input.path.as_str() == path)
            .expect("the input was added");
        let start = snapshot
            .text()
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found in {path}"));
        let start = u32::try_from(start).expect("test offsets fit in u32");
        let end = start + u32::try_from(needle.len()).expect("test offsets fit in u32");
        Evidence {
            path: input.path.clone(),
            range: ByteRange { start, end },
            block: None,
            pointer: Some(pointer.into()),
        }
    }

    /// Adds an entrance with no targets. The trigger status is `exact` for a trigger and
    /// `unknown` without one; change it on the built graph if a test needs `partial`.
    pub fn entrance(
        &mut self,
        detector: DetectorId,
        kind: EntranceKind,
        method: Option<HttpMethod>,
        trigger: Option<&str>,
        evidence: Evidence,
    ) -> EntranceRef {
        self.entrances.push(Entrance {
            key: Key::from_text(""),
            kind,
            label: None,
            method,
            trigger: trigger.map(Into::into),
            trigger_status: if trigger.is_some() {
                TriggerStatus::Exact
            } else {
                TriggerStatus::Unknown
            },
            detector,
            evidence,
            targets: Vec::new(),
        });
        EntranceRef(self.entrances.len() - 1)
    }

    /// Adds the next target of `entrance`, with the role its kind requires.
    pub fn target(&mut self, entrance: EntranceRef, outcome: Outcome, text: &str) {
        let entrance = &mut self.entrances[entrance.0];
        let (target, resolution, reason, module) = outcome.split();
        let position = u32::try_from(entrance.targets.len()).expect("test target counts fit in u32");
        entrance.targets.push(EntranceTarget {
            position,
            role: entrance.kind.target_role(),
            target,
            text: text.into(),
            resolution,
            reason,
            module,
        });
    }

    /// Snapshots of every file and input, for source-aware validation.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let files = self.files.iter().map(|draft| draft.snapshot.clone());
        files
            .chain(self.inputs.iter().map(|(_, snapshot)| snapshot.clone()))
            .collect()
    }

    /// The graph, normalized (files, inputs, and entrances sorted; entrance keys derived;
    /// typed records in canonical order).
    pub fn build(&self) -> Graph {
        let mut graph = Graph {
            files: self.files.iter().map(|draft| draft.graph.clone()).collect(),
            inputs: self.inputs.iter().map(|(input, _)| input.clone()).collect(),
            entrances: self.entrances.clone(),
        };
        graph.normalize();
        for file in &mut graph.files {
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
    fn the_builder_numbers_edges_per_source() {
        let graph = sample::sample().build();
        let orders = graph.file("src/orders.ts").unwrap();
        let seqs: Vec<(u32, u32)> = orders.edges.iter().map(|edge| (edge.source, edge.seq)).collect();
        assert_eq!(seqs, [(1, 0), (5, 0), (7, 0), (7, 1)]);
        assert_eq!(graph.entrances.len(), 3);
        assert!(graph.entrances.iter().all(|entrance| entrance.key.is_entrance()));
    }
}
