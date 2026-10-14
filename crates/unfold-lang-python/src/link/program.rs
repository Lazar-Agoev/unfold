//! The linked program as the binder sees it: every Python file's decoded binder facts, one
//! name table for the whole program, link-time writes and bindings (star imports expanded), and
//! name lookup by Python's scoping rules.

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::vocab::Language;
use unfold_engine::{FactsSet, FileId};

use super::modules::{Module, Modules};
use crate::bind::ir::{
    BindFacts, Effect, ExprFact, ExprId, ExprKind, MODULE_SCOPE, NodeFact, NodeId, NodeKind, ScopeFact,
    ScopeId, ScopeKind, StarFact, WriteKind,
};
use crate::payload::{ImportSyntax, Payload};

/// A name in the program-wide table.
pub(crate) type Sym = u32;

/// A declaration of the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NodeRef {
    pub(crate) file: FileId,
    pub(crate) node: NodeId,
}

/// The file of the nodes that stand for builtin exception classes in linearizations: no file
/// of the program has this ID, so every fact lookup on such a node finds nothing.
const BUILTIN_EXCEPTIONS: FileId = FileId(u32::MAX);

impl NodeRef {
    /// The node standing for the builtin exception class at `index` of
    /// [`crate::stdlib::BUILTIN_EXCEPTIONS`].
    pub(crate) const fn builtin_exception(index: u32) -> Self {
        Self {
            file: BUILTIN_EXCEPTIONS,
            node: index,
        }
    }

    /// Whether this node stands for a builtin exception class rather than a declaration.
    pub(crate) const fn is_builtin(self) -> bool {
        self.file.0 == BUILTIN_EXCEPTIONS.0
    }
}

/// An expression of the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ExprRef {
    pub(crate) file: FileId,
    pub(crate) expr: ExprId,
}

/// What an import write binds, resolved.
#[derive(Debug, Clone)]
pub(crate) struct Imported {
    pub(crate) module: Module,
    pub(crate) member: Option<Sym>,
}

/// One write, at link time.
#[derive(Debug, Clone)]
pub(crate) struct Write {
    pub(crate) node: NodeRef,
    pub(crate) kind: WriteKind,
    pub(crate) value: Option<ExprRef>,
    pub(crate) annotation: Option<ExprRef>,
    pub(crate) import: Option<Imported>,
    pub(crate) conditional: bool,
}

/// Every write of one name in one scope.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) file: FileId,
    pub(crate) scope: ScopeId,
    pub(crate) name: Sym,
    pub(crate) writes: Vec<u32>,
    /// Another module or a non-receiver write assigns this name as an attribute.
    pub(crate) mutated: bool,
}

/// One Python file's facts.
pub(crate) struct FileBind {
    pub(crate) scopes: Vec<ScopeFact>,
    pub(crate) nodes: Vec<NodeFact>,
    pub(crate) exprs: Vec<ExprFact>,
    pub(crate) values: Vec<ExprId>,
    /// The expression of each written type name, aligned with the file's type references.
    pub(crate) types: Vec<ExprId>,
    pub(crate) effects: Vec<Effect>,
    pub(crate) stars: Vec<StarFact>,
    pub(crate) imports: Vec<ImportSyntax>,
    pub(crate) census: crate::payload::CensusData,
    /// Each local name's program-wide symbol.
    pub(crate) syms: Vec<Sym>,
    /// Scopes holding a star import whose names are unknown.
    pub(crate) unknown_star: Vec<bool>,
    /// A proven `exec` or `eval` in the module makes every identity in it unsupported.
    pub(crate) unsupported: bool,
}

/// The program-wide name table.
#[derive(Default)]
pub(crate) struct Names {
    texts: Vec<Box<str>>,
    ids: FxHashMap<Box<str>, Sym>,
}

impl Names {
    pub(crate) fn intern(&mut self, text: &str) -> Sym {
        if let Some(&sym) = self.ids.get(text) {
            return sym;
        }
        self.intern_owned(text.into())
    }

    fn intern_owned(&mut self, text: Box<str>) -> Sym {
        if let Some(&sym) = self.ids.get(&text) {
            return sym;
        }
        let sym = self.texts.len() as Sym;
        self.texts.push(text.clone());
        self.ids.insert(text, sym);
        sym
    }

    pub(crate) fn get(&self, text: &str) -> Option<Sym> {
        self.ids.get(text).copied()
    }

    pub(crate) fn text(&self, sym: Sym) -> &str {
        self.texts.get(sym as usize).map_or("", |text| text)
    }
}

/// Symbols the binder compares against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Known {
    pub(crate) star: Sym,
    pub(crate) all: Sym,
    pub(crate) init: Sym,
    pub(crate) new: Sym,
    pub(crate) call: Sym,
    pub(crate) getattribute: Sym,
}

/// The program: files, names, writes, bindings.
pub(crate) struct Program<'a> {
    pub(crate) facts: &'a FactsSet,
    pub(crate) files: Vec<Option<FileBind>>,
    pub(crate) names: Names,
    pub(crate) known: Known,
    pub(crate) writes: Vec<Write>,
    pub(crate) bindings: Vec<Binding>,
    symbols: FxHashMap<(FileId, ScopeId, Sym), u32>,
    /// The binding of each declaration's last write (the proof of concept's `bindingFor`).
    node_bindings: FxHashMap<NodeRef, u32>,
    /// Every class, in program order.
    pub(crate) classes: Vec<NodeRef>,
    /// The bindings of each scope, in order (filled by [`Program::index_scopes`]).
    scope_bindings: FxHashMap<(FileId, ScopeId), Vec<u32>>,
}

impl<'a> Program<'a> {
    /// Decodes every Python file's payload and builds names, writes, and bindings. Star imports
    /// are expanded afterwards ([`Program::expand_stars`]).
    pub(crate) fn new(facts: &'a FactsSet, modules: &mut Modules<'_>) -> Self {
        let mut names = Names::default();
        let known = Known {
            star: names.intern("*"),
            all: names.intern("__all__"),
            init: names.intern("__init__"),
            new: names.intern("__new__"),
            call: names.intern("__call__"),
            getattribute: names.intern("__getattribute__"),
        };
        let mut program = Self {
            facts,
            files: Vec::with_capacity(facts.files().len()),
            names,
            known,
            writes: Vec::new(),
            bindings: Vec::new(),
            symbols: FxHashMap::default(),
            node_bindings: FxHashMap::default(),
            classes: Vec::new(),
            scope_bindings: FxHashMap::default(),
        };
        // Decoding is per file and runs in parallel; interning names then runs in file order, so
        // symbols are numbered the same on any thread count.
        let payloads: Vec<Option<Payload>> = facts
            .files()
            .par_iter()
            .map(|file| {
                file.facts
                    .as_deref()
                    .filter(|_| file.language == Language::Python)
                    .and_then(|facts| Payload::from_bytes(&facts.payload).ok())
                    // A damaged cache entry that still decodes is treated like one that doesn't.
                    .filter(|payload| payload.bind.is_consistent())
            })
            .collect();
        for ((id, _), payload) in facts.iter().zip(payloads) {
            let Some(payload) = payload else {
                program.files.push(None);
                continue;
            };
            let bind = program.add_file(id, payload, modules);
            program.files.push(Some(bind));
        }
        program
    }

    fn add_file(&mut self, id: FileId, payload: Payload, modules: &mut Modules<'_>) -> FileBind {
        let Payload {
            census,
            imports,
            bind,
            ..
        } = payload;
        let BindFacts {
            names,
            scopes,
            nodes,
            exprs,
            writes,
            bindings,
            values,
            types,
            stars,
            effects,
        } = bind;
        let syms: Vec<Sym> = names
            .into_iter()
            .map(|name| self.names.intern_owned(name))
            .collect();
        let sym = |name: u32| syms.get(name as usize).copied().unwrap_or(u32::MAX);
        let path = self.facts.file(id).map_or("", |file| file.path.as_str());
        let base = self.writes.len() as u32;
        for write in writes {
            let import = write.import.map(|import| Imported {
                module: modules.resolve(id, path, &import.module.text()),
                member: import.member.map(sym),
            });
            self.writes.push(Write {
                node: NodeRef {
                    file: id,
                    node: write.node,
                },
                kind: write.kind,
                value: write.value.map(|expr| ExprRef { file: id, expr }),
                annotation: write.annotation.map(|expr| ExprRef { file: id, expr }),
                import,
                conditional: write.conditional,
            });
        }
        for binding in bindings {
            let index = self.bindings.len() as u32;
            let name = sym(binding.name);
            self.symbols.insert((id, binding.scope, name), index);
            let writes: Vec<u32> = binding.writes.iter().map(|write| base + write).collect();
            for &write in &writes {
                self.node_bindings.insert(self.writes[write as usize].node, index);
            }
            self.bindings.push(Binding {
                file: id,
                scope: binding.scope,
                name,
                writes,
                mutated: false,
            });
        }
        for (index, node) in nodes.iter().enumerate() {
            if matches!(node.kind, NodeKind::Class(_)) {
                self.classes.push(NodeRef {
                    file: id,
                    node: index as NodeId,
                });
            }
        }
        FileBind {
            unknown_star: vec![false; scopes.len()],
            scopes,
            nodes,
            exprs,
            values,
            types,
            effects,
            stars,
            imports,
            census,
            syms,
            unsupported: false,
        }
    }

    /// Expands star imports from their target's literal `__all__` (a list or tuple of string
    /// literals, written once); any other star import makes its scope's names unknown.
    pub(crate) fn expand_stars(&mut self, modules: &mut Modules<'_>) {
        let stars: Vec<(FileId, StarFact)> = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, bind)| Some((FileId(index as u32), bind.as_ref()?)))
            .flat_map(|(id, bind)| bind.stars.iter().map(move |star| (id, star.clone())))
            .collect();
        for (id, star) in stars {
            let path = self.facts.file(id).map_or("", |file| file.path.as_str());
            let module = modules.resolve(id, path, &star.module.text());
            let names = match module.paths.as_slice() {
                [path] => modules.indexed(path).and_then(|target| self.literal_all(target)),
                _ => None,
            };
            let Some(names) = names else {
                if let Some(bind) = self.files.get_mut(id.index()).and_then(Option::as_mut)
                    && let Some(flag) = bind.unknown_star.get_mut(star.scope as usize)
                {
                    *flag = true;
                }
                continue;
            };
            for name in names {
                let write = self.writes.len() as u32;
                let node = NodeRef {
                    file: id,
                    node: star.node,
                };
                self.writes.push(Write {
                    node,
                    kind: WriteKind::Import,
                    value: None,
                    annotation: None,
                    import: Some(Imported {
                        module: module.clone(),
                        member: Some(name),
                    }),
                    conditional: star.conditional,
                });
                let scope = self.write_scope(id, star.scope, name);
                let next = self.bindings.len() as u32;
                let binding = *self.symbols.entry((id, scope, name)).or_insert(next);
                if binding == next {
                    self.bindings.push(Binding {
                        file: id,
                        scope,
                        name,
                        writes: Vec::new(),
                        mutated: false,
                    });
                }
                self.bindings[binding as usize].writes.push(write);
                self.node_bindings.insert(node, binding);
            }
        }
    }

    /// Indexes every scope's bindings; call once all bindings exist.
    pub(crate) fn index_scopes(&mut self) {
        let mut index: FxHashMap<(FileId, ScopeId), Vec<u32>> = FxHashMap::default();
        for (position, binding) in self.bindings.iter().enumerate() {
            index
                .entry((binding.file, binding.scope))
                .or_default()
                .push(position as u32);
        }
        self.scope_bindings = index;
    }

    /// The bindings of a scope, in the order of their first write.
    pub(crate) fn bindings_in(&self, file: FileId, scope: ScopeId) -> &[u32] {
        self.scope_bindings.get(&(file, scope)).map_or(&[], Vec::as_slice)
    }

    /// The names of a module's `__all__`, when it is written once as a list or tuple of string
    /// literals.
    fn literal_all(&self, file: FileId) -> Option<Vec<Sym>> {
        let binding = self.binding_in(file, MODULE_SCOPE, self.known.all)?;
        let [write] = self.bindings[binding as usize].writes.as_slice() else {
            return None;
        };
        let value = self.writes[*write as usize].value?;
        let ExprKind::Sequence { items, .. } = &self.expr(value)?.kind else {
            return None;
        };
        items
            .iter()
            .map(|&item| match self.expr(ExprRef { file, expr: item })?.kind {
                ExprKind::Str(name) => Some(self.sym(file, name)),
                _ => None,
            })
            .collect()
    }

    /// Where a link-time write of `name` in `scope` binds (`global` and `nonlocal` redirect).
    fn write_scope(&self, file: FileId, scope: ScopeId, name: Sym) -> ScopeId {
        let Some(fact) = self.scope(file, scope) else {
            return scope;
        };
        if fact.globals.iter().any(|&global| self.sym(file, global) == name) {
            return MODULE_SCOPE;
        }
        fact.nonlocals
            .iter()
            .find(|&&(local, _)| self.sym(file, local) == name)
            .map_or(scope, |&(_, target)| target)
    }

    pub(crate) fn file(&self, file: FileId) -> Option<&FileBind> {
        self.files.get(file.index())?.as_ref()
    }

    pub(crate) fn node(&self, node: NodeRef) -> Option<&NodeFact> {
        self.file(node.file)?.nodes.get(node.node as usize)
    }

    pub(crate) fn expr(&self, expr: ExprRef) -> Option<&ExprFact> {
        self.file(expr.file)?.exprs.get(expr.expr as usize)
    }

    pub(crate) fn scope(&self, file: FileId, scope: ScopeId) -> Option<&ScopeFact> {
        self.file(file)?.scopes.get(scope as usize)
    }

    /// The program-wide symbol of a file's local name.
    pub(crate) fn sym(&self, file: FileId, name: u32) -> Sym {
        self.file(file)
            .and_then(|bind| bind.syms.get(name as usize))
            .copied()
            .unwrap_or(u32::MAX)
    }

    pub(crate) fn text(&self, sym: Sym) -> &str {
        self.names.text(sym)
    }

    /// The binding of `name` written directly in `scope`.
    pub(crate) fn binding_in(&self, file: FileId, scope: ScopeId, name: Sym) -> Option<u32> {
        self.symbols.get(&(file, scope, name)).copied()
    }

    /// The binding of a declaration's last write.
    pub(crate) fn binding_of(&self, node: NodeRef) -> Option<u32> {
        self.node_bindings.get(&node).copied()
    }

    pub(crate) fn binding(&self, binding: u32) -> &Binding {
        &self.bindings[binding as usize]
    }

    pub(crate) fn write(&self, write: u32) -> &Write {
        &self.writes[write as usize]
    }

    /// Looks `name` up from `scope`: a `global` or `nonlocal` declaration redirects; otherwise
    /// the scope, then its ancestors. A class body is visible only to code directly in it: from
    /// the class scope itself, or through PEP 695 type-parameter scopes alone (annotation
    /// scopes see the class around them); never from a function, lambda, comprehension, or a
    /// nested class body.
    pub(crate) fn lookup(&self, file: FileId, scope: ScopeId, name: Sym) -> Option<u32> {
        let redirected = self.write_scope(file, scope, name);
        if redirected != scope {
            return self.binding_in(file, redirected, name);
        }
        // Whether every scope left so far is a type-parameter scope.
        let mut direct = true;
        let mut current = Some(scope);
        while let Some(index) = current {
            let fact = self.scope(file, index)?;
            let visible = index == scope || direct || fact.kind != ScopeKind::Class;
            if visible && let Some(binding) = self.binding_in(file, index, name) {
                return Some(binding);
            }
            direct &= fact.kind == ScopeKind::Type;
            current = fact.parent;
        }
        None
    }

    /// Whether a scope or any scope around it holds a star import with unknown names.
    pub(crate) fn unknown_star(&self, file: FileId, scope: ScopeId) -> bool {
        let Some(bind) = self.file(file) else {
            return false;
        };
        let mut current = Some(scope);
        while let Some(index) = current {
            if bind.unknown_star.get(index as usize).copied().unwrap_or(false) {
                return true;
            }
            current = bind.scopes.get(index as usize).and_then(|fact| fact.parent);
        }
        false
    }

    /// Whether a scope's own star import has unknown names.
    pub(crate) fn own_unknown_star(&self, file: FileId, scope: ScopeId) -> bool {
        self.file(file)
            .and_then(|bind| bind.unknown_star.get(scope as usize))
            .copied()
            .unwrap_or(false)
    }

    /// Whether the scope's module holds a proven `exec` or `eval`.
    pub(crate) fn unsupported(&self, file: FileId) -> bool {
        self.file(file).is_some_and(|bind| bind.unsupported)
    }

    /// The nearest enclosing function or class of a node, skipping lambdas.
    pub(crate) fn enclosing_definition(&self, node: NodeRef) -> Option<NodeRef> {
        let mut current = self.node(node)?.enclosing;
        while let Some(index) = current {
            let candidate = NodeRef {
                file: node.file,
                node: index,
            };
            let fact = self.node(candidate)?;
            match &fact.kind {
                NodeKind::Function(function) if function.name.is_none() => current = fact.enclosing,
                _ => return Some(candidate),
            }
        }
        None
    }

    /// The class directly around a node (its nearest enclosing function, class, or lambda,
    /// when that is a class).
    pub(crate) fn owner_class(&self, node: NodeRef) -> Option<NodeRef> {
        let enclosing = self.node(node)?.enclosing?;
        let owner = NodeRef {
            file: node.file,
            node: enclosing,
        };
        matches!(self.node(owner)?.kind, NodeKind::Class(_)).then_some(owner)
    }

    /// The module scope of a node's file.
    pub(crate) fn module_binding(&self, file: FileId, name: Sym) -> Option<u32> {
        self.binding_in(file, MODULE_SCOPE, name)
    }

    /// Whether `node` is a class.
    pub(crate) fn is_class(&self, node: NodeRef) -> bool {
        self.node(node)
            .is_some_and(|fact| matches!(fact.kind, NodeKind::Class(_)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use unfold_core::{RepoPath, Snapshot};
    use unfold_engine::settings::PythonVersion;
    use unfold_engine::{FileFacts, Linked, ProgramFile, ProjectInputs};

    use super::*;
    use crate::link::PythonLinked;

    const SOURCE: &str = "import os\n\n\ndef outer():\n    x = os.getcwd\n\n    def inner():\n        nonlocal x\n        x()\n\n    class C:\n        def m(self):\n            return inner()\n\n    return C\n";

    fn facts() -> FileFacts {
        let snapshot = Snapshot::from_text(RepoPath::new("m.py").expect("path"), SOURCE).expect("text");
        crate::extract(&snapshot, PythonVersion::default()).expect("extracts")
    }

    fn with_payload(facts: &FileFacts, change: impl FnOnce(&mut Payload)) -> FileFacts {
        let mut payload = Payload::from_bytes(&facts.payload).expect("decodes");
        change(&mut payload);
        FileFacts {
            graph: facts.graph.clone(),
            payload: payload.to_bytes().expect("encodes").into(),
        }
    }

    /// Damaged binder facts that still decode (an index out of range, a scope that is its own
    /// parent, an enclosing declaration after what it encloses) are refused when linking loads
    /// them, instead of panicking or walking forever.
    #[test]
    fn damaged_payloads_are_refused_not_followed() {
        let facts = facts();
        assert!(
            Payload::from_bytes(&facts.payload)
                .expect("decodes")
                .bind
                .is_consistent()
        );
        let damaged = [
            with_payload(&facts, |payload| payload.bind.bindings[0].writes.push(u32::MAX)),
            with_payload(&facts, |payload| {
                let last = payload.bind.scopes.len() - 1;
                payload.bind.scopes[last].parent = Some(last as ScopeId);
            }),
            with_payload(&facts, |payload| {
                let last = payload.bind.nodes.len() - 1;
                payload.bind.nodes[0].enclosing = Some(last as NodeId);
            }),
            with_payload(&facts, |payload| payload.bind.values.push(u32::MAX)),
        ];
        for facts in damaged {
            assert!(
                !Payload::from_bytes(&facts.payload)
                    .expect("decodes")
                    .bind
                    .is_consistent()
            );
            let program = FactsSet::new([ProgramFile {
                path: RepoPath::new("m.py").expect("path"),
                language: Language::Python,
                facts: Some(Arc::new(facts)),
            }])
            .expect("one file");
            let inputs = ProjectInputs::default();
            let linked = PythonLinked::new(&program, &inputs);
            assert!(linked.module_dependencies(FileId(0)).is_empty());
        }
    }
}
