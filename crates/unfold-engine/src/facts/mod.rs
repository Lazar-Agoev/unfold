//! Per-file facts: what extraction produces and the facts cache stores.
//!
//! [`FileFacts`] has two parts. [`GraphFacts`] is language neutral: the blocks and slots of the
//! shared selection, the file's value table, its call and import occurrences, its typed facts
//! ([`TypeFacts`]: signatures, parameters, members, call results, and written type names), and
//! the `import` blocks that exist only if linking keeps them ([`GraphFacts::conditional`]). The payload is
//! opaque bytes that only the owning language crate reads (its local bindings, declarations,
//! exports, class shapes, census data, ...). Neither part holds a syntax tree, so the whole
//! program can be relinked from cached facts without parsing anything (architecture §6.2, §7.3).
//!
//! Facts serialize compactly with postcard ([`FileFacts::to_bytes`]). They are cached by path,
//! content hash, [`ENGINE_VERSION`], and settings hash.

use serde::{Deserialize, Serialize};
use unfold_core::text::short_label;
use unfold_core::vocab::{BlockKind, EdgeKind, Expansion};
use unfold_core::{Block, FileGraph, FileRecord, FileTypes, Slot};

use crate::edges::FileLinks;
use crate::select::Selection;

mod conditional;
mod typed;

pub use typed::{TypeFacts, TypeRefFact, innermost};

/// The engine's identity in facts-cache keys. It changes whenever the facts format or anything
/// that shapes extracted facts changes, so stale cache entries never match.
pub const ENGINE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+facts.3");

/// The syntactic shape of a recorded value, after transparent wrappers (parentheses, type
/// assertions, ...) are unwrapped. Shapes drive the shared policy's no-candidate reasons,
/// reference eligibility, and handler checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueShape {
    /// An identifier.
    Name,
    /// A property access, or an element access with a literal key.
    Member,
    /// A call expression used as a value.
    CallResult,
    /// An inline function or arrow function.
    InlineFunction,
    /// An inline class expression.
    InlineClass,
    /// Anything else: literals, computed accesses, `this`, conditionals, ...
    Other,
}

impl ValueShape {
    /// Whether values of this shape can supply a named reference: identifiers and member
    /// accesses. Inline functions and classes are child blocks instead.
    pub const fn is_reference(self) -> bool {
        matches!(self, Self::Name | Self::Member)
    }
}

/// One expression the backend may ask about: a callee, an argument, a handler, a receiver, or
/// a factory callee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFact {
    /// The shape of the unwrapped expression.
    pub shape: ValueShape,
    /// The short written label (see [`crate::edges::text`]); at most 120 UTF-16 units.
    pub label: Box<str>,
    /// For a property or element access, the receiver's index in the same value table. It must
    /// be smaller than this value's index, so receiver chains always end.
    pub receiver: Option<u32>,
    /// For a property access, the member name (the step provenance appends).
    pub member: Option<Box<str>>,
    /// For an inline function or class, its block's local ID.
    pub block: Option<u32>,
}

impl ValueFact {
    /// A value with no receiver, member, or block.
    pub fn new(shape: ValueShape, label: &str) -> Self {
        Self {
            shape,
            label: short_label(label).into(),
            receiver: None,
            member: None,
            block: None,
        }
    }
}

/// Whether an invocation selects a call target or a construction target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallContext {
    /// An ordinary call or tagged template.
    Call,
    /// A construction, or a TypeScript `super(...)` call, which keeps its `call` block and
    /// `calls` edge but selects a construction target.
    Construct,
}

/// How an invocation is resolved: its occurrence edge kind and the context its target is
/// selected in. Extraction records one in every [`CallFact`]; a language whose linking proves
/// more (a Python callee with class identity) may override it with
/// [`Linked::invocation`](crate::Linked::invocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Invocation {
    /// The occurrence edge kind: `calls` or `constructs`.
    pub edge: EdgeKind,
    /// Which target the policy selects.
    pub context: CallContext,
}

/// One argument of a call or construction, aligned with the block's `argument` slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgumentFact {
    /// The argument's index in the value table.
    pub value: u32,
    /// Its zero-based written position (Python keywords and expansions keep their ordinal),
    /// which is its `argument` slot's index.
    pub position: u32,
    /// Whether it can supply a named reference; `false` for expansions such as `*args`.
    pub reference: bool,
    /// How it is expanded (`...x`, `*x`, `**x`), if it is.
    pub expand: Option<Expansion>,
    /// A Python keyword argument's name.
    pub keyword: Option<Box<str>>,
}

impl ArgumentFact {
    /// A plain positional argument at `position`.
    pub const fn positional(value: u32, position: u32, reference: bool) -> Self {
        Self {
            value,
            position,
            reference,
            expand: None,
            keyword: None,
        }
    }
}

/// One call, tagged template, or construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFact {
    /// The `call` or `new` block's local ID.
    pub block: u32,
    /// The callee's index in the value table; `None` when there is no callee expression, which
    /// labels the edge `(…)`.
    pub callee: Option<u32>,
    /// The occurrence edge kind: `calls`, or `constructs` for `new` (and a Python call with
    /// proven class identity).
    pub edge: EdgeKind,
    /// Which target the policy selects.
    pub context: CallContext,
    /// The arguments in written order. Tagged templates have none.
    pub arguments: Vec<ArgumentFact>,
}

impl CallFact {
    /// The invocation extraction recorded.
    pub const fn invocation(&self) -> Invocation {
        Invocation {
            edge: self.edge,
            context: self.context,
        }
    }
}

/// One runtime module dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFact {
    /// The `import` block's local ID.
    pub block: u32,
    /// The decoded literal specifier; `None` for a non-literal dynamic import.
    pub specifier: Option<Box<str>>,
}

/// Why extracted facts are inconsistent. These are bugs in a language crate, reported instead of
/// producing a broken graph.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactsError {
    /// A call fact names a block that is not a `call` or `new`.
    #[error("call fact names block {block}, which is not a call or new block")]
    NotACall {
        /// The named block.
        block: u32,
    },
    /// An import fact names a block that is not an `import`.
    #[error("import fact names block {block}, which is not an import block")]
    NotAnImport {
        /// The named block.
        block: u32,
    },
    /// An occurrence block has no fact, or more than one.
    #[error("occurrence block {block} needs exactly one occurrence fact")]
    Occurrence {
        /// The block.
        block: u32,
    },
    /// A call's edge kind does not fit its block kind.
    #[error("call block {block} cannot source a {edge} occurrence")]
    EdgeKind {
        /// The block.
        block: u32,
        /// The edge kind.
        edge: EdgeKind,
    },
    /// A call's arguments are not in strictly increasing position order.
    #[error("the arguments of call block {block} are not in written order")]
    ArgumentOrder {
        /// The block.
        block: u32,
    },
    /// A value index is out of range, or a receiver does not precede its value.
    #[error("value {value} is out of range or has an invalid receiver")]
    Value {
        /// The offending value index.
        value: u32,
    },
    /// An inline value's block is missing or is not a function or class.
    #[error("value {value} names block {block}, which is not an inline function or class")]
    InlineBlock {
        /// The value.
        value: u32,
        /// The named block.
        block: u32,
    },
    /// A conditional block is not a leaf `import` block, or the list is not sorted and
    /// distinct.
    #[error("conditional block {block} is not a distinct leaf import block in order")]
    Conditional {
        /// The offending block.
        block: u32,
    },
    /// Renumbering after dropping conditional blocks produced an invalid block tree.
    #[error("cannot renumber blocks: {0}")]
    Renumber(String),
    /// A typed fact names a block of the wrong kind, is out of order, or names no slot.
    #[error("{what} fact for block {block} does not fit the file's blocks")]
    Typed {
        /// The kind of fact.
        what: &'static str,
        /// The block it names.
        block: u32,
    },
    /// The cached bytes could not be decoded.
    #[error("cannot decode file facts: {0}")]
    Decode(String),
    /// The facts could not be encoded.
    #[error("cannot encode file facts: {0}")]
    Encode(String),
}

/// The language-neutral part of one file's facts.
///
/// Invariants (checked by [`GraphFacts::new`] and again when decoding): blocks are a valid
/// preorder selection; calls and imports are sorted by block and pair one-to-one with the
/// `call`/`new` and `import` blocks; every value index is in range; receivers precede their
/// values; inline values name function or class blocks; conditional blocks are distinct leaf
/// `import` blocks, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFacts {
    blocks: Vec<Block>,
    slots: Vec<Slot>,
    values: Vec<ValueFact>,
    calls: Vec<CallFact>,
    imports: Vec<ImportFact>,
    /// Import blocks that exist only if linking keeps them (see [`GraphFacts::conditional`]).
    conditional: Vec<u32>,
    /// Signatures, parameters, members, call results, and written type names.
    types: TypeFacts,
}

impl GraphFacts {
    /// Combines a selection with the occurrences and values recorded during the same walk.
    /// Calls and imports may be recorded in any order; they are sorted by block. Labels longer
    /// than 120 UTF-16 units are truncated.
    pub fn new(
        selection: Selection,
        mut values: Vec<ValueFact>,
        mut calls: Vec<CallFact>,
        mut imports: Vec<ImportFact>,
    ) -> Result<Self, FactsError> {
        for value in &mut values {
            if value.label.len() > unfold_core::text::LABEL_MAX_UTF16 {
                value.label = short_label(&value.label).into();
            }
        }
        calls.sort_by_key(|call| call.block);
        imports.sort_by_key(|import| import.block);
        let facts = Self {
            blocks: selection.blocks,
            slots: selection.slots,
            values,
            calls,
            imports,
            conditional: Vec::new(),
            types: TypeFacts::default(),
        };
        facts.check()?;
        Ok(facts)
    }

    /// Adds the file's typed facts (see [`TypeFacts::new`], which sets each type reference's
    /// block from these facts' blocks).
    pub fn with_types(mut self, types: TypeFacts) -> Result<Self, FactsError> {
        self.types = types;
        self.check()?;
        Ok(self)
    }

    /// The file's typed facts.
    pub fn types(&self) -> &TypeFacts {
        &self.types
    }

    /// Marks `blocks` as conditional: tentatively selected `import` blocks that stay in the
    /// graph only when the owning language's linking keeps them
    /// ([`Linked::keeps_import`](crate::Linked::keeps_import)), because whether they exist
    /// depends on other files. Each must be a leaf `import` block (it has an
    /// [`ImportFact`] like any other); the list may be in any order.
    pub fn with_conditional(mut self, mut blocks: Vec<u32>) -> Result<Self, FactsError> {
        blocks.sort_unstable();
        self.conditional = blocks;
        self.check()?;
        Ok(self)
    }

    /// The conditional `import` blocks, in block order (see [`GraphFacts::with_conditional`]).
    /// The facts cache stores them as extracted; which of them a graph keeps is decided again
    /// at every link.
    pub fn conditional(&self) -> &[u32] {
        &self.conditional
    }

    /// Blocks in preorder; `blocks[i].local_id == i`.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Slots ordered by block, then start.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The value table.
    pub fn values(&self) -> &[ValueFact] {
        &self.values
    }

    /// Calls and constructions, in block order.
    pub fn calls(&self) -> &[CallFact] {
        &self.calls
    }

    /// Runtime imports, in block order.
    pub fn imports(&self) -> &[ImportFact] {
        &self.imports
    }

    /// This file's graph record: its blocks, slots, and typed facts with `links` (its edges,
    /// argument pairings, and type links), which the backend resolved.
    pub fn file_graph(&self, file: FileRecord, links: FileLinks) -> FileGraph {
        FileGraph {
            file,
            blocks: self.blocks.clone(),
            slots: self.slots.clone(),
            edges: links.edges,
            types: self.file_types(links.arguments, links.type_links, &links.bounds),
        }
    }

    /// The file's typed records: its typed facts with the arguments, type links, and the calls'
    /// bound counts resolved at link time.
    fn file_types(
        &self,
        arguments: Vec<unfold_core::Argument>,
        type_links: Vec<unfold_core::TypeLink>,
        bounds: &[(u32, u32)],
    ) -> FileTypes {
        let mut types = FileTypes {
            signatures: self.types.signatures.clone(),
            parameters: self.types.parameters.clone(),
            members: self.types.members.clone(),
            arguments,
            results: self.types.results.clone(),
            type_links,
        };
        types.sort();
        for &(call, bound) in bounds {
            if let Ok(index) = types.results.binary_search_by_key(&call, |result| result.call) {
                types.results[index].bound = Some(bound);
            }
        }
        types
    }

    fn check(&self) -> Result<(), FactsError> {
        let value_count = self.values.len();
        for (index, value) in self.values.iter().enumerate() {
            let index = index as u32;
            if value.receiver.is_some_and(|receiver| receiver >= index) {
                return Err(FactsError::Value { value: index });
            }
            if let Some(block) = value.block {
                let fits = match (value.shape, self.blocks.get(block as usize)) {
                    (ValueShape::InlineFunction, Some(found)) => found.kind.is_function_like(),
                    (ValueShape::InlineClass, Some(found)) => found.kind == BlockKind::Class,
                    _ => false,
                };
                if !fits {
                    return Err(FactsError::InlineBlock { value: index, block });
                }
            }
        }
        let in_range = |value: u32| (value as usize) < value_count;
        let mut calls = self.calls.iter().peekable();
        let mut imports = self.imports.iter().peekable();
        for block in &self.blocks {
            let local = block.local_id;
            let call_facts = count_for(&mut calls, |call| call.block, local);
            let import_facts = count_for(&mut imports, |import| import.block, local);
            let expected = match block.kind {
                BlockKind::Call | BlockKind::New => (1, 0),
                BlockKind::Import => (0, 1),
                _ => (0, 0),
            };
            if (call_facts, import_facts) != expected {
                return Err(match (call_facts, import_facts) {
                    (1.., _) if expected.0 == 0 => FactsError::NotACall { block: local },
                    (_, 1..) if expected.1 == 0 => FactsError::NotAnImport { block: local },
                    _ => FactsError::Occurrence { block: local },
                });
            }
        }
        if let Some(call) = calls.next() {
            return Err(FactsError::NotACall { block: call.block });
        }
        if let Some(import) = imports.next() {
            return Err(FactsError::NotAnImport { block: import.block });
        }
        self.check_conditional()?;
        self.types.check(&self.blocks, &self.slots)?;
        for call in &self.calls {
            let kind = self.blocks[call.block as usize].kind;
            let allowed = match kind {
                BlockKind::New => call.edge == EdgeKind::Constructs,
                _ => matches!(call.edge, EdgeKind::Calls | EdgeKind::Constructs),
            };
            if !allowed {
                return Err(FactsError::EdgeKind {
                    block: call.block,
                    edge: call.edge,
                });
            }
            if let Some(callee) = call.callee.filter(|&callee| !in_range(callee)) {
                return Err(FactsError::Value { value: callee });
            }
            if let Some(argument) = call.arguments.iter().find(|argument| !in_range(argument.value)) {
                return Err(FactsError::Value {
                    value: argument.value,
                });
            }
            if call
                .arguments
                .windows(2)
                .any(|pair| pair[0].position >= pair[1].position)
            {
                return Err(FactsError::ArgumentOrder { block: call.block });
            }
        }
        Ok(())
    }
}

/// Advances `items` past every entry for `local` (they are sorted by block) and counts them.
fn count_for<'a, T: 'a>(
    items: &mut std::iter::Peekable<impl Iterator<Item = &'a T>>,
    block_of: impl Fn(&T) -> u32,
    local: u32,
) -> usize {
    let mut count = 0;
    while items.next_if(|item| block_of(item) == local).is_some() {
        count += 1;
    }
    count
}

/// Everything extraction produces for one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFacts {
    /// The language-neutral part.
    pub graph: GraphFacts,
    /// The language's private part, in a format only the owning language crate reads.
    pub payload: Box<[u8]>,
}

impl FileFacts {
    /// Facts from their two parts.
    pub fn new(graph: GraphFacts, payload: Vec<u8>) -> Self {
        Self {
            graph,
            payload: payload.into_boxed_slice(),
        }
    }

    /// The compact cache encoding.
    pub fn to_bytes(&self) -> Result<Vec<u8>, FactsError> {
        postcard::to_allocvec(self).map_err(|error| FactsError::Encode(error.to_string()))
    }

    /// Decodes cached facts and re-checks their invariants, so a corrupt cache entry is
    /// reported rather than trusted.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FactsError> {
        let facts: Self =
            postcard::from_bytes(bytes).map_err(|error| FactsError::Decode(error.to_string()))?;
        facts.graph.check_decoded()?;
        Ok(facts)
    }
}

impl GraphFacts {
    /// The checks [`GraphFacts::new`] makes, plus the selection's own shape (which a fresh
    /// selection guarantees by construction): local IDs, parents, and slot owners.
    fn check_decoded(&self) -> Result<(), FactsError> {
        let corrupt = || FactsError::Decode("blocks or slots are not a valid selection".into());
        for (index, block) in self.blocks.iter().enumerate() {
            let index = index as u32;
            let parent_ok = match block.parent {
                None => index == 0 && block.kind == BlockKind::File,
                Some(parent) => parent < index && block.kind != BlockKind::File,
            };
            if block.local_id != index || !parent_ok {
                return Err(corrupt());
            }
        }
        if self
            .slots
            .iter()
            .any(|slot| slot.block as usize >= self.blocks.len())
        {
            return Err(corrupt());
        }
        if self.conditional.windows(2).any(|pair| pair[0] >= pair[1])
            || self.calls.windows(2).any(|pair| pair[0].block >= pair[1].block)
            || self.imports.windows(2).any(|pair| pair[0].block >= pair[1].block)
        {
            return Err(corrupt());
        }
        self.check()
    }
}

#[cfg(test)]
mod tests {
    use unfold_core::RepoPath;
    use unfold_core::text::{ByteRange, LineIndex};
    use unfold_core::vocab::Role;

    use super::*;
    use crate::select::{MappedSlot, Mapping, Saved, Tree, walk};

    /// `file > call > new`, plus an import and a function, as an arena of `(kind, children)`.
    struct Arena(Vec<(BlockKind, Vec<usize>)>);

    impl Mapping for Arena {
        type Node = usize;
        type Slot = ();

        fn range(&self, node: usize) -> ByteRange {
            ByteRange::new(node as u32, node as u32 + 1).unwrap()
        }

        fn kind(&self, node: usize) -> Option<BlockKind> {
            Some(self.0[node].0)
        }

        fn name(&self, _node: usize, _kind: BlockKind) -> Option<Box<str>> {
            None
        }

        fn role(&self, _parent: Saved<usize>, _slot: Option<()>, _decorator: bool) -> Option<Role> {
            Some(Role::Body)
        }

        fn slots(&self, _node: usize, _kind: BlockKind, _out: &mut Vec<MappedSlot>) {}
    }

    impl Tree for Arena {
        fn children(&self, node: usize, out: &mut Vec<((), usize)>) {
            out.extend(self.0[node].1.iter().map(|&child| ((), child)));
        }
    }

    fn selection() -> Selection {
        let arena = Arena(vec![
            (BlockKind::File, vec![1, 3, 4]),
            (BlockKind::Call, vec![2]),
            (BlockKind::New, vec![]),
            (BlockKind::Import, vec![]),
            (BlockKind::Function, vec![]),
        ]);
        let lines = LineIndex::new("0123456789");
        walk(&arena, &RepoPath::new("a.x").unwrap(), &lines, 0, |_, _| {}).unwrap()
    }

    fn call(block: u32, edge: EdgeKind, arguments: Vec<ArgumentFact>) -> CallFact {
        CallFact {
            block,
            callee: Some(0),
            edge,
            context: CallContext::Call,
            arguments,
        }
    }

    fn values() -> Vec<ValueFact> {
        let mut inline = ValueFact::new(ValueShape::InlineFunction, "(…)");
        inline.block = Some(4);
        let mut member = ValueFact::new(ValueShape::Member, &format!("{}.x", "y".repeat(200)));
        member.receiver = Some(0);
        vec![ValueFact::new(ValueShape::Name, "f"), inline, member]
    }

    fn import() -> Vec<ImportFact> {
        vec![ImportFact {
            block: 3,
            specifier: Some("pkg".into()),
        }]
    }

    fn valid() -> GraphFacts {
        // Out of block order on purpose: facts sort calls by block.
        let calls = vec![
            call(2, EdgeKind::Constructs, vec![]),
            call(1, EdgeKind::Calls, vec![ArgumentFact::positional(1, 0, true)]),
        ];
        GraphFacts::new(selection(), values(), calls, import()).unwrap()
    }

    #[test]
    fn facts_sort_occurrences_truncate_labels_and_round_trip() {
        let facts = valid();
        assert_eq!(
            facts.calls().iter().map(|call| call.block).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(unfold_core::text::utf16_len(&facts.values()[2].label), 120);
        let file = FileFacts::new(facts, vec![1, 2, 3]);
        let bytes = file.to_bytes().unwrap();
        assert_eq!(FileFacts::from_bytes(&bytes).unwrap(), file);
        assert!(matches!(
            FileFacts::from_bytes(&bytes[..bytes.len() / 2]),
            Err(FactsError::Decode(_))
        ));
        assert!(!ENGINE_VERSION.is_empty());
    }

    #[test]
    fn inconsistent_facts_are_rejected() {
        let new = |values, calls, imports| GraphFacts::new(selection(), values, calls, imports);
        let both = |extra: CallFact| {
            vec![
                call(1, EdgeKind::Calls, vec![]),
                call(2, EdgeKind::Constructs, vec![]),
                extra,
            ]
        };
        assert_eq!(
            new(values(), both(call(4, EdgeKind::Calls, vec![])), import()),
            Err(FactsError::NotACall { block: 4 })
        );
        assert_eq!(
            new(values(), vec![call(1, EdgeKind::Calls, vec![])], import()),
            Err(FactsError::Occurrence { block: 2 })
        );
        assert_eq!(
            new(values(), both(call(9, EdgeKind::Calls, vec![])), import()),
            Err(FactsError::NotACall { block: 9 })
        );
        assert_eq!(
            new(
                values(),
                vec![call(1, EdgeKind::Calls, vec![]), call(2, EdgeKind::Calls, vec![])],
                import()
            ),
            Err(FactsError::EdgeKind {
                block: 2,
                edge: EdgeKind::Calls
            })
        );
        assert_eq!(
            new(
                values(),
                vec![
                    call(1, EdgeKind::Calls, vec![]),
                    call(2, EdgeKind::Constructs, vec![])
                ],
                Vec::new()
            ),
            Err(FactsError::Occurrence { block: 3 })
        );
        let argument = |position| ArgumentFact::positional(0, position, true);
        assert_eq!(
            new(
                values(),
                vec![
                    call(1, EdgeKind::Calls, vec![argument(1), argument(1)]),
                    call(2, EdgeKind::Constructs, vec![])
                ],
                import()
            ),
            Err(FactsError::ArgumentOrder { block: 1 })
        );
        let mut bad = values();
        bad[0].receiver = Some(0);
        assert_eq!(
            new(
                bad,
                vec![
                    call(1, EdgeKind::Calls, vec![]),
                    call(2, EdgeKind::Constructs, vec![])
                ],
                import()
            ),
            Err(FactsError::Value { value: 0 })
        );
        let mut bad = values();
        bad[1].block = Some(2);
        assert_eq!(
            new(
                bad,
                vec![
                    call(1, EdgeKind::Calls, vec![]),
                    call(2, EdgeKind::Constructs, vec![])
                ],
                import()
            ),
            Err(FactsError::InlineBlock { value: 1, block: 2 })
        );
    }
}
