//! Reading the syntactic forms the lowering decides by, without lowering anything: a call's
//! normalized callee, what a value is through its wrappers, whether a call runs only after an
//! optional link, which `yield` expressions are exits, and whether an expression reaches an
//! inline function or contains a call.

use oxc_allocator::{Address, UnstableAddress};
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, CallExpression, ChainElement, Class, ConditionalExpression,
    Expression, Function, ImportExpression, NewExpression, PropertyKey, Statement, TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use unfold_core::key::Callee;

/// A callee as the key and the node's name write it ([`Callee`]): `await`, parentheses, `!`,
/// and type casts dropped; `?.` written `.`; arguments `()`; subscripts `[]`; a string literal
/// `""`; any other expression `(…)`.
pub(super) fn callee(expression: &Expression<'_>) -> Callee {
    // The steps after the root, outermost first, gathered without recursion.
    enum Step<'e> {
        Member(&'e str),
        Private(&'e str),
        Call,
        Index,
    }
    let mut steps: Vec<Step<'_>> = Vec::new();
    let mut current = expression;
    let root = loop {
        current = match current {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::AwaitExpression(it) => &it.argument,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            Expression::TSInstantiationExpression(it) => &it.expression,
            Expression::StaticMemberExpression(it) => {
                steps.push(Step::Member(it.property.name.as_str()));
                &it.object
            }
            Expression::PrivateFieldExpression(it) => {
                steps.push(Step::Private(it.field.name.as_str()));
                &it.object
            }
            Expression::ComputedMemberExpression(it) => {
                steps.push(Step::Index);
                &it.object
            }
            Expression::CallExpression(it) => {
                steps.push(Step::Call);
                &it.callee
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(it) => {
                    steps.push(Step::Call);
                    &it.callee
                }
                ChainElement::TSNonNullExpression(it) => &it.expression,
                ChainElement::StaticMemberExpression(it) => {
                    steps.push(Step::Member(it.property.name.as_str()));
                    &it.object
                }
                ChainElement::PrivateFieldExpression(it) => {
                    steps.push(Step::Private(it.field.name.as_str()));
                    &it.object
                }
                ChainElement::ComputedMemberExpression(it) => {
                    steps.push(Step::Index);
                    &it.object
                }
            },
            Expression::Identifier(it) => break Callee::name(it.name.as_str()),
            Expression::ThisExpression(_) => break Callee::name("this"),
            Expression::Super(_) => break Callee::name("super"),
            Expression::ImportMeta(_) => break Callee::name("import.meta"),
            Expression::NewTarget(_) => break Callee::name("new.target"),
            Expression::StringLiteral(_) => break Callee::string(),
            Expression::TemplateLiteral(it) if it.expressions.is_empty() => break Callee::string(),
            _ => break Callee::expression(),
        };
    };
    steps.into_iter().rev().fold(root, |callee, step| match step {
        Step::Member(name) => callee.member(name),
        Step::Private(name) => callee.member(&format!("#{name}")),
        Step::Call => callee.call(),
        Step::Index => callee.index(),
    })
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

/// Whether a property key names a definition: a written name or literal with no `.` in it, so
/// a qualified name built from it reads back as the same names.
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
    (!name.is_empty() && !name.contains('.')).then_some(name)
}

/// Whether an argument reaches an inline function before any expression that is a node of its
/// own (a call, a construction, a tagged template, a dynamic import, a conditional expression
/// that contains a call, or a class): that function is then a callback of the call.
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

    fn visit_class(&mut self, _it: &Class<'a>) {}

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
