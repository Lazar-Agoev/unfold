//! The definition catalog (docs/reader.md#derived-views): every function-like, class, or type declaration
//! (`interface`, `type_alias`, `enum`) block that has a name or is the target of a resolved edge
//! or entrance, with its qualified name and display label.
//!
//! - The **label** is the block's name. An unnamed callable passed as an argument is labeled
//!   `callback to <callee>`, after the written callee of the call or construction it is passed
//!   to; any other unnamed callable is an `anonymous function`, and an unnamed class `class`.
//! - The **qualified name** prepends the names of enclosing classes and callables, and of a
//!   variable whose initializer contains the definition (`OrderService.place`,
//!   `handlers.onCreate`), joined with `.`. Type declarations are in the catalog but name
//!   nothing written inside them (a callback in an enum member's initializer).

use unfold_core::vocab::{BlockKind, EdgeKind, Role};
use unfold_core::{Block, FileGraph, Graph};
use unfold_store::Definition;

use crate::index::{BlockId, GraphIndex, to_u32};

/// Whether blocks of `kind` can be catalog definitions: function-like blocks, classes, and type
/// declarations.
fn is_definition_kind(kind: BlockKind) -> bool {
    kind.is_function_like() || kind.is_type_declaration()
}

/// Whether blocks of `kind` name the definitions written inside them: function-like blocks and
/// classes.
fn qualifies(kind: BlockKind) -> bool {
    kind.is_function_like() || kind == BlockKind::Class
}

/// One file's blocks and edges, for name lookups.
struct FileView<'a> {
    file: &'a FileGraph,
}

impl FileView<'_> {
    fn block(&self, local: u32) -> &Block {
        &self.file.blocks[local as usize]
    }

    fn parent(&self, local: u32) -> Option<u32> {
        self.block(local).parent.filter(|&parent| parent < local)
    }

    /// The written callee of a call or construction: the text of its first non-reference edge.
    fn callee_text(&self, local: u32) -> Option<&str> {
        let edges = &self.file.edges;
        let start = edges.partition_point(|edge| edge.source < local);
        edges[start..]
            .iter()
            .take_while(|edge| edge.source == local)
            .find(|edge| edge.kind != EdgeKind::References)
            .map(|edge| &*edge.text)
    }

    /// The block's display label.
    fn label(&self, local: u32) -> String {
        let block = self.block(local);
        if let Some(name) = &block.name {
            return name.to_string();
        }
        match block.kind {
            BlockKind::File => self.file.file.path.as_str().to_owned(),
            kind if kind.is_function_like() => self.callable_label(local),
            kind => kind.as_str().to_owned(),
        }
    }

    /// `callback to <callee>` when the unnamed callable is (inside) an argument of a call or
    /// construction before any enclosing definition or file; otherwise `anonymous function`.
    fn callable_label(&self, local: u32) -> String {
        let mut child = local;
        while let Some(parent) = self.parent(child) {
            let parent_kind = self.block(parent).kind;
            if qualifies(parent_kind) || parent_kind == BlockKind::File {
                break;
            }
            if matches!(parent_kind, BlockKind::Call | BlockKind::New)
                && self.block(child).role == Some(Role::Argument)
            {
                let callee = self.callee_text(parent).filter(|text| !text.is_empty());
                return format!("callback to {}", callee.unwrap_or("call"));
            }
            child = parent;
        }
        "anonymous function".to_owned()
    }

    /// The qualified name: enclosing class, callable, and initialized-variable names, then the
    /// label.
    fn qualified_name(&self, local: u32) -> String {
        let mut names: Vec<&str> = Vec::new();
        let mut child = local;
        while let Some(parent) = self.parent(child) {
            let block = self.block(parent);
            let named = qualifies(block.kind)
                || (block.kind == BlockKind::Variable && self.block(child).role == Some(Role::Initializer));
            if let Some(name) = block.name.as_deref().filter(|_| named) {
                names.push(name);
            }
            child = parent;
        }
        let label = self.label(local);
        let mut qualified =
            String::with_capacity(names.iter().map(|name| name.len() + 1).sum::<usize>() + label.len());
        for name in names.iter().rev() {
            qualified.push_str(name);
            qualified.push('.');
        }
        qualified.push_str(&label);
        qualified
    }
}

/// The qualified name of block `local` in `file`.
pub fn qualified_name(file: &FileGraph, local: u32) -> Option<String> {
    ((local as usize) < file.blocks.len()).then(|| FileView { file }.qualified_name(local))
}

/// The display label of block `local` in `file`.
pub fn label(file: &FileGraph, local: u32) -> Option<String> {
    ((local as usize) < file.blocks.len()).then(|| FileView { file }.label(local))
}

/// The catalog of every file, aligned with `graph.files`. `index` must be built from `graph`.
pub(crate) fn catalog(graph: &Graph, index: &GraphIndex) -> Vec<Vec<Definition>> {
    let mut targeted = vec![false; index.len()];
    let mut mark = |id: BlockId| {
        if let Some(slot) = targeted.get_mut(id as usize) {
            *slot = true;
        }
    };
    for edge in index.edges() {
        if let Some(target) = edge.resolved_target() {
            mark(target);
        }
    }
    for entrance in index.entrances() {
        entrance.resolved.iter().copied().for_each(&mut mark);
    }
    graph
        .files
        .iter()
        .enumerate()
        .map(|(file_index, file)| {
            let base = index.file_start(file_index);
            let view = FileView { file };
            file.blocks
                .iter()
                .enumerate()
                .filter(|(local, block)| {
                    is_definition_kind(block.kind)
                        && (block.name.is_some() || targeted[(base + to_u32(*local)) as usize])
                })
                .map(|(local, block)| {
                    let local = to_u32(local);
                    Definition {
                        key: block.key.clone(),
                        local_id: local,
                        kind: block.kind,
                        qualified_name: view.qualified_name(local).into(),
                        label: view.label(local).into(),
                    }
                })
                .collect()
        })
        .collect()
}
