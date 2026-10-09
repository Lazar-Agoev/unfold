//! Per-file binding (`docs/languages/typescript.md`, "Binding"): the proof of concept's binder
//! rules (`frontend/native/names.ts`, `classes.ts`) ported onto oxc's tree.
//!
//! One walk of oxc's tree ([`visit`]) builds the file's scopes and symbols ([`scope`]), creates a
//! declaration for everything evidence can name (functions, classes, members, variables,
//! parameters, namespaces, enums, imports, the members of `const` object literals, inline
//! functions and classes), records what `this` and `super` mean at each occurrence, resolves
//! private names to their class, converts every expression into the payload's compact form
//! ([`ir`]), and collects narrowing sites and member writes. Names are looked up after the walk,
//! once every declaration of the file is known; the walk only notes where each name was used.
//! Finally the expressions no root reaches are dropped.
//!
//! `oxc_semantic` was considered and rejected for this (the S2 decision): it does not merge
//! namespace and enum declarations, so a merged namespace's members are unresolved, and it has
//! no notion of the target-dependent parameter scope.

mod callables;
mod finish;
mod forms;
mod ir;
mod members;
mod scope;
mod visit;

use oxc_allocator::Address;
use oxc_ast::ast::Program;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_core::text::ByteRange;

use crate::config::BindingContext;
use crate::payload::binding::{
    AliasRef, Binding, Context, Decl, DeclFlags, DeclKind, Expr, Lookup, Meanings, MemberWrite, ObjectFacts,
    TypeRef,
};
use crate::payload::{DeclarationKind, ModuleFacts};
use crate::syntax::{NONE, Syntax};
use crate::values;
use scope::{DeclarationSpans, ScopeKind, Scopes};

/// What binding reads besides oxc's tree.
pub(crate) struct Input<'s, 'a> {
    pub(crate) program: &'s Program<'a>,
    pub(crate) syntax: &'s Syntax<'a>,
    /// Each normalized node's block, or [`NONE`].
    pub(crate) block_of: &'s [u32],
    pub(crate) module: &'s ModuleFacts,
    pub(crate) context: BindingContext,
}

/// Binds one parsed file. Values evidence needs that the occurrence walk didn't record (the
/// callees of `const` factories) are added to `values`.
pub(crate) fn bind<'a>(input: &Input<'_, 'a>, values: &mut values::Table<'_, 'a>) -> Binding {
    let mut binder = Binder::new(input);
    binder.walk(input.program);
    binder.finish(input, values)
}

/// A class while binding.
struct ClassDraft<'a> {
    decl: u32,
    heritage: Option<u32>,
    members: Vec<u32>,
    /// Private names → the members declaring them.
    private: FxHashMap<&'a str, SmallVec<[u32; 1]>>,
}

/// What `this` inside a frame means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A non-arrow function or an object-literal method: `this` is unbound.
    Function,
    /// A class member (method, constructor, accessor, property initializer, static block).
    Member { class: u32, is_static: bool },
    /// A class member's computed key: `this` is unbound, `super` looks further out.
    MemberKey,
    /// The file, a namespace, or an enum: `this` is unbound, `super` looks further out.
    Boundary,
}

/// Where a name was used: its scope and the parameter scopes around it.
#[derive(Debug, Clone, Copy)]
struct Place {
    scope: u32,
    parameters: u32,
}

/// A narrowing key before its root name is looked up.
#[derive(Debug, Clone)]
enum RootDraft<'a> {
    Name(&'a str, Place),
    This(u32),
}

/// A narrowing key while binding.
#[derive(Debug, Clone)]
struct KeyDraft<'a> {
    root: RootDraft<'a>,
    path: Vec<Box<str>>,
}

/// A callee name path while binding.
#[derive(Debug, Clone)]
struct PathDraft<'a> {
    first: &'a str,
    place: Place,
    rest: Vec<Box<str>>,
}

/// A raw import-equals alias before its first name is looked up.
enum AliasDraft<'a> {
    Qualified {
        first: &'a str,
        place: Place,
        rest: Vec<Box<str>>,
        type_only: bool,
    },
    Dynamic {
        specifier: Box<str>,
        imported: Box<str>,
    },
    Unsupported,
}

/// The key kinds module declarations are matched by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ModuleKey {
    Function,
    Class,
    Variable,
    Enum,
    Namespace,
    Interface,
    TypeAlias,
    Expression,
}

/// The state of one file's binding.
pub(crate) struct Binder<'a> {
    context: BindingContext,
    scopes: Scopes<'a>,
    /// The current scope.
    scope: u32,
    /// The current parameter-scope set (an index into `parameter_sets`).
    parameters: u32,
    parameter_sets: Vec<SmallVec<[u32; 2]>>,
    decls: Vec<Option<Decl>>,
    /// Module declarations by key kind and range → their index.
    module_decls: FxHashMap<(ModuleKey, u32, u32), u32>,
    /// Module-scope import bindings by the start of their local name → their index.
    module_imports: FxHashMap<u32, u32>,
    /// oxc node address → normalized node.
    origins: FxHashMap<Address, u32>,
    block_of: Vec<u32>,
    frames: Vec<Frame>,
    classes: Vec<ClassDraft<'a>>,
    class_stack: Vec<u32>,
    /// The container overload signatures share (a statement list, class body, or object).
    container: u32,
    next_container: u32,
    /// Declarations that are direct statements of the file (by address).
    top_level: FxHashSet<Address>,
    /// Set while visiting the declaration of an `export` statement.
    exporting: bool,
    /// Set while inside an ambient declaration.
    ambient: bool,
    contexts: Vec<Context>,
    /// `this`/`super` start → its context.
    context_at: FxHashMap<u32, u32>,
    /// Converted expressions by node address.
    memo: FxHashMap<Address, u32>,
    exprs: Vec<Expr>,
    types: Vec<TypeRef>,
    /// Expressions that are names, and where they were used.
    name_places: Vec<(u32, Place)>,
    /// Types whose first name is looked up from a scope.
    type_places: Vec<(u32, u32)>,
    /// Object expressions that initialize a `const` → (the variable, whether it is plain).
    pending_objects: FxHashMap<Address, (u32, bool)>,
    objects: Vec<ObjectFacts>,
    aliases: Vec<AliasDraft<'a>>,
    /// Import alias symbols → what they import.
    alias_refs: FxHashMap<u32, AliasRef>,
    /// Inline function and class expressions by address → their declaration.
    inline_decls: FxHashMap<Address, u32>,
    /// `const` declarations → the address of their factory callee.
    factories: Vec<(u32, Address)>,
    /// The namespace chains of [`Lookup::Namespaced`] lookups.
    namespace_chains: std::cell::RefCell<Vec<Vec<u32>>>,
    /// Nesting depth of conditions (narrowing).
    condition: u32,
    instanceof: Vec<KeyDraft<'a>>,
    calls: Vec<(Option<PathDraft<'a>>, bool, Vec<KeyDraft<'a>>)>,
    writes: Vec<MemberWrite>,
}

impl<'a> Binder<'a> {
    fn new(input: &Input<'_, 'a>) -> Self {
        let mut scopes = Scopes::default();
        let file = scopes.push(ScopeKind::File, None);
        let mut module_decls = FxHashMap::default();
        for (index, declaration) in input.module.declarations.iter().enumerate() {
            let key = match declaration.kind {
                DeclarationKind::Module => continue,
                DeclarationKind::Function => ModuleKey::Function,
                DeclarationKind::Class => ModuleKey::Class,
                DeclarationKind::Var
                | DeclarationKind::Let
                | DeclarationKind::Const
                | DeclarationKind::Using => ModuleKey::Variable,
                DeclarationKind::Enum => ModuleKey::Enum,
                DeclarationKind::Namespace { .. } => ModuleKey::Namespace,
                DeclarationKind::Interface => ModuleKey::Interface,
                DeclarationKind::TypeAlias => ModuleKey::TypeAlias,
                DeclarationKind::Expression => ModuleKey::Expression,
            };
            module_decls
                .entry((key, declaration.range.start, declaration.range.end))
                .or_insert(index as u32);
        }
        let module_imports = input
            .module
            .imports
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.range.start, index as u32))
            .collect();
        let mut origins = FxHashMap::default();
        for &(address, node) in input.syntax.origins() {
            origins.entry(address).or_insert(node);
        }
        let mut decls = Vec::with_capacity(input.module.declarations.len() * 4);
        decls.resize_with(input.module.declarations.len(), || None);
        Self {
            context: input.context,
            scopes,
            scope: file,
            parameters: 0,
            parameter_sets: vec![SmallVec::new()],
            decls,
            module_decls,
            module_imports,
            origins,
            block_of: input.block_of.to_vec(),
            frames: vec![Frame::Boundary],
            classes: Vec::new(),
            class_stack: Vec::new(),
            container: 0,
            next_container: 1,
            top_level: FxHashSet::default(),
            exporting: false,
            ambient: false,
            contexts: Vec::new(),
            context_at: FxHashMap::default(),
            memo: FxHashMap::default(),
            exprs: Vec::new(),
            types: Vec::new(),
            name_places: Vec::new(),
            type_places: Vec::new(),
            pending_objects: FxHashMap::default(),
            objects: Vec::new(),
            aliases: Vec::new(),
            alias_refs: FxHashMap::default(),
            inline_decls: FxHashMap::default(),
            factories: Vec::new(),
            namespace_chains: std::cell::RefCell::new(Vec::new()),
            condition: 0,
            instanceof: Vec::new(),
            calls: Vec::new(),
            writes: Vec::new(),
        }
    }

    /// The block of the normalized node converted from the oxc node at `address`.
    fn block(&self, address: Address) -> Option<u32> {
        let node = *self.origins.get(&address)?;
        self.block_of
            .get(node as usize)
            .copied()
            .filter(|&block| block != NONE)
    }

    /// A fresh container id.
    fn new_container(&mut self) -> u32 {
        let id = self.next_container;
        self.next_container += 1;
        id
    }

    /// Where a name used now is.
    fn place(&self) -> Place {
        Place {
            scope: self.scope,
            parameters: self.parameters,
        }
    }

    /// The class declaration of the innermost class being bound.
    fn current_class(&self) -> Option<u32> {
        self.class_stack
            .last()
            .map(|&class| self.classes[class as usize].decl)
    }

    /// A declaration with every optional fact absent.
    fn blank(&self, kind: DeclKind, name: Option<&str>, range: ByteRange) -> Decl {
        Decl {
            kind,
            name: name.map(Box::from),
            flags: DeclFlags::default().with(DeclFlags::AMBIENT, self.ambient),
            range,
            block: None,
            container: self.container,
            owner: self.current_class(),
            symbol: None,
            var_kind: None,
            initializer: None,
            ty: None,
            returns: None,
            parameter: None,
            inline: None,
            object: None,
            constructor: None,
            factory: None,
        }
    }

    /// Adds a declaration, at its module index when it is the module declaration `key`.
    fn add(&mut self, decl: Decl, key: Option<ModuleKey>) -> u32 {
        let slot = key.and_then(|key| {
            self.module_decls
                .get(&(key, decl.range.start, decl.range.end))
                .copied()
                .filter(|&index| self.decls[index as usize].is_none())
        });
        match slot {
            Some(index) => {
                self.decls[index as usize] = Some(decl);
                index
            }
            None => {
                self.decls.push(Some(decl));
                (self.decls.len() - 1) as u32
            }
        }
    }

    fn decl_mut(&mut self, decl: u32) -> &mut Decl {
        self.decls[decl as usize]
            .as_mut()
            .expect("a declaration is filled when it is added")
    }

    fn decl(&self, decl: u32) -> Option<&Decl> {
        self.decls.get(decl as usize)?.as_ref()
    }

    /// Declares `name` for `decl` in `scope` and links them.
    fn declare(&mut self, scope: u32, name: &'a str, decl: u32, meanings: Meanings, exported: bool) -> u32 {
        let symbol = self.scopes.declare(scope, name, decl, meanings, exported);
        let found = self.decl_mut(decl);
        if found.symbol.is_none() {
            found.symbol = Some(symbol);
        }
        symbol
    }

    /// Looks `name` up where it was used.
    fn lookup(&self, name: &str, place: Place, meaning: Meanings) -> Lookup {
        let parameters = &self.parameter_sets[place.parameters as usize];
        let found = self.scopes.lookup(name, place.scope, meaning, parameters, self);
        self.namespaced(found, place.scope)
    }

    /// An unresolved lookup made inside namespaces, which other declarations of those
    /// namespaces may answer at link time.
    fn namespaced(&self, found: Lookup, scope: u32) -> Lookup {
        if found != Lookup::Unresolved {
            return found;
        }
        let crossed = self.scopes.namespaces_crossed(scope);
        if crossed.is_empty() {
            return found;
        }
        let mut chains = self.namespace_chains.borrow_mut();
        let index = match chains.iter().position(|chain| *chain == crossed) {
            Some(index) => index,
            None => {
                chains.push(crossed);
                chains.len() - 1
            }
        };
        Lookup::Namespaced(index as u32)
    }

    /// Records what a `this` (or, with `superclass`, a `super`) at `start` means here.
    fn context_here(&mut self, start: u32, superclass: bool) {
        let receiver = if superclass {
            self.frames
                .iter()
                .rev()
                .find_map(|frame| match *frame {
                    Frame::Member { class, is_static } => Some(Some((class, is_static))),
                    Frame::Function => Some(None),
                    Frame::MemberKey | Frame::Boundary => None,
                })
                .flatten()
        } else {
            match self.frames.last() {
                Some(&Frame::Member { class, is_static }) => Some((class, is_static)),
                _ => None,
            }
        };
        let index = self.contexts.len() as u32;
        self.contexts.push(Context {
            receiver,
            containing: self.current_class(),
        });
        self.context_at.insert(start, index);
    }

    /// The parameter-scope set `base` plus `separate`, when a callable keeps parameter
    /// evaluation separate from its body.
    fn parameter_set(&mut self, base: u32, separate: Option<u32>) -> u32 {
        let Some(scope) = separate else {
            return base;
        };
        let mut set = self.parameter_sets[base as usize].clone();
        set.push(scope);
        self.parameter_sets.push(set);
        (self.parameter_sets.len() - 1) as u32
    }
}

impl DeclarationSpans for Binder<'_> {
    fn variable_range(&self, declaration: u32) -> Option<ByteRange> {
        let decl = self.decl(declaration)?;
        matches!(decl.kind, DeclKind::Variable | DeclKind::BindingElement).then_some(decl.range)
    }
}
