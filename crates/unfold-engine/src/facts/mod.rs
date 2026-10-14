//! Per-file facts: what extraction produces and the facts cache stores.
//!
//! [`FileFacts`] has two parts. [`GraphFacts`] is language neutral: the file's node tree, its
//! unresolved links, and its typed records. The payload is opaque bytes that only the owning
//! language crate reads: what its binder recorded (scopes, declarations, writes, exports, class
//! shapes, census data, ...), aligned by index with the tables below. Neither part holds a
//! syntax tree, so the whole program can be relinked from cached facts without parsing
//! anything.
//!
//! The unresolved links are everything about a link except its outcome:
//!
//! | Link | Written as | Linking asks |
//! | --- | --- | --- |
//! | `calls` | one per [`CallFact`]: from its call node, at its callee value's range and label | [`Linked::callee_evidence`](crate::Linked::callee_evidence) |
//! | `passes` | an [`ArgumentFact::value`]: from the call, at the value's range and label, kept only when it reaches a project definition | [`Linked::value_evidence`](crate::Linked::value_evidence) |
//! | `imports` | one per [`ImportFact`]; a conditional one only if linking keeps it | [`Linked::module_dependencies`](crate::Linked::module_dependencies), [`Linked::keeps_import`](crate::Linked::keeps_import) |
//! | `types`, `extends` | one per [`TypeRefFact`] | [`Linked::type_evidence`](crate::Linked::type_evidence) |
//!
//! Facts serialize compactly with postcard ([`FileFacts::to_bytes`]). They are cached by path,
//! content hash, [`ENGINE_VERSION`], and settings hash.

mod typed;

use serde::{Deserialize, Serialize};
use unfold_core::text::{ByteRange, LABEL_MAX_UTF16, short_label};
use unfold_core::vocab::{Expansion, Form, NodeKind};
use unfold_core::{File, FileGraph, FileTypes, Node};

use crate::links::FileLinks;

pub use typed::TypeFacts;

/// The engine's identity in facts-cache keys. It changes whenever the facts format or anything
/// that shapes extracted facts changes, so stale cache entries never match.
pub const ENGINE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+facts.6");

/// The syntactic shape of a recorded value, after transparent wrappers (parentheses, type
/// assertions, `await`, ...) are unwrapped. Shapes decide the no-candidate reasons, which
/// arguments may pass a definition, and which access roots may be `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueShape {
    /// An identifier.
    Name,
    /// A property access, or an element access with a literal key.
    Member,
    /// Anything else: calls, literals, computed accesses, `this`, inline functions, ...
    Other,
}

impl ValueShape {
    /// Whether values of this shape can name a definition: identifiers and member accesses.
    pub const fn is_reference(self) -> bool {
        matches!(self, Self::Name | Self::Member)
    }
}

/// One expression the backend may ask about: a callee, a passed argument, or a receiver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFact {
    /// The shape of the unwrapped expression.
    pub shape: ValueShape,
    /// The short written label (see [`crate::links::text`]); at most 120 UTF-16 units. It is
    /// the text of the `calls` or `passes` link written at this value.
    pub label: Box<str>,
    /// Where the expression is written: the range of the `calls` or `passes` link written at
    /// this value. Token-exact and nonempty.
    pub range: ByteRange,
    /// For a property or element access, the receiver's index in the same value table. It must
    /// be smaller than this value's index, so receiver chains always end.
    pub receiver: Option<u32>,
    /// For a property access, the member name (the step provenance appends).
    pub member: Option<Box<str>>,
}

impl ValueFact {
    /// A value with no receiver or member.
    pub fn new(shape: ValueShape, label: &str, range: ByteRange) -> Self {
        Self {
            shape,
            label: short_label(label).into(),
            range,
            receiver: None,
            member: None,
        }
    }
}

/// Whether a call selects a call target or a construction target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallContext {
    /// An ordinary call or tagged template.
    Call,
    /// A construction (`new X()`), or a TypeScript `super(...)` call, which keeps its `call`
    /// form but selects a construction target.
    Construct,
}

/// One argument of a call, in written order. It becomes the call's [`unfold_core::Argument`]
/// record, whose index is its position in [`CallFact::arguments`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgumentFact {
    /// The whole argument as written (a Python keyword argument includes its keyword), inside
    /// the call's range.
    pub range: ByteRange,
    /// The argument's value in the value table when it may pass a definition: an identifier or
    /// a member access, not expanded. Linking writes a `passes` link at it when it resolves to
    /// a project function or class. `None` for every other argument.
    pub value: Option<u32>,
    /// How it is expanded (`...x`, `*x`, `**x`), if it is.
    pub expand: Option<Expansion>,
    /// A Python keyword argument's name.
    pub keyword: Option<Box<str>>,
}

/// One call node's facts: what its `calls` link resolves, and its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFact {
    /// The call node's local ID.
    pub node: u32,
    /// The callee's index in the value table. Every call has one: a dynamic `import(x)` records
    /// its `import` keyword, a tagged template its tag.
    pub callee: u32,
    /// Which target the policy selects. A language whose linking proves more (a Python call of
    /// a proven class) says so with [`Linked::constructs`](crate::Linked::constructs).
    pub context: CallContext,
    /// The arguments, in written order. Tagged templates have none.
    pub arguments: Vec<ArgumentFact>,
}

/// One written runtime module dependency: an `imports` link before linking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFact {
    /// The node the link starts at: the definition the import is written in (the module, a
    /// class, or a function), or a dynamic-import call.
    pub from: u32,
    /// The specifier as written (a non-literal dynamic import's argument): the link's range,
    /// and the range the language's census reports for it.
    pub range: ByteRange,
    /// The decoded literal specifier, which is the link's text; `None` for a non-literal
    /// dynamic import, whose text is `(…)`.
    pub specifier: Option<Box<str>>,
    /// Whether the link exists only if linking keeps it ([`Linked::keeps_import`]), because that
    /// depends on other files: a Python named child import (`from pkg import child`) is a
    /// dependency of its own only when `child` is proven absent from `pkg`'s namespace.
    ///
    /// [`Linked::keeps_import`]: crate::Linked::keeps_import
    pub conditional: bool,
}

/// What a written type name is used for, which decides its link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeUse {
    /// Named in an annotation or a handler clause: a `types` link from the nearest definition,
    /// or from the `except` or `catch` part whose clause names it.
    Annotation,
    /// A base the class or type extends: an `extends` link from it.
    Extends,
    /// An interface a TypeScript class implements: an `extends` link flagged `implements`.
    Implements,
}

/// One written type name: a `types` or `extends` link before linking. Its language resolves it
/// with [`Linked::type_evidence`](crate::Linked::type_evidence), from what its payload records
/// at the same index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRefFact {
    /// The node the link starts at: a definition or an `except`/`catch` part for an annotation,
    /// a class or type for a base.
    pub from: u32,
    /// The written name's range: `Cart`, or a qualified name whole (`Stripe.PaymentIntent`),
    /// without type arguments.
    pub range: ByteRange,
    /// The name as written, cut to 120 UTF-16 units: the link's text.
    pub text: Box<str>,
    /// Whether it is a qualified name (`a.B`), whose root is a namespace or module.
    pub qualified: bool,
    /// What it is used for.
    pub usage: TypeUse,
}

impl TypeRefFact {
    /// A type name at `range`, used as `usage`, with its link starting at node `from`.
    pub fn new(from: u32, range: ByteRange, text: &str, qualified: bool, usage: TypeUse) -> Self {
        Self {
            from,
            range,
            text: short_label(text).into(),
            qualified,
            usage,
        }
    }
}

/// Why extracted facts are inconsistent. These are bugs in a language crate, reported instead of
/// producing a broken graph.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactsError {
    /// The nodes are not a preorder tree under one module.
    #[error("node {node} is not in preorder under the file's module")]
    Tree {
        /// The offending node.
        node: u32,
    },
    /// A call node has no call fact or several, a call fact names another node, or the facts
    /// are not in node order.
    #[error("call node {node} needs exactly one call fact, in node order")]
    Call {
        /// The node.
        node: u32,
    },
    /// A value index is out of range, or a receiver does not precede its value.
    #[error("value {value} is out of range or has an invalid receiver")]
    Value {
        /// The offending value index.
        value: u32,
    },
    /// An import starts at a node that can't import, or has an empty range.
    #[error("import {import} starts at node {from}, which is not a module, class, function, or call")]
    Import {
        /// The import's index.
        import: u32,
        /// The node it starts at.
        from: u32,
    },
    /// A type name starts at a node its use doesn't allow, or has an empty range.
    #[error("type name {index} starts at node {from}, which its use doesn't allow")]
    TypeRef {
        /// The type name's index.
        index: u32,
        /// The node it starts at.
        from: u32,
    },
    /// A typed record names a node of the wrong kind, or is out of order.
    #[error("{what} for node {node} does not fit the file's nodes")]
    Typed {
        /// The kind of record.
        what: &'static str,
        /// The node it names.
        node: u32,
    },
    /// The cached bytes could not be decoded.
    #[error("cannot decode file facts: {0}")]
    Decode(String),
    /// The owning language can't read the cached payload
    /// ([`Language::reads_payload`](crate::Language::reads_payload)).
    #[error("the language cannot read the cached payload")]
    Payload,
    /// The facts could not be encoded.
    #[error("cannot encode file facts: {0}")]
    Encode(String),
}

/// The language-neutral part of one file's facts.
///
/// Invariants (checked by [`GraphFacts::new`] and again when decoding): the nodes are a preorder
/// tree under the module; the call facts pair one-to-one with the call nodes, in node order;
/// every value index is in range and receivers precede their values; imports and type names
/// start at nodes that may source their links; typed records name nodes of their kind, in
/// canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFacts {
    nodes: Vec<Node>,
    values: Vec<ValueFact>,
    calls: Vec<CallFact>,
    imports: Vec<ImportFact>,
    type_refs: Vec<TypeRefFact>,
    types: TypeFacts,
}

impl GraphFacts {
    /// One file's facts from its lowering:
    ///
    /// - `nodes`: the node tree in preorder, the module first, each keyed with
    ///   [`unfold_core::KeyBuilder`] and carrying every field the front end knows (Python calls
    ///   are all `call`; linking may make one `new`);
    /// - `values`: the value table, receivers before the accesses that use them;
    /// - `calls`: one per call node, in node order;
    /// - `imports` and `type_refs`: in the language's order, which its payload and
    ///   [`Linked::module_dependencies`](crate::Linked::module_dependencies) are aligned with;
    /// - `types`: signatures, parameters, and members ([`TypeFacts::new`]).
    ///
    /// Labels and texts longer than 120 UTF-16 units are cut.
    pub fn new(
        nodes: Vec<Node>,
        mut values: Vec<ValueFact>,
        calls: Vec<CallFact>,
        mut imports: Vec<ImportFact>,
        mut type_refs: Vec<TypeRefFact>,
        types: TypeFacts,
    ) -> Result<Self, FactsError> {
        for value in &mut values {
            cut(&mut value.label);
        }
        for import in &mut imports {
            if let Some(specifier) = &mut import.specifier {
                cut(specifier);
            }
        }
        for reference in &mut type_refs {
            cut(&mut reference.text);
        }
        let facts = Self {
            nodes,
            values,
            calls,
            imports,
            type_refs,
            types,
        };
        facts.check()?;
        Ok(facts)
    }

    /// The nodes in preorder; the module is 0.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The value table.
    pub fn values(&self) -> &[ValueFact] {
        &self.values
    }

    /// One call fact per call node, in node order.
    pub fn calls(&self) -> &[CallFact] {
        &self.calls
    }

    /// The written module dependencies, in the language's order.
    pub fn imports(&self) -> &[ImportFact] {
        &self.imports
    }

    /// The written type names, in the language's order.
    pub fn type_refs(&self) -> &[TypeRefFact] {
        &self.type_refs
    }

    /// Signatures, parameters, and members.
    pub fn types(&self) -> &TypeFacts {
        &self.types
    }

    /// This file's graph record: its nodes (with the call nodes linking proved to be
    /// constructions given the form `new`), its links, and its typed records with the arguments
    /// linking paired.
    pub fn file_graph(&self, file: File, links: FileLinks) -> FileGraph {
        let mut nodes = self.nodes.clone();
        for construction in links.constructions {
            if let Some(node) = nodes.get_mut(construction as usize) {
                node.form = Form::New;
            }
        }
        let mut types = FileTypes {
            signatures: self.types.signatures.clone(),
            parameters: self.types.parameters.clone(),
            members: self.types.members.clone(),
            arguments: links.arguments,
        };
        types.sort();
        FileGraph {
            file,
            nodes,
            links: links.links,
            types,
        }
    }

    fn check(&self) -> Result<(), FactsError> {
        self.check_tree()?;
        let kind = |node: u32| self.nodes.get(node as usize).map(Node::kind);
        let value_count = self.values.len();
        for (index, value) in self.values.iter().enumerate() {
            let index = index as u32;
            if value.receiver.is_some_and(|receiver| receiver >= index) {
                return Err(FactsError::Value { value: index });
            }
        }
        let in_range = |value: u32| (value as usize) < value_count;
        let mut calls = self.calls.iter();
        for (index, node) in self.nodes.iter().enumerate() {
            if node.kind() != NodeKind::Call {
                continue;
            }
            let node = index as u32;
            let Some(call) = calls.next().filter(|call| call.node == node) else {
                return Err(FactsError::Call { node });
            };
            if !in_range(call.callee) {
                return Err(FactsError::Value { value: call.callee });
            }
            if let Some(value) = call
                .arguments
                .iter()
                .find_map(|argument| argument.value.filter(|&value| !in_range(value)))
            {
                return Err(FactsError::Value { value });
            }
        }
        if let Some(call) = calls.next() {
            return Err(FactsError::Call { node: call.node });
        }
        for (index, import) in self.imports.iter().enumerate() {
            let fits = matches!(
                kind(import.from),
                Some(NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Call)
            );
            if !fits || import.range.is_empty() {
                return Err(FactsError::Import {
                    import: index as u32,
                    from: import.from,
                });
            }
        }
        for (index, reference) in self.type_refs.iter().enumerate() {
            let from = self.nodes.get(reference.from as usize);
            let fits = from.is_some_and(|from| match reference.usage {
                TypeUse::Annotation => {
                    from.kind().is_definition() || matches!(from.form, Form::Except | Form::Catch)
                }
                TypeUse::Extends | TypeUse::Implements => {
                    matches!(from.kind(), NodeKind::Class | NodeKind::Type)
                }
            });
            if !fits || reference.range.is_empty() {
                return Err(FactsError::TypeRef {
                    index: index as u32,
                    from: reference.from,
                });
            }
        }
        self.types.check(&self.nodes)
    }

    /// The module first, with no parent, and every later node under an earlier one.
    fn check_tree(&self) -> Result<(), FactsError> {
        let Some(module) = self.nodes.first() else {
            return Err(FactsError::Tree { node: 0 });
        };
        if module.form != Form::Module || module.parent.is_some() {
            return Err(FactsError::Tree { node: 0 });
        }
        for (index, node) in self.nodes.iter().enumerate().skip(1) {
            let index = index as u32;
            if node.form == Form::Module || node.parent.is_none_or(|parent| parent >= index) {
                return Err(FactsError::Tree { node: index });
            }
        }
        Ok(())
    }
}

/// Cuts `text` to 120 UTF-16 units.
fn cut(text: &mut Box<str>) {
    if text.len() > LABEL_MAX_UTF16 {
        *text = short_label(text).into();
    }
}

/// Everything extraction produces for one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFacts {
    /// The language-neutral part.
    pub graph: GraphFacts,
    /// The binder payload, in a format only the owning language crate reads.
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
        facts.graph.check()?;
        Ok(facts)
    }
}

#[cfg(test)]
mod tests;
