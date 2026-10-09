//! Narrowed receivers (the proof of concept's `isNarrowed`): a receiver whose root and member
//! path (or any prefix of it) is the left operand of `instanceof`, or an argument of a call to a
//! type predicate, anywhere in the program, may hold a narrower type than its annotation says,
//! so its declared receiver is withheld. A call in a condition whose callee is unknown may be a
//! predicate, so it narrows its arguments too.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_engine::{DeclRef, FileId};

use super::Walk;
use crate::link::TypeScriptLinked;
use crate::payload::binding::{DeclKind, Expr, Key, KeyRoot, Lookup, Meanings, NamePath, TypeRef};

/// The root of a receiver key: a file symbol, a script global, or a class's `this`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RootId {
    Symbol(FileId, u32),
    Global(u32, Box<str>),
    This(DeclRef),
}

/// Whether a callee is a type predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Predicate {
    Yes,
    No,
    Unknown,
}

impl TypeScriptLinked<'_> {
    /// The root identity of a lexical name.
    fn root_of(&self, file: FileId, name: &str, lookup: Lookup) -> Option<RootId> {
        let group = *self.tables.groups.get(file.index())?;
        match lookup {
            Lookup::Symbol(symbol) => {
                let script_global = !self.is_module(file)
                    && self
                        .tables
                        .file_scope
                        .get(file.index())
                        .and_then(|scope| scope.get(symbol as usize))
                        .copied()
                        .unwrap_or(false);
                Some(if script_global {
                    RootId::Global(group, name.into())
                } else {
                    RootId::Symbol(file, symbol)
                })
            }
            Lookup::Unresolved | Lookup::Namespaced(_) => self
                .has_global(file, name)
                .then(|| RootId::Global(group, name.into())),
            Lookup::Dynamic => None,
        }
    }

    fn key_root(&self, file: FileId, root: &KeyRoot) -> Option<RootId> {
        match root {
            KeyRoot::Symbol(symbol) => {
                let name = &self.binding(file)?.symbols.get(*symbol as usize)?.name;
                self.root_of(file, name, Lookup::Symbol(*symbol))
            }
            KeyRoot::Name(name, lookup) => self.root_of(file, name, *lookup),
            KeyRoot::This(class) => Some(RootId::This(DeclRef { file, index: *class })),
        }
    }

    /// Every narrowed receiver key of the program.
    pub(crate) fn find_narrowed(&self) -> FxHashMap<RootId, FxHashSet<Vec<Box<str>>>> {
        let mut narrowed: FxHashMap<RootId, FxHashSet<Vec<Box<str>>>> = FxHashMap::default();
        let mut mark = |root: Option<RootId>, key: &Key| {
            if let Some(root) = root {
                narrowed.entry(root).or_default().insert(key.path.clone());
            }
        };
        for (index, _) in self.program().files().iter().enumerate() {
            let file = FileId(index as u32);
            let Some(binding) = self.binding(file) else {
                continue;
            };
            for key in &binding.narrowing.instanceof {
                mark(self.key_root(file, &key.root), key);
            }
            for call in &binding.narrowing.calls {
                let predicate = match &call.callee {
                    Some(path) => self.predicate(file, path),
                    None => Predicate::Unknown,
                };
                if predicate == Predicate::Yes || (call.condition && predicate == Predicate::Unknown) {
                    for key in &call.keys {
                        mark(self.key_root(file, &key.root), key);
                    }
                }
            }
        }
        narrowed
    }

    /// Whether a callee written as a name path is a type predicate.
    fn predicate(&self, file: FileId, path: &NamePath) -> Predicate {
        let mut walk = Walk::default();
        let mut target = self.symbol_lookup(&mut walk, file, &path.first, path.lookup, Meanings::VALUE);
        for name in &path.rest {
            target = self.member(&mut walk, &target, name, Meanings::VALUE);
        }
        if !target.external.is_empty() || !target.excluded.is_empty() || target.ambiguous {
            return Predicate::Unknown;
        }
        let declarations: Vec<DeclRef> = target
            .declarations
            .iter()
            .copied()
            .filter(|&declaration| {
                self.decl(declaration)
                    .is_some_and(|decl| !matches!(decl.kind, DeclKind::Namespace | DeclKind::Interface))
            })
            .collect();
        if declarations.is_empty() {
            return Predicate::Unknown;
        }
        let mut unknown = false;
        for declaration in declarations {
            let returns = self
                .callable(declaration)
                .and_then(|callable| Some((callable.file, self.decl(callable)?.returns?)));
            match returns {
                None => unknown = true,
                Some((file, ty)) => {
                    let predicate = self
                        .binding(file)
                        .and_then(|binding| binding.types.get(ty as usize))
                        .is_some_and(|ty| matches!(ty, TypeRef::Predicate));
                    if predicate {
                        return Predicate::Yes;
                    }
                }
            }
        }
        if unknown {
            Predicate::Unknown
        } else {
            Predicate::No
        }
    }

    /// The narrowing key of a receiver expression: its root and member path.
    fn receiver_key(&self, file: FileId, mut index: u32) -> Option<(RootId, Vec<Box<str>>)> {
        let mut path: Vec<Box<str>> = Vec::new();
        let root = loop {
            match self.expr(file, index)? {
                Expr::Name { name, lookup } => break self.root_of(file, name, *lookup)?,
                Expr::This(context) => {
                    let (class, _) = self.binding(file)?.contexts.get(*context as usize)?.receiver?;
                    break RootId::This(DeclRef { file, index: class });
                }
                Expr::Paren(inner)
                | Expr::NonNull(inner)
                | Expr::Satisfies(inner)
                | Expr::Assert { inner, .. } => {
                    index = *inner;
                }
                Expr::Member { object, name, .. } => {
                    path.push(name.clone());
                    index = *object;
                }
                Expr::Element {
                    object,
                    key: Some(key),
                    direct: true,
                    ..
                } => {
                    path.push(key.clone());
                    index = *object;
                }
                _ => return None,
            }
        };
        path.reverse();
        Some((root, path))
    }

    /// Whether receiver expression `index` (or a receiver it is a member of) is narrowed
    /// somewhere.
    pub(crate) fn is_narrowed(&self, file: FileId, index: u32) -> bool {
        if self.tables.narrowed.is_empty() {
            return false;
        }
        let Some((root, path)) = self.receiver_key(file, index) else {
            return false;
        };
        let Some(paths) = self.tables.narrowed.get(&root) else {
            return false;
        };
        (0..=path.len()).any(|length| paths.contains(&path[..length]))
    }
}
