//! Links: order, sources, ranges, outcomes, and targets per link kind.

use super::nodes::Tree;
use super::{Checker, Subject};
use crate::model::{FileGraph, Link, LinkTarget, Node};
use crate::vocab::{Form, Language, LinkKind, NodeKind};

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph, tree: &Tree) {
    let mut previous: Option<(u32, u32)> = None;
    for link in &file.links {
        let Some(source) = file.nodes.get(link.from as usize) else {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!("link {} names missing node {}", link.seq, link.from),
            );
            continue;
        };
        let subject = || Subject::Link {
            from: source.key.clone(),
            seq: link.seq,
        };
        let expected = match previous {
            Some((from, seq)) if from == link.from => seq + 1,
            _ => 0,
        };
        if previous.is_some_and(|previous| previous >= (link.from, link.seq)) {
            checker.report(subject(), "links are not in from, then seq, order");
        } else if link.seq != expected {
            checker.report(
                subject(),
                format!("seqs are contiguous from zero; expected {expected}"),
            );
        }
        previous = Some((link.from, link.seq));
        check_source(checker, file, link, source, &subject);
        check_range(checker, file, tree, link, source, &subject);
        checker.check_label(&subject, "text", &link.text);
        check_target(checker, file, link, &subject);
    }
    check_calls(checker, file);
}

/// What each link kind may start at.
fn check_source(
    checker: &mut Checker<'_>,
    file: &FileGraph,
    link: &Link,
    source: &Node,
    subject: &dyn Fn() -> Subject,
) {
    let kind = source.kind();
    let fits = match link.kind {
        LinkKind::Calls | LinkKind::Passes => kind == NodeKind::Call,
        LinkKind::Extends => matches!(kind, NodeKind::Class | NodeKind::Type),
        LinkKind::Imports => matches!(kind, NodeKind::Module | NodeKind::Function | NodeKind::Call),
        LinkKind::Types => kind.is_definition() || matches!(source.form, Form::Except | Form::Catch),
    };
    if !fits {
        checker.report(
            subject(),
            format!("`{}` links cannot start at {} nodes", link.kind, source.form),
        );
    }
    let implements = link.kind == LinkKind::Extends
        && kind == NodeKind::Class
        && file.file.language == Language::TypeScript;
    if link.implements && !implements {
        checker.report(
            subject(),
            "only an `extends` link from a TypeScript class implements",
        );
    }
}

/// A link's range lies inside its node; a link from a definition is not written inside a
/// nearer definition, since a block belongs to the nearest definition above it.
fn check_range(
    checker: &mut Checker<'_>,
    file: &FileGraph,
    tree: &Tree,
    link: &Link,
    source: &Node,
    subject: &dyn Fn() -> Subject,
) {
    if link.range.is_empty() {
        checker.report(subject(), "range is empty");
    }
    if !source.range.contains(link.range) {
        checker.report(
            subject(),
            format!(
                "range {} is outside its node's range {}",
                link.range, source.range
            ),
        );
        return;
    }
    if !source.kind().is_definition() {
        return;
    }
    // Walk down through the children that contain the range, stopping at a definition.
    let mut open = tree.children(link.from).to_vec();
    while let Some(id) = open.pop() {
        let Some(node) = file.nodes.get(id as usize) else {
            continue;
        };
        if !node.range.contains(link.range) {
            continue;
        }
        if node.kind().is_definition() {
            checker.report(
                subject(),
                format!("range {} lies in {}, a nearer definition", link.range, node.key),
            );
            return;
        }
        open.extend_from_slice(tree.children(id));
    }
}

/// What each link kind may reach.
fn check_target(checker: &mut Checker<'_>, file: &FileGraph, link: &Link, subject: &dyn Fn() -> Subject) {
    let mut reaches_function = false;
    match &link.to {
        LinkTarget::Project(key) => {
            let Some((target_file, target, _)) = checker.resolve(key) else {
                checker.report(subject(), format!("target `{key}` is not a node in the graph"));
                return;
            };
            let kind = target.kind();
            let fits = match link.kind {
                LinkKind::Calls | LinkKind::Passes => matches!(kind, NodeKind::Function | NodeKind::Class),
                LinkKind::Extends | LinkKind::Types => matches!(kind, NodeKind::Class | NodeKind::Type),
                LinkKind::Imports => kind == NodeKind::Module,
            };
            if !fits {
                checker.report(
                    subject(),
                    format!("`{}` links cannot reach {kind} nodes", link.kind),
                );
            }
            if target_file.file.language != file.file.language {
                checker.report(subject(), "a project link cannot cross languages");
            }
            reaches_function = kind == NodeKind::Function;
        }
        LinkTarget::Library(label) | LinkTarget::Builtin(label) => {
            if !label.is_valid() {
                checker.report(
                    subject(),
                    format!(
                        "module `{}` is not a label or a sorted set of labels",
                        label.as_str()
                    ),
                );
            }
        }
        LinkTarget::Unknown(_) => {}
    }
    if link.kind == LinkKind::Passes && !matches!(link.to, LinkTarget::Project(_)) {
        checker.report(subject(), "a `passes` link reaches a project definition");
    }
    if link.bound.is_some() && !(link.kind == LinkKind::Calls && reaches_function) {
        checker.report(
            subject(),
            "only a `calls` link to a project function has a bound count",
        );
    }
}

/// Every call has exactly one `calls` link, its first: an unresolved call is never dropped.
fn check_calls(checker: &mut Checker<'_>, file: &FileGraph) {
    let mut links = file.links.iter().peekable();
    for (index, node) in file.nodes.iter().enumerate() {
        if node.kind() != NodeKind::Call {
            continue;
        }
        let id = u32::try_from(index).unwrap_or(u32::MAX);
        let mut calls = 0;
        while let Some(link) = links.next_if(|link| link.from <= id) {
            if link.from != id || link.kind != LinkKind::Calls {
                continue;
            }
            calls += 1;
            if link.seq != 0 {
                checker.report(
                    Subject::Link {
                        from: node.key.clone(),
                        seq: link.seq,
                    },
                    "a call's `calls` link is its first link",
                );
            }
        }
        if calls != 1 {
            checker.report(
                Subject::Node(node.key.clone()),
                format!("a call has exactly one `calls` link, found {calls}"),
            );
        }
    }
}
