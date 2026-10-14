//! Module-level facts for the payload ([`ModuleFacts`]): what a file declares at module scope,
//! its import bindings, and its export entries, read from oxc's tree.
//!
//! Module scope holds the top-level declarations, plus `var` declarations hoisted out of
//! top-level blocks, loops, conditionals, and `try` statements (never out of functions or
//! classes). Function declarations nested in blocks are block-scoped in modules, so they are not
//! module-scope declarations. `declare global` and `declare module "…"` bodies are not module
//! scope either.

use oxc_ast::ast::{
    BindingPattern, Class, Declaration, ExportDefaultDeclarationKind, Expression, ForStatementInit,
    ForStatementLeft, Function, ImportDeclarationSpecifier, ImportOrExportKind, Program, Statement,
    TSModuleReference, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_span::{GetSpan, Span};

use crate::payload::{
    Declaration as Decl, DeclarationKind, ExportEntry, ImportBinding, ImportSource, ModuleFacts,
};
use crate::spans::range;
use forms::{access_path, export_name, namespace_instantiated, qualified, unwrap};

/// How deep namespace bodies are searched for runtime declarations before one is assumed.
const MAX_NAMESPACE_DEPTH: u32 = 64;

/// Collects the module facts of a parsed program.
pub(crate) fn collect(program: &Program<'_>, module_syntax: bool, jsx: bool) -> ModuleFacts {
    let mut collector = Collector {
        facts: ModuleFacts {
            module_syntax,
            jsx,
            ..ModuleFacts::default()
        },
    };
    collector.facts.declarations.push(Decl {
        name: "".into(),
        kind: DeclarationKind::Module,
        range: range(program.span),
        exported: false,
        default: false,
        ambient: false,
    });
    for statement in &program.body {
        collector.statement(statement);
    }
    collector.facts
}

/// Whether a parsed `.tsx` program contains JSX.
pub(crate) fn uses_jsx(program: &Program<'_>) -> bool {
    struct Finder(bool);
    impl<'a> Visit<'a> for Finder {
        fn visit_jsx_element(&mut self, _it: &oxc_ast::ast::JSXElement<'a>) {
            self.0 = true;
        }
        fn visit_jsx_fragment(&mut self, _it: &oxc_ast::ast::JSXFragment<'a>) {
            self.0 = true;
        }
    }
    let mut finder = Finder(false);
    finder.visit_program(program);
    finder.0
}

struct Collector {
    facts: ModuleFacts,
}

/// How a declaration is exported.
#[derive(Clone, Copy, Default)]
struct Exported {
    named: bool,
    default: bool,
}

impl Collector {
    fn declare(&mut self, declaration: Decl) -> u32 {
        let index = self.facts.declarations.len() as u32;
        self.facts.declarations.push(declaration);
        index
    }

    fn statement(&mut self, statement: &Statement<'_>) {
        match statement {
            Statement::ImportDeclaration(it) => {
                let Some(specifiers) = &it.specifiers else {
                    return;
                };
                let all_types = it.import_kind == ImportOrExportKind::Type;
                for specifier in specifiers {
                    let (local, imported, type_only) = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => (
                            &named.local,
                            export_name(&named.imported),
                            named.import_kind == ImportOrExportKind::Type,
                        ),
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                            (&default.local, "default", false)
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                            (&namespace.local, "*", false)
                        }
                    };
                    self.facts.imports.push(ImportBinding {
                        local: local.name.as_str().into(),
                        range: range(local.span),
                        imported: imported.into(),
                        source: ImportSource::Module {
                            specifier: it.source.value.as_str().into(),
                            require: false,
                        },
                        type_only: all_types || type_only,
                        exported: false,
                    });
                }
            }
            Statement::ExportNamedDeclaration(it) => {
                let all_types = it.export_kind == ImportOrExportKind::Type;
                for specifier in &it.specifiers {
                    self.facts.exports.push(ExportEntry::Local {
                        exported: export_name(&specifier.exported).into(),
                        local: export_name(&specifier.local).into(),
                        type_only: all_types || specifier.export_kind == ImportOrExportKind::Type,
                    });
                }
            }
            Statement::ExportFromDeclaration(it) => {
                let all_types = it.export_kind == ImportOrExportKind::Type;
                for specifier in &it.specifiers {
                    self.facts.exports.push(ExportEntry::Reexport {
                        exported: export_name(&specifier.exported).into(),
                        imported: export_name(&specifier.local).into(),
                        specifier: it.source.value.as_str().into(),
                        type_only: all_types || specifier.export_kind == ImportOrExportKind::Type,
                    });
                }
            }
            Statement::ExportAllDeclaration(it) => {
                let type_only = it.export_kind == ImportOrExportKind::Type;
                let specifier: Box<str> = it.source.value.as_str().into();
                self.facts.exports.push(match &it.exported {
                    Some(name) => ExportEntry::Reexport {
                        exported: export_name(name).into(),
                        imported: "*".into(),
                        specifier,
                        type_only,
                    },
                    None => ExportEntry::Star { specifier, type_only },
                });
            }
            Statement::ExportDeclaration(it) => {
                let exported = Exported {
                    named: true,
                    default: false,
                };
                self.declaration(&it.declaration, exported);
            }
            Statement::ExportDefaultDeclaration(it) => {
                let exported = Exported {
                    named: false,
                    default: true,
                };
                match &it.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        self.function(function, exported);
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => self.class(class, exported),
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        self.declare(plain(
                            &interface.id.name,
                            DeclarationKind::Interface,
                            interface.span,
                            exported,
                        ));
                    }
                    other => {
                        if let Some(expression) = other.as_expression() {
                            self.value_export("default", expression);
                        }
                    }
                }
            }
            Statement::TSExportAssignment(it) => self.value_export("export=", &it.expression),
            Statement::TSNamespaceExportDeclaration(_) => {}
            _ => {
                if let Some(declaration) = statement.as_declaration() {
                    self.declaration(declaration, Exported::default());
                } else {
                    self.hoist_vars(statement);
                }
            }
        }
    }

    fn declaration(&mut self, declaration: &Declaration<'_>, exported: Exported) {
        match declaration {
            Declaration::VariableDeclaration(list) => self.variables(list, exported, false),
            Declaration::FunctionDeclaration(function) => self.function(function, exported),
            Declaration::ClassDeclaration(class) => self.class(class, exported),
            Declaration::TSTypeAliasDeclaration(it) => {
                self.declare(plain(&it.id.name, DeclarationKind::TypeAlias, it.span, exported));
            }
            Declaration::TSInterfaceDeclaration(it) => {
                self.declare(plain(&it.id.name, DeclarationKind::Interface, it.span, exported));
            }
            Declaration::TSEnumDeclaration(it) => {
                let mut declaration = plain(&it.id.name, DeclarationKind::Enum, it.span, exported);
                declaration.ambient = it.declare;
                self.declare(declaration);
            }
            Declaration::TSNamespaceDeclaration(it) => {
                let instantiated = namespace_instantiated(it, 0);
                let mut declaration = plain(
                    &it.id.name,
                    DeclarationKind::Namespace { instantiated },
                    it.span,
                    exported,
                );
                declaration.ambient = it.declare;
                self.declare(declaration);
            }
            Declaration::TSImportEqualsDeclaration(it) => {
                let (source, imported) = match &it.module_reference {
                    TSModuleReference::ExternalModuleReference(reference) => (
                        ImportSource::Module {
                            specifier: reference.expression.value.as_str().into(),
                            require: true,
                        },
                        "export=",
                    ),
                    TSModuleReference::IdentifierReference(name) => {
                        (ImportSource::Qualified(vec![name.name.as_str().into()]), "")
                    }
                    TSModuleReference::QualifiedName(name) => (
                        ImportSource::Qualified(qualified(&name.left, name.right.name.as_str())),
                        "",
                    ),
                };
                self.facts.imports.push(ImportBinding {
                    local: it.id.name.as_str().into(),
                    range: range(it.id.span),
                    imported: imported.into(),
                    source,
                    type_only: it.import_kind == ImportOrExportKind::Type,
                    exported: exported.named,
                });
            }
            // Global augmentations and ambient module declarations are not module scope.
            Declaration::TSGlobalDeclaration(_) | Declaration::TSExternalModuleDeclaration(_) => {}
        }
    }

    fn function(&mut self, function: &Function<'_>, exported: Exported) {
        self.declare(Decl {
            name: function.id.as_ref().map_or("", |id| id.name.as_str()).into(),
            kind: DeclarationKind::Function,
            range: range(function.span),
            exported: exported.named,
            default: exported.default,
            ambient: function.declare,
        });
    }

    fn class(&mut self, class: &Class<'_>, exported: Exported) {
        self.declare(Decl {
            name: class.id.as_ref().map_or("", |id| id.name.as_str()).into(),
            kind: DeclarationKind::Class,
            range: range(class.span),
            exported: exported.named,
            default: exported.default,
            ambient: class.declare,
        });
    }

    /// Every binding of a variable list; with `only_var`, nothing unless it is a `var` list.
    fn variables(&mut self, list: &VariableDeclaration<'_>, exported: Exported, only_var: bool) {
        let kind = match list.kind {
            VariableDeclarationKind::Var => DeclarationKind::Var,
            _ if only_var => return,
            VariableDeclarationKind::Let => DeclarationKind::Let,
            VariableDeclarationKind::Const => DeclarationKind::Const,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => DeclarationKind::Using,
        };
        for declarator in &list.declarations {
            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                self.declare(Decl {
                    name: id.name.as_str().into(),
                    kind,
                    range: range(declarator.span),
                    exported: exported.named,
                    default: false,
                    ambient: list.declare,
                });
                continue;
            }
            for id in declarator.id.get_binding_identifiers() {
                self.declare(Decl {
                    name: id.name.as_str().into(),
                    kind,
                    range: range(id.span),
                    exported: exported.named,
                    default: false,
                    ambient: list.declare,
                });
            }
        }
    }

    /// `var` declarations inside a top-level statement that isn't a declaration, found
    /// without recursion: they belong to module scope.
    fn hoist_vars(&mut self, statement: &Statement<'_>) {
        let mut pending = vec![statement];
        while let Some(statement) = pending.pop() {
            match statement {
                Statement::VariableDeclaration(list) => self.variables(list, Exported::default(), true),
                Statement::BlockStatement(block) => pending.extend(block.body.iter().rev()),
                Statement::IfStatement(it) => {
                    pending.extend(it.alternate.iter());
                    pending.push(&it.consequent);
                }
                Statement::LabeledStatement(it) => pending.push(&it.body),
                Statement::WhileStatement(it) => pending.push(&it.body),
                Statement::DoWhileStatement(it) => pending.push(&it.body),
                Statement::ForStatement(it) => {
                    pending.push(&it.body);
                    if let Some(ForStatementInit::VariableDeclaration(list)) = &it.init {
                        self.variables(list, Exported::default(), true);
                    }
                }
                Statement::ForInStatement(it) => {
                    pending.push(&it.body);
                    if let ForStatementLeft::VariableDeclaration(list) = &it.left {
                        self.variables(list, Exported::default(), true);
                    }
                }
                Statement::ForOfStatement(it) => {
                    pending.push(&it.body);
                    if let ForStatementLeft::VariableDeclaration(list) = &it.left {
                        self.variables(list, Exported::default(), true);
                    }
                }
                Statement::TryStatement(it) => {
                    if let Some(finalizer) = &it.finalizer {
                        pending.extend(finalizer.body.iter().rev());
                    }
                    if let Some(handler) = &it.handler {
                        pending.extend(handler.body.body.iter().rev());
                    }
                    pending.extend(it.block.body.iter().rev());
                }
                Statement::SwitchStatement(it) => {
                    for case in it.cases.iter().rev() {
                        pending.extend(case.consequent.iter().rev());
                    }
                }
                Statement::WithStatement(it) => pending.push(&it.body),
                _ => {}
            }
        }
    }

    /// `export default <expression>` or `export = <expression>`.
    fn value_export(&mut self, exported: &str, expression: &Expression<'_>) {
        let expression = unwrap(expression);
        let entry = match access_path(expression) {
            Some(path) => ExportEntry::Value {
                exported: exported.into(),
                path: Some(path),
                declaration: None,
            },
            None => {
                let declaration = self.declare(Decl {
                    name: "".into(),
                    kind: DeclarationKind::Expression,
                    range: range(expression.span()),
                    exported: false,
                    default: exported == "default",
                    ambient: false,
                });
                ExportEntry::Value {
                    exported: exported.into(),
                    path: None,
                    declaration: Some(declaration),
                }
            }
        };
        self.facts.exports.push(entry);
    }
}

/// A declaration with nothing but its name, kind, range, and export.
fn plain(name: &str, kind: DeclarationKind, span: Span, exported: Exported) -> Decl {
    Decl {
        name: name.into(),
        kind,
        range: range(span),
        exported: exported.named,
        default: exported.default,
        ambient: false,
    }
}

/// Whether `expression` is an access path (`a`, `a.b`, `a["b"]`, through transparent
/// wrappers), which an `export default` or `export =` follows instead of declaring.
pub(crate) fn is_access_path(expression: &Expression<'_>) -> bool {
    access_path(expression).is_some()
}

/// The expression inside parentheses, `as`, `satisfies`, `!`, and `<T>` assertions.
pub(crate) fn unwrap_expression<'e, 'a>(expression: &'e Expression<'a>) -> &'e Expression<'a> {
    unwrap(expression)
}

mod forms;
#[cfg(test)]
mod tests;
