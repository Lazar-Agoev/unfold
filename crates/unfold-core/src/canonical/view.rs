//! The version-1 view of a graph: what a version-1 export (the proof of concept's references)
//! records, so parity compares only what the reference has.

use rustc_hash::FxHashMap;

use crate::key::{Key, KeyBuilder};
use crate::model::{Block, FileGraph, Graph};
use crate::typed::FileTypes;
use crate::vocab::BlockKind;

/// `graph` as version 1 describes it: no typed records, and no `interface`, `type_alias`, or
/// `enum` blocks (or their slots). A block nested in a removed one (a call in an enum member's
/// initializer) moves up to the removed block's parent, and every key is derived again, so the
/// call under `enum:Status/` is keyed as it was before type declarations were blocks. Edge and
/// entrance targets, and entrance evidence blocks, follow the new keys.
pub fn version_one(graph: &Graph) -> Graph {
    let mut renamed: FxHashMap<Key, Key> = FxHashMap::default();
    let mut files: Vec<FileGraph> = graph
        .files
        .iter()
        .map(|file| strip_file(file, &mut renamed))
        .collect();
    let rename = |key: &Key| renamed.get(key).cloned().unwrap_or_else(|| key.clone());
    for file in &mut files {
        for edge in &mut file.edges {
            if let Some(target) = &mut edge.target {
                *target = rename(target);
            }
        }
    }
    let mut entrances = graph.entrances.clone();
    for entrance in &mut entrances {
        if let Some(block) = &mut entrance.evidence.block {
            *block = rename(block);
        }
        for target in &mut entrance.targets {
            if let Some(key) = &mut target.target {
                *key = rename(key);
            }
        }
    }
    Graph {
        files,
        inputs: graph.inputs.clone(),
        entrances,
    }
}

/// One file without its type declaration blocks, recording every key that changed.
fn strip_file(file: &FileGraph, renamed: &mut FxHashMap<Key, Key>) -> FileGraph {
    let declaration = |kind: BlockKind| {
        matches!(
            kind,
            BlockKind::Interface | BlockKind::TypeAlias | BlockKind::Enum
        )
    };
    if !file.blocks.iter().any(|block| declaration(block.kind)) {
        return FileGraph {
            types: FileTypes::default(),
            ..file.clone()
        };
    }
    // Old local ID → new local ID, for kept blocks.
    let mut map: Vec<Option<u32>> = Vec::with_capacity(file.blocks.len());
    let mut keys = KeyBuilder::new(&file.file.path);
    let mut blocks: Vec<Block> = Vec::with_capacity(file.blocks.len());
    for block in &file.blocks {
        if declaration(block.kind) {
            map.push(None);
            continue;
        }
        // The nearest kept ancestor; blocks precede their children, so it is already mapped.
        let mut parent = block.parent;
        while let Some(old) = parent {
            match map.get(old as usize) {
                Some(Some(new)) => {
                    parent = Some(*new);
                    break;
                }
                _ => parent = file.blocks.get(old as usize).and_then(|found| found.parent),
            }
        }
        let local_id = blocks.len() as u32;
        let key = keys
            .push(parent, block.kind, block.name.as_deref())
            .unwrap_or_else(|_| block.key.clone());
        if key != block.key {
            renamed.insert(block.key.clone(), key.clone());
        }
        map.push(Some(local_id));
        blocks.push(Block {
            local_id,
            parent,
            key,
            ..block.clone()
        });
    }
    let local = |old: u32| map.get(old as usize).copied().flatten();
    let slots = file
        .slots
        .iter()
        .filter_map(|slot| {
            Some(crate::model::Slot {
                block: local(slot.block)?,
                ..*slot
            })
        })
        .collect();
    let edges = file
        .edges
        .iter()
        .filter_map(|edge| {
            let mut edge = edge.clone();
            edge.source = local(edge.source)?;
            Some(edge)
        })
        .collect();
    FileGraph {
        file: file.file.clone(),
        blocks,
        slots,
        edges,
        types: FileTypes::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::GraphBuilder;
    use crate::testkit::sample::{sample, typed};
    use crate::vocab::{EdgeKind, Language, Role, SlotName};

    #[test]
    fn graphs_without_type_declarations_only_lose_typed_records() {
        let graph = sample().build();
        let view = version_one(&graph);
        assert!(view.files.iter().all(|file| file.types.is_empty()));
        let mut untyped = graph;
        for file in &mut untyped.files {
            file.types = FileTypes::default();
        }
        assert_eq!(view, untyped);
        let typed = version_one(&typed().build());
        assert!(
            typed
                .files
                .iter()
                .flat_map(|file| &file.blocks)
                .all(|block| !block.kind.is_type_declaration() || block.kind == BlockKind::Class)
        );
    }

    #[test]
    fn calls_in_enum_members_move_up_and_are_keyed_again() {
        let text = "f();\nenum E { A = g() }\n";
        let mut g = GraphBuilder::new();
        let file = g.file("a.ts", Language::TypeScript, text);
        let statement = g.block(
            file,
            BlockKind::ExpressionStatement,
            Some(Role::Body),
            None,
            g.find(file, "f();"),
        );
        let first = g.block(
            statement,
            BlockKind::Call,
            Some(Role::Expression),
            None,
            g.find(file, "f()"),
        );
        let enumeration = g.block(
            file,
            BlockKind::Enum,
            Some(Role::Body),
            Some("E"),
            g.find(file, "enum E { A = g() }"),
        );
        g.slot(enumeration, SlotName::Name, None, g.find(file, "E"));
        let call = g.block(
            enumeration,
            BlockKind::Call,
            Some(Role::Body),
            None,
            g.find(file, "g()"),
        );
        g.edge(
            first,
            EdgeKind::Calls,
            crate::testkit::Outcome::Resolved(g.key(call)),
            "f",
            None,
        );
        g.edge(
            call,
            EdgeKind::Calls,
            crate::testkit::Outcome::Resolved(g.key(call)),
            "g",
            None,
        );
        let view = version_one(&g.build());
        let keys: Vec<&str> = view.files[0]
            .blocks
            .iter()
            .map(|block| block.key.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "a.ts#",
                "a.ts#expression_statement[0]",
                "a.ts#expression_statement[0]/call[0]",
                "a.ts#call[0]"
            ]
        );
        assert_eq!(view.files[0].blocks[3].parent, Some(0));
        assert!(view.files[0].slots.is_empty());
        let targets: Vec<&str> = view.files[0]
            .edges
            .iter()
            .filter_map(|edge| edge.target.as_ref().map(Key::as_str))
            .collect();
        assert_eq!(targets, ["a.ts#call[0]", "a.ts#call[0]"]);
        assert_eq!(view.files[0].edges[1].source, 3);
    }
}
