//! Type names written in recorded annotations (docs/languages/typescript.md#typed-records): every `TSTypeReference` name of the written types that typed records carry,
//! qualified names whole, recursing into type arguments, unions, intersections, arrays, tuples,
//! function and constructor types, object type literals, indexed accesses, type operators,
//! template literal types, predicates, and conditional and mapped types.
//!
//! Not names: keyword and literal types, `this`, `typeof x`, and `import("m").T` (whose type
//! arguments are still walked). A name that binds to a type parameter is not a link either,
//! decided lexically: the type parameters of the enclosing type syntax (an interface's, a type
//! alias's, a function type's or signature's, a mapped type's key, an `infer` in a conditional
//! type's true branch) are tracked during the walk, and the enclosing callables' and classes'
//! type parameters are symbols of their scopes, so a name whose lookup finds one is dropped when
//! names are looked up ([`Binder::finish`](super::Binder)).
//!
//! Heritage clauses of an interface or class that is a node are walked too: each base is a name
//! used as a base (`extends` for an interface's or a class's `extends`, `implements` for a
//! class's `implements`), and its type arguments are ordinary type names. A class's
//! `extends` is an expression: only a name or a member chain names a type (`mixin(Base)` names
//! none). A class's clauses are walked in its own scope, where its type parameters are.
//!
//! Each name is recorded as a [`TypeRef::Names`] whose first name is looked up in the type
//! meaning from the scope where its type is written; binding lists them in source order
//! (`Binding::type_links`), aligned with the file's type reference facts.

use oxc_ast::ast::{
    Class, Expression, FormalParameters, TSInterfaceHeritage, TSSignature, TSThisParameter, TSTupleElement,
    TSType, TSTypeAnnotation, TSTypeName, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_span::{GetSpan, Span};
use unfold_core::text::ByteRange;
use unfold_engine::facts::TypeUse;

use super::{Binder, TypeSite};
use crate::census::MAX_DEPTH;
use crate::payload::binding::{Lookup, TypeRef};
use crate::spans::range;

impl<'a> Binder<'a> {
    /// Records the type names of `annotation` when its written type is linked (each linked
    /// type once, wherever binding meets it first).
    pub(super) fn link_annotation(&mut self, annotation: Option<&TSTypeAnnotation<'a>>) {
        if let Some(annotation) = annotation {
            self.link_type(&annotation.type_annotation);
        }
    }

    /// Records the type names of `ty` when it is linked.
    fn link_type(&mut self, ty: &TSType<'a>) {
        if self.linked.remove(&range(ty.span())) {
            let outer = self.type_locals.len();
            self.type_names(ty, 0);
            self.type_locals.truncate(outer);
        }
    }

    /// Records the type names of an interface body or of a type alias's aliased type, written
    /// at `body`, when it is linked; the declaration's own type parameters are in scope.
    pub(super) fn link_declaration(
        &mut self,
        body: ByteRange,
        parameters: Option<&TSTypeParameterDeclaration<'a>>,
        walk: impl FnOnce(&mut Self),
    ) {
        if !self.linked.remove(&body) {
            return;
        }
        let outer = self.type_locals.len();
        self.type_locals.extend(
            parameters
                .into_iter()
                .flat_map(|parameters| &parameters.params)
                .map(|parameter| parameter.name.name.as_str()),
        );
        walk(self);
        self.type_locals.truncate(outer);
    }

    /// Records the bases of an interface's `extends` clause and their type arguments' names.
    pub(super) fn interface_heritage(&mut self, bases: &[TSInterfaceHeritage<'a>]) {
        for base in bases {
            self.type_name(&base.type_name, TypeUse::Extends);
            self.type_arguments(base.type_arguments.as_deref(), 0);
        }
    }

    /// Records the bases of a class's `extends` and `implements` clauses and their type
    /// arguments' names.
    pub(super) fn class_heritage(&mut self, class: &Class<'a>) {
        if let Some(heritage) = &class.heritage {
            if let Some(names) = expression_names(&heritage.expression) {
                self.record_name(names, heritage.expression.span(), TypeUse::Extends);
            }
            self.type_arguments(heritage.type_arguments.as_deref(), 0);
        }
        for implemented in &class.implements {
            self.type_name(&implemented.expression, TypeUse::Implements);
            self.type_arguments(implemented.type_arguments.as_deref(), 0);
        }
    }

    /// Records every member type of an interface body or object type literal.
    pub(super) fn signature_names(&mut self, member: &TSSignature<'a>, depth: u32) {
        match member {
            TSSignature::TSPropertySignature(it) => {
                if let Some(annotation) = &it.type_annotation {
                    self.type_names(&annotation.type_annotation, depth + 1);
                }
            }
            TSSignature::TSIndexSignature(it) => {
                self.type_names(&it.parameter.type_annotation.type_annotation, depth + 1);
                self.type_names(&it.type_annotation.type_annotation, depth + 1);
            }
            TSSignature::TSMethodSignature(it) => self.callable_names(
                it.type_parameters.as_deref(),
                it.this_param.as_deref(),
                &it.params,
                it.return_type.as_deref(),
                depth,
            ),
            TSSignature::TSCallSignatureDeclaration(it) => self.callable_names(
                it.type_parameters.as_deref(),
                it.this_param.as_deref(),
                &it.params,
                it.return_type.as_deref(),
                depth,
            ),
            TSSignature::TSConstructSignatureDeclaration(it) => self.callable_names(
                it.type_parameters.as_deref(),
                None,
                &it.params,
                it.return_type.as_deref(),
                depth,
            ),
        }
    }

    /// The names of a written signature: its type parameters' constraints and defaults, its
    /// parameter types, and its return type, with its type parameters in scope.
    fn callable_names(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'a>>,
        this: Option<&TSThisParameter<'a>>,
        parameters: &FormalParameters<'a>,
        returns: Option<&TSTypeAnnotation<'a>>,
        depth: u32,
    ) {
        let outer = self.type_locals.len();
        let depth = depth + 1;
        if let Some(type_parameters) = type_parameters {
            self.type_locals.extend(
                type_parameters
                    .params
                    .iter()
                    .map(|parameter| parameter.name.name.as_str()),
            );
            for parameter in &type_parameters.params {
                for ty in parameter.constraint.iter().chain(&parameter.default) {
                    self.type_names(ty, depth);
                }
            }
        }
        let annotations = this
            .and_then(|this| this.type_annotation.as_deref())
            .into_iter()
            .chain(
                parameters
                    .items
                    .iter()
                    .filter_map(|parameter| parameter.type_annotation.as_deref()),
            )
            .chain(
                parameters
                    .rest
                    .as_ref()
                    .and_then(|rest| rest.type_annotation.as_deref()),
            )
            .chain(returns);
        for annotation in annotations {
            self.type_names(&annotation.type_annotation, depth);
        }
        self.type_locals.truncate(outer);
    }

    fn type_arguments(&mut self, arguments: Option<&TSTypeParameterInstantiation<'a>>, depth: u32) {
        for argument in arguments.into_iter().flat_map(|arguments| &arguments.params) {
            self.type_names(argument, depth + 1);
        }
    }

    /// Records every type name written in `ty`. Types nested deeper than the converter's
    /// depth limit record nothing more.
    pub(super) fn type_names(&mut self, ty: &TSType<'a>, depth: u32) {
        if depth > MAX_DEPTH {
            return;
        }
        let next = depth + 1;
        match ty {
            TSType::TSTypeReference(it) => {
                self.type_name(&it.type_name, TypeUse::Annotation);
                self.type_arguments(it.type_arguments.as_deref(), depth);
            }
            TSType::TSArrayType(it) => self.type_names(&it.element_type, next),
            TSType::TSUnionType(it) => {
                for member in &it.types {
                    self.type_names(member, next);
                }
            }
            TSType::TSIntersectionType(it) => {
                for member in &it.types {
                    self.type_names(member, next);
                }
            }
            TSType::TSParenthesizedType(it) => self.type_names(&it.type_annotation, next),
            TSType::TSTypeOperatorType(it) => self.type_names(&it.type_annotation, next),
            TSType::TSIndexedAccessType(it) => {
                self.type_names(&it.object_type, next);
                self.type_names(&it.index_type, next);
            }
            TSType::TSTupleType(it) => {
                for element in &it.element_types {
                    self.tuple_element(element, next);
                }
            }
            TSType::TSNamedTupleMember(it) => self.tuple_element(&it.element_type, next),
            TSType::TSFunctionType(it) => self.callable_names(
                it.type_parameters.as_deref(),
                it.this_param.as_deref(),
                &it.params,
                Some(&it.return_type),
                depth,
            ),
            TSType::TSConstructorType(it) => self.callable_names(
                it.type_parameters.as_deref(),
                None,
                &it.params,
                Some(&it.return_type),
                depth,
            ),
            TSType::TSTypeLiteral(it) => {
                for member in &it.members {
                    self.signature_names(member, next);
                }
            }
            TSType::TSConditionalType(it) => {
                self.type_names(&it.check_type, next);
                // The `infer` declarations of the extends clause are in scope in the true branch.
                let outer = self.type_locals.len();
                self.type_names(&it.extends_type, next);
                self.type_names(&it.true_type, next);
                self.type_locals.truncate(outer);
                self.type_names(&it.false_type, next);
            }
            TSType::TSInferType(it) => {
                if let Some(constraint) = &it.type_parameter.constraint {
                    self.type_names(constraint, next);
                }
                // Declared for the enclosing conditional type's true branch.
                self.type_locals.push(it.type_parameter.name.name.as_str());
            }
            TSType::TSMappedType(it) => {
                self.type_names(&it.constraint, next);
                let outer = self.type_locals.len();
                self.type_locals.push(it.key.name.as_str());
                for inner in it.name_type.iter().chain(&it.type_annotation) {
                    self.type_names(inner, next);
                }
                self.type_locals.truncate(outer);
            }
            TSType::TSTypePredicate(it) => {
                if let Some(annotation) = &it.type_annotation {
                    self.type_names(&annotation.type_annotation, next);
                }
            }
            TSType::TSTemplateLiteralType(it) => {
                for member in &it.types {
                    self.type_names(member, next);
                }
            }
            TSType::TSTypeQuery(it) => self.type_arguments(it.type_arguments.as_deref(), depth),
            TSType::TSImportType(it) => self.type_arguments(it.type_arguments.as_deref(), depth),
            TSType::JSDocNullableType(it) => self.type_names(&it.type_annotation, next),
            TSType::JSDocNonNullableType(it) => self.type_names(&it.type_annotation, next),
            TSType::TSAnyKeyword(_)
            | TSType::TSBigIntKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSIntrinsicKeyword(_)
            | TSType::TSNeverKeyword(_)
            | TSType::TSNullKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSObjectKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSSymbolKeyword(_)
            | TSType::TSUndefinedKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSVoidKeyword(_)
            | TSType::TSThisType(_)
            | TSType::TSLiteralType(_)
            | TSType::JSDocUnknownType(_) => {}
        }
    }

    fn tuple_element(&mut self, element: &TSTupleElement<'a>, depth: u32) {
        match element {
            TSTupleElement::TSOptionalType(it) => self.type_names(&it.type_annotation, depth),
            TSTupleElement::TSRestType(it) => self.type_names(&it.type_annotation, depth),
            _ => {
                if let Some(ty) = element.as_ts_type() {
                    self.type_names(ty, depth);
                }
            }
        }
    }

    /// Records one written type name, used as `usage`, unless its root is `this`.
    fn type_name(&mut self, name: &TSTypeName<'a>, usage: TypeUse) {
        let mut names: Vec<&'a str> = Vec::new();
        let mut current = name;
        loop {
            match current {
                TSTypeName::IdentifierReference(id) => break names.push(id.name.as_str()),
                TSTypeName::QualifiedName(qualified) => {
                    names.push(qualified.right.name.as_str());
                    current = &qualified.left;
                }
                TSTypeName::ThisExpression(_) => return,
            }
        }
        names.reverse();
        self.record_name(names, name.span(), usage);
    }

    /// Records the type name `names` (first to last) written at `span`, unless its first name
    /// is a type parameter of the enclosing type syntax.
    fn record_name(&mut self, names: Vec<&'a str>, span: Span, usage: TypeUse) {
        if names.first().is_none_or(|first| self.type_locals.contains(first)) {
            return;
        }
        let qualified = names.len() > 1;
        self.types.push(TypeRef::Names {
            names: names.into_iter().map(Box::from).collect(),
            value: false,
            lookup: Lookup::Unresolved,
            shadowed: false,
        });
        let index = (self.types.len() - 1) as u32;
        self.type_places.push((index, self.scope));
        self.type_sites.push(TypeSite {
            range: range(span),
            qualified,
            usage,
            ty: index,
        });
    }
}

/// The names of a class's `extends` expression when it is a name or a chain of static members
/// on a name (`Base`, `models.Base`), first to last; `None` for any other expression.
fn expression_names<'a>(expression: &Expression<'a>) -> Option<Vec<&'a str>> {
    let mut names = Vec::new();
    let mut current = expression;
    loop {
        match current {
            Expression::Identifier(id) => {
                names.push(id.name.as_str());
                break;
            }
            Expression::StaticMemberExpression(member) if !member.optional => {
                names.push(member.property.name.as_str());
                current = &member.object;
            }
            _ => return None,
        }
    }
    names.reverse();
    Some(names)
}
