//! The value table and call facts: one [`CallFact`] per call node, with a [`ValueFact`] for its
//! callee (receivers first) and for every argument that may pass a definition.
//!
//! | Expression | Shape | Label |
//! | --- | --- | --- |
//! | `name` | `Name` | Its written text |
//! | `receiver.member` | `Member`, with the receiver recorded first | Its written text |
//! | anything else (calls, subscripts, literals, lambdas, ...) | `Other` | Its written text |
//!
//! A value's range is its expression's, which is where its `calls` or `passes` link is written.
//! Labels are the written source of that range, cut to 120 UTF-16 units. An argument records a
//! value only when it is a name or a member access that isn't expanded.

use ruff_python_ast::{ArgOrKeyword, Expr};
use ruff_text_size::Ranged;
use unfold_core::vocab::Expansion;
use unfold_engine::facts::{ArgumentFact, CallContext, CallFact, ValueFact, ValueShape};

use crate::ast::{arguments, bytes};
use crate::lower::Called;

/// Builds one file's value table and call facts.
pub(crate) struct Values<'a> {
    text: &'a str,
    values: Vec<ValueFact>,
    /// The expression of each value, which the binder interns.
    exprs: Vec<&'a Expr>,
}

impl<'a> Values<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            values: Vec::new(),
            exprs: Vec::new(),
        }
    }

    /// The finished value table, with each value's expression.
    pub(crate) fn finish(self) -> (Vec<ValueFact>, Vec<&'a Expr>) {
        (self.values, self.exprs)
    }

    /// The call fact of one call node.
    pub(crate) fn call(&mut self, called: &Called<'a>) -> CallFact {
        let callee = self.value(called.callee);
        let arguments = called.arguments.map_or_else(Vec::new, |written| {
            arguments(written)
                .map(|argument| {
                    let (passed, range, expand, keyword) = match argument {
                        ArgOrKeyword::Arg(Expr::Starred(starred)) => {
                            (None, starred.range, Some(Expansion::Spread), None)
                        }
                        ArgOrKeyword::Arg(expr) => (Some(expr), expr.range(), None, None),
                        ArgOrKeyword::Keyword(keyword) => match &keyword.arg {
                            Some(name) => (Some(&keyword.value), keyword.range, None, Some(name.id.as_str())),
                            None => (None, keyword.range, Some(Expansion::Keywords), None),
                        },
                    };
                    let value = passed
                        .filter(|passed| matches!(passed, Expr::Name(_) | Expr::Attribute(_)))
                        .map(|passed| self.value(passed));
                    ArgumentFact {
                        range: bytes(range),
                        value,
                        expand,
                        keyword: keyword.map(Into::into),
                    }
                })
                .collect()
        });
        CallFact {
            node: called.node,
            callee,
            context: CallContext::Call,
            arguments,
        }
    }

    /// Records `expr` and its receiver chain, and returns its index. Receivers come first, so a
    /// member's receiver always precedes it; the chain is walked iteratively, so long chains
    /// cannot overflow the stack.
    fn value(&mut self, expr: &'a Expr) -> u32 {
        let mut chain = vec![expr];
        while let Some(Expr::Attribute(access)) = chain.last() {
            chain.push(&access.value);
        }
        let mut receiver = None;
        for expr in chain.into_iter().rev() {
            let range = expr.range();
            let label = self
                .text
                .get(range.start().to_usize()..range.end().to_usize())
                .unwrap_or_default();
            let mut fact = match expr {
                Expr::Name(_) => ValueFact::new(ValueShape::Name, label, bytes(range)),
                Expr::Attribute(access) => {
                    let mut fact = ValueFact::new(ValueShape::Member, label, bytes(range));
                    fact.member = Some(access.attr.id.as_str().into());
                    fact
                }
                _ => ValueFact::new(ValueShape::Other, label, bytes(range)),
            };
            fact.receiver = receiver.filter(|_| matches!(expr, Expr::Attribute(_)));
            receiver = Some(u32::try_from(self.values.len()).unwrap_or(u32::MAX));
            self.values.push(fact);
            self.exprs.push(expr);
        }
        receiver.unwrap_or_default()
    }
}
