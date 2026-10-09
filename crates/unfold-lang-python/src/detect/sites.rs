//! The program's call sites as the detectors see them: each call expression of the binder facts
//! with its `call` block and call fact, the declaration each decorator expression decorates,
//! and the value-table entry of each recorded expression (for labels and handler targets).

use rustc_hash::FxHashMap;
use unfold_engine::{BlockRef, CallRef, FileId, ValueRef};

use crate::bind::ir::{Effect, ExprKind, NodeKind};
use crate::link::binder::Binder;
use crate::link::program::{ExprRef, NodeRef};

/// One call with a `call` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Site {
    /// The call expression.
    pub(crate) call: ExprRef,
    /// Its `call` block.
    pub(crate) block: BlockRef,
    /// Its call fact.
    pub(crate) fact: CallRef,
}

/// The program's call sites, in file order and then preorder.
#[derive(Debug, Default)]
pub(crate) struct Sites {
    sites: Vec<Site>,
    decorated: FxHashMap<ExprRef, NodeRef>,
    values: FxHashMap<ExprRef, u32>,
}

impl Sites {
    /// Indexes every Python file of the binder's program.
    pub(crate) fn new(binder: &Binder<'_>) -> Self {
        let mut sites = Self::default();
        let program = &binder.program;
        for (index, bind) in program.files.iter().enumerate() {
            let Some(bind) = bind else {
                continue;
            };
            let file = FileId(index as u32);
            let Some(facts) = program.facts.facts(file) else {
                continue;
            };
            for (value, &expr) in bind.values.iter().enumerate() {
                sites.values.entry(ExprRef { file, expr }).or_insert(value as u32);
            }
            // Each call fact's callee expression identifies its call expression.
            let mut callees: FxHashMap<u32, u32> = FxHashMap::default();
            for (position, call) in facts.graph.calls().iter().enumerate() {
                if let Some(&callee) = call.callee.and_then(|callee| bind.values.get(callee as usize)) {
                    callees.insert(callee, position as u32);
                }
            }
            for effect in &bind.effects {
                let &Effect::Call(call) = effect else {
                    continue;
                };
                let Some(ExprKind::Call { callee, .. }) =
                    bind.exprs.get(call as usize).map(|fact| &fact.kind)
                else {
                    continue;
                };
                let Some(&position) = callees.get(callee) else {
                    continue;
                };
                let Some(fact) = facts.graph.calls().get(position as usize) else {
                    continue;
                };
                sites.sites.push(Site {
                    call: ExprRef { file, expr: call },
                    block: BlockRef {
                        file,
                        local: fact.block,
                    },
                    fact: CallRef { file, call: position },
                });
            }
            for (node, fact) in bind.nodes.iter().enumerate() {
                if let NodeKind::Function(function) = &fact.kind
                    && function.name.is_some()
                {
                    for &decorator in &function.decorators {
                        let node = NodeRef {
                            file,
                            node: node as u32,
                        };
                        sites.decorated.insert(
                            ExprRef {
                                file,
                                expr: decorator,
                            },
                            node,
                        );
                    }
                }
            }
        }
        sites
    }

    /// Every call site, in file order and then preorder.
    pub(crate) fn all(&self) -> &[Site] {
        &self.sites
    }

    /// The `def` an expression decorates, when it is one of its decorators.
    pub(crate) fn decorated(&self, expr: ExprRef) -> Option<NodeRef> {
        self.decorated.get(&expr).copied()
    }

    /// The value-table entry recorded for an expression.
    pub(crate) fn value(&self, expr: ExprRef) -> Option<ValueRef> {
        self.values.get(&expr).map(|&value| ValueRef {
            file: expr.file,
            value,
        })
    }
}
