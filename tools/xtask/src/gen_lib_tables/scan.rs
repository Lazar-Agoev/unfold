//! Reading declaration files into a model of the global scope: which names are values, which
//! are classes or interfaces, the namespaces they form (merged across files), and the ambient
//! module names they declare. This reproduces, syntactically, what the TypeScript checker's
//! global symbol table says about the default library and `@types/node`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, bail};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, ModuleDeclaration, Statement,
    TSModuleReference, TSNamespaceDeclaration, TSNamespaceDeclarationBody,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Deepest namespace nesting followed; declaration files stay far below it.
const MAX_DEPTH: usize = 64;

/// One name of a scope, merged across its declarations.
#[derive(Debug, Default)]
pub(crate) struct Symbol {
    /// Some declaration has value meaning.
    pub(crate) value: bool,
    /// Some declaration is a class or an interface.
    pub(crate) class_or_interface: bool,
    /// The exports of its namespace declarations, if it has any.
    pub(crate) namespace: Option<Scope>,
}

/// A symbol table: the global scope, or a namespace's exports.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub(crate) symbols: BTreeMap<String, Symbol>,
}

impl Scope {
    fn symbol(&mut self, name: &str) -> &mut Symbol {
        self.symbols.entry(name.to_owned()).or_default()
    }
}

/// Everything the declaration files contribute.
#[derive(Debug, Default)]
pub(crate) struct Model {
    /// The global scope.
    pub(crate) globals: Scope,
    /// The quoted names of ambient module declarations (`declare module "fs"`).
    pub(crate) ambient_modules: BTreeSet<String>,
}

impl Model {
    /// Adds one declaration file. `name` is only used in error messages.
    pub(crate) fn add_file(&mut self, name: &str, text: &str) -> anyhow::Result<()> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, text, SourceType::d_ts()).parse();
        if let Some(diagnostic) = parsed.diagnostics.first() {
            bail!("{name} does not parse: {}", diagnostic.message);
        }
        if parsed.fatal_error {
            bail!("{name} does not parse");
        }
        let body = &parsed.program.body;
        let mut walker = Walker {
            ambient_modules: &mut self.ambient_modules,
            augmentations: Vec::new(),
            enclosing: vec![body],
        };
        let result = if body.iter().any(is_module_indicator) {
            // A module's own declarations are not global; only its augmentations are.
            body.iter()
                .try_for_each(|statement| walker.outside_scope(statement, 0))
        } else {
            body.iter()
                .try_for_each(|statement| walker.statement(&mut self.globals, statement, true, 0))
        };
        result.with_context(|| format!("in {name}"))?;
        // Global augmentations found anywhere merge into the global scope (and may hold more).
        let mut next = 0;
        while let Some(augmentation) = walker.augmentations.get(next).cloned() {
            next += 1;
            walker.enclosing = augmentation.enclosing;
            walker
                .block(&mut self.globals, augmentation.statements, augmentation.depth)
                .with_context(|| format!("in {name}"))?;
        }
        Ok(())
    }
}

/// One file's walk: the ambient module names it declares, and the bodies of its global
/// augmentations, which are declared into the global scope after the walk.
/// A `declare global` body: its statements, depth, and the statement lists around it.
#[derive(Clone)]
struct Augmentation<'a> {
    statements: &'a [Statement<'a>],
    depth: usize,
    enclosing: Vec<&'a [Statement<'a>]>,
}

struct Walker<'m, 'a> {
    ambient_modules: &'m mut BTreeSet<String>,
    /// The global augmentations found, declared into the global scope after the walk.
    augmentations: Vec<Augmentation<'a>>,
    /// The statement lists around the statement being walked, outermost first.
    enclosing: Vec<&'a [Statement<'a>]>,
}

impl<'a> Walker<'_, 'a> {
    /// Queues a global augmentation's body.
    fn augment(&mut self, statements: &'a [Statement<'a>], depth: usize) {
        self.augmentations.push(Augmentation {
            statements,
            depth,
            enclosing: self.enclosing.clone(),
        });
    }

    /// The global augmentations and ambient modules of a statement that isn't in the global
    /// scope (a module file's top level, or an ambient module's body).
    fn outside_scope(&mut self, statement: &'a Statement<'a>, depth: usize) -> anyhow::Result<()> {
        match statement.as_declaration() {
            Some(Declaration::TSGlobalDeclaration(global)) => {
                self.augment(&global.body.body, depth + 1);
                Ok(())
            }
            Some(Declaration::TSExternalModuleDeclaration(module)) => self.ambient_module(module, depth),
            _ => Ok(()),
        }
    }

    fn ambient_module(
        &mut self,
        module: &'a oxc_ast::ast::TSExternalModuleDeclaration<'a>,
        depth: usize,
    ) -> anyhow::Result<()> {
        if depth > MAX_DEPTH {
            bail!("declarations nest more than {MAX_DEPTH} levels deep");
        }
        self.ambient_modules.insert(module.id.value.to_string());
        let Some(body) = &module.body else {
            return Ok(());
        };
        self.enclosing.push(&body.body);
        let result = body
            .body
            .iter()
            .try_for_each(|statement| self.outside_scope(statement, depth + 1));
        self.enclosing.pop();
        result
    }

    /// The members of a namespace-like block (a namespace body or `declare global`): every
    /// declaration is exported unless the block has export declarations, in which case only
    /// `export`-modified ones are.
    fn block(
        &mut self,
        scope: &mut Scope,
        statements: &'a [Statement<'a>],
        depth: usize,
    ) -> anyhow::Result<()> {
        let implicit = !has_export_declarations(statements);
        self.enclosing.push(statements);
        let result = statements
            .iter()
            .try_for_each(|statement| self.statement(scope, statement, implicit, depth));
        self.enclosing.pop();
        result
    }

    /// Declares what `statement` adds to `scope`; plain declarations count when `implicit`.
    fn statement(
        &mut self,
        scope: &mut Scope,
        statement: &'a Statement<'a>,
        implicit: bool,
        depth: usize,
    ) -> anyhow::Result<()> {
        if depth > MAX_DEPTH {
            bail!("declarations nest more than {MAX_DEPTH} levels deep");
        }
        if let Some(declaration) = statement.as_declaration() {
            return match declaration {
                // Ambient modules and global augmentations never belong to the scope itself.
                Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => {
                    self.outside_scope(statement, depth)
                }
                _ if implicit => self.declare(scope, declaration, depth),
                _ => Ok(()),
            };
        }
        match statement.as_module_declaration() {
            Some(ModuleDeclaration::ExportDeclaration(export)) => {
                self.declare(scope, &export.declaration, depth)
            }
            _ => Ok(()),
        }
    }

    fn declare(
        &mut self,
        scope: &mut Scope,
        declaration: &'a Declaration<'a>,
        depth: usize,
    ) -> anyhow::Result<()> {
        match declaration {
            Declaration::VariableDeclaration(list) => {
                for declarator in &list.declarations {
                    for name in binding_names(&declarator.id) {
                        scope.symbol(name).value = true;
                    }
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    scope.symbol(id.name.as_str()).value = true;
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    let symbol = scope.symbol(id.name.as_str());
                    symbol.value = true;
                    symbol.class_or_interface = true;
                }
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                scope.symbol(interface.id.name.as_str()).class_or_interface = true;
            }
            Declaration::TSEnumDeclaration(enumeration) => {
                scope.symbol(enumeration.id.name.as_str()).value = true;
            }
            Declaration::TSNamespaceDeclaration(namespace) => self.namespace(scope, namespace, depth + 1)?,
            Declaration::TSExternalModuleDeclaration(module) => self.ambient_module(module, depth)?,
            Declaration::TSGlobalDeclaration(global) => {
                self.augment(&global.body.body, depth + 1);
            }
            // Type aliases add no class, interface, or value; import aliases are aliases.
            Declaration::TSTypeAliasDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {}
        }
        Ok(())
    }

    fn namespace(
        &mut self,
        scope: &mut Scope,
        namespace: &'a TSNamespaceDeclaration<'a>,
        depth: usize,
    ) -> anyhow::Result<()> {
        if depth > MAX_DEPTH {
            bail!("namespaces nest more than {MAX_DEPTH} levels deep");
        }
        let symbol = scope.symbol(namespace.id.name.as_str());
        symbol.value |= namespace_instantiated(namespace, &self.enclosing);
        let exports = symbol.namespace.get_or_insert_with(Scope::default);
        match &namespace.body {
            // `namespace A.B {}`: `B` is always an export of `A`.
            TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
                self.namespace(exports, inner, depth + 1)
            }
            TSNamespaceDeclarationBody::TSModuleBlock(block) => self.block(exports, &block.body, depth + 1),
        }
    }
}

/// Whether a top-level statement makes its file a module: an import, an export, or an
/// `import x = require(…)`.
fn is_module_indicator(statement: &Statement<'_>) -> bool {
    match statement.as_module_declaration() {
        Some(ModuleDeclaration::TSNamespaceExportDeclaration(_)) => false,
        Some(_) => true,
        None => matches!(
            statement.as_declaration(),
            Some(Declaration::TSImportEqualsDeclaration(it))
                if matches!(it.module_reference, TSModuleReference::ExternalModuleReference(_))
        ),
    }
}

/// Whether a block has export declarations (`export {…}`, `export * from`, `export =`, or
/// `export default <expression>`), which turn off implicit exports in ambient blocks.
fn has_export_declarations(statements: &[Statement<'_>]) -> bool {
    statements
        .iter()
        .any(|statement| match statement.as_module_declaration() {
            Some(
                ModuleDeclaration::ExportNamedDeclaration(_)
                | ModuleDeclaration::ExportFromDeclaration(_)
                | ModuleDeclaration::ExportAllDeclaration(_)
                | ModuleDeclaration::TSExportAssignment(_),
            ) => true,
            Some(ModuleDeclaration::ExportDefaultDeclaration(default)) => !matches!(
                default.declaration,
                ExportDefaultDeclarationKind::FunctionDeclaration(_)
                    | ExportDefaultDeclarationKind::ClassDeclaration(_)
                    | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_)
            ),
            _ => false,
        })
}

/// The identifiers a binding pattern binds.
fn binding_names<'a>(pattern: &'a BindingPattern<'_>) -> Vec<&'a str> {
    pattern
        .get_binding_identifiers()
        .into_iter()
        .map(|id| id.name.as_str())
        .collect()
}

/// Whether a namespace declaration is instantiated (TypeScript's module instance state is not
/// `NonInstantiated`), which gives its symbol value meaning. `enclosing` holds the statement
/// lists around the declaration, outermost first, where `export { name }` looks names up.
fn namespace_instantiated<'a>(
    namespace: &'a TSNamespaceDeclaration<'a>,
    enclosing: &[&'a [Statement<'a>]],
) -> bool {
    match &namespace.body {
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => namespace_instantiated(inner, enclosing),
        TSNamespaceDeclarationBody::TSModuleBlock(block) => {
            let mut scopes = enclosing.to_vec();
            scopes.push(&block.body);
            block
                .body
                .iter()
                .any(|statement| statement_instantiated(statement, &scopes))
        }
    }
}

/// Whether one statement of the innermost of `scopes` is instantiated.
fn statement_instantiated<'a>(statement: &'a Statement<'a>, scopes: &[&'a [Statement<'a>]]) -> bool {
    if let Some(declaration) = statement.as_declaration() {
        return declaration_instantiated(declaration, false, scopes);
    }
    match statement.as_module_declaration() {
        Some(ModuleDeclaration::ExportDeclaration(export)) => {
            declaration_instantiated(&export.declaration, true, scopes)
        }
        // `export { a, b }` is as instantiated as what its names refer to in the nearest
        // enclosing statement list that declares them (or instantiated when a name isn't found,
        // since it could refer to a value).
        Some(ModuleDeclaration::ExportNamedDeclaration(named)) => named.specifiers.iter().any(|specifier| {
            let name = specifier.local.name();
            for (depth, statements) in scopes.iter().enumerate().rev() {
                let declaring: Vec<&Statement<'_>> =
                    statements.iter().filter(|other| declares(other, &name)).collect();
                if !declaring.is_empty() {
                    return declaring
                        .iter()
                        .any(|other| statement_instantiated(other, &scopes[..=depth]));
                }
            }
            true
        }),
        _ => true,
    }
}

fn declaration_instantiated<'a>(
    declaration: &'a Declaration<'a>,
    exported: bool,
    scopes: &[&'a [Statement<'a>]],
) -> bool {
    match declaration {
        Declaration::TSInterfaceDeclaration(_) | Declaration::TSTypeAliasDeclaration(_) => false,
        Declaration::TSImportEqualsDeclaration(_) => exported,
        Declaration::TSNamespaceDeclaration(inner) => namespace_instantiated(inner, scopes),
        _ => true,
    }
}

/// Whether a statement declares `name` (for `export { name }` lookups).
fn declares(statement: &Statement<'_>, name: &str) -> bool {
    let declaration = match statement.as_module_declaration() {
        Some(ModuleDeclaration::ExportDeclaration(export)) => &export.declaration,
        _ => match statement.as_declaration() {
            Some(declaration) => declaration,
            None => return false,
        },
    };
    match declaration {
        Declaration::VariableDeclaration(list) => list
            .declarations
            .iter()
            .any(|declarator| binding_names(&declarator.id).contains(&name)),
        Declaration::FunctionDeclaration(it) => it.id.as_ref().is_some_and(|id| id.name == name),
        Declaration::ClassDeclaration(it) => it.id.as_ref().is_some_and(|id| id.name == name),
        Declaration::TSInterfaceDeclaration(it) => it.id.name == name,
        Declaration::TSTypeAliasDeclaration(it) => it.id.name == name,
        Declaration::TSEnumDeclaration(it) => it.id.name == name,
        Declaration::TSNamespaceDeclaration(it) => it.id.name == name,
        Declaration::TSImportEqualsDeclaration(it) => it.id.name == name,
        Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => false,
    }
}

/// Whether `name` is a plain ASCII identifier, as the tables keep.
pub(crate) fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

impl Model {
    /// Global value names, plus the checker's intrinsic `globalThis` and `undefined`.
    pub(crate) fn global_values(&self) -> Vec<String> {
        let mut names: BTreeSet<String> = self
            .globals
            .symbols
            .iter()
            .filter(|(name, symbol)| symbol.value && is_plain_identifier(name))
            .map(|(name, _)| name.clone())
            .collect();
        names.insert("globalThis".to_owned());
        names.insert("undefined".to_owned());
        names.into_iter().collect()
    }

    /// Class and interface names in the global scope, qualified through namespaces.
    pub(crate) fn library_types(&self) -> Vec<String> {
        let mut names = BTreeSet::new();
        collect_types(&self.globals, "", &mut names, 0);
        names.into_iter().collect()
    }
}

fn collect_types(scope: &Scope, prefix: &str, out: &mut BTreeSet<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for (name, symbol) in &scope.symbols {
        if !is_plain_identifier(name) {
            continue;
        }
        let qualified = format!("{prefix}{name}");
        if symbol.class_or_interface {
            out.insert(qualified.clone());
        }
        if let Some(namespace) = &symbol.namespace {
            collect_types(namespace, &format!("{qualified}."), out, depth + 1);
        }
    }
}
