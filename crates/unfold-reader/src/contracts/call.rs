//! A call's contract, by its block key.
//!
//! - **Sends**: each argument in written order, with its written text (from the caller's
//!   verified source), expansion, keyword, and the parameter it fills (with that parameter's
//!   written type) or why it fills none. Then each parameter no argument filled, when the
//!   pairings prove which ones were filled (no argument is expanded or of unknown binding) and
//!   the call's bound count is proven: the first `bound` parameters, which the call binds itself
//!   (a Python method's `self` reached through an instance), are left out.
//! - **Returns**: the callee's written return type, or, for a construction, the class it
//!   constructs; whether the call is awaited; and where the value lands: what it `binds`, the
//!   exit whose value it is, the outer call it is an argument or receiver of, or nowhere.
//! - **Raises**: every `raise` and `throw` exit that belongs to a project callee (a class's
//!   `constructor`, for a class) and that the callee doesn't catch itself, with its value and, when the value is exactly a construction,
//!   what it constructs.
//! - **Caught**: for each raise, the first enclosing `except` or `catch` part of the caller
//!   (inside the caller's own definition, around the `body` of its `try`) that catches it.
//!
//! **What a handler catches**, by written facts only: a TypeScript `catch`, or a bare
//! `except:`, catches everything. A bare `except` is told by its recorded head, which is the
//! clause through its type and name: a head that covers only the `except` keyword writes no
//! type expression. Any other `except` writes one, resolved or not (`except ERRORS[0]:` names
//! no class the graph can follow, so it catches nothing proven). Otherwise a handler catches a
//! raise when one of its `types` links reaches the raised class, or a class it extends through
//! project `extends` links, or (for a library or builtin class, such as `ValueError`) names the
//! same module and the same written name. A raise whose class is
//! unknown is caught only by a handler that catches everything.

use std::collections::BTreeSet;
use std::rc::Rc;

use rustc_hash::FxHashSet;
use unfold_core::text::short_label;
use unfold_core::vocab::{Flag, Form, LinkKind, NodeKind, Outcome, PairingReason};
use unfold_core::{Key, Link, LinkTarget, ModuleLabel, Pairing, Parameter};

use super::facts::Facts;
use super::text::{type_text, whole_text};
use super::{
    CallContract, ContractParameter, ContractRaise, ContractReturn, ContractSend, Landing, SentArgument,
    TypeText, UnpairedArgument,
};
use crate::error::Result;
use crate::notes::{LinkEnd, pairing_text};
use crate::tree::FileTree;

/// How many `extends` links a catch follows up from a raised class.
const MAX_BASE_DEPTH: usize = 8;

/// What a raise raises, for matching it against handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Raised {
    /// A project class.
    Class(Key),
    /// A library or builtin class: its outcome, module, and written name.
    Outside(Outcome, ModuleLabel, Box<str>),
    /// Nothing proven: a raise of a value, a bare `raise`, or an unknown construction.
    Unknown,
}

/// The class a construction of `target` (a class, or a constructor or `__init__` method)
/// builds, located: the target itself when it is a class, else its parent when that is a class.
fn constructed(file: &FileTree, target: u32) -> Option<u32> {
    match file.kind(target) {
        NodeKind::Class => Some(target),
        NodeKind::Function => file
            .parent(target)
            .filter(|&parent| file.kind(parent) == NodeKind::Class),
        _ => None,
    }
}

/// The class `link` (a construction's `calls` link) constructs, as a raise to match.
fn raised(facts: &mut Facts<'_>, link: &Link) -> Result<Raised> {
    Ok(match &link.to {
        LinkTarget::Project(key) => match facts.locate(key.as_str())? {
            Some((file, target)) => match constructed(&file, target) {
                Some(class) => Raised::Class(file.key(class).clone()),
                None => Raised::Unknown,
            },
            None => Raised::Unknown,
        },
        LinkTarget::Library(module) => Raised::Outside(Outcome::Library, module.clone(), link.text.clone()),
        LinkTarget::Builtin(module) => Raised::Outside(Outcome::Builtin, module.clone(), link.text.clone()),
        LinkTarget::Unknown(_) => Raised::Unknown,
    })
}

/// Whether a handler's `types` link names `raised` exactly.
fn names(link: &Link, raised: &Raised) -> bool {
    match (&link.to, raised) {
        (LinkTarget::Project(key), Raised::Class(class)) => key == class,
        (LinkTarget::Library(module), Raised::Outside(Outcome::Library, raised, text))
        | (LinkTarget::Builtin(module), Raised::Outside(Outcome::Builtin, raised, text)) => {
            module == raised && link.text == *text
        }
        _ => false,
    }
}

/// `raised` and what it extends through project `extends` links (each class's own `extends`
/// links, which name its bases), breadth first, at most [`MAX_BASE_DEPTH`] levels up.
fn lineage(facts: &mut Facts<'_>, raised: Raised) -> Result<Vec<Raised>> {
    let mut lineage = vec![raised];
    let mut seen: FxHashSet<Key> = FxHashSet::default();
    let mut level = 0..lineage.len();
    for _ in 0..MAX_BASE_DEPTH {
        let mut next = Vec::new();
        for index in level.clone() {
            let Raised::Class(class) = &lineage[index] else {
                continue;
            };
            if !seen.insert(class.clone()) {
                continue;
            }
            let Some((file, id)) = facts.locate(class.as_str())? else {
                continue;
            };
            for link in file
                .links(id)
                .iter()
                .filter(|link| link.kind == LinkKind::Extends)
            {
                next.push(match &link.to {
                    LinkTarget::Project(base) => Raised::Class(base.clone()),
                    LinkTarget::Library(module) => {
                        Raised::Outside(Outcome::Library, module.clone(), link.text.clone())
                    }
                    LinkTarget::Builtin(module) => {
                        Raised::Outside(Outcome::Builtin, module.clone(), link.text.clone())
                    }
                    LinkTarget::Unknown(_) => continue,
                });
            }
        }
        if next.is_empty() {
            break;
        }
        let start = lineage.len();
        lineage.extend(next);
        level = start..lineage.len();
    }
    Ok(lineage)
}

/// Whether `except` part `handler` is bare: its head (the clause) covers only its keyword.
fn is_bare_except(file: &FileTree, handler: u32) -> bool {
    file.node(handler)
        .head
        .is_some_and(|head| head.len() as usize == Form::Except.as_str().len())
}

/// Whether handler part `handler` of `file` catches a raise whose lineage is `lineage`.
fn catches(file: &FileTree, handler: u32, lineage: &[Raised]) -> bool {
    match file.node(handler).form {
        Form::Catch => true,
        Form::Except if is_bare_except(file, handler) => true,
        Form::Except => file
            .links(handler)
            .iter()
            .filter(|link| link.kind == LinkKind::Types)
            .any(|link| lineage.iter().any(|raised| names(link, raised))),
        _ => false,
    }
}

/// The first `except` or `catch` part that catches a raise of `lineage` at node `id` of
/// `file`: walking up to `scope` (the definition the code belongs to), around the `body` of
/// each enclosing `try`, handlers in written order.
fn handler_for(file: &FileTree, id: u32, scope: u32, lineage: &[Raised]) -> Option<u32> {
    let mut child = id;
    while let Some(parent) = file.parent(child) {
        if parent == scope {
            break;
        }
        let in_body = file.node(child).form == Form::Body && file.kind(parent) == NodeKind::Try;
        if in_body
            && let Some(&handler) = file
                .children(parent)
                .iter()
                .find(|&&part| catches(file, part, lineage))
        {
            return Some(handler);
        }
        child = parent;
    }
    None
}

/// The value expression of exit `exit`, when it is exactly a call: its child flagged
/// `exit_value`.
fn exit_call(file: &FileTree, exit: u32) -> Option<u32> {
    file.children(exit)
        .iter()
        .copied()
        .find(|&child| file.kind(child) == NodeKind::Call && file.node(child).flags.contains(Flag::ExitValue))
}

/// A raise as it reads: what it constructs, and its value as written.
struct Raise {
    exit: u32,
    lineage: Vec<Raised>,
    ty: Option<TypeText>,
    value: Option<String>,
}

/// The raises of function `function` of `callee` that it doesn't catch itself.
fn raises(facts: &mut Facts<'_>, callee: &Rc<FileTree>, function: u32) -> Result<Vec<Raise>> {
    let text = facts.text(callee);
    let mut found = Vec::new();
    let exits: Vec<u32> = callee
        .owned(function)
        .filter(|&id| matches!(callee.node(id).form, Form::Raise | Form::Throw))
        .collect();
    for exit in exits {
        let construction = exit_call(callee, exit).and_then(|call| callee.calls_link(call));
        let (raise, ty) = match construction {
            Some(link) => {
                let raise = raised(facts, link)?;
                let ty = match &raise {
                    Raised::Class(class) => {
                        let label = match facts.locate(class.as_str())? {
                            Some((file, id)) => file.label(id),
                            None => class.to_string(),
                        };
                        whole_text(&label, &LinkTarget::Project(class.clone()))
                    }
                    Raised::Outside(..) | Raised::Unknown => whole_text(&link.text, &link.to),
                };
                (raise, Some(ty))
            }
            None => (Raised::Unknown, None),
        };
        let lineage = lineage(facts, raise)?;
        if handler_for(callee, exit, function, &lineage).is_some() {
            continue;
        }
        let value = callee.node(exit).value.and_then(|range| {
            let text = text.as_ref()?;
            text.slice(range).ok().map(|value| short_label(value).to_owned())
        });
        found.push(Raise {
            exit,
            lineage,
            ty,
            value,
        });
    }
    Ok(found)
}

/// The function whose raises a call of `target` meets: a function itself, or a class's
/// `constructor`.
fn raising(file: &FileTree, target: u32) -> Option<u32> {
    match file.kind(target) {
        NodeKind::Function => Some(target),
        NodeKind::Class => file
            .children(target)
            .iter()
            .copied()
            .find(|&child| file.node(child).form == Form::Constructor),
        _ => None,
    }
}

/// A callee parameter as the contract shows it.
fn contract_parameter(file: &FileTree, parameter: &Parameter) -> ContractParameter {
    ContractParameter {
        index: parameter.index,
        name: parameter.name.as_deref().map(str::to_owned),
        kind: parameter.kind,
        optional: parameter.optional,
        default: parameter.default,
        ty: parameter
            .ty
            .as_ref()
            .map(|ty| type_text(ty, file.type_links_in(ty.range))),
    }
}

/// Where the value of call `call` of `file` lands.
fn landing(file: &FileTree, call: u32) -> Landing {
    let node = file.node(call);
    if let Some(target) = &node.binds {
        return Landing::Binds {
            target: target.to_string(),
        };
    }
    let Some(parent) = file.parent(call) else {
        return Landing::Nowhere;
    };
    match file.kind(parent) {
        NodeKind::Exit if node.flags.contains(Flag::ExitValue) => Landing::Exit {
            exit: file.key(parent).clone(),
            form: file.node(parent).form,
        },
        NodeKind::Call => {
            let outer = file.key(parent).clone();
            match file
                .arguments(parent)
                .iter()
                .find(|argument| argument.range.contains(node.range))
            {
                Some(argument) => Landing::Argument {
                    call: outer,
                    index: argument.index,
                },
                None => Landing::Receiver { call: outer },
            }
        }
        _ => Landing::Nowhere,
    }
}

/// The contract of the call with block key `key`. `None` when no call has that key.
pub fn call_contract(facts: &mut Facts<'_>, key: &str) -> Result<Option<CallContract>> {
    let Some((caller, call)) = facts.locate(key)? else {
        return Ok(None);
    };
    let Some(link) = caller.calls_link(call).cloned() else {
        return Ok(None);
    };
    let node = caller.node(call).clone();
    let callee = match &link.to {
        LinkTarget::Project(target) => facts.locate(target.as_str())?,
        _ => None,
    };
    let caller_text = facts.text(&caller);

    let arguments = caller.arguments(call);
    let parameter = |index: u32| {
        let (file, function) = callee.as_ref()?;
        let parameter = file
            .parameters(*function)
            .iter()
            .find(|parameter| parameter.index == index)?;
        Some(contract_parameter(file, parameter))
    };
    let mut sends: Vec<ContractSend> = arguments
        .iter()
        .map(|argument| {
            let text = caller_text.as_ref().and_then(|source| {
                let text = source.slice(argument.range).ok()?;
                Some(short_label(text).to_owned())
            });
            let (parameter, unpaired) = match argument.pairing {
                Some(Pairing::Parameter(index)) => (parameter(index), None),
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
    if known
        && let (Some(bound), Some((file, function))) = (link.bound, callee.as_ref())
        && file.kind(*function) == NodeKind::Function
    {
        let filled: BTreeSet<u32> = arguments
            .iter()
            .filter_map(|argument| match argument.pairing {
                Some(Pairing::Parameter(index)) => Some(index),
                _ => None,
            })
            .collect();
        sends.extend(
            file.parameters(*function)
                .iter()
                .filter(|parameter| {
                    parameter.kind.takes_argument()
                        && parameter.index >= bound
                        && !filled.contains(&parameter.index)
                })
                .map(|parameter| ContractSend {
                    argument: None,
                    parameter: Some(contract_parameter(file, parameter)),
                    unpaired: None,
                }),
        );
    }

    let ty = callee.as_ref().and_then(|(file, target)| {
        let class = (node.form == Form::New || file.kind(*target) == NodeKind::Class)
            .then(|| constructed(file, *target))
            .flatten();
        match class {
            Some(class) => Some(whole_text(
                &file.label(class),
                &LinkTarget::Project(file.key(class).clone()),
            )),
            None => file
                .signature(*target)
                .and_then(|signature| signature.returns.as_ref())
                .map(|returns| type_text(returns, file.type_links_in(returns.range))),
        }
    });
    let returns = ContractReturn {
        ty,
        awaited: node.flags.contains(Flag::Awaited),
        lands: landing(&caller, call),
    };

    let mut contract_raises = Vec::new();
    if let Some((file, target)) = &callee
        && let Some(function) = raising(file, *target)
    {
        let scope = caller.owner(call).unwrap_or(0);
        for raise in raises(facts, file, function)? {
            contract_raises.push(ContractRaise {
                exit: file.key(raise.exit).clone(),
                line: file.node(raise.exit).lines.start,
                ty: raise.ty,
                value: raise.value,
                caught_by: handler_for(&caller, call, scope, &raise.lineage)
                    .map(|part| caller.key(part).clone()),
            });
        }
    }

    Ok(Some(CallContract {
        call: node.key.clone(),
        line: node.lines.start,
        text: link.text.clone(),
        callee: LinkEnd::of(&link.to),
        sends,
        returns,
        raises: contract_raises,
    }))
}
