//! Callables (functions, arrows, methods, parameters) and classes.

use oxc_allocator::UnstableAddress;
use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, Class, ClassElement, Decorator, FormalParameter, Function,
    FunctionBody, MethodDefinitionKind, PropertyKey,
};
use oxc_parser::Kind;
use oxc_span::GetSpan;
use unfold_core::ByteRange;

use super::{Binding, Builder, Ctx, Head, range};
use crate::syntax::{Cat, Flags, Label};

impl<'a> Builder<'a, '_> {
    /// A function declaration or expression.
    pub(super) fn function(
        &mut self,
        parent: u32,
        label: Label,
        function: &Function<'a>,
        head: Head,
        ctx: Ctx,
    ) {
        if !self.enter() {
            return;
        }
        let ctx = ctx.ambient(function.declare);
        let mut span = range(function.span);
        if let Some(start) = head.export_start {
            span.start = span.start.min(start);
        }
        let flags = Flags::default()
            .with(Flags::DEFAULT, head.default)
            .with(Flags::AMBIENT, ctx.ambient)
            .with(Flags::HAS_BODY, function.body.is_some());
        let node = self.push(parent, label, head.cat, span, flags);
        // The tree is immutable while it is converted and bound, so addresses are stable.
        self.syntax.record_origin(function.unstable_address(), node);
        if let Some(id) = &function.id {
            self.push_text(node, Label::Name, Cat::Identifier, id.span, id.name.as_str());
        }
        self.function_parts(node, function, ctx);
        self.leave();
    }

    /// Type parameters, parameters (an explicit `this` first), return type, and body.
    pub(super) fn function_parts(&mut self, node: u32, function: &Function<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        if let Some(parameters) = &function.type_parameters {
            self.push(node, Label::Other, Cat::Type, range(parameters.span), flags);
        }
        if let Some(this) = &function.this_param {
            let parameter = self.push(node, Label::Parameters, Cat::Parameter, range(this.span), flags);
            self.syntax.push(
                parameter,
                Label::Name,
                Cat::Identifier,
                range(this.this_span),
                flags,
                "this",
            );
            if let Some(annotation) = &this.type_annotation {
                self.type_annotation(parameter, Label::Type, annotation);
            }
        }
        self.parameters(node, &function.params, ctx);
        if let Some(annotation) = &function.return_type {
            self.type_annotation(node, Label::Type, annotation);
        }
        if let Some(body) = &function.body {
            self.body(node, body, ctx);
        }
    }

    fn body(&mut self, node: u32, body: &FunctionBody<'a>, ctx: Ctx) {
        let block = self.push(node, Label::Body, Cat::Other, range(body.span), ctx.flags());
        self.directives(block, &body.directives);
        self.statements(block, &body.statements, ctx);
    }

    pub(super) fn arrow(&mut self, parent: u32, label: Label, arrow: &ArrowFunctionExpression<'a>, ctx: Ctx) {
        let flags = ctx.flags().with(Flags::HAS_BODY, true);
        let node = self.push(parent, label, Cat::ArrowFunction, range(arrow.span), flags);
        if let Some(parameters) = &arrow.type_parameters {
            self.push(node, Label::Other, Cat::Type, range(parameters.span), flags);
        }
        self.parameters(node, &arrow.params, ctx);
        if let Some(annotation) = &arrow.return_type {
            self.type_annotation(node, Label::Type, annotation);
        }
        match &arrow.body {
            ArrowFunctionBody::FunctionBody(body) => self.body(node, body, ctx),
            _ => {
                if let Some(expression) = arrow.body.as_expression() {
                    self.expression(node, Label::Body, expression, ctx);
                }
            }
        }
    }

    fn parameters(&mut self, node: u32, parameters: &oxc_ast::ast::FormalParameters<'a>, ctx: Ctx) {
        let binding = ctx.binding(Binding::Parameter);
        for parameter in &parameters.items {
            self.parameter(node, parameter, binding);
        }
        if let Some(rest) = &parameters.rest {
            let item = self.push(
                node,
                Label::Parameters,
                Cat::Parameter,
                range(rest.span),
                ctx.flags(),
            );
            self.decorators(item, &rest.decorators, ctx);
            self.pattern(item, Label::Name, &rest.rest.argument, binding);
            if let Some(annotation) = &rest.type_annotation {
                self.type_annotation(item, Label::Type, annotation);
            }
        }
    }

    fn parameter(&mut self, node: u32, parameter: &FormalParameter<'a>, ctx: Ctx) {
        let item = self.push(
            node,
            Label::Parameters,
            Cat::Parameter,
            range(parameter.span),
            ctx.flags(),
        );
        self.decorators(item, &parameter.decorators, ctx);
        match (&parameter.pattern, &parameter.initializer) {
            (oxc_ast::ast::BindingPattern::AssignmentPattern(it), None) => {
                self.pattern(item, Label::Name, &it.left, ctx);
                if let Some(annotation) = &parameter.type_annotation {
                    self.type_annotation(item, Label::Type, annotation);
                }
                self.expression(item, Label::Initializer, &it.right, ctx);
            }
            (pattern, initializer) => {
                self.pattern(item, Label::Name, pattern, ctx);
                if let Some(annotation) = &parameter.type_annotation {
                    self.type_annotation(item, Label::Type, annotation);
                }
                if let Some(initializer) = initializer {
                    self.expression(item, Label::Initializer, initializer, ctx);
                }
            }
        }
    }

    fn decorators(&mut self, parent: u32, decorators: &[Decorator<'a>], ctx: Ctx) {
        for decorator in decorators {
            let node = self.push(
                parent,
                Label::Other,
                Cat::Decorator,
                range(decorator.span),
                ctx.flags(),
            );
            self.expression(node, Label::Expression, &decorator.expression, ctx);
        }
    }

    /// A class. Its range starts at its first decorator or at a preceding `export` keyword,
    /// whichever comes first.
    pub(super) fn class(&mut self, parent: u32, label: Label, class: &Class<'a>, head: Head, ctx: Ctx) {
        if !self.enter() {
            return;
        }
        let ctx = ctx.ambient(class.declare);
        let mut span = range(class.span);
        for start in class
            .decorators
            .first()
            .map(|decorator| decorator.span.start)
            .into_iter()
            .chain(head.export_start)
        {
            span.start = span.start.min(start);
        }
        let flags = Flags::default()
            .with(Flags::DEFAULT, head.default)
            .with(Flags::AMBIENT, ctx.ambient);
        let node = self.push(parent, label, head.cat, span, flags);
        self.syntax.record_origin(class.unstable_address(), node);
        self.decorators(node, &class.decorators, ctx);
        if let Some(id) = &class.id {
            self.push_text(node, Label::Name, Cat::Identifier, id.span, id.name.as_str());
        }
        if let Some(parameters) = &class.type_parameters {
            self.push(node, Label::Other, Cat::Type, range(parameters.span), ctx.flags());
        }
        if let Some(heritage) = &class.heritage {
            // The clause starts at `extends` and ends after the base's type arguments.
            let expression = heritage.expression.span();
            let start = self
                .token_before(expression.start)
                .filter(|token| token.kind() == Kind::Extends)
                .map_or(expression.start, |token| token.start());
            let end = heritage
                .type_arguments
                .as_ref()
                .map_or(expression.end, |arguments| arguments.span.end);
            let clause = self.push(
                node,
                Label::HeritageClauses,
                Cat::Other,
                ByteRange { start, end },
                ctx.flags(),
            );
            self.expression(clause, Label::Other, &heritage.expression, ctx);
        }
        if let (Some(first), Some(last)) = (class.implements.first(), class.implements.last()) {
            let start = self
                .token_before(first.span.start)
                .filter(|token| token.kind() == Kind::Implements)
                .map_or(first.span.start, |token| token.start());
            let clause = ByteRange {
                start,
                end: last.span.end,
            };
            self.push(node, Label::HeritageClauses, Cat::Type, clause, ctx.flags());
        }
        for element in &class.body.body {
            self.class_element(node, element, ctx);
        }
        self.leave();
    }

    fn class_element(&mut self, node: u32, element: &ClassElement<'a>, ctx: Ctx) {
        let flags = ctx.flags();
        match element {
            ClassElement::MethodDefinition(it) => {
                let cat = match it.kind {
                    MethodDefinitionKind::Constructor => Cat::Constructor,
                    MethodDefinitionKind::Method => Cat::Method,
                    MethodDefinitionKind::Get => Cat::Getter,
                    MethodDefinitionKind::Set => Cat::Setter,
                };
                let flags = flags.with(Flags::HAS_BODY, it.value.body.is_some());
                let member = self.push(node, Label::Members, cat, range(it.span), flags);
                self.syntax.record_origin(it.unstable_address(), member);
                self.decorators(member, &it.decorators, ctx);
                // A constructor has no written name child.
                if cat != Cat::Constructor {
                    self.property_key(member, Label::Name, &it.key, it.computed, ctx);
                }
                self.function_parts(member, &it.value, ctx);
            }
            ClassElement::PropertyDefinition(it) => {
                let ctx = ctx.ambient(it.declare);
                let member = self.push(node, Label::Members, Cat::Property, range(it.span), ctx.flags());
                self.decorators(member, &it.decorators, ctx);
                self.property_key(member, Label::Name, &it.key, it.computed, ctx);
                if let Some(annotation) = &it.type_annotation {
                    self.type_annotation(member, Label::Type, annotation);
                }
                if let Some(value) = &it.value {
                    self.expression(member, Label::Initializer, value, ctx);
                }
            }
            ClassElement::AccessorProperty(it) => {
                let member = self.push(node, Label::Members, Cat::Property, range(it.span), flags);
                self.decorators(member, &it.decorators, ctx);
                self.property_key(member, Label::Name, &it.key, it.computed, ctx);
                if let Some(annotation) = &it.type_annotation {
                    self.type_annotation(member, Label::Type, annotation);
                }
                if let Some(value) = &it.value {
                    self.expression(member, Label::Initializer, value, ctx);
                }
            }
            ClassElement::StaticBlock(it) => {
                let member = self.push(node, Label::Members, Cat::Other, range(it.span), flags);
                self.statements(member, &it.body, ctx);
            }
            ClassElement::TSIndexSignature(it) => {
                self.push(node, Label::Members, Cat::Type, range(it.span), flags);
            }
        }
    }

    /// A member or property name filling `label`. A computed name is a node spanning its
    /// brackets.
    pub(super) fn property_key(
        &mut self,
        parent: u32,
        label: Label,
        key: &PropertyKey<'a>,
        computed: bool,
        ctx: Ctx,
    ) {
        match key {
            PropertyKey::StaticIdentifier(it) => {
                self.push_text(parent, label, Cat::Identifier, it.span, it.name.as_str());
            }
            PropertyKey::PrivateIdentifier(it) => self.private_name(parent, label, it),
            _ => {
                let Some(expression) = key.as_expression() else {
                    return;
                };
                if !computed {
                    self.expression(parent, label, expression, ctx);
                    return;
                }
                let inner = expression.span();
                let start = self
                    .token_before(inner.start)
                    .filter(|token| token.kind() == Kind::LBrack)
                    .map_or(inner.start, |token| token.start());
                let end = self
                    .token_from(inner.end, Kind::RBrack)
                    .map_or(inner.end, |token| token.end());
                let node = self.push(
                    parent,
                    label,
                    Cat::ComputedPropertyName,
                    ByteRange { start, end },
                    ctx.flags(),
                );
                self.expression(node, Label::Expression, expression, ctx);
            }
        }
    }
}
