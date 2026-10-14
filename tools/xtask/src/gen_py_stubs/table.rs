//! Turning the stub modules into the table: the members of the covered modules and of every class
//! they reach, with written types read without generics.
//!
//! A written type is kept in exactly the forms a call's value allows: a class (`-> Logger`), a class
//! or nothing (`-> Match[str] | None`, `Optional[X]`), a quoted name (`-> "Repo"`), read through
//! `await` for an `async def`; and `Self`, which is the receiver's class. Type arguments are
//! dropped (`dict[str, int]` is `dict`); anything else (a type variable, a union of classes, a
//! callable, `type[X]`, `Any`) leaves the value unknown. `LiteralString` is read as `str`, which
//! it is. A name with several bindings (overloads,
//! or the branches of platform conditions) keeps a type only when every binding agrees.
//!
//! One table is read per Python version, and the tables are then intersected ([`intersect`]): a
//! module, class, or member is kept only when every version has it, and a type only when every
//! version agrees on it.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use anyhow::bail;
use ruff_python_ast::{Expr, Operator};

use super::stubs::{Binding, ClassDef, FunctionDef, Loader, last_name};

/// Deepest chain of imports, assignments, and aliases followed for one name.
const MAX_DEPTH: usize = 32;

/// The special forms of `typing` and `typing_extensions` the reading treats by name. Any other
/// name of theirs is looked up in the stubs like any other.
const SPECIAL_FORMS: &[&str] = &[
    "Annotated",
    "Any",
    "Callable",
    "ClassVar",
    "Concatenate",
    "Final",
    "Generic",
    "Literal",
    "LiteralString",
    "Never",
    "NoReturn",
    "NotRequired",
    "Optional",
    "Protocol",
    "ReadOnly",
    "Required",
    "Self",
    "Type",
    "TypeAlias",
    "TypeGuard",
    "TypeIs",
    "TypedDict",
    "Union",
    "Unpack",
];

/// What calling a function gives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Returns {
    Unknown,
    /// An instance of the class.
    Instance(String),
    /// An instance of the receiver's class (`Self`).
    Receiver,
    /// After `await` (an `async def`): an instance of the class.
    AwaitInstance(String),
    /// After `await`: an instance of the receiver's class.
    AwaitReceiver,
}

/// One member of a module or class.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Member {
    /// A function or method.
    Function(Returns),
    /// A class, by its qualified name.
    Class(String),
    /// A module.
    Module(String),
    /// A variable, attribute, or property, with its written class when it has one.
    Variable(Option<String>),
    /// A property written `Self`: the instance it is read on.
    SelfVariable,
}

/// A module of the table.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TableModule {
    pub(crate) name: String,
    pub(crate) members: BTreeMap<String, Member>,
}

/// A class of the table.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TableClass {
    pub(crate) name: String,
    /// The base classes as written, in order; `Generic[…]` and `Protocol[…]` are left out.
    pub(crate) bases: Vec<String>,
    /// Whether `Protocol` (or `Protocol[…]`) is among its written bases: any class with the
    /// members implements it, whatever its bases.
    pub(crate) protocol: bool,
    pub(crate) members: BTreeMap<String, Member>,
}

/// The table.
#[derive(Debug, Default)]
pub(crate) struct Table {
    pub(crate) modules: Vec<TableModule>,
    pub(crate) classes: Vec<TableClass>,
}

/// What a name refers to.
#[derive(Debug, Clone)]
enum Symbol {
    Class(Rc<ClassDef>),
    /// A function's definitions, each with the module its annotations refer to.
    Function(Vec<(String, Rc<FunctionDef>)>),
    Module(String),
    /// A variable's annotations, each with the module it refers to.
    Variable(Vec<(String, Option<Expr>)>),
    TypeAlias(Expr, String),
    Special(&'static str),
    /// A value of unknown kind (`TypeVar(…)`, `x = 1`).
    Opaque,
    Missing,
    /// Bindings that disagree in kind or class.
    Ambiguous,
}

/// A base class, read.
enum Base {
    Class(Rc<ClassDef>),
    /// `Generic[…]`, which adds no members.
    Marker,
    /// `Protocol` or `Protocol[…]`, which adds no members and makes the class a protocol.
    Protocol,
    /// `Any`: the class may have any attribute.
    Any,
}

/// A written type, read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Form {
    Unknown,
    NoneType,
    Class(String),
    Receiver,
}

/// Builds the table for `roots` (module names).
pub(crate) fn build(loader: &mut Loader, roots: &[&str]) -> anyhow::Result<Table> {
    let mut reader = Reader {
        loader,
        reached: BTreeMap::new(),
        open: BTreeMap::new(),
    };
    let mut table = Table::default();
    let mut pending: Vec<String> = roots.iter().map(|root| (*root).to_owned()).collect();
    let mut done = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !done.insert(name.clone()) {
            continue;
        }
        let members = reader.module_members(&name)?;
        // A submodule of a covered module (`os.path`) is covered too.
        for member in members.values() {
            if let Member::Module(module) = member
                && module.starts_with(&format!("{name}."))
            {
                pending.push(module.clone());
            }
        }
        table.modules.push(TableModule { name, members });
    }
    let mut finished = BTreeSet::new();
    loop {
        let next = reader
            .reached
            .iter()
            .find(|(name, _)| !finished.contains(*name))
            .map(|(name, class)| (name.clone(), class.clone()));
        let Some((name, class)) = next else {
            break;
        };
        finished.insert(name);
        table.classes.push(reader.class(&class)?);
    }
    table.modules.sort_by(|a, b| a.name.cmp(&b.name));
    table.classes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(table)
}

struct Reader<'l> {
    loader: &'l mut Loader,
    /// Every class a member, return type, or base reaches, by qualified name.
    reached: BTreeMap<String, Rc<ClassDef>>,
    /// Whether each class read so far is open: `Any` is among its bases or theirs, so any
    /// attribute may exist on it. Open classes are left out of the table.
    open: BTreeMap<String, bool>,
}

impl Reader<'_> {
    /// The exported members of a module.
    fn module_members(&mut self, name: &str) -> anyhow::Result<BTreeMap<String, Member>> {
        let Some(module) = self.loader.module(name)? else {
            bail!("typeshed has no stub for `{name}`");
        };
        let mut members = BTreeMap::new();
        for (member, bindings) in &module.bindings {
            let listed = module.all.as_ref().is_some_and(|all| all.contains(member));
            if !listed && !bindings.iter().any(Binding::exported) {
                continue;
            }
            // Stub-only names (type aliases, type variables, `@type_check_only` classes) aren't
            // members at run time.
            let symbol = self.bindings(name, bindings, 0)?;
            let value = match symbol {
                Symbol::Class(class) if class.type_check_only => None,
                Symbol::Opaque if member.starts_with('_') && !listed => None,
                Symbol::TypeAlias(..) | Symbol::Special(_) | Symbol::Missing | Symbol::Ambiguous => None,
                symbol => self.member(symbol)?,
            };
            if let Some(value) = value {
                members.insert(member.clone(), value);
            }
        }
        Ok(members)
    }

    /// A class's bases and members.
    fn class(&mut self, class: &Rc<ClassDef>) -> anyhow::Result<TableClass> {
        let mut bases = Vec::new();
        let mut protocol = false;
        for base in &class.bases {
            match self.base(&class.module, base)? {
                Base::Class(base) => bases.extend(self.reach(&base)?),
                Base::Protocol => protocol = true,
                Base::Marker | Base::Any => {}
            }
        }
        let mut members = BTreeMap::new();
        for (name, bindings) in &class.members {
            if let Some(member) = self.class_member(class, bindings)? {
                members.insert(name.clone(), member);
            }
        }
        Ok(TableClass {
            name: class.qualified.clone(),
            bases,
            protocol,
            members,
        })
    }

    /// A base class; one written as a type alias (`_TimeTuple`) is read through it. Any base but
    /// a class, `Generic[…]`, `Protocol[…]`, or `Any` fails, or the table would hide part of the
    /// class's members.
    fn base(&mut self, module: &str, base: &Expr) -> anyhow::Result<Base> {
        self.base_at(module, base, 0)
    }

    fn base_at(&mut self, module: &str, base: &Expr, depth: usize) -> anyhow::Result<Base> {
        let target = match base {
            Expr::Subscript(subscript) => &*subscript.value,
            other => other,
        };
        match self.expression(module, target, 0)? {
            Symbol::Class(class) => Ok(Base::Class(class)),
            Symbol::TypeAlias(alias, alias_module) if depth < MAX_DEPTH => {
                self.base_at(&alias_module, &alias, depth + 1)
            }
            Symbol::Special("Generic") => Ok(Base::Marker),
            Symbol::Special("Protocol") => Ok(Base::Protocol),
            Symbol::Special("Any") => Ok(Base::Any),
            other => bail!(
                "a base class in `{module}` is not a class: {}",
                describe(base, &other)
            ),
        }
    }

    /// Whether `Any` is among a class's bases or theirs.
    fn is_open(&mut self, class: &ClassDef) -> anyhow::Result<bool> {
        if let Some(open) = self.open.get(&class.qualified) {
            return Ok(*open);
        }
        // A cycle through the class itself (never in valid stubs) counts as closed.
        self.open.insert(class.qualified.clone(), false);
        let mut open = false;
        for base in &class.bases {
            open |= match self.base(&class.module, base)? {
                Base::Any => true,
                Base::Class(base) => self.is_open(&base)?,
                Base::Marker | Base::Protocol => false,
            };
        }
        self.open.insert(class.qualified.clone(), open);
        Ok(open)
    }

    /// One member of a class body.
    fn class_member(&mut self, class: &ClassDef, bindings: &[Binding]) -> anyhow::Result<Option<Member>> {
        let functions: Vec<&Rc<FunctionDef>> = bindings
            .iter()
            .filter_map(|binding| match binding {
                Binding::Function(function) => Some(function),
                _ => None,
            })
            .collect();
        if functions.len() == bindings.len() {
            // `@x.setter` and `@x.deleter` belong to the property `x` their getter declares.
            let functions: Vec<&Rc<FunctionDef>> = functions
                .into_iter()
                .filter(|function| !function.decorated("setter") && !function.decorated("deleter"))
                .collect();
            let property = functions
                .iter()
                .any(|function| function.decorated("property") || function.decorated("cached_property"));
            if property {
                let mut forms = Vec::new();
                for function in &functions {
                    forms.push(self.annotation(&class.module, function.returns.as_ref())?);
                }
                if forms.iter().all(|form| *form == Form::Receiver) {
                    return Ok(Some(Member::SelfVariable));
                }
                return Ok(Some(Member::Variable(agree_class(&forms))));
            }
            let functions: Vec<(String, Rc<FunctionDef>)> = functions
                .into_iter()
                .map(|function| (class.module.clone(), function.clone()))
                .collect();
            return Ok(Some(Member::Function(self.returns(&functions)?)));
        }
        let symbol = self.bindings(&class.module, bindings, 0)?;
        Ok(match symbol {
            Symbol::Class(class) => Some(self.class_value(&class)?),
            Symbol::Variable(annotations) => Some(Member::Variable(self.variable(&annotations)?)),
            Symbol::TypeAlias(..) => None,
            _ => Some(Member::Variable(None)),
        })
    }

    /// The member a module-level symbol is.
    fn member(&mut self, symbol: Symbol) -> anyhow::Result<Option<Member>> {
        Ok(Some(match symbol {
            Symbol::Class(class) => self.class_value(&class)?,
            Symbol::Function(functions) => Member::Function(self.returns(&functions)?),
            Symbol::Module(module) => {
                if !self.loader.exists(&module)? {
                    return Ok(None);
                }
                Member::Module(module)
            }
            Symbol::Variable(annotations) => Member::Variable(self.variable(&annotations)?),
            Symbol::Opaque => Member::Variable(None),
            Symbol::TypeAlias(..) | Symbol::Special(_) | Symbol::Missing | Symbol::Ambiguous => {
                return Ok(None);
            }
        }))
    }

    /// A variable's class: the one every annotation agrees on.
    fn variable(&mut self, annotations: &[(String, Option<Expr>)]) -> anyhow::Result<Option<String>> {
        let mut forms = Vec::new();
        for (module, annotation) in annotations {
            forms.push(self.annotation(module, annotation.as_ref())?);
        }
        Ok(agree_class(&forms))
    }

    /// What calling the function gives: the written return type every definition agrees on.
    fn returns(&mut self, functions: &[(String, Rc<FunctionDef>)]) -> anyhow::Result<Returns> {
        let mut agreed: Option<Returns> = None;
        for (module, function) in functions {
            let form = self.annotation(module, function.returns.as_ref())?;
            let returns = match (form, function.is_async) {
                (Form::Class(class), false) => Returns::Instance(class),
                (Form::Class(class), true) => Returns::AwaitInstance(class),
                (Form::Receiver, false) => Returns::Receiver,
                (Form::Receiver, true) => Returns::AwaitReceiver,
                (Form::Unknown | Form::NoneType, _) => return Ok(Returns::Unknown),
            };
            match &agreed {
                Some(previous) if *previous != returns => return Ok(Returns::Unknown),
                _ => agreed = Some(returns),
            }
        }
        Ok(agreed.unwrap_or(Returns::Unknown))
    }

    /// Reads an annotation (absent is unknown).
    fn annotation(&mut self, module: &str, annotation: Option<&Expr>) -> anyhow::Result<Form> {
        match annotation {
            Some(annotation) => self.form(module, annotation, 0),
            None => Ok(Form::Unknown),
        }
    }

    /// Reads a written type in `module`.
    fn form(&mut self, module: &str, expr: &Expr, depth: usize) -> anyhow::Result<Form> {
        if depth > MAX_DEPTH {
            return Ok(Form::Unknown);
        }
        match expr {
            Expr::NoneLiteral(_) => Ok(Form::NoneType),
            Expr::StringLiteral(literal) => {
                match ruff_python_parser::parse_expression(literal.value.to_str()) {
                    Ok(parsed) => self.form(module, parsed.expr(), depth + 1),
                    Err(_) => Ok(Form::Unknown),
                }
            }
            Expr::BinOp(operation) if operation.op == Operator::BitOr => {
                let mut arms = Vec::new();
                union_arms(expr, &mut arms);
                let mut forms = Vec::new();
                for arm in arms {
                    forms.push(self.form(module, arm, depth + 1)?);
                }
                Ok(union(forms))
            }
            Expr::Subscript(subscript) => {
                let arguments: Vec<&Expr> = match &*subscript.slice {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                    other => vec![other],
                };
                match self.expression(module, &subscript.value, 0)? {
                    Symbol::Special("Optional") => match arguments[..] {
                        [argument] => self.form(module, argument, depth + 1),
                        _ => Ok(Form::Unknown),
                    },
                    Symbol::Special("Union") => {
                        let mut forms = Vec::new();
                        for argument in arguments {
                            forms.push(self.form(module, argument, depth + 1)?);
                        }
                        Ok(union(forms))
                    }
                    Symbol::Special(
                        "Annotated" | "ClassVar" | "Final" | "Required" | "NotRequired" | "ReadOnly",
                    ) => match arguments.first() {
                        Some(argument) => self.form(module, argument, depth + 1),
                        None => Ok(Form::Unknown),
                    },
                    // `type[X]` is the class object, not an instance.
                    Symbol::Class(class) if class.qualified == "builtins.type" => Ok(Form::Unknown),
                    Symbol::Class(class) => Ok(self.reach(&class)?.map_or(Form::Unknown, Form::Class)),
                    Symbol::TypeAlias(alias, alias_module) => self.form(&alias_module, &alias, depth + 1),
                    _ => Ok(Form::Unknown),
                }
            }
            Expr::Name(_) | Expr::Attribute(_) => match self.expression(module, expr, 0)? {
                Symbol::Class(class) => Ok(self.reach(&class)?.map_or(Form::Unknown, Form::Class)),
                Symbol::Special("Self") => Ok(Form::Receiver),
                // A literal string is a `str` (PEP 675); typeshed overloads most `str` methods on it.
                Symbol::Special("LiteralString") => match self.lookup("builtins", "str", 0)? {
                    Symbol::Class(class) => Ok(self.reach(&class)?.map_or(Form::Unknown, Form::Class)),
                    _ => Ok(Form::Unknown),
                },
                Symbol::TypeAlias(alias, alias_module) => self.form(&alias_module, &alias, depth + 1),
                _ => Ok(Form::Unknown),
            },
            _ => Ok(Form::Unknown),
        }
    }

    /// Records a reached class and returns its qualified name; `None` for an open class.
    fn reach(&mut self, class: &Rc<ClassDef>) -> anyhow::Result<Option<String>> {
        if self.is_open(class)? {
            return Ok(None);
        }
        self.reached
            .entry(class.qualified.clone())
            .or_insert_with(|| class.clone());
        Ok(Some(class.qualified.clone()))
    }

    /// The member a class is: itself, or a value of unknown type when it is open.
    fn class_value(&mut self, class: &Rc<ClassDef>) -> anyhow::Result<Member> {
        Ok(self.reach(class)?.map_or(Member::Variable(None), Member::Class))
    }

    /// What a name or dotted expression refers to in `module` (names fall back to `builtins`).
    fn expression(&mut self, module: &str, expr: &Expr, depth: usize) -> anyhow::Result<Symbol> {
        if depth > MAX_DEPTH {
            return Ok(Symbol::Missing);
        }
        match expr {
            Expr::Name(name) => {
                let found = self.lookup(module, name.id.as_str(), depth + 1)?;
                if matches!(found, Symbol::Missing) && module != "builtins" {
                    return self.lookup("builtins", name.id.as_str(), depth + 1);
                }
                Ok(found)
            }
            Expr::Attribute(attribute) => match self.expression(module, &attribute.value, depth + 1)? {
                Symbol::Module(inner) => self.lookup(&inner, attribute.attr.as_str(), depth + 1),
                Symbol::Class(class) => match class.members.get(attribute.attr.as_str()) {
                    Some(bindings) => self.bindings(&class.module, bindings, depth + 1),
                    None => Ok(Symbol::Missing),
                },
                _ => Ok(Symbol::Missing),
            },
            _ => Ok(Symbol::Opaque),
        }
    }

    /// What `name` refers to in `module`: its bindings, or its submodule.
    fn lookup(&mut self, module: &str, name: &str, depth: usize) -> anyhow::Result<Symbol> {
        if (module == "typing" || module == "typing_extensions")
            && let Some(special) = SPECIAL_FORMS.iter().find(|special| **special == name)
        {
            return Ok(Symbol::Special(special));
        }
        let Some(stub) = self.loader.module(module)? else {
            return Ok(Symbol::Missing);
        };
        match stub.bindings.get(name) {
            Some(bindings) => self.bindings(module, bindings, depth + 1),
            None => {
                let submodule = format!("{module}.{name}");
                if stub.package && self.loader.exists(&submodule)? {
                    Ok(Symbol::Module(submodule))
                } else {
                    Ok(Symbol::Missing)
                }
            }
        }
    }

    /// What a name's bindings in `module` refer to, merged.
    fn bindings(&mut self, module: &str, bindings: &[Binding], depth: usize) -> anyhow::Result<Symbol> {
        if depth > MAX_DEPTH {
            return Ok(Symbol::Missing);
        }
        let mut symbols = Vec::new();
        for binding in bindings {
            symbols.push(match binding {
                Binding::Function(function) => Symbol::Function(vec![(module.to_owned(), function.clone())]),
                Binding::Class(class) => Symbol::Class(class.clone()),
                Binding::Module { module, .. } => Symbol::Module(module.clone()),
                // `from pkg import x` names the submodule `pkg.x` when there is one (`os` binds
                // `from . import path as _path` before its own `path`).
                Binding::Import { module, name, .. } => match self.loader.module(module)? {
                    None => Symbol::Missing,
                    Some(stub) if stub.package && self.loader.exists(&format!("{module}.{name}"))? => {
                        Symbol::Module(format!("{module}.{name}"))
                    }
                    Some(_) => self.lookup(module, name, depth + 1)?,
                },
                Binding::Variable(annotation) => {
                    Symbol::Variable(vec![(module.to_owned(), annotation.clone())])
                }
                Binding::TypeAlias(expr) => Symbol::TypeAlias(expr.clone(), module.to_owned()),
                Binding::Assign(expr) => self.expression(module, expr, depth + 1)?,
                Binding::Opaque => Symbol::Opaque,
            });
        }
        Ok(merge(symbols))
    }
}

/// Several symbols for one name, merged: functions' definitions and variables' annotations are
/// combined; the same class or module stays itself; anything else disagrees.
fn merge(symbols: Vec<Symbol>) -> Symbol {
    let mut merged: Option<Symbol> = None;
    for symbol in symbols {
        merged = Some(match (merged, symbol) {
            (None, symbol) => symbol,
            (Some(Symbol::Function(mut functions)), Symbol::Function(more)) => {
                functions.extend(more);
                Symbol::Function(functions)
            }
            (Some(Symbol::Variable(mut annotations)), Symbol::Variable(more)) => {
                annotations.extend(more);
                Symbol::Variable(annotations)
            }
            (Some(Symbol::Class(a)), Symbol::Class(b)) if a.qualified == b.qualified => Symbol::Class(a),
            (Some(Symbol::Module(a)), Symbol::Module(b)) if a == b => Symbol::Module(a),
            (Some(Symbol::Special(a)), Symbol::Special(b)) if a == b => Symbol::Special(a),
            (Some(Symbol::Opaque), Symbol::Opaque) => Symbol::Opaque,
            _ => return Symbol::Ambiguous,
        });
    }
    merged.unwrap_or(Symbol::Missing)
}

/// The tables of several Python versions, intersected: the modules and classes every table has,
/// each with the members every table gives it ([`shared`]), and then only the classes the kept
/// modules still reach through members and bases. A class whose bases differ between versions
/// fails: its members would be looked up differently.
pub(crate) fn intersect(tables: Vec<Table>) -> anyhow::Result<Table> {
    let mut tables = tables.into_iter();
    let Some(first) = tables.next() else {
        return Ok(Table::default());
    };
    let rest: Vec<Table> = tables.collect();
    let mut modules = Vec::new();
    for module in first.modules {
        let theirs: Option<Vec<&BTreeMap<String, Member>>> = rest
            .iter()
            .map(|table| {
                let found = table.modules.iter().find(|other| other.name == module.name)?;
                Some(&found.members)
            })
            .collect();
        if let Some(theirs) = theirs {
            modules.push(TableModule {
                members: shared(module.members, &theirs),
                name: module.name,
            });
        }
    }
    let mut classes = BTreeMap::new();
    for class in first.classes {
        let mut theirs = Vec::new();
        for table in &rest {
            let Some(found) = table.classes.iter().find(|other| other.name == class.name) else {
                break;
            };
            if found.bases != class.bases || found.protocol != class.protocol {
                bail!("the bases of `{}` differ between Python versions", class.name);
            }
            theirs.push(&found.members);
        }
        if theirs.len() == rest.len() {
            let members = shared(class.members, &theirs);
            classes.insert(class.name.clone(), TableClass { members, ..class });
        }
    }
    // Keep the classes the modules reach, through members and bases.
    let mut pending: Vec<&str> = modules
        .iter()
        .flat_map(|module| module.members.values())
        .filter_map(Member::class)
        .collect();
    let mut reached = BTreeSet::new();
    while let Some(name) = pending.pop() {
        let Some(class) = classes.get(name) else {
            continue;
        };
        if reached.insert(name.to_owned()) {
            pending.extend(class.bases.iter().map(String::as_str));
            pending.extend(class.members.values().filter_map(Member::class));
        }
    }
    let classes = classes
        .into_values()
        .filter(|class| reached.contains(&class.name))
        .collect();
    Ok(Table { modules, classes })
}

/// The members every version has, each with the form every version agrees on; a function they
/// disagree on gives nothing known, and any other member is a variable of unknown type.
fn shared(
    members: BTreeMap<String, Member>,
    theirs: &[&BTreeMap<String, Member>],
) -> BTreeMap<String, Member> {
    members
        .into_iter()
        .filter_map(|(name, mut member)| {
            for other in theirs {
                let found = other.get(&name)?;
                if *found != member {
                    member = match (&member, found) {
                        (Member::Function(_), Member::Function(_)) => Member::Function(Returns::Unknown),
                        _ => Member::Variable(None),
                    };
                }
            }
            Some((name, member))
        })
        .collect()
}

impl Member {
    /// The class a member names or gives an instance of.
    fn class(&self) -> Option<&str> {
        match self {
            Self::Class(class)
            | Self::Variable(Some(class))
            | Self::Function(Returns::Instance(class) | Returns::AwaitInstance(class)) => Some(class),
            _ => None,
        }
    }
}

/// The arms of a `|` union, flattened.
fn union_arms<'e>(expr: &'e Expr, out: &mut Vec<&'e Expr>) {
    match expr {
        Expr::BinOp(operation) if operation.op == Operator::BitOr => {
            union_arms(&operation.left, out);
            union_arms(&operation.right, out);
        }
        other => out.push(other),
    }
}

/// A union read: one class (or `Self`) with or without `None` stays itself; anything else is
/// unknown.
fn union(forms: Vec<Form>) -> Form {
    let mut kept: Vec<Form> = forms.into_iter().filter(|form| *form != Form::NoneType).collect();
    match kept.len() {
        0 => Form::NoneType,
        1 => kept.pop().unwrap_or(Form::Unknown),
        _ => Form::Unknown,
    }
}

/// The class every form agrees on, if any.
fn agree_class(forms: &[Form]) -> Option<String> {
    let mut agreed: Option<&String> = None;
    for form in forms {
        let Form::Class(class) = form else {
            return None;
        };
        if agreed.is_some_and(|previous| previous != class) {
            return None;
        }
        agreed = Some(class);
    }
    agreed.cloned()
}

/// A base class expression and what it resolved to, for error messages.
fn describe(base: &Expr, symbol: &Symbol) -> String {
    let written = last_name(base).unwrap_or("an expression");
    let kind = match symbol {
        Symbol::Class(_) => "a class",
        Symbol::Function(..) => "a function",
        Symbol::Module(_) => "a module",
        Symbol::Variable(..) => "a variable",
        Symbol::TypeAlias(..) => "a type alias",
        Symbol::Special(_) => "a special form",
        Symbol::Opaque => "a value",
        Symbol::Missing => "not found",
        Symbol::Ambiguous => "bound to different things",
    };
    format!("`{written}` is {kind}")
}
