//! The end of the binder pass: interning the queued expressions, redirecting writes by `global`
//! and `nonlocal`, dropping type names bound to type parameters, and assembling the facts.

use rustc_hash::FxHashMap;
use unfold_engine::facts::TypeRefFact;

use crate::mapping::bytes;

use super::build::{Builder, Built};
use super::ir::{BindFacts, BindingFact, MODULE_SCOPE, ScopeFact, ScopeId, ScopeKind};

impl<'a> Builder<'a> {
    /// Interns the queued expressions, resolves redirection, and assembles the facts.
    pub(super) fn finish(mut self) -> Built {
        self.interner.fill();
        let values = self
            .inputs
            .values
            .iter()
            .map(|&expr| self.interner.find(expr))
            .collect();
        let mut facts = BindFacts {
            values,
            stars: std::mem::take(&mut self.stars),
            effects: std::mem::take(&mut self.effects),
            nodes: std::mem::take(&mut self.nodes),
            ..BindFacts::default()
        };
        let mut bindings: FxHashMap<(ScopeId, &'a str), u32> = FxHashMap::default();
        let writes = std::mem::take(&mut self.writes);
        for raw in writes {
            let scope = self.write_scope(raw.scope, raw.name, raw.walrus);
            let index = facts.writes.len() as u32;
            facts.writes.push(raw.write);
            let next = facts.bindings.len() as u32;
            let binding = *bindings.entry((scope, raw.name)).or_insert(next);
            if binding == next {
                facts.bindings.push(BindingFact {
                    scope,
                    name: self.interner.name(raw.name),
                    writes: Vec::new(),
                });
            }
            facts.bindings[binding as usize].writes.push(index);
        }
        facts.scopes = (0..self.scopes.len())
            .map(|index| {
                let scope = &self.scopes[index];
                let globals = scope.globals.clone();
                let nonlocals = scope.nonlocals.clone();
                let (kind, parent) = (scope.kind, scope.parent);
                ScopeFact {
                    kind,
                    parent,
                    globals: globals.iter().map(|name| self.interner.name(name)).collect(),
                    nonlocals: nonlocals
                        .iter()
                        .filter_map(|name| {
                            let target = self.nonlocal_target(index as ScopeId, name)?;
                            Some((self.interner.name(name), target))
                        })
                        .collect(),
                }
            })
            .collect();
        let mut types: Vec<(TypeRefFact, u32)> = std::mem::take(&mut self.types)
            .into_iter()
            .filter(|name| !self.type_parameter(name.scope, &name.root))
            .map(|name| {
                let range = bytes(name.range);
                let text = self
                    .inputs
                    .text
                    .get(range.start as usize..range.end as usize)
                    .unwrap_or_default();
                (TypeRefFact::new(range, text, name.qualified), name.expr)
            })
            .collect();
        types.sort_by_key(|(reference, _)| reference.range);
        let (type_refs, type_exprs) = types.into_iter().unzip();
        facts.types = type_exprs;
        let (names, exprs) = self.interner.finish();
        facts.names = names;
        facts.exprs = exprs;
        Built {
            facts,
            aliases: self.aliases,
            type_refs,
            awaited: self.awaited,
        }
    }

    /// Where a write of `name` lexically in `scope` binds: an assignment expression skips
    /// comprehension scopes; `global` binds in the module; `nonlocal` in the nearest enclosing
    /// function scope that writes the name.
    pub(super) fn write_scope(&self, mut scope: ScopeId, name: &str, walrus: bool) -> ScopeId {
        if walrus {
            while self.scopes[scope as usize].kind == ScopeKind::Comprehension {
                match self.scopes[scope as usize].parent {
                    Some(parent) => scope = parent,
                    None => break,
                }
            }
        }
        let declared = &self.scopes[scope as usize];
        if declared.globals.contains(&name) {
            return MODULE_SCOPE;
        }
        if declared.nonlocals.contains(&name) {
            return self.nonlocal_target(scope, name).unwrap_or(scope);
        }
        scope
    }

    /// The scope `nonlocal name` in `scope` binds in: the nearest enclosing function where the
    /// name is local (written there and not itself declared `nonlocal` or `global`, which would
    /// pass the name on outwards).
    pub(super) fn nonlocal_target(&self, scope: ScopeId, name: &str) -> Option<ScopeId> {
        let mut current = self.scopes[scope as usize].parent;
        while let Some(index) = current {
            let outer = &self.scopes[index as usize];
            if outer.kind == ScopeKind::Function
                && outer.lexical.contains(name)
                && !outer.nonlocals.contains(&name)
                && !outer.globals.contains(&name)
            {
                return Some(index);
            }
            current = outer.parent;
        }
        None
    }
}
