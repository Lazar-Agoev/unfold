//! The value table: a [`ValueFact`] for every callee and every argument that may pass a
//! definition, and for the receivers and callees their labels depend on. Each value keeps the
//! oxc expression it was recorded for, so binding can give it its converted expression.
//!
//! A value's label, the text of the `calls` or `passes` link written at it, is its callee as a
//! call's key and name write it ([`Callee`], [`callee_step`]), so a call reads the same as its
//! node and as its link: `"".trim` for `"str".trim()`, `a[]` for `a["key"]()`, `(…).open` for
//! `new Box().open()`, `getRepo().save` for `(await getRepo()).save()`.
//!
//! | Unwrapped expression | Shape |
//! | --- | --- |
//! | Identifier | `Name` |
//! | Property access (`a.b`, `a?.b`, `a.#b`), element access with a literal key | `Member` |
//! | Anything else | `Other` |
//!
//! Parentheses, `!`, `as`, `satisfies`, and `<T>` assertions are transparent. Values are created
//! dependencies first, so a receiver always precedes the access that uses it. A value's range is
//! the written expression, wrappers included: where its `calls` or `passes` link is written.

use oxc_allocator::{Address, GetAddress};
use oxc_ast::ast::{ChainElement, Expression, MemberExpression};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use unfold_core::key::Callee;
use unfold_core::text::ByteRange;
use unfold_engine::facts::{ValueFact, ValueShape};

use crate::lower::{CalleeStep, callee_step};
use crate::spans::range;

/// One file's value table, being recorded.
#[derive(Debug, Default)]
pub(crate) struct Values {
    facts: Vec<ValueFact>,
    /// The expression each value was recorded for; `None` for an `import` keyword.
    addresses: Vec<Option<Address>>,
    /// Expression address → its value.
    memo: FxHashMap<Address, u32>,
}

impl Values {
    /// The value of `expression`, creating it and everything its label depends on.
    pub(crate) fn value(&mut self, expression: &Expression<'_>) -> u32 {
        // Walk the dependency chain (receiver or callee) iteratively, then create values
        // innermost first, so long access chains never recurse.
        let mut chain = Vec::new();
        let mut current = Some(expression);
        while let Some(link) = current {
            if self.memo.contains_key(&link.address()) {
                break;
            }
            chain.push(link);
            current = dependency(link);
        }
        let mut dependency = current.and_then(|link| self.memo.get(&link.address()).copied());
        for link in chain.into_iter().rev() {
            let index = self.create(link, dependency);
            self.memo.insert(link.address(), index);
            dependency = Some(index);
        }
        dependency.unwrap_or_default()
    }

    /// The value of a dynamic import's `import` keyword at `range`: its callee, which names
    /// nothing.
    pub(crate) fn import_keyword(&mut self, range: ByteRange) -> u32 {
        let index = self.facts.len() as u32;
        self.facts
            .push(ValueFact::new(ValueShape::Other, "import", range));
        self.addresses.push(None);
        index
    }

    /// The value recorded for the expression at `address`.
    pub(crate) fn index_of(&self, address: Address) -> Option<u32> {
        self.memo.get(&address).copied()
    }

    /// The expression of each value, in value order.
    pub(crate) fn addresses(&self) -> &[Option<Address>] {
        &self.addresses
    }

    /// The finished table.
    pub(crate) fn finish(self) -> Vec<ValueFact> {
        self.facts
    }

    fn create(&mut self, expression: &Expression<'_>, dependency: Option<u32>) -> u32 {
        let unwrapped = unwrap(expression);
        let (shape, member) = match member_of(unwrapped) {
            Some(MemberExpression::StaticMemberExpression(it)) => {
                (ValueShape::Member, Some(it.property.name.as_str().into()))
            }
            Some(MemberExpression::PrivateFieldExpression(it)) => {
                (ValueShape::Member, Some(format!("#{}", it.field.name).into()))
            }
            Some(MemberExpression::ComputedMemberExpression(it))
                if is_name_literal(unwrap(&it.expression)) =>
            {
                (ValueShape::Member, None)
            }
            Some(MemberExpression::ComputedMemberExpression(_)) => (ValueShape::Other, None),
            None if matches!(unwrapped, Expression::Identifier(_)) => (ValueShape::Name, None),
            None => (ValueShape::Other, None),
        };
        let label = match callee_step(expression) {
            CalleeStep::Root(root) => root,
            CalleeStep::After(_, then) => {
                let inner = dependency.map_or("", |index| &*self.facts[index as usize].label);
                then.after(Callee::name(inner))
            }
        };
        let mut fact = ValueFact::new(shape, label.text(), range(expression.span()));
        if member_of(unwrapped).is_some() {
            fact.receiver = dependency;
        }
        fact.member = member;
        let index = self.facts.len() as u32;
        self.facts.push(fact);
        self.addresses.push(Some(expression.address()));
        index
    }
}

/// The expression inside parentheses, `!`, `as`, `satisfies`, and `<T>` assertions.
pub(crate) fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::TSNonNullExpression(it) => &it.expression,
                _ => return expression,
            },
            _ => return expression,
        };
    }
}

/// Whether an expression may name a definition: through transparent wrappers, an identifier,
/// a property access, or an element access with a literal key.
pub(crate) fn may_name(expression: &Expression<'_>) -> bool {
    let unwrapped = unwrap(expression);
    match member_of(unwrapped) {
        Some(MemberExpression::ComputedMemberExpression(it)) => is_name_literal(unwrap(&it.expression)),
        Some(_) => true,
        None => matches!(unwrapped, Expression::Identifier(_)),
    }
}

/// The property or element access an unwrapped expression is, inside an optional chain too.
fn member_of<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e MemberExpression<'a>> {
    match expression {
        Expression::ChainExpression(chain) => chain.expression.as_member_expression(),
        other => other.as_member_expression(),
    }
}

/// The expression a value's label is built from, and an access's receiver: the expression
/// inside it in the callee normalization ([`callee_step`]).
fn dependency<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e Expression<'a>> {
    match callee_step(expression) {
        CalleeStep::After(inner, _) => Some(inner),
        CalleeStep::Root(_) => None,
    }
}

/// Whether an expression is a literal that can key an element access by name: a string, a
/// number, or a template without substitutions.
fn is_name_literal(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::StringLiteral(_) | Expression::NumericLiteral(_) => true,
        Expression::TemplateLiteral(it) => it.expressions.is_empty(),
        _ => false,
    }
}
