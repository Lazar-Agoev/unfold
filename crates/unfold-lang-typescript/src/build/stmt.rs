//! Statements: blocks, control flow, loops, switches, exceptions, and variable lists.

use oxc_ast::ast::{
    ForStatementInit, ForStatementLeft, Statement, SwitchStatement, TryStatement, VariableDeclaration,
};
use oxc_parser::{Kind, Token};
use oxc_span::GetSpan;
use unfold_core::text::ByteRange;

use super::{Binding, Builder, Ctx, range};
use crate::syntax::{Cat, Label};

impl<'a> Builder<'a, '_> {
    /// A block-like list of statements under one node.
    pub(super) fn statements(&mut self, parent: u32, statements: &[Statement<'a>], ctx: Ctx) {
        for statement in statements {
            self.statement(parent, Label::Statements, statement, ctx);
        }
    }

    pub(super) fn statement(&mut self, parent: u32, label: Label, statement: &Statement<'a>, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        self.statement_inner(parent, label, statement, ctx);
        self.leave();
    }

    fn statement_inner(&mut self, parent: u32, label: Label, statement: &Statement<'a>, ctx: Ctx) {
        let span = range(statement.span());
        let flags = ctx.flags();
        match statement {
            Statement::BlockStatement(block) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.statements(node, &block.body, ctx);
            }
            Statement::BreakStatement(_) => {
                self.push(parent, label, Cat::Break, span, flags);
            }
            Statement::ContinueStatement(_) => {
                self.push(parent, label, Cat::Continue, span, flags);
            }
            Statement::DebuggerStatement(_) | Statement::EmptyStatement(_) => {
                self.push(parent, label, Cat::Other, span, flags);
            }
            Statement::DoWhileStatement(it) => {
                let node = self.push(parent, label, Cat::DoWhile, span, flags);
                self.statement(node, Label::Statement, &it.body, ctx);
                self.expression(node, Label::Expression, &it.test, ctx);
            }
            Statement::ExpressionStatement(it) => {
                let node = self.push(parent, label, Cat::ExpressionStatement, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            Statement::ForInStatement(it) => {
                let node = self.push(parent, label, Cat::ForIn, span, flags);
                self.for_left(node, &it.left, ctx);
                self.expression(node, Label::Expression, &it.right, ctx);
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            Statement::ForOfStatement(it) => {
                let node = self.push(parent, label, Cat::ForOf, span, flags);
                self.for_left(node, &it.left, ctx);
                self.expression(node, Label::Expression, &it.right, ctx);
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            Statement::ForStatement(it) => {
                let node = self.push(parent, label, Cat::For, span, flags);
                match &it.init {
                    Some(ForStatementInit::VariableDeclaration(list)) => {
                        self.variable_list(node, Label::Initializer, list, None, ctx);
                    }
                    Some(init) => {
                        if let Some(expression) = init.as_expression() {
                            self.expression(node, Label::Initializer, expression, ctx);
                        }
                    }
                    None => {}
                }
                if let Some(test) = &it.test {
                    self.expression(node, Label::Condition, test, ctx);
                }
                if let Some(update) = &it.update {
                    self.expression(node, Label::Incrementor, update, ctx);
                }
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            Statement::IfStatement(it) => {
                let node = self.push(parent, label, Cat::If, span, flags);
                self.expression(node, Label::Expression, &it.test, ctx);
                self.statement(node, Label::ThenStatement, &it.consequent, ctx);
                if let Some(alternate) = &it.alternate {
                    self.statement(node, Label::ElseStatement, alternate, ctx);
                }
            }
            Statement::LabeledStatement(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            Statement::ReturnStatement(it) => {
                let node = self.push(parent, label, Cat::Return, span, flags);
                if let Some(argument) = &it.argument {
                    self.expression(node, Label::Expression, argument, ctx);
                }
            }
            Statement::SwitchStatement(it) => self.switch(parent, label, it, ctx),
            Statement::ThrowStatement(it) => {
                let node = self.push(parent, label, Cat::Throw, span, flags);
                self.expression(node, Label::Expression, &it.argument, ctx);
            }
            Statement::TryStatement(it) => self.try_statement(parent, label, it, ctx),
            Statement::WhileStatement(it) => {
                let node = self.push(parent, label, Cat::While, span, flags);
                self.expression(node, Label::Expression, &it.test, ctx);
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            Statement::WithStatement(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Expression, &it.object, ctx);
                self.statement(node, Label::Statement, &it.body, ctx);
            }
            _ => {
                if let Some(declaration) = statement.as_declaration() {
                    self.declaration(parent, label, declaration, None, ctx);
                } else if let Some(module) = statement.as_module_declaration() {
                    self.module_declaration(parent, label, module, ctx);
                }
            }
        }
    }

    /// A `for…in`/`for…of` head: a declaration list or an assignment target.
    fn for_left(&mut self, parent: u32, left: &ForStatementLeft<'a>, ctx: Ctx) {
        match left {
            ForStatementLeft::VariableDeclaration(list) => {
                self.variable_list(parent, Label::Initializer, list, None, ctx);
            }
            _ => {
                if let Some(target) = left.as_assignment_target() {
                    self.assignment_target(parent, Label::Initializer, target, ctx);
                }
            }
        }
    }

    fn switch(&mut self, parent: u32, label: Label, it: &SwitchStatement<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        let node = self.push(parent, label, Cat::Switch, range(it.span), flags);
        self.expression(node, Label::Expression, &it.discriminant, ctx);
        // The case block spans the braces after the discriminant, even when it has no clauses.
        let open = self
            .token_from(it.discriminant.span().end, Kind::LCurly)
            .map_or(it.span.end, Token::start);
        let block = ByteRange {
            start: open.min(it.span.end),
            end: it.span.end,
        };
        let cases = self.push(node, Label::CaseBlock, Cat::Other, block, flags);
        for case in &it.cases {
            let cat = if case.test.is_some() {
                Cat::Case
            } else {
                Cat::Default
            };
            let clause = self.push(cases, Label::Other, cat, range(case.span), flags);
            if let Some(test) = &case.test {
                self.expression(clause, Label::Expression, test, ctx);
            }
            self.statements(clause, &case.consequent, ctx);
        }
    }

    fn try_statement(&mut self, parent: u32, label: Label, it: &TryStatement<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        let node = self.push(parent, label, Cat::Try, range(it.span), flags);
        let block = self.push(node, Label::TryBlock, Cat::Other, range(it.block.span), flags);
        self.statements(block, &it.block.body, ctx);
        if let Some(handler) = &it.handler {
            let clause = self.push(node, Label::CatchClause, Cat::Catch, range(handler.span), flags);
            if let Some(param) = &handler.param {
                let declaration = self.push(
                    clause,
                    Label::VariableDeclaration,
                    Cat::VariableDeclaration,
                    range(param.span),
                    flags,
                );
                let binding = ctx.binding(Binding::Variable);
                self.pattern(declaration, Label::Name, &param.pattern, binding);
                if let Some(annotation) = &param.type_annotation {
                    self.type_annotation(declaration, Label::Type, annotation);
                }
            }
            let body = self.push(clause, Label::Block, Cat::Other, range(handler.body.span), flags);
            self.statements(body, &handler.body.body, ctx);
        }
        if let Some(finalizer) = &it.finalizer {
            let block = self.push(
                node,
                Label::FinallyBlock,
                Cat::Other,
                range(finalizer.span),
                flags,
            );
            self.statements(block, &finalizer.body, ctx);
        }
    }

    /// A `var`/`let`/`const`/`using` list and its declarators. `start` overrides the list's
    /// start (for `export`).
    pub(super) fn variable_list(
        &mut self,
        parent: u32,
        label: Label,
        list: &VariableDeclaration<'a>,
        start: Option<u32>,
        ctx: Ctx,
    ) {
        let ctx = ctx.ambient(list.declare);
        let mut span = range(list.span);
        if let Some(start) = start {
            span.start = span.start.min(start);
        }
        let node = self.push(parent, label, Cat::Other, span, ctx.flags());
        let binding = ctx.binding(Binding::Variable);
        for declarator in &list.declarations {
            let declaration = self.push(
                node,
                Label::Other,
                Cat::VariableDeclaration,
                range(declarator.span),
                ctx.flags(),
            );
            self.pattern(declaration, Label::Name, &declarator.id, binding);
            if let Some(annotation) = &declarator.type_annotation {
                self.type_annotation(declaration, Label::Type, annotation);
            }
            if let Some(init) = &declarator.init {
                self.expression(declaration, Label::Initializer, init, ctx);
            }
        }
    }
}
