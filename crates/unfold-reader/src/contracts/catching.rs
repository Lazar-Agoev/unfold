//! Which handler catches a raise, by written facts only. Call contracts ask it of the raises a
//! callee leaves uncaught, and the drawing asks it of each `raise` and `throw` in a `try`.
//!
//! A TypeScript `catch`, or a bare `except:`, catches everything. A bare `except` is told by its
//! recorded head, which is the clause through its type and name: a head that covers only the
//! `except` keyword writes no type expression. Any other `except` writes one, resolved or not
//! (`except ERRORS[0]:` names no class the graph can follow, so it catches nothing proven).
//! Otherwise a handler catches a raise when one of its `types` links names the raised class or a
//! class it derives from: through project `extends` links, and through the bases typeshed writes
//! for Python's builtin exception classes ([`BUILTIN_EXCEPTIONS`]), so `except Exception`
//! catches `ValueError` and a project class whose bases reach `Exception`. A library class
//! matches by its module and written name. A raise whose class is unknown is caught only by a
//! handler that catches everything.

use unfold_core::vocab::{Flag, Form, LinkKind, NodeKind, Outcome};
use unfold_core::{Key, Link, LinkTarget, ModuleLabel};

use super::facts::Facts;
use super::python_exceptions::BUILTIN_EXCEPTIONS;
use crate::error::Result;
use crate::tree::FileTree;

/// How many project `extends` links a catch follows up from a raised class.
const MAX_BASE_DEPTH: usize = 8;

/// The module Python's builtins are labeled with.
const PYTHON_BUILTINS: &str = "python:builtins";

/// What a raise raises, for matching it against handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Raised {
    /// A project class.
    Class(Key),
    /// A library or builtin class: its outcome, module, and name (for one of Python's builtin
    /// exceptions, the class the written name names: `IOError` is `OSError`).
    Outside(Outcome, ModuleLabel, Box<str>),
    /// Nothing proven: a raise of a value, a bare `raise`, or an unknown construction.
    Unknown,
}

impl Raised {
    /// A library or builtin class, by the module and name a link wrote.
    fn outside(outcome: Outcome, module: &ModuleLabel, name: &str) -> Self {
        let name = match python_exception(outcome, module, name) {
            Some((class, _)) => class,
            None => name,
        };
        Self::Outside(outcome, module.clone(), name.into())
    }

    /// The class a link names, as a raise to match.
    fn named(link: &Link) -> Option<Self> {
        match &link.to {
            LinkTarget::Project(key) => Some(Self::Class(key.clone())),
            LinkTarget::Library(module) => Some(Self::outside(Outcome::Library, module, &link.text)),
            LinkTarget::Builtin(module) => Some(Self::outside(Outcome::Builtin, module, &link.text)),
            LinkTarget::Unknown(_) => None,
        }
    }
}

/// The builtin exception class `name` names in `module`, and its bases, when `module` is
/// Python's builtins and `name` is one of them.
fn python_exception(
    outcome: Outcome,
    module: &ModuleLabel,
    name: &str,
) -> Option<(&'static str, &'static [&'static str])> {
    if outcome != Outcome::Builtin || module.as_str() != PYTHON_BUILTINS {
        return None;
    }
    let index = BUILTIN_EXCEPTIONS
        .binary_search_by(|(found, ..)| (*found).cmp(name))
        .ok()?;
    let (_, class, bases) = BUILTIN_EXCEPTIONS[index];
    Some((class, bases))
}

/// Where a catch looks project classes up: the store's facts, or the one file a drawing reads.
pub(crate) trait Classes {
    /// `read` of the tree of the file holding the node with `key`, and the node's ID; `None`
    /// when there is no such node here.
    fn read<R>(&mut self, key: &Key, read: impl FnOnce(&FileTree, u32) -> R) -> Result<Option<R>>;
}

impl Classes for Facts<'_> {
    fn read<R>(&mut self, key: &Key, read: impl FnOnce(&FileTree, u32) -> R) -> Result<Option<R>> {
        Ok(self.locate(key.as_str())?.map(|(file, id)| read(&file, id)))
    }
}

impl Classes for &FileTree {
    fn read<R>(&mut self, key: &Key, read: impl FnOnce(&FileTree, u32) -> R) -> Result<Option<R>> {
        Ok(self.id(key.as_str()).map(|id| read(self, id)))
    }
}

/// The class a construction of `target` (a class, or a constructor or `__init__` method)
/// builds, located: the target itself when it is a class, else its parent when that is a class.
pub(crate) fn constructed(file: &FileTree, target: u32) -> Option<u32> {
    match file.kind(target) {
        NodeKind::Class => Some(target),
        NodeKind::Function => file
            .parent(target)
            .filter(|&parent| file.kind(parent) == NodeKind::Class),
        _ => None,
    }
}

/// The class `link` (a construction's `calls` link) constructs, as a raise to match.
pub(crate) fn raised(classes: &mut impl Classes, link: &Link) -> Result<Raised> {
    Ok(match &link.to {
        LinkTarget::Project(key) => classes
            .read(key, |file, target| {
                constructed(file, target).map(|class| file.key(class).clone())
            })?
            .flatten()
            .map_or(Raised::Unknown, Raised::Class),
        _ => Raised::named(link).unwrap_or(Raised::Unknown),
    })
}

/// The value expression of exit `exit`, when it is exactly a call: its child flagged
/// `exit_value`.
pub(crate) fn exit_call(file: &FileTree, exit: u32) -> Option<u32> {
    file.children(exit)
        .iter()
        .copied()
        .find(|&child| file.kind(child) == NodeKind::Call && file.node(child).flags.contains(Flag::ExitValue))
}

/// What `raise` or `throw` exit `exit` of `file` raises: the class its value constructs, when
/// the value is exactly a construction.
pub(crate) fn exit_raises(classes: &mut impl Classes, file: &FileTree, exit: u32) -> Result<Raised> {
    match exit_call(file, exit).and_then(|call| file.calls_link(call)) {
        Some(link) => raised(classes, link),
        None => Ok(Raised::Unknown),
    }
}

/// `raised` and every class it derives from: up project `extends` links (each class's own,
/// which name its bases), breadth first, at most [`MAX_BASE_DEPTH`] levels; and from one of
/// Python's builtin exceptions, up its bases to `BaseException`.
pub(crate) fn lineage(classes: &mut impl Classes, raised: Raised) -> Result<Vec<Raised>> {
    let mut lineage = Vec::new();
    add(&mut lineage, raised);
    let mut level = 0..lineage.len();
    for _ in 0..MAX_BASE_DEPTH {
        let mut next = Vec::new();
        for index in level.clone() {
            let Raised::Class(class) = &lineage[index] else {
                continue;
            };
            let bases = classes.read(class, |file, id| {
                file.links(id)
                    .iter()
                    .filter(|link| link.kind == LinkKind::Extends)
                    .filter_map(Raised::named)
                    .collect::<Vec<_>>()
            })?;
            next.extend(bases.into_iter().flatten());
        }
        let start = lineage.len();
        for base in next {
            add(&mut lineage, base);
        }
        if lineage.len() == start {
            break;
        }
        level = start..lineage.len();
    }
    Ok(lineage)
}

/// Adds `raised` to `lineage` unless it is there, with a builtin exception's bases all the way
/// up.
fn add(lineage: &mut Vec<Raised>, raised: Raised) {
    if lineage.contains(&raised) {
        return;
    }
    let builtin = match &raised {
        Raised::Outside(outcome, module, name) => {
            python_exception(*outcome, module, name).map(|(_, bases)| (*outcome, module.clone(), bases))
        }
        Raised::Class(_) | Raised::Unknown => None,
    };
    lineage.push(raised);
    if let Some((outcome, module, bases)) = builtin {
        for base in bases {
            add(lineage, Raised::Outside(outcome, module.clone(), (*base).into()));
        }
    }
}

/// Whether `except` part `handler` is bare: its head (the clause) covers only its keyword.
fn is_bare_except(file: &FileTree, handler: u32) -> bool {
    file.node(handler)
        .head
        .is_some_and(|head| head.len() as usize == Form::Except.as_str().len())
}

/// Whether handler part `handler` of `file` catches a raise whose lineage is `lineage`.
pub(crate) fn catches(file: &FileTree, handler: u32, lineage: &[Raised]) -> bool {
    match file.node(handler).form {
        Form::Catch => true,
        Form::Except if is_bare_except(file, handler) => true,
        Form::Except => file
            .links(handler)
            .iter()
            .filter(|link| link.kind == LinkKind::Types)
            .filter_map(Raised::named)
            .any(|named| lineage.contains(&named)),
        _ => false,
    }
}
