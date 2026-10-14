//! The whole-program binder: the program's facts plus everything linking learns about them
//! (mutated bindings and members, invalidated modules, receiver fields, `exec` and `eval`), the
//! class tables, and the caches that make answering evidence questions cheap.
//!
//! Linking runs in three steps:
//!
//! 1. **Program** ([`super::program`]): names, writes, bindings, star imports.
//! 2. **Mutations** ([`super::mutations`], [`super::children`]): two passes over every file's
//!    attribute and item targets and calls, in program order, as in the proof of concept. Each
//!    pass resolves values with the state the earlier steps left, so nothing is cached yet.
//! 3. **Freeze**: the class tables are rebuilt from the final state and the caches switch on.
//!    From here on, the binder is only read, from any thread.
//!
//! Cycles (a decorator whose identity needs its own decorators, a class that is its own base)
//! are cut by [`Ctx`]'s guard stack, exactly where the proof of concept cut them. A cached
//! answer is stored only when its computation didn't depend on a guard set by a caller, so a
//! cached answer never depends on where it was first asked.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_engine::{FactsSet, FileId, ProjectInputs};

use super::memo::Memo;
use super::modules::Modules;
use super::program::{ExprRef, NodeRef, Program, Sym};
use crate::bind::ir::NodeKind;

/// A class attribute or module attribute a write invalidates: a name, or every name (`*`).
pub(crate) type Invalid = FxHashSet<Sym>;

/// A field of a class's instances: written annotations and the receiver writes of its methods.
#[derive(Debug, Default, Clone)]
pub(crate) struct Field {
    pub(crate) annotations: Vec<ExprRef>,
    /// Each write's value, and whether it is in `__init__`.
    pub(crate) writes: Vec<(Option<ExprRef>, bool)>,
}

/// The key of a module whose attributes can be invalidated: a module file, a namespace folder
/// (`namespace:`), or an external module (`external:`).
pub(crate) type ModuleKey = Box<str>;

/// A cycle guard on [`Ctx`]'s stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Guard {
    Decorators(NodeRef),
    Mro(NodeRef),
    Concrete(NodeRef),
    Access(NodeRef),
}

/// The state one question carries down: the cycle guards and the recursion depth.
#[derive(Debug, Default)]
pub(crate) struct Ctx {
    stack: Vec<Guard>,
    /// The lowest stack position whose guard fired since the current cached computation
    /// started.
    low: usize,
    depth: u32,
}

/// The deepest resolution recursion; deeper chains withhold their answer.
pub(crate) const MAX_DEPTH: u32 = 256;

impl Ctx {
    pub(crate) fn new() -> Self {
        Self {
            stack: Vec::new(),
            low: usize::MAX,
            depth: 0,
        }
    }

    /// Whether `guard` is already being computed; if so, records that the current computation
    /// depends on it.
    pub(crate) fn guarded(&mut self, guard: Guard) -> bool {
        match self.stack.iter().position(|found| *found == guard) {
            Some(position) => {
                self.low = self.low.min(position);
                true
            }
            None => false,
        }
    }

    /// Runs `compute` with `guard` pushed. Returns its result and whether it is independent of
    /// guards pushed by callers (so it may be cached).
    pub(crate) fn with<T>(&mut self, guard: Guard, compute: impl FnOnce(&mut Self) -> T) -> (T, bool) {
        let position = self.stack.len();
        self.stack.push(guard);
        let saved = std::mem::replace(&mut self.low, usize::MAX);
        let result = compute(self);
        let inner = self.low;
        self.stack.pop();
        self.low = saved.min(inner);
        (result, inner >= position)
    }

    /// Enters one level of recursion; `false` when the limit is reached.
    pub(crate) fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            // A truncated answer depends on where it was asked: never cache it.
            self.low = 0;
            return false;
        }
        self.depth += 1;
        true
    }

    pub(crate) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

/// What has been seen on one resolution path (the proof of concept's `seen` sets).
#[derive(Debug, Clone, Default)]
pub(crate) struct Seen(SmallVec<[SeenKey; 8]>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeenKey {
    Expr(ExprRef),
    Node(NodeRef),
}

impl Seen {
    pub(crate) fn has(&self, key: SeenKey) -> bool {
        self.0.contains(&key)
    }

    pub(crate) fn add(&mut self, key: SeenKey) {
        if !self.has(key) {
            self.0.push(key);
        }
    }

    /// Whether the path is as long as a path may get; longer ones withhold their answer, so
    /// long chains of aliases can't exhaust the stack.
    pub(crate) fn full(&self) -> bool {
        self.0.len() >= MAX_DEPTH as usize
    }
}

/// Identities of a declaration's decorators, such as `builtins.staticmethod`; `unknown` for one
/// without a proven identity.
pub(crate) type Identities = Arc<[Box<str>]>;

/// Per-class tables, rebuilt when the binder freezes.
#[derive(Debug, Default)]
pub(crate) struct ClassTables {
    /// Each class's position in [`Program::classes`].
    pub(crate) index: FxHashMap<NodeRef, u32>,
    /// For each class, every class that descends from it through its known bases (itself
    /// included), in program order.
    pub(crate) descendants: Vec<Vec<u32>>,
    /// For each standard-library class (by qualified name), the members of the project classes
    /// that derive from it: the names their bodies bind, their instance fields, and the members
    /// written on them from outside (`*` for all).
    pub(crate) library_overrides: FxHashMap<&'static str, FxHashSet<Sym>>,
}

/// The whole-program binder.
pub(crate) struct Binder<'a> {
    pub(crate) program: Program<'a>,
    pub(crate) modules: Modules<'a>,
    /// Members written on a class from outside its own receiver (or by `setattr`).
    pub(crate) invalid_members: FxHashMap<NodeRef, Invalid>,
    /// Attributes written on a module from anywhere.
    pub(crate) invalid_modules: FxHashMap<ModuleKey, Invalid>,
    /// Further negative facts, used only to prove a child module absent from its package.
    pub(crate) invalid_children: FxHashMap<ModuleKey, Invalid>,
    /// Instance fields of each class, by name.
    pub(crate) fields: FxHashMap<NodeRef, FxHashMap<Sym, Field>>,
    pub(crate) classes: ClassTables,
    pub(crate) memo: Memo,
    /// Changes whenever the mutation passes change what values resolve to.
    pub(crate) epoch: u64,
    /// Whether the mutation passes are done, so answers can be kept for good.
    pub(crate) frozen: bool,
}

impl<'a> Binder<'a> {
    /// Binds the program.
    pub(crate) fn new(facts: &'a FactsSet, inputs: &'a ProjectInputs) -> Self {
        let mut modules = Modules::new(facts, inputs);
        let mut program = Program::new(facts, &mut modules);
        program.expand_stars(&mut modules);
        program.index_scopes();
        let mut binder = Self {
            program,
            modules,
            invalid_members: FxHashMap::default(),
            invalid_modules: FxHashMap::default(),
            invalid_children: FxHashMap::default(),
            fields: FxHashMap::default(),
            classes: ClassTables::default(),
            memo: Memo::default(),
            epoch: 0,
            frozen: false,
        };
        binder.classes = binder.class_tables();
        binder.collect_mutations();
        binder.collect_child_mutations();
        binder.classes = binder.class_tables();
        let mut decorated = Vec::new();
        for (index, file) in binder.program.files.iter().enumerate() {
            let Some(file) = file else {
                continue;
            };
            // Only declarations with decorators keep identities (`decorators` answers the rest
            // without the memo).
            for (node, fact) in file.nodes.iter().enumerate() {
                let decorated_here = match &fact.kind {
                    NodeKind::Function(function) => !function.decorators.is_empty(),
                    NodeKind::Class(class) => !class.decorators.is_empty(),
                    _ => false,
                };
                if decorated_here {
                    decorated.push(NodeRef {
                        file: FileId(index as u32),
                        node: node as u32,
                    });
                }
            }
        }
        binder.memo = Memo::frozen(binder.program.classes.len(), decorated);
        binder.frozen = true;
        binder
    }

    /// Starts a new epoch: what values resolve to may have changed.
    pub(crate) fn changed(&mut self) {
        self.epoch += 1;
    }

    /// The position of a class in the program's class list.
    pub(crate) fn class_index(&self, class: NodeRef) -> Option<u32> {
        self.classes.index.get(&class).copied()
    }
}
