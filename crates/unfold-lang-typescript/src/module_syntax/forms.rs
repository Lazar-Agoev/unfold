//! Reading the syntactic forms module facts need: export names, transparent wrappers, access
//! paths, entity names, and whether a namespace has a runtime value.

use oxc_ast::ast::{
    Declaration, Expression, ImportOrExportKind, ModuleExportName, Statement, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody, TSTypeName,
};

use super::MAX_NAMESPACE_DEPTH;

/// The name a module export name spells.
pub(super) fn export_name<'a>(name: &'a ModuleExportName<'_>) -> &'a str {
    match name {
        ModuleExportName::IdentifierName(it) => it.name.as_str(),
        ModuleExportName::IdentifierReference(it) => it.name.as_str(),
        ModuleExportName::StringLiteral(it) => it.value.as_str(),
    }
}

/// The expression inside parentheses, `as`, `satisfies`, `!`, and `<T>` assertions.
pub(super) fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(it) => &it.expression,
            Expression::TSAsExpression(it) => &it.expression,
            Expression::TSSatisfiesExpression(it) => &it.expression,
            Expression::TSNonNullExpression(it) => &it.expression,
            Expression::TSTypeAssertion(it) => &it.expression,
            _ => return expression,
        };
    }
}

/// `a`, `a.b.c`, or `a["b"]` as its root name and member names; `None` for anything else.
pub(super) fn access_path(expression: &Expression<'_>) -> Option<Vec<Box<str>>> {
    let mut members = Vec::new();
    let mut current = expression;
    loop {
        current = unwrap(current);
        match current {
            Expression::Identifier(it) => {
                members.push(it.name.as_str().into());
                members.reverse();
                return Some(members);
            }
            Expression::StaticMemberExpression(it) => {
                members.push(it.property.name.as_str().into());
                current = &it.object;
            }
            Expression::ComputedMemberExpression(it) => match unwrap(&it.expression) {
                Expression::StringLiteral(key) => {
                    members.push(key.value.as_str().into());
                    current = &it.object;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
}

/// The names of an entity name (`A.B.C`) whose last name is `right`, leftmost first.
pub(super) fn qualified(left: &TSTypeName<'_>, right: &str) -> Vec<Box<str>> {
    let mut parts: Vec<Box<str>> = vec![right.into()];
    let mut current = left;
    loop {
        match current {
            TSTypeName::IdentifierReference(it) => {
                parts.push(it.name.as_str().into());
                break;
            }
            TSTypeName::QualifiedName(it) => {
                parts.push(it.right.name.as_str().into());
                current = &it.left;
            }
            TSTypeName::ThisExpression(_) => {
                parts.push("this".into());
                break;
            }
        }
    }
    parts.reverse();
    parts
}

/// Whether a namespace has a runtime value: it (or a nested namespace) contains a declaration
/// other than an interface, type alias, `const enum`, type-only import or export, or a
/// namespace without one.
pub(super) fn namespace_instantiated(namespace: &TSNamespaceDeclaration<'_>, depth: u32) -> bool {
    if depth > MAX_NAMESPACE_DEPTH {
        return true;
    }
    match &namespace.body {
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => namespace_instantiated(inner, depth + 1),
        TSNamespaceDeclarationBody::TSModuleBlock(block) => block.body.iter().any(|statement| {
            let declaration = match statement {
                Statement::ExportDeclaration(it) => &it.declaration,
                Statement::ExportNamedDeclaration(it) => {
                    return it.export_kind == ImportOrExportKind::Value && !it.specifiers.is_empty();
                }
                Statement::TSExportAssignment(_) | Statement::ExportDefaultDeclaration(_) => return true,
                Statement::ImportDeclaration(_)
                | Statement::ExportFromDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                | Statement::TSNamespaceExportDeclaration(_)
                | Statement::EmptyStatement(_) => return false,
                other => match other.as_declaration() {
                    Some(declaration) => declaration,
                    None => return true,
                },
            };
            match declaration {
                Declaration::TSTypeAliasDeclaration(_)
                | Declaration::TSInterfaceDeclaration(_)
                | Declaration::TSGlobalDeclaration(_)
                | Declaration::TSExternalModuleDeclaration(_) => false,
                Declaration::TSEnumDeclaration(it) => !it.r#const,
                Declaration::TSNamespaceDeclaration(inner) => namespace_instantiated(inner, depth + 1),
                Declaration::TSImportEqualsDeclaration(it) => it.import_kind == ImportOrExportKind::Value,
                Declaration::VariableDeclaration(_)
                | Declaration::FunctionDeclaration(_)
                | Declaration::ClassDeclaration(_) => true,
            }
        }),
    }
}
