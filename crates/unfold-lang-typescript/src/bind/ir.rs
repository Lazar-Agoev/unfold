//! Converting oxc expressions and types into the payload's compact forms ([`Expr`],
//! [`TypeRef`]) during the binding walk, and dropping the expressions nothing needs.
//!
//! Expressions are converted after their children (the walk converts on the way back up), so a
//! child always has a smaller index than its parent. They keep the wrappers evidence tells apart
//! (parentheses, `!`, `satisfies`, `as`, instantiations) and resolve what is lexical: `this` and
//! `super` to their context, private names to their class's members, inline functions and
//! classes to their declarations. Identifiers and type names are looked up after the walk.

use oxc_allocator::{Address, GetAddress};
use oxc_ast::ast::{
    ChainElement, ComputedMemberExpression, Expression, MemberExpression, TSType, TSTypeName,
    TSTypeQueryExprName,
};

use super::{Binder, KeyDraft, PathDraft, RootDraft};
use crate::number::js_number;
use crate::payload::binding::{BuiltinType, ClassFacts, Decl, Expr, Lookup, MemberWrite, TypeRef};

impl<'a> Binder<'a> {
    fn push_expr(&mut self, expr: Expr) -> u32 {
        self.exprs.push(expr);
        (self.exprs.len() - 1) as u32
    }

    /// The converted index of an expression the walk has already visited; a placeholder when
    /// it hasn't (an expression inside syntax binding skips).
    pub(super) fn converted(&mut self, expression: &Expression<'a>) -> u32 {
        match self.memo.get(&expression.address()) {
            Some(&index) => index,
            None => self.push_expr(Expr::Other),
        }
    }

    /// Converts `expression`, whose children the walk has just visited.
    pub(super) fn convert(&mut self, expression: &Expression<'a>) {
        let converted = match expression {
            Expression::Identifier(id) => {
                let index = self.push_expr(Expr::Name {
                    name: id.name.as_str().into(),
                    lookup: Lookup::Unresolved,
                });
                self.name_places.push((index, self.place()));
                self.memo.insert(expression.address(), index);
                return;
            }
            Expression::ThisExpression(it) => self
                .context_at
                .get(&it.span.start)
                .map_or(Expr::Other, |&context| Expr::This(context)),
            Expression::Super(it) => self
                .context_at
                .get(&it.span.start)
                .map_or(Expr::Other, |&context| Expr::Super(context)),
            Expression::CallExpression(call) => Expr::Call {
                callee: Some(self.converted(&call.callee)),
            },
            Expression::ImportExpression(_) => Expr::Call { callee: None },
            Expression::NewExpression(it) => Expr::New {
                callee: Some(self.converted(&it.callee)),
            },
            Expression::TaggedTemplateExpression(it) => Expr::Tagged {
                tag: self.converted(&it.tag),
            },
            Expression::ParenthesizedExpression(it) => Expr::Paren(self.converted(&it.expression)),
            Expression::TSNonNullExpression(it) => Expr::NonNull(self.converted(&it.expression)),
            Expression::TSSatisfiesExpression(it) => Expr::Satisfies(self.converted(&it.expression)),
            Expression::AwaitExpression(it) => Expr::Await(self.converted(&it.argument)),
            Expression::TSAsExpression(it) => {
                let inner = self.converted(&it.expression);
                let ty = Some(self.ty(&it.type_annotation, self.scope));
                Expr::Assert { inner, ty }
            }
            Expression::TSTypeAssertion(it) => {
                let inner = self.converted(&it.expression);
                let ty = Some(self.ty(&it.type_annotation, self.scope));
                Expr::Assert { inner, ty }
            }
            Expression::TSInstantiationExpression(it) => Expr::Instantiation(self.converted(&it.expression)),
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => self
                .inline_decls
                .get(&expression.address())
                .map_or(Expr::Other, |&decl| Expr::Function(decl)),
            Expression::ClassExpression(_) => self
                .inline_decls
                .get(&expression.address())
                .map_or(Expr::Other, |&decl| Expr::Class(decl)),
            Expression::NullLiteral(_) => Expr::Null,
            Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => {
                Expr::Literal(BuiltinType::String)
            }
            Expression::NumericLiteral(_) => Expr::Literal(BuiltinType::Number),
            Expression::BigIntLiteral(_) => Expr::Literal(BuiltinType::BigInt),
            Expression::BooleanLiteral(_) => Expr::Literal(BuiltinType::Boolean),
            Expression::RegExpLiteral(_) => Expr::Literal(BuiltinType::RegExp),
            Expression::ArrayExpression(_) => Expr::Literal(BuiltinType::Array),
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => Expr::Call {
                    callee: Some(self.converted(&call.callee)),
                },
                ChainElement::TSNonNullExpression(it) => Expr::NonNull(self.converted(&it.expression)),
                _ => match chain.expression.as_member_expression() {
                    Some(member) => self.member(member),
                    None => Expr::Other,
                },
            },
            _ => match expression.as_member_expression() {
                Some(member) => self.member(member),
                None => Expr::Other,
            },
        };
        let index = self.push_expr(converted);
        self.memo.insert(expression.address(), index);
    }

    /// A member access whose object the walk has visited.
    pub(super) fn member(&mut self, member: &MemberExpression<'a>) -> Expr {
        match member {
            MemberExpression::StaticMemberExpression(it) => Expr::Member {
                object: self.converted(&it.object),
                name: it.property.name.as_str().into(),
                private: None,
            },
            MemberExpression::PrivateFieldExpression(it) => {
                let name = it.field.name.as_str();
                let private = self
                    .class_stack
                    .iter()
                    .rev()
                    .find_map(|&class| self.classes[class as usize].private.get(name))
                    .map(|members| members.to_vec())
                    .unwrap_or_default();
                Expr::Member {
                    object: self.converted(&it.object),
                    name: format!("#{name}").into(),
                    private: Some(private),
                }
            }
            MemberExpression::ComputedMemberExpression(it) => {
                let object = self.converted(&it.object);
                element(it, object)
            }
        }
    }

    /// Converts the written type `ty`; its names are looked up from `scope` after the walk.
    pub(super) fn ty(&mut self, mut ty: &TSType<'a>, scope: u32) -> u32 {
        while let TSType::TSParenthesizedType(inner) = ty {
            ty = &inner.type_annotation;
        }
        let converted = match ty {
            TSType::TSArrayType(_) => TypeRef::Array,
            TSType::TSStringKeyword(_) => TypeRef::Primitive(BuiltinType::String),
            TSType::TSNumberKeyword(_) => TypeRef::Primitive(BuiltinType::Number),
            TSType::TSBooleanKeyword(_) => TypeRef::Primitive(BuiltinType::Boolean),
            TSType::TSBigIntKeyword(_) => TypeRef::Primitive(BuiltinType::BigInt),
            TSType::TSSymbolKeyword(_) => TypeRef::Primitive(BuiltinType::Symbol),
            TSType::TSObjectKeyword(_) => TypeRef::Primitive(BuiltinType::Object),
            TSType::TSTypeReference(reference) => names(&reference.type_name, false),
            TSType::TSTypeQuery(query) => match &query.expr_name {
                TSTypeQueryExprName::IdentifierReference(id) => TypeRef::Names {
                    names: vec![id.name.as_str().into()],
                    value: true,
                    lookup: Lookup::Unresolved,
                    shadowed: false,
                },
                TSTypeQueryExprName::QualifiedName(_) => query
                    .expr_name
                    .as_ts_type_name()
                    .map_or(TypeRef::Other, |name| names(name, true)),
                _ => TypeRef::Other,
            },
            TSType::TSTypePredicate(predicate) if predicate.type_annotation.is_some() => TypeRef::Predicate,
            _ => TypeRef::Other,
        };
        let names = matches!(converted, TypeRef::Names { .. });
        self.types.push(converted);
        let index = (self.types.len() - 1) as u32;
        if names {
            self.type_places.push((index, scope));
        }
        index
    }

    /// A receiver's narrowing key: an identifier or a bound `this`, then property names and
    /// direct string or numeric keys, through any wrapper.
    pub(super) fn key_draft(&self, mut expression: &Expression<'a>) -> Option<KeyDraft<'a>> {
        let mut path: Vec<Box<str>> = Vec::new();
        let root = loop {
            match expression {
                Expression::Identifier(id) => break RootDraft::Name(id.name.as_str(), self.place()),
                Expression::ThisExpression(it) => {
                    let context = *self.context_at.get(&it.span.start)?;
                    let (class, _) = self.contexts.get(context as usize)?.receiver?;
                    break RootDraft::This(class);
                }
                Expression::ParenthesizedExpression(it) => expression = &it.expression,
                Expression::TSNonNullExpression(it) => expression = &it.expression,
                Expression::TSSatisfiesExpression(it) => expression = &it.expression,
                Expression::TSAsExpression(it) => expression = &it.expression,
                Expression::TSTypeAssertion(it) => expression = &it.expression,
                Expression::ChainExpression(chain) => {
                    let (object, name) = key_step(chain.expression.as_member_expression()?)?;
                    path.push(name);
                    expression = object;
                }
                _ => {
                    let (object, name) = key_step(expression.as_member_expression()?)?;
                    path.push(name);
                    expression = object;
                }
            }
        };
        path.reverse();
        Some(KeyDraft { root, path })
    }

    /// A callee written as an identifier followed by property accesses.
    pub(super) fn path_draft(&self, mut expression: &Expression<'a>) -> Option<PathDraft<'a>> {
        let mut rest: Vec<Box<str>> = Vec::new();
        loop {
            match expression {
                Expression::Identifier(id) => {
                    rest.reverse();
                    return Some(PathDraft {
                        first: id.name.as_str(),
                        place: self.place(),
                        rest,
                    });
                }
                Expression::ParenthesizedExpression(it) => expression = &it.expression,
                Expression::TSNonNullExpression(it) => expression = &it.expression,
                Expression::TSSatisfiesExpression(it) => expression = &it.expression,
                Expression::TSAsExpression(it) => expression = &it.expression,
                Expression::TSTypeAssertion(it) => expression = &it.expression,
                Expression::StaticMemberExpression(it) => {
                    rest.push(it.property.name.as_str().into());
                    expression = &it.object;
                }
                _ => return None,
            }
        }
    }
}

/// A type reference's names, leftmost first; looked up after the walk.
fn names(name: &TSTypeName<'_>, value: bool) -> TypeRef {
    let mut parts: Vec<Box<str>> = Vec::new();
    let mut current = name;
    loop {
        match current {
            TSTypeName::IdentifierReference(id) => {
                parts.push(id.name.as_str().into());
                break;
            }
            TSTypeName::QualifiedName(qualified) => {
                parts.push(qualified.right.name.as_str().into());
                current = &qualified.left;
            }
            TSTypeName::ThisExpression(_) => return TypeRef::Other,
        }
    }
    parts.reverse();
    TypeRef::Names {
        names: parts,
        value,
        lookup: Lookup::Unresolved,
        shadowed: false,
    }
}

/// One step of a narrowing key: the object and the property name or direct literal key.
fn key_step<'e, 'a>(member: &'e MemberExpression<'a>) -> Option<(&'e Expression<'a>, Box<str>)> {
    match member {
        MemberExpression::StaticMemberExpression(it) => Some((&it.object, it.property.name.as_str().into())),
        MemberExpression::PrivateFieldExpression(it) => {
            Some((&it.object, format!("#{}", it.field.name).into()))
        }
        MemberExpression::ComputedMemberExpression(it) => match &it.expression {
            Expression::StringLiteral(literal) => Some((&it.object, literal.value.as_str().into())),
            Expression::NumericLiteral(literal) => Some((&it.object, js_number(literal.value).into())),
            _ => None,
        },
    }
}

/// Removes parentheses, `!`, `as`, `satisfies`, and `<T>` assertions.
pub(super) fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            _ => return expression,
        };
    }
}

/// The address of the callee of an initializer that is a call or `new` (after transparent
/// wrappers), which provenance traces.
pub(super) fn factory_callee(init: &Expression<'_>) -> Option<Address> {
    match unwrap(init) {
        Expression::CallExpression(call) => Some(call.callee.address()),
        Expression::NewExpression(it) => Some(it.callee.address()),
        _ => None,
    }
}

/// An element access: its literal key, whether the key is written directly, and whether it
/// reads an array element.
fn element(it: &ComputedMemberExpression<'_>, object: u32) -> Expr {
    let direct = matches!(
        it.expression,
        Expression::StringLiteral(_) | Expression::NumericLiteral(_)
    );
    let (key, index) = match unwrap(&it.expression) {
        Expression::StringLiteral(literal) => {
            let key = literal.value.as_str();
            (Some(Box::from(key)), is_index(key))
        }
        Expression::NumericLiteral(literal) => (Some(js_number(literal.value).into()), true),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            let quasi = template.quasis.first();
            let cooked = quasi.and_then(|quasi| quasi.value.cooked.as_ref());
            let raw = quasi.map_or("", |quasi| quasi.value.raw.as_str());
            // An escape may decode to an index, so an escaped template key reads an element.
            let index = raw.contains('\\') || is_index(raw);
            (cooked.map(|cooked| Box::from(cooked.as_str())), index)
        }
        _ => (None, true),
    };
    Expr::Element {
        object,
        key,
        direct,
        index,
    }
}

/// Whether a key spells an array index (`0`, or digits without a leading zero).
fn is_index(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit()) && (key == "0" || !key.starts_with('0'))
}

/// The children of an expression.
fn children(expr: &Expr) -> impl Iterator<Item = u32> {
    let (first, second) = match *expr {
        Expr::Member { object, .. } | Expr::Element { object, .. } => (Some(object), None),
        Expr::Call { callee } | Expr::New { callee } => (callee, None),
        Expr::Tagged { tag } => (Some(tag), None),
        Expr::Paren(inner)
        | Expr::NonNull(inner)
        | Expr::Satisfies(inner)
        | Expr::Await(inner)
        | Expr::Instantiation(inner) => (Some(inner), None),
        Expr::Assert { inner, .. } => (Some(inner), None),
        _ => (None, None),
    };
    first.into_iter().chain(second)
}

/// Rewrites an expression's children through `map`.
fn remap(expr: &mut Expr, map: &[u32]) {
    let fix = |index: &mut u32| *index = map[*index as usize];
    match expr {
        Expr::Member { object, .. } | Expr::Element { object, .. } => fix(object),
        Expr::Call { callee } | Expr::New { callee } => {
            if let Some(callee) = callee {
                fix(callee);
            }
        }
        Expr::Tagged { tag } => fix(tag),
        Expr::Paren(inner)
        | Expr::NonNull(inner)
        | Expr::Satisfies(inner)
        | Expr::Await(inner)
        | Expr::Instantiation(inner)
        | Expr::Assert { inner, .. } => fix(inner),
        _ => {}
    }
}

/// Keeps only the expressions a root reaches (values, initializers, class bases, member
/// writes), in their order, and rewrites every index to the kept positions.
pub(super) fn prune(
    exprs: Vec<Expr>,
    values: &mut [Option<u32>],
    declarations: &mut [Decl],
    classes: &mut [ClassFacts],
    writes: &mut [MemberWrite],
) -> Vec<Expr> {
    let mut keep = vec![false; exprs.len()];
    let mut pending: Vec<u32> = values.iter().flatten().copied().collect();
    pending.extend(declarations.iter().filter_map(|decl| decl.initializer));
    pending.extend(classes.iter().filter_map(|class| class.heritage));
    pending.extend(writes.iter().map(|write| write.object));
    while let Some(index) = pending.pop() {
        let Some(seen) = keep.get_mut(index as usize) else {
            continue;
        };
        if *seen {
            continue;
        }
        *seen = true;
        pending.extend(children(&exprs[index as usize]));
    }
    let mut map = vec![u32::MAX; exprs.len()];
    let mut kept = Vec::with_capacity(keep.iter().filter(|&&kept| kept).count());
    for (index, mut expr) in exprs.into_iter().enumerate() {
        if !keep[index] {
            continue;
        }
        remap(&mut expr, &map);
        map[index] = kept.len() as u32;
        kept.push(expr);
    }
    for value in values.iter_mut().flatten() {
        *value = map[*value as usize];
    }
    for decl in declarations {
        if let Some(initializer) = &mut decl.initializer {
            *initializer = map[*initializer as usize];
        }
    }
    for class in classes {
        if let Some(heritage) = &mut class.heritage {
            *heritage = map[*heritage as usize];
        }
    }
    for write in writes {
        write.object = map[write.object as usize];
    }
    kept
}
