//! Reading typeshed `.pyi` files into the names each module and class binds, with Ruff's parser.
//!
//! `sys.version_info` conditions are decided for one Python version. `sys.platform` conditions
//! (and any other condition) are taken both ways: the names of every platform are kept, and a
//! name bound differently on different platforms is merged later like an overload. Star imports
//! follow the imported module's `__all__` (or its public names when it has none).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, bail};
use ruff_python_ast::{BoolOp, CmpOp, Expr, Number, Operator, Stmt, UnaryOp};

/// Deepest nesting of star imports followed.
const MAX_DEPTH: usize = 32;

/// What one statement binds a name to. A name may have several bindings: overloads, and the
/// branches of platform conditions.
#[derive(Debug, Clone)]
pub(crate) enum Binding {
    /// `def` or `async def`.
    Function(Rc<FunctionDef>),
    /// `class`.
    Class(Rc<ClassDef>),
    /// `import a.b as c` (the module `a.b`), or `import a.b` (the module `a`).
    Module { module: String, exported: bool },
    /// `from m import n as k`, or one name of `from m import *`.
    Import {
        module: String,
        name: String,
        exported: bool,
    },
    /// `x: T` or `x: T = …`.
    Variable(Option<Expr>),
    /// `X: TypeAlias = T`, `type X = T`, or `X = T[…]` / `X = A | B`.
    TypeAlias(Expr),
    /// `x = y` or `x = m.y`: the name `y` refers to.
    Assign(Expr),
    /// Any other assignment (`_T = TypeVar("_T")`, `x = 1`).
    Opaque,
}

impl Binding {
    /// Whether the binding is exported from a stub: definitions are; imports only when written
    /// `as` the same name or star-imported.
    pub(crate) fn exported(&self) -> bool {
        match self {
            Self::Module { exported, .. } | Self::Import { exported, .. } => *exported,
            _ => true,
        }
    }
}

/// A `def`.
#[derive(Debug)]
pub(crate) struct FunctionDef {
    pub(crate) is_async: bool,
    /// The decorators' expressions.
    pub(crate) decorators: Vec<Expr>,
    /// The written return annotation.
    pub(crate) returns: Option<Expr>,
}

impl FunctionDef {
    /// Whether some decorator's last name is `name` (`@property`, `@functools.cached_property`,
    /// `@x.setter`).
    pub(crate) fn decorated(&self, name: &str) -> bool {
        self.decorators
            .iter()
            .any(|decorator| last_name(decorator) == Some(name))
    }
}

/// A `class`.
#[derive(Debug)]
pub(crate) struct ClassDef {
    /// The module whose names its annotations refer to.
    pub(crate) module: String,
    /// `module.Name`, or `module.Outer.Name` for a nested class.
    pub(crate) qualified: String,
    /// `@type_check_only`: the class exists only in the stubs.
    pub(crate) type_check_only: bool,
    /// The base class expressions as written (keywords such as `metaclass=` left out).
    pub(crate) bases: Vec<Expr>,
    /// The names its body binds.
    pub(crate) members: BTreeMap<String, Vec<Binding>>,
}

/// One stub module: the names it binds and its `__all__`.
#[derive(Debug, Default)]
pub(crate) struct ModuleStub {
    pub(crate) bindings: BTreeMap<String, Vec<Binding>>,
    pub(crate) all: Option<Vec<String>>,
    /// Whether it is a package (`__init__.pyi`).
    pub(crate) package: bool,
}

/// One stub file read: its path under `stdlib/` and its text.
pub(crate) struct File {
    pub(crate) name: String,
    pub(crate) text: String,
}

/// Reads and caches the modules of a typeshed `stdlib/` folder.
pub(crate) struct Loader {
    stdlib: PathBuf,
    /// The `sys.version_info` the conditions are decided for.
    version: (u64, u64),
    modules: BTreeMap<String, Option<Rc<ModuleStub>>>,
    /// Modules being read, to refuse star-import cycles.
    reading: Vec<String>,
    /// Every file read, for the header's digest.
    pub(crate) files: Vec<File>,
}

impl Loader {
    pub(crate) fn new(stdlib: PathBuf, version: (u64, u64)) -> Self {
        Self {
            stdlib,
            version,
            modules: BTreeMap::new(),
            reading: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The module `name` (`os.path`), or `None` when typeshed has no stub for it.
    pub(crate) fn module(&mut self, name: &str) -> anyhow::Result<Option<Rc<ModuleStub>>> {
        if let Some(module) = self.modules.get(name) {
            return Ok(module.clone());
        }
        if self.reading.iter().any(|reading| reading == name) {
            bail!("star imports form a cycle through `{name}`");
        }
        if self.reading.len() > MAX_DEPTH {
            bail!("star imports nest more than {MAX_DEPTH} levels deep at `{name}`");
        }
        let parts: Vec<&str> = name.split('.').collect();
        let relative = parts.join("/");
        let candidates = [
            (format!("{relative}.pyi"), false),
            (format!("{relative}/__init__.pyi"), true),
        ];
        let Some((file, package)) = candidates
            .into_iter()
            .find(|(file, _)| self.stdlib.join(file).is_file())
        else {
            self.modules.insert(name.to_owned(), None);
            return Ok(None);
        };
        let path = self.stdlib.join(&file);
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let parsed = ruff_python_parser::parse_module(&text)
            .map_err(|error| anyhow::anyhow!("{file} does not parse: {error}"))?;
        self.files.push(File {
            name: file.clone(),
            text,
        });
        self.reading.push(name.to_owned());
        let mut module = ModuleStub {
            package,
            ..ModuleStub::default()
        };
        let body = parsed.into_syntax().body;
        let result = self.module_body(name, &mut module, &body);
        self.reading.pop();
        result.with_context(|| format!("in {file}"))?;
        let module = Rc::new(module);
        self.modules.insert(name.to_owned(), Some(module.clone()));
        Ok(Some(module))
    }

    /// Whether `name` is a module typeshed has a stub for.
    pub(crate) fn exists(&mut self, name: &str) -> anyhow::Result<bool> {
        Ok(self.module(name)?.is_some())
    }

    fn module_body(&mut self, name: &str, module: &mut ModuleStub, body: &[Stmt]) -> anyhow::Result<()> {
        let mut statements = Vec::new();
        self.flatten(body, &mut statements);
        for statement in statements {
            self.module_statement(name, module, statement)?;
        }
        Ok(())
    }

    /// The statements of `body` that apply, with `if` blocks decided or taken both ways.
    fn flatten<'s>(&self, body: &'s [Stmt], out: &mut Vec<&'s Stmt>) {
        for statement in body {
            let Stmt::If(branch) = statement else {
                out.push(statement);
                continue;
            };
            match condition(&branch.test, self.version) {
                Truth::True => {
                    self.flatten(&branch.body, out);
                    continue;
                }
                Truth::Either => self.flatten(&branch.body, out),
                Truth::False => {}
            }
            for clause in &branch.elif_else_clauses {
                let truth = clause
                    .test
                    .as_ref()
                    .map_or(Truth::True, |test| condition(test, self.version));
                if truth != Truth::False {
                    self.flatten(&clause.body, out);
                }
                if truth == Truth::True {
                    break;
                }
            }
        }
    }

    fn module_statement(
        &mut self,
        name: &str,
        module: &mut ModuleStub,
        statement: &Stmt,
    ) -> anyhow::Result<()> {
        match statement {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let imported = alias.name.as_str();
                    match &alias.asname {
                        Some(asname) => bind(
                            &mut module.bindings,
                            asname.as_str(),
                            Binding::Module {
                                module: imported.to_owned(),
                                exported: asname.as_str() == imported,
                            },
                        ),
                        None => {
                            let first = imported.split('.').next().unwrap_or(imported);
                            bind(
                                &mut module.bindings,
                                first,
                                Binding::Module {
                                    module: first.to_owned(),
                                    exported: false,
                                },
                            );
                        }
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                let from = absolute_module(
                    name,
                    module.package,
                    import.level,
                    import.module.as_ref().map(|module| module.as_str()),
                )?;
                for alias in &import.names {
                    let imported = alias.name.as_str();
                    if imported == "*" {
                        for star in self.star_names(&from)? {
                            bind(
                                &mut module.bindings,
                                &star,
                                Binding::Import {
                                    module: from.clone(),
                                    name: star.clone(),
                                    exported: true,
                                },
                            );
                        }
                    } else if imported == "__all__" {
                        let source = self.module(&from)?;
                        module.all = source.and_then(|source| source.all.clone());
                    } else {
                        let local = alias.asname.as_ref().map_or(imported, |asname| asname.as_str());
                        bind(
                            &mut module.bindings,
                            local,
                            Binding::Import {
                                module: from.clone(),
                                name: imported.to_owned(),
                                exported: alias
                                    .asname
                                    .as_ref()
                                    .is_some_and(|asname| asname.as_str() == imported),
                            },
                        );
                    }
                }
            }
            Stmt::Assign(assign) if is_all(&assign.targets) => {
                module.all = Some(string_list(&assign.value));
            }
            Stmt::AugAssign(assign) if assign.op == Operator::Add && is_name(&assign.target, "__all__") => {
                module
                    .all
                    .get_or_insert_with(Vec::new)
                    .extend(string_list(&assign.value));
            }
            Stmt::AnnAssign(assign) if is_name(&assign.target, "__all__") => {
                if let Some(value) = &assign.value {
                    module.all = Some(string_list(value));
                }
            }
            _ => body_statement(name, name, &mut module.bindings, statement, self),
        }
        Ok(())
    }

    /// The names `from module import *` binds: its `__all__`, or its exported names that don't
    /// start with an underscore.
    fn star_names(&mut self, module: &str) -> anyhow::Result<Vec<String>> {
        let Some(stub) = self.module(module)? else {
            bail!("`from {module} import *`: typeshed has no stub for `{module}`");
        };
        if let Some(all) = &stub.all {
            return Ok(all.clone());
        }
        Ok(stub
            .bindings
            .iter()
            .filter(|(name, bindings)| !name.starts_with('_') && bindings.iter().any(Binding::exported))
            .map(|(name, _)| name.clone())
            .collect())
    }
}

/// Adds a binding for `name`.
fn bind(bindings: &mut BTreeMap<String, Vec<Binding>>, name: &str, binding: Binding) {
    bindings.entry(name.to_owned()).or_default().push(binding);
}

/// Binds what a definition or assignment statement declares, in a module body or (with
/// `scope` naming the enclosing class) a class body. `module` is the module the annotations
/// refer to.
fn body_statement(
    module: &str,
    scope: &str,
    bindings: &mut BTreeMap<String, Vec<Binding>>,
    statement: &Stmt,
    loader: &Loader,
) {
    match statement {
        Stmt::FunctionDef(function) => bind(
            bindings,
            function.name.as_str(),
            Binding::Function(Rc::new(FunctionDef {
                is_async: function.is_async,
                decorators: function
                    .decorator_list
                    .iter()
                    .map(|decorator| decorator.expression.clone())
                    .collect(),
                returns: function.returns.as_deref().cloned(),
            })),
        ),
        Stmt::ClassDef(class) => {
            let qualified = format!("{scope}.{}", class.name.as_str());
            let mut members = BTreeMap::new();
            let mut statements = Vec::new();
            loader.flatten(&class.body, &mut statements);
            for statement in statements {
                body_statement(module, &qualified, &mut members, statement, loader);
            }
            let bases = class
                .arguments
                .as_ref()
                .map(|arguments| arguments.args.to_vec())
                .unwrap_or_default();
            let type_check_only = class
                .decorator_list
                .iter()
                .any(|decorator| last_name(&decorator.expression) == Some("type_check_only"));
            bind(
                bindings,
                class.name.as_str(),
                Binding::Class(Rc::new(ClassDef {
                    module: module.to_owned(),
                    qualified,
                    type_check_only,
                    bases,
                    members,
                })),
            );
        }
        Stmt::AnnAssign(assign) => {
            let Expr::Name(target) = &*assign.target else {
                return;
            };
            let binding = match &assign.value {
                Some(value) if last_name(&assign.annotation) == Some("TypeAlias") => {
                    Binding::TypeAlias((**value).clone())
                }
                _ => Binding::Variable(Some((*assign.annotation).clone())),
            };
            bind(bindings, target.id.as_str(), binding);
        }
        Stmt::Assign(assign) => {
            for target in &assign.targets {
                let Expr::Name(target) = target else {
                    continue;
                };
                let binding = match &*assign.value {
                    Expr::Name(_) | Expr::Attribute(_) => Binding::Assign((*assign.value).clone()),
                    Expr::Subscript(_) | Expr::BinOp(_) => Binding::TypeAlias((*assign.value).clone()),
                    _ => Binding::Opaque,
                };
                bind(bindings, target.id.as_str(), binding);
            }
        }
        Stmt::TypeAlias(alias) => {
            if let Expr::Name(name) = &*alias.name {
                bind(
                    bindings,
                    name.id.as_str(),
                    Binding::TypeAlias((*alias.value).clone()),
                );
            }
        }
        _ => {}
    }
}

/// The absolute name of the module `from <dots><name> import …` reads in module `current`.
pub(crate) fn absolute_module(
    current: &str,
    package: bool,
    level: u32,
    name: Option<&str>,
) -> anyhow::Result<String> {
    if level == 0 {
        return name.map(str::to_owned).context("`from import` without a module");
    }
    let mut parts: Vec<&str> = current.split('.').collect();
    // A module's own package is one level up; a package is its own.
    let up = usize::try_from(level)? - usize::from(package);
    for _ in 0..up {
        if parts.pop().is_none() {
            bail!("a relative import in `{current}` leaves the top-level package");
        }
    }
    if parts.is_empty() {
        bail!("a relative import in `{current}` leaves the top-level package");
    }
    if let Some(name) = name {
        parts.push(name);
    }
    Ok(parts.join("."))
}

/// A condition's value: decided, or either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Truth {
    True,
    False,
    Either,
}

impl Truth {
    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Either => Self::Either,
        }
    }
}

/// Decides `sys.version_info` comparisons for `version`; everything else is either.
pub(crate) fn condition(test: &Expr, version: (u64, u64)) -> Truth {
    match test {
        Expr::BoolOp(operation) => {
            let values = operation.values.iter().map(|value| condition(value, version));
            let (absorbing, identity) = match operation.op {
                BoolOp::And => (Truth::False, Truth::True),
                BoolOp::Or => (Truth::True, Truth::False),
            };
            let mut result = identity;
            for value in values {
                if value == absorbing {
                    return absorbing;
                }
                if value == Truth::Either {
                    result = Truth::Either;
                }
            }
            result
        }
        Expr::UnaryOp(operation) if operation.op == UnaryOp::Not => {
            condition(&operation.operand, version).not()
        }
        Expr::Compare(compare) => version_comparison(compare, version).unwrap_or(Truth::Either),
        _ => Truth::Either,
    }
}

/// `sys.version_info <op> (major, minor, …)`.
fn version_comparison(compare: &ruff_python_ast::ExprCompare, version: (u64, u64)) -> Option<Truth> {
    let ([op], [left, right]) = (&*compare.ops, &*compare.operands) else {
        return None;
    };
    let Expr::Attribute(attribute) = left else {
        return None;
    };
    if attribute.attr.as_str() != "version_info" || !is_name(&attribute.value, "sys") {
        return None;
    }
    let Expr::Tuple(tuple) = right else {
        return None;
    };
    let mut written = Vec::new();
    for element in &tuple.elts {
        let Expr::NumberLiteral(number) = element else {
            return None;
        };
        let Number::Int(int) = &number.value else {
            return None;
        };
        written.push(int.as_u64()?);
    }
    // `sys.version_info` is longer than any written prefix of major and minor, so it compares
    // greater when they are equal; a written micro version can't be decided.
    let ours = [version.0, version.1];
    let shared = written.len().min(ours.len());
    let ordering = match ours[..shared].cmp(&written[..shared]) {
        Ordering::Equal if written.len() > ours.len() => return None,
        Ordering::Equal => Ordering::Greater,
        other => other,
    };
    let truth = match op {
        CmpOp::Lt => ordering == Ordering::Less,
        CmpOp::LtE => ordering != Ordering::Greater,
        CmpOp::Gt => ordering == Ordering::Greater,
        CmpOp::GtE => ordering != Ordering::Less,
        CmpOp::Eq => ordering == Ordering::Equal,
        CmpOp::NotEq => ordering != Ordering::Equal,
        _ => return None,
    };
    Some(if truth { Truth::True } else { Truth::False })
}

/// Whether `expr` is the plain name `name`.
pub(crate) fn is_name(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Name(it) if it.id.as_str() == name)
}

/// Whether an assignment's only target is `__all__`.
fn is_all(targets: &[Expr]) -> bool {
    matches!(targets, [target] if is_name(target, "__all__"))
}

/// The last name of a dotted expression or call (`typing.TypeAlias` → `TypeAlias`,
/// `deprecated("…")` → `deprecated`).
pub(crate) fn last_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        Expr::Call(call) => last_name(&call.func),
        _ => None,
    }
}

/// The strings of a list or tuple of string literals.
fn string_list(expr: &Expr) -> Vec<String> {
    let elements = match expr {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return Vec::new(),
    };
    elements
        .iter()
        .filter_map(|element| match element {
            Expr::StringLiteral(literal) => Some(literal.value.to_str().to_owned()),
            _ => None,
        })
        .collect()
}
