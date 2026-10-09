//! Scopes and symbols of one file, with the proof of concept's declaration and lookup rules
//! (`frontend/native/names.ts`, adapted from TypeScript's binder and `createNameResolver`).
//!
//! - Every declaration of a meaning (value or type) merges into one symbol per name and scope;
//!   overloads, namespace and enum merging, and interface merging all fall out of this.
//! - An exported namespace member is also entered in the namespace's shared member scope, which
//!   every declaration of the namespace shares, while each declaration keeps its own locals.
//! - Lookup walks the scope chain with a meaning. A `with` statement stops it (no static
//!   answer). A reference inside a parameter list whose callable keeps parameter evaluation
//!   separate from its body skips the body's variable declarations.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use unfold_core::text::ByteRange;

use crate::payload::binding::{Lookup, Meanings};

/// What kind of container a scope is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScopeKind {
    /// The file.
    File,
    /// A `declare global { … }` block: the configuration group's global scope.
    Global,
    /// A function, method, constructor, or accessor (rebinds `this`).
    Function,
    /// An arrow function.
    Arrow,
    /// A class static block.
    StaticBlock,
    /// A block statement.
    Block,
    /// A `for`, `for…in`, or `for…of` loop.
    Loop,
    /// A catch clause.
    Catch,
    /// A switch statement's case block.
    Switch,
    /// A class body.
    Class,
    /// A namespace declaration (each declaration has its own; members are shared).
    Namespace,
    /// An enum (merged declarations share one).
    Enum,
    /// A `with` statement's body.
    With,
}

impl ScopeKind {
    /// Whether `var` declarations bind here.
    const fn holds_vars(self) -> bool {
        matches!(
            self,
            Self::Function | Self::Arrow | Self::StaticBlock | Self::File | Self::Namespace | Self::Global
        )
    }
}

/// One scope.
#[derive(Debug)]
pub(super) struct Scope<'a> {
    pub(super) kind: ScopeKind,
    pub(super) parent: Option<u32>,
    values: FxHashMap<&'a str, u32>,
    types: FxHashMap<&'a str, u32>,
    /// For a namespace declaration's scope, the shared scope its exported members go to.
    pub(super) export: Option<u32>,
    /// For a callable's scope, its body: the declarations a parameter reference may skip.
    pub(super) body: Option<ByteRange>,
    /// For a namespace's shared member scope, the namespace's symbol.
    pub(super) namespace: Option<u32>,
}

/// One symbol while binding.
#[derive(Debug)]
pub(super) struct SymbolDraft<'a> {
    pub(super) name: &'a str,
    pub(super) meanings: Meanings,
    pub(super) declarations: Vec<u32>,
    /// The scope it was first declared in.
    pub(super) scope: u32,
    /// The shared member scope of a namespace or enum.
    pub(super) namespace: Option<u32>,
}

/// What lookup needs to know about a declaration: whether it is a variable (or binding
/// element) and where it is written.
pub(super) trait DeclarationSpans {
    /// The range of a variable or binding element declaration; `None` for other kinds.
    fn variable_range(&self, declaration: u32) -> Option<ByteRange>;
}

/// Every scope and symbol of a file.
#[derive(Debug, Default)]
pub(super) struct Scopes<'a> {
    pub(super) scopes: Vec<Scope<'a>>,
    pub(super) symbols: Vec<SymbolDraft<'a>>,
}

impl<'a> Scopes<'a> {
    /// Adds a scope.
    pub(super) fn push(&mut self, kind: ScopeKind, parent: Option<u32>) -> u32 {
        let id = self.scopes.len() as u32;
        self.scopes.push(Scope {
            kind,
            parent,
            values: FxHashMap::default(),
            types: FxHashMap::default(),
            export: None,
            body: None,
            namespace: None,
        });
        id
    }

    /// The namespace symbols whose shared member scopes are on `scope`'s chain, innermost
    /// first.
    pub(super) fn namespaces_crossed(&self, scope: u32) -> Vec<u32> {
        let mut crossed = Vec::new();
        let mut current = Some(scope);
        while let Some(id) = current {
            let found = self.scope(id);
            if let Some(symbol) = found.namespace {
                crossed.push(symbol);
            }
            current = found.parent;
        }
        crossed
    }

    pub(super) fn scope(&self, id: u32) -> &Scope<'a> {
        &self.scopes[id as usize]
    }

    pub(super) fn scope_mut(&mut self, id: u32) -> &mut Scope<'a> {
        &mut self.scopes[id as usize]
    }

    /// The nearest scope at or above `scope` where `var` declarations bind.
    pub(super) fn var_scope(&self, mut scope: u32) -> u32 {
        loop {
            let found = self.scope(scope);
            match found.parent {
                Some(parent) if !found.kind.holds_vars() => scope = parent,
                _ => return scope,
            }
        }
    }

    fn table(&self, scope: u32, meaning: Meanings) -> &FxHashMap<&'a str, u32> {
        let scope = self.scope(scope);
        if meaning == Meanings::VALUE {
            &scope.values
        } else {
            &scope.types
        }
    }

    /// Declares `name` with `meanings` in `scope`, merging into an existing symbol of either
    /// meaning (in the scope, or, for an exported declaration, in its shared member scope).
    /// Returns the symbol.
    pub(super) fn declare(
        &mut self,
        scope: u32,
        name: &'a str,
        declaration: u32,
        meanings: Meanings,
        exported: bool,
    ) -> u32 {
        let shared = if exported { self.scope(scope).export } else { None };
        let each = [Meanings::VALUE, Meanings::TYPE];
        let mut symbol = None;
        for meaning in each.into_iter().filter(|meaning| meanings.has(*meaning)) {
            let found = self
                .table(scope, meaning)
                .get(name)
                .copied()
                .or_else(|| shared.and_then(|shared| self.table(shared, meaning).get(name).copied()));
            if found.is_some() {
                symbol = found;
                break;
            }
        }
        let symbol = symbol.unwrap_or_else(|| {
            let id = self.symbols.len() as u32;
            self.symbols.push(SymbolDraft {
                name,
                meanings: Meanings::default(),
                declarations: Vec::new(),
                scope,
                namespace: None,
            });
            id
        });
        let draft = &mut self.symbols[symbol as usize];
        if !draft.declarations.contains(&declaration) {
            draft.declarations.push(declaration);
        }
        draft.meanings = Meanings(draft.meanings.0 | meanings.0);
        for meaning in each.into_iter().filter(|meaning| meanings.has(*meaning)) {
            for target in std::iter::once(scope).chain(shared) {
                let found = self.scope_mut(target);
                let table = if meaning == Meanings::VALUE {
                    &mut found.values
                } else {
                    &mut found.types
                };
                table.insert(name, symbol);
            }
        }
        symbol
    }

    /// Enters `symbol` under `name` in `scope`'s tables for `meanings` without declaring
    /// anything (a class's own name inside its body).
    pub(super) fn alias(&mut self, scope: u32, name: &'a str, symbol: u32, meanings: Meanings) {
        let found = self.scope_mut(scope);
        if meanings.has(Meanings::VALUE) {
            found.values.insert(name, symbol);
        }
        if meanings.has(Meanings::TYPE) {
            found.types.insert(name, symbol);
        }
    }

    /// Looks `name` up from `scope` in `meaning`. `parameters` are the callable scopes whose
    /// parameter lists the reference is in (when they keep parameter evaluation separate).
    pub(super) fn lookup(
        &self,
        name: &str,
        scope: u32,
        meaning: Meanings,
        parameters: &[u32],
        declarations: &dyn DeclarationSpans,
    ) -> Lookup {
        let mut current = Some(scope);
        while let Some(id) = current {
            let found = self.scope(id);
            if found.kind == ScopeKind::With {
                return Lookup::Dynamic;
            }
            if let Some(&symbol) = self.table(id, meaning).get(name) {
                let skipped = parameters.contains(&id)
                    && found.body.is_some_and(|body| {
                        self.symbols[symbol as usize]
                            .declarations
                            .first()
                            .and_then(|&first| declarations.variable_range(first))
                            .is_some_and(|range| range.start >= body.start && range.end <= body.end)
                    });
                if !skipped {
                    return Lookup::Symbol(symbol);
                }
            }
            current = found.parent;
        }
        Lookup::Unresolved
    }

    /// Whether `name` is declared in either meaning anywhere on `scope`'s chain.
    pub(super) fn visible(&self, name: &str, scope: u32) -> bool {
        let mut current = Some(scope);
        while let Some(id) = current {
            let found = self.scope(id);
            if found.kind == ScopeKind::With {
                return false;
            }
            if found.values.contains_key(name) || found.types.contains_key(name) {
                return true;
            }
            current = found.parent;
        }
        false
    }

    /// The members of a shared namespace or enum scope: every name in either table, sorted.
    pub(super) fn members(&self, scope: u32) -> Vec<(Box<str>, u32)> {
        let found = self.scope(scope);
        let mut members: SmallVec<[(&str, u32); 16]> = found
            .values
            .iter()
            .chain(found.types.iter())
            .map(|(&name, &symbol)| (name, symbol))
            .collect();
        members.sort_unstable();
        members.dedup_by(|a, b| a.0 == b.0);
        members
            .into_iter()
            .map(|(name, symbol)| (Box::from(name), symbol))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Declarations 0 and 1 are variables inside `0..100`; everything else is not a variable.
    struct Spans;

    impl DeclarationSpans for Spans {
        fn variable_range(&self, declaration: u32) -> Option<ByteRange> {
            (declaration < 2).then_some(ByteRange { start: 10, end: 20 })
        }
    }

    #[test]
    fn declarations_merge_by_meaning() {
        let mut scopes = Scopes::default();
        let file = scopes.push(ScopeKind::File, None);
        let interface = scopes.declare(file, "IO", 5, Meanings::TYPE, false);
        let value = scopes.declare(file, "IO", 6, Meanings::VALUE, false);
        // An interface and a later value stay two symbols; a class merges into both tables.
        assert_ne!(interface, value);
        let class = scopes.declare(file, "C", 7, Meanings::BOTH, false);
        assert_eq!(scopes.declare(file, "C", 8, Meanings::TYPE, false), class);
        assert_eq!(scopes.symbols[class as usize].declarations, [7, 8]);
        assert_eq!(
            scopes.lookup("IO", file, Meanings::TYPE, &[], &Spans),
            Lookup::Symbol(interface)
        );
        assert_eq!(
            scopes.lookup("IO", file, Meanings::VALUE, &[], &Spans),
            Lookup::Symbol(value)
        );
        assert_eq!(
            scopes.lookup("missing", file, Meanings::VALUE, &[], &Spans),
            Lookup::Unresolved
        );
    }

    #[test]
    fn exported_namespace_members_are_shared() {
        let mut scopes = Scopes::default();
        let file = scopes.push(ScopeKind::File, None);
        let shared = scopes.push(ScopeKind::Namespace, Some(file));
        let first = scopes.push(ScopeKind::Namespace, Some(shared));
        let second = scopes.push(ScopeKind::Namespace, Some(shared));
        for scope in [first, second] {
            scopes.scope_mut(scope).export = Some(shared);
        }
        let exported = scopes.declare(first, "f", 3, Meanings::VALUE, true);
        let hidden = scopes.declare(first, "hidden", 4, Meanings::VALUE, false);
        assert_eq!(
            scopes.lookup("f", second, Meanings::VALUE, &[], &Spans),
            Lookup::Symbol(exported)
        );
        assert_eq!(
            scopes.lookup("hidden", first, Meanings::VALUE, &[], &Spans),
            Lookup::Symbol(hidden)
        );
        assert_eq!(
            scopes.lookup("hidden", second, Meanings::VALUE, &[], &Spans),
            Lookup::Unresolved
        );
        assert_eq!(scopes.members(shared), [(Box::from("f"), exported)]);
    }

    #[test]
    fn parameters_skip_body_variables_and_with_stops_lookup() {
        let mut scopes = Scopes::default();
        let file = scopes.push(ScopeKind::File, None);
        let outer = scopes.declare(file, "x", 9, Meanings::VALUE, false);
        let function = scopes.push(ScopeKind::Function, Some(file));
        scopes.scope_mut(function).body = Some(ByteRange { start: 0, end: 100 });
        let inner = scopes.declare(function, "x", 0, Meanings::VALUE, false);
        assert_eq!(
            scopes.lookup("x", function, Meanings::VALUE, &[], &Spans),
            Lookup::Symbol(inner)
        );
        assert_eq!(
            scopes.lookup("x", function, Meanings::VALUE, &[function], &Spans),
            Lookup::Symbol(outer)
        );
        let with = scopes.push(ScopeKind::With, Some(function));
        assert_eq!(
            scopes.lookup("x", with, Meanings::VALUE, &[], &Spans),
            Lookup::Dynamic
        );
        let block = scopes.push(ScopeKind::Block, Some(function));
        assert_eq!(scopes.var_scope(block), function);
    }
}
