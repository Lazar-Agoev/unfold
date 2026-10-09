//! Statements: plain statements, variables, calls, returns, throws, transfers, and the nested
//! definitions a statement lists.

use unfold_core::vocab::{BlockKind, EdgeKind, Role, SlotName};

use super::{Builder, CALL_LABEL_LENGTH, Extra, Fragment, Port};
use crate::catalog;
use crate::flow::names::clip;
use crate::flow::{FlowCallback, FlowNodeKind, RouteKind, WarningCode};
use crate::index::to_u32;

impl Builder<'_> {
    /// An unlabeled `break` or `continue` transfers to the enclosing loop. The exact block text
    /// tells a plain transfer from a labeled one without guessing the label's statement.
    pub(super) fn transfer(&mut self, local: u32) -> Fragment {
        let kind = self.block(local).kind;
        let word = kind.as_str();
        let plain = self
            .block_text(local)
            .trim()
            .strip_prefix(word)
            .is_some_and(|rest| matches!(rest.trim_start(), "" | ";"));
        let (node_kind, fallback, route, code) = if kind == BlockKind::Break {
            (
                FlowNodeKind::Break,
                "Break",
                RouteKind::Break,
                WarningCode::LabeledBreak,
            )
        } else {
            (
                FlowNodeKind::Continue,
                "Continue",
                RouteKind::Continue,
                WarningCode::LabeledContinue,
            )
        };
        if !plain {
            let label = if kind == BlockKind::Break {
                "Labeled break"
            } else {
                "Labeled continue"
            };
            return self.opaque(local, label.to_owned(), code);
        }
        let mut part = self.node(Some(local), node_kind, fallback.to_owned(), Extra::default());
        let port = Port {
            node: part.entry.unwrap_or_default(),
            kind: route,
        };
        if kind == BlockKind::Break {
            part.breaks.push(port);
        } else {
            part.continues.push(port);
        }
        part
    }

    /// The nested definitions written in statement `local`, not descending into them.
    fn callbacks(&self, local: u32) -> Vec<FlowCallback> {
        let mut callbacks = Vec::new();
        let mut at = local + 1;
        while at < self.ends[local as usize] {
            let block = self.block(at);
            if block.kind.is_function_like() || block.kind == BlockKind::Class {
                callbacks.push(FlowCallback {
                    key: block.key.clone(),
                    label: catalog::label(self.file, at).unwrap_or_default(),
                    lines: block.lines.end.saturating_sub(block.lines.start) + 1,
                    throws: self.throws(at),
                });
                at = self.ends[at as usize];
            } else {
                at += 1;
            }
        }
        callbacks
    }

    /// Whether `local` contains a `throw` outside nested callables (classes are entered).
    fn throws(&self, local: u32) -> bool {
        let mut at = local + 1;
        while at < self.ends[local as usize] {
            let block = self.block(at);
            if block.kind.is_function_like() {
                at = self.ends[at as usize];
                continue;
            }
            if block.kind == BlockKind::Throw {
                return true;
            }
            at += 1;
        }
        false
    }

    pub(super) fn statement(&mut self, local: u32) -> Fragment {
        let block = self.block(local);
        let chips = self.chips_for(local, false);
        let expression_slot = if block.kind == BlockKind::Variable {
            SlotName::Initializer
        } else {
            SlotName::Expression
        };
        let expression = self.slots(local, expression_slot).next().map(|slot| slot.range);
        let main = expression.and_then(|expression| {
            chips.iter().position(|&index| {
                let edge = &self.file.edges[index];
                matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs)
                    && self.block(edge.source).range.end == expression.end
            })
        });
        let callbacks = self.callbacks(local);
        match block.kind {
            BlockKind::Return | BlockKind::Throw => {
                let (kind, fallback) = if block.kind == BlockKind::Return {
                    (FlowNodeKind::Return, "Return")
                } else {
                    (FlowNodeKind::Throw, "Throw")
                };
                let mut part = self.node(
                    Some(local),
                    kind,
                    fallback.to_owned(),
                    Extra {
                        chips,
                        callbacks,
                        ..Extra::default()
                    },
                );
                if kind == FlowNodeKind::Return {
                    part.returns.extend(part.entry);
                }
                part
            }
            _ if main.is_some() => {
                let call = main.unwrap_or_default();
                let name = self.chip(chips[call]).name;
                let fallback = clip(if name.is_empty() { "call" } else { &name }, CALL_LABEL_LENGTH);
                self.node(
                    Some(local),
                    FlowNodeKind::Call,
                    fallback,
                    Extra {
                        chips,
                        call: Some(to_u32(call)),
                        callbacks,
                        ..Extra::default()
                    },
                )
            }
            BlockKind::Variable => {
                let bound = match &block.name {
                    Some(name) => name.to_string(),
                    None => {
                        let bindings: Vec<&str> = self
                            .children(local)
                            .iter()
                            .map(|&child| self.block(child))
                            .filter(|child| child.role == Some(Role::Binding))
                            .filter_map(|child| child.name.as_deref())
                            .collect();
                        if bindings.is_empty() {
                            self.names(&self.slot_text(local, SlotName::Name), 4).join(", ")
                        } else {
                            bindings.join(", ")
                        }
                    }
                };
                let fallback = if bound.is_empty() {
                    "Variable".to_owned()
                } else {
                    bound
                };
                self.node(
                    Some(local),
                    FlowNodeKind::Variable,
                    fallback,
                    Extra {
                        chips,
                        callbacks,
                        ..Extra::default()
                    },
                )
            }
            _ => {
                let names = self.names(self.block_text(local), 2);
                let fallback = if names.is_empty() {
                    "Statement".to_owned()
                } else {
                    names.join(", ")
                };
                self.node(
                    Some(local),
                    FlowNodeKind::Statement,
                    fallback,
                    Extra {
                        chips,
                        callbacks,
                        ..Extra::default()
                    },
                )
            }
        }
    }
}
