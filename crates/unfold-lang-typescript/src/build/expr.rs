//! Expressions, assignment targets, binding patterns, and JSX.

use oxc_allocator::{GetAddress, UnstableAddress};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, AssignmentTargetMaybeDefault,
    AssignmentTargetProperty, BindingPattern, ChainElement, Expression, JSXAttributeItem, JSXAttributeValue,
    JSXChild, JSXElement, JSXExpression, JSXFragment, MemberExpression, ObjectPropertyKind, PropertyKind,
    SimpleAssignmentTarget, TemplateLiteral,
};
use oxc_span::GetSpan;

use super::{Binding, Builder, Ctx, Head, range};
use crate::number::js_number;
use crate::syntax::{Cat, Flags, Label};

impl<'a> Builder<'a, '_> {
    pub(super) fn expression(&mut self, parent: u32, label: Label, expression: &Expression<'a>, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        let first = self.syntax.len() as u32;
        self.expression_inner(parent, label, expression, ctx);
        if self.syntax.len() as u32 > first {
            self.syntax.record_origin(expression.address(), first);
        }
        self.leave();
    }

    fn expression_inner(&mut self, parent: u32, label: Label, expression: &Expression<'a>, ctx: Ctx) {
        let span = range(expression.span());
        let flags = ctx.flags();
        match expression {
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::ImportMeta(_)
            | Expression::NewTarget(_) => {
                self.push(parent, label, Cat::Other, span, flags);
            }
            Expression::NumericLiteral(it) => {
                let text = self.allocator.alloc_str(&js_number(it.value));
                self.push_text(parent, label, Cat::NumericLiteral, it.span, text);
            }
            Expression::StringLiteral(it) => {
                self.push_text(parent, label, Cat::StringLiteral, it.span, it.value.as_str());
            }
            Expression::TemplateLiteral(it) => self.template(parent, label, it, ctx),
            Expression::Identifier(it) => {
                self.push_text(parent, label, Cat::Identifier, it.span, it.name.as_str());
            }
            Expression::Super(_) => {
                self.push(parent, label, Cat::Super, span, flags);
            }
            Expression::ThisExpression(_) => {
                self.push(parent, label, Cat::This, span, flags);
            }
            Expression::ArrayExpression(it) => {
                let node = self.push(parent, label, Cat::Array, span, flags);
                for element in &it.elements {
                    match element {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            let item = self.push(node, Label::Other, Cat::Spread, range(spread.span), flags);
                            self.expression(item, Label::Expression, &spread.argument, ctx);
                        }
                        // A hole keeps its place, so detectors see every written element.
                        ArrayExpressionElement::Elision(hole) => {
                            self.push(node, Label::Other, Cat::Hole, range(hole.span), flags);
                        }
                        _ => {
                            if let Some(item) = element.as_expression() {
                                self.expression(node, Label::Other, item, ctx);
                            }
                        }
                    }
                }
            }
            Expression::ArrowFunctionExpression(it) => self.arrow(parent, label, it, ctx),
            Expression::AssignmentExpression(it) => {
                let node =
                    self.syntax
                        .push(parent, label, Cat::Assignment, span, flags, it.operator.as_str());
                self.assignment_target(node, Label::Left, &it.left, ctx);
                self.expression(node, Label::Right, &it.right, ctx);
            }
            Expression::AwaitExpression(it) => {
                let node = self.push(parent, label, Cat::Await, span, flags);
                self.expression(node, Label::Expression, &it.argument, ctx);
            }
            Expression::BinaryExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Other, &it.left, ctx);
                self.expression(node, Label::Other, &it.right, ctx);
            }
            Expression::LogicalExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Other, &it.left, ctx);
                self.expression(node, Label::Other, &it.right, ctx);
            }
            Expression::CallExpression(it) => self.call(parent, label, it, ctx),
            Expression::ChainExpression(it) => match &it.expression {
                ChainElement::CallExpression(call) => self.call(parent, label, call, ctx),
                ChainElement::TSNonNullExpression(inner) => {
                    let node = self.push(parent, label, Cat::NonNull, range(inner.span), flags);
                    self.expression(node, Label::Expression, &inner.expression, ctx);
                }
                _ => {
                    if let Some(member) = it.expression.as_member_expression() {
                        self.member(parent, label, member, ctx);
                    }
                }
            },
            Expression::ClassExpression(it) => {
                self.class(parent, label, it, Head::new(Cat::ClassExpression), ctx);
            }
            Expression::ConditionalExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Other, &it.test, ctx);
                self.expression(node, Label::Other, &it.consequent, ctx);
                self.expression(node, Label::Other, &it.alternate, ctx);
            }
            Expression::FunctionExpression(it) => {
                self.function(parent, label, it, Head::new(Cat::FunctionExpression), ctx);
            }
            Expression::ImportExpression(it) => {
                let node = self.push(
                    parent,
                    label,
                    Cat::ImportCall,
                    span,
                    flags.with(Flags::RUNTIME, true),
                );
                self.expression(node, Label::Arguments, &it.source, ctx);
                if let Some(options) = &it.options {
                    self.expression(node, Label::Arguments, options, ctx);
                }
            }
            Expression::NewExpression(it) => {
                let node = self.push(parent, label, Cat::New, span, flags);
                self.expression(node, Label::Expression, &it.callee, ctx);
                if let Some(arguments) = &it.type_arguments {
                    self.push(node, Label::Other, Cat::Type, range(arguments.span), flags);
                }
                self.arguments(node, &it.arguments, ctx);
            }
            Expression::ObjectExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                for property in &it.properties {
                    self.object_property(node, property, ctx);
                }
            }
            Expression::ParenthesizedExpression(it) => {
                let node = self.push(parent, label, Cat::Parenthesized, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            Expression::SequenceExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                for item in &it.expressions {
                    self.expression(node, Label::Other, item, ctx);
                }
            }
            Expression::TaggedTemplateExpression(it) => {
                let node = self.push(parent, label, Cat::TaggedTemplate, span, flags);
                self.expression(node, Label::Tag, &it.tag, ctx);
                if let Some(arguments) = &it.type_arguments {
                    self.push(node, Label::Other, Cat::Type, range(arguments.span), flags);
                }
                self.template(node, Label::Other, &it.quasi, ctx);
            }
            Expression::UnaryExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Other, &it.argument, ctx);
            }
            Expression::UpdateExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.simple_target(node, Label::Other, &it.argument, ctx);
            }
            Expression::YieldExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                if let Some(argument) = &it.argument {
                    self.expression(node, Label::Expression, argument, ctx);
                }
            }
            Expression::PrivateInExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.private_name(node, Label::Other, &it.left);
                self.expression(node, Label::Other, &it.right, ctx);
            }
            Expression::JSXElement(it) => self.jsx_element(parent, label, it, ctx),
            Expression::JSXFragment(it) => self.jsx_fragment(parent, label, it, ctx),
            Expression::TSAsExpression(it) => {
                let node = self.push(parent, label, Cat::As, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
                self.push(
                    node,
                    Label::Type,
                    Cat::Type,
                    range(it.type_annotation.span()),
                    flags,
                );
            }
            Expression::TSSatisfiesExpression(it) => {
                let node = self.push(parent, label, Cat::Satisfies, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
                self.push(
                    node,
                    Label::Type,
                    Cat::Type,
                    range(it.type_annotation.span()),
                    flags,
                );
            }
            Expression::TSTypeAssertion(it) => {
                let node = self.push(parent, label, Cat::TypeAssertion, span, flags);
                self.push(
                    node,
                    Label::Type,
                    Cat::Type,
                    range(it.type_annotation.span()),
                    flags,
                );
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            Expression::TSNonNullExpression(it) => {
                let node = self.push(parent, label, Cat::NonNull, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            Expression::TSInstantiationExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
                self.push(
                    node,
                    Label::Other,
                    Cat::Type,
                    range(it.type_arguments.span),
                    flags,
                );
            }
            Expression::V8IntrinsicExpression(it) => {
                let node = self.push(parent, label, Cat::Other, span, flags);
                self.arguments(node, &it.arguments, ctx);
            }
            _ => {
                if let Some(member) = expression.as_member_expression() {
                    self.member(parent, label, member, ctx);
                }
            }
        }
    }

    fn call(&mut self, parent: u32, label: Label, call: &oxc_ast::ast::CallExpression<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        let node = self.push(parent, label, Cat::Call, range(call.span), flags);
        self.expression(node, Label::Expression, &call.callee, ctx);
        if let Some(arguments) = &call.type_arguments {
            self.push(node, Label::Other, Cat::Type, range(arguments.span), flags);
        }
        self.arguments(node, &call.arguments, ctx);
    }

    fn arguments(&mut self, parent: u32, arguments: &[Argument<'a>], ctx: Ctx) {
        for argument in arguments {
            match argument {
                Argument::SpreadElement(spread) => {
                    let item = self.push(
                        parent,
                        Label::Arguments,
                        Cat::Spread,
                        range(spread.span),
                        ctx.flags(),
                    );
                    self.expression(item, Label::Expression, &spread.argument, ctx);
                }
                _ => {
                    if let Some(expression) = argument.as_expression() {
                        self.expression(parent, Label::Arguments, expression, ctx);
                    }
                }
            }
        }
    }

    pub(super) fn member(&mut self, parent: u32, label: Label, member: &MemberExpression<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        match member {
            MemberExpression::StaticMemberExpression(it) => {
                let node = self.push(parent, label, Cat::PropertyAccess, range(it.span), flags);
                self.expression(node, Label::Expression, &it.object, ctx);
                let name = &it.property;
                self.push_text(node, Label::Name, Cat::Identifier, name.span, name.name.as_str());
            }
            MemberExpression::PrivateFieldExpression(it) => {
                let node = self.push(parent, label, Cat::PropertyAccess, range(it.span), flags);
                self.expression(node, Label::Expression, &it.object, ctx);
                self.private_name(node, Label::Name, &it.field);
            }
            MemberExpression::ComputedMemberExpression(it) => {
                let node = self.push(parent, label, Cat::ElementAccess, range(it.span), flags);
                self.expression(node, Label::Expression, &it.object, ctx);
                self.expression(node, Label::ArgumentExpression, &it.expression, ctx);
            }
        }
    }

    /// A private name; its text keeps the `#`, as written.
    pub(super) fn private_name(
        &mut self,
        parent: u32,
        label: Label,
        name: &oxc_ast::ast::PrivateIdentifier<'a>,
    ) {
        let text = self.allocator.alloc_str(&format!("#{}", name.name.as_str()));
        self.push_text(parent, label, Cat::PrivateIdentifier, name.span, text);
    }

    fn template(&mut self, parent: u32, label: Label, template: &TemplateLiteral<'a>, ctx: Ctx) {
        if template.expressions.is_empty() {
            let cooked = template
                .quasis
                .first()
                .and_then(|quasi| quasi.value.cooked.as_ref())
                .map_or("", |cooked| cooked.as_str());
            self.push_text(parent, label, Cat::NoSubstitutionTemplate, template.span, cooked);
            return;
        }
        let node = self.push(parent, label, Cat::Other, range(template.span), ctx.flags());
        for expression in &template.expressions {
            self.expression(node, Label::Other, expression, ctx);
        }
    }

    fn object_property(&mut self, parent: u32, property: &ObjectPropertyKind<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        match property {
            ObjectPropertyKind::SpreadProperty(spread) => {
                let item = self.push(parent, Label::Other, Cat::Spread, range(spread.span), flags);
                self.expression(item, Label::Expression, &spread.argument, ctx);
            }
            ObjectPropertyKind::ObjectProperty(it) => {
                let span = range(it.span);
                let function = match &it.value {
                    Expression::FunctionExpression(function)
                        if it.method || it.kind != PropertyKind::Init =>
                    {
                        Some(function)
                    }
                    _ => None,
                };
                if let Some(function) = function {
                    let cat = match it.kind {
                        PropertyKind::Get => Cat::Getter,
                        PropertyKind::Set => Cat::Setter,
                        PropertyKind::Init => Cat::Method,
                    };
                    let flags = flags.with(Flags::HAS_BODY, function.body.is_some());
                    let node = self.push(parent, Label::Other, cat, span, flags);
                    // The tree is immutable while it is converted and bound.
                    self.syntax.record_origin(it.unstable_address(), node);
                    self.property_key(node, Label::Name, &it.key, it.computed, ctx);
                    self.function_parts(node, function, ctx);
                } else if it.shorthand {
                    let node = self.push(parent, Label::Other, Cat::Other, span, flags);
                    self.expression(node, Label::Other, &it.value, ctx);
                } else {
                    let node = self.push(parent, Label::Other, Cat::PropertyAssignment, span, flags);
                    self.property_key(node, Label::Name, &it.key, it.computed, ctx);
                    self.expression(node, Label::Initializer, &it.value, ctx);
                }
            }
        }
    }

    /// The target of an assignment, `for…in`/`for…of`, or update.
    pub(super) fn assignment_target(
        &mut self,
        parent: u32,
        label: Label,
        target: &AssignmentTarget<'a>,
        ctx: Ctx,
    ) {
        if !self.enter() {
            return;
        }
        let flags = ctx.flags();
        match target {
            AssignmentTarget::ArrayAssignmentTarget(it) => {
                let node = self.push(parent, label, Cat::Other, range(it.span), flags);
                for element in it.elements.iter().flatten() {
                    self.target_maybe_default(node, element, ctx);
                }
                if let Some(rest) = &it.rest {
                    self.assignment_target(node, Label::Other, &rest.target, ctx);
                }
            }
            AssignmentTarget::ObjectAssignmentTarget(it) => {
                let node = self.push(parent, label, Cat::Other, range(it.span), flags);
                for property in &it.properties {
                    match property {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(item) => {
                            let entry = self.push(node, Label::Other, Cat::Other, range(item.span), flags);
                            let binding = &item.binding;
                            self.push_text(
                                entry,
                                Label::Name,
                                Cat::Identifier,
                                binding.span,
                                binding.name.as_str(),
                            );
                            if let Some(init) = &item.init {
                                self.expression(entry, Label::Other, init, ctx);
                            }
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(item) => {
                            let entry = self.push(node, Label::Other, Cat::Other, range(item.span), flags);
                            self.property_key(entry, Label::Other, &item.name, item.computed, ctx);
                            self.target_maybe_default(entry, &item.binding, ctx);
                        }
                    }
                }
                if let Some(rest) = &it.rest {
                    self.assignment_target(node, Label::Other, &rest.target, ctx);
                }
            }
            _ => {
                if let Some(simple) = target.as_simple_assignment_target() {
                    self.simple_target(parent, label, simple, ctx);
                }
            }
        }
        self.leave();
    }

    fn target_maybe_default(&mut self, parent: u32, target: &AssignmentTargetMaybeDefault<'a>, ctx: Ctx) {
        match target {
            // TypeScript reads a default in an assignment pattern as an assignment, so an
            // anonymous function default takes the target's name.
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(it) => {
                let node = self.syntax.push(
                    parent,
                    Label::Other,
                    Cat::Assignment,
                    range(it.span),
                    ctx.flags(),
                    "=",
                );
                self.assignment_target(node, Label::Left, &it.binding, ctx);
                self.expression(node, Label::Right, &it.init, ctx);
            }
            _ => {
                if let Some(inner) = target.as_assignment_target() {
                    self.assignment_target(parent, Label::Other, inner, ctx);
                }
            }
        }
    }

    fn simple_target(&mut self, parent: u32, label: Label, target: &SimpleAssignmentTarget<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(it) => {
                self.push_text(parent, label, Cat::Identifier, it.span, it.name.as_str());
            }
            SimpleAssignmentTarget::TSAsExpression(it) => {
                let node = self.push(parent, label, Cat::As, range(it.span), flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            SimpleAssignmentTarget::TSSatisfiesExpression(it) => {
                let node = self.push(parent, label, Cat::Satisfies, range(it.span), flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            SimpleAssignmentTarget::TSNonNullExpression(it) => {
                let node = self.push(parent, label, Cat::NonNull, range(it.span), flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            SimpleAssignmentTarget::TSTypeAssertion(it) => {
                let node = self.push(parent, label, Cat::TypeAssertion, range(it.span), flags);
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            _ => {
                if let Some(member) = target.as_member_expression() {
                    self.member(parent, label, member, ctx);
                }
            }
        }
    }

    /// A binding pattern bound by `ctx.binding`. Every element of an object or array pattern is
    /// a binding element whose `name` is the bound pattern and whose `initializer` is its default.
    pub(super) fn pattern(&mut self, parent: u32, label: Label, pattern: &BindingPattern<'a>, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        let flags = ctx.flags();
        match pattern {
            BindingPattern::BindingIdentifier(it) => {
                self.push_text(parent, label, Cat::Identifier, it.span, it.name.as_str());
            }
            BindingPattern::ObjectPattern(it) => {
                let node = self.push(parent, label, Cat::BindingPattern, range(it.span), flags);
                for property in &it.properties {
                    let element = self.binding_element(node, range(property.span), ctx);
                    if !property.shorthand {
                        // The property name is not the bound name.
                        self.property_key(element, Label::Other, &property.key, property.computed, ctx);
                    }
                    self.element_value(element, &property.value, ctx);
                }
                if let Some(rest) = &it.rest {
                    let element = self.binding_element(node, range(rest.span), ctx);
                    self.pattern(element, Label::Name, &rest.argument, ctx);
                }
            }
            BindingPattern::ArrayPattern(it) => {
                let node = self.push(parent, label, Cat::BindingPattern, range(it.span), flags);
                for item in it.elements.iter().flatten() {
                    let element = self.binding_element(node, range(item.span()), ctx);
                    self.element_value(element, item, ctx);
                }
                if let Some(rest) = &it.rest {
                    let element = self.binding_element(node, range(rest.span), ctx);
                    self.pattern(element, Label::Name, &rest.argument, ctx);
                }
            }
            BindingPattern::AssignmentPattern(it) => {
                // Only reachable outside a pattern element (elements unpack defaults themselves).
                let node = self.push(parent, label, Cat::Other, range(it.span), flags);
                self.pattern(node, Label::Name, &it.left, ctx);
                self.expression(node, Label::Initializer, &it.right, ctx);
            }
        }
        self.leave();
    }

    fn binding_element(&mut self, parent: u32, span: unfold_core::ByteRange, ctx: Ctx) -> u32 {
        let flags = ctx
            .flags()
            .with(Flags::VARIABLE_BINDING, ctx.binding == Binding::Variable);
        self.push(parent, Label::Other, Cat::BindingElement, span, flags)
    }

    /// A pattern element's bound name and default value.
    pub(super) fn element_value(&mut self, element: u32, value: &BindingPattern<'a>, ctx: Ctx) {
        match value {
            BindingPattern::AssignmentPattern(it) => {
                self.pattern(element, Label::Name, &it.left, ctx);
                self.expression(element, Label::Initializer, &it.right, ctx);
            }
            _ => self.pattern(element, Label::Name, value, ctx),
        }
    }

    fn jsx_element(&mut self, parent: u32, label: Label, element: &JSXElement<'a>, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        let node = self.push(parent, label, Cat::Other, range(element.span), ctx.flags());
        for attribute in &element.opening_element.attributes {
            match attribute {
                JSXAttributeItem::Attribute(it) => match &it.value {
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        self.jsx_expression(node, &container.expression, ctx);
                    }
                    Some(JSXAttributeValue::Element(inner)) => {
                        self.jsx_element(node, Label::Other, inner, ctx);
                    }
                    Some(JSXAttributeValue::Fragment(inner)) => {
                        self.jsx_fragment(node, Label::Other, inner, ctx);
                    }
                    Some(JSXAttributeValue::StringLiteral(_)) | None => {}
                },
                JSXAttributeItem::SpreadAttribute(it) => {
                    let item = self.push(node, Label::Other, Cat::Spread, range(it.span), ctx.flags());
                    self.expression(item, Label::Expression, &it.argument, ctx);
                }
            }
        }
        self.jsx_children(node, &element.children, ctx);
        self.leave();
    }

    fn jsx_fragment(&mut self, parent: u32, label: Label, fragment: &JSXFragment<'a>, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        let node = self.push(parent, label, Cat::Other, range(fragment.span), ctx.flags());
        self.jsx_children(node, &fragment.children, ctx);
        self.leave();
    }

    fn jsx_children(&mut self, parent: u32, children: &[JSXChild<'a>], ctx: Ctx) {
        for child in children {
            match child {
                JSXChild::Element(inner) => self.jsx_element(parent, Label::Other, inner, ctx),
                JSXChild::Fragment(inner) => self.jsx_fragment(parent, Label::Other, inner, ctx),
                JSXChild::ExpressionContainer(container) => {
                    self.jsx_expression(parent, &container.expression, ctx);
                }
                JSXChild::Spread(spread) => self.expression(parent, Label::Other, &spread.expression, ctx),
                JSXChild::Text(_) => {}
            }
        }
    }

    fn jsx_expression(&mut self, parent: u32, expression: &JSXExpression<'a>, ctx: Ctx) {
        if let Some(expression) = expression.as_expression() {
            self.expression(parent, Label::Other, expression, ctx);
        }
    }
}
