//! Expressions that are nodes: calls, constructions, tagged templates, dynamic imports, and
//! conditional expressions that contain a call; with what `binds`, `exit_value`, `awaited`, and
//! `conditional` record, and assignments, short circuits, and `yield`.

use oxc_allocator::{Address, UnstableAddress};
use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentOperator, CallExpression, ConditionalExpression, Expression,
    ImportExpression, NewExpression, TaggedTemplateExpression, YieldExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use unfold_core::key::{Callee, Child};
use unfold_core::text::ByteRange;
use unfold_core::vocab::{Expansion, Flag, Form};
use unfold_engine::facts::{ArgumentFact, CallContext, CallFact, ImportFact};

use super::{Lowering, Within, forms};
use crate::payload::ImportKind;
use crate::spans::range;
use crate::values;

/// Everything a call node is made from.
pub(super) struct CallNode {
    /// The oxc call, construction, tagged template, dynamic import, or decorator expression.
    pub(super) address: Address,
    pub(super) range: ByteRange,
    /// The written callee.
    pub(super) head: ByteRange,
    pub(super) callee: Callee,
    /// The callee's value.
    pub(super) value: u32,
    pub(super) form: Form,
    pub(super) context: CallContext,
    pub(super) first_literal: Option<String>,
    pub(super) takes_callback: bool,
    pub(super) arguments: Vec<ArgumentFact>,
    /// The call runs only after an optional link of its own chain.
    pub(super) after_optional: bool,
}

impl Lowering<'_> {
    /// A call node with its call fact. Written directly in a test, parameter defaults, a
    /// decorator, or a class's base, it has that role.
    pub(super) fn call_node(&mut self, call: CallNode) -> Option<u32> {
        let child = Child::Call {
            callee: &call.callee,
            first_literal: call.first_literal.as_deref(),
            takes_callback: call.takes_callback,
        };
        let id = self.push(
            self.at.parent,
            child,
            call.form,
            Some(call.callee.text()),
            call.range,
        )?;
        let binds = self.binds.remove(&call.address);
        let exit_value = self.exit_values.remove(&call.address);
        let awaited = self.awaited.remove(&call.address);
        let conditional = self.conditional || call.after_optional;
        let role = self.at.within.role();
        let node = self.node(id);
        node.head = Some(call.head);
        node.binds = binds;
        node.role = role;
        for (flag, on) in [
            (Flag::Awaited, awaited),
            (Flag::Conditional, conditional),
            (Flag::ExitValue, exit_value),
        ] {
            if on {
                node.flags.insert(flag);
            }
        }
        self.calls.push(CallFact {
            node: id,
            callee: call.value,
            context: call.context,
            arguments: call.arguments,
        });
        Some(id)
    }

    /// One fact per written argument; a plain name or member access that isn't spread may
    /// pass a definition.
    fn argument_facts(&mut self, arguments: &[Argument<'_>]) -> Vec<ArgumentFact> {
        arguments
            .iter()
            .map(|argument| match argument {
                Argument::SpreadElement(spread) => ArgumentFact {
                    range: range(spread.span),
                    value: None,
                    expand: Some(Expansion::Spread),
                    keyword: None,
                },
                _ => {
                    let expression = argument.to_expression();
                    let value =
                        values::may_name(expression).then(|| self.values.value(expression, self.source));
                    ArgumentFact {
                        range: range(expression.span()),
                        value,
                        expand: None,
                        keyword: None,
                    }
                }
            })
            .collect()
    }

    /// Lowers a call's receiver, then its arguments, which run only sometimes when the call
    /// does.
    fn call_children(
        &mut self,
        call: u32,
        receiver: impl FnOnce(&mut Self),
        arguments: impl FnOnce(&mut Self),
        after_optional: bool,
    ) {
        self.within(call, Within::Value, receiver);
        let outer = self.conditional;
        self.conditional |= after_optional;
        self.within(call, Within::Argument, arguments);
        self.conditional = outer;
    }

    pub(super) fn call_expression(&mut self, it: &CallExpression<'_>) {
        let value = self.values.value(&it.callee, self.source);
        let arguments = self.argument_facts(&it.arguments);
        let context = if matches!(it.callee, Expression::Super(_)) {
            CallContext::Construct
        } else {
            CallContext::Call
        };
        let after_optional = forms::after_optional(it);
        let call = self.call_node(CallNode {
            address: it.unstable_address(),
            range: range(it.span),
            head: range(it.callee.span()),
            callee: forms::callee(&it.callee),
            value,
            form: Form::Call,
            context,
            first_literal: forms::first_literal(&it.arguments),
            takes_callback: it.arguments.iter().any(forms::reaches_inline),
            arguments,
            after_optional,
        });
        let Some(call) = call else {
            return;
        };
        self.call_children(
            call,
            |lowering| lowering.visit_expression(&it.callee),
            |lowering| {
                for argument in &it.arguments {
                    lowering.visit_argument(argument);
                }
            },
            after_optional,
        );
    }

    pub(super) fn new_expression(&mut self, it: &NewExpression<'_>) {
        let value = self.values.value(&it.callee, self.source);
        let arguments = self.argument_facts(&it.arguments);
        let call = self.call_node(CallNode {
            address: it.unstable_address(),
            range: range(it.span),
            head: range(it.callee.span()),
            callee: forms::callee(&it.callee),
            value,
            form: Form::New,
            context: CallContext::Construct,
            first_literal: forms::first_literal(&it.arguments),
            takes_callback: it.arguments.iter().any(forms::reaches_inline),
            arguments,
            after_optional: false,
        });
        let Some(call) = call else {
            return;
        };
        self.call_children(
            call,
            |lowering| lowering.visit_expression(&it.callee),
            |lowering| {
                for argument in &it.arguments {
                    lowering.visit_argument(argument);
                }
            },
            false,
        );
    }

    /// A tagged template: a call of its tag, with no arguments; the code in its template is the
    /// call's.
    pub(super) fn tagged_template(&mut self, it: &TaggedTemplateExpression<'_>) {
        let value = self.values.value(&it.tag, self.source);
        let call = self.call_node(CallNode {
            address: it.unstable_address(),
            range: range(it.span),
            head: range(it.tag.span()),
            callee: forms::callee(&it.tag),
            value,
            form: Form::Call,
            context: CallContext::Call,
            first_literal: None,
            takes_callback: false,
            arguments: Vec::new(),
            after_optional: false,
        });
        let Some(call) = call else {
            return;
        };
        self.within(call, Within::Value, |lowering| {
            lowering.visit_expression(&it.tag);
            for expression in &it.quasi.expressions {
                lowering.visit_expression(expression);
            }
        });
    }

    /// `import(…)`: a call of its `import` keyword, with an `imports` link from it at its
    /// specifier.
    pub(super) fn import_expression(&mut self, it: &ImportExpression<'_>) {
        let keyword = ByteRange {
            start: it.span.start,
            end: it.span.start + "import".len() as u32,
        };
        let value = self.values.import_keyword(keyword);
        let written: Vec<&Expression<'_>> = std::iter::once(&it.source).chain(&it.options).collect();
        let arguments = written
            .iter()
            .map(|&expression| {
                let value = values::may_name(expression).then(|| self.values.value(expression, self.source));
                ArgumentFact {
                    range: range(expression.span()),
                    value,
                    expand: None,
                    keyword: None,
                }
            })
            .collect();
        let call = self.call_node(CallNode {
            address: it.unstable_address(),
            range: range(it.span),
            head: keyword,
            callee: Callee::name("import"),
            value,
            form: Form::Call,
            context: CallContext::Call,
            first_literal: None,
            takes_callback: false,
            arguments,
            after_optional: false,
        });
        let Some(call) = call else {
            return;
        };
        let specifier = match &it.source {
            Expression::StringLiteral(literal) => Some(literal.value.as_str().into()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
                .quasis
                .first()
                .and_then(|quasi| quasi.value.cooked.as_ref())
                .map(|cooked| cooked.as_str().into()),
            _ => None,
        };
        self.imports.push(ImportFact {
            from: call,
            range: range(it.source.span()),
            specifier,
            conditional: false,
        });
        self.import_kinds.push(ImportKind::Dynamic);
        self.within(call, Within::Value, |lowering| {
            for expression in written {
                lowering.visit_expression(expression);
            }
        });
    }

    /// `c ? a : b`: a `ternary` decision when it contains a call, whose `then` part has the
    /// test `c` and the body `a`, and whose `else` part is `b`; otherwise code.
    pub(super) fn conditional_expression(&mut self, it: &ConditionalExpression<'_>) {
        if !forms::contains_call(it) {
            walk::walk_conditional_expression(self, it);
            return;
        }
        let address = it.unstable_address();
        let decision = self.push(
            self.at.parent,
            Child::Form(Form::Ternary),
            Form::Ternary,
            None,
            range(it.span),
        );
        let Some(decision) = decision else {
            return;
        };
        let binds = self.binds.remove(&address);
        let exit_value = self.exit_values.remove(&address);
        let node = self.node(decision);
        node.binds = binds;
        if exit_value {
            node.flags.insert(Flag::ExitValue);
        }
        let test = range(it.test.span());
        let consequent = range(it.consequent.span());
        let then = ByteRange {
            start: test.start,
            end: consequent.end,
        };
        if let Some(part) = self.part(decision, Form::Then, then, Some(test), Some(consequent)) {
            self.within(part, Within::Test, |lowering| {
                lowering.visit_expression(&it.test);
            });
            self.within(part, Within::Body, |lowering| {
                lowering.visit_expression(&it.consequent);
            });
        }
        let alternate = range(it.alternate.span());
        if let Some(part) = self.part(decision, Form::Else, alternate, None, Some(alternate)) {
            self.within(part, Within::Body, |lowering| {
                lowering.visit_expression(&it.alternate);
            });
        }
    }

    /// An assignment: its target's code, then its value, whose call or conditional expression
    /// binds the written target for a plain `=`; a logical assignment's value runs only
    /// sometimes.
    pub(super) fn assignment(&mut self, it: &AssignmentExpression<'_>) {
        self.visit_assignment_target(&it.left);
        match it.operator {
            AssignmentOperator::Assign => {
                let target = self.text(range(it.left.span())).to_owned();
                self.bind_value(&it.right, &target);
                self.visit_expression(&it.right);
            }
            AssignmentOperator::LogicalAnd
            | AssignmentOperator::LogicalOr
            | AssignmentOperator::LogicalNullish => {
                self.sometimes(|lowering| lowering.visit_expression(&it.right));
            }
            _ => self.visit_expression(&it.right),
        }
    }

    /// `yield`: an exit where the code is a body's; inside another node's expression its value
    /// is that node's code.
    pub(super) fn yield_expression(&mut self, it: &YieldExpression<'_>) {
        if self.at.within == Within::Body {
            self.exit(Form::Yield, range(it.span), it.argument.as_ref(), None);
        } else {
            walk::walk_yield_expression(self, it);
        }
    }
}
