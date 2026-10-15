//! Reading the syntactic forms the lowering decides by, without lowering anything: a call's
//! normalized callee, what a value is through its wrappers, whether a call runs only after an
//! optional link, which `yield` expressions are exits, and whether an expression reaches an
//! inline definition or contains a call.

use oxc_allocator::{Address, UnstableAddress};
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, CallExpression, ChainElement, Class, ConditionalExpression,
    Expression, Function, ImportExpression, NewExpression, PropertyKey, Statement, TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use unfold_core::key::Callee;

/// How the callee normalization ([`Callee`]) reads an expression, through the wrappers it
/// drops: the root a callee starts with, or what it writes after the callee of the expression
/// inside.
pub(crate) enum CalleeStep<'e, 'a> {
    /// A name or keyword; `""` for a string literal; `(…)` for any other expression.
    Root(Callee),
    /// The callee of the expression inside, then this.
    After(&'e Expression<'a>, Then<'e>),
}

/// What a callee writes after the callee of the expression inside.
pub(crate) enum Then<'e> {
    /// `.name`.
    Member(&'e str),
    /// `.#name`.
    Private(&'e str),
    /// Arguments: `()`.
    Call,
    /// A subscript: `[]`.
    Index,
}

impl Then<'_> {
    /// `callee` followed by this.
    pub(crate) fn after(&self, callee: Callee) -> Callee {
        match self {
            Self::Member(name) => callee.member(name),
            Self::Private(name) => callee.member(&format!("#{name}")),
            Self::Call => callee.call(),
            Self::Index => callee.index(),
        }
    }
}

/// One step of the callee normalization ([`Callee`]): `await`, parentheses, `!`, type casts,
/// and instantiations are dropped, and `?.` is written `.`.
pub(crate) fn callee_step<'e, 'a>(mut expression: &'e Expression<'a>) -> CalleeStep<'e, 'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::AwaitExpression(it) => &it.argument,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            Expression::TSInstantiationExpression(it) => &it.expression,
            Expression::StaticMemberExpression(it) => {
                return CalleeStep::After(&it.object, Then::Member(it.property.name.as_str()));
            }
            Expression::PrivateFieldExpression(it) => {
                return CalleeStep::After(&it.object, Then::Private(it.field.name.as_str()));
            }
            Expression::ComputedMemberExpression(it) => return CalleeStep::After(&it.object, Then::Index),
            Expression::CallExpression(it) => return CalleeStep::After(&it.callee, Then::Call),
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::TSNonNullExpression(it) => &it.expression,
                ChainElement::CallExpression(it) => return CalleeStep::After(&it.callee, Then::Call),
                ChainElement::StaticMemberExpression(it) => {
                    return CalleeStep::After(&it.object, Then::Member(it.property.name.as_str()));
                }
                ChainElement::PrivateFieldExpression(it) => {
                    return CalleeStep::After(&it.object, Then::Private(it.field.name.as_str()));
                }
                ChainElement::ComputedMemberExpression(it) => {
                    return CalleeStep::After(&it.object, Then::Index);
                }
            },
            Expression::Identifier(it) => return CalleeStep::Root(Callee::name(it.name.as_str())),
            Expression::ThisExpression(_) => return CalleeStep::Root(Callee::name("this")),
            Expression::Super(_) => return CalleeStep::Root(Callee::name("super")),
            Expression::ImportMeta(_) => return CalleeStep::Root(Callee::name("import.meta")),
            Expression::NewTarget(_) => return CalleeStep::Root(Callee::name("new.target")),
            Expression::StringLiteral(_) => return CalleeStep::Root(Callee::string()),
            Expression::TemplateLiteral(it) if it.expressions.is_empty() => {
                return CalleeStep::Root(Callee::string());
            }
            _ => return CalleeStep::Root(Callee::expression()),
        };
    }
}

/// A callee as the key and the node's name write it ([`Callee`]), step by step
/// ([`callee_step`]): `await`, parentheses, `!`, and type casts dropped; `?.` written `.`;
/// arguments `()`; subscripts `[]`; a string literal `""`; any other expression `(…)`.
pub(super) fn callee(expression: &Expression<'_>) -> Callee {
    // The steps after the root, outermost first, gathered without recursion.
    let mut steps = Vec::new();
    let mut current = expression;
    let root = loop {
        match callee_step(current) {
            CalleeStep::Root(root) => break root,
            CalleeStep::After(inner, then) => {
                steps.push(then);
                current = inner;
            }
        }
    };
    steps
        .into_iter()
        .rev()
        .fold(root, |callee, then| then.after(callee))
}

/// The decoded value of a call's first argument when it is a string literal (or a template
/// without substitutions).
pub(super) fn first_literal(arguments: &[Argument<'_>]) -> Option<String> {
    match arguments.first()? {
        Argument::StringLiteral(it) => Some(it.value.to_string()),
        Argument::TemplateLiteral(it) if it.expressions.is_empty() => {
            Some(it.quasis.first()?.value.cooked.as_ref()?.to_string())
        }
        _ => None,
    }
}

/// The address of the call, construction, tagged template, dynamic import, or conditional
/// expression an expression's value is, through parentheses, `!`, type casts, and `await`.
pub(super) fn value_node(mut expression: &Expression<'_>) -> Option<Address> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            Expression::AwaitExpression(it) => &it.argument,
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(it) => return Some(it.unstable_address()),
                ChainElement::TSNonNullExpression(it) => &it.expression,
                _ => return None,
            },
            Expression::CallExpression(it) => return Some(it.unstable_address()),
            Expression::NewExpression(it) => return Some(it.unstable_address()),
            Expression::TaggedTemplateExpression(it) => return Some(it.unstable_address()),
            Expression::ImportExpression(it) => return Some(it.unstable_address()),
            Expression::ConditionalExpression(it) => return Some(it.unstable_address()),
            _ => return None,
        };
    }
}

/// The `yield` expressions a statement exits by: its whole expression, the whole value an
/// assignment statement assigns, or a declarator's whole initializer. A `yield` anywhere else,
/// such as in a conditional branch, a test, or an argument, is code.
pub(super) fn statement_yields<'s>(statement: &'s Statement<'_>) -> impl Iterator<Item = Address> + 's {
    let (expression, declarators) = match statement {
        Statement::ExpressionStatement(it) => (Some(&it.expression), None),
        Statement::VariableDeclaration(it) if !it.declare => (None, Some(&it.declarations)),
        _ => (None, None),
    };
    let initializers = declarators
        .into_iter()
        .flatten()
        .filter_map(|declarator| declarator.init.as_ref());
    expression
        .into_iter()
        .chain(initializers)
        .filter_map(|value| match assigned(value) {
            Expression::YieldExpression(it) => Some(it.unstable_address()),
            _ => None,
        })
}

/// What an expression assigns, through parentheses and assignments: `yield f()` in
/// `a = b = yield f()`. A logical assignment's value runs only sometimes, so it ends there.
fn assigned<'s, 'a>(mut expression: &'s Expression<'a>) -> &'s Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::AssignmentExpression(it) if !it.operator.is_logical() => &it.right,
            _ => return expression,
        };
    }
}

/// Whether a call runs only after an optional link of its own chain (`a?.b()`, `f?.()`,
/// `a?.b.c()`, `a?.b().c()`): the call itself, or a member access or call in its callee's
/// chain, is optional. Parentheses end the chain.
pub(super) fn after_optional(call: &CallExpression<'_>) -> bool {
    call.optional || chain_optional(&call.callee)
}

/// Whether an expression of an optional chain has an optional link in it, so what comes after
/// it runs only sometimes (`a?.b` in `a?.b[k()]`). Parentheses end the chain.
pub(super) fn chain_optional(expression: &Expression<'_>) -> bool {
    let mut current = expression;
    loop {
        current = match current {
            Expression::StaticMemberExpression(it) if it.optional => return true,
            Expression::ComputedMemberExpression(it) if it.optional => return true,
            Expression::PrivateFieldExpression(it) if it.optional => return true,
            Expression::StaticMemberExpression(it) => &it.object,
            Expression::ComputedMemberExpression(it) => &it.object,
            Expression::PrivateFieldExpression(it) => &it.object,
            Expression::CallExpression(it) if it.optional => return true,
            Expression::CallExpression(it) => &it.callee,
            Expression::TSNonNullExpression(it) => &it.expression,
            _ => return false,
        };
    }
}

/// The name a property key gives a definition: a written name or literal; `None` for a
/// computed key or an empty name.
pub(super) fn property_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    let name = match key {
        PropertyKey::StaticIdentifier(it) if !computed => it.name.to_string(),
        PropertyKey::PrivateIdentifier(it) => format!("#{}", it.name),
        PropertyKey::StringLiteral(it) => it.value.to_string(),
        PropertyKey::NumericLiteral(it) => crate::number::js_number(it.value),
        PropertyKey::TemplateLiteral(it) if it.expressions.is_empty() => {
            it.quasis.first()?.value.cooked.as_ref()?.to_string()
        }
        _ => return None,
    };
    (!name.is_empty()).then_some(name)
}

/// Whether an argument reaches an inline definition, a function or class, before any expression
/// that is a node of its own (a call, a construction, a tagged template, a dynamic import, or a
/// conditional expression that contains a call): that definition is then a callback of the
/// call.
pub(super) fn reaches_inline(argument: &Argument<'_>) -> bool {
    let mut finder = Inline(false);
    finder.visit_argument(argument);
    finder.0
}

/// Whether a conditional expression contains a call, outside the functions and classes written
/// in it: then it is a `ternary` decision.
pub(super) fn contains_call(conditional: &ConditionalExpression<'_>) -> bool {
    let mut finder = Calls(false);
    walk::walk_conditional_expression(&mut finder, conditional);
    finder.0
}

struct Inline(bool);

impl<'a> Visit<'a> for Inline {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if !self.0 {
            walk::walk_expression(self, it);
        }
    }

    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {
        self.0 = true;
    }

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {
        self.0 = true;
    }

    fn visit_call_expression(&mut self, _it: &CallExpression<'a>) {}

    fn visit_new_expression(&mut self, _it: &NewExpression<'a>) {}

    fn visit_tagged_template_expression(&mut self, _it: &TaggedTemplateExpression<'a>) {}

    fn visit_import_expression(&mut self, _it: &ImportExpression<'a>) {}

    fn visit_class(&mut self, _it: &Class<'a>) {
        self.0 = true;
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        if !contains_call(it) {
            walk::walk_conditional_expression(self, it);
        }
    }
}

struct Calls(bool);

impl<'a> Visit<'a> for Calls {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if !self.0 {
            walk::walk_expression(self, it);
        }
    }

    fn visit_call_expression(&mut self, _it: &CallExpression<'a>) {
        self.0 = true;
    }

    fn visit_new_expression(&mut self, _it: &NewExpression<'a>) {
        self.0 = true;
    }

    fn visit_tagged_template_expression(&mut self, _it: &TaggedTemplateExpression<'a>) {
        self.0 = true;
    }

    fn visit_import_expression(&mut self, _it: &ImportExpression<'a>) {
        self.0 = true;
    }

    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, _it: &Class<'a>) {}
}
