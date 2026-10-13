//! The contract of one selected line: a resolved call or construction (`seq` 0).
//!
//! - **Sends**: each argument in written order, with its written text (from the caller's
//!   verified source), expansion, keyword, and the parameter it fills or why it fills none. Then
//!   each parameter no argument filled, when the pairings prove which ones were filled (not when
//!   an argument is expanded or its receiver binding is unknown) and the call's bound count is
//!   proven: the first `bound` parameters, which the call binds itself (a Python method's `self`
//!   reached through an instance), are left out. Without a proven bound count no unfilled
//!   parameter is listed.
//! - **Returns**: where the value lands (the variable's name, or the outer call's written callee),
//!   whether it is awaited, and the callee's return type (a constructed class's name).
//! - **Throws**: the callee's direct throws ([`direct_throws`]).
//! - **Caught**: the call sits in the `body` of a `try` that has a `catch` handler, in the
//!   caller's own function.
//! - **Callee lines**: the lines of the callee's parameter and return-type slots (read from its
//!   verified source; none when it changed) and each listed throw's line.

use std::collections::BTreeSet;
use std::rc::Rc;

use unfold_core::text::short_label;
use unfold_core::vocab::{BlockKind, EdgeKind, PairingReason, Resolution, ResultBinding, Role, SlotName};
use unfold_core::{Edge, Key, Pairing, Parameter};

use super::facts::{Facts, FileFacts};
use super::text::{outcome_span, whole_text};
use super::{
    CallContract, ContractParameter, ContractReturn, ContractSend, ContractThrow, SentArgument, TypeText,
    UnpairedArgument,
};
use crate::error::Result;
use crate::notes::pairing_text;

/// A resolved edge's callee, located in its file.
struct Callee {
    file: Rc<FileFacts>,
    local: u32,
    key: Key,
}

impl Callee {
    /// The callee of `edge`, when it resolved to a block that exists.
    fn of(facts: &mut Facts<'_>, edge: &Edge) -> Result<Option<Self>> {
        let Some(key) = edge
            .target
            .as_ref()
            .filter(|_| edge.resolution == Resolution::Resolved)
        else {
            return Ok(None);
        };
        Ok(facts.locate(key.as_str())?.map(|(file, local)| Self {
            file,
            local,
            key: key.clone(),
        }))
    }

    /// The class an edge of `kind` to this callee constructs: a class target itself, or the
    /// class of a constructor (or of the method a Python construction reaches, `__init__`).
    fn constructed_class(&self, kind: EdgeKind) -> Option<u32> {
        class_of(&self.file, self.local, kind)
    }

    /// What a call of kind `kind` gives back: the constructed class's name, else the written
    /// return type.
    fn output(&self, kind: EdgeKind) -> Option<TypeText> {
        match self.constructed_class(kind) {
            Some(class) => Some(class_name(&self.file, class)),
            None => self
                .file
                .signature(self.local)
                .and_then(|signature| signature.returns.as_ref())
                .map(|returns| self.file.type_text(returns)),
        }
    }
}

/// The class block `local` of `file` stands for when reached by an edge of `kind`: itself for a
/// class, its parent class for a constructor, or for a method reached by a construction.
fn class_of(file: &FileFacts, local: u32, kind: EdgeKind) -> Option<u32> {
    let block = file.block(local)?;
    let parent_class = || {
        file.parent(local).filter(|&parent| {
            file.block(parent)
                .is_some_and(|parent| parent.kind == BlockKind::Class)
        })
    };
    match block.kind {
        BlockKind::Class => Some(local),
        BlockKind::Constructor => parent_class(),
        BlockKind::Method if kind == EdgeKind::Constructs => parent_class(),
        _ => None,
    }
}

/// The name of class `class` as a `declared` type.
fn class_name(file: &FileFacts, class: u32) -> TypeText {
    let key = file.block(class).map(|block| block.key.clone());
    whole_text(&file.label(class), |end| {
        outcome_span(0, end, Resolution::Resolved, key.as_ref(), None, None)
    })
}

/// Whether block `local` of `file` sits in the `body` of a `try` that has a `catch` handler,
/// below `scope` (an ancestor: the function whose throws or calls are asked about).
pub(super) fn guarded(file: &FileFacts, local: u32, scope: u32) -> bool {
    let mut child = local;
    while let Some(parent) = file.parent(child) {
        if parent == scope {
            break;
        }
        let in_body = file
            .block(child)
            .is_some_and(|block| block.role == Some(Role::Body));
        let handled = file
            .block(parent)
            .is_some_and(|block| block.kind == BlockKind::Try)
            && file.children(parent).any(|handler| {
                file.block(handler)
                    .is_some_and(|block| block.kind == BlockKind::Catch)
            });
        if in_body && handled {
            return true;
        }
        child = parent;
    }
    false
}

/// The direct throws of function-like block `function` of `file`: every `throw` it owns (not
/// one inside a nested callable) whose expression is exactly a construction with a `constructs`
/// edge (`throw new X(…)`, `raise X(…)`), and which isn't in the body of a `try` with a `catch`
/// handler in the same function. A resolved construction names its class (a constructor's
/// parent); an external or unresolved one keeps its written text and its module or reason. Bare
/// `throw error`, `raise`, and `raise Name` construct nothing and are not listed.
pub(super) fn direct_throws(
    facts: &mut Facts<'_>,
    file: &FileFacts,
    function: u32,
) -> Result<Vec<ContractThrow>> {
    if !file
        .block(function)
        .is_some_and(|block| block.kind.is_function_like())
    {
        return Ok(Vec::new());
    }
    let mut throws = Vec::new();
    for local in file.descendants(function) {
        let Some(block) = file.block(local) else {
            continue;
        };
        if block.kind != BlockKind::Throw || file.owner(local) != function || guarded(file, local, function) {
            continue;
        }
        let Some(expression) = file.slot(local, SlotName::Expression, None) else {
            continue;
        };
        let construction = file.children(local).find(|&child| {
            file.block(child).is_some_and(|child| {
                matches!(child.kind, BlockKind::New | BlockKind::Call)
                    && child.role == Some(Role::Expression)
                    && child.range == expression.range
            })
        });
        let Some(edge) = construction
            .and_then(|construction| file.edge(construction, 0))
            .filter(|edge| edge.kind == EdgeKind::Constructs)
        else {
            continue;
        };
        let class = match Callee::of(facts, edge)? {
            Some(callee) => class_of(&callee.file, callee.local, EdgeKind::Constructs)
                .map(|class| class_name(&callee.file, class)),
            None => None,
        };
        let ty = class.unwrap_or_else(|| {
            whole_text(&edge.text, |end| {
                outcome_span(0, end, edge.resolution, None, edge.reason, edge.module.as_ref())
            })
        });
        throws.push(ContractThrow {
            ty,
            line: block.lines.start,
        });
    }
    Ok(throws)
}

/// A callee parameter as the contract shows it.
fn contract_parameter(file: &FileFacts, parameter: &Parameter) -> ContractParameter {
    ContractParameter {
        index: parameter.index,
        name: parameter.name.as_deref().map(str::to_owned),
        kind: parameter.kind,
        optional: parameter.optional,
        default: parameter.default,
        ty: parameter.ty.as_ref().map(|ty| file.type_text(ty)),
    }
}

/// How many leading parameters the call at `call` binds itself, when linking proved it.
fn bound_count(caller: &FileFacts, call: u32) -> Option<u32> {
    caller.result(call)?.bound
}

/// The contract of the arrow for edge `seq` of occurrence `source`. `None` when the occurrence
/// doesn't exist or the edge isn't a resolved call or construction (`seq` 0) of a block that
/// exists.
pub fn call_contract(facts: &mut Facts<'_>, source: &str, seq: u32) -> Result<Option<CallContract>> {
    let Some((caller, call)) = facts.locate(source)? else {
        return Ok(None);
    };
    let Some(edge) = caller
        .edge(call, seq)
        .filter(|edge| seq == 0 && matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs))
    else {
        return Ok(None);
    };
    let Some(callee) = Callee::of(facts, edge)? else {
        return Ok(None);
    };
    let Some(occurrence) = caller.block(call) else {
        return Ok(None);
    };
    let caller_text = facts.text(&caller);

    let arguments = caller.arguments(call);
    let mut sends: Vec<ContractSend> = arguments
        .iter()
        .map(|argument| {
            let text = caller_text.as_ref().and_then(|snapshot| {
                let slot = caller.slot(call, SlotName::Argument, Some(argument.index))?;
                snapshot
                    .text()
                    .get(slot.range.as_usize())
                    .map(|text| short_label(text).to_owned())
            });
            let (parameter, unpaired) = match argument.pairing {
                Some(Pairing::Parameter(index)) => (
                    callee
                        .file
                        .parameter(callee.local, index)
                        .map(|parameter| contract_parameter(&callee.file, parameter)),
                    None,
                ),
                Some(Pairing::Unpaired(reason)) => (
                    None,
                    Some(UnpairedArgument {
                        reason,
                        note: pairing_text(reason).to_owned(),
                    }),
                ),
                None => (None, None),
            };
            ContractSend {
                argument: Some(SentArgument {
                    index: argument.index,
                    text,
                    expand: argument.expand,
                    keyword: argument.keyword.as_deref().map(str::to_owned),
                }),
                parameter,
                unpaired,
            }
        })
        .collect();

    // Unfilled parameters, only when the pairings say which parameters were filled.
    let known = arguments.iter().all(|argument| {
        matches!(
            argument.pairing,
            Some(Pairing::Parameter(_) | Pairing::Unpaired(PairingReason::NoParameter))
        )
    });
    if known && let Some(bound) = bound_count(&caller, call) {
        let filled: BTreeSet<u32> = arguments
            .iter()
            .filter_map(|argument| match argument.pairing {
                Some(Pairing::Parameter(index)) => Some(index),
                _ => None,
            })
            .collect();
        sends.extend(
            callee
                .file
                .parameters(callee.local)
                .iter()
                .filter(|parameter| {
                    parameter.kind.takes_argument()
                        && parameter.index >= bound
                        && !filled.contains(&parameter.index)
                })
                .map(|parameter| ContractSend {
                    argument: None,
                    parameter: Some(contract_parameter(&callee.file, parameter)),
                    unpaired: None,
                }),
        );
    }

    let result = caller.result(call);
    let binding = result.map_or(ResultBinding::Other, |result| result.binding);
    let text = result.and_then(|result| {
        let target = result.target?;
        match binding {
            ResultBinding::Variable => caller.block(target)?.name.as_deref().map(str::to_owned),
            ResultBinding::Argument => caller.edge(target, 0).map(|edge| edge.text.to_string()),
            ResultBinding::Return | ResultBinding::Discard | ResultBinding::Other => None,
        }
    });
    let returns = ContractReturn {
        binding,
        text,
        awaited: result.is_some_and(|result| result.awaited),
        ty: callee.output(edge.kind),
    };

    let throws = direct_throws(facts, &callee.file, callee.local)?;
    let mut lines: BTreeSet<u32> = throws.iter().map(|thrown| thrown.line).collect();
    if let Some(snapshot) = facts.text(&callee.file) {
        let index = snapshot.line_index();
        for slot in callee.file.slots(callee.local) {
            if matches!(slot.slot, SlotName::Parameter | SlotName::ReturnType) {
                let span = index.span(slot.range);
                lines.extend(span.start..=span.end);
            }
        }
    }

    Ok(Some(CallContract {
        source: occurrence.key.clone(),
        seq,
        target: callee.key.clone(),
        caller_line: occurrence.lines.start,
        sends,
        returns,
        throws,
        caught: guarded(&caller, call, caller.owner(call)),
        callee_lines: lines.into_iter().collect(),
    }))
}
