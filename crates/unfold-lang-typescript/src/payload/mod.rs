//! The TypeScript part of a file's cached facts ([`FileFacts::payload`]), encoded with postcard.
//!
//! It holds what linking and validation need without the syntax tree: what each recorded value
//! is (aligned with the neutral value table), what kind each runtime import is (aligned with the
//! import facts), the module-level facts (module-scope declarations, import bindings, and export
//! entries; [`ModuleFacts`]), the file's binding ([`Binding`]: declarations, symbols, the
//! expressions evidence evaluates, classes, and censuses), and the independent census. Any change
//! to this format, or to anything else that shapes this crate's extracted facts, must bump
//! [`PAYLOAD_VERSION`], which is part of the settings hash and so of the facts-cache key.
//!
//! [`FileFacts::payload`]: unfold_engine::FileFacts::payload

use serde::{Deserialize, Serialize};
use unfold_core::text::ByteRange;

use crate::census::CensusRecord;

pub mod binding;

pub use binding::Binding;

/// The version of TypeScript extraction: the payload format and every rule that shapes facts.
pub const PAYLOAD_VERSION: u32 = 6;

/// One file's TypeScript facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    /// What each value of the neutral value table is, in the same order.
    pub values: Vec<ValueSyntax>,
    /// The kind of each runtime import, in the order of the neutral import facts.
    pub imports: Vec<ImportKind>,
    /// Module-scope declarations, import bindings, and export entries.
    pub module: ModuleFacts,
    /// Token boundaries, call sites, and module dependencies, found independently of selection.
    pub census: CensusRecord,
    /// Declarations, symbols, and the expressions evidence evaluates.
    pub binding: Binding,
    /// The argument syntax of the call sites an entrance detector may recognize, by block.
    pub sites: Vec<Site>,
}

/// One call or construction an entrance detector may recognize (a `new`, or a call of a name or
/// of a member named like a framework registration), with the argument syntax that its values
/// don't keep. Sites whose arguments are all [`ArgumentSyntax::Plain`] are not recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    /// The `call` or `new` block's local ID.
    pub block: u32,
    /// One entry per written argument, aligned with the call fact's arguments.
    pub arguments: Vec<ArgumentSyntax>,
}

/// What a detector needs to know about one written argument beyond its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgumentSyntax {
    /// Nothing beyond its value.
    Plain,
    /// A string or no-substitution template literal (through transparent wrappers), decoded.
    /// Recorded for the first two arguments.
    Literal(Box<str>),
    /// An array literal (through transparent wrappers) in a route registration's handler
    /// position, with nested array literals flattened in written order.
    Array(Vec<Element>),
    /// A `new` expression (through transparent wrappers) as a construction's second argument,
    /// with its source text exactly as written.
    Construction(Box<str>),
}

/// One element of a flattened array literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Element {
    /// An expression: its value.
    Value(u32),
    /// A spread element (`...rest`): its value.
    Spread(u32),
    /// A hole (`[a, , b]`).
    Hole,
}

/// The syntax behind one recorded value. Receivers and member names are in the neutral
/// [`ValueFact`](unfold_engine::facts::ValueFact); indices here are value-table indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueSyntax {
    /// An identifier reference at `range`.
    Name {
        /// The identifier's range.
        range: ByteRange,
    },
    /// A property access (`a.b`, `a?.b`, `a.#b`).
    Member,
    /// An element access with a literal key, decoded.
    LiteralKey {
        /// The decoded key.
        key: Box<str>,
    },
    /// An element access with a computed key.
    Computed,
    /// The result of a call (or of a dynamic import, which has no callee).
    CallResult {
        /// The callee's value.
        callee: Option<u32>,
    },
    /// A `new` expression used as a value.
    Construction {
        /// The constructed expression's value.
        callee: Option<u32>,
    },
    /// A tagged template used as a value.
    TaggedTemplate {
        /// The tag's value.
        tag: Option<u32>,
    },
    /// `this` at `range`.
    This {
        /// The keyword's range.
        range: ByteRange,
    },
    /// `super` at `range`.
    Super {
        /// The keyword's range.
        range: ByteRange,
    },
    /// An inline function or class; its block is in the neutral value.
    Inline,
    /// Anything else: literals, spreads, operators, and other expressions with no static shape.
    Other,
}

/// How a runtime module dependency is written, which decides how its specifier resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportKind {
    /// An `import` declaration.
    Import,
    /// An `export … from` or `export * from` declaration.
    Reexport,
    /// `import x = require("…")`.
    ImportEquals,
    /// A dynamic `import(…)`.
    Dynamic,
}

/// What a file declares at module scope, imports, and exports: what export tables and import
/// bindings are built from at link time.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModuleFacts {
    /// Whether the file has `import` or `export` syntax (including `import x = require(…)`,
    /// `export =`, and `import.meta`), which makes it a module under `auto` module detection.
    pub module_syntax: bool,
    /// Whether the file contains JSX, which makes it a module under `auto` detection with an
    /// automatic JSX runtime.
    pub jsx: bool,
    /// The declarations exports and locators can name. Index 0 is always the file itself
    /// ([`DeclarationKind::Module`]); a [`DeclRef`](unfold_engine::DeclRef)'s index is a
    /// position in this list.
    pub declarations: Vec<Declaration>,
    /// Import bindings (and `import x = …` aliases), in written order.
    pub imports: Vec<ImportBinding>,
    /// Export entries, in written order.
    pub exports: Vec<ExportEntry>,
}

/// One declaration at module scope (or the file itself, or an exported expression).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    /// The declared name; empty for the file, an anonymous default export, or an expression.
    pub name: Box<str>,
    /// What it declares.
    pub kind: DeclarationKind,
    /// The declaration's range (for a variable, its declarator or binding identifier).
    pub range: ByteRange,
    /// The declaration's own block (a function, class, or variable block), if it has one.
    pub block: Option<u32>,
    /// The block a manual locator names for it: a function or class body, or the function or
    /// class a `const` is initialized with.
    pub locator: Option<u32>,
    /// Written with `export` (a named export of its name).
    pub exported: bool,
    /// Written with `export default`.
    pub default: bool,
    /// Ambient (`declare`, or inside an ambient declaration).
    pub ambient: bool,
}

/// What a [`Declaration`] declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeclarationKind {
    /// The file itself, as a namespace re-export or import denotes it.
    Module,
    /// A function declaration (an overload signature has no block).
    Function,
    /// A class declaration.
    Class,
    /// A `var` binding.
    Var,
    /// A `let` binding.
    Let,
    /// A `const` binding.
    Const,
    /// A `using` or `await using` binding.
    Using,
    /// An `enum` (`const enum` included).
    Enum,
    /// A namespace; `instantiated` when it contains runtime declarations.
    Namespace {
        /// Whether the namespace has a runtime value.
        instantiated: bool,
    },
    /// An interface.
    Interface,
    /// A type alias.
    TypeAlias,
    /// An exported expression (`export default <expression>`, `export = <expression>`) that is
    /// not a name.
    Expression,
}

impl DeclarationKind {
    /// Whether the declaration has a runtime value.
    pub const fn is_value(self) -> bool {
        !matches!(
            self,
            Self::Interface | Self::TypeAlias | Self::Namespace { instantiated: false }
        )
    }

    /// Whether the declaration only augments a value it merges with (a namespace, interface, or
    /// type alias), supplying no body of its own.
    pub const fn is_augmentation(self) -> bool {
        matches!(self, Self::Namespace { .. } | Self::Interface | Self::TypeAlias)
    }
}

/// One import binding: `import x from`, `import { a as b } from`, `import * as ns from`,
/// `import x = require(…)`, or `import x = A.B`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBinding {
    /// The local name.
    pub local: Box<str>,
    /// The local name's range.
    pub range: ByteRange,
    /// The imported export: `default`, `*`, `export=`, or a name.
    pub imported: Box<str>,
    /// Where it comes from.
    pub source: ImportSource,
    /// `import type`, or a `type` specifier.
    pub type_only: bool,
    /// `export import x = …`, which also exports the alias.
    pub exported: bool,
}

/// Where an [`ImportBinding`] comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportSource {
    /// A module specifier; `require` for `import x = require(…)`.
    Module {
        /// The decoded specifier.
        specifier: Box<str>,
        /// Written as `import x = require(…)`.
        require: bool,
    },
    /// An entity name (`import x = A.B.C`).
    Qualified(Vec<Box<str>>),
}

/// One export entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportEntry {
    /// `export { local as exported }`.
    Local {
        /// The export name.
        exported: Box<str>,
        /// The local name it exports.
        local: Box<str>,
        /// `export type { … }`, or a `type` specifier.
        type_only: bool,
    },
    /// `export default <expression>` (`exported` is `default`) or `export = <expression>`
    /// (`export=`). A name or a chain of property accesses is followed through `path`; any other
    /// expression is the declaration at `declaration`.
    Value {
        /// `default` or `export=`.
        exported: Box<str>,
        /// The root name and the member names after it, when the expression is an access path.
        path: Option<Vec<Box<str>>>,
        /// The [`DeclarationKind::Expression`] declaration, when it is not a path.
        declaration: Option<u32>,
    },
    /// `export { imported as exported } from "…"`, or `export * as exported from "…"`
    /// (`imported` is `*`).
    Reexport {
        /// The export name.
        exported: Box<str>,
        /// The re-exported name, or `*`.
        imported: Box<str>,
        /// The decoded specifier.
        specifier: Box<str>,
        /// `export type …`, or a `type` specifier.
        type_only: bool,
    },
    /// `export * from "…"`.
    Star {
        /// The decoded specifier.
        specifier: Box<str>,
        /// `export type * from`.
        type_only: bool,
    },
}

impl Payload {
    /// The payload's cache encoding.
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Decodes a cached payload.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }
}
