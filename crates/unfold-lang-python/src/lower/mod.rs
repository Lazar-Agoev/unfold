//! Lowering one parsed file straight to its node tree, with one rule per syntax form
//! (docs/languages/python.md#lowering). Statements are lowered in [`statements`], expressions in [`expressions`].
//!
//! | Written | Node |
//! | --- | --- |
//! | the module | `module`; its docstring is `doc` |
//! | `def`, `async def` | `function` (`function`; in a class body `method`, `constructor` for `__init__`, `getter`/`setter` for a proven `@property` and its setter); the first block of its statement, which runs from its first decorator to its end |
//! | `class` | `class`, the first block of its statement (from its first decorator to its end); a base written as a name is an `extends` link, any call in its header a `base` call |
//! | decorators (plain names included), and a `def`'s defaults and annotations | calls in the enclosing body before the definition, role `decorator` or `default` (a class's header: `base`), `for` the definition |
//! | `lambda` | `function` (`lambda`): named when it is exactly the value of a simple assignment, `callback[n]` in a call's arguments, `lambda[n]` anywhere else; its defaults are `default` calls beside it |
//! | `if`/`elif`/`else` | `decision` (`if`): `then`, `elif[n]`, `else`; each test is its part's `head`, its calls `test` calls |
//! | `match`/`case`/`case _:` | `decision` (`match`), headed by its subject: `case[n]` (its pattern and guard are its head), and `default` for the guardless `case _:`, with no head |
//! | `a if c else b` with a call | `decision` (`ternary`): `then` (test `c`, body `a`), `else` |
//! | `for`, `async for`, `while`, loop `else` | `loop` (`for`, `while`): header calls, `body`, `else` |
//! | comprehension or generator with a call | `loop` (`comprehension`): its `for` clauses are the header, its element the body, its `if` clauses the body's test |
//! | `try`/`except`/`except*`/`else`/`finally` | `try`: `body`, `except[n]` (flag `group` for `except*`, the clause as its head), `else`, `finally` |
//! | `with`, `async with` | `with` (flag `async`): header calls, `body` |
//! | `return`, `raise`, `break`, `continue` | `exit`, with its statement as `stmt` and its value as `value`; `break` and `continue` name their loop as `target` |
//! | `yield`, `yield from` as a statement or an assignment's whole value | `exit` (`yield`) |
//! | calls, decorators | `call`, with its normalized callee, `binds`, `exit_value`, `awaited`, `conditional`, `role`, `for`, and `stmt` (a decorator has none: its definition is its statement's first block); the expression blocks in its receiver and arguments are its children |
//! | imports, `importlib.import_module("x")` | `imports` links from the definition the import is written in (the module, a class, or a function); the call is also a call |
//! | assignments, expression statements, `pass`, `assert`, `del`, `global`, `nonlocal` | code: only their calls are blocks, and the first block of a statement records it as `stmt` |
//!
//! A `def` a decorator proves a `typing.overload` stub is left out with everything written in
//! it, so the implementation takes the plain key.
//!
//! Past [`MAX_DEPTH`] levels of nodes, a node is marked `truncated` and nothing below it is
//! lowered (a truncated control block keeps its parts, themselves truncated and empty).
//! Lowering recurses with the tree, on a stack that grows as needed, so deep syntax can't
//! overflow it.

mod expressions;
mod statements;

pub(crate) use expressions::address;

use ruff_python_ast::{Alias, Arguments, Expr, ModModule, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::key::{Child, KeyBuilder};
use unfold_core::text::{ByteRange, LineBreaks, LineIndex};
use unfold_core::typed::{Member, Parameter, Signature};
use unfold_core::vocab::{CallRole, Flag, Flags, Form, NodeKind};
use unfold_core::{Key, Node, RepoPath};
use unfold_engine::SourceFailure;
use unfold_engine::facts::ImportFact;

use crate::analysis::Proofs;
use crate::ast::bytes;
use crate::parse::failure;
use crate::payload::ImportSyntax;
use crate::tokens::Tokens;

/// The deepest a node nests below the module; a node at this depth is truncated.
pub(crate) const MAX_DEPTH: u32 = 256;

/// The stack left before lowering grows it, and how much each growth adds.
const RED_ZONE: usize = 64 * 1024;
const STACK_GROWTH: usize = 1024 * 1024;

/// What one call node is written as: a call expression, or a decorator that is not one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Called<'a> {
    /// The call's node.
    pub(crate) node: u32,
    /// The call expression, or the decorator's expression.
    pub(crate) expr: &'a Expr,
    /// The callee: the call's function, or the decorator's expression.
    pub(crate) callee: &'a Expr,
    /// The written arguments; a decorator that is not a call has none.
    pub(crate) arguments: Option<&'a Arguments>,
}

/// One written runtime module dependency.
#[derive(Debug, Clone)]
pub(crate) struct Import<'a> {
    pub(crate) fact: ImportFact,
    pub(crate) syntax: ImportSyntax,
    /// For a named child import, its alias, whose declaration the binder knows.
    pub(crate) child: Option<&'a Alias>,
}

/// A node lowering stopped below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Truncated {
    pub(crate) range: ByteRange,
    /// Whether the node is a call, which is lowered itself: only what is written inside it is
    /// left out.
    pub(crate) call: bool,
}

impl Truncated {
    /// Whether a call or dependency at `range` is left out of the graph by this truncation.
    pub(crate) fn hides(&self, range: ByteRange) -> bool {
        self.range.contains(range) && !(self.call && self.range == range)
    }
}

/// One file lowered: its nodes in preorder, and what extraction builds the rest from.
#[derive(Debug)]
pub(crate) struct Lowered<'a> {
    pub(crate) nodes: Vec<Node>,
    /// One per call node, in node order.
    pub(crate) calls: Vec<Called<'a>>,
    /// In written order.
    pub(crate) imports: Vec<Import<'a>>,
    /// The node of every `def` and `class` statement and every `lambda`, by its address.
    pub(crate) definitions: FxHashMap<usize, u32>,
    pub(crate) truncated: Vec<Truncated>,
    /// The statements of the `typing.overload` stubs, left out with everything written in them.
    pub(crate) stubs: Vec<ByteRange>,
    pub(crate) signatures: Vec<Signature>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) members: Vec<Member>,
}

/// Where blocks are being lowered.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The node new blocks are children of.
    parent: u32,
    /// The parent's depth: the module is 0.
    depth: u32,
    /// The definition the code is written in (the module, a class, or a function), which the
    /// `imports` links of its import statements start at.
    definition: u32,
    /// The innermost `for` or `while` whose body this is, inside the same definition.
    target: Option<u32>,
    /// The role of the calls written here.
    role: Option<CallRole>,
    /// Whether this is in the arguments of `parent`, a call: a function written here is a
    /// callback.
    argument: bool,
    /// Whether code here runs only sometimes inside its expression: on the right side of an
    /// `and` or `or`, including inside the calls, ternaries, and comprehensions written there.
    conditional: bool,
}

impl Place {
    /// Directly inside `node`, a child of this place's parent. Code inside a block that runs
    /// only sometimes runs only sometimes too.
    fn under(self, node: u32) -> Self {
        Self {
            parent: node,
            depth: self.depth + 1,
            role: None,
            argument: false,
            ..self
        }
    }

    /// Inside the definition `node`: a new scope with no loop around it, whose code runs when
    /// the definition is called rather than where it is written.
    fn definition(self, node: u32) -> Self {
        Self {
            definition: node,
            target: None,
            conditional: false,
            ..self.under(node)
        }
    }

    fn with_role(self, role: CallRole) -> Self {
        Self {
            role: Some(role),
            ..self
        }
    }
}

/// How an expression's own value is used, for the node it becomes.
#[derive(Debug, Clone, Copy, Default)]
struct Value<'a> {
    /// The written target a statement assigns it to.
    binds: Option<&'a str>,
    /// It is exactly what its exit returns, raises, or yields.
    exit_value: bool,
    /// It is written under `await`.
    awaited: bool,
}

struct Lowering<'a> {
    text: &'a str,
    tokens: Tokens<'a>,
    proofs: &'a Proofs,
    lines: LineIndex,
    keys: KeyBuilder,
    /// The `if` expressions and comprehensions (by address) that contain a call.
    calling: FxHashSet<usize>,
    /// The `def` statements and lambdas (by address) whose own body yields.
    generators: FxHashSet<usize>,
    /// The statement whose first block has not been lowered yet.
    statement: Option<TextRange>,
    /// Decorator, default, and base calls waiting for the definition they belong to.
    owned: Vec<u32>,
    lowered: Lowered<'a>,
}

/// Lowers `module`, the parse of `text` at `path`.
pub(crate) fn lower<'a>(
    path: &RepoPath,
    text: &'a str,
    module: &'a ModModule,
    tokens: Tokens<'a>,
    proofs: &'a Proofs,
) -> Result<Lowered<'a>, SourceFailure> {
    let (calling, generators) = expressions::shapes(module);
    let lines = LineIndex::new(text, LineBreaks::Newlines);
    let keys = KeyBuilder::new(path);
    let range = TextRange::up_to(TextSize::of(text));
    let mut lowering = Lowering {
        text,
        tokens,
        proofs,
        keys,
        calling,
        generators,
        statement: None,
        owned: Vec::new(),
        lowered: Lowered {
            nodes: vec![Node {
                key: Key::module(path),
                parent: None,
                form: Form::Module,
                name: None,
                range: bytes(range),
                lines: lines.span(bytes(range)),
                head: None,
                body: Some(bytes(range)),
                doc: docstring(&module.body).map(bytes),
                stmt: None,
                value: None,
                binds: None,
                role: None,
                r#for: None,
                flags: Flags::NONE,
                target: None,
            }],
            calls: Vec::new(),
            imports: Vec::new(),
            definitions: FxHashMap::default(),
            truncated: Vec::new(),
            stubs: Vec::new(),
            signatures: Vec::new(),
            parameters: Vec::new(),
            members: Vec::new(),
        },
        lines,
    };
    let root = Place {
        parent: 0,
        depth: 0,
        definition: 0,
        target: None,
        role: None,
        argument: false,
        conditional: false,
    };
    lowering.body(&module.body, root)?;
    Ok(lowering.lowered)
}

/// The docstring of a body: its first statement, when that is a plain string literal.
pub(crate) fn docstring(body: &[Stmt]) -> Option<TextRange> {
    match body.first()? {
        Stmt::Expr(statement) if statement.value.is_string_literal_expr() => Some(statement.value.range()),
        _ => None,
    }
}

impl<'a> Lowering<'a> {
    /// The text of `range`.
    fn slice(&self, range: TextRange) -> &'a str {
        self.text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or_default()
    }

    /// Pushes the next node in preorder, a child of `place`'s parent, with only its range; the
    /// caller fills the rest. A node past the depth limit is truncated; the first node of a
    /// pending statement records it.
    fn push(
        &mut self,
        place: Place,
        child: Child<'_>,
        form: Form,
        range: TextRange,
    ) -> Result<u32, SourceFailure> {
        let key = self
            .keys
            .push(place.parent, child)
            .map_err(|error| failure(format!("cannot key a {form} block: {error}"), Some(range.start())))?;
        let id = u32::try_from(self.lowered.nodes.len())
            .map_err(|_| failure("the file has too many blocks", None))?;
        let name = match child {
            Child::Definition(names) => Some(names.join(".").into()),
            Child::Call { callee, .. } => Some(callee.text().into()),
            Child::Inline(_) | Child::Form(_) => None,
        };
        let truncated = place.depth + 1 >= MAX_DEPTH;
        let mut flags = Flags::NONE;
        if truncated {
            flags.insert(Flag::Truncated);
            self.lowered.truncated.push(Truncated {
                range: bytes(range),
                call: matches!(child, Child::Call { .. }),
            });
        }
        let stmt = match form.kind() {
            NodeKind::Part => None,
            _ => self.statement.take().map(bytes),
        };
        self.lowered.nodes.push(Node {
            key,
            parent: Some(place.parent),
            form,
            name,
            range: bytes(range),
            lines: self.lines.span(bytes(range)),
            head: None,
            body: None,
            doc: None,
            stmt,
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags,
            target: None,
        });
        Ok(id)
    }

    fn node(&mut self, id: u32) -> &mut Node {
        &mut self.lowered.nodes[id as usize]
    }

    /// The place inside `node`, unless lowering stops there.
    fn inside(&self, node: u32, place: Place) -> Option<Place> {
        let truncated = self.lowered.nodes[node as usize].flags.contains(Flag::Truncated);
        (!truncated).then(|| place.under(node))
    }

    /// Gives the decorator, default, and base calls lowered since `start` their definition.
    fn own(&mut self, start: usize, definition: u32) {
        for call in self.owned.split_off(start) {
            self.node(call).r#for = Some(definition);
        }
    }

    /// Runs `lower` on a stack with room for it, so deeply nested syntax can't overflow.
    fn grow<T>(&mut self, lower: impl FnOnce(&mut Self) -> T) -> T {
        stacker::maybe_grow(RED_ZONE, STACK_GROWTH, || lower(self))
    }
}

/// `start..end` as a range, empty when `end` comes first (which valid code never has).
fn span(start: TextSize, end: TextSize) -> TextRange {
    TextRange::new(start, end.max(start))
}
