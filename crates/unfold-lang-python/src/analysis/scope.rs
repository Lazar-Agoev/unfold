//! A per-file write census and the identities it proves: which import, builtin, or local
//! declaration a name refers to, by Python's scoping rules (LEGB, class scopes invisible to
//! nested functions, `global` and `nonlocal` redirection).
//!
//! Lookups are flow-insensitive, like the proof of concept's binder: every write anywhere in a
//! scope counts, whether it comes before or after the use. A name has a proven *identity* only
//! when its binding is a single import (`typing.TYPE_CHECKING`, `importlib.import_module`), or
//! when nothing in the file binds it (`builtins.property`).

use ruff_python_ast::StmtFunctionDef;
use rustc_hash::{FxHashMap, FxHashSet};

/// What kind of scope a scope is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    /// The module.
    Module,
    /// A class body: visible only to code directly in it.
    Class,
    /// A function or lambda body.
    Function,
    /// A comprehension or generator expression.
    Comprehension,
    /// The annotation scope PEP 695 type parameters live in.
    TypeParameters,
}

/// Where an import binding comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin<'a> {
    /// `import a.b` binds `a` to module `a`; `import a.b as c` binds `c` to module `a.b`.
    Module(&'a str),
    /// `from m import n` (absolute) binds `n` to member `n` of module `m`.
    Member(&'a str, &'a str),
    /// A relative `from` import: its target is a local module, never an external identity.
    Relative,
}

/// One write to a name.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Write<'a> {
    /// An import binding.
    Import(Origin<'a>),
    /// A `def` statement.
    Def(&'a StmtFunctionDef),
    /// Any other binding: assignment, deletion, parameter, loop target, class, ...
    Other,
}

/// A written name in a scope, before `global` and `nonlocal` redirection.
#[derive(Debug, Clone, Copy)]
struct RawWrite<'a> {
    scope: u32,
    name: &'a str,
    write: Write<'a>,
}

#[derive(Debug)]
struct Scope<'a> {
    kind: ScopeKind,
    parent: Option<u32>,
    globals: FxHashSet<&'a str>,
    nonlocals: FxHashSet<&'a str>,
}

/// Every write to one name in one scope.
#[derive(Debug, Default)]
pub(crate) struct Binding<'a> {
    /// All writes, in no particular order.
    pub(crate) writes: Vec<Write<'a>>,
}

/// The outcome of looking a name up.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Lookup<'b, 'a> {
    /// A scope binds it.
    Bound(&'b Binding<'a>),
    /// Nothing in the file binds it: a builtin, if anything.
    Unbound,
}

/// Scopes and writes, collected during a walk and resolved once at the end.
#[derive(Debug)]
pub(crate) struct Census<'a> {
    scopes: Vec<Scope<'a>>,
    raw: Vec<RawWrite<'a>>,
    /// Each scope's bindings, by scope index (filled by [`Census::resolve`]).
    bindings: Vec<FxHashMap<&'a str, Binding<'a>>>,
}

/// The module scope's index.
pub(crate) const MODULE: u32 = 0;

impl<'a> Census<'a> {
    /// A census with only the module scope.
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![Scope {
                kind: ScopeKind::Module,
                parent: None,
                globals: FxHashSet::default(),
                nonlocals: FxHashSet::default(),
            }],
            raw: Vec::new(),
            bindings: Vec::new(),
        }
    }

    /// Opens a scope nested in `parent` and returns its index.
    pub(crate) fn push(&mut self, kind: ScopeKind, parent: u32) -> u32 {
        let index = self.scopes.len() as u32;
        self.scopes.push(Scope {
            kind,
            parent: Some(parent),
            globals: FxHashSet::default(),
            nonlocals: FxHashSet::default(),
        });
        index
    }

    /// The kind of a scope.
    pub(crate) fn kind(&self, scope: u32) -> ScopeKind {
        self.scopes[scope as usize].kind
    }

    /// The nearest scope at or above `scope` that is not a comprehension: where an assignment
    /// expression binds its target.
    pub(crate) fn walrus_target(&self, scope: u32) -> u32 {
        let mut current = scope;
        while self.kind(current) == ScopeKind::Comprehension {
            match self.scopes[current as usize].parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        current
    }

    /// Records a write of `name` in `scope`.
    pub(crate) fn write(&mut self, scope: u32, name: &'a str, write: Write<'a>) {
        self.raw.push(RawWrite { scope, name, write });
    }

    /// Records `global name` in `scope`.
    pub(crate) fn declare_global(&mut self, scope: u32, name: &'a str) {
        self.scopes[scope as usize].globals.insert(name);
    }

    /// Records `nonlocal name` in `scope`.
    pub(crate) fn declare_nonlocal(&mut self, scope: u32, name: &'a str) {
        self.scopes[scope as usize].nonlocals.insert(name);
    }

    /// Resolves every write to the scope it binds in. Call once, after the walk.
    pub(crate) fn resolve(&mut self) {
        let mut locals: FxHashSet<(u32, &'a str)> = FxHashSet::default();
        for raw in &self.raw {
            let scope = &self.scopes[raw.scope as usize];
            if !scope.globals.contains(raw.name) && !scope.nonlocals.contains(raw.name) {
                locals.insert((raw.scope, raw.name));
            }
        }
        self.bindings = (0..self.scopes.len()).map(|_| FxHashMap::default()).collect();
        for raw in std::mem::take(&mut self.raw) {
            let target = self.binding_scope(raw.scope, raw.name, &locals);
            if let Some(scope) = self.bindings.get_mut(target as usize) {
                scope.entry(raw.name).or_default().writes.push(raw.write);
            }
        }
    }

    /// The scope a write of `name` in `scope` binds in.
    fn binding_scope(&self, scope: u32, name: &str, locals: &FxHashSet<(u32, &'a str)>) -> u32 {
        let declared = &self.scopes[scope as usize];
        if declared.globals.contains(name) {
            return MODULE;
        }
        if !declared.nonlocals.contains(name) {
            return scope;
        }
        // `nonlocal` binds in the nearest enclosing function scope that binds the name.
        let mut current = declared.parent;
        let mut nearest_function = None;
        while let Some(index) = current {
            let candidate = &self.scopes[index as usize];
            if matches!(candidate.kind, ScopeKind::Function) {
                nearest_function.get_or_insert(index);
                if locals.contains(&(index, name)) {
                    return index;
                }
            }
            current = candidate.parent;
        }
        nearest_function.unwrap_or(scope)
    }

    /// Looks `name` up from `scope`: the scope itself, then enclosing function scopes, then the
    /// module. Class scopes are visible only to code directly in them, or through PEP 695
    /// type-parameter scopes alone.
    pub(crate) fn lookup(&self, scope: u32, name: &str) -> Lookup<'_, 'a> {
        let declared = &self.scopes[scope as usize];
        if declared.globals.contains(name) {
            return self.binding(MODULE, name);
        }
        // Whether every scope left so far is a type-parameter scope.
        let mut direct = true;
        let mut current = Some(scope);
        while let Some(index) = current {
            let candidate = &self.scopes[index as usize];
            let visible = index == scope || direct || candidate.kind != ScopeKind::Class;
            direct &= candidate.kind == ScopeKind::TypeParameters;
            if visible
                && let Some(binding) = self
                    .bindings
                    .get(index as usize)
                    .and_then(|scope| scope.get(name))
            {
                return Lookup::Bound(binding);
            }
            current = candidate.parent;
        }
        Lookup::Unbound
    }

    fn binding(&self, scope: u32, name: &str) -> Lookup<'_, 'a> {
        match self
            .bindings
            .get(scope as usize)
            .and_then(|bindings| bindings.get(name))
        {
            Some(binding) => Lookup::Bound(binding),
            None => Lookup::Unbound,
        }
    }
}
