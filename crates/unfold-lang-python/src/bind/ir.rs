//! The binder's per-file facts: everything linking needs to bind names across the program
//! without a syntax tree. Extraction writes them into the payload ([`crate::payload::Payload`]);
//! linking reads them for every file and answers evidence questions
//! ([`crate::link`]).
//!
//! | Table | Holds | Indexed by |
//! | --- | --- | --- |
//! | [`BindFacts::names`] | Identifiers (NFKC), member names, and string literals | [`NameId`] |
//! | [`BindFacts::scopes`] | Module, function, class, comprehension, and type-parameter scopes | [`ScopeId`] (0 is the module) |
//! | [`BindFacts::nodes`] | Declarations: definitions, parameters, and every statement that writes a name | [`NodeId`], the `index` of a Python [`DeclRef`](unfold_engine::DeclRef) |
//! | [`BindFacts::exprs`] | The expressions binding evaluates, in a small closed vocabulary | [`ExprId`] |
//! | [`BindFacts::writes`] | One write of one name | write index |
//! | [`BindFacts::bindings`] | Every write of one name in one scope, after `global`/`nonlocal` | binding index |
//! | [`BindFacts::types`] | The expression of each written type name, in its annotation's scope | the file's type references |
//!
//! Writes are a complete census: definitions, imports, assignments (chained, annotated,
//! augmented, unpacking), `del`, loop, `with`, and `except` targets, assignment expressions,
//! comprehension targets, parameters, type parameters, `type` aliases, and pattern captures.

use serde::{Deserialize, Serialize};

/// An index into [`BindFacts::names`].
pub type NameId = u32;
/// An index into [`BindFacts::scopes`].
pub type ScopeId = u32;
/// An index into [`BindFacts::nodes`].
pub type NodeId = u32;
/// An index into [`BindFacts::exprs`].
pub type ExprId = u32;

/// The module scope.
pub const MODULE_SCOPE: ScopeId = 0;

/// One file's binder facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindFacts {
    /// Interned text: NFKC identifiers, member names, and decoded string literals.
    pub names: Vec<Box<str>>,
    /// Scopes; the module is first.
    pub scopes: Vec<ScopeFact>,
    /// Declarations.
    pub nodes: Vec<NodeFact>,
    /// Expressions.
    pub exprs: Vec<ExprFact>,
    /// Writes, in the order the proof of concept collected them (preorder).
    pub writes: Vec<WriteFact>,
    /// Bindings, in the order of their first write.
    pub bindings: Vec<BindingFact>,
    /// The expression of each entry of the file's value table, aligned with it.
    pub values: Vec<ExprId>,
    /// The expression of each written type name, aligned with the file's type references
    /// ([`TypeFacts::type_refs`](unfold_engine::facts::TypeFacts::type_refs)): a name or a
    /// member chain, evaluated in the scope its annotation is (a string annotation's name is
    /// synthesized).
    pub types: Vec<ExprId>,
    /// Star imports, which linking expands from the target's literal `__all__`.
    pub stars: Vec<StarFact>,
    /// What the mutation passes read, in preorder: attribute and item targets, augmented
    /// targets, and every call.
    pub effects: Vec<Effect>,
}

/// What kind of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    /// The module.
    Module,
    /// A function or lambda body.
    Function,
    /// A class body.
    Class,
    /// A comprehension or generator expression.
    Comprehension,
    /// The annotation scope of PEP 695 type parameters.
    Type,
}

/// One scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeFact {
    /// Its kind.
    pub kind: ScopeKind,
    /// The enclosing scope; `None` only for the module.
    pub parent: Option<ScopeId>,
    /// Names declared `global` here: reads go to the module.
    pub globals: Vec<NameId>,
    /// Names declared `nonlocal` here, with the enclosing function scope reads go to (when one
    /// binds the name).
    pub nonlocals: Vec<(NameId, ScopeId)>,
}

/// One declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeFact {
    /// What it is.
    pub kind: NodeKind,
    /// The scope it is written in.
    pub scope: ScopeId,
    /// The nearest enclosing function, class, or lambda.
    pub enclosing: Option<NodeId>,
}

/// A declaration's kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// A `def` or `lambda`.
    Function(FunctionFact),
    /// A `class`.
    Class(ClassFact),
    /// A named parameter.
    Parameter {
        /// Its name.
        name: NameId,
        /// The function or lambda it belongs to.
        function: NodeId,
        /// Its position among the named parameters (written ordinal, receivers included).
        position: u32,
        /// Its written annotation.
        annotation: Option<ExprId>,
    },
    /// An assignment, annotated assignment, or assignment expression.
    Assignment {
        /// The value it assigns (for an outer level of a chain, the next level).
        value: Option<ExprId>,
        /// The written annotation.
        annotation: Option<ExprId>,
        /// When the value is a call, the callee's index in the value table.
        factory: Option<u32>,
    },
    /// An `import` item, a `from` import name, or a star import statement.
    Import,
    /// Any other write: loop, `with`, and `except` targets, `del`, augmented assignment, type
    /// parameters and aliases, pattern captures.
    Other,
}

/// A `def` or `lambda`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionFact {
    /// Its block's local ID.
    pub block: u32,
    /// Its name; `None` for a lambda.
    pub name: Option<NameId>,
    /// Decorator expressions, in written order.
    pub decorators: Vec<ExprId>,
    /// The named parameters, in written order.
    pub parameters: Vec<NodeId>,
    /// The name of the first written parameter, unless it is a bare `*`.
    pub first_parameter: Option<NameId>,
    /// The body scope.
    pub body: ScopeId,
}

/// A `class`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassFact {
    /// Its block's local ID.
    pub block: u32,
    /// Its name.
    pub name: NameId,
    /// Decorator expressions, in written order.
    pub decorators: Vec<ExprId>,
    /// Bases and keywords, in written order.
    pub arguments: Vec<Argument>,
    /// The body scope.
    pub body: ScopeId,
}

/// One written argument of a call or class header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    /// The keyword, for `name=value`.
    pub keyword: Option<NameId>,
    /// The value (for `*x` and `**x`, the expanded expression).
    pub value: ExprId,
    /// Whether it is `*x` or `**x`.
    pub expansion: bool,
}

/// One expression, in its scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExprFact {
    /// What it is.
    pub kind: ExprKind,
    /// The scope it is evaluated in.
    pub scope: ScopeId,
}

/// The expression shapes binding distinguishes. Everything else is [`ExprKind::Other`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExprKind {
    /// A name.
    Name(NameId),
    /// `base.name`.
    Member {
        /// The receiver.
        base: ExprId,
        /// The member name.
        name: NameId,
    },
    /// A call.
    Call {
        /// The callee.
        callee: ExprId,
        /// The written arguments.
        arguments: Vec<Argument>,
        /// The nearest enclosing function or lambda, which `super()` reads.
        function: Option<NodeId>,
    },
    /// `base[...]`, with the key when it is a single string literal.
    Index {
        /// The receiver.
        base: ExprId,
        /// The literal key.
        key: Option<NameId>,
    },
    /// `await value`.
    Await(ExprId),
    /// `a if test else b`.
    Ternary,
    /// A plain string literal (bytes and f-strings are [`ExprKind::Other`]).
    Str(NameId),
    /// A list or tuple display.
    Sequence(Vec<ExprId>),
    /// `*value` inside a display or target, which only the detectors' negative census reads.
    Starred(ExprId),
    /// A dict display's values (and `**` expansions).
    Dict(Vec<ExprId>),
    /// A lambda.
    Lambda(NodeId),
    /// An outer level of a chained assignment, whose value is the next level's value.
    Chain(ExprId),
    /// Anything else.
    Other,
}

/// How a write binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteKind {
    /// `def` or `class`.
    Definition,
    /// A parameter.
    Parameter,
    /// An assignment, annotated assignment, or assignment expression.
    Assignment,
    /// An import.
    Import,
    /// Anything else.
    Other,
}

/// The module an import names, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSpec {
    /// Leading dots.
    pub level: u32,
    /// The dotted name after the dots (NFKC); empty for `from . import x`.
    pub module: Box<str>,
}

impl ModuleSpec {
    /// The specifier text: dots, then the dotted name.
    pub fn text(&self) -> String {
        let mut text = ".".repeat(self.level as usize);
        text.push_str(&self.module);
        text
    }
}

/// What an import write binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportWrite {
    /// The module: for `import a.b` its first part, for `import a.b as c` the whole name, for
    /// `from m import n` the module `m`.
    pub module: ModuleSpec,
    /// The imported member, for `from m import n`.
    pub member: Option<NameId>,
}

/// One write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteFact {
    /// The declaration that writes.
    pub node: NodeId,
    /// How it binds.
    pub kind: WriteKind,
    /// The assigned value, for an assignment.
    pub value: Option<ExprId>,
    /// The written annotation, for a parameter or an annotated assignment.
    pub annotation: Option<ExprId>,
    /// What it imports, for an import.
    pub import: Option<ImportWrite>,
    /// Whether an `if`, `try`, `match`, `for`, or `while` (inside the same function, class, or
    /// module) contains it.
    pub conditional: bool,
}

/// Every write of one name in one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingFact {
    /// The scope.
    pub scope: ScopeId,
    /// The name.
    pub name: NameId,
    /// Its writes, in order.
    pub writes: Vec<u32>,
}

/// One `from m import *`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarFact {
    /// The scope it binds in.
    pub scope: ScopeId,
    /// The module.
    pub module: ModuleSpec,
    /// The statement's declaration.
    pub node: NodeId,
    /// Whether it is conditional (see [`WriteFact::conditional`]).
    pub conditional: bool,
}

/// What a target write is, for the mutation passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    /// An assignment level (its value is [`Effect::Target::value`]).
    Assignment,
    /// An annotation, `target: T` (alone or as part of an annotated assignment).
    Annotation,
    /// Anything else: augmented assignment, `del`, loop and `with` targets.
    Other,
}

/// One input of the mutation passes, in preorder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// A written target that contains an attribute or item.
    Target {
        /// The target (possibly a tuple or list).
        target: ExprId,
        /// What writes it.
        kind: TargetKind,
        /// The assigned value, for an assignment.
        value: Option<ExprId>,
        /// The annotation, for an annotation or annotated assignment.
        annotation: Option<ExprId>,
        /// The nearest enclosing function or lambda.
        function: Option<NodeId>,
        /// Whether this is an augmented assignment (`target op= value`).
        augmented: bool,
    },
    /// A call.
    Call(ExprId),
}

impl BindFacts {
    /// Whether every index in the facts is in range, scope parents and enclosing declarations
    /// come before what they enclose (so walking outwards ends), and the module scope is first.
    /// Extraction always writes consistent facts; linking checks them once per file because
    /// they come back from the on-disk cache, where a damaged entry may still decode.
    pub fn is_consistent(&self) -> bool {
        let name = |id: NameId| (id as usize) < self.names.len();
        let scope = |id: ScopeId| (id as usize) < self.scopes.len();
        let node = |id: NodeId| (id as usize) < self.nodes.len();
        let expr = |id: ExprId| (id as usize) < self.exprs.len();
        let argument = |argument: &Argument| argument.keyword.is_none_or(name) && expr(argument.value);
        let module = self
            .scopes
            .first()
            .is_some_and(|first| first.kind == ScopeKind::Module && first.parent.is_none());
        module
            && self.scopes.iter().enumerate().skip(1).all(|(index, fact)| {
                fact.parent.is_some_and(|parent| (parent as usize) < index)
                    && fact.globals.iter().all(|&id| name(id))
                    && fact
                        .nonlocals
                        .iter()
                        .all(|&(local, target)| name(local) && scope(target))
            })
            && self.nodes.iter().enumerate().all(|(index, fact)| {
                scope(fact.scope)
                    && fact
                        .enclosing
                        .is_none_or(|enclosing| (enclosing as usize) < index)
                    && match &fact.kind {
                        NodeKind::Function(function) => {
                            function.name.is_none_or(name)
                                && function.decorators.iter().all(|&id| expr(id))
                                && function.parameters.iter().all(|&id| node(id))
                                && function.first_parameter.is_none_or(name)
                                && scope(function.body)
                        }
                        NodeKind::Class(class) => {
                            name(class.name)
                                && class.decorators.iter().all(|&id| expr(id))
                                && class.arguments.iter().all(argument)
                                && scope(class.body)
                        }
                        NodeKind::Parameter {
                            name: parameter,
                            function,
                            annotation,
                            ..
                        } => name(*parameter) && node(*function) && annotation.is_none_or(expr),
                        NodeKind::Assignment {
                            value, annotation, ..
                        } => value.is_none_or(expr) && annotation.is_none_or(expr),
                        NodeKind::Import | NodeKind::Other => true,
                    }
            })
            && self.exprs.iter().all(|fact| {
                scope(fact.scope)
                    && match &fact.kind {
                        ExprKind::Name(id) | ExprKind::Str(id) => name(*id),
                        ExprKind::Member { base, name: member } => expr(*base) && name(*member),
                        ExprKind::Call {
                            callee,
                            arguments,
                            function,
                        } => expr(*callee) && arguments.iter().all(argument) && function.is_none_or(node),
                        ExprKind::Index { base, key } => expr(*base) && key.is_none_or(name),
                        ExprKind::Await(id) | ExprKind::Starred(id) | ExprKind::Chain(id) => expr(*id),
                        ExprKind::Sequence(items) | ExprKind::Dict(items) => items.iter().all(|&id| expr(id)),
                        ExprKind::Lambda(id) => node(*id),
                        ExprKind::Ternary | ExprKind::Other => true,
                    }
            })
            && self.writes.iter().all(|write| {
                node(write.node)
                    && write.value.is_none_or(expr)
                    && write.annotation.is_none_or(expr)
                    && write
                        .import
                        .as_ref()
                        .is_none_or(|import| import.member.is_none_or(name))
            })
            && self.bindings.iter().all(|binding| {
                scope(binding.scope)
                    && name(binding.name)
                    && binding
                        .writes
                        .iter()
                        .all(|&write| (write as usize) < self.writes.len())
            })
            && self.values.iter().all(|&id| expr(id))
            && self.types.iter().all(|&id| expr(id))
            && self.stars.iter().all(|star| scope(star.scope) && node(star.node))
            && self.effects.iter().all(|effect| match effect {
                Effect::Target {
                    target,
                    value,
                    annotation,
                    function,
                    ..
                } => {
                    expr(*target)
                        && value.is_none_or(expr)
                        && annotation.is_none_or(expr)
                        && function.is_none_or(node)
                }
                Effect::Call(id) => expr(*id),
            })
    }
}
