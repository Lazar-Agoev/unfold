//! Definitions: functions in every form, classes, type declarations, decorators and bases,
//! namespaces, and module syntax.

use oxc_allocator::{Address, GetAddress, UnstableAddress};
use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, Class, ClassElement, ClassType, Decorator,
    ExportAllDeclaration, ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    Expression, FormalParameters, Function, FunctionBody, FunctionType, ImportDeclaration,
    MethodDefinitionKind, ObjectExpression, ObjectProperty, ObjectPropertyKind, PropertyKind,
    TSEnumDeclaration, TSImportEqualsDeclaration, TSInterfaceDeclaration, TSModuleReference,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSThisParameter, TSTypeAliasDeclaration,
    TSTypeAnnotation,
};
use oxc_ast_visit::Visit;
use oxc_parser::Kind;
use oxc_span::{GetSpan, Span};
use unfold_core::key::Child;
use unfold_core::text::ByteRange;
use unfold_core::vocab::{Flag, Flags, Form, Inline, NodeKind};
use unfold_engine::facts::{CallContext, ImportFact};

use super::expressions::CallNode;
use super::{At, Lowering, Within, forms};
use crate::imports;
use crate::payload::ImportKind;
use crate::spans::range;
use crate::typed::Callable;
use crate::values;

/// The name a definition is keyed by, and where a JSDoc before it would end.
pub(super) struct Naming {
    pub(super) name: String,
    pub(super) doc: Option<u32>,
}

/// A function's code: a block, or an arrow function's expression.
enum Code<'s, 'a> {
    Block(&'s FunctionBody<'a>),
    Expression(&'s Expression<'a>),
}

/// Everything a function node is made from.
struct Definition<'s, 'a> {
    address: Address,
    form: Form,
    naming: Option<Naming>,
    /// The property a callback is written as (`onSuccess` in `useMutation({ onSuccess: … })`),
    /// which labels it; its key stays `callback[n]`.
    property: Option<String>,
    range: ByteRange,
    head: ByteRange,
    flags: Flags,
    this: Option<&'s TSThisParameter<'a>>,
    parameters: &'s FormalParameters<'a>,
    returns: Option<&'s TSTypeAnnotation<'a>>,
    code: Code<'s, 'a>,
}

impl Lowering<'_> {
    /// A function node: named, or else a callback in a call's arguments (with the name of the
    /// property it is written as, if it is one) and a lambda anywhere else. Its parameter
    /// defaults are its first code, then its body.
    fn define(&mut self, definition: Definition<'_, '_>) -> Option<u32> {
        let (child, form, name) = match &definition.naming {
            Some(naming) => (
                Child::Definition(&naming.name),
                definition.form,
                Some(&*naming.name),
            ),
            None if self.at.within == Within::Argument => (
                Child::Inline(Inline::Callback),
                Form::Lambda,
                definition.property.as_deref(),
            ),
            None => (Child::Inline(Inline::Lambda), Form::Lambda, None),
        };
        let id = self.push(self.at.parent, child, form, name, definition.range)?;
        let body = match definition.code {
            Code::Block(block) => range(block.span),
            Code::Expression(expression) => range(expression.span()),
        };
        let doc = definition
            .naming
            .and_then(|naming| naming.doc)
            .and_then(|anchor| self.doc_before(anchor));
        let node = self.node(id);
        node.head = Some(definition.head);
        node.body = Some(body);
        node.doc = doc;
        node.flags = definition.flags;
        self.definitions.insert(definition.address, id);
        self.typed.function(
            id,
            &Callable {
                this: definition.this,
                parameters: definition.parameters,
                returns: definition.returns,
            },
        );
        let saved = self.enter(At {
            parent: id,
            within: Within::Default,
        });
        for parameter in &definition.parameters.items {
            self.visit_formal_parameter(parameter);
        }
        if let Some(rest) = &definition.parameters.rest {
            self.visit_formal_parameter_rest(rest);
        }
        self.at.within = Within::Body;
        match definition.code {
            Code::Block(block) => self.statements(&block.statements),
            Code::Expression(expression) => self.visit_expression(expression),
        }
        self.leave(saved);
        Some(id)
    }

    /// The end of a signature: its return type, or else its parameter list.
    fn signature_end(parameters: &FormalParameters<'_>, returns: Option<&TSTypeAnnotation<'_>>) -> u32 {
        returns.map_or(parameters.span.end, |returns| returns.type_annotation.span().end)
    }

    /// A function declaration or expression. A declaration is named by its identifier (or the
    /// `naming` given, for an anonymous default export) and starts at its `export`; an
    /// expression is named only by `naming`, or labeled by the `property` it is written as. One
    /// without a body has no code.
    pub(super) fn function(
        &mut self,
        it: &Function<'_>,
        naming: Option<Naming>,
        property: Option<String>,
    ) -> Option<u32> {
        let body = it.body.as_ref()?;
        let declaration = it.r#type == FunctionType::FunctionDeclaration;
        let span = range(it.span);
        let start = if declaration {
            self.export.take().unwrap_or(span.start)
        } else {
            span.start
        };
        let naming = match naming {
            Some(naming) => Some(naming),
            None if declaration => it.id.as_ref().map(|id| Naming {
                name: format!("{}{}", self.prefix, id.name),
                doc: Some(start),
            }),
            None => None,
        };
        let form = if declaration { Form::Function } else { Form::Lambda };
        let mut flags = Flags::NONE;
        if it.r#async {
            flags.insert(Flag::Async);
        }
        if it.generator {
            flags.insert(Flag::Generator);
        }
        self.define(Definition {
            address: it.unstable_address(),
            form,
            naming,
            property,
            range: ByteRange { start, end: span.end },
            head: ByteRange {
                start: span.start,
                end: Self::signature_end(&it.params, it.return_type.as_deref()),
            },
            flags,
            this: it.this_param.as_deref(),
            parameters: &it.params,
            returns: it.return_type.as_deref(),
            code: Code::Block(body),
        })
    }

    /// An arrow function, named only by `naming`, or labeled by the `property` it is written as.
    /// Its head runs through its `=>`.
    pub(super) fn arrow(
        &mut self,
        it: &ArrowFunctionExpression<'_>,
        naming: Option<Naming>,
        property: Option<String>,
    ) -> Option<u32> {
        let signature = Self::signature_end(&it.params, it.return_type.as_deref());
        let head_end = self
            .tokens
            .find(signature, Kind::Arrow)
            .map_or(signature, |token| token.end());
        let code = match &it.body {
            ArrowFunctionBody::FunctionBody(body) => Code::Block(body),
            body => Code::Expression(body.as_expression()?),
        };
        let flags = if it.r#async {
            Flags::NONE.with(Flag::Async)
        } else {
            Flags::NONE
        };
        self.define(Definition {
            address: it.unstable_address(),
            form: Form::Lambda,
            naming,
            property,
            range: range(it.span),
            head: ByteRange {
                start: it.span.start,
                end: head_end,
            },
            flags,
            this: None,
            parameters: &it.params,
            returns: it.return_type.as_deref(),
            code,
        })
    }

    /// A method, constructor, or accessor of a class or object literal, at the oxc node
    /// `address` (its member or property), written at `range`: named by `naming`, or a lambda
    /// (labeled by its `property` when it is a callback).
    fn method(
        &mut self,
        address: Address,
        function: &Function<'_>,
        form: Form,
        naming: Option<Naming>,
        property: Option<String>,
        range: ByteRange,
    ) -> Option<u32> {
        let body = function.body.as_ref()?;
        let mut flags = Flags::NONE;
        if function.r#async {
            flags.insert(Flag::Async);
        }
        if function.generator {
            flags.insert(Flag::Generator);
        }
        self.define(Definition {
            address,
            form,
            naming,
            property,
            range,
            head: ByteRange {
                start: range.start,
                end: Self::signature_end(&function.params, function.return_type.as_deref()),
            },
            flags,
            this: function.this_param.as_deref(),
            parameters: &function.params,
            returns: function.return_type.as_deref(),
            code: Code::Block(body),
        })
    }

    /// Lowers `value` named `name` (qualified by the namespaces around it) when it is a
    /// function, class, or object literal written in a body, and returns the definition it
    /// named; any other value is lowered as it is.
    pub(super) fn named_value(&mut self, value: &Expression<'_>, name: &str) -> Option<u32> {
        if self.at.within != Within::Body {
            self.visit_expression(value);
            return None;
        }
        let naming = Naming {
            name: format!("{}{name}", self.prefix),
            doc: self.enclosing.map(|statement| statement.start),
        };
        self.named(value, naming)
    }

    /// Lowers `value` with this naming when it is a function, class, or object literal.
    fn named(&mut self, value: &Expression<'_>, naming: Naming) -> Option<u32> {
        match values::unwrap(value) {
            Expression::FunctionExpression(it) => self.function(it, Some(naming), None),
            Expression::ArrowFunctionExpression(it) => self.arrow(it, Some(naming), None),
            Expression::ClassExpression(it) => self.class(it, Some(naming)),
            Expression::ObjectExpression(it) => {
                self.object(it, Some(&naming.name));
                None
            }
            _ => {
                self.visit_expression(value);
                None
            }
        }
    }

    /// An object literal. The members of one a declaration names (`name`) are named
    /// definitions qualified by it (`handlers.onCreate`); in any other, a method is an inline
    /// function written from its key, and in a call's arguments a function-valued property is a
    /// callback labeled by its property name.
    pub(super) fn object(&mut self, it: &ObjectExpression<'_>, name: Option<&str>) {
        for property in &it.properties {
            match property {
                ObjectPropertyKind::SpreadProperty(spread) => self.visit_expression(&spread.argument),
                ObjectPropertyKind::ObjectProperty(property) => self.property(property, name),
            }
        }
    }

    fn property(&mut self, property: &ObjectProperty<'_>, object: Option<&str>) {
        if property.computed
            && let Some(key) = property.key.as_expression()
        {
            self.visit_expression(key);
        }
        let naming = object
            .filter(|_| self.at.within == Within::Body)
            .and_then(|object| {
                forms::property_name(&property.key, property.computed).map(|key| (object, key))
            })
            .map(|(object, key)| Naming {
                name: format!("{object}.{key}"),
                doc: Some(property.span.start),
            });
        let label = (naming.is_none() && self.at.within == Within::Argument)
            .then(|| forms::property_name(&property.key, property.computed))
            .flatten();
        match &property.value {
            Expression::FunctionExpression(function)
                if property.method || property.kind != PropertyKind::Init =>
            {
                let form = match property.kind {
                    PropertyKind::Get => Form::Getter,
                    PropertyKind::Set => Form::Setter,
                    PropertyKind::Init => Form::Method,
                };
                self.method(
                    property.unstable_address(),
                    function,
                    form,
                    naming,
                    label,
                    range(property.span),
                );
            }
            value => match (naming, label) {
                (Some(naming), _) => {
                    self.named(value, naming);
                }
                (None, Some(label)) => match values::unwrap(value) {
                    Expression::FunctionExpression(it) => {
                        self.function(it, None, Some(label));
                    }
                    Expression::ArrowFunctionExpression(it) => {
                        self.arrow(it, None, Some(label));
                    }
                    _ => self.visit_expression(value),
                },
                (None, None) => self.visit_expression(value),
            },
        }
    }

    /// The start of what follows `decorators` in `span`: the first token after the last
    /// decorator written inside it.
    fn after_decorators(&self, span: Span, decorators: &[Decorator<'_>]) -> u32 {
        match decorators.last() {
            Some(last) if last.span.end > span.start => self
                .tokens
                .from(last.span.end)
                .map_or(span.start, |token| token.start()),
            _ => span.start,
        }
    }

    /// Lowers `lower` at `within` in the enclosing body and returns the calls it wrote there
    /// with that role (decorators and bases), which belong to the definition after
    /// them.
    fn belonging(&mut self, within: Within, lower: impl FnOnce(&mut Self)) -> Vec<u32> {
        let before = self.nodes.len();
        let parent = self.at.parent;
        let outer = std::mem::replace(&mut self.at.within, within);
        lower(self);
        self.at.within = outer;
        let role = within.role();
        (before..self.nodes.len())
            .filter(|&id| self.nodes[id].parent == Some(parent) && self.nodes[id].role == role)
            .map(|id| id as u32)
            .collect()
    }

    /// Gives `calls` (decorators or bases) the definition they belong to; without one they
    /// are plain calls.
    fn belong(&mut self, calls: Vec<u32>, definition: Option<u32>) {
        for call in calls {
            let node = self.node(call);
            match definition {
                Some(definition) => node.r#for = Some(definition),
                None => node.role = None,
            }
        }
    }

    /// Decorators, each a call in the enclosing body with `within`'s role (a plain name or
    /// member is a call of its own).
    fn decorators(&mut self, decorators: &[Decorator<'_>], within: Within) -> Vec<u32> {
        self.belonging(within, |lowering| {
            for decorator in decorators {
                lowering.decorator(decorator);
            }
        })
    }

    /// One decorator: its call, or, for a decorator that isn't a call, a call node of its
    /// expression. A decorator is part of its definition's statement, so none of its calls
    /// begins one.
    pub(super) fn decorator(&mut self, decorator: &Decorator<'_>) {
        let statement = self.statement.take();
        let expression = &decorator.expression;
        if decorator_is_call(expression) {
            self.visit_expression(expression);
        } else {
            let value = self.values.value(expression, self.source);
            let written = range(expression.span());
            let call = self.call_node(CallNode {
                address: expression.address(),
                range: written,
                head: written,
                callee: forms::callee(expression),
                value,
                form: Form::Call,
                context: CallContext::Call,
                first_literal: None,
                takes_callback: false,
                arguments: Vec::new(),
                after_optional: false,
            });
            if let Some(call) = call {
                self.within(call, Within::Value, |lowering| {
                    lowering.visit_expression(expression);
                });
            }
        }
        self.statement = statement;
    }

    /// A class, named by `naming`, or by its identifier when it is a declaration or a class
    /// expression written in a body. Its decorators and its base, when that is a call, are calls
    /// in the enclosing body before it; its members are its code. It is the first node of its
    /// statement, which runs from its first decorator. A class expression with no name can't be
    /// keyed: it is left out, and the node around it is truncated.
    pub(super) fn class(&mut self, it: &Class<'_>, naming: Option<Naming>) -> Option<u32> {
        if it.declare {
            return None;
        }
        let declaration = it.r#type == ClassType::ClassDeclaration;
        let export = if declaration { self.export.take() } else { None };
        let start = self.after_decorators(it.span, &it.decorators);
        let doc = it
            .decorators
            .first()
            .map(|decorator| decorator.span.start)
            .or(export)
            .unwrap_or(start);
        let naming = match naming {
            Some(naming) => Some(naming),
            None if declaration || self.at.within == Within::Body => it.id.as_ref().map(|id| Naming {
                name: format!("{}{}", self.prefix, id.name),
                doc: Some(doc),
            }),
            None => None,
        };
        let Some(naming) = naming.filter(|_| self.at.within == Within::Body) else {
            let first = it
                .decorators
                .first()
                .map_or(it.span.start, |decorator| decorator.span.start);
            self.truncate(self.at.parent);
            self.cuts.push(ByteRange {
                start: first.min(it.span.start),
                end: it.span.end,
            });
            return None;
        };
        let range_start = match (export, it.decorators.last()) {
            (Some(export), Some(last)) if export >= last.span.end => export,
            (Some(export), None) => export,
            _ => start,
        };
        let span = ByteRange {
            start: range_start,
            end: it.span.end,
        };
        // `@dec export class A {}` is a statement that starts at `export`.
        let statement = self
            .statement
            .take_if(|statement| statement.contains(span))
            .map(|statement| ByteRange {
                start: it
                    .decorators
                    .first()
                    .map_or(statement.start, |first| first.span.start.min(statement.start)),
                end: statement.end,
            });
        let decorators = self.decorators(&it.decorators, Within::Decorator);
        let bases = match &it.heritage {
            Some(heritage) => self.belonging(Within::Base, |lowering| {
                lowering.visit_expression(&heritage.expression);
            }),
            None => Vec::new(),
        };
        let body = range(it.body.span);
        let head_end = self
            .tokens
            .before(body.start)
            .map_or(body.start, |token| token.end());
        let id = self.push(
            self.at.parent,
            Child::Definition(&naming.name),
            Form::Class,
            Some(&naming.name),
            span,
        );
        self.belong(decorators, id);
        self.belong(bases, id);
        let id = id?;
        let doc = naming.doc.and_then(|anchor| self.doc_before(anchor));
        let node = self.node(id);
        node.head = Some(ByteRange { start, end: head_end });
        node.body = Some(body);
        node.doc = doc;
        node.stmt = statement;
        self.definitions.insert(it.unstable_address(), id);
        self.typed.class(id, &it.body.body);
        let saved = self.enter(At {
            parent: id,
            within: Within::Body,
        });
        for element in &it.body.body {
            self.class_element(element);
        }
        self.leave(saved);
        Some(id)
    }

    /// One member of the class being lowered: its decorators and computed key are class-level
    /// code, then the member itself.
    fn class_element(&mut self, element: &ClassElement<'_>) {
        let span = range(element.span());
        match element {
            ClassElement::MethodDefinition(it) => {
                let mut decorators = self.decorators(&it.decorators, Within::Decorator);
                if it.computed
                    && let Some(key) = it.key.as_expression()
                {
                    self.visit_expression(key);
                }
                let parameters = it
                    .value
                    .params
                    .items
                    .iter()
                    .map(|parameter| &parameter.decorators);
                let rest = it.value.params.rest.iter().map(|rest| &rest.decorators);
                for list in parameters.chain(rest) {
                    decorators.extend(self.decorators(list, Within::Decorator));
                }
                let (form, name) = match it.kind {
                    MethodDefinitionKind::Constructor => (Form::Constructor, Some("constructor".to_owned())),
                    MethodDefinitionKind::Method => {
                        (Form::Method, forms::property_name(&it.key, it.computed))
                    }
                    MethodDefinitionKind::Get => (Form::Getter, forms::property_name(&it.key, it.computed)),
                    MethodDefinitionKind::Set => (Form::Setter, forms::property_name(&it.key, it.computed)),
                };
                let naming = name.map(|name| Naming {
                    name,
                    doc: Some(span.start),
                });
                let start = self.after_decorators(it.span, &it.decorators);
                let written = ByteRange { start, end: span.end };
                let id = self.method(it.unstable_address(), &it.value, form, naming, None, written);
                // A member is a statement of the class body, from its first decorator; the method
                // is its first node.
                if let Some(id) = id {
                    self.node(id).stmt = Some(span);
                }
                self.belong(decorators, id);
            }
            ClassElement::PropertyDefinition(it) => {
                let value = it.value.as_ref().filter(|_| !it.declare);
                self.class_property(span, &it.decorators, &it.key, it.computed, value);
            }
            ClassElement::AccessorProperty(it) => {
                self.class_property(span, &it.decorators, &it.key, it.computed, it.value.as_ref());
            }
            ClassElement::StaticBlock(it) => self.statements(&it.body),
            ClassElement::TSIndexSignature(_) => {}
        }
    }

    /// A class field or `accessor` property: a function, class, or object literal as its value
    /// is named by its key (and its decorators belong to that definition); any other value is
    /// class-level code whose call or conditional expression binds the key.
    fn class_property(
        &mut self,
        span: ByteRange,
        decorators: &[Decorator<'_>],
        key: &oxc_ast::ast::PropertyKey<'_>,
        computed: bool,
        value: Option<&Expression<'_>>,
    ) {
        let name = forms::property_name(key, computed);
        let defines = name.is_some()
            && value.is_some_and(|value| {
                matches!(
                    values::unwrap(value),
                    Expression::FunctionExpression(_)
                        | Expression::ArrowFunctionExpression(_)
                        | Expression::ClassExpression(_)
                )
            });
        // A field isn't a definition, so its decorators have nothing for `for` to name: they are
        // plain calls of the class body.
        let within = if defines { Within::Decorator } else { Within::Body };
        let decorators = self.decorators(decorators, within);
        if computed && let Some(key) = key.as_expression() {
            self.visit_expression(key);
        }
        let Some(value) = value else {
            self.belong(decorators, None);
            return;
        };
        let enclosing = self.enclosing.replace(span);
        self.statement = Some(span);
        let defined = match name {
            Some(name) => {
                self.bind_value(value, &name);
                self.named(
                    value,
                    Naming {
                        name,
                        doc: Some(span.start),
                    },
                )
            }
            None => {
                self.visit_expression(value);
                None
            }
        };
        self.statement = None;
        self.enclosing = enclosing;
        self.belong(decorators, defined);
    }

    /// A type declaration of `form` named `name`, starting at its `export`, with its head and
    /// body; `None` when it is ambient.
    fn type_declaration(
        &mut self,
        address: Address,
        form: Form,
        name: &str,
        span: Span,
        head_end: u32,
        body: ByteRange,
    ) -> Option<u32> {
        let start = self.export.take().unwrap_or(span.start);
        let name = format!("{}{name}", self.prefix);
        let written = ByteRange { start, end: span.end };
        let id = self.push(
            self.at.parent,
            Child::Definition(&name),
            form,
            Some(&name),
            written,
        )?;
        let doc = self.doc_before(start);
        let node = self.node(id);
        node.head = Some(ByteRange {
            start: span.start,
            end: head_end,
        });
        node.body = Some(body);
        node.doc = doc;
        self.definitions.insert(address, id);
        Some(id)
    }

    pub(super) fn interface(&mut self, it: &TSInterfaceDeclaration<'_>) {
        if it.declare {
            return;
        }
        let body = range(it.body.span);
        let head_end = self
            .tokens
            .before(body.start)
            .map_or(body.start, |token| token.end());
        let address = it.unstable_address();
        if let Some(id) =
            self.type_declaration(address, Form::Interface, &it.id.name, it.span, head_end, body)
        {
            self.typed.interface(id, it);
        }
    }

    pub(super) fn type_alias(&mut self, it: &TSTypeAliasDeclaration<'_>) {
        if it.declare {
            return;
        }
        let head_end = it
            .type_parameters
            .as_ref()
            .map_or(it.id.span.end, |parameters| parameters.span.end);
        let body = range(it.type_annotation.span());
        let address = it.unstable_address();
        if let Some(id) =
            self.type_declaration(address, Form::TypeAlias, &it.id.name, it.span, head_end, body)
        {
            self.typed.type_alias(id, it);
        }
    }

    /// An enum: a `type` node. Its members' initializers run when the enum is created, so
    /// their calls are code of the enclosing body, after it.
    pub(super) fn enumeration(&mut self, it: &TSEnumDeclaration<'_>) {
        if it.declare {
            return;
        }
        let body = range(it.body.span);
        let head_end = self
            .tokens
            .before(body.start)
            .map_or(body.start, |token| token.end());
        let address = it.unstable_address();
        if let Some(id) = self.type_declaration(address, Form::Enum, &it.id.name, it.span, head_end, body) {
            self.typed.enumeration(id, it);
        }
        for member in &it.body.members {
            if let Some(initializer) = &member.initializer {
                self.visit_expression(initializer);
            }
        }
    }

    /// A namespace: no node; its code is the enclosing body's, its definitions qualified by its
    /// name (`N.f`).
    pub(super) fn namespace(&mut self, it: &TSNamespaceDeclaration<'_>) {
        if it.declare {
            return;
        }
        self.export = None;
        let outer = self.prefix.len();
        self.prefix.push_str(&it.id.name);
        self.prefix.push('.');
        match &it.body {
            TSNamespaceDeclarationBody::TSModuleBlock(block) => self.statements(&block.body),
            TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => self.namespace(inner),
        }
        self.prefix.truncate(outer);
    }

    /// `export default`: a declaration starting at `export`, an anonymous one named
    /// `default`, and a function, class, or object literal expression named `default`.
    pub(super) fn export_default(&mut self, it: &ExportDefaultDeclaration<'_>) {
        let start = it.span.start;
        let default = || Naming {
            name: "default".to_owned(),
            doc: Some(start),
        };
        self.export = Some(start);
        match &it.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                self.function(function, function.id.is_none().then(default), None);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                self.class(class, class.id.is_none().then(default));
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => self.interface(interface),
            other => {
                self.export = None;
                if let Some(expression) = other.as_expression() {
                    if self.at.within == Within::Body {
                        self.named(expression, default());
                    } else {
                        self.visit_expression(expression);
                    }
                }
            }
        }
        self.export = None;
    }

    // --- Module syntax ----------------------------------------------------------------------

    /// A written module dependency at `specifier` (its literal), from the module or function
    /// it is written in.
    fn import(&mut self, specifier: ByteRange, text: &str, kind: ImportKind) {
        let mut from = self.at.parent;
        while !matches!(
            self.nodes[from as usize].kind(),
            NodeKind::Module | NodeKind::Function
        ) {
            from = self.nodes[from as usize].parent.unwrap_or(0);
        }
        self.imports.push(ImportFact {
            from,
            range: specifier,
            specifier: Some(text.into()),
            conditional: false,
        });
        self.import_kinds.push(kind);
    }

    pub(super) fn import_declaration(&mut self, it: &ImportDeclaration<'_>) {
        if imports::import_declaration(it, self.verbatim) {
            self.import(range(it.source.span), &it.source.value, ImportKind::Import);
        }
    }

    pub(super) fn export_from(&mut self, it: &ExportFromDeclaration<'_>) {
        if imports::export_from(it, self.verbatim) {
            self.import(range(it.source.span), &it.source.value, ImportKind::Reexport);
        }
    }

    pub(super) fn export_all(&mut self, it: &ExportAllDeclaration<'_>) {
        if imports::export_all(it) {
            self.import(range(it.source.span), &it.source.value, ImportKind::Reexport);
        }
    }

    pub(super) fn import_equals(&mut self, it: &TSImportEqualsDeclaration<'_>) {
        if imports::import_equals(it)
            && let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference
        {
            let specifier = &reference.expression;
            self.import(range(specifier.span), &specifier.value, ImportKind::ImportEquals);
        }
    }
}

/// Whether a decorator's expression, inside its parentheses, is a call.
pub(crate) fn decorator_is_call(mut expression: &Expression<'_>) -> bool {
    while let Expression::ParenthesizedExpression(it) = expression {
        expression = &it.expression;
    }
    matches!(expression, Expression::CallExpression(_))
}
