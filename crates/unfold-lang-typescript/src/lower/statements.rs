//! Statements: decisions, `switch`, loops, `try`, `using`, exits, labels, and variable
//! declarations.

use oxc_allocator::UnstableAddress;
use oxc_ast::ast::{
    BindingPattern, BreakStatement, ContinueStatement, DoWhileStatement, Expression, ForInStatement,
    ForOfStatement, ForStatement, IfStatement, LabeledStatement, Statement, SwitchStatement, TryStatement,
    VariableDeclaration,
};
use oxc_ast_visit::Visit;
use oxc_parser::Kind;
use oxc_span::GetSpan;
use unfold_core::key::Child;
use unfold_core::text::ByteRange;
use unfold_core::vocab::{Flag, Form};

use super::{Breakable, Breaks, Lowering, Within, forms};
use crate::spans::range;

impl Lowering<'_> {
    /// `if` / `else if` / `else`: one decision whose parts are `then`, `elif[n]`, and `else`.
    pub(super) fn if_statement(&mut self, it: &IfStatement<'_>) {
        let Some(decision) = self.push(
            self.at.parent,
            Child::Form(Form::If),
            Form::If,
            None,
            range(it.span),
        ) else {
            return;
        };
        let mut current = it;
        let mut form = Form::Then;
        let mut start = it.span.start;
        loop {
            let consequent = range(current.consequent.span());
            let part_range = ByteRange {
                start,
                end: consequent.end,
            };
            let test = range(current.test.span());
            if let Some(part) = self.part(decision, form, part_range, Some(test), Some(consequent)) {
                self.within(part, Within::Test, |lowering| {
                    lowering.visit_expression(&current.test);
                });
                self.within(part, Within::Body, |lowering| {
                    lowering.visit_statement(&current.consequent);
                });
            }
            let Some(alternate) = &current.alternate else {
                return;
            };
            let alternate_range = range(alternate.span());
            start = self
                .tokens
                .find_back(alternate_range.start, consequent.end, Kind::Else)
                .map_or(alternate_range.start, |token| token.start());
            if let Statement::IfStatement(next) = alternate {
                current = next;
                form = Form::Elif;
                continue;
            }
            let part_range = ByteRange {
                start,
                end: alternate_range.end,
            };
            if let Some(part) = self.part(decision, Form::Else, part_range, None, Some(alternate_range)) {
                self.within(part, Within::Body, |lowering| {
                    lowering.visit_statement(alternate);
                });
            }
            return;
        }
    }

    /// `switch`: a decision whose header is the discriminant and whose parts are its cases, in
    /// written order. A case runs into the next one unless it ends in `break`, `return`,
    /// `throw`, or `continue`; a `break` that ends a case is no node.
    pub(super) fn switch_statement(&mut self, it: &SwitchStatement<'_>) {
        let labels = std::mem::take(&mut self.labels);
        let span = range(it.span);
        let Some(decision) = self.push(
            self.at.parent,
            Child::Form(Form::Switch),
            Form::Switch,
            None,
            span,
        ) else {
            return;
        };
        self.node(decision).head = Some(range(it.discriminant.span()));
        self.within(decision, Within::Header, |lowering| {
            lowering.visit_expression(&it.discriminant);
        });
        self.breakables.push(Breakable {
            breaks: Breaks::Switch(decision),
            labels: labels.clone(),
        });
        let last = it.cases.len().saturating_sub(1);
        for (index, case) in it.cases.iter().enumerate() {
            let form = if case.test.is_some() {
                Form::Case
            } else {
                Form::Default
            };
            let body = match (case.consequent.first(), case.consequent.last()) {
                (Some(first), Some(last)) => Some(ByteRange {
                    start: first.span().start,
                    end: last.span().end,
                }),
                _ => None,
            };
            let test = case.test.as_ref().map(|test| range(test.span()));
            let Some(part) = self.part(decision, form, range(case.span), test, body) else {
                continue;
            };
            let ends = match final_statement(&case.consequent) {
                Some(Statement::BreakStatement(stop)) => {
                    let own = stop
                        .label
                        .as_ref()
                        .is_none_or(|label| labels.iter().any(|own| **own == *label.name));
                    if own {
                        self.final_breaks.insert(stop.unstable_address());
                    }
                    true
                }
                Some(
                    Statement::ReturnStatement(_)
                    | Statement::ThrowStatement(_)
                    | Statement::ContinueStatement(_),
                ) => true,
                _ => false,
            };
            if !ends && index < last {
                self.node(part).flags.insert(Flag::FallsThrough);
            }
            if let Some(test) = &case.test {
                self.within(part, Within::Test, |lowering| {
                    lowering.visit_expression(test);
                });
            }
            self.within(part, Within::Body, |lowering| {
                lowering.statements(&case.consequent);
            });
        }
        self.breakables.pop();
    }

    /// A loop of `form` spanning `span` with header `head`, whose code `header` lowers; its
    /// body is its `body` part. A `do … while` has its header after its body.
    fn loop_statement(
        &mut self,
        form: Form,
        span: oxc_span::Span,
        head: ByteRange,
        body: &Statement<'_>,
        header: impl FnOnce(&mut Self),
    ) {
        let labels = std::mem::take(&mut self.labels);
        let Some(control) = self.push(self.at.parent, Child::Form(form), form, None, range(span)) else {
            return;
        };
        self.node(control).head = Some(head);
        if form == Form::Do {
            self.loop_body(control, labels, body);
            self.within(control, Within::Header, header);
        } else {
            self.within(control, Within::Header, header);
            self.loop_body(control, labels, body);
        }
    }

    /// The `body` part of loop `control`, which `break` and `continue` (by its `labels`) leave.
    fn loop_body(&mut self, control: u32, labels: Vec<Box<str>>, body: &Statement<'_>) {
        self.breakables.push(Breakable {
            breaks: Breaks::Loop(control),
            labels,
        });
        let body_range = range(body.span());
        if let Some(part) = self.part(control, Form::Body, body_range, None, Some(body_range)) {
            self.within(part, Within::Body, |lowering| {
                lowering.visit_statement(body);
            });
        }
        self.breakables.pop();
    }

    /// The header of a `for` loop: from `for` to the `)` before its body.
    fn for_head(&self, span: oxc_span::Span, body: &Statement<'_>) -> ByteRange {
        let body = body.span();
        let end = self
            .tokens
            .find_back(body.start, span.start, Kind::RParen)
            .map_or(body.start, |token| token.end());
        ByteRange {
            start: span.start,
            end,
        }
    }

    pub(super) fn for_statement(&mut self, it: &ForStatement<'_>) {
        let head = self.for_head(it.span, &it.body);
        self.loop_statement(Form::For, it.span, head, &it.body, |lowering| {
            if let Some(init) = &it.init {
                lowering.visit_for_statement_init(init);
            }
            if let Some(test) = &it.test {
                lowering.visit_expression(test);
            }
            if let Some(update) = &it.update {
                lowering.visit_expression(update);
            }
        });
    }

    pub(super) fn for_in_statement(&mut self, it: &ForInStatement<'_>) {
        let head = self.for_head(it.span, &it.body);
        self.loop_statement(Form::For, it.span, head, &it.body, |lowering| {
            lowering.visit_for_statement_left(&it.left);
            lowering.visit_expression(&it.right);
        });
    }

    pub(super) fn for_of_statement(&mut self, it: &ForOfStatement<'_>) {
        let head = self.for_head(it.span, &it.body);
        self.loop_statement(Form::For, it.span, head, &it.body, |lowering| {
            lowering.visit_for_statement_left(&it.left);
            lowering.visit_expression(&it.right);
        });
    }

    pub(super) fn while_statement(&mut self, it: &oxc_ast::ast::WhileStatement<'_>) {
        let test = it.test.span();
        let end = self
            .tokens
            .find(test.end, Kind::RParen)
            .map_or(test.end, |token| token.end());
        let head = ByteRange {
            start: it.span.start,
            end,
        };
        self.loop_statement(Form::While, it.span, head, &it.body, |lowering| {
            lowering.visit_expression(&it.test);
        });
    }

    pub(super) fn do_while_statement(&mut self, it: &DoWhileStatement<'_>) {
        let body = it.body.span();
        let test = it.test.span();
        let start = self
            .tokens
            .find(body.end, Kind::While)
            .map_or(test.start, |token| token.start());
        let end = self
            .tokens
            .find(test.end, Kind::RParen)
            .map_or(test.end, |token| token.end());
        self.loop_statement(
            Form::Do,
            it.span,
            ByteRange { start, end },
            &it.body,
            |lowering| {
                lowering.visit_expression(&it.test);
            },
        );
    }

    /// `try`: its `body`, its `catch` (the clause through the binding's `)`), and its
    /// `finally`.
    pub(super) fn try_statement(&mut self, it: &TryStatement<'_>) {
        let Some(control) = self.push(
            self.at.parent,
            Child::Form(Form::Try),
            Form::Try,
            None,
            range(it.span),
        ) else {
            return;
        };
        let block = range(it.block.span);
        if let Some(part) = self.part(control, Form::Body, block, None, Some(block)) {
            self.within(part, Within::Body, |lowering| {
                lowering.statements(&it.block.body);
            });
        }
        if let Some(handler) = &it.handler {
            let clause_end = match &handler.param {
                Some(parameter) => self
                    .tokens
                    .find(parameter.span.end, Kind::RParen)
                    .map_or(parameter.span.end, |token| token.end()),
                None => handler.span.start + "catch".len() as u32,
            };
            let clause = ByteRange {
                start: handler.span.start,
                end: clause_end,
            };
            let body = range(handler.body.span);
            if let Some(part) = self.part(
                control,
                Form::Catch,
                range(handler.span),
                Some(clause),
                Some(body),
            ) {
                if let Some(parameter) = &handler.param {
                    self.typed.annotation(parameter.type_annotation.as_deref(), part);
                }
                self.within(part, Within::Body, |lowering| {
                    if let Some(parameter) = &handler.param {
                        lowering.visit_binding_pattern(&parameter.pattern);
                    }
                    lowering.statements(&handler.body.body);
                });
            }
        }
        if let Some(finalizer) = &it.finalizer {
            let block = range(finalizer.span);
            let start = self
                .tokens
                .find_back(block.start, it.block.span.end, Kind::Finally)
                .map_or(block.start, |token| token.start());
            let part_range = ByteRange {
                start,
                end: block.end,
            };
            if let Some(part) = self.part(control, Form::Finally, part_range, None, Some(block)) {
                self.within(part, Within::Body, |lowering| {
                    lowering.statements(&finalizer.body);
                });
            }
        }
    }

    /// `using` and `await using`: a `with` block whose header is the declaration and whose
    /// body is the rest of the statements after it.
    pub(super) fn using(&mut self, declaration: &VariableDeclaration<'_>, rest: &[Statement<'_>]) {
        let head = range(declaration.span);
        let rest_range = match (rest.first(), rest.last()) {
            (Some(first), Some(last)) => Some(ByteRange {
                start: first.span().start,
                end: last.span().end,
            }),
            _ => None,
        };
        let span = ByteRange {
            start: head.start,
            end: rest_range.map_or(head.end, |rest| rest.end),
        };
        let enclosing = self.enclosing.replace(head);
        self.statement = Some(head);
        let control = self.push(self.at.parent, Child::Form(Form::Using), Form::Using, None, span);
        let Some(control) = control else {
            self.statement = None;
            self.enclosing = enclosing;
            return;
        };
        let node = self.node(control);
        node.head = Some(head);
        if declaration.kind.is_await() {
            node.flags.insert(Flag::Async);
        }
        self.within(control, Within::Header, |lowering| {
            lowering.declarators(declaration);
        });
        self.statement = None;
        self.enclosing = enclosing;
        let part = self.part(control, Form::Body, rest_range.unwrap_or(head), None, rest_range);
        if let Some(part) = part {
            self.within(part, Within::Body, |lowering| {
                lowering.statements(rest);
            });
        }
    }

    /// A labeled statement: a loop or `switch` takes the label; any other statement is a
    /// plain one a `break` may leave.
    pub(super) fn labeled_statement(&mut self, it: &LabeledStatement<'_>) {
        self.labels.push(it.label.name.as_str().into());
        match &it.body {
            Statement::ForStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::WhileStatement(_)
            | Statement::DoWhileStatement(_)
            | Statement::SwitchStatement(_)
            | Statement::LabeledStatement(_) => self.visit_statement(&it.body),
            body => {
                let labels = std::mem::take(&mut self.labels);
                self.breakables.push(Breakable {
                    breaks: Breaks::Plain,
                    labels,
                });
                self.visit_statement(body);
                self.breakables.pop();
            }
        }
    }

    /// An exit of `form` written at `span` (its whole statement), holding `value`.
    pub(super) fn exit(
        &mut self,
        form: Form,
        span: ByteRange,
        value: Option<&Expression<'_>>,
        target: Option<u32>,
    ) {
        let Some(exit) = self.push(self.at.parent, Child::Form(form), form, None, span) else {
            return;
        };
        let statement = self
            .enclosing
            .filter(|statement| statement.contains(span))
            .unwrap_or(span);
        let node = self.node(exit);
        node.stmt = Some(statement);
        node.value = value.map(|value| range(value.span()));
        node.target = target;
        if let Some(value) = value {
            if let Some(address) = forms::value_node(value) {
                self.exit_values.insert(address);
            }
            self.within(exit, Within::Value, |lowering| {
                lowering.visit_expression(value);
            });
        }
    }

    /// `break`: an exit to its loop, or to its `switch` when it leaves the switch before its
    /// case ends; a `break` that ends a case is no node, and one that leaves a labeled plain
    /// statement can't be placed, so the node around it is truncated.
    pub(super) fn break_statement(&mut self, it: &BreakStatement<'_>) {
        let found = match &it.label {
            Some(label) => self
                .breakables
                .iter()
                .rev()
                .find(|breakable| breakable.labels.iter().any(|own| **own == *label.name)),
            None => self
                .breakables
                .iter()
                .rev()
                .find(|breakable| breakable.breaks != Breaks::Plain),
        };
        match found.map(|breakable| breakable.breaks) {
            Some(Breaks::Switch(_)) if self.final_breaks.contains(&it.unstable_address()) => {}
            Some(Breaks::Loop(target) | Breaks::Switch(target)) => {
                self.exit(Form::Break, range(it.span), None, Some(target));
            }
            Some(Breaks::Plain) | None => self.truncate(self.at.parent),
        }
    }

    /// `continue`: an exit to its loop.
    pub(super) fn continue_statement(&mut self, it: &ContinueStatement<'_>) {
        let found = self
            .breakables
            .iter()
            .rev()
            .find_map(|breakable| match breakable.breaks {
                Breaks::Loop(target)
                    if it
                        .label
                        .as_ref()
                        .is_none_or(|label| breakable.labels.iter().any(|own| **own == *label.name)) =>
                {
                    Some(target)
                }
                _ => None,
            });
        match found {
            Some(target) => self.exit(Form::Continue, range(it.span), None, Some(target)),
            None => self.truncate(self.at.parent),
        }
    }

    /// A variable declaration: its declarators' code; an ambient one has none.
    pub(super) fn variable_declaration(&mut self, it: &VariableDeclaration<'_>) {
        if !it.declare {
            self.declarators(it);
        }
    }

    /// Each declarator's binding pattern's code, its initializer, and its annotation. The
    /// initializer is a function, class, or object literal it names, or else code whose call or
    /// conditional expression binds the written targets (`x = a` in `const x = a = f()`). The
    /// annotation links from the definition it names ([`Lowering::annotate`]).
    pub(super) fn declarators(&mut self, it: &VariableDeclaration<'_>) {
        for declarator in &it.declarations {
            self.visit_binding_pattern(&declarator.id);
            let defined = declarator.init.as_ref().and_then(|init| {
                let (targets, value) = self.chain(self.text(range(declarator.id.span())), init);
                self.bind_value(value, &targets);
                match &declarator.id {
                    BindingPattern::BindingIdentifier(id) => self.named_value(init, id.name.as_str()),
                    _ => {
                        self.visit_expression(init);
                        None
                    }
                }
            });
            self.annotate(declarator.type_annotation.as_deref(), defined);
        }
    }
}

/// A case's last statement, through the blocks that end it.
fn final_statement<'s, 'a>(statements: &'s [Statement<'a>]) -> Option<&'s Statement<'a>> {
    let mut last = statements.last()?;
    while let Statement::BlockStatement(block) = last {
        last = block.body.last()?;
    }
    Some(last)
}
