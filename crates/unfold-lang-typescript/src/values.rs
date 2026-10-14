//! The value table: a [`ValueFact`] for every callee and every argument that may pass a
//! definition, and for the receivers, callees, and tags their labels depend on. Each value keeps
//! the oxc expression it was recorded for, so binding can give it its converted expression.
//!
//! | Unwrapped expression | Shape | Label |
//! | --- | --- | --- |
//! | Identifier | `Name` | its text |
//! | Property access (`a.b`, `a?.b`, `a.#b`) | `Member` | `receiver.name` |
//! | Element access with a literal key | `Member` | `receiver["key"]` as written |
//! | Other element access | `Other` | `receiver[…]` |
//! | Call (or dynamic import) used as a value | `Other` | `callee()` |
//! | `new`, tagged template | `Other` | the target's or tag's label |
//! | `this`, `super` | `Other` | the keyword |
//! | Anything else, including inline functions and classes | `Other` | `(…)` |
//!
//! Parentheses, `!`, `as`, `satisfies`, and `<T>` assertions are transparent. Values are created
//! dependencies first, so a receiver always precedes the access that uses it. A value's range is
//! the written expression, wrappers included: where its `calls` or `passes` link is written.

use oxc_allocator::{Address, GetAddress};
use oxc_ast::ast::{ChainElement, Expression, MemberExpression};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use unfold_core::text::ByteRange;
use unfold_engine::facts::{ValueFact, ValueShape};
use unfold_engine::links::text;

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
    /// The value of `expression` (written in `source`), creating it and everything its label
    /// depends on.
    pub(crate) fn value(&mut self, expression: &Expression<'_>, source: &str) -> u32 {
        // Walk the dependency chain (receiver, callee, or tag) iteratively, then create values
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
            let index = self.create(link, dependency, source);
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

    fn create(&mut self, expression: &Expression<'_>, dependency: Option<u32>, source: &str) -> u32 {
        let unwrapped = unwrap(expression);
        let inner = dependency.map_or(text::OTHER, |index| &*self.facts[index as usize].label);
        let (shape, label, member) = match unwrapped {
            Expression::Identifier(it) => (ValueShape::Name, it.name.to_string(), None),
            Expression::ThisExpression(_) => (ValueShape::Other, "this".to_owned(), None),
            Expression::Super(_) => (ValueShape::Other, "super".to_owned(), None),
            Expression::CallExpression(_) => (ValueShape::Other, text::call_result(inner), None),
            Expression::ImportExpression(_) => (ValueShape::Other, text::call_result(text::OTHER), None),
            Expression::NewExpression(_) | Expression::TaggedTemplateExpression(_) => {
                (ValueShape::Other, inner.to_owned(), None)
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(_) => (ValueShape::Other, text::call_result(inner), None),
                element => match element.as_member_expression() {
                    Some(access) => self.access(access, inner, source),
                    None => (ValueShape::Other, text::OTHER.to_owned(), None),
                },
            },
            other => match other.as_member_expression() {
                Some(access) => self.access(access, inner, source),
                None => (ValueShape::Other, text::OTHER.to_owned(), None),
            },
        };
        let mut fact = ValueFact::new(shape, &label, range(expression.span()));
        if member_of(unwrapped).is_some() {
            fact.receiver = dependency;
        }
        fact.member = member;
        let index = self.facts.len() as u32;
        self.facts.push(fact);
        self.addresses.push(Some(expression.address()));
        index
    }

    /// The shape, label, and member name of a property or element access whose receiver's
    /// label is `inner`.
    fn access(
        &self,
        access: &MemberExpression<'_>,
        inner: &str,
        source: &str,
    ) -> (ValueShape, String, Option<Box<str>>) {
        match access {
            MemberExpression::StaticMemberExpression(it) => {
                let name = it.property.name.as_str();
                (ValueShape::Member, text::member(inner, name), Some(name.into()))
            }
            MemberExpression::PrivateFieldExpression(it) => {
                let name = format!("#{}", it.field.name);
                (ValueShape::Member, text::member(inner, &name), Some(name.into()))
            }
            MemberExpression::ComputedMemberExpression(it) => {
                let shape = if is_name_literal(unwrap(&it.expression)) {
                    ValueShape::Member
                } else {
                    ValueShape::Other
                };
                let label = if is_name_literal(&it.expression) {
                    let written = range(it.expression.span());
                    let spelling = source
                        .get(written.start as usize..written.end as usize)
                        .unwrap_or("…");
                    text::literal_key(inner, spelling)
                } else {
                    text::computed_key(inner)
                };
                (shape, label, None)
            }
        }
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

/// The expression a value's label and facts are built from: an access's receiver, a call's or
/// construction's callee, or a template's tag.
fn dependency<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e Expression<'a>> {
    let unwrapped = unwrap(expression);
    if let Some(access) = member_of(unwrapped) {
        return Some(access.object());
    }
    match unwrapped {
        Expression::CallExpression(it) => Some(&it.callee),
        Expression::NewExpression(it) => Some(&it.callee),
        Expression::TaggedTemplateExpression(it) => Some(&it.tag),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(it) => Some(&it.callee),
            _ => None,
        },
        _ => None,
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
