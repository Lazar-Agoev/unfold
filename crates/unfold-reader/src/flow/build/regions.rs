//! Regions: conditions and `else if` chains, loops, and `try`/`catch`.

use unfold_core::vocab::{BlockKind, Language, Role, SlotName};

use super::{Builder, Extra, Fragment, Port};
use crate::flow::{ChainOutcome, FlowItem, FlowNodeKind, RouteKind, WarningCode};

impl Builder<'_> {
    /// The `if` that continues an `else if` chain after `local`, if any.
    fn else_if(&self, local: u32) -> Option<u32> {
        match self.children_with(local, Role::Else).as_slice() {
            &[only] if self.block(only).kind == BlockKind::If => Some(only),
            _ => None,
        }
    }

    pub(super) fn condition(&mut self, local: u32) -> Fragment {
        let names = self.names(&self.slot_text(local, SlotName::Condition), 4);
        let fallback = if names.is_empty() {
            "Condition?".to_owned()
        } else {
            format!("{}?", names.join(", "))
        };
        let chips = self.chips_for(local, true);
        let gate = self.node(
            Some(local),
            FlowNodeKind::Condition,
            fallback,
            Extra {
                chips,
                ..Extra::default()
            },
        );
        let gate_id = gate.entry.unwrap_or_default();
        let mut yes = self.sequence(&self.children_with(local, Role::Then));
        let else_blocks = self.children_with(local, Role::Else);
        let else_if = self.else_if(local).is_some();
        let mut no = self.sequence(&else_blocks);

        // A long `else if` chain folds into one condition node in the UI.
        let mut checks = 1;
        let mut outcomes: Vec<ChainOutcome> = Vec::new();
        let mut cursor = Some(local);
        while let Some(at) = cursor {
            let last = self
                .children_with(at, Role::Then)
                .last()
                .map(|&last| self.block(last).kind);
            let outcome = match last {
                Some(BlockKind::Throw) => ChainOutcome::Throw,
                Some(BlockKind::Return) => ChainOutcome::Return,
                _ => ChainOutcome::Mixed,
            };
            if !outcomes.contains(&outcome) {
                outcomes.push(outcome);
            }
            cursor = self.else_if(at);
            if cursor.is_some() {
                checks += 1;
            }
        }
        if checks >= 3 {
            let mut all: Vec<String> = Vec::new();
            let mut at = Some(local);
            while let Some(check) = at {
                for name in self.names(&self.slot_text(check, SlotName::Condition), 99) {
                    if !all.contains(&name) {
                        all.push(name);
                    }
                }
                at = self.else_if(check);
            }
            if all.len() > 4 {
                all.truncate(4);
                all.push("…".to_owned());
            }
            let node = &mut self.nodes[gate_id as usize];
            if node.fallback {
                node.label = format!("{}?", all.join(", "));
            }
            node.checks = Some(checks);
            node.outcome = Some(match outcomes.as_slice() {
                [only] => *only,
                _ => ChainOutcome::Mixed,
            });
        }

        let yes_ends = yes.ends;
        let no_ends = no.entry.is_some() && no.ends;
        let no_below = no.entry.is_some() && yes_ends;
        let yes_lane = if no.entry.is_some() && !no_below {
            no.lanes
        } else {
            1
        };
        let ends = yes_ends && no_ends;
        let lanes = (yes_lane + yes.lanes).max(if no_below { no.lanes } else { 1 });
        if let Some(entry) = yes.entry {
            self.route(gate_id, entry, RouteKind::Yes);
        }
        if let Some(entry) = no.entry {
            self.route(
                gate_id,
                entry,
                if else_if { RouteKind::ElseIf } else { RouteKind::No },
            );
        }
        let mut exits = if yes.entry.is_some() {
            std::mem::take(&mut yes.exits)
        } else {
            vec![Port {
                node: gate_id,
                kind: RouteKind::Yes,
            }]
        };
        if no.entry.is_some() {
            exits.append(&mut no.exits);
        } else {
            exits.push(Port {
                node: gate_id,
                kind: RouteKind::No,
            });
        }
        let mut result = Fragment {
            items: vec![FlowItem::Branch {
                node: gate_id,
                yes: std::mem::take(&mut yes.items),
                no: std::mem::take(&mut no.items),
                yes_ends,
                no_ends,
                ends,
                lanes,
                yes_lane,
                else_if,
            }],
            entry: Some(gate_id),
            exits,
            ends,
            lanes,
            ..Fragment::empty()
        };
        result.absorb(&mut yes);
        result.absorb(&mut no);
        result
    }

    pub(super) fn loop_region(&mut self, local: u32) -> Fragment {
        let python = self.language == Language::Python;
        if python
            && (self.has_slot(local, SlotName::Else) || !self.children_with(local, Role::Else).is_empty())
        {
            return self.opaque(local, "Loop with else".to_owned(), WarningCode::PythonLoopElse);
        }
        let mut body = self.sequence(&self.children_with(local, Role::Body));
        let test_after = self.block(local).kind == BlockKind::DoWhile;
        let header = if test_after {
            self.slot_text(local, SlotName::Condition)
        } else {
            [SlotName::Initializer, SlotName::Iterable, SlotName::Condition]
                .iter()
                .map(|&slot| self.slot_text(local, slot))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let names = self.names(&header, 4);
        let fallback = if names.is_empty() {
            "Loop".to_owned()
        } else {
            names.join(", ")
        };
        let chips = self.chips_for(local, true);
        let gate = self
            .node(
                Some(local),
                FlowNodeKind::Loop,
                fallback,
                Extra {
                    chips,
                    ..Extra::default()
                },
            )
            .entry
            .unwrap_or_default();
        let (into, back) = if test_after {
            (RouteKind::Again, RouteKind::Test)
        } else {
            (RouteKind::Each, RouteKind::Next)
        };
        match body.entry {
            Some(entry) => {
                self.route(gate, entry, into);
                for port in std::mem::take(&mut body.exits) {
                    let kind = if port.kind == RouteKind::Sequence {
                        back
                    } else {
                        port.kind
                    };
                    self.route(port.node, gate, kind);
                }
            }
            None => self.route(
                gate,
                gate,
                if test_after {
                    RouteKind::Again
                } else {
                    RouteKind::Next
                },
            ),
        }
        for port in std::mem::take(&mut body.continues) {
            self.route(port.node, gate, RouteKind::Continue);
        }
        let lanes = body.lanes.max(1);
        let mut exits = vec![Port {
            node: gate,
            kind: RouteKind::Done,
        }];
        exits.append(&mut body.breaks);
        Fragment {
            items: vec![FlowItem::Loop {
                node: gate,
                body: std::mem::take(&mut body.items),
                test_after,
                ends: false,
                lanes,
            }],
            entry: if test_after {
                body.entry.or(Some(gate))
            } else {
                Some(gate)
            },
            exits,
            returns: std::mem::take(&mut body.returns),
            lanes,
            ..Fragment::empty()
        }
    }

    pub(super) fn attempt(&mut self, local: u32) -> Fragment {
        if self.language == Language::Python {
            return self.opaque(local, "try / except".to_owned(), WarningCode::PythonTry);
        }
        let handler = self
            .children(local)
            .iter()
            .copied()
            .find(|&child| self.block(child).kind == BlockKind::Catch);
        let finally =
            self.has_slot(local, SlotName::Finally) || !self.children_with(local, Role::Finally).is_empty();
        let Some(handler) = handler.filter(|_| !finally) else {
            return self.opaque(local, "try … finally".to_owned(), WarningCode::TryFinally);
        };
        let mut body = self.sequence(&self.children_with(local, Role::Body));
        let mut caught = self.sequence(&self.children_with(handler, Role::Body));
        let binding = self
            .names(&self.slot_text(handler, SlotName::Binding), 1)
            .into_iter()
            .next();
        if let (Some(from), Some(to)) = (body.entry, caught.entry) {
            self.route(from, to, RouteKind::Error);
        }
        let catch_lane = body.lanes.max(1);
        let ends = body.ends && caught.ends;
        let lanes = catch_lane + caught.lanes.max(1);
        let mut exits = std::mem::take(&mut body.exits);
        exits.append(&mut caught.exits);
        let mut result = Fragment {
            items: vec![FlowItem::Try {
                key: self.block(local).key.clone(),
                catch_key: self.block(handler).key.clone(),
                binding,
                body: std::mem::take(&mut body.items),
                handler: std::mem::take(&mut caught.items),
                body_ends: body.ends,
                handler_ends: caught.ends,
                ends,
                lanes,
                catch_lane,
            }],
            entry: body.entry.or(caught.entry),
            exits,
            ends,
            lanes,
            ..Fragment::empty()
        };
        if body.entry.is_none() {
            self.warn(WarningCode::EmptyTry);
        }
        result.absorb(&mut body);
        result.absorb(&mut caught);
        result
    }
}
