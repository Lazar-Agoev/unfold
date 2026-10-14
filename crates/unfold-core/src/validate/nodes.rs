//! One file: its record, its nodes (preorder, containment, parts, fields, flags, and ranges),
//! its keys, and its ranges against the snapshot and the census. The file's links and typed
//! records are checked from here too.

use rustc_hash::FxHashMap;

use super::{Census, Checker, Subject, links, records};
use crate::key::{Callee, Child, KeyBuilder, ParsedKey, Segment};
use crate::model::{FileGraph, Node};
use crate::text::{ByteRange, Snapshot};
use crate::vocab::{CallRole, FileState, Flag, Form, Inline, Language, LinkKind, NodeKind};

pub(super) fn check(checker: &mut Checker<'_>, file: &FileGraph, census: Option<&dyn Census>) {
    let snapshot = check_record(checker, file);
    let tree = Tree::new(file);
    check_nodes(checker, file, &tree);
    check_parts(checker, file, &tree);
    check_keys(checker, file, &tree);
    links::check(checker, file, &tree);
    records::check(checker, file);
    if let Some(snapshot) = snapshot {
        check_text(checker, file, snapshot);
    }
    if let Some(census) = census {
        check_census(checker, file, census);
    }
}

/// What the file checks share about its nodes, built once.
pub(super) struct Tree {
    /// Each node's children in preorder (only children whose parent precedes them).
    children: Vec<Vec<u32>>,
    /// Each node's last key segment (none for a module key), or why its key isn't a block key.
    steps: Vec<Result<Option<Segment>, String>>,
}

impl Tree {
    fn new(file: &FileGraph) -> Self {
        let mut children = vec![Vec::new(); file.nodes.len()];
        for (index, node) in file.nodes.iter().enumerate() {
            if let Some(parent) = node.parent
                && (parent as usize) < index
            {
                children[parent as usize].push(u32::try_from(index).unwrap_or(u32::MAX));
            }
        }
        let steps = file
            .nodes
            .iter()
            .map(|node| match node.key.parse() {
                Ok(ParsedKey::Block(block)) => Ok(block.segments().last().cloned()),
                Ok(_) => Err("the key is not a block key".to_owned()),
                Err(error) => Err(error.to_string()),
            })
            .collect();
        Self { children, steps }
    }

    /// The children of node `id`, in preorder.
    pub(super) fn children(&self, id: u32) -> &[u32] {
        self.children.get(id as usize).map_or(&[], Vec::as_slice)
    }

    /// Whether node `id`'s key ends in `callback[n]`.
    fn is_callback(&self, id: u32) -> bool {
        matches!(
            self.steps.get(id as usize),
            Some(Ok(Some(Segment::Inline {
                inline: Inline::Callback,
                ..
            })))
        )
    }
}

/// Whether node `id` is an expression block: a call, a ternary, a comprehension, or an inline
/// function (unnamed, or a callback labeled by the property it is written as).
fn is_expression(tree: &Tree, id: u32, node: &Node) -> bool {
    match node.kind() {
        NodeKind::Call => true,
        NodeKind::Function => node.name.is_none() || tree.is_callback(id),
        _ => matches!(node.form, Form::Ternary | Form::Comprehension),
    }
}

/// Checks the file record and returns the snapshot to check ranges against, if it matches.
fn check_record<'g>(checker: &mut Checker<'g>, file: &FileGraph) -> Option<&'g Snapshot> {
    let record = &file.file;
    let subject = || Subject::File(record.path.clone());
    let has_nodes = !file.nodes.is_empty();
    match record.state {
        FileState::Current | FileState::LastGood => {
            if record.hash.is_none() {
                checker.report(
                    subject(),
                    format!("a {} file needs the hash its facts came from", record.state),
                );
            }
            if !has_nodes {
                checker.report(subject(), format!("a {} file needs its nodes", record.state));
            }
            match (record.state, &record.failure) {
                (FileState::Current, Some(_)) => checker.report(subject(), "a current file has no failure"),
                (FileState::LastGood, None) => {
                    checker.report(subject(), "a last_good file needs its failure");
                }
                _ => {}
            }
        }
        FileState::Failed => {
            let types = &file.types;
            let has_types = !types.signatures.is_empty()
                || !types.parameters.is_empty()
                || !types.members.is_empty()
                || !types.arguments.is_empty();
            if record.hash.is_some() || has_nodes || !file.links.is_empty() || has_types {
                checker.report(
                    subject(),
                    "a failed file has no facts: no hash, nodes, links, or typed records",
                );
            }
            if record.failure.is_none() {
                checker.report(subject(), "a failed file needs its failure");
            }
        }
    }
    if record.failure.as_deref() == Some("") {
        checker.report(subject(), "failure is empty");
    }
    // Last-good facts come from earlier bytes, so only current facts are checked against disk.
    if checker.context.sources.is_none() || record.state != FileState::Current {
        return None;
    }
    let Some(snapshot) = checker.snapshot(&record.path) else {
        checker.report(subject(), "no snapshot was supplied");
        return None;
    };
    if record.hash != Some(snapshot.hash()) {
        checker.report(
            subject(),
            format!("hash does not match the snapshot's {}", snapshot.hash()),
        );
        return None;
    }
    Some(snapshot)
}

/// Checks every node: preorder, its form and flags, its place in the grammar, its ranges, and
/// its fields.
fn check_nodes(checker: &mut Checker<'_>, file: &FileGraph, tree: &Tree) {
    let language = file.file.language;
    let nodes = &file.nodes;
    // The chain of open ancestors of the next node in preorder.
    let mut ancestors: Vec<u32> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let id = u32::try_from(index).unwrap_or(u32::MAX);
        let subject = || Subject::Node(node.key.clone());
        if !node.form.allowed_in(language) {
            checker.report(
                subject(),
                format!("form {} does not occur in {language} files", node.form),
            );
        }
        for flag in node.flags.iter() {
            if !flag.applies_to(node.form) {
                checker.report(
                    subject(),
                    format!("flag {flag} does not apply to {} nodes", node.form),
                );
            }
        }
        if node.lines.start == 0 || node.lines.end < node.lines.start {
            checker.report(
                subject(),
                format!("{:?} are not one-based and ordered", node.lines),
            );
        }
        check_inner_ranges(checker, node);
        if id == 0 {
            if node.form != Form::Module || node.parent.is_some() {
                checker.report(subject(), "the first node is the module, with no parent");
            }
            if node.range.start != 0 || node.lines.start != 1 {
                checker.report(subject(), "the module starts at byte 0 on line 1");
            }
            check_fields(checker, file, id, node, None);
            ancestors.push(0);
            continue;
        }
        if node.form == Form::Module {
            checker.report(subject(), "only the first node is a module");
        }
        let Some(parent_id) = node.parent else {
            checker.report(subject(), "a node other than the module needs a parent");
            continue;
        };
        match ancestors.iter().rposition(|&ancestor| ancestor == parent_id) {
            Some(position) => ancestors.truncate(position + 1),
            None => {
                checker.report(
                    subject(),
                    format!("parent {parent_id} is not an open ancestor, so nodes are not in preorder"),
                );
                ancestors.truncate(1);
            }
        }
        ancestors.push(id);
        let Some(parent) = nodes.get(parent_id as usize).filter(|_| parent_id < id) else {
            checker.report(subject(), format!("parent {parent_id} does not precede the node"));
            continue;
        };
        check_containment(checker, tree, id, node, parent);
        check_ranges(checker, node, parent);
        check_fields(checker, file, id, node, Some(parent));
    }
}

/// The containment grammar: what a node may sit in.
fn check_containment(checker: &mut Checker<'_>, tree: &Tree, id: u32, node: &Node, parent: &Node) {
    let expression = is_expression(tree, id, node);
    let problem = match parent.kind() {
        NodeKind::Type => Some("a type contains nothing".to_owned()),
        kind if kind.is_control() => match node.kind() {
            // A control block's parts are checked with the control block.
            NodeKind::Part => None,
            _ if !expression => Some(format!(
                "{} blocks hold only their parts and the expression blocks in their header",
                parent.form
            )),
            _ if !parent.head.is_some_and(|head| head.contains(node.range)) => Some(format!(
                "an expression block lies in the header of its {} block",
                parent.form
            )),
            _ => None,
        },
        kind if node.kind() == NodeKind::Part => Some(format!(
            "parts sit directly in control blocks, not in {kind} nodes"
        )),
        NodeKind::Call if !expression => {
            Some("a call holds only the expression blocks in its receiver and arguments".to_owned())
        }
        NodeKind::Exit if !expression => {
            Some("an exit holds only the expression blocks in its value".to_owned())
        }
        NodeKind::Exit if !parent.value.is_some_and(|value| value.contains(node.range)) => {
            Some("an expression block in an exit lies in its value".to_owned())
        }
        _ => None,
    };
    if let Some(problem) = problem {
        checker.report(Subject::Node(node.key.clone()), problem);
    }
    if tree.is_callback(id) && parent.kind() != NodeKind::Call {
        checker.report(
            Subject::Node(node.key.clone()),
            "a callback is written in a call's arguments",
        );
    }
}

/// `head`, `body`, and `value` lie inside the node's range.
fn check_inner_ranges(checker: &mut Checker<'_>, node: &Node) {
    for (field, range) in [("head", node.head), ("body", node.body), ("value", node.value)] {
        if let Some(range) = range
            && !node.range.contains(range)
        {
            checker.report(
                Subject::Node(node.key.clone()),
                format!("{field} {range} is outside the node's range {}", node.range),
            );
        }
    }
    if node.parent.is_none()
        && let Some(doc) = node.doc
        && !node.range.contains(doc)
    {
        checker.report(
            Subject::Node(node.key.clone()),
            format!("doc {doc} is outside the module"),
        );
    }
}

/// A node's ranges against its parent's: its range inside the parent's, its statement around
/// it and inside the parent, and its doc inside it or just before it.
fn check_ranges(checker: &mut Checker<'_>, node: &Node, parent: &Node) {
    let subject = || Subject::Node(node.key.clone());
    if node.range.is_empty() {
        checker.report(subject(), "range is empty");
    }
    if !parent.range.contains(node.range) {
        checker.report(
            subject(),
            format!(
                "range {} is outside its parent's range {}",
                node.range, parent.range
            ),
        );
    } else if !node.lines.within(parent.lines) {
        checker.report(
            subject(),
            format!("{:?} are outside its parent's {:?}", node.lines, parent.lines),
        );
    }
    if let Some(stmt) = node.stmt {
        if !stmt.contains(node.range) {
            checker.report(
                subject(),
                format!("stmt {stmt} does not contain the node's range {}", node.range),
            );
        } else if !parent.range.contains(stmt) {
            checker.report(
                subject(),
                format!("stmt {stmt} is outside its parent's range {}", parent.range),
            );
        }
    }
    if let Some(doc) = node.doc {
        // A docstring lies inside its definition; a leading JSDoc just before it.
        let before = doc.end <= node.range.start && parent.range.contains(doc);
        if !node.range.contains(doc) && !before {
            checker.report(
                subject(),
                format!("doc {doc} is neither inside the node nor just before it"),
            );
        }
    }
}

/// Which fields a node of its kind carries, and the rules for `role`, `for`, and `target`.
fn check_fields(checker: &mut Checker<'_>, file: &FileGraph, id: u32, node: &Node, parent: Option<&Node>) {
    let kind = node.kind();
    let subject = || Subject::Node(node.key.clone());
    match (kind, &node.name) {
        (_, Some(name)) if name.is_empty() => {
            checker.report(subject(), "name is empty; an absent name is omitted");
        }
        (NodeKind::Class | NodeKind::Type, None) => {
            checker.report(subject(), format!("{} nodes need their name", node.form));
        }
        (NodeKind::Call, None) => checker.report(subject(), "a call needs its callee as its name"),
        (NodeKind::Call, Some(callee)) if callee.chars().count() > Callee::MAX_CHARS => checker.report(
            subject(),
            format!("a callee is cut at {} characters", Callee::MAX_CHARS),
        ),
        (NodeKind::Function, None) if node.form != Form::Lambda => {
            checker.report(subject(), "an unnamed function is a lambda");
        }
        (
            NodeKind::Module
            | NodeKind::Decision
            | NodeKind::Loop
            | NodeKind::Try
            | NodeKind::With
            | NodeKind::Part
            | NodeKind::Exit,
            Some(_),
        ) => checker.report(subject(), format!("{kind} nodes have no name")),
        _ => {}
    }
    let definition = kind.is_definition();
    let has_head = (definition && kind != NodeKind::Module)
        || kind.is_control()
        || matches!(kind, NodeKind::Part | NodeKind::Call);
    if node.head.is_some() && !has_head {
        checker.report(subject(), format!("{kind} nodes have no head"));
    }
    if node.body.is_some() && !definition && kind != NodeKind::Part {
        checker.report(subject(), format!("{kind} nodes have no body"));
    }
    if node.doc.is_some() && !definition {
        checker.report(subject(), format!("{kind} nodes have no doc"));
    }
    if node.stmt.is_some() && matches!(kind, NodeKind::Module | NodeKind::Part) {
        checker.report(subject(), format!("{kind} nodes have no stmt"));
    }
    if kind == NodeKind::Exit && node.stmt.is_none() {
        checker.report(subject(), "an exit needs its statement's range as its stmt");
    }
    if node.value.is_some() {
        if kind != NodeKind::Exit {
            checker.report(subject(), "only an exit has a value");
        } else if matches!(node.form, Form::Break | Form::Continue) {
            checker.report(subject(), format!("{} exits have no value", node.form));
        }
    }
    if let Some(binds) = &node.binds {
        if kind != NodeKind::Call && !matches!(node.form, Form::Ternary | Form::Comprehension) {
            checker.report(subject(), "only a call, ternary, or comprehension binds a target");
        } else if binds.is_empty() {
            checker.report(subject(), "binds is empty; an absent binds is omitted");
        }
    }
    let parent_kind = parent.map(Node::kind);
    if node.flags.contains(Flag::ExitValue) && parent_kind != Some(NodeKind::Exit) {
        checker.report(subject(), "an exit_value block is written as its exit's value");
    }
    if node.flags.contains(Flag::FallsThrough) && parent.map(|parent| parent.form) != Some(Form::Switch) {
        checker.report(subject(), "only a switch's parts fall through");
    }
    check_role(checker, file, id, node, parent);
    check_target(checker, file, id, node);
}

fn check_role(checker: &mut Checker<'_>, file: &FileGraph, id: u32, node: &Node, parent: Option<&Node>) {
    let subject = || Subject::Node(node.key.clone());
    let language = file.file.language;
    match (node.role, node.r#for) {
        (None, None) => {}
        (Some(_), _) if node.kind() != NodeKind::Call => checker.report(subject(), "only a call has a role"),
        (None, Some(_)) => checker.report(subject(), "only a decorator, base, or default call has `for`"),
        (Some(CallRole::Test), definition) => {
            if definition.is_some() {
                checker.report(subject(), "a test call has no `for`");
            }
            let in_test = parent
                .filter(|parent| parent.kind() == NodeKind::Part)
                .and_then(|parent| parent.head)
                .is_some_and(|test| test.contains(node.range));
            if !in_test {
                checker.report(subject(), "a test call is written in its part's test");
            }
        }
        (Some(CallRole::Default), definition) if language == Language::TypeScript => {
            if definition.is_some() || parent.map(Node::kind) != Some(NodeKind::Function) {
                checker.report(
                    subject(),
                    "a TypeScript default runs in its own function's body, with no `for`",
                );
            }
        }
        (Some(role), None) => checker.report(
            subject(),
            format!("{role} calls name the definition they belong to in `for`"),
        ),
        (Some(role), Some(definition)) => {
            let target = file
                .nodes
                .get(definition as usize)
                .filter(|target| definition != id && target.parent.is_some() && target.parent == node.parent);
            let Some(target) = target.filter(|target| target.kind().is_definition()) else {
                checker.report(
                    subject(),
                    format!("`for` {definition} is not a definition in the same body"),
                );
                return;
            };
            let fits = match role {
                CallRole::Base => target.kind() == NodeKind::Class,
                CallRole::Default => target.kind() == NodeKind::Function,
                CallRole::Decorator => matches!(target.kind(), NodeKind::Function | NodeKind::Class),
                CallRole::Test => false,
            };
            if !fits {
                checker.report(
                    subject(),
                    format!("`for` names {}, which {role} calls can't belong to", target.key),
                );
            }
        }
    }
}

fn check_target(checker: &mut Checker<'_>, file: &FileGraph, id: u32, node: &Node) {
    let subject = || Subject::Node(node.key.clone());
    let jumps = matches!(node.form, Form::Break | Form::Continue);
    match (node.target, jumps) {
        (Some(_), false) => checker.report(subject(), "only break and continue have a target"),
        (None, true) => checker.report(
            subject(),
            format!("{} exits need their loop as their target", node.form),
        ),
        (Some(target), true) if !encloses(&file.nodes, target, id, node.form) => checker.report(
            subject(),
            format!(
                "target {target} is not a loop{} enclosing it in the same definition",
                if node.form == Form::Break {
                    " or switch"
                } else {
                    ""
                }
            ),
        ),
        _ => {}
    }
}

/// Whether node `target` is a loop above node `id` (or, for a `break`, a `switch`), with no
/// definition between them.
fn encloses(nodes: &[Node], target: u32, id: u32, form: Form) -> bool {
    let mut current = id;
    while let Some(parent) = nodes.get(current as usize).and_then(|node| node.parent) {
        let Some(ancestor) = nodes.get(parent as usize).filter(|_| parent < current) else {
            return false;
        };
        if parent == target {
            return ancestor.kind() == NodeKind::Loop
                || (form == Form::Break && ancestor.form == Form::Switch);
        }
        if ancestor.kind().is_definition() {
            return false;
        }
        current = parent;
    }
    false
}

/// The place of a part of `form` among the parts of a `control` block, in written order;
/// `None` when such a block has no such part. Parts of one place may come in any order.
fn part_place(control: Form, part: Form, language: Language) -> Option<u8> {
    use Form as F;
    let python = language == Language::Python;
    match (control, part) {
        (F::If | F::Ternary, F::Then)
        | (F::Match | F::Switch, F::Case | F::Default)
        | (F::For | F::While | F::Do | F::Comprehension | F::Try | F::With | F::Using, F::Body) => Some(0),
        (F::If, F::Elif) | (F::Ternary, F::Else) | (F::Try, F::Except | F::Catch) => Some(1),
        (F::For | F::While, F::Else) if python => Some(1),
        (F::If, F::Else) => Some(2),
        (F::Try, F::Else) if python => Some(2),
        (F::Try, F::Finally) => Some(3),
        _ => None,
    }
}

/// The parts every block of a control form has.
const fn required_parts(control: Form) -> &'static [Form] {
    match control {
        Form::If => &[Form::Then],
        Form::Ternary => &[Form::Then, Form::Else],
        Form::For | Form::While | Form::Do | Form::Comprehension | Form::Try | Form::With | Form::Using => {
            &[Form::Body]
        }
        _ => &[],
    }
}

/// Each control block's parts: which forms it has, in written order, and the ones it needs.
fn check_parts(checker: &mut Checker<'_>, file: &FileGraph, tree: &Tree) {
    let language = file.file.language;
    for (index, control) in file.nodes.iter().enumerate() {
        if !control.kind().is_control() {
            continue;
        }
        let id = u32::try_from(index).unwrap_or(u32::MAX);
        let parts = tree
            .children(id)
            .iter()
            .filter_map(|&child| file.nodes.get(child as usize))
            .filter(|child| child.kind() == NodeKind::Part);
        let mut last = 0;
        for part in parts {
            let subject = || Subject::Node(part.key.clone());
            match part_place(control.form, part.form, language) {
                None => checker.report(
                    subject(),
                    format!("{} blocks have no {} part", control.form, part.form),
                ),
                Some(place) if place < last => checker.report(
                    subject(),
                    format!("the {} part is out of written order", part.form),
                ),
                Some(place) => last = place,
            }
        }
        for &required in required_parts(control.form) {
            let has = tree.children(id).iter().any(|&child| {
                file.nodes
                    .get(child as usize)
                    .is_some_and(|child| child.form == required)
            });
            if !has {
                checker.report(
                    Subject::Node(control.key.clone()),
                    format!("{} blocks need a {required} part", control.form),
                );
            }
        }
    }
}

/// Re-derives every key from the tree with [`KeyBuilder`] and compares.
///
/// Nodes record everything a key is derived from except a call's first literal, which only
/// the key holds. So a call's literal is read from its own key, and checked only against the
/// tree: a key carries one exactly when the call's arguments hold a callback.
fn check_keys(checker: &mut Checker<'_>, file: &FileGraph, tree: &Tree) {
    let mut builder = KeyBuilder::new(&file.file.path);
    let Some(module) = file.nodes.first() else {
        return;
    };
    if module.key != builder.keys()[0] {
        checker.report(
            Subject::Node(module.key.clone()),
            format!("key differs from its derivation `{}`", builder.keys()[0]),
        );
    }
    for (index, node) in file.nodes.iter().enumerate().skip(1) {
        let id = u32::try_from(index).unwrap_or(u32::MAX);
        let subject = || Subject::Node(node.key.clone());
        let Some(parent) = node.parent else {
            // Reported by the node checks; later keys can't be derived without the tree.
            return;
        };
        let step = match tree.steps.get(index) {
            Some(Ok(step)) => step.as_ref(),
            Some(Err(problem)) => {
                checker.report(subject(), problem.as_str());
                return;
            }
            None => return,
        };
        let callee;
        let child = match (node.kind(), &node.name, step) {
            (NodeKind::Module, ..) => return,
            // A callback may carry the name of the property it is written as; its key is still
            // its position.
            (NodeKind::Function, name, Some(Segment::Inline { inline, .. })) => {
                if name.is_some() && *inline != Inline::Callback {
                    checker.report(
                        subject(),
                        "only a callback is named, by the property it is written as",
                    );
                }
                Child::Inline(*inline)
            }
            (NodeKind::Class | NodeKind::Type | NodeKind::Function, Some(name), _) => Child::Definition(name),
            // Reported by the node checks.
            (NodeKind::Class | NodeKind::Type, None, _) => return,
            (NodeKind::Function, None, _) => {
                checker.report(
                    subject(),
                    "an unnamed function's key does not end in callback[n] or lambda[n]",
                );
                return;
            }
            (NodeKind::Call, Some(name), step) => {
                let literal = match step {
                    Some(Segment::Call { literal, .. }) => literal.as_deref(),
                    _ => None,
                };
                let takes_callback = tree.children(id).iter().any(|&child| tree.is_callback(child));
                if literal.is_some() && !takes_callback {
                    checker.report(
                        subject(),
                        "the key carries a first literal, but no callback is written in the call's arguments",
                    );
                }
                callee = Callee::name(name);
                Child::Call {
                    callee: &callee,
                    first_literal: literal,
                    takes_callback: literal.is_some(),
                }
            }
            // Reported by the node checks.
            (NodeKind::Call, None, _) => return,
            _ => Child::Form(node.form),
        };
        match builder.push(parent, child) {
            Ok(key) if key != node.key => {
                checker.report(subject(), format!("key differs from its derivation `{key}`"));
            }
            Ok(_) => {}
            Err(error) => {
                // Later keys can't be derived once one node is missing from the builder.
                checker.report(subject(), format!("cannot derive the key: {error}"));
                return;
            }
        }
    }
}

/// Checks a current file's ranges against its snapshot: the module's length, every node's
/// lines, character boundaries, and surrounding trivia. Link and typed-record ranges are
/// checked for character boundaries only: a Python string annotation's name lies inside its
/// quotes.
fn check_text(checker: &mut Checker<'_>, file: &FileGraph, snapshot: &Snapshot) {
    let text = snapshot.text();
    let lines = snapshot.line_index();
    let language = file.file.language;
    if let Some(module) = file.nodes.first()
        && module.range.end != snapshot.len()
    {
        checker.report(
            Subject::Node(module.key.clone()),
            format!("the module spans the whole file ({} bytes)", snapshot.len()),
        );
    }
    for (index, node) in file.nodes.iter().enumerate() {
        let subject = || Subject::Node(node.key.clone());
        let span = lines.span(node.range);
        if node.lines != span {
            checker.report(
                subject(),
                format!("{:?} disagree with the text ({span:?})", node.lines),
            );
        }
        let fields = [
            ("range", Some(node.range)),
            ("head", node.head),
            ("body", node.body),
            ("doc", node.doc),
            ("stmt", node.stmt),
            ("value", node.value),
        ];
        for (field, range) in fields {
            let Some(range) = range else { continue };
            let Some(slice) = text.get(range.as_usize()) else {
                checker.report(
                    subject(),
                    format!("{field} {range} is outside the file or not on character boundaries"),
                );
                continue;
            };
            // The module spans the file, whatever surrounds its code.
            let module = index == 0 && matches!(field, "range" | "body");
            // A suite may start with a comment, and a JSDoc is one.
            let comments = !matches!(field, "body" | "doc");
            if let Some(problem) = trivia(slice, language, comments).filter(|_| !module) {
                checker.report(subject(), format!("{field} {problem}"));
            }
        }
    }
    for link in &file.links {
        if text.get(link.range.as_usize()).is_none()
            && let Some(from) = file.nodes.get(link.from as usize)
        {
            checker.report(
                Subject::Link {
                    from: from.key.clone(),
                    seq: link.seq,
                },
                format!(
                    "range {} is outside the file or not on character boundaries",
                    link.range
                ),
            );
        }
    }
    for (subject, field, range) in records::ranges(file) {
        if text.get(range.as_usize()).is_none() {
            checker.report(
                subject,
                format!("{field} {range} is outside the file or not on character boundaries"),
            );
        }
    }
}

/// Why a token-exact range is not, judging by its text alone.
fn trivia(text: &str, language: Language, comments: bool) -> Option<&'static str> {
    let is_space = |character: char| character.is_whitespace() || character == '\u{feff}';
    if text.starts_with(is_space) {
        return Some("starts with whitespace");
    }
    if text.ends_with(is_space) {
        return Some("ends with whitespace");
    }
    let comment = match language {
        Language::TypeScript => text.starts_with("//") || text.starts_with("/*"),
        Language::Python => text.starts_with('#'),
    };
    (comments && comment).then_some("starts with a comment")
}

/// Checks a file against its language's census: token boundaries, one call node per call in
/// the parse tree, and one `imports` link per module dependency.
fn check_census(checker: &mut Checker<'_>, file: &FileGraph, census: &dyn Census) {
    let path = &file.file.path;
    for node in file.nodes.iter().skip(1) {
        let fields = [
            ("range", Some(node.range)),
            ("head", node.head),
            ("stmt", node.stmt),
            ("value", node.value),
        ];
        for (field, range) in fields {
            if let Some(range) = range
                && !census.is_token_range(path, range)
            {
                checker.report(
                    Subject::Node(node.key.clone()),
                    format!("{field} {range} is not on token boundaries"),
                );
            }
        }
    }
    for (subject, field, range) in records::ranges(file) {
        if !census.is_token_range(path, range) {
            checker.report(subject, format!("{field} {range} is not on token boundaries"));
        }
    }
    if let Some(sites) = census.call_sites(path) {
        check_call_sites(checker, file, sites);
    }
    if let Some(dependencies) = census.module_dependencies(path) {
        check_dependencies(checker, file, dependencies);
    }
}

fn check_call_sites(checker: &mut Checker<'_>, file: &FileGraph, sites: &[super::CallSite]) {
    let mut calls: FxHashMap<ByteRange, Vec<usize>> = FxHashMap::default();
    for (index, node) in file.nodes.iter().enumerate() {
        if node.kind() == NodeKind::Call {
            calls.entry(node.range).or_default().push(index);
        }
    }
    let mut matched: FxHashMap<ByteRange, u32> = FxHashMap::default();
    for site in sites {
        *matched.entry(site.range).or_insert(0) += 1;
        let &[index] = calls.get(&site.range).map_or(&[][..], Vec::as_slice) else {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!("call site {} needs exactly one call node", site.range),
            );
            continue;
        };
        let node = &file.nodes[index];
        if let Some(form) = site.form
            && form != node.form
        {
            checker.report(
                Subject::Node(node.key.clone()),
                format!("the node is a {}, but the front end found a {form}", node.form),
            );
        }
    }
    let mut unmatched: Vec<usize> = calls
        .iter()
        .filter(|(range, _)| matched.get(*range) != Some(&1))
        .flat_map(|(_, indexes)| indexes.iter().copied())
        .collect();
    unmatched.sort_unstable();
    for index in unmatched {
        checker.report(
            Subject::Node(file.nodes[index].key.clone()),
            "the call does not match exactly one call site",
        );
    }
}

fn check_dependencies(checker: &mut Checker<'_>, file: &FileGraph, dependencies: &[ByteRange]) {
    let mut imports: FxHashMap<ByteRange, Vec<usize>> = FxHashMap::default();
    for (index, link) in file.links.iter().enumerate() {
        if link.kind == LinkKind::Imports {
            imports.entry(link.range).or_default().push(index);
        }
    }
    let mut matched: FxHashMap<ByteRange, u32> = FxHashMap::default();
    for range in dependencies {
        *matched.entry(*range).or_insert(0) += 1;
        if imports.get(range).map_or(0, Vec::len) != 1 {
            checker.report(
                Subject::File(file.file.path.clone()),
                format!("module dependency {range} needs exactly one `imports` link"),
            );
        }
    }
    let mut unmatched: Vec<usize> = imports
        .iter()
        .filter(|(range, _)| matched.get(*range) != Some(&1))
        .flat_map(|(_, indexes)| indexes.iter().copied())
        .collect();
    unmatched.sort_unstable();
    for index in unmatched {
        let link = &file.links[index];
        if let Some(from) = file.nodes.get(link.from as usize) {
            checker.report(
                Subject::Link {
                    from: from.key.clone(),
                    seq: link.seq,
                },
                "the `imports` link does not match exactly one module dependency",
            );
        }
    }
}
