//! Finishing a file's binding: filling module declarations the walk never reached, looking every
//! recorded name up, dropping the type names that bind to a type parameter, finding factory
//! callees in the value table, dropping the expressions no root reaches, and assembling the
//! payload's tables.

use rustc_hash::FxHashMap;

use super::ir;
use super::scope::{ScopeKind, Scopes};
use super::{AliasDraft, Binder, Input, KeyDraft, RootDraft, TypeSite};
use crate::payload::DeclarationKind;
use crate::payload::binding::{
    Alias, Binding, ClassFacts, Decl, DeclFlags, DeclKind, Expr, Key, KeyRoot, Lookup, Meanings, NamePath,
    Namespace, Narrowing, NarrowingCall, Symbol, TypeRef,
};
use crate::values::Values;

impl<'a> Binder<'a> {
    /// Fills the slot of every module declaration the walk never reached (none in practice),
    /// so indices stay aligned.
    fn fill_unreached(&mut self, input: &Input<'_, 'a>) {
        for (index, slot) in self.decls.iter_mut().enumerate() {
            if slot.is_some() {
                continue;
            }
            let Some(module) = input.module.declarations.get(index) else {
                continue;
            };
            let kind = match module.kind {
                DeclarationKind::Module => DeclKind::Module,
                DeclarationKind::Expression => DeclKind::Expression,
                DeclarationKind::Function => DeclKind::Function,
                DeclarationKind::Class => DeclKind::Class,
                DeclarationKind::Enum => DeclKind::Enum,
                DeclarationKind::Namespace { .. } => DeclKind::Namespace,
                DeclarationKind::Interface => DeclKind::Interface,
                DeclarationKind::TypeAlias => DeclKind::TypeAlias,
                DeclarationKind::Var
                | DeclarationKind::Let
                | DeclarationKind::Const
                | DeclarationKind::Using => DeclKind::Variable,
            };
            *slot = Some(Decl {
                kind,
                name: (!module.name.is_empty()).then(|| module.name.clone()),
                flags: DeclFlags::default().with(DeclFlags::AMBIENT, module.ambient),
                range: module.range,
                block: (kind == DeclKind::Module).then_some(0),
                container: 0,
                owner: None,
                symbol: None,
                var_kind: None,
                initializer: None,
                ty: None,
                returns: None,
                inline: None,
                object: None,
                constructor: None,
                factory: None,
            });
        }
    }

    /// Looks every recorded name up, now that every declaration is known.
    fn resolve_names(&mut self) {
        let lookups: Vec<(u32, Lookup)> = self
            .name_places
            .iter()
            .filter_map(|&(index, place)| match &self.exprs[index as usize] {
                Expr::Name { name, .. } => Some((index, self.lookup(name, place, Meanings::VALUE))),
                _ => None,
            })
            .collect();
        for (index, found) in lookups {
            if let Expr::Name { lookup, .. } = &mut self.exprs[index as usize] {
                *lookup = found;
            }
        }
        let types: Vec<(u32, Lookup, bool)> = self
            .type_places
            .iter()
            .filter_map(|&(index, scope)| match &self.types[index as usize] {
                TypeRef::Names { names, value, .. } => {
                    let first = names.first()?;
                    let meaning = if *value { Meanings::VALUE } else { Meanings::TYPE };
                    let lookup = self.scopes.lookup(first, scope, meaning, &[], self);
                    Some((
                        index,
                        self.namespaced(lookup, scope),
                        self.scopes.visible(first, scope),
                    ))
                }
                _ => None,
            })
            .collect();
        for (index, found, visible) in types {
            if let TypeRef::Names { lookup, shadowed, .. } = &mut self.types[index as usize] {
                *lookup = found;
                *shadowed = visible;
            }
        }
    }

    /// The recorded type names in source order, without those whose first name binds to a
    /// type parameter (of an enclosing callable or class).
    fn type_sites(&mut self) -> Vec<TypeSite> {
        let mut sites = std::mem::take(&mut self.type_sites);
        sites.retain(|site| {
            let Some(TypeRef::Names {
                lookup: Lookup::Symbol(symbol),
                ..
            }) = self.types.get(site.ty as usize)
            else {
                return true;
            };
            let Some(symbol) = self.scopes.symbols.get(*symbol as usize) else {
                return true;
            };
            !symbol.declarations.iter().any(|&declaration| {
                self.decl(declaration)
                    .is_some_and(|decl| decl.kind == DeclKind::TypeParameter)
            })
        });
        sites.sort_by_key(|site| (site.range.start, site.range.end));
        sites.dedup_by_key(|site| site.range);
        sites
    }

    /// A narrowing key; `None` inside a `with` statement, where nothing is known.
    fn key(&self, draft: &KeyDraft<'_>) -> Option<Key> {
        let root = match &draft.root {
            RootDraft::Name(name, place) => match self.lookup(name, *place, Meanings::VALUE) {
                Lookup::Symbol(symbol) => KeyRoot::Symbol(symbol),
                Lookup::Dynamic => return None,
                lookup => KeyRoot::Name((*name).into(), lookup),
            },
            RootDraft::This(class) => KeyRoot::This(*class),
        };
        Some(Key {
            root,
            path: draft.path.clone(),
        })
    }

    /// Converts everything recorded into the payload's form, with the type names of linked
    /// types (aligned with the binding's `type_links`).
    pub(super) fn finish(mut self, input: &Input<'_, 'a>, values: &Values) -> (Binding, Vec<TypeSite>) {
        self.fill_unreached(input);
        self.resolve_names();
        let sites = self.type_sites();
        let narrowing = Narrowing {
            instanceof: self
                .instanceof
                .iter()
                .filter_map(|draft| self.key(draft))
                .collect(),
            calls: self
                .calls
                .iter()
                .map(|(callee, condition, keys)| NarrowingCall {
                    callee: callee.as_ref().map(|path| NamePath {
                        first: path.first.into(),
                        lookup: self.lookup(path.first, path.place, Meanings::VALUE),
                        rest: path.rest.clone(),
                    }),
                    condition: *condition,
                    keys: keys.iter().filter_map(|draft| self.key(draft)).collect(),
                })
                .collect(),
        };
        let aliases = self
            .aliases
            .iter()
            .map(|alias| match alias {
                AliasDraft::Qualified {
                    first,
                    place,
                    rest,
                    type_only,
                } => Alias::Qualified {
                    first: (*first).into(),
                    lookup: self.lookup(first, *place, Meanings::VALUE),
                    rest: rest.clone(),
                    type_only: *type_only,
                },
                AliasDraft::Dynamic { specifier, imported } => Alias::Dynamic {
                    specifier: specifier.clone(),
                    imported: imported.clone(),
                },
                AliasDraft::Unsupported => Alias::Unsupported,
            })
            .collect();
        for &(decl, callee) in &self.factories {
            let Some(value) = values.index_of(callee) else {
                continue;
            };
            if let Some(Some(found)) = self.decls.get_mut(decl as usize) {
                found.factory = Some(value);
            }
        }
        let mut value_exprs: Vec<Option<u32>> = values
            .addresses()
            .iter()
            .map(|address| address.and_then(|address| self.memo.get(&address).copied()))
            .collect();
        let mut declarations: Vec<Decl> = std::mem::take(&mut self.decls).into_iter().flatten().collect();
        let mut classes: Vec<ClassFacts> = self
            .classes
            .iter()
            .map(|class| ClassFacts {
                decl: class.decl,
                heritage: class.heritage,
                members: class.members.clone(),
            })
            .collect();
        let mut writes = std::mem::take(&mut self.writes);
        let exprs = ir::prune(
            std::mem::take(&mut self.exprs),
            &mut value_exprs,
            &mut declarations,
            &mut classes,
            &mut writes,
        );
        let mut exprs = exprs;
        let value_exprs = value_exprs
            .into_iter()
            .map(|found| {
                found.unwrap_or_else(|| {
                    exprs.push(Expr::Other);
                    (exprs.len() - 1) as u32
                })
            })
            .collect();
        let mut namespace_ids: FxHashMap<u32, u32> = FxHashMap::default();
        let mut namespaces = Vec::new();
        let mut symbols = Vec::with_capacity(self.scopes.symbols.len());
        for (index, draft) in self.scopes.symbols.iter().enumerate() {
            let namespace = draft.namespace.map(|scope| {
                *namespace_ids.entry(scope).or_insert_with(|| {
                    namespaces.push(Namespace {
                        members: self.scopes.members(scope),
                    });
                    (namespaces.len() - 1) as u32
                })
            });
            symbols.push(Symbol {
                name: draft.name.into(),
                meanings: draft.meanings,
                declarations: draft.declarations.clone(),
                namespace,
                alias: self.alias_refs.get(&(index as u32)).copied(),
            });
        }
        let file_symbols = symbols_in(&self.scopes, |kind| kind == ScopeKind::File);
        let global_symbols = symbols_in(&self.scopes, |kind| kind == ScopeKind::Global);
        let binding = Binding {
            declarations,
            symbols,
            namespaces,
            aliases,
            exprs,
            value_exprs,
            contexts: std::mem::take(&mut self.contexts),
            types: std::mem::take(&mut self.types),
            type_links: sites.iter().map(|site| site.ty).collect(),
            classes,
            objects: std::mem::take(&mut self.objects),
            file_symbols,
            global_symbols,
            namespace_chains: self.namespace_chains.take(),
            narrowing,
            writes,
        };
        (binding, sites)
    }
}

/// The symbols first declared in a scope of a kind `wanted` accepts, in symbol order.
fn symbols_in(scopes: &Scopes<'_>, wanted: impl Fn(ScopeKind) -> bool) -> Vec<u32> {
    scopes
        .symbols
        .iter()
        .enumerate()
        .filter(|(_, symbol)| wanted(scopes.scope(symbol.scope).kind))
        .map(|(index, _)| index as u32)
        .collect()
}
