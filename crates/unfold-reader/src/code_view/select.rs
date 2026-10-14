//! What a target shows and marks, and its path, from the records alone ("What each block
//! shows" and "The path" in `docs/designs/viewing-code.md`).

use std::rc::Rc;

use unfold_core::vocab::{Form, Language, NodeKind};
use unfold_core::{ByteRange, Input, Key, RepoPath};

use super::{CodeSources, PathStep, Target, settings_input};
use crate::catalog::{self, Start};
use crate::contracts::Facts;
use crate::error::Result;
use crate::tree::FileTree;

/// Where a target's lines come from.
pub(super) enum Origin {
    /// An analyzed source file.
    Source(Rc<FileTree>),
    /// An input of the compile, hashed but never parsed: `unfold.toml` or an evidence file.
    Input(Input),
    /// A file outside the graph.
    Outside(RepoPath),
}

impl Origin {
    /// The file's path.
    pub(super) fn path(&self) -> &RepoPath {
        match self {
            Self::Source(tree) => tree.path(),
            Self::Input(input) => &input.path,
            Self::Outside(path) => path,
        }
    }
}

/// A part of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Span {
    /// The whole file.
    Whole,
    /// The lines a stored range starts and ends on.
    Bytes(ByteRange),
    /// One-based lines, first and last.
    Lines([u32; 2]),
}

/// What a selection marks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct MarkSpec {
    /// The lines to tint.
    pub lines: Option<Span>,
    /// A call's own expression.
    pub expression: Option<ByteRange>,
}

/// Which step of the path a view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Shows {
    /// The file.
    File,
    /// The step with this key.
    Key(Key),
    /// The last step: lines or a field.
    Last,
}

/// One step of a path, before it is labeled.
enum Step {
    File,
    Block(u32),
    Named { label: String, key: Option<Key> },
}

/// A target resolved against the records.
pub(super) struct Resolved {
    /// Its file.
    pub origin: Origin,
    /// The lines it shows.
    pub view: Span,
    /// What it marks in its own view.
    pub mark: MarkSpec,
    /// What it marks once the path widened the view.
    pub region: MarkSpec,
    /// Which step its own view shows.
    pub shows: Shows,
    steps: Vec<Step>,
}

/// `target` against the records; `None` when its key no longer resolves.
pub(super) fn resolve(
    facts: &mut Facts<'_>,
    sources: &CodeSources<'_>,
    target: &Target,
) -> Result<Option<Resolved>> {
    let resolved = match target {
        Target::Block(key) => facts.locate(key.as_str())?.map(|(tree, id)| block(tree, id)),
        Target::Lines {
            path,
            lines,
            mark,
            title,
        } => {
            let origin = origin(facts, sources, path)?;
            let mut steps = vec![Step::File];
            if let Origin::Source(tree) = &origin {
                steps.extend(chain(tree, enclosing(tree, *lines)).map(Step::Block));
            }
            steps.push(Step::Named {
                label: title.clone().unwrap_or_else(|| lines_label(*lines)),
                key: None,
            });
            Some(Resolved {
                origin,
                view: Span::Lines(*lines),
                mark: MarkSpec {
                    lines: mark.map(Span::Lines),
                    expression: None,
                },
                region: tint(Span::Lines(mark.unwrap_or(*lines))),
                shows: Shows::Last,
                steps,
            })
        }
        Target::Field { key, index } => facts
            .locate(key.as_str())?
            .and_then(|(tree, id)| field(tree, id, *index)),
        Target::File(path) => Some(Resolved {
            origin: origin(facts, sources, path)?,
            view: Span::Whole,
            mark: MarkSpec::default(),
            region: MarkSpec::default(),
            shows: Shows::File,
            steps: vec![Step::File],
        }),
        Target::Start { key, span } => catalog::start(sources.apps, key.as_str())
            .zip(settings_input(sources.inputs))
            .map(|(start, input)| {
                let app = start.app();
                let mut steps = vec![
                    Step::File,
                    Step::Named {
                        label: app.name.to_string(),
                        key: Some(app.key.clone()),
                    },
                ];
                if let Start::Entrance { entrance, .. } = start {
                    steps.push(Step::Named {
                        label: entrance.name.to_string(),
                        key: Some(entrance.key.clone()),
                    });
                }
                Resolved {
                    origin: Origin::Input(input.clone()),
                    view: Span::Bytes(*span),
                    mark: MarkSpec::default(),
                    region: tint(Span::Bytes(*span)),
                    shows: Shows::Key(start.key().clone()),
                    steps,
                }
            }),
    };
    Ok(resolved)
}

/// The file at `path`: analyzed, an input, or outside the graph.
fn origin(facts: &mut Facts<'_>, sources: &CodeSources<'_>, path: &RepoPath) -> Result<Origin> {
    if let Some(tree) = facts.file(path)? {
        return Ok(Origin::Source(tree));
    }
    let input = sources.inputs.iter().find(|input| input.path == *path);
    Ok(input.map_or_else(
        || Origin::Outside(path.clone()),
        |input| Origin::Input(input.clone()),
    ))
}

/// A mark that tints `lines`.
fn tint(lines: Span) -> MarkSpec {
    MarkSpec {
        lines: Some(lines),
        expression: None,
    }
}

/// What block `id` shows and marks, per kind.
fn block(tree: Rc<FileTree>, id: u32) -> Resolved {
    let node = tree.node(id);
    let own = Shows::Key(node.key.clone());
    let (view, mark, region, shows) = match node.kind() {
        NodeKind::Module => (Span::Whole, MarkSpec::default(), MarkSpec::default(), Shows::File),
        NodeKind::Class | NodeKind::Function | NodeKind::Type => {
            let extent = Span::Bytes(tree.extent(id));
            (extent, MarkSpec::default(), tint(extent), own)
        }
        NodeKind::Decision | NodeKind::Loop | NodeKind::Try | NodeKind::With => {
            let range = Span::Bytes(node.range);
            (range, MarkSpec::default(), tint(range), own)
        }
        NodeKind::Part => {
            let control = node.parent.unwrap_or(id);
            let part = tint(Span::Bytes(part_lines(&tree, id)));
            let shows = Shows::Key(tree.key(control).clone());
            (Span::Bytes(tree.node(control).range), part, part, shows)
        }
        NodeKind::Call => {
            let expression = MarkSpec {
                lines: None,
                expression: Some(node.range),
            };
            let region = MarkSpec {
                lines: Some(Span::Bytes(node.range)),
                expression: Some(node.range),
            };
            (Span::Bytes(statement(&tree, id)), expression, region, own)
        }
        NodeKind::Exit => {
            let statement = Span::Bytes(node.stmt.unwrap_or(node.range));
            (statement, MarkSpec::default(), tint(statement), own)
        }
    };
    let steps = std::iter::once(Step::File)
        .chain(chain(&tree, id).map(Step::Block))
        .collect();
    Resolved {
        origin: Origin::Source(tree),
        view,
        mark,
        region,
        shows,
        steps,
    }
}

/// Member `index` of class or type `id`, shown as its line: the whole member as written.
fn field(tree: Rc<FileTree>, id: u32, index: u32) -> Option<Resolved> {
    let member = tree.members(id).iter().find(|member| member.index == index)?;
    let line = Span::Bytes(member.range);
    let label = member
        .name
        .as_deref()
        .map_or_else(|| member.kind.as_str().to_owned(), str::to_owned);
    let steps = std::iter::once(Step::File)
        .chain(chain(&tree, id).map(Step::Block))
        .chain([Step::Named { label, key: None }])
        .collect();
    Some(Resolved {
        origin: Origin::Source(tree),
        view: line,
        mark: MarkSpec::default(),
        region: tint(line),
        shows: Shows::Last,
        steps,
    })
}

/// What part `id` marks: the line that opens it and its body. A `body` part has no opening
/// line of its own, so it starts on its control block's header.
fn part_lines(tree: &FileTree, id: u32) -> ByteRange {
    let node = tree.node(id);
    match node.parent.filter(|_| node.form == Form::Body) {
        Some(control) => ByteRange {
            start: tree.node(control).range.start,
            end: node.range.end,
        },
        None => node.range,
    }
}

/// The statement call `id` is written in: its own `stmt`, or that of the nearest block above it
/// that has one. A call in a test or a header shows that test or header; a call that runs when
/// a definition is created (a decorator, a base, a default) shows itself. A part of a ternary
/// or comprehension is inside an expression, so the climb goes on to the statement holding it.
fn statement(tree: &FileTree, id: u32) -> ByteRange {
    let call = tree.node(id).range;
    let mut at = Some(id);
    while let Some(current) = at {
        let node = tree.node(current);
        if let Some(stmt) = node.stmt {
            return stmt;
        }
        match node.kind() {
            NodeKind::Call | NodeKind::Exit => {}
            NodeKind::Decision | NodeKind::Loop if is_expression(node.form) => {}
            NodeKind::Function if node.form == Form::Lambda || parent_is_call(tree, current) => {}
            NodeKind::Part if parent_is_expression(tree, current) => {}
            NodeKind::Decision | NodeKind::Loop | NodeKind::Try | NodeKind::With | NodeKind::Part => {
                return node.head.filter(|head| head.contains(call)).unwrap_or(call);
            }
            NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Type => return call,
        }
        at = node.parent;
    }
    call
}

/// Whether `form` is a control block written as an expression.
pub(super) fn is_expression(form: Form) -> bool {
    matches!(form, Form::Ternary | Form::Comprehension)
}

/// Whether node `id`'s parent is a ternary or comprehension.
fn parent_is_expression(tree: &FileTree, id: u32) -> bool {
    tree.parent(id)
        .is_some_and(|parent| is_expression(tree.node(parent).form))
}

/// Whether node `id` is written in a call's arguments.
fn parent_is_call(tree: &FileTree, id: u32) -> bool {
    tree.parent(id)
        .is_some_and(|parent| tree.kind(parent) == NodeKind::Call)
}

/// The innermost block of `tree` other than a call or an exit whose lines hold all of `lines`.
fn enclosing(tree: &FileTree, [first, last]: [u32; 2]) -> u32 {
    let mut best = 0;
    for (id, node) in tree.nodes().iter().enumerate().skip(1) {
        let id = crate::tree::to_u32(id);
        let holds = node.lines.start <= first && last <= node.lines.end;
        if holds && !matches!(node.kind(), NodeKind::Call | NodeKind::Exit) && tree.contains(best, id) {
            best = id;
        }
    }
    best
}

/// Block `id` and the blocks above it, outermost first, as the path shows them: the module is
/// the file's step, and `body` parts are left out.
fn chain(tree: &FileTree, id: u32) -> impl Iterator<Item = u32> {
    let mut blocks = Vec::new();
    let mut at = Some(id);
    while let Some(current) = at {
        let node = tree.node(current);
        if node.parent.is_some() && node.form != Form::Body {
            blocks.push(current);
        }
        at = node.parent;
    }
    blocks.into_iter().rev()
}

/// A gap tick's label: `line 24`, or `lines 24–26`.
fn lines_label([first, last]: [u32; 2]) -> String {
    if first == last {
        format!("line {first}")
    } else {
        format!("lines {first}–{last}")
    }
}

/// The labeled path of `resolved`, with no step showing yet. Headers and tests are read from
/// `text`, the verified source; without it they fall back to their keywords.
pub(super) fn path(resolved: &Resolved, text: Option<&str>) -> Vec<PathStep> {
    let tree = match &resolved.origin {
        Origin::Source(tree) => Some(&**tree),
        Origin::Input(_) | Origin::Outside(_) => None,
    };
    resolved
        .steps
        .iter()
        .filter_map(|step| {
            let (label, key) = match step {
                Step::File => (resolved.origin.path().as_str().to_owned(), None),
                // Block steps come only from source files.
                Step::Block(id) => tree.map(|tree| (label(tree, *id, text), Some(tree.key(*id).clone())))?,
                Step::Named { label, key } => (label.clone(), key.clone()),
            };
            Some(PathStep {
                label,
                key,
                showing: false,
            })
        })
        .collect()
}

/// Block `id`'s label in the path.
fn label(tree: &FileTree, id: u32, text: Option<&str>) -> String {
    let node = tree.node(id);
    let head = node
        .head
        .and_then(|head| text?.get(head.as_usize()))
        .map(|head| head.split_whitespace().collect::<Vec<_>>().join(" "));
    let keyword = node.form.as_str();
    match node.kind() {
        NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Type => tree.label(id),
        // A `switch` is headed by its discriminant and a `match` by its subject.
        NodeKind::Decision => match (node.form, head) {
            (Form::Switch, Some(head)) => format!("{keyword} ({head})"),
            (Form::Match, Some(head)) => format!("{keyword} {head}"),
            (_, head) => head.unwrap_or_else(|| keyword.to_owned()),
        },
        NodeKind::Loop | NodeKind::Try | NodeKind::With => head.unwrap_or_else(|| keyword.to_owned()),
        NodeKind::Part => match (node.form, head) {
            (Form::Then | Form::Except | Form::Catch, Some(head)) => head,
            // A Python `default` is the guardless `case _:`, which has no head.
            (Form::Default, _) if tree.language() == Language::Python => "case _".to_owned(),
            (Form::Elif | Form::Case, Some(head)) => format!("{keyword} {head}"),
            _ => keyword.to_owned(),
        },
        NodeKind::Call => tree
            .calls_link(id)
            .map(|link| link.text.to_string())
            .or_else(|| node.name.as_deref().map(str::to_owned))
            .unwrap_or_else(|| keyword.to_owned()),
        NodeKind::Exit => keyword.to_owned(),
    }
}
