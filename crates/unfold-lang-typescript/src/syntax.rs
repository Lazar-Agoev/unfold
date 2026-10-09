//! The normalized syntax tree that block selection walks.
//!
//! oxc's AST is converted once per file into a flat arena of [`Node`]s that follows the proof of
//! concept's normalized TypeScript syntax: each node has a category, a token-exact range, and
//! children labeled with the child slot they fill in their parent (`statements`, `expression`,
//! `thenStatement`, ...). Roles, slots, and names are all derived from categories and labels, so
//! the mapping never needs to know oxc's node types.
//!
//! Only what selection, naming, values, and imports read is kept: type syntax becomes a single
//! childless [`Cat::Type`] node with its range, and syntax with no graph meaning becomes
//! [`Cat::Other`] (whose runtime descendants are still present).

use oxc_allocator::Address;
use unfold_core::text::ByteRange;

/// The absent node index (no parent, child, or sibling).
pub(crate) const NONE: u32 = u32::MAX;

/// A node's syntactic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cat {
    SourceFile,
    FunctionDeclaration,
    FunctionExpression,
    ArrowFunction,
    ClassDeclaration,
    ClassExpression,
    Method,
    Constructor,
    Getter,
    Setter,
    /// A class property or auto-accessor declaration.
    Property,
    /// An object literal's `key: value` property.
    PropertyAssignment,
    VariableDeclaration,
    /// One element of a binding pattern (object property, array element, or rest).
    BindingElement,
    /// An object or array binding pattern.
    BindingPattern,
    Parameter,
    /// `export default <expression>` or `export = <expression>`.
    ExportAssignment,
    /// An `import` declaration.
    ImportDeclaration,
    /// An `export … from` or `export * from` declaration.
    Reexport,
    /// `import x = require(…)`.
    ImportEquals,
    Call,
    /// A dynamic `import(…)`.
    ImportCall,
    New,
    TaggedTemplate,
    If,
    For,
    ForIn,
    ForOf,
    While,
    DoWhile,
    Switch,
    Case,
    Default,
    Try,
    Catch,
    Return,
    Throw,
    Break,
    Continue,
    ExpressionStatement,
    Identifier,
    PrivateIdentifier,
    StringLiteral,
    NumericLiteral,
    /// A template literal without substitutions.
    NoSubstitutionTemplate,
    PropertyAccess,
    ElementAccess,
    This,
    Super,
    Parenthesized,
    NonNull,
    As,
    Satisfies,
    TypeAssertion,
    /// An assignment expression; the node's text is its operator.
    Assignment,
    ComputedPropertyName,
    Decorator,
    /// A spread argument or element (`...value`).
    Spread,
    /// An array literal.
    Array,
    /// A hole in an array literal (`[a, , b]`).
    Hole,
    /// Type syntax: never selected, never descended into.
    Type,
    /// Any other syntax, kept for its range and runtime descendants.
    Other,
}

impl Cat {
    /// Expressions that unwrap to their `expression` child before value-shape decisions.
    pub(crate) const fn is_transparent(self) -> bool {
        matches!(
            self,
            Self::Parenthesized | Self::NonNull | Self::As | Self::Satisfies | Self::TypeAssertion
        )
    }

    /// Literals whose decoded text can name a declaration or key an element access.
    /// Module syntax that names a dependency through its `moduleSpecifier` child.
    pub(crate) const fn is_module_declaration(self) -> bool {
        matches!(
            self,
            Self::ImportDeclaration | Self::Reexport | Self::ImportEquals
        )
    }

    pub(crate) const fn is_name_literal(self) -> bool {
        matches!(
            self,
            Self::StringLiteral | Self::NumericLiteral | Self::NoSubstitutionTemplate
        )
    }
}

/// The child slot a node fills in its parent, named after the proof of concept's (TypeScript's)
/// child fields. Only labels that roles, slots, names, or values read are distinguished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Label {
    /// The root, which has no parent.
    Root,
    Statements,
    Expression,
    ThenStatement,
    ElseStatement,
    Initializer,
    Condition,
    Incrementor,
    Statement,
    TryBlock,
    CatchClause,
    FinallyBlock,
    VariableDeclaration,
    Block,
    CaseBlock,
    Name,
    Parameters,
    /// A written type, such as a callable's return type.
    Type,
    Body,
    Members,
    HeritageClauses,
    Arguments,
    Tag,
    ModuleSpecifier,
    Left,
    Right,
    ArgumentExpression,
    Other,
}

/// Boolean facts about a node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flags(u8);

impl Flags {
    /// A function-like node with a body.
    pub(crate) const HAS_BODY: Self = Self(1);
    /// The node is inside, or is, an ambient (`declare`) declaration.
    pub(crate) const AMBIENT: Self = Self(1 << 1);
    /// The declaration of `export default`.
    pub(crate) const DEFAULT: Self = Self(1 << 2);
    /// A binding element bound by a variable declaration (not a parameter).
    pub(crate) const VARIABLE_BINDING: Self = Self(1 << 3);
    /// An import, re-export, import-equals, or dynamic import that is a runtime dependency.
    pub(crate) const RUNTIME: Self = Self(1 << 4);

    pub(crate) const fn has(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    pub(crate) const fn with(self, flag: Self, on: bool) -> Self {
        if on { Self(self.0 | flag.0) } else { self }
    }
}

/// One normalized syntax node.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node<'a> {
    pub(crate) cat: Cat,
    pub(crate) label: Label,
    pub(crate) flags: Flags,
    pub(crate) range: ByteRange,
    /// An identifier's name, a literal's decoded value, or an assignment's operator.
    pub(crate) text: &'a str,
    pub(crate) parent: u32,
    first_child: u32,
    last_child: u32,
    next_sibling: u32,
}

/// One file's normalized syntax. Node 0 is the root; nodes are stored in preorder.
#[derive(Debug, Default)]
pub(crate) struct Syntax<'a> {
    nodes: Vec<Node<'a>>,
    /// The oxc node each expression, function, class, and class member was converted from, by
    /// its arena address, so binding (which walks oxc's tree) can find the normalized node and
    /// its block.
    origins: Vec<(Address, u32)>,
}

impl<'a> Syntax<'a> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            origins: Vec::new(),
        }
    }

    /// Records that `node` was converted from the oxc node at `address`.
    pub(crate) fn record_origin(&mut self, address: Address, node: u32) {
        self.origins.push((address, node));
    }

    /// Every recorded origin, in conversion order.
    pub(crate) fn origins(&self) -> &[(Address, u32)] {
        &self.origins
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn node(&self, id: u32) -> &Node<'a> {
        &self.nodes[id as usize]
    }

    pub(crate) fn cat(&self, id: u32) -> Cat {
        self.nodes[id as usize].cat
    }

    pub(crate) fn range(&self, id: u32) -> ByteRange {
        self.nodes[id as usize].range
    }

    /// Appends a node as the last child of `parent` (or as the root when `parent` is [`NONE`]).
    pub(crate) fn push(
        &mut self,
        parent: u32,
        label: Label,
        cat: Cat,
        range: ByteRange,
        flags: Flags,
        text: &'a str,
    ) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            cat,
            label,
            flags,
            range,
            text,
            parent,
            first_child: NONE,
            last_child: NONE,
            next_sibling: NONE,
        });
        if parent != NONE {
            let previous = self.nodes[parent as usize].last_child;
            if previous == NONE {
                self.nodes[parent as usize].first_child = id;
            } else {
                self.nodes[previous as usize].next_sibling = id;
            }
            self.nodes[parent as usize].last_child = id;
        }
        id
    }

    /// The node's children in source order.
    pub(crate) fn children(&self, id: u32) -> Children<'_, 'a> {
        Children {
            syntax: self,
            next: self.nodes[id as usize].first_child,
        }
    }

    /// The first child filling `label`.
    pub(crate) fn child(&self, id: u32, label: Label) -> Option<u32> {
        self.children(id)
            .find(|&child| self.nodes[child as usize].label == label)
    }

    /// The node with transparent wrappers (parentheses, assertions, non-null) removed.
    pub(crate) fn unwrap(&self, mut id: u32) -> u32 {
        while self.cat(id).is_transparent() {
            match self.child(id, Label::Expression) {
                Some(inner) => id = inner,
                None => break,
            }
        }
        id
    }
}

/// An iterator over a node's children.
pub(crate) struct Children<'s, 'a> {
    syntax: &'s Syntax<'a>,
    next: u32,
}

impl Iterator for Children<'_, '_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        self.next = self.syntax.nodes[id as usize].next_sibling;
        Some(id)
    }
}
