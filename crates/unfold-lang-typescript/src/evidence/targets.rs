//! What names, members, and expressions denote (the proof of concept's `expressionTarget`,
//! `modules.resolve`, and `modules.member`).
//!
//! | Expression | Target |
//! | --- | --- |
//! | Identifier | Its symbol's declarations; an import's target through the export tables; a script global of the configuration group; an ECMAScript global (a builtin) or another known library global (external); otherwise nothing |
//! | Property or literal-key access | A private name's members; else a method of the receiver's ECMAScript type (a builtin); the receiver's class member (through bases); an external receiver's member; a `const` object literal's function member (C-2); else the receiver's namespace member |
//! | `super` | The written base class |
//! | `this` in a static member | The class |
//! | Inline function or class | Itself |
//! | Anything else | Nothing |

use std::borrow::Cow;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use unfold_engine::evidence::ExternalLocation;
use unfold_engine::{DeclRef, FileId};

use super::{GlobalSymbol, MergedNamespace, Namespace, Target, Walk};
use crate::exports::ExportTarget;
use crate::link::TypeScriptLinked;
use crate::modules::{Resolved, SpecifierKind};
use crate::payload::ImportSource;
use crate::payload::binding::{
    Alias, AliasRef, Binding, Decl, DeclFlags, DeclKind, Expr, Lookup, Meanings, VarKind,
};
use crate::tables;

impl TypeScriptLinked<'_> {
    /// A TypeScript file's binding.
    pub(crate) fn binding(&self, file: FileId) -> Option<&Binding> {
        Some(&self.payload(file)?.binding)
    }

    /// A declaration's facts.
    pub(crate) fn decl(&self, declaration: DeclRef) -> Option<&Decl> {
        self.binding(declaration.file)?
            .declarations
            .get(declaration.index as usize)
    }

    /// An expression of a file.
    pub(crate) fn expr(&self, file: FileId, index: u32) -> Option<&Expr> {
        self.binding(file)?.exprs.get(index as usize)
    }

    /// The expression with parentheses, `!`, `as`, `satisfies`, `<T>`, and instantiations
    /// removed.
    pub(crate) fn unwrap_expr(&self, file: FileId, mut index: u32) -> u32 {
        while let Some(
            Expr::Paren(inner)
            | Expr::NonNull(inner)
            | Expr::Satisfies(inner)
            | Expr::Instantiation(inner)
            | Expr::Assert { inner, .. },
        ) = self.expr(file, index)
        {
            index = *inner;
        }
        index
    }

    /// The expression with only parentheses, `!`, and `satisfies` removed (an assertion's
    /// written type stays authoritative for a receiver).
    pub(crate) fn unwrap_receiver(&self, file: FileId, mut index: u32) -> u32 {
        while let Some(Expr::Paren(inner) | Expr::NonNull(inner) | Expr::Satisfies(inner)) =
            self.expr(file, index)
        {
            index = *inner;
        }
        index
    }

    /// Whether a file's symbol is a script global (declared at file scope of a script).
    fn is_script_global(&self, file: FileId, symbol: u32) -> bool {
        !self.is_module(file)
            && self
                .tables
                .file_scope
                .get(file.index())
                .and_then(|scope| scope.get(symbol as usize))
                .copied()
                .unwrap_or(false)
    }

    /// The configuration group's global symbol `name` with `meaning`, as a target.
    pub(crate) fn global(&self, file: FileId, name: &str, meaning: Meanings) -> Option<Target> {
        let global = self.global_symbol(file, name)?;
        global.meanings.has(meaning).then(|| global.target.clone())
    }

    /// The configuration group's merged global symbol `name`.
    fn global_symbol(&self, file: FileId, name: &str) -> Option<&GlobalSymbol> {
        let group = *self.tables.groups.get(file.index())?;
        self.tables.globals.get(group as usize)?.get(name)
    }

    /// Fills in what each merged global symbol denotes, and indexes the members of merged
    /// namespaces declared in several files, once per link.
    pub(crate) fn merge_globals(
        &self,
        globals: &mut [FxHashMap<Box<str>, GlobalSymbol>],
    ) -> FxHashMap<(FileId, u32), MergedNamespace> {
        let mut merged = FxHashMap::default();
        for global in globals.iter_mut().flat_map(|group| group.values_mut()) {
            let mut target = Target::default();
            let mut tables: SmallVec<[(FileId, u32); 1]> = SmallVec::new();
            for &(member_file, symbol) in &global.members {
                let Some(found) = self
                    .binding(member_file)
                    .and_then(|binding| binding.symbols.get(symbol as usize))
                else {
                    continue;
                };
                for &declaration in &found.declarations {
                    target.add_declaration(DeclRef {
                        file: member_file,
                        index: declaration,
                    });
                }
                if let Some(namespace) = found.namespace {
                    tables.push((member_file, namespace));
                }
            }
            if tables.len() > 1 {
                merged
                    .entry(tables[0])
                    .or_insert_with(|| self.merged_namespace(&tables));
            }
            if !tables.is_empty() {
                target.namespace = Some(Namespace::Local(tables));
            }
            global.target = target;
        }
        merged
    }

    /// The member index of several namespace tables: for each name, the symbol each table's
    /// lookup finds, in table order.
    fn merged_namespace(&self, tables: &[(FileId, u32)]) -> MergedNamespace {
        let mut members: FxHashMap<Box<str>, SmallVec<[(FileId, u32); 1]>> = FxHashMap::default();
        for &(file, namespace) in tables {
            let Some(table) = self
                .binding(file)
                .and_then(|binding| binding.namespaces.get(namespace as usize))
            else {
                continue;
            };
            for (position, (name, _)) in table.members.iter().enumerate() {
                if position > 0 && table.members[position - 1].0 == *name {
                    continue;
                }
                // The symbol `member` finds for this name in this table.
                if let Ok(found) = table.members.binary_search_by(|(member, _)| (**member).cmp(name)) {
                    members
                        .entry(name.clone())
                        .or_default()
                        .push((file, table.members[found].1));
                }
            }
        }
        MergedNamespace {
            tables: tables.iter().copied().collect(),
            members,
        }
    }

    /// Whether the group has a global named `name` in either meaning.
    pub(crate) fn has_global(&self, file: FileId, name: &str) -> bool {
        self.tables
            .groups
            .get(file.index())
            .and_then(|&group| self.tables.globals.get(group as usize))
            .is_some_and(|globals| globals.contains_key(name))
    }

    /// What symbol `symbol` of `file` denotes in `meaning`.
    pub(crate) fn resolve_symbol(
        &self,
        walk: &mut Walk,
        file: FileId,
        symbol: u32,
        meaning: Meanings,
    ) -> Target {
        let Some(found) = self
            .binding(file)
            .and_then(|binding| binding.symbols.get(symbol as usize))
        else {
            return Target::default();
        };
        if self.is_script_global(file, symbol) {
            return self.global(file, &found.name, meaning).unwrap_or_default();
        }
        if let Some(alias) = found.alias {
            if !walk.symbols.insert((file, symbol)) || !walk.enter() {
                return Target::default();
            }
            let target = self.alias_target(walk, file, alias, meaning);
            walk.leave();
            walk.symbols.remove(&(file, symbol));
            return target;
        }
        if meaning == Meanings::TYPE && !found.meanings.has(Meanings::TYPE) {
            return Target::default();
        }
        let mut target = Target::of(found.declarations.iter().map(|&index| DeclRef { file, index }));
        target.namespace = found
            .namespace
            .map(|namespace| Namespace::Local(SmallVec::from_elem((file, namespace), 1)));
        target
    }

    /// What an import alias imports, in `meaning`.
    fn alias_target(&self, walk: &mut Walk, file: FileId, alias: AliasRef, meaning: Meanings) -> Target {
        match alias {
            AliasRef::Module(index) => {
                let Some(binding) = self
                    .module_facts(file)
                    .and_then(|facts| facts.imports.get(index as usize))
                else {
                    return Target::default();
                };
                if binding.type_only && meaning == Meanings::VALUE {
                    return Target::default();
                }
                // Qualified `import x = A.B` aliases are bound lexically (`AliasRef::Local`).
                if matches!(binding.source, ImportSource::Qualified(_)) {
                    return Target::default();
                }
                let export = if meaning == Meanings::TYPE {
                    self.exports().type_binding(file, index)
                } else {
                    self.exports().value_binding(file, index)
                };
                export.map_or_else(Target::default, |export| self.export_target(export))
            }
            AliasRef::Local(index) => {
                match self
                    .binding(file)
                    .and_then(|binding| binding.aliases.get(index as usize))
                {
                    Some(Alias::Qualified {
                        first,
                        lookup,
                        rest,
                        type_only,
                    }) => {
                        if *type_only && meaning == Meanings::VALUE {
                            return Target::default();
                        }
                        let mut target = self.symbol_lookup(walk, file, first, *lookup, meaning);
                        if target.namespace.is_none() && target.external.is_empty() {
                            return Target::default();
                        }
                        for name in rest {
                            target = self.member(walk, &target, name, meaning);
                        }
                        target
                    }
                    Some(Alias::Dynamic { specifier, imported }) => {
                        let resolved = match self.alias_resolution(file, index) {
                            Some(resolved) => Cow::Borrowed(resolved),
                            None => Cow::Owned(self.resolve(file, specifier, SpecifierKind::Dynamic)),
                        };
                        let module = match &*resolved {
                            Resolved::Indexed(module) => Target {
                                namespace: Some(Namespace::Module(*module)),
                                ..Target::default()
                            },
                            Resolved::Package(name) => {
                                Target::external([ExternalLocation::Package(name.clone())])
                            }
                            Resolved::Unindexed {
                                path,
                                excluded: false,
                            } => Target::external([ExternalLocation::File(path.as_str().into())]),
                            Resolved::Unindexed { path, excluded: true } => {
                                Target::excluded([ExternalLocation::File(path.as_str().into())])
                            }
                            Resolved::Failed(failed) => match self.program().file(*failed) {
                                Some(entry) => {
                                    Target::excluded([ExternalLocation::File(entry.path.as_str().into())])
                                }
                                None => Target::default(),
                            },
                            Resolved::NotFound | Resolved::NotIndexed => Target::default(),
                        };
                        self.member(walk, &module, imported, meaning)
                    }
                    _ => Target::default(),
                }
            }
        }
    }

    /// A lexical name's target through symbols and script globals only (no library
    /// globals).
    pub(crate) fn symbol_lookup(
        &self,
        walk: &mut Walk,
        file: FileId,
        name: &str,
        lookup: Lookup,
        meaning: Meanings,
    ) -> Target {
        match lookup {
            Lookup::Symbol(symbol) => self.resolve_symbol(walk, file, symbol, meaning),
            Lookup::Unresolved | Lookup::Namespaced(_) => self
                .unresolved(walk, file, name, lookup, meaning)
                .unwrap_or_default(),
            Lookup::Dynamic => Target::default(),
        }
    }

    /// A name the file doesn't declare where it is used: an export of another declaration of
    /// an enclosing script-global namespace (innermost first), or a script global.
    pub(crate) fn unresolved(
        &self,
        walk: &mut Walk,
        file: FileId,
        name: &str,
        lookup: Lookup,
        meaning: Meanings,
    ) -> Option<Target> {
        if let Lookup::Namespaced(chain) = lookup
            && let Some(found) = self.namespace_member(walk, file, name, chain, meaning)
        {
            return Some(found);
        }
        self.global(file, name, meaning)
    }

    /// Member `name` of the namespaces of `chain` (innermost first), each merged with its
    /// declarations in other files when it is a script global or a member of one.
    fn namespace_member(
        &self,
        walk: &mut Walk,
        file: FileId,
        name: &str,
        chain: u32,
        meaning: Meanings,
    ) -> Option<Target> {
        let binding = self.binding(file)?;
        let chain = binding.namespace_chains.get(chain as usize)?;
        let mut targets: Vec<Target> = Vec::with_capacity(chain.len());
        let mut parent: Option<Target> = None;
        for &symbol in chain.iter().rev() {
            let own = self.resolve_symbol(walk, file, symbol, Meanings::VALUE);
            let target = match (&parent, binding.symbols.get(symbol as usize)) {
                (Some(parent), Some(found)) => {
                    let member = self.member(walk, parent, &found.name, Meanings::VALUE);
                    if member.namespace.is_some() { member } else { own }
                }
                _ => own,
            };
            parent = Some(target.clone());
            targets.push(target);
        }
        targets
            .iter()
            .rev()
            .filter(|target| matches!(target.namespace, Some(Namespace::Local(_))))
            .map(|target| self.member(walk, target, name, meaning))
            .find(|found| !found.is_empty())
    }

    /// Whether a lexical name has a symbol (a file symbol or a script global).
    pub(crate) fn has_symbol(&self, file: FileId, name: &str, lookup: Lookup) -> bool {
        match lookup {
            Lookup::Symbol(_) => true,
            Lookup::Unresolved | Lookup::Namespaced(_) => self
                .unresolved(&mut Walk::default(), file, name, lookup, Meanings::VALUE)
                .is_some(),
            Lookup::Dynamic => false,
        }
    }

    /// An identifier's value target: its symbol, a script global, or a known library global.
    fn name_target(&self, walk: &mut Walk, file: FileId, name: &str, lookup: Lookup) -> Target {
        match lookup {
            Lookup::Symbol(symbol) => self.resolve_symbol(walk, file, symbol, Meanings::VALUE),
            Lookup::Unresolved | Lookup::Namespaced(_) => {
                match self.unresolved(walk, file, name, lookup, Meanings::VALUE) {
                    Some(target) => target,
                    None if tables::is_ecmascript_global(name) => {
                        Target::external([ExternalLocation::Builtin])
                    }
                    None if tables::is_known_global(name) => Target::external([ExternalLocation::Library]),
                    None => Target::default(),
                }
            }
            Lookup::Dynamic => Target::default(),
        }
    }

    /// An export table target as evidence sees it: a module namespace is only its members
    /// (not a declaration), an exported expression is its inline function or class, and a
    /// namespace or enum declaration brings its members.
    pub(crate) fn export_target(&self, export: &ExportTarget) -> Target {
        let mut target = Target {
            external: export.external.iter().cloned().collect(),
            excluded: export.excluded.iter().cloned().collect(),
            ambiguous: export.ambiguous,
            ..Target::default()
        };
        let mut namespaces: SmallVec<[Namespace; 1]> = SmallVec::new();
        if let Some(module) = export.namespace {
            namespaces.push(Namespace::Module(module));
        }
        for &declaration in &export.declarations {
            let Some(decl) = self.decl(declaration) else {
                continue;
            };
            let declaration = match (decl.kind, decl.inline) {
                (DeclKind::Module, _) => continue,
                (DeclKind::Expression, Some(inline)) => DeclRef {
                    file: declaration.file,
                    index: inline,
                },
                _ => declaration,
            };
            target.add_declaration(declaration);
            let namespace = self
                .decl(declaration)
                .and_then(|decl| decl.symbol)
                .and_then(|symbol| self.binding(declaration.file)?.symbols.get(symbol as usize))
                .and_then(|symbol| symbol.namespace);
            if let Some(namespace) = namespace {
                let local = Namespace::Local(SmallVec::from_elem((declaration.file, namespace), 1));
                if !namespaces.contains(&local) {
                    namespaces.push(local);
                }
            }
        }
        target.ambiguous |= namespaces.len() > 1;
        if namespaces.len() == 1 {
            target.namespace = namespaces.pop();
        }
        target
    }

    /// Member `name` of a target in `meaning`: a module's export, a namespace's or enum's
    /// member, or an external member.
    pub(crate) fn member(&self, walk: &mut Walk, target: &Target, name: &str, meaning: Meanings) -> Target {
        if target.ambiguous || !target.excluded.is_empty() {
            return target.clone();
        }
        if !target.external.is_empty() {
            return Target::external(target.external.iter().cloned());
        }
        match &target.namespace {
            Some(Namespace::Module(module)) => {
                let module = ExportTarget::module(*module);
                let export = if meaning == Meanings::TYPE {
                    self.exports().type_member(&module, name)
                } else {
                    self.exports().value_member(&module, name)
                };
                self.export_target(&export)
            }
            Some(Namespace::Local(tables)) => {
                if let Some(merged) = tables
                    .first()
                    .and_then(|first| self.tables.merged_namespaces.get(first))
                    .filter(|merged| merged.tables == *tables)
                {
                    let symbols = merged
                        .members
                        .get(name)
                        .map_or(&[][..], |symbols| symbols.as_slice());
                    return Target::join(
                        symbols
                            .iter()
                            .map(|&(file, symbol)| self.resolve_symbol(walk, file, symbol, meaning)),
                    );
                }
                let mut found = Vec::new();
                for &(file, namespace) in tables {
                    let Some(members) = self
                        .binding(file)
                        .and_then(|binding| binding.namespaces.get(namespace as usize))
                    else {
                        continue;
                    };
                    if let Ok(position) = members
                        .members
                        .binary_search_by(|(member, _)| (**member).cmp(name))
                    {
                        let symbol = members.members[position].1;
                        found.push(self.resolve_symbol(walk, file, symbol, meaning));
                    }
                }
                Target::join(found)
            }
            None => Target::default(),
        }
    }

    /// What expression `index` of `file` denotes.
    pub(crate) fn target(&self, walk: &mut Walk, file: FileId, index: u32) -> Target {
        if let Some(done) = walk.target_memo.get(&(file, index)) {
            return done.clone();
        }
        if !walk.targets.insert((file, index)) || !walk.enter() {
            return Target::default();
        }
        let target = self.target_uncached(walk, file, index);
        walk.leave();
        walk.targets.remove(&(file, index));
        walk.target_memo.insert((file, index), target.clone());
        target
    }

    fn target_uncached(&self, walk: &mut Walk, file: FileId, index: u32) -> Target {
        let index = self.unwrap_expr(file, index);
        let Some(expr) = self.expr(file, index) else {
            return Target::default();
        };
        match expr {
            Expr::Name { name, lookup } => self.name_target(walk, file, name, *lookup),
            Expr::Member {
                object,
                name,
                private,
            } => {
                if let Some(private) = private {
                    return Target::of(private.iter().map(|&index| DeclRef { file, index }));
                }
                self.access_target(walk, file, *object, name, false)
            }
            Expr::Element {
                object,
                key: Some(key),
                index: element,
                ..
            } => self.access_target(walk, file, *object, key, *element),
            Expr::Super(context) => self.super_target(walk, file, *context),
            Expr::This(context) => match self
                .binding(file)
                .and_then(|binding| binding.contexts.get(*context as usize))
            {
                Some(context) => match context.receiver {
                    Some((class, true)) => Target::of([DeclRef { file, index: class }]),
                    _ => Target::default(),
                },
                None => Target::default(),
            },
            Expr::Function(decl) | Expr::Class(decl) => Target::of([DeclRef { file, index: *decl }]),
            _ => Target::default(),
        }
    }

    /// Member `name` of the value `object` (`element` when the access reads an index).
    fn access_target(&self, walk: &mut Walk, file: FileId, object: u32, name: &str, element: bool) -> Target {
        let base = self.target(walk, file, object);
        let receiver = self.class_receiver(walk, file, object);
        if receiver.unbound_this || (receiver.array && element) {
            return Target::default();
        }
        if receiver.excluded {
            return Target::excluded(receiver.locations);
        }
        if receiver
            .builtin
            .is_some_and(|builtin| tables::is_ecmascript_method(builtin, name))
        {
            return Target::external([ExternalLocation::Builtin]);
        }
        if let Some(class) = receiver.class {
            let member = self.lookup_member(walk, class, name, receiver.is_static);
            if !member.is_empty() {
                return member;
            }
        }
        if receiver.external {
            return Target::external(receiver.locations);
        }
        if let Some(member) = self.object_member(&base, name) {
            return member;
        }
        self.member(walk, &base, name, Meanings::VALUE)
    }

    /// The function members `name` of a constant object literal (C-2): the base must be
    /// exactly one non-ambient `const` whose value, as written, is an object literal with no
    /// spread or unknown computed key; every member of that name must be a method or a
    /// function-valued property. Anything else is left to the other rules.
    fn object_member(&self, base: &Target, name: &str) -> Option<Target> {
        if base.ambiguous
            || !base.external.is_empty()
            || !base.excluded.is_empty()
            || base.namespace.is_some()
        {
            return None;
        }
        let [variable] = base.declarations.as_slice() else {
            return None;
        };
        let decl = self.decl(*variable)?;
        if decl.kind != DeclKind::Variable
            || decl.var_kind != Some(VarKind::Const)
            || decl.flags.has(DeclFlags::AMBIENT)
        {
            return None;
        }
        let object = *self.tables.objects.get(variable)?;
        let facts = self.binding(variable.file)?.objects.get(object as usize)?;
        if !facts.closed || !facts.plain {
            return None;
        }
        let mut members = SmallVec::<[DeclRef; 2]>::new();
        for &member in &facts.members {
            let reference = DeclRef {
                file: variable.file,
                index: member,
            };
            let Some(found) = self.decl(reference) else {
                continue;
            };
            if found.name.as_deref() != Some(name) {
                continue;
            }
            let function = match found.kind {
                DeclKind::Method => found.flags.has(DeclFlags::HAS_BODY),
                DeclKind::PropertyAssignment => found.inline.is_some(),
                _ => false,
            };
            if !function {
                return None;
            }
            members.push(reference);
        }
        (!members.is_empty()).then(|| Target::of(members))
    }
}
