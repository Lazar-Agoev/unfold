//! The Python graph mapping: which nodes select which block kinds, their names, roles, ranges,
//! and slots (the proof of concept's `frontend/python/mapping.ts`, over Ruff's tree).
//!
//! | Syntax | Block | Slots |
//! | --- | --- | --- |
//! | module | `file` | |
//! | `def` (async too) | `function`; in a class body `method`, `constructor` (`__init__`), `getter` (proven `property`), `setter` (`@x.setter` of a proven property) | `name`, `parameter` (named ones), `return_type`, `body` |
//! | `lambda` | `function` | `parameter`, `body` |
//! | `class` | `class` | `name`, `heritage` (all bases and keywords) |
//! | assignment (each chain level), augmented or annotated assignment, `x: T`, `type X = ...`, `name := value` | `variable` | `name`, `initializer` |
//! | call | `call`, or `import` for a proven literal `importlib.import_module` | `callee`, `argument`; `specifier` |
//! | `import a, b` item, `from x import ...` | `import` | `specifier` |
//! | a name of `from x import a` that may import a child module | `import` (conditional), nested in the statement's | `specifier` (the name) |
//! | `if`, `elif` | `if` | `condition`, `then`, `else` |
//! | `for`, `async for` | `for_of` | `initializer`, `iterable`, `body`, `else` |
//! | `while` | `while` | `condition`, `body`, `else` |
//! | `try`, `try`/`except*` | `try` | `body`, `catch` (indexed), `else`, `finally` |
//! | `except`, `except*` | `catch` | `binding`, `body` |
//! | `with`, `async with` | `with` | `expression`, `body` |
//! | `match` | `match` | `expression`, `body` |
//! | `case` | `case` | `condition`, `body` |
//! | `return`, `raise` | `return`, `throw` | `expression` |
//! | `break`, `continue` | `break`, `continue` | |
//! | expression statement | `expression_statement` | `expression` |
//!
//! Decorators, parameter annotations and defaults, return annotations, type parameters, and
//! class bases have role `header`: they belong to the enclosing scope. Comprehensions select no
//! blocks. Suites (`body`, `then`, `else`, `finally`) run from their colon to their last
//! statement.

use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use unfold_core::text::{ByteRange, short_label};
use unfold_core::vocab::{BlockKind, Role, SlotName};
use unfold_engine::select::{MappedSlot, Mapping, Saved, Tree};

use crate::ast::arguments;
use crate::syntax::{Node, Part, Syntax};

/// A byte range from Ruff's range.
pub(crate) fn bytes(range: TextRange) -> ByteRange {
    ByteRange {
        start: range.start().to_u32(),
        end: range.end().to_u32(),
    }
}

impl<'a> Syntax<'a> {
    /// The suite of `body`: from its colon to the end of its last statement.
    fn suite(&self, body: &[Stmt]) -> Option<TextRange> {
        let first = body.first()?;
        let last = body.last()?;
        self.tokens.suite(first.start(), self.statement_end(last))
    }

    /// Where a statement ends as the proof of concept saw it: an assignment, return, or
    /// expression statement ends with its value, so an unparenthesized tuple's trailing comma
    /// is outside it.
    fn statement_end(&self, stmt: &Stmt) -> ruff_text_size::TextSize {
        let range = stmt.range();
        let last = match stmt {
            Stmt::Assign(node) => Some(&*node.value),
            Stmt::AugAssign(node) => Some(&*node.value),
            Stmt::AnnAssign(node) => node.value.as_deref(),
            Stmt::Return(node) => node.value.as_deref(),
            Stmt::Expr(node) => Some(&*node.value),
            Stmt::TypeAlias(node) => Some(&*node.value),
            _ => None,
        };
        last.map_or(range.end(), |value| self.range_in(value, range).end())
    }

    /// The range of a statement block.
    fn statement_range(&self, stmt: &Stmt) -> TextRange {
        TextRange::new(stmt.start(), self.statement_end(stmt))
    }

    /// The text of the range a target has inside `bound`, as a block name.
    fn target_name(&self, target: &Expr, bound: TextRange) -> Box<str> {
        short_label(self.slice(self.range_in(target, bound))).into()
    }

    /// The written module path of an import statement or item.
    pub(crate) fn module_range(&self, stmt: &ruff_python_ast::StmtImportFrom) -> Option<TextRange> {
        self.tokens
            .import_module_path(stmt.range.start(), stmt.module.as_ref().map(Ranged::end))
    }
}

fn add(out: &mut Vec<MappedSlot>, slot: SlotName, range: Option<TextRange>) {
    if let Some(range) = range.filter(|range| !range.is_empty()) {
        out.push(MappedSlot {
            slot,
            index: None,
            range: bytes(range),
        });
    }
}

fn add_indexed(out: &mut Vec<MappedSlot>, slot: SlotName, ranges: impl Iterator<Item = TextRange>) {
    for (index, range) in ranges.enumerate() {
        out.push(MappedSlot {
            slot,
            index: Some(index as u32),
            range: bytes(range),
        });
    }
}

/// The range from the first range's start to the last one's end.
fn span(mut ranges: impl Iterator<Item = TextRange>) -> Option<TextRange> {
    let first = ranges.next()?;
    let last = ranges.last().unwrap_or(first);
    Some(TextRange::new(first.start(), last.end()))
}

impl<'a> Mapping for Syntax<'a> {
    type Node = Node<'a>;
    type Slot = Part;

    fn range(&self, node: Node<'a>) -> ByteRange {
        let range = match node {
            Node::Module => self.module.range,
            Node::Stmt(stmt) => self.statement_range(stmt),
            Node::ExprStatement(stmt) => self.range_in(&stmt.value, stmt.range),
            Node::Assign(assign, _) => TextRange::new(
                assign.range.start(),
                self.range_in(&assign.value, assign.range).end(),
            ),
            Node::Elif(statement, index) => statement
                .elif_else_clauses
                .get(index)
                .map_or(statement.range, |clause| {
                    TextRange::new(clause.start(), statement.end())
                }),
            Node::Alias(alias) | Node::Child(alias) => alias.range,
            Node::Handler(handler) => handler.range,
            Node::Case(case) => case.range,
            Node::Expr(expr, widened) => match expr {
                // Pyright stripped the parentheses around calls and lambdas, as Ruff does.
                Expr::Call(_) | Expr::Lambda(_) => expr.range(),
                _ => widened,
            },
        };
        bytes(range)
    }

    fn kind(&self, node: Node<'a>) -> Option<BlockKind> {
        Some(match node {
            Node::Module => BlockKind::File,
            Node::Stmt(stmt) => match stmt {
                Stmt::FunctionDef(function) => self.proofs.function_kind(function),
                Stmt::ClassDef(_) => BlockKind::Class,
                Stmt::Return(_) => BlockKind::Return,
                Stmt::Raise(_) => BlockKind::Throw,
                Stmt::Break(_) => BlockKind::Break,
                Stmt::Continue(_) => BlockKind::Continue,
                Stmt::If(_) => BlockKind::If,
                Stmt::For(_) => BlockKind::ForOf,
                Stmt::While(_) => BlockKind::While,
                Stmt::Try(_) => BlockKind::Try,
                Stmt::With(_) => BlockKind::With,
                Stmt::Match(_) => BlockKind::Match,
                Stmt::AugAssign(_) | Stmt::AnnAssign(_) | Stmt::TypeAlias(_) => BlockKind::Variable,
                Stmt::ImportFrom(_) => BlockKind::Import,
                _ => return None,
            },
            Node::ExprStatement(_) => BlockKind::ExpressionStatement,
            Node::Assign(..) => BlockKind::Variable,
            Node::Elif(..) => BlockKind::If,
            Node::Alias(_) | Node::Child(_) => BlockKind::Import,
            Node::Handler(_) => BlockKind::Catch,
            Node::Case(_) => BlockKind::Case,
            Node::Expr(expr, _) => match expr {
                Expr::Call(call) if self.proofs.dynamic_import(call).is_some() => BlockKind::Import,
                Expr::Call(_) => BlockKind::Call,
                Expr::Lambda(_) => BlockKind::Function,
                Expr::Named(_) => BlockKind::Variable,
                _ => return None,
            },
        })
    }

    fn name(&self, node: Node<'a>, kind: BlockKind) -> Option<Box<str>> {
        if kind == BlockKind::Import {
            return None;
        }
        match node {
            Node::Stmt(stmt) => match stmt {
                Stmt::FunctionDef(function) => Some(self.slice(function.name.range).into()),
                Stmt::ClassDef(class) => Some(self.slice(class.name.range).into()),
                Stmt::AugAssign(node) => Some(self.target_name(&node.target, node.range)),
                Stmt::AnnAssign(node) => Some(self.target_name(&node.target, node.range)),
                Stmt::TypeAlias(node) => Some(self.target_name(&node.name, node.range)),
                _ => None,
            },
            Node::Assign(assign, level) => assign
                .targets
                .get(level)
                .map(|target| self.target_name(target, assign.range)),
            Node::Expr(Expr::Named(named), widened) => Some(self.target_name(&named.target, widened)),
            _ => None,
        }
    }

    fn role(
        &self,
        _parent: Saved<Node<'a>>,
        first_slot: Option<Part>,
        through_decorator: bool,
    ) -> Option<Role> {
        if through_decorator {
            return Some(Role::Header);
        }
        Some(match first_slot.unwrap_or(Part::Body) {
            Part::Body => Role::Body,
            Part::Header => Role::Header,
            Part::Name => Role::Name,
            Part::Condition => Role::Condition,
            Part::Then => Role::Then,
            Part::Else => Role::Else,
            Part::Initializer => Role::Initializer,
            Part::Iterable => Role::Iterable,
            Part::Catch => Role::Catch,
            Part::Finally => Role::Finally,
            Part::Expression => Role::Expression,
            Part::Binding => Role::Binding,
            Part::Callee => Role::Callee,
            Part::Argument => Role::Argument,
        })
    }

    fn slots(&self, node: Node<'a>, kind: BlockKind, out: &mut Vec<MappedSlot>) {
        match node {
            Node::Module => {}
            Node::Stmt(stmt) => self.statement_slots(stmt, out),
            Node::ExprStatement(stmt) => add(
                out,
                SlotName::Expression,
                Some(self.range_in(&stmt.value, stmt.range)),
            ),
            Node::Assign(assign, level) => {
                let target = assign
                    .targets
                    .get(level)
                    .map(|target| self.range_in(target, assign.range));
                add(out, SlotName::Name, target);
                let initializer = match level.checked_sub(1) {
                    Some(inner) => Some(bytes_range(self.range(Node::Assign(assign, inner)))),
                    None => Some(self.range_in(&assign.value, assign.range)),
                };
                add(out, SlotName::Initializer, initializer);
            }
            Node::Elif(statement, index) => {
                if let Some(clause) = statement.elif_else_clauses.get(index) {
                    add(
                        out,
                        SlotName::Condition,
                        clause.test.as_ref().map(|test| self.range_in(test, clause.range)),
                    );
                    add(out, SlotName::Then, self.suite(&clause.body));
                    add(out, SlotName::Else, self.else_range(statement, index + 1));
                }
            }
            Node::Alias(alias) | Node::Child(alias) => add(out, SlotName::Specifier, Some(alias.name.range)),
            Node::Handler(handler) => {
                let binding = match (&handler.name, &handler.type_) {
                    (Some(name), _) => Some(name.range),
                    (None, Some(kind)) => Some(self.range_in(kind, handler.range)),
                    (None, None) => None,
                };
                add(out, SlotName::Binding, binding);
                add(out, SlotName::Body, self.suite(&handler.body));
            }
            Node::Case(case) => {
                let header = self.case_header(case);
                let pattern = self.tokens.widen(case.pattern.range(), header);
                let end = case
                    .guard
                    .as_ref()
                    .map_or(pattern.end(), |guard| self.range_in(guard, case.range).end());
                add(
                    out,
                    SlotName::Condition,
                    Some(TextRange::new(pattern.start(), end)),
                );
                add(out, SlotName::Body, self.suite(&case.body));
            }
            Node::Expr(expr, widened) => self.expression_slots(expr, widened, kind, out),
        }
    }
}

/// A `ByteRange` back as a `TextRange`.
fn bytes_range(range: ByteRange) -> TextRange {
    TextRange::new(range.start.into(), range.end.into())
}

impl<'a> Syntax<'a> {
    /// The `else` slot of an `if` or `elif` whose next clause is at `index`: the next `elif`
    /// (nested `if`) or the `else` suite.
    fn else_range(&self, statement: &ruff_python_ast::StmtIf, index: usize) -> Option<TextRange> {
        let clause = statement.elif_else_clauses.get(index)?;
        match clause.test {
            Some(_) => Some(TextRange::new(clause.start(), statement.end())),
            None => self.suite(&clause.body),
        }
    }

    /// The part of a `case` its pattern's parentheses lie in: after `case`, before the guard
    /// or the colon.
    fn case_header(&self, case: &ruff_python_ast::MatchCase) -> TextRange {
        let end = match &case.guard {
            Some(guard) => guard.start(),
            None => case.body.first().map_or(case.end(), Ranged::start),
        };
        TextRange::new(case.start(), end)
    }

    fn statement_slots(&self, stmt: &Stmt, out: &mut Vec<MappedSlot>) {
        let range = stmt.range();
        match stmt {
            Stmt::FunctionDef(function) => {
                add(out, SlotName::Name, Some(function.name.range));
                add_indexed(
                    out,
                    SlotName::Parameter,
                    function.parameters.iter().map(|parameter| parameter.range()),
                );
                add(
                    out,
                    SlotName::ReturnType,
                    function
                        .returns
                        .as_ref()
                        .map(|returns| self.range_in(returns, range)),
                );
                add(out, SlotName::Body, self.suite(&function.body));
            }
            Stmt::ClassDef(class) => {
                add(out, SlotName::Name, Some(class.name.range));
                if let Some(written) = &class.arguments {
                    let interior = Self::interior(written.range);
                    add(
                        out,
                        SlotName::Heritage,
                        span(arguments(written).map(|argument| self.argument_range(argument, interior))),
                    );
                }
            }
            Stmt::AugAssign(node) => {
                add(out, SlotName::Name, Some(self.range_in(&node.target, range)));
                add(
                    out,
                    SlotName::Initializer,
                    Some(self.range_in(&node.value, range)),
                );
            }
            Stmt::AnnAssign(node) => {
                add(out, SlotName::Name, Some(self.range_in(&node.target, range)));
                add(
                    out,
                    SlotName::Initializer,
                    node.value.as_ref().map(|value| self.range_in(value, range)),
                );
            }
            Stmt::TypeAlias(node) => {
                add(out, SlotName::Name, Some(self.range_in(&node.name, range)));
                add(
                    out,
                    SlotName::Initializer,
                    Some(self.range_in(&node.value, range)),
                );
            }
            Stmt::If(node) => {
                add(out, SlotName::Condition, Some(self.range_in(&node.test, range)));
                add(out, SlotName::Then, self.suite(&node.body));
                add(out, SlotName::Else, self.else_range(node, 0));
            }
            Stmt::For(node) => {
                add(
                    out,
                    SlotName::Initializer,
                    Some(self.range_in(&node.target, range)),
                );
                add(out, SlotName::Iterable, Some(self.range_in(&node.iter, range)));
                add(out, SlotName::Body, self.suite(&node.body));
                add(out, SlotName::Else, self.suite(&node.orelse));
            }
            Stmt::While(node) => {
                add(out, SlotName::Condition, Some(self.range_in(&node.test, range)));
                add(out, SlotName::Body, self.suite(&node.body));
                add(out, SlotName::Else, self.suite(&node.orelse));
            }
            Stmt::Try(node) => {
                add(out, SlotName::Body, self.suite(&node.body));
                add_indexed(out, SlotName::Catch, node.handlers.iter().map(Ranged::range));
                add(out, SlotName::Else, self.suite(&node.orelse));
                add(out, SlotName::Finally, self.suite(&node.finalbody));
            }
            Stmt::With(node) => {
                add(
                    out,
                    SlotName::Expression,
                    span(node.items.iter().map(Ranged::range)),
                );
                add(out, SlotName::Body, self.suite(&node.body));
            }
            Stmt::Match(node) => {
                add(
                    out,
                    SlotName::Expression,
                    Some(self.range_in(&node.subject, range)),
                );
                add(out, SlotName::Body, span(node.cases.iter().map(Ranged::range)));
            }
            Stmt::Return(node) => add(
                out,
                SlotName::Expression,
                node.value.as_ref().map(|value| self.range_in(value, range)),
            ),
            Stmt::Raise(node) => add(
                out,
                SlotName::Expression,
                node.exc.as_ref().map(|exc| self.range_in(exc, range)),
            ),
            Stmt::ImportFrom(node) => add(out, SlotName::Specifier, self.module_range(node)),
            _ => {}
        }
    }

    /// The range of one written argument: a positional value widened inside the list, a
    /// starred argument with its star, a keyword with its name or `**`.
    pub(crate) fn argument_range(
        &self,
        argument: ruff_python_ast::ArgOrKeyword<'_>,
        interior: TextRange,
    ) -> TextRange {
        match argument {
            ruff_python_ast::ArgOrKeyword::Arg(expr) => self.range_in(expr, interior),
            ruff_python_ast::ArgOrKeyword::Keyword(keyword) => keyword.range,
        }
    }

    fn expression_slots(&self, expr: &Expr, widened: TextRange, kind: BlockKind, out: &mut Vec<MappedSlot>) {
        let range = expr.range();
        match expr {
            Expr::Call(call) => {
                let interior = Self::interior(call.arguments.range);
                if kind == BlockKind::Import {
                    let first = arguments(&call.arguments).next().map(|argument| match argument {
                        ruff_python_ast::ArgOrKeyword::Arg(value) => self.range_in(value, interior),
                        ruff_python_ast::ArgOrKeyword::Keyword(keyword) => {
                            self.range_in(&keyword.value, keyword.range)
                        }
                    });
                    add(out, SlotName::Specifier, first);
                    return;
                }
                add(out, SlotName::Callee, Some(self.range_in(&call.func, range)));
                add_indexed(
                    out,
                    SlotName::Argument,
                    arguments(&call.arguments).map(|argument| self.argument_range(argument, interior)),
                );
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    add_indexed(
                        out,
                        SlotName::Parameter,
                        parameters.iter().map(|parameter| parameter.range()),
                    );
                }
                add(out, SlotName::Body, Some(self.range_in(&lambda.body, range)));
            }
            Expr::Named(named) => {
                add(out, SlotName::Name, Some(self.range_in(&named.target, widened)));
                add(
                    out,
                    SlotName::Initializer,
                    Some(self.range_in(&named.value, range)),
                );
            }
            _ => {}
        }
    }
}

impl<'a> Tree for Syntax<'a> {
    fn children(&self, node: Node<'a>, out: &mut Vec<(Part, Node<'a>)>) {
        Syntax::children(self, node, out);
    }
}
