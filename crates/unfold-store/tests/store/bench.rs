//! A first-compile-sized commit: about 260k blocks, 520k slots, and 100k edges in 600 files,
//! the size of TypeScript's compiler source (docs/testing.md#budgets).
//!
//! Ignored by default. Run it optimized:
//! `cargo test -p unfold-store --release --test store -- --ignored --nocapture bench`

use std::time::Instant;

use unfold_core::vocab::{BlockKind, EdgeKind, FileState, Language, Resolution, Role, SlotName};
use unfold_core::{
    Block, ByteRange, ContentHash, Edge, FileGraph, FileRecord, Key, KeyBuilder, LineSpan, RepoPath, Slot,
};
use unfold_store::{CandidateKind, EntranceChanges, FileChange, Update};

use crate::common::{definitions_of, meta, stamp, temp_store};

const FILES: usize = 600;
const FUNCTIONS: usize = 54;
const NESTED: [BlockKind; 7] = [
    BlockKind::Variable,
    BlockKind::Call,
    BlockKind::If,
    BlockKind::Return,
    BlockKind::Call,
    BlockKind::ExpressionStatement,
    BlockKind::Call,
];

fn path(file: usize) -> RepoPath {
    RepoPath::new(&format!("src/compiler/module{:03}/file{file:04}.ts", file % 40)).expect("a valid path")
}

/// The key of function `function` in file `file`.
fn function_key(file: usize, function: usize) -> Key {
    Key::from_text(&format!("{}#function:function{function}", path(file)))
}

fn synthetic_file(file: usize) -> FileGraph {
    let path = path(file);
    let mut keys = KeyBuilder::new(&path);
    let mut blocks = Vec::new();
    let mut slots = Vec::new();
    let mut edges = Vec::new();
    let size = (FUNCTIONS * 800) as u32;
    let root_key = keys
        .push(None, BlockKind::File, None)
        .expect("the root comes first");
    blocks.push(Block {
        local_id: 0,
        parent: None,
        kind: BlockKind::File,
        role: None,
        name: None,
        range: ByteRange { start: 0, end: size },
        lines: LineSpan {
            start: 1,
            end: (FUNCTIONS * 20) as u32,
        },
        key: root_key,
    });
    for function in 0..FUNCTIONS {
        let base = (function * 800) as u32;
        let line = (function * 20) as u32 + 1;
        let local_id = blocks.len() as u32;
        let name = format!("function{function}");
        blocks.push(Block {
            local_id,
            parent: Some(0),
            kind: BlockKind::Function,
            role: Some(Role::Body),
            name: Some(name.as_str().into()),
            range: ByteRange {
                start: base,
                end: base + 790,
            },
            lines: LineSpan {
                start: line,
                end: line + 18,
            },
            key: keys
                .push(Some(0), BlockKind::Function, Some(&name))
                .expect("blocks in preorder"),
        });
        for (slot, offset) in [
            (SlotName::Name, 9),
            (SlotName::Parameter, 30),
            (SlotName::Body, 60),
        ] {
            slots.push(Slot {
                block: local_id,
                slot,
                index: (slot == SlotName::Parameter).then_some(0),
                range: ByteRange {
                    start: base + offset,
                    end: base + offset + 20,
                },
            });
        }
        for (index, kind) in NESTED.iter().copied().enumerate() {
            let child = blocks.len() as u32;
            let start = base + 100 + index as u32 * 90;
            blocks.push(Block {
                local_id: child,
                parent: Some(local_id),
                kind,
                role: Some(Role::Body),
                name: (kind == BlockKind::Variable).then(|| "value".into()),
                range: ByteRange {
                    start,
                    end: start + 80,
                },
                lines: LineSpan {
                    start: line + 2 + index as u32 * 2,
                    end: line + 3 + index as u32 * 2,
                },
                key: keys
                    .push(
                        Some(local_id),
                        kind,
                        (kind == BlockKind::Variable).then_some("value"),
                    )
                    .expect("blocks in preorder"),
            });
            let (first, second) = match kind {
                BlockKind::Call => (SlotName::Callee, SlotName::Argument),
                BlockKind::Variable => (SlotName::Name, SlotName::Initializer),
                BlockKind::If => (SlotName::Condition, SlotName::Then),
                _ => (SlotName::Expression, SlotName::Body),
            };
            for (slot, offset) in [(first, 0), (second, 40)] {
                slots.push(Slot {
                    block: child,
                    slot,
                    index: (slot == SlotName::Argument).then_some(0),
                    range: ByteRange {
                        start: start + offset,
                        end: start + offset + 30,
                    },
                });
            }
            if kind == BlockKind::Call {
                // A deterministic spread of targets across the program.
                let target_file = (file * 7 + function * 13 + index) % FILES;
                let target_function = (function * 5 + index) % FUNCTIONS;
                edges.push(Edge {
                    source: child,
                    seq: 0,
                    kind: EdgeKind::Calls,
                    target: Some(function_key(target_file, target_function)),
                    text: format!("function{target_function}").into(),
                    resolution: Resolution::Resolved,
                    reason: None,
                    module: None,
                    position: None,
                });
            }
        }
    }
    FileGraph {
        file: FileRecord {
            path,
            language: Language::TypeScript,
            state: FileState::Current,
            hash: Some(ContentHash::of(&file.to_le_bytes())),
            failure: None,
        },
        blocks,
        slots,
        edges,
        types: unfold_core::FileTypes::default(),
    }
}

#[test]
#[ignore = "a benchmark; run optimized with --ignored --nocapture"]
fn bench_first_compile_commit() {
    let files: Vec<FileGraph> = (0..FILES).map(synthetic_file).collect();
    let definitions: Vec<_> = files.iter().map(definitions_of).collect();
    let blocks: usize = files.iter().map(|file| file.blocks.len()).sum();
    let slots: usize = files.iter().map(|file| file.slots.len()).sum();
    let edges: usize = files.iter().map(|file| file.edges.len()).sum();
    let update = Update {
        files: files
            .iter()
            .zip(&definitions)
            .map(|(file, definitions)| FileChange {
                definitions: Some(definitions),
                ..FileChange::full(file, stamp(file))
            })
            .collect(),
        entrances: EntranceChanges::default(),
        meta: Some(meta(1)),
        ..Update::default()
    };

    let (dir, mut store) = temp_store();
    let started = Instant::now();
    let stats = store.writer.commit(&update).unwrap();
    let first = started.elapsed();
    eprintln!(
        "first compile: {FILES} files, {blocks} blocks, {slots} slots, {edges} edges, {} definitions in {first:?}",
        stats.definitions_written
    );

    // A background update: one file's edges change.
    let mut changed = files[17].clone();
    for edge in &mut changed.edges {
        edge.target = Some(function_key(3, 3));
    }
    let incremental = Update {
        files: vec![FileChange::between(&files[17], &changed, stamp(&changed))],
        meta: Some(meta(2)),
        ..Update::default()
    };
    let started = Instant::now();
    store.writer.commit(&incremental).unwrap();
    eprintln!("one-file edge update: {:?}", started.elapsed());

    // Rewriting every file's definitions (and their search rows), as a settings change would.
    let all_definitions: Vec<_> = files.iter().map(definitions_of).collect();
    let rewrite = Update {
        files: files
            .iter()
            .zip(&all_definitions)
            .map(|(file, definitions)| FileChange {
                definitions: Some(definitions),
                ..FileChange::between(file, file, stamp(file))
            })
            .collect(),
        ..Update::default()
    };
    let started = Instant::now();
    store.writer.commit(&rewrite).unwrap();
    eprintln!("every file's definitions rewritten: {:?}", started.elapsed());

    let reader = store.readers.get().unwrap();
    let started = Instant::now();
    let graph = reader.file_graph(path(400).as_str()).unwrap().unwrap();
    eprintln!(
        "file graph ({} blocks): {:?}",
        graph.blocks.len(),
        started.elapsed()
    );
    let started = Instant::now();
    let incoming = reader.incoming(function_key(3, 3).as_str(), None, 100).unwrap();
    eprintln!(
        "incoming ({} edges): {:?}",
        incoming.items.len(),
        started.elapsed()
    );
    let started = Instant::now();
    let found = reader
        .search("function12", CandidateKind::Definition, &[], 50)
        .unwrap();
    eprintln!("search ({} candidates): {:?}", found.len(), started.elapsed());
    let started = Instant::now();
    let stats = reader.stats().unwrap();
    eprintln!("stats: {stats:?} in {:?}", started.elapsed());
    drop(reader);
    drop(store);

    // Reopen and commit again: includes loading the key mirror.
    let started = Instant::now();
    let mut store = unfold_store::Store::open(
        &dir.path().join("graph.sqlite"),
        &unfold_store::StoreConfig::new(crate::common::ENGINE),
    )
    .unwrap();
    let opened = started.elapsed();
    let started = Instant::now();
    store.writer.commit(&incremental).unwrap();
    eprintln!(
        "reopen: {opened:?}; first update after reopen (loads the mirror): {:?}",
        started.elapsed()
    );
}
