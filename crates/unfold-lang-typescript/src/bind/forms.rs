//! Reading syntactic forms: property keys and literal names, awaited dynamic imports,
//! assertions, and whether a parameter list keeps its own scope.

use oxc_ast::ast::{
    Class, ClassElement, Expression, FormalParameters, Function, LogicalExpression, PropertyKey,
    TSTypeAnnotation,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::operator::LogicalOperator;
use oxc_syntax::scope::ScopeFlags;

use crate::config::BindingContext;
use crate::number::js_number;

/// A property key's name: an identifier, a private name (with `#`), or a decoded string,
/// numeric, or plain template literal (written directly or as a computed key).
pub(super) fn key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(it) if !computed => Some(it.name.to_string()),
        PropertyKey::PrivateIdentifier(it) => Some(format!("#{}", it.name)),
        _ => literal_name(key.as_expression()?).map(String::from),
    }
}

/// A private name's spelling without `#`.
pub(super) fn private_name<'a>(key: &PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::PrivateIdentifier(it) => Some(it.name.as_str()),
        _ => None,
    }
}

/// The decoded text of a string, numeric, or plain template literal.
pub(super) fn literal_name(expression: &Expression<'_>) -> Option<Box<str>> {
    match expression {
        Expression::StringLiteral(it) => Some(it.value.as_str().into()),
        Expression::NumericLiteral(it) => Some(js_number(it.value).into()),
        Expression::TemplateLiteral(it) if it.expressions.is_empty() => it
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|cooked| cooked.as_str().into()),
        _ => None,
    }
}

/// The specifier of `await import("…")` (through parentheses) with a literal specifier, as a
/// dynamic import's `imports` link reads it: a direct string or plain template literal.
pub(super) fn awaited_import<'e>(init: &'e Expression<'_>) -> Option<&'e str> {
    let mut expression = init;
    while let Expression::ParenthesizedExpression(it) = expression {
        expression = &it.expression;
    }
    let Expression::AwaitExpression(awaited) = expression else {
        return None;
    };
    let mut argument = &awaited.argument;
    while let Expression::ParenthesizedExpression(it) = argument {
        argument = &it.expression;
    }
    let Expression::ImportExpression(import) = argument else {
        return None;
    };
    if import.phase.is_some() {
        return None;
    }
    match &import.source {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|cooked| cooked.as_str()),
        _ => None,
    }
}

/// Whether an initializer wraps its value in an `as` or `<T>` assertion.
pub(super) fn asserted(mut expression: &Expression<'_>) -> bool {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSAsExpression(_) | Expression::TSTypeAssertion(_) => return true,
            _ => return false,
        };
    }
}

/// Whether a callable's parameter references keep their own scope, hiding its body variables
/// (TypeScript's `useOuterVariableScopeInParameter`, with the proof of concept's fork's
/// lowering checks): from ES2015 on, unless the parameter list uses syntax the target lowers
/// into the body (an optional chain or `??` below ES2020, an object rest below ES2017, a static
/// field of a class expression without standard class fields).
pub(super) fn uses_outer_scope(parameters: &FormalParameters<'_>, context: BindingContext) -> bool {
    if !context.preserve_parameters {
        return false;
    }
    let mut scan = OuterScan {
        context,
        lowered: false,
    };
    for parameter in &parameters.items {
        scan.visit_binding_pattern(&parameter.pattern);
        if let Some(initializer) = &parameter.initializer {
            scan.visit_expression(initializer);
        }
        if scan.lowered {
            return false;
        }
    }
    if let Some(rest) = &parameters.rest {
        scan.visit_binding_pattern(&rest.rest.argument);
    }
    !scan.lowered
}

/// Looks for parameter syntax the target lowers into the callable's body.
struct OuterScan {
    context: BindingContext,
    lowered: bool,
}

impl<'a> Visit<'a> for OuterScan {
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _it: &oxc_ast::ast::ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        for element in &it.body.body {
            match element {
                ClassElement::PropertyDefinition(property) if property.r#static => {
                    if !self.context.standard_class_fields {
                        self.lowered = true;
                    }
                }
                ClassElement::MethodDefinition(method) if method.computed => {
                    if let Some(key) = method.key.as_expression() {
                        self.visit_expression(key);
                    }
                }
                ClassElement::PropertyDefinition(property) if property.computed => {
                    if let Some(key) = property.key.as_expression() {
                        self.visit_expression(key);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_object_property(&mut self, it: &oxc_ast::ast::ObjectProperty<'a>) {
        if it.computed
            && let Some(key) = it.key.as_expression()
        {
            self.visit_expression(key);
        }
    }

    fn visit_chain_expression(&mut self, _it: &oxc_ast::ast::ChainExpression<'a>) {
        if self.context.lower_optional_chains {
            self.lowered = true;
        }
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        if it.operator == LogicalOperator::Coalesce {
            if self.context.lower_optional_chains {
                self.lowered = true;
            }
            return;
        }
        walk::walk_logical_expression(self, it);
    }

    fn visit_object_pattern(&mut self, it: &oxc_ast::ast::ObjectPattern<'a>) {
        if it.rest.is_some() && self.context.lower_object_rest {
            self.lowered = true;
            return;
        }
        walk::walk_object_pattern(self, it);
    }

    fn visit_ts_type_annotation(&mut self, _it: &TSTypeAnnotation<'a>) {}
}
