//! The value table and occurrences: one [`CallFact`] per call, one [`ImportFact`] per runtime
//! import, and a [`ValueFact`] for every callee and argument (receivers first), and for every
//! item of a list or tuple argument (which only the entrance detectors read).
//!
//! Python values keep the proof of concept's shapes and labels:
//!
//! | Expression | Shape | Label |
//! | --- | --- | --- |
//! | `name` | `Name` | Its written text |
//! | `receiver.member` | `Member`, with the receiver recorded first | Its written text |
//! | a call | `CallResult` | Its written text, arguments included (`datetime.now(timezone.utc).isoformat`) |
//! | `lambda` | `InlineFunction`, naming the lambda's block | Its written text |
//! | anything else (subscripts, literals, ...) | `Other` | Its written text |
//!
//! Labels are the written source slice of the proof-of-concept range (parentheses included,
//! except around calls and lambdas), cut to 120 UTF-16 units. Argument positions are written
//! ordinals that count positional, starred, keyword, and `**` arguments alike; starred and
//! `**` arguments never supply references.

use ruff_python_ast::{ArgOrKeyword, Expr, ExprCall};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;
use unfold_core::vocab::EdgeKind;
use unfold_core::vocab::Expansion;
use unfold_engine::facts::{ArgumentFact, CallContext, CallFact, ValueFact, ValueShape};

use crate::ast::arguments;
use crate::syntax::Syntax;

/// Builds one file's value table and call facts.
pub(crate) struct Values<'s, 'a> {
    syntax: &'s Syntax<'a>,
    /// The block of every selected lambda, by the lambda's address in the tree.
    lambdas: &'s FxHashMap<usize, u32>,
    values: Vec<ValueFact>,
    /// The expression of each value, which the binder interns.
    exprs: Vec<&'a Expr>,
}

/// An expression's identity in the tree, for looking up the block it selected.
pub(crate) fn address(expr: &Expr) -> usize {
    std::ptr::from_ref(expr) as usize
}

impl<'s, 'a> Values<'s, 'a> {
    pub(crate) fn new(syntax: &'s Syntax<'a>, lambdas: &'s FxHashMap<usize, u32>) -> Self {
        Self {
            syntax,
            lambdas,
            values: Vec::new(),
            exprs: Vec::new(),
        }
    }

    /// The finished value table, with each value's expression.
    pub(crate) fn finish(self) -> (Vec<ValueFact>, Vec<&'a Expr>) {
        (self.values, self.exprs)
    }

    /// The call fact of the `call` block `block`.
    pub(crate) fn call(&mut self, call: &'a ExprCall, block: u32) -> CallFact {
        let callee = self.value(&call.func, self.syntax.range_in(&call.func, call.range()));
        let interior = Syntax::interior(call.arguments.range);
        let arguments = arguments(&call.arguments)
            .enumerate()
            .map(|(position, argument)| {
                let (value, bound, expand, keyword) = match argument {
                    ArgOrKeyword::Arg(Expr::Starred(starred)) => {
                        (&*starred.value, starred.range, Some(Expansion::Spread), None)
                    }
                    ArgOrKeyword::Arg(expr) => (expr, interior, None, None),
                    ArgOrKeyword::Keyword(keyword) => match &keyword.arg {
                        Some(name) => (&keyword.value, keyword.range, None, Some(name.id.as_str())),
                        None => (&keyword.value, keyword.range, Some(Expansion::Keywords), None),
                    },
                };
                let range = self.syntax.range_in(value, bound);
                ArgumentFact {
                    value: self.value(value, range),
                    position: position as u32,
                    reference: expand.is_none(),
                    expand,
                    keyword: keyword.map(Into::into),
                }
            })
            .collect();
        // The items of a list or tuple argument get values too, so the entrance detectors can
        // name each one (a FastAPI `dependencies=[...]` item kept as an unsupported target).
        for argument in crate::ast::arguments(&call.arguments) {
            let value = match argument {
                ArgOrKeyword::Arg(Expr::Starred(_)) => continue,
                ArgOrKeyword::Arg(expr) => expr,
                ArgOrKeyword::Keyword(keyword) => &keyword.value,
            };
            let items = match value {
                Expr::List(list) => &list.elts,
                Expr::Tuple(tuple) => &tuple.elts,
                _ => continue,
            };
            for item in items {
                self.value(item, self.syntax.range_in(item, value.range()));
            }
        }
        CallFact {
            block,
            callee: Some(callee),
            edge: EdgeKind::Calls,
            context: CallContext::Call,
            arguments,
        }
    }

    /// Records `expr` (whose proof-of-concept range is `range`) and its receiver chain, and
    /// returns its index. Receivers come first, so a member's receiver always precedes it; the
    /// chain is walked iteratively, so long chains cannot overflow the stack.
    fn value(&mut self, expr: &'a Expr, range: TextRange) -> u32 {
        let mut chain = vec![(expr, range)];
        while let Some(&(Expr::Attribute(access), _)) = chain.last() {
            let receiver = self.syntax.range_in(&access.value, access.range);
            chain.push((&access.value, receiver));
        }
        let mut receiver = None;
        for (expr, range) in chain.into_iter().rev() {
            let fact = self.fact(expr, range, receiver);
            receiver = Some(self.values.len() as u32);
            self.values.push(fact);
            self.exprs.push(expr);
        }
        receiver.unwrap_or_default()
    }

    fn fact(&self, expr: &Expr, range: TextRange, receiver: Option<u32>) -> ValueFact {
        let label_range = match expr {
            Expr::Call(_) | Expr::Lambda(_) => expr.range(),
            _ => range,
        };
        let label = self.syntax.slice(label_range);
        let mut fact = match expr {
            Expr::Name(_) => ValueFact::new(ValueShape::Name, label),
            Expr::Attribute(access) => {
                let mut fact = ValueFact::new(ValueShape::Member, label);
                fact.receiver = receiver;
                fact.member = Some(access.attr.id.as_str().into());
                fact
            }
            Expr::Call(_) => ValueFact::new(ValueShape::CallResult, label),
            Expr::Lambda(_) => ValueFact::new(ValueShape::InlineFunction, label),
            _ => ValueFact::new(ValueShape::Other, label),
        };
        if let Expr::Lambda(_) = expr {
            fact.block = self.lambdas.get(&address(expr)).copied();
            if fact.block.is_none() {
                fact.shape = ValueShape::Other;
            }
        }
        fact
    }
}
