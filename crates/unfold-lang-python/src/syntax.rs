//! The syntax tree as block selection sees it: node handles over Ruff's tree, and each node's
//! children with the labels roles come from.
//!
//! Ruff's tree and the proof of concept's (Pyright's) differ in shape, so a few nodes are
//! synthetic, chosen so that selection, roles, and keys come out as they did in the proof of
//! concept:
//!
//! | Node | Stands for |
//! | --- | --- |
//! | [`Node::ExprStatement`] | A standalone expression (Pyright wrapped these; an assignment expression statement is not wrapped) |
//! | [`Node::Assign`] | One target of `a = b = value`, nested like Pyright's chain: the last target outermost, each level's initializer the next |
//! | [`Node::Elif`] | An `elif` clause, which Pyright nests as an `if` in the `else` branch, running to the end of the chain |
//! | [`Node::Alias`] | One item of `import a, b` |
//! | [`Node::Child`] | One name of `from m import a, b`, tentatively a named child import (see [`is_child_candidate`]) |
//!
//! Every expression handle carries its proof-of-concept range: Ruff's range widened over the
//! parentheses that enclose it inside its parent ([`crate::tokens::Tokens::widen`]).

use ruff_python_ast::{
    Alias, ArgOrKeyword, ExceptHandlerExceptHandler, Expr, MatchCase, ModModule, Parameters, Stmt,
    StmtAssign, StmtExpr, StmtIf, TypeParam, TypeParams,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::analysis::Proofs;
use crate::ast::{arguments, expr_children};
use crate::tokens::Tokens;

/// A node handle.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Node<'a> {
    /// The module: the file root.
    Module,
    /// A statement that is a node of its own.
    Stmt(&'a Stmt),
    /// A standalone expression statement.
    ExprStatement(&'a StmtExpr),
    /// Level `n` of an assignment chain (`targets[n]` is this level's target).
    Assign(&'a StmtAssign, usize),
    /// The `elif` clause at this index.
    Elif(&'a StmtIf, usize),
    /// An `import` item.
    Alias(&'a Alias),
    /// A name of a `from` import that may import a child module; its block is conditional.
    Child(&'a Alias),
    /// An `except` or `except*` handler.
    Handler(&'a ExceptHandlerExceptHandler),
    /// A `match` case.
    Case(&'a MatchCase),
    /// An expression with its proof-of-concept range.
    Expr(&'a Expr, TextRange),
}

/// The label of the edge from a node to a child, as the proof of concept named the child's
/// field. Roles are the first label on the path leaving a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Body,
    Header,
    Name,
    Condition,
    Then,
    Else,
    Initializer,
    Iterable,
    Catch,
    Finally,
    Expression,
    Binding,
    Callee,
    Argument,
}

/// One file's syntax with what selection needs to read it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Syntax<'a> {
    /// The file's text.
    pub(crate) text: &'a str,
    /// The parsed module.
    pub(crate) module: &'a ModModule,
    /// The parser's tokens.
    pub(crate) tokens: Tokens<'a>,
    /// The pre-pass's proofs.
    pub(crate) proofs: &'a Proofs,
}

type Children<'a> = Vec<(Part, Node<'a>)>;

impl<'a> Syntax<'a> {
    /// The text of `range`.
    pub(crate) fn slice(&self, range: TextRange) -> &'a str {
        self.text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or_default()
    }

    /// The proof-of-concept range of `expr` inside `bound`: an unparenthesized tuple ends at
    /// its last element (not a trailing comma), and the range widens over the parentheses that
    /// enclose it inside `bound`.
    pub(crate) fn range_in(&self, expr: &Expr, bound: TextRange) -> TextRange {
        let mut range = expr.range();
        if let Expr::Tuple(tuple) = expr
            && !tuple.parenthesized
            && let Some(last) = tuple.elts.last()
        {
            range = TextRange::new(range.start(), self.tokens.widen(last.range(), range).end());
        }
        self.tokens.widen(range, bound)
    }

    /// An expression node inside `bound`.
    pub(crate) fn expr(&self, expr: &'a Expr, bound: TextRange) -> Node<'a> {
        Node::Expr(expr, self.range_in(expr, bound))
    }

    /// The interior of a parenthesized list (`(` ... `)`), where its items' own parentheses are.
    pub(crate) fn interior(range: TextRange) -> TextRange {
        if range.len().to_u32() >= 2 {
            TextRange::new(range.start() + TextSize::new(1), range.end() - TextSize::new(1))
        } else {
            range
        }
    }

    /// The bound a `with` statement's items widen within: inside the item list's parentheses
    /// when it has them, else the statement.
    pub(crate) fn with_bound(&self, statement: TextRange) -> TextRange {
        self.tokens
            .with_list_parens(statement.start())
            .map_or(statement, Self::interior)
    }

    /// Appends `node`'s children in source order.
    pub(crate) fn children(&self, node: Node<'a>, out: &mut Children<'a>) {
        match node {
            Node::Module => self.statements(Part::Body, &self.module.body, out),
            Node::Stmt(stmt) => self.stmt_children(stmt, out),
            Node::ExprStatement(stmt) => out.push((Part::Expression, self.expr(&stmt.value, stmt.range))),
            Node::Assign(assign, level) => {
                if let Some(target) = assign.targets.get(level) {
                    out.push((Part::Name, self.expr(target, assign.range)));
                }
                let initializer = match level.checked_sub(1) {
                    Some(inner) => Node::Assign(assign, inner),
                    None => self.expr(&assign.value, assign.range),
                };
                out.push((Part::Initializer, initializer));
            }
            Node::Elif(statement, index) => self.clause(statement, index, out),
            Node::Alias(_) | Node::Child(_) => {}
            Node::Handler(handler) => {
                if let Some(kind) = &handler.type_ {
                    out.push((Part::Binding, self.expr(kind, handler.range)));
                }
                self.statements(Part::Body, &handler.body, out);
            }
            Node::Case(case) => {
                if let Some(guard) = &case.guard {
                    out.push((Part::Condition, self.expr(guard, case.range)));
                }
                self.statements(Part::Body, &case.body, out);
            }
            Node::Expr(expr, _) => self.expr_children(expr, out),
        }
    }

    /// The statements of a body. Statements that are not nodes of their own contribute their
    /// pieces directly: the items of `import a, b`, an assignment chain's outermost level, an
    /// assignment expression used as a statement. Imports a proven `TYPE_CHECKING` guard erases
    /// contribute nothing.
    fn statements(&self, part: Part, body: &'a [Stmt], out: &mut Children<'a>) {
        for stmt in body {
            match stmt {
                Stmt::Expr(statement) => match &*statement.value {
                    Expr::Named(_) => out.push((part, self.expr(&statement.value, statement.range))),
                    _ => out.push((part, Node::ExprStatement(statement))),
                },
                Stmt::Assign(assign) => {
                    if let Some(last) = assign.targets.len().checked_sub(1) {
                        out.push((part, Node::Assign(assign, last)));
                    }
                }
                Stmt::Import(import) => {
                    if !self.proofs.is_erased(import.range.start()) {
                        out.extend(import.names.iter().map(|alias| (part, Node::Alias(alias))));
                    }
                }
                Stmt::ImportFrom(import) => {
                    if !self.proofs.is_erased(import.range.start()) {
                        out.push((part, Node::Stmt(stmt)));
                    }
                }
                Stmt::Pass(_) | Stmt::Global(_) | Stmt::Nonlocal(_) | Stmt::IpyEscapeCommand(_) => {}
                _ => out.push((part, Node::Stmt(stmt))),
            }
        }
    }

    /// The children of the `elif` clause at `index`: its test, its then-branch, and the clause
    /// after it.
    fn clause(&self, statement: &'a StmtIf, index: usize, out: &mut Children<'a>) {
        let Some(clause) = statement.elif_else_clauses.get(index) else {
            return;
        };
        if let Some(test) = &clause.test {
            out.push((Part::Condition, self.expr(test, clause.range)));
        }
        self.statements(Part::Then, &clause.body, out);
        self.next_clause(statement, index + 1, out);
    }

    /// The clause at `index` as the `else` of the one before it.
    fn next_clause(&self, statement: &'a StmtIf, index: usize, out: &mut Children<'a>) {
        match statement.elif_else_clauses.get(index) {
            Some(clause) if clause.test.is_some() => out.push((Part::Else, Node::Elif(statement, index))),
            Some(clause) => self.statements(Part::Else, &clause.body, out),
            None => {}
        }
    }

    fn stmt_children(&self, stmt: &'a Stmt, out: &mut Children<'a>) {
        let range = stmt.range();
        match stmt {
            Stmt::FunctionDef(function) => {
                for decorator in &function.decorator_list {
                    out.push((Part::Header, self.expr(&decorator.expression, decorator.range)));
                }
                self.type_parameters(Part::Header, function.type_params.as_deref(), out);
                self.parameters(&function.parameters, out);
                if let Some(returns) = &function.returns {
                    out.push((Part::Header, self.expr(returns, range)));
                }
                self.statements(Part::Body, &function.body, out);
            }
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    out.push((Part::Header, self.expr(&decorator.expression, decorator.range)));
                }
                self.type_parameters(Part::Header, class.type_params.as_deref(), out);
                if let Some(written) = &class.arguments {
                    let interior = Self::interior(written.range);
                    for argument in arguments(written) {
                        out.push((Part::Header, self.argument(argument, interior)));
                    }
                }
                self.statements(Part::Body, &class.body, out);
            }
            Stmt::Return(node) => {
                if let Some(value) = &node.value {
                    out.push((Part::Expression, self.expr(value, range)));
                }
            }
            Stmt::Delete(node) => out.extend(
                node.targets
                    .iter()
                    .map(|target| (Part::Body, self.expr(target, range))),
            ),
            Stmt::TypeAlias(node) => {
                out.push((Part::Body, self.expr(&node.name, range)));
                self.type_parameters(Part::Body, node.type_params.as_deref(), out);
                out.push((Part::Expression, self.expr(&node.value, range)));
            }
            Stmt::AugAssign(node) => {
                out.push((Part::Name, self.expr(&node.target, range)));
                out.push((Part::Initializer, self.expr(&node.value, range)));
            }
            Stmt::AnnAssign(node) => {
                out.push((Part::Name, self.expr(&node.target, range)));
                // Pyright nested the annotation in the assignment's target when there is a
                // value, and made it the statement's own annotation when there is none.
                let annotation = if node.value.is_some() {
                    Part::Name
                } else {
                    Part::Header
                };
                out.push((annotation, self.expr(&node.annotation, range)));
                if let Some(value) = &node.value {
                    out.push((Part::Initializer, self.expr(value, range)));
                }
            }
            Stmt::For(node) => {
                out.push((Part::Initializer, self.expr(&node.target, range)));
                out.push((Part::Iterable, self.expr(&node.iter, range)));
                self.statements(Part::Body, &node.body, out);
                self.statements(Part::Else, &node.orelse, out);
            }
            Stmt::While(node) => {
                out.push((Part::Condition, self.expr(&node.test, range)));
                self.statements(Part::Body, &node.body, out);
                self.statements(Part::Else, &node.orelse, out);
            }
            Stmt::If(node) => {
                out.push((Part::Condition, self.expr(&node.test, range)));
                self.statements(Part::Then, &node.body, out);
                self.next_clause(node, 0, out);
            }
            Stmt::With(node) => {
                let bound = self.with_bound(range);
                for item in &node.items {
                    out.push((Part::Expression, self.expr(&item.context_expr, bound)));
                    if let Some(target) = &item.optional_vars {
                        out.push((Part::Expression, self.expr(target, bound)));
                    }
                }
                self.statements(Part::Body, &node.body, out);
            }
            Stmt::Match(node) => {
                out.push((Part::Expression, self.expr(&node.subject, range)));
                out.extend(node.cases.iter().map(|case| (Part::Body, Node::Case(case))));
            }
            Stmt::Raise(node) => {
                if let Some(exception) = &node.exc {
                    out.push((Part::Expression, self.expr(exception, range)));
                }
                if let Some(cause) = &node.cause {
                    out.push((Part::Body, self.expr(cause, range)));
                }
            }
            Stmt::Try(node) => {
                self.statements(Part::Body, &node.body, out);
                for ruff_python_ast::ExceptHandler::ExceptHandler(handler) in &node.handlers {
                    out.push((Part::Catch, Node::Handler(handler)));
                }
                self.statements(Part::Else, &node.orelse, out);
                self.statements(Part::Finally, &node.finalbody, out);
            }
            Stmt::Assert(node) => {
                out.push((Part::Condition, self.expr(&node.test, range)));
                if let Some(message) = &node.msg {
                    out.push((Part::Body, self.expr(message, range)));
                }
            }
            Stmt::Expr(node) => out.push((Part::Expression, self.expr(&node.value, range))),
            Stmt::ImportFrom(import) => out.extend(
                import
                    .names
                    .iter()
                    .filter(|alias| is_child_candidate(alias))
                    .map(|alias| (Part::Body, Node::Child(alias))),
            ),
            Stmt::Assign(_)
            | Stmt::Import(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::IpyEscapeCommand(_) => {}
        }
    }

    /// A call or class argument's value node: a positional argument widens inside the list's
    /// parentheses, a keyword's value inside the keyword.
    pub(crate) fn argument(&self, argument: ArgOrKeyword<'a>, interior: TextRange) -> Node<'a> {
        match argument {
            ArgOrKeyword::Arg(expr) => self.expr(expr, interior),
            ArgOrKeyword::Keyword(keyword) => self.expr(&keyword.value, keyword.range),
        }
    }

    fn type_parameters(&self, part: Part, parameters: Option<&'a TypeParams>, out: &mut Children<'a>) {
        let Some(parameters) = parameters else {
            return;
        };
        for parameter in &parameters.type_params {
            let (bound, default) = match parameter {
                TypeParam::TypeVar(node) => (node.bound.as_deref(), node.default.as_deref()),
                TypeParam::ParamSpec(node) => (None, node.default.as_deref()),
                TypeParam::TypeVarTuple(node) => (None, node.default.as_deref()),
            };
            let range = parameter.range();
            out.extend(
                bound
                    .into_iter()
                    .chain(default)
                    .map(|expr| (part, self.expr(expr, range))),
            );
        }
    }

    /// Parameter annotations and defaults, which belong to the enclosing scope (`header`).
    fn parameters(&self, parameters: &'a Parameters, out: &mut Children<'a>) {
        for parameter in parameters {
            let range = parameter.range();
            if let Some(annotation) = parameter.annotation() {
                out.push((Part::Header, self.expr(annotation, range)));
            }
            if let Some(default) = parameter.default() {
                out.push((Part::Header, self.expr(default, range)));
            }
        }
    }

    fn expr_children(&self, expr: &'a Expr, out: &mut Children<'a>) {
        let range = expr.range();
        match expr {
            Expr::Call(call) => {
                // A proven dynamic import is an `import` block; its callee and literal argument
                // hold no blocks.
                if self.proofs.dynamic_import(call).is_some() {
                    return;
                }
                out.push((Part::Callee, self.expr(&call.func, range)));
                let interior = Self::interior(call.arguments.range);
                for argument in arguments(&call.arguments) {
                    out.push((Part::Argument, self.argument(argument, interior)));
                }
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters {
                        if let Some(default) = parameter.default() {
                            out.push((Part::Header, self.expr(default, parameter.range())));
                        }
                    }
                }
                out.push((Part::Body, self.expr(&lambda.body, range)));
            }
            Expr::Named(named) => {
                out.push((Part::Body, self.expr(&named.target, range)));
                out.push((Part::Initializer, self.expr(&named.value, range)));
            }
            _ => {
                expr_children(
                    expr,
                    &mut BodyChildren {
                        syntax: self,
                        bound: range,
                        out,
                    },
                );
            }
        }
    }
}

/// Collects child expressions as `body` children widened inside `bound`, without an
/// intermediate list.
struct BodyChildren<'s, 'a> {
    syntax: &'s Syntax<'a>,
    bound: TextRange,
    out: &'s mut Children<'a>,
}

impl<'a> Extend<&'a Expr> for BodyChildren<'_, 'a> {
    fn extend<I: IntoIterator<Item = &'a Expr>>(&mut self, children: I) {
        for child in children {
            self.out.push((Part::Body, self.syntax.expr(child, self.bound)));
        }
    }
}

/// Whether a name of a runtime `from` import may import a child module of its package: anything
/// but `*` and dunder names (`__x__`), which the import system never loads as submodules from a
/// from-list. Whether it does depends on other files, so its block is conditional and linking
/// decides.
pub(crate) fn is_child_candidate(alias: &Alias) -> bool {
    let name = alias.name.id.as_str();
    name != "*" && !(name.len() >= 4 && name.starts_with("__") && name.ends_with("__"))
}
