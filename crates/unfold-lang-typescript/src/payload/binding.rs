//! One file's binding, as extraction records it for linking (`docs/languages/typescript.md`,
//! "Binding"): every declaration, the symbols that merge them, the expressions evidence
//! evaluates, the written types it reads, class members, narrowing sites, and member writes.
//!
//! Everything lexical is decided here, once per file: which declaration an identifier names (or
//! that the file declares no such name), what `this` and `super` mean at each occurrence, and
//! which class a private name belongs to. Linking only joins files: it follows imports, script
//! globals, class bases, and member chains through these tables.
//!
//! Indices are positions in this file's tables: a declaration index is a `DeclRef` index (the
//! first `ModuleFacts::declarations.len()` declarations are the module declarations, in the
//! same order), a symbol index points into [`Binding::symbols`], an expression index into
//! [`Binding::exprs`], and a type index into [`Binding::types`].

use serde::{Deserialize, Serialize};
use unfold_core::text::ByteRange;

/// One file's binding facts.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Binding {
    /// Every declaration evidence can name; the module declarations come first, aligned with
    /// `ModuleFacts::declarations`.
    pub declarations: Vec<Decl>,
    /// Symbols: one name in one scope with all its merged declarations.
    pub symbols: Vec<Symbol>,
    /// Exported members of local namespaces and enums, by name (merged declarations share one).
    pub namespaces: Vec<Namespace>,
    /// Import aliases declared outside module scope, or by `import x = A.B` anywhere.
    pub aliases: Vec<Alias>,
    /// The expressions evidence evaluates.
    pub exprs: Vec<Expr>,
    /// For each value of the neutral value table, its expression.
    pub value_exprs: Vec<u32>,
    /// What `this` and `super` mean at each occurrence ([`Expr::This`], [`Expr::Super`]).
    pub contexts: Vec<Context>,
    /// Written types.
    pub types: Vec<TypeRef>,
    /// Every class (declaration or expression) of the file.
    pub classes: Vec<ClassFacts>,
    /// Constant object literals: the members a `const` initialized with one declares.
    pub objects: Vec<ObjectFacts>,
    /// Symbols declared at file scope. In a script they are global to the configuration group.
    pub file_symbols: Vec<u32>,
    /// Symbols declared inside `declare global { … }`: always global to the group.
    pub global_symbols: Vec<u32>,
    /// The namespace symbols a [`Lookup::Namespaced`] crossed, innermost first.
    pub namespace_chains: Vec<Vec<u32>>,
    /// Sites that can narrow a receiver (`instanceof`, type-predicate calls).
    pub narrowing: Narrowing,
    /// Member writes (`obj.name = …`, `delete obj.name`, `obj[key] = …`), for the mutation
    /// census.
    pub writes: Vec<MemberWrite>,
}

/// What a declaration declares, in the proof of concept's syntax categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeclKind {
    /// The file itself (module declaration 0).
    Module,
    /// `export default <expression>` / `export = <expression>` that is not a name; its inline
    /// function or class, if it is one, is [`Decl::inline`].
    Expression,
    /// A function declaration (an overload signature has no body).
    Function,
    /// A function expression.
    FunctionExpression,
    /// An arrow function.
    Arrow,
    /// A class declaration.
    Class,
    /// A class expression.
    ClassExpression,
    /// A method of a class or an object literal.
    Method,
    /// A class constructor.
    Constructor,
    /// A getter.
    Getter,
    /// A setter.
    Setter,
    /// A class property (field or `accessor`).
    Property,
    /// A variable declarator binding one identifier.
    Variable,
    /// One identifier bound by a destructuring pattern (of a variable or a parameter).
    BindingElement,
    /// A parameter (an explicit `this` parameter included, a parameter property too).
    Parameter,
    /// A namespace (`namespace A.B` declares one per name).
    Namespace,
    /// An enum.
    Enum,
    /// An enum member.
    EnumMember,
    /// An interface.
    Interface,
    /// A type alias.
    TypeAlias,
    /// A type parameter.
    TypeParameter,
    /// An import alias (`import x from`, `import x = …`).
    Import,
    /// An object literal `key: value` property.
    PropertyAssignment,
    /// An object literal shorthand property (`{ name }`).
    Shorthand,
}

impl DeclKind {
    /// Whether the declaration only augments a value it merges with (a namespace, interface,
    /// or type alias), supplying no body of its own.
    pub const fn is_augmentation(self) -> bool {
        matches!(self, Self::Namespace | Self::Interface | Self::TypeAlias)
    }

    /// A function, method, or constructor: what an overload signature can be.
    pub const fn is_overloadable(self) -> bool {
        matches!(self, Self::Function | Self::Method | Self::Constructor)
    }
}

/// How a variable binding was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VarKind {
    /// `var`.
    Var,
    /// `let`.
    Let,
    /// `const`.
    Const,
    /// `using` or `await using`.
    Using,
}

/// Boolean facts about a declaration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeclFlags(pub u16);

impl DeclFlags {
    /// A function-like declaration with a body.
    pub const HAS_BODY: Self = Self(1);
    /// Ambient: `declare`, or inside an ambient declaration.
    pub const AMBIENT: Self = Self(1 << 1);
    /// A `static` class member.
    pub const STATIC: Self = Self(1 << 2);
    /// Named by a private name (`#x`).
    pub const PRIVATE: Self = Self(1 << 3);
    /// A variable or binding element of a parameter (rather than of a variable declaration).
    pub const IN_PARAMETER: Self = Self(1 << 5);
    /// A direct statement of the file (for manual locators).
    pub const TOP_LEVEL: Self = Self(1 << 6);
    /// A catch clause binding (its stability is unknown).
    pub const CATCH: Self = Self(1 << 8);

    /// Whether every flag of `flag` is set.
    pub const fn has(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }

    /// These flags with `flag` set when `on`.
    #[must_use]
    pub const fn with(self, flag: Self, on: bool) -> Self {
        if on { Self(self.0 | flag.0) } else { self }
    }
}

/// A parameter declaration's position, for `parameter` reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamInfo {
    /// The containing parameter's declaration (itself, for a parameter).
    pub parameter: u32,
    /// The callable that declares it.
    pub owner: u32,
    /// Its runtime argument position: an explicit `this` parameter is skipped (C-4).
    pub position: u32,
}

/// One declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decl {
    /// What it declares.
    pub kind: DeclKind,
    /// Its name: an identifier, a private name (with `#`), or a decoded literal key; `None`
    /// for an anonymous declaration or a computed key that is not a literal.
    pub name: Option<Box<str>>,
    /// Boolean facts.
    pub flags: DeclFlags,
    /// Its range (from the declaration's own syntax).
    pub range: ByteRange,
    /// Its selected block, for a function-like declaration with a body or a class.
    pub block: Option<u32>,
    /// The syntax it is written in (a class, a statement list, an object literal): overload
    /// signatures and their implementation share it.
    pub container: u32,
    /// The nearest enclosing class.
    pub owner: Option<u32>,
    /// The symbol it is a declaration of.
    pub symbol: Option<u32>,
    /// For a variable or binding element, how it was declared.
    pub var_kind: Option<VarKind>,
    /// A variable's, property's, or parameter's initializer.
    pub initializer: Option<u32>,
    /// A written type: a variable's, parameter's, or property's annotation, a getter's return
    /// type, or a type alias's type.
    pub ty: Option<u32>,
    /// A callable's written return type.
    pub returns: Option<u32>,
    /// For a parameter, or a binding element of one, the containing parameter.
    pub parameter: Option<ParamInfo>,
    /// For an `Expression` declaration, or a property assignment whose value is one, its
    /// inline function or class.
    pub inline: Option<u32>,
    /// For an object-literal member, the [`ObjectFacts`] it belongs to.
    pub object: Option<u32>,
    /// For a class, its first explicit constructor with a body.
    pub constructor: Option<u32>,
    /// For a non-ambient `const` initialized by a call or `new`, the callee's value (an index
    /// into the neutral value table), which provenance traces.
    pub factory: Option<u32>,
}

/// The meanings a symbol answers to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Meanings(pub u8);

impl Meanings {
    /// A value.
    pub const VALUE: Self = Self(1);
    /// A type.
    pub const TYPE: Self = Self(2);
    /// Both.
    pub const BOTH: Self = Self(3);

    /// Whether every meaning of `meaning` is included.
    pub const fn has(self, meaning: Self) -> bool {
        self.0 & meaning.0 == meaning.0
    }
}

/// One name in one scope, with every declaration merged into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// The name.
    pub name: Box<str>,
    /// The meanings its declarations give it.
    pub meanings: Meanings,
    /// Its declarations, in binding order.
    pub declarations: Vec<u32>,
    /// The shared member table of a namespace or enum.
    pub namespace: Option<u32>,
    /// What an import alias imports.
    pub alias: Option<AliasRef>,
}

/// Where an import alias's target comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AliasRef {
    /// A module-scope import binding: an index into `ModuleFacts::imports`.
    Module(u32),
    /// An alias of [`Binding::aliases`].
    Local(u32),
}

/// An import alias outside module scope, or a qualified `import x = A.B.C`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alias {
    /// `import x = A.B.C`: the first name's lexical lookup and the member names after it.
    Qualified {
        /// The first name, looked up where the alias is declared.
        first: Box<str>,
        /// What it names.
        lookup: Lookup,
        /// The member names after it.
        rest: Vec<Box<str>>,
        /// `import type`.
        type_only: bool,
    },
    /// A binding destructured from a literal awaited dynamic import
    /// (`const { name } = await import("./m.js")`): the module's export `imported` (C-8).
    Dynamic {
        /// The decoded specifier.
        specifier: Box<str>,
        /// The export the binding reads.
        imported: Box<str>,
    },
    /// An import whose target linking does not follow (a module import inside a namespace).
    Unsupported,
}

/// A namespace's or enum's exported members.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Namespace {
    /// Member name → symbol, sorted by name.
    pub members: Vec<(Box<str>, u32)>,
}

/// What an identifier names in its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lookup {
    /// A symbol of this file.
    Symbol(u32),
    /// The file declares no such name where it is used: a script global of the configuration
    /// group, a known global, or nothing.
    Unresolved,
    /// Like [`Lookup::Unresolved`], but used inside namespaces whose other declarations (in
    /// other files of a script group) may export the name: the namespace symbols crossed,
    /// innermost first, are [`Binding::namespace_chains`] at this index.
    Namespaced(u32),
    /// Inside a `with` statement: no static lookup.
    Dynamic,
}

/// One expression, as evidence evaluates it. Child indices are smaller than the expression's
/// own index, so evaluation always ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    /// An identifier reference.
    Name {
        /// The name.
        name: Box<str>,
        /// What it names.
        lookup: Lookup,
    },
    /// A property access (`a.b`, `a?.b`, `a.#b`).
    Member {
        /// The receiver.
        object: u32,
        /// The member name (a private name keeps its `#`).
        name: Box<str>,
        /// For a private name, the members of the nearest enclosing class that declares it.
        private: Option<Vec<u32>>,
    },
    /// An element access.
    Element {
        /// The receiver.
        object: u32,
        /// The key, when it is a string, numeric, or plain template literal (decoded; numbers
        /// spelled as JavaScript spells them).
        key: Option<Box<str>>,
        /// Whether the key is written directly as a string or numeric literal.
        direct: bool,
        /// Whether the access reads an array element (any key but a non-index string).
        index: bool,
    },
    /// A call, or a dynamic import (no callee).
    Call {
        /// The callee.
        callee: Option<u32>,
    },
    /// A tagged template.
    Tagged {
        /// The tag.
        tag: u32,
    },
    /// A `new` expression.
    New {
        /// The constructed expression.
        callee: Option<u32>,
    },
    /// `this`, with its context in [`Binding::contexts`].
    This(u32),
    /// `super`, with its context in [`Binding::contexts`].
    Super(u32),
    /// Parentheses.
    Paren(u32),
    /// A non-null assertion `x!`.
    NonNull(u32),
    /// `x satisfies T`.
    Satisfies(u32),
    /// `x as T` or `<T>x`.
    Assert {
        /// The asserted expression.
        inner: u32,
        /// The asserted type.
        ty: Option<u32>,
    },
    /// An instantiation expression `f<T>`.
    Instantiation(u32),
    /// A function or arrow expression: its declaration.
    Function(u32),
    /// A class expression: its declaration.
    Class(u32),
    /// The `null` literal.
    Null,
    /// Anything else.
    Other,
}

/// What `this` or `super` means where it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// The class whose member's `this` (or `super`) it is, and whether that member is
    /// static; `None` when `this` is unbound (or `super` has no class member around it).
    pub receiver: Option<(u32, bool)>,
    /// The nearest enclosing class, whatever function lies between.
    pub containing: Option<u32>,
}

/// A written type, as far as evidence reads types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeRef {
    /// An array type `T[]`.
    Array,
    /// `string`, `number`, `boolean`, `bigint`, `symbol`, or `object`.
    Primitive,
    /// A type reference `A`, `A.B<…>`, or a type query `typeof a.b`.
    Names {
        /// The names, first to last.
        names: Vec<Box<str>>,
        /// A type query (`typeof`): the names are values.
        value: bool,
        /// What the first name names, in the meaning the type asks for.
        lookup: Lookup,
        /// Whether some declaration of the first name, in either meaning, is visible in this
        /// file (a local name can hide a library type even if it is not a type).
        shadowed: bool,
    },
    /// A type predicate `x is T` (or `asserts x is T`), which narrows its argument.
    Predicate,
    /// Any other type: unions, literals, function types, keywords such as `any`, …
    Other,
}

/// One class of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassFacts {
    /// The class declaration.
    pub decl: u32,
    /// The `extends` expression, if written.
    pub heritage: Option<u32>,
    /// Its members in written order (constructors, methods, accessors, properties), then its
    /// parameter properties.
    pub members: Vec<u32>,
}

/// The members a `const` object literal declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectFacts {
    /// The `const` variable it initializes.
    pub variable: u32,
    /// Its properties' declarations, in written order.
    pub members: Vec<u32>,
    /// Whether every member is known: no spread and no computed key that isn't a literal.
    pub closed: bool,
    /// Whether the object is the variable's value as written: the variable has no type
    /// annotation and the literal no `as` or `<T>` assertion.
    pub plain: bool,
}

/// Sites that can narrow a receiver.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Narrowing {
    /// The left operands of `instanceof`.
    pub instanceof: Vec<Key>,
    /// Calls with at least one keyed argument.
    pub calls: Vec<NarrowingCall>,
}

/// A receiver's narrowing key: a root and the member names after it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    /// The root.
    pub root: KeyRoot,
    /// The member names.
    pub path: Vec<Box<str>>,
}

/// The root of a narrowing key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyRoot {
    /// An identifier naming a symbol of this file.
    Symbol(u32),
    /// An identifier the file doesn't declare there (a [`Lookup::Unresolved`] or
    /// [`Lookup::Namespaced`] lookup).
    Name(Box<str>, Lookup),
    /// `this` of a class member: the class.
    This(u32),
}

/// A call that may narrow its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrowingCall {
    /// The callee as a name path (`f`, `ns.f`); `None` for any other callee.
    pub callee: Option<NamePath>,
    /// Whether the call is part of a condition.
    pub condition: bool,
    /// The keys of its arguments.
    pub keys: Vec<Key>,
}

/// An identifier followed by property names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePath {
    /// The identifier.
    pub first: Box<str>,
    /// What it names.
    pub lookup: Lookup,
    /// The property names after it.
    pub rest: Vec<Box<str>>,
}

/// A write to a member of some object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberWrite {
    /// The object written to.
    pub object: u32,
    /// The member written; `None` for a computed key, which may write any member.
    pub name: Option<Box<str>>,
}
