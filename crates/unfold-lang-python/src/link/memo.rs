//! The binder's memo: linearizations, class concreteness, and decorator identities.
//!
//! Before the binder freezes, the mutation passes still change what values resolve to, so
//! answers are kept only for the current *epoch*: any change to an invalidated member or
//! module, a mutated binding, or an `exec`/`eval` starts a new epoch and drops them. (Instance
//! fields don't: no linearization, concreteness, or decorator identity reads them.) Once the
//! binder is frozen, answers are kept for good in thread-safe cells, filled by whichever query
//! asks first; [`super::binder::Ctx`] makes sure only answers that don't depend on the asker
//! are kept, so which query is first never matters.

use std::sync::{Arc, Mutex, OnceLock};

use rustc_hash::FxHashMap;

use super::binder::{Binder, Identities};
use super::program::NodeRef;

/// A linearization, or `None` when it can't be proven.
pub(crate) type Mro = Option<Arc<[NodeRef]>>;

#[derive(Debug, Default)]
struct Epoch {
    epoch: u64,
    mro: FxHashMap<NodeRef, Mro>,
    concrete: FxHashMap<NodeRef, bool>,
    decorators: FxHashMap<NodeRef, Identities>,
}

/// The memo's two layers.
#[derive(Debug, Default)]
pub(crate) struct Memo {
    /// Answers of the current epoch, before freezing.
    early: Mutex<Epoch>,
    /// Answers after freezing, by class position.
    mro: Vec<OnceLock<Mro>>,
    concrete: Vec<OnceLock<bool>>,
    decorators: FxHashMap<NodeRef, OnceLock<Identities>>,
}

impl Memo {
    /// Cells for `classes` classes and these decorated declarations.
    pub(crate) fn frozen(classes: usize, decorated: impl IntoIterator<Item = NodeRef>) -> Self {
        Self {
            early: Mutex::default(),
            mro: (0..classes).map(|_| OnceLock::new()).collect(),
            concrete: (0..classes).map(|_| OnceLock::new()).collect(),
            decorators: decorated
                .into_iter()
                .map(|node| (node, OnceLock::new()))
                .collect(),
        }
    }
}

impl Binder<'_> {
    /// Runs `read` on the current epoch's answers.
    fn early<T>(&self, read: impl FnOnce(&mut Epoch) -> T) -> T {
        let mut early = match self.memo.early.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if early.epoch != self.epoch {
            *early = Epoch {
                epoch: self.epoch,
                ..Epoch::default()
            };
        }
        read(&mut early)
    }

    pub(crate) fn memo_mro(&self, class: NodeRef) -> Option<Mro> {
        if self.frozen {
            let index = self.class_index(class)?;
            return self.memo.mro.get(index as usize)?.get().cloned();
        }
        self.early(|early| early.mro.get(&class).cloned())
    }

    pub(crate) fn keep_mro(&self, class: NodeRef, mro: Mro) {
        if self.frozen {
            if let Some(cell) = self
                .class_index(class)
                .and_then(|index| self.memo.mro.get(index as usize))
            {
                let _ = cell.set(mro);
            }
        } else {
            self.early(|early| early.mro.insert(class, mro));
        }
    }

    pub(crate) fn memo_concrete(&self, class: NodeRef) -> Option<bool> {
        if self.frozen {
            let index = self.class_index(class)?;
            return self.memo.concrete.get(index as usize)?.get().copied();
        }
        self.early(|early| early.concrete.get(&class).copied())
    }

    pub(crate) fn keep_concrete(&self, class: NodeRef, concrete: bool) {
        if self.frozen {
            if let Some(cell) = self
                .class_index(class)
                .and_then(|index| self.memo.concrete.get(index as usize))
            {
                let _ = cell.set(concrete);
            }
        } else {
            self.early(|early| early.concrete.insert(class, concrete));
        }
    }

    pub(crate) fn memo_decorators(&self, node: NodeRef) -> Option<Identities> {
        if self.frozen {
            return self.memo.decorators.get(&node)?.get().cloned();
        }
        self.early(|early| early.decorators.get(&node).cloned())
    }

    pub(crate) fn keep_decorators(&self, node: NodeRef, identities: Identities) {
        if self.frozen {
            if let Some(cell) = self.memo.decorators.get(&node) {
                let _ = cell.set(identities);
            }
        } else {
            self.early(|early| early.decorators.insert(node, identities));
        }
    }
}
