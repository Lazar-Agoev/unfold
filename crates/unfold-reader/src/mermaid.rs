//! Mermaid export (architecture §10.5, decision U-27): a board page, an entrance, or one Flow as
//! a Mermaid flowchart.
//!
//! - Each card becomes a node (an entrance, or a Code- or Diagram-view card as its name and
//!   location; a type declaration in Flow view too) or a subgraph (a Flow-view card, with its
//!   groups as nested subgraphs). A card from a project that is not connected is a "From <name>,
//!   not connected" placeholder.
//! - Resource cards are nodes shaped by their kind; frames are subgraphs around their members,
//!   nested as the frames are; notes are `%%` comments where they sit.
//! - Arrows between cards of one project come from its graph: solid for calls and
//!   constructions, dashed for imports and passed references; a Diagram-view card draws one
//!   arrow to each card it calls ("3 calls"); an entrance points at its handlers. Links drawn on the page are dashed arrows with their label; a connection's line
//!   and words follow its kind.
//! - Node labels are exactly the app's: the board's authored label, or the fallback name from
//!   the code, never exact source. An outdated description or label is marked
//!   `(description outdated)`, and so is an outdated frame description; an outdated resource
//!   card or connection is marked `(outdated)`. References held for review and broken groups
//!   are drawn as opaque "needs review" nodes instead of being applied to changed code.
//! - Output is deterministic: it depends only on the page, the graphs, and verified sources.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{BlockKind, EdgeKind, Resolution};
use unfold_core::{Entrance, Key};

use crate::board::{
    Board, BoardGraph, Card, CardRef, Column, ColumnKey, Description, FrameDirection, HealthKind, Link,
    LinkKind, Note, Page, PageGraphs, ProjectKey, ProjectName, Resource, ResourceKind, Target, View, health,
};
use crate::flow::{FlowContext, FlowModel, FlowNode, FlowNodeKind};
use crate::notes::note;

const CLASS_DEF: &str = "  classDef opaque fill:#fff4dc,stroke:#a66b10,stroke-dasharray:4 3";
const OUTDATED: &str = " (description outdated)";
const CLAIM_OUTDATED: &str = " (outdated)";

/// Escapes text for a quoted Mermaid label: markup and syntax characters become entity codes
/// (`#34;`), and line breaks become `<br/>`.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '&' | '<' | '>' | '"' | '#' | '|' | '\\' | '`' | '[' | ']' | '{' | '}' => {
                out.push('#');
                out.push_str(&u32::from(character).to_string());
                out.push(';');
            }
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                out.push_str("<br/>");
            }
            '\n' => out.push_str("<br/>"),
            _ => out.push(character),
        }
    }
    out
}

/// A node shape.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Stadium,
    Diamond,
    Hexagon,
    Subroutine,
    Circle,
    Flag,
    Box,
    Cylinder,
    Lean,
    LeanLeft,
    Trapezoid,
    DoubleCircle,
}

impl Shape {
    fn of(kind: FlowNodeKind) -> Self {
        match kind {
            FlowNodeKind::Start | FlowNodeKind::End => Self::Stadium,
            FlowNodeKind::Condition => Self::Diamond,
            FlowNodeKind::Loop => Self::Hexagon,
            FlowNodeKind::Call => Self::Subroutine,
            FlowNodeKind::Return | FlowNodeKind::Break | FlowNodeKind::Continue => Self::Circle,
            FlowNodeKind::Throw => Self::Flag,
            FlowNodeKind::Variable
            | FlowNodeKind::Statement
            | FlowNodeKind::Definition
            | FlowNodeKind::Opaque => Self::Box,
        }
    }

    /// A resource card's shape: a table is a cylinder, a queue a parallelogram, a cache the
    /// other parallelogram, storage a trapezoid, an outside service a hexagon, an actor a double
    /// circle.
    fn of_resource(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Table => Self::Cylinder,
            ResourceKind::Queue => Self::Lean,
            ResourceKind::Cache => Self::LeanLeft,
            ResourceKind::Storage => Self::Trapezoid,
            ResourceKind::External => Self::Hexagon,
            ResourceKind::Actor => Self::DoubleCircle,
        }
    }
}

fn shape(id: &str, shape: Shape, label: &str) -> String {
    let label = escape(label);
    match shape {
        Shape::Stadium => format!("{id}([\"{label}\"])"),
        Shape::Diamond => format!("{id}{{\"{label}\"}}"),
        Shape::Hexagon => format!("{id}{{{{\"{label}\"}}}}"),
        Shape::Subroutine => format!("{id}[[\"{label}\"]]"),
        Shape::Circle => format!("{id}((\"{label}\"))"),
        Shape::Flag => format!("{id}>\"{label}\"]"),
        Shape::Box => format!("{id}[\"{label}\"]"),
        Shape::Cylinder => format!("{id}[(\"{label}\")]"),
        Shape::Lean => format!("{id}[/\"{label}\"/]"),
        Shape::LeanLeft => format!("{id}[\\\"{label}\"\\]"),
        Shape::Trapezoid => format!("{id}[/\"{label}\"\\]"),
        Shape::DoubleCircle => format!("{id}(((\"{label}\")))"),
    }
}

/// A node's label with the nested definitions it lists.
fn node_text(node: &FlowNode) -> String {
    let mut text = node.label.clone();
    for callback in &node.callbacks {
        text.push_str("\nƒ ");
        text.push_str(&callback.label);
        text.push_str(" (separate definition)");
    }
    text
}

fn node_line(prefix: &str, node: &FlowNode) -> String {
    shape(
        &format!("{prefix}n{}", node.id),
        Shape::of(node.kind),
        &node_text(node),
    )
}

fn arrow(from: &str, dashed: bool, label: &str, to: &str) -> String {
    line(from, if dashed { "-.->" } else { "-->" }, label, to)
}

/// A line drawn with `connector` (`-->`, `-.->`, `==>`, `---`), with its label when it has one.
fn line(from: &str, connector: &str, label: &str, to: &str) -> String {
    if label.is_empty() {
        format!("{from} {connector} {to}")
    } else {
        format!("{from} {connector}|\"{}\"| {to}", escape(label))
    }
}

/// Text for a `%%` comment line: every line break (including U+2028 and U+2029, which
/// JavaScript treats as line terminators) becomes a space, so the text cannot start a new line.
fn comment(text: &str) -> String {
    text.replace(['\r', '\n', '\u{2028}', '\u{2029}'], " ")
}

/// One Flow as its own diagram.
pub fn flow(flow: &FlowModel) -> String {
    let mut lines = vec![
        "flowchart TD".to_owned(),
        "  %% Paths from written structure; not a runtime trace.".to_owned(),
    ];
    for node in &flow.nodes {
        lines.push(format!("  {}", node_line("f_", node)));
    }
    for route in &flow.routes {
        let (from, to) = (format!("f_n{}", route.from), format!("f_n{}", route.to));
        lines.push(format!(
            "  {}",
            arrow(&from, route.kind.dashed(), route.kind.label(), &to)
        ));
    }
    for node in flow.nodes.iter().filter(|node| node.kind == FlowNodeKind::Opaque) {
        lines.push(format!("  class f_n{} opaque", node.id));
    }
    for warning in &flow.warnings {
        lines.push(format!("  %% {}", comment(warning.message)));
    }
    lines.push(CLASS_DEF.to_owned());
    lines.join("\n") + "\n"
}

/// An entrance's title: its label, or its method (or kind) and trigger.
pub fn entrance_title(entrance: &Entrance) -> String {
    if let Some(label) = entrance.label.as_deref().filter(|label| !label.is_empty()) {
        return label.to_owned();
    }
    let head = entrance
        .method
        .map_or(entrance.kind.as_str(), |method| method.as_str());
    format!(
        "{head} {}",
        entrance.trigger.as_deref().unwrap_or("unknown trigger")
    )
}

/// The board an entrance export draws: one page with the entrance card and a Flow card for
/// each distinct resolved target, in registration order.
pub fn entrance_board(graph: &BoardGraph, key: &str) -> Option<Board> {
    let entrance = graph.entrance(key)?;
    let mut board = Board::new(&entrance_title(entrance));
    let page = &mut board.pages[0];
    page.cards.push(Card {
        id: "entrance".to_owned(),
        reference: CardRef::entrance(entrance.key.clone(), None),
        at: [0.0, 0.0],
        view: Some(View::Code),
        folded: false,
        description: None,
    });
    for (index, target) in entrance.targets.iter().enumerate() {
        let Some(target_key) = target
            .target
            .as_ref()
            .filter(|_| target.resolution == Resolution::Resolved)
        else {
            continue;
        };
        if page
            .cards
            .iter()
            .any(|card| card.reference.block_key() == Some(target_key))
        {
            continue;
        }
        let row = u32::try_from(index).unwrap_or(u32::MAX);
        page.cards.push(Card {
            id: format!("handler-{}", index + 1),
            reference: CardRef::block(target_key.clone(), None),
            at: [420.0, f64::from(row) * 180.0],
            view: Some(View::Flow),
            folded: false,
            description: None,
        });
    }
    Some(board)
}

/// An entrance and its handlers as a diagram, from `graph` (the entrance's project's).
pub fn entrance(graph: &BoardGraph, key: &str) -> Option<String> {
    let board = entrance_board(graph, key)?;
    Some(render(&board.pages[0], &PageGraphs::own_only(graph)))
}

/// Board page `page` as a diagram, or `None` when the board has no such page.
pub fn page(board: &Board, page: &str, graphs: &PageGraphs<'_>) -> Option<String> {
    board.page(page).map(|page| render(page, graphs))
}

/// The labels of one project's blocks for Flows inside an export (authored labels, marked when
/// outdated, dropped when held for review) and target names from its graph.
struct Context<'a> {
    labels: Option<&'a FxHashMap<&'a str, String>>,
    graph: &'a BoardGraph,
}

impl FlowContext for Context<'_> {
    fn label(&self, key: &str) -> Option<&str> {
        self.labels?.get(key).map(String::as_str)
    }

    fn target_name(&self, key: &str) -> Option<String> {
        self.graph.qualified_name(key)
    }
}

fn with_description(title: String, description: Option<&Description>, outdated: bool) -> String {
    match description {
        Some(description) => {
            let mark = if outdated { OUTDATED } else { "" };
            format!("{title}\n{}{mark}", description.text)
        }
        None => title,
    }
}

fn health_kind(kind: HealthKind) -> &'static str {
    match kind {
        HealthKind::Missing => "missing",
        HealthKind::Outdated => "outdated",
        HealthKind::BrokenGroup => "brokenGroup",
        HealthKind::Review => "review",
        HealthKind::NotConnected => "notConnected",
        HealthKind::BrokenLink => "brokenLink",
    }
}

/// A column of a table card: its name, its type, and its marks.
fn column_text(column: &Column) -> String {
    let mut parts = vec![match &column.written_type {
        Some(written) => format!("{}: {written}", column.name),
        None => column.name.clone(),
    }];
    match (column.key, &column.references) {
        (Some(ColumnKey::Primary), _) => parts.push("PK".to_owned()),
        (Some(ColumnKey::Foreign), Some(references)) => parts.push(format!("FK → {references}")),
        (Some(ColumnKey::Foreign), None) => parts.push("FK".to_owned()),
        (Some(ColumnKey::Unique), _) => parts.push("unique".to_owned()),
        (None, _) => {}
    }
    if column.optional {
        parts.push("optional".to_owned());
    }
    parts.join(" · ")
}

/// A resource card's node: its kind's shape around its kind and name (marked when `outdated`),
/// its columns, and its description.
fn resource_node(id: &str, resource: &Resource, outdated: bool) -> String {
    let mark = if outdated { CLAIM_OUTDATED } else { "" };
    let mut label = format!("{} · {}{mark}", resource.kind.as_str(), resource.name);
    for column in &resource.columns {
        label.push('\n');
        label.push_str(&column_text(column));
    }
    if let Some(description) = &resource.description {
        label.push('\n');
        label.push_str(&description.text);
    }
    shape(id, Shape::of_resource(resource.kind), &label)
}

/// A note as a comment line, naming what it is attached to.
fn note_comment(note: &Note) -> String {
    match &note.attach {
        Some(target) => format!("%% Note on {target}: {}", comment(&note.text)),
        None => format!("%% Note: {}", comment(&note.text)),
    }
}

/// Writes frame `index` of `page` as subgraph `f<index>`, `depth` levels deep, around its
/// members' lines (taken from `blocks`) in member order; frames inside it are written the same
/// way.
fn write_frame(
    lines: &mut Vec<String>,
    page: &Page,
    (index, depth): (usize, usize),
    blocks: &mut FxHashMap<&str, Vec<String>>,
    outdated: &dyn Fn(&Target) -> bool,
) {
    let frame = &page.frames[index];
    let indent = "  ".repeat(depth);
    let title = with_description(
        frame.title.clone(),
        frame.description.as_ref(),
        outdated(&Target::Frame(frame.id.clone())),
    );
    lines.push(format!("{indent}subgraph f{index}[\"{}\"]", escape(&title)));
    if let Some(direction) = frame.direction {
        let direction = match direction {
            FrameDirection::Right => "LR",
            FrameDirection::Down => "TB",
        };
        lines.push(format!("{indent}  direction {direction}"));
    }
    for member in &frame.members {
        if let Some(inner) = page.frames.iter().position(|item| &item.id == member) {
            write_frame(lines, page, (inner, depth + 1), blocks, outdated);
        } else if let Some(block) = blocks.remove(member.as_str()) {
            lines.extend(block.into_iter().map(|line| format!("{indent}{line}")));
        }
    }
    lines.push(format!("{indent}end"));
}

/// The Flow node of card `card` that shows `step` (its statement, or the statement holding the
/// call), when the card is drawn as a Flow.
fn step_node(page: &Page, flows: &FxHashMap<&str, FlowModel>, card: &str, step: &Key) -> Option<String> {
    let flow = flows.get(card)?;
    let index = page.cards.iter().position(|item| item.id == card)?;
    let node = flow
        .nodes
        .iter()
        .find(|node| node.key.as_ref() == Some(step) || node.chips.iter().any(|chip| &chip.source == step))?;
    Some(format!("c{index}_n{}", node.id))
}

/// A connection's words: `HTTP`, `queue`, `runtime call`, … (none for a drawn link).
fn kind_word(kind: LinkKind) -> Option<&'static str> {
    match kind {
        LinkKind::Http => Some("HTTP"),
        LinkKind::Rpc => Some("RPC"),
        LinkKind::Queue => Some("queue"),
        LinkKind::Event => Some("event"),
        LinkKind::Writes => Some("writes"),
        LinkKind::Reads => Some("reads"),
        LinkKind::Calls => Some("runtime call"),
        LinkKind::Relates => Some("relates"),
        LinkKind::Other => None,
    }
}

/// A link's arrow from `from` to `to`, drawn with its kind's line (solid for requests and
/// writes, thick for queues, dotted for events, runtime calls, and drawn links, a plain line for
/// `relates`; a `reads` arrow points back at the code that reads), and labeled with its kind's
/// word, a relationship's cardinality and columns, its channel, and its label (marked when
/// `outdated`).
fn connection(link: &Link, from: &str, to: &str, outdated: bool) -> String {
    let kind = link.kind.unwrap_or(LinkKind::Other);
    let mut words: Vec<String> = kind_word(kind).map(str::to_owned).into_iter().collect();
    if let Some(cardinality) = link.cardinality {
        words.push(cardinality.as_str().replace('-', " "));
    }
    match (&link.from_column, &link.to_column) {
        (Some(from), Some(to)) => words.push(format!("{from} → {to}")),
        (Some(column), None) | (None, Some(column)) => words.push(column.clone()),
        (None, None) => {}
    }
    words.extend(link.channel.iter().cloned());
    words.extend(link.label.iter().map(|label| label.text.clone()));
    let mut label = words.join(" · ");
    if outdated {
        label.push_str(CLAIM_OUTDATED);
    }
    let connector = match kind {
        LinkKind::Http | LinkKind::Rpc | LinkKind::Writes | LinkKind::Reads => "-->",
        LinkKind::Queue => "==>",
        LinkKind::Relates => "---",
        LinkKind::Event | LinkKind::Calls | LinkKind::Other => "-.->",
    };
    if kind == LinkKind::Reads {
        line(to, connector, &label, from)
    } else {
        line(from, connector, &label, to)
    }
}

/// One page as a diagram.
fn render(page: &Page, graphs: &PageGraphs<'_>) -> String {
    let health = health(page, graphs);
    let broken: FxHashSet<&Target> = health
        .items
        .iter()
        .filter(|item| !matches!(item.kind, HealthKind::Outdated | HealthKind::NotConnected))
        .map(|item| &item.target)
        .collect();
    let is_outdated = |target: &Target| health.has(&page.id, HealthKind::Outdated, target);
    let mut lines = vec![
        "flowchart LR".to_owned(),
        "  %% Paths from written structure; call arrows come from the index.".to_owned(),
    ];
    let mut labels: FxHashMap<Option<&ProjectName>, FxHashMap<&str, String>> = FxHashMap::default();
    for entry in &page.labels {
        let project = entry.project.as_ref();
        let target = Target::Label(ProjectKey::new(project, entry.key.as_str()));
        if graphs.get(project).is_none() || broken.contains(&target) || entry.label.text.trim().is_empty() {
            continue;
        }
        let text = if is_outdated(&target) {
            format!("{}{OUTDATED}", entry.label.text)
        } else {
            entry.label.text.clone()
        };
        labels
            .entry(project)
            .or_default()
            .insert(entry.key.as_str(), text);
    }
    let mut first_card: FxHashMap<(Option<&ProjectName>, &str), &str> = FxHashMap::default();
    let mut ports: FxHashMap<&str, String> = FxHashMap::default();
    let mut flows: FxHashMap<&str, FlowModel> = FxHashMap::default();
    // Each item's lines, written where the item sits: at the top level or inside its frame.
    let mut blocks: FxHashMap<&str, Vec<String>> = FxHashMap::default();
    for (index, card) in page.cards.iter().enumerate() {
        let id = format!("c{index}");
        let target = Target::Card(card.id.clone());
        let out = blocks.entry(card.id.as_str()).or_default();
        let key = card.reference.key().as_str();
        let project = card.reference.project();
        let Some(graph) = graphs.get(project) else {
            let name = project.map_or("", ProjectName::as_str);
            out.push(format!(
                "  {}",
                shape(&id, Shape::Box, &format!("From {name}, not connected"))
            ));
            out.push(format!("  class {id} opaque"));
            ports.insert(&card.id, id);
            continue;
        };
        if broken.contains(&target) {
            out.push(format!(
                "  {}",
                shape(&id, Shape::Box, &format!("Reference needs review: {key}"))
            ));
            out.push(format!("  class {id} opaque"));
            ports.insert(&card.id, id);
            continue;
        }
        if let CardRef::Entrance { .. } = card.reference {
            let title = graph
                .entrance(key)
                .map_or_else(|| format!("Missing entrance: {key}"), entrance_title);
            out.push(format!("  {}", shape(&id, Shape::Stadium, &title)));
            ports.insert(&card.id, id);
            continue;
        }
        first_card.entry((project, key)).or_insert(&card.id);
        let Some(block) = graph.block(key) else {
            out.push(format!(
                "  {}",
                shape(&id, Shape::Box, &format!("Missing definition: {key}"))
            ));
            out.push(format!("  class {id} opaque"));
            ports.insert(&card.id, id);
            continue;
        };
        let name = graph.qualified_name(key).unwrap_or_else(|| key.to_owned());
        let context = Context {
            labels: labels.get(&project),
            graph,
        };
        // A type declaration has no paths: in Flow view it is a box, as its fields are in the app.
        let declaration = matches!(
            block.block().kind,
            BlockKind::Interface | BlockKind::TypeAlias | BlockKind::Enum
        );
        let flow = (page.view_of(card) == View::Flow && !declaration)
            .then(|| graph.flow(key, &context))
            .flatten();
        let Some(flow) = flow else {
            let location = format!(
                "{name} · {}:{}",
                block.file().file.path,
                block.block().lines.start
            );
            let label = with_description(location, card.description.as_ref(), is_outdated(&target));
            out.push(format!("  {}", shape(&id, Shape::Box, &label)));
            ports.insert(&card.id, id);
            continue;
        };
        ports.insert(&card.id, format!("{id}_n{}", flow.start));
        let title = with_description(name, card.description.as_ref(), is_outdated(&target));
        out.push(format!("  subgraph {id}_card[\"{}\"]", escape(&title)));
        out.push("    direction TB".to_owned());
        let card_groups: Vec<_> = page.groups.iter().filter(|group| group.card == card.id).collect();
        let groups: Vec<_> = card_groups
            .iter()
            .filter(|group| !broken.contains(&Target::Group(group.id.clone())))
            .collect();
        let grouped: FxHashSet<&Key> = groups.iter().flat_map(|group| group.members.iter()).collect();
        let prefix = format!("{id}_");
        for node in &flow.nodes {
            if node.key.as_ref().is_none_or(|key| !grouped.contains(key)) {
                out.push(format!("    {}", node_line(&prefix, node)));
            }
        }
        for (index, group) in groups.iter().enumerate() {
            let members: Vec<&FlowNode> = flow
                .nodes
                .iter()
                .filter(|node| node.key.as_ref().is_some_and(|key| group.members.contains(key)))
                .collect();
            if members.is_empty() {
                continue;
            }
            let title = with_description(
                group.title.clone(),
                group.description.as_ref(),
                is_outdated(&Target::Group(group.id.clone())),
            );
            out.push(format!("    subgraph {id}_g{index}[\"{}\"]", escape(&title)));
            for node in members {
                out.push(format!("      {}", node_line(&prefix, node)));
            }
            out.push("    end".to_owned());
        }
        for route in &flow.routes {
            let (from, to) = (
                format!("{prefix}n{}", route.from),
                format!("{prefix}n{}", route.to),
            );
            out.push(format!(
                "    {}",
                arrow(&from, route.kind.dashed(), route.kind.label(), &to)
            ));
        }
        for node in flow.nodes.iter().filter(|node| node.kind == FlowNodeKind::Opaque) {
            out.push(format!("    class {prefix}n{} opaque", node.id));
        }
        let broken_groups = card_groups
            .iter()
            .filter(|group| broken.contains(&Target::Group(group.id.clone())));
        for (index, group) in broken_groups.enumerate() {
            let review = format!("{id}_review{index}");
            out.push(format!(
                "    {}",
                shape(
                    &review,
                    Shape::Box,
                    &format!("Group needs review: {}", group.title)
                )
            ));
            out.push(format!("    class {review} opaque"));
        }
        for warning in &flow.warnings {
            out.push(format!("    %% {}", comment(warning.message)));
        }
        out.push("  end".to_owned());
        flows.insert(&card.id, flow);
    }
    for (index, resource) in page.resources.iter().enumerate() {
        let id = format!("r{index}");
        let outdated = is_outdated(&Target::Resource(resource.id.clone()));
        blocks.insert(
            &resource.id,
            vec![format!("  {}", resource_node(&id, resource, outdated))],
        );
        ports.insert(&resource.id, id);
    }
    for note in &page.notes {
        blocks.insert(&note.id, vec![format!("  {}", note_comment(note))]);
    }
    for (index, frame) in page.frames.iter().enumerate() {
        ports.insert(&frame.id, format!("f{index}"));
    }
    let framed: FxHashSet<&str> = page
        .frames
        .iter()
        .flat_map(|frame| frame.members.iter().map(String::as_str))
        .collect();
    let top = page
        .cards
        .iter()
        .map(|card| card.id.as_str())
        .chain(page.resources.iter().map(|resource| resource.id.as_str()))
        .chain(page.notes.iter().map(|note| note.id.as_str()))
        .filter(|id| !framed.contains(id));
    for id in top {
        lines.extend(blocks.remove(id).unwrap_or_default());
    }
    for (index, frame) in page.frames.iter().enumerate() {
        if !framed.contains(frame.id.as_str()) {
            write_frame(&mut lines, page, (index, 1), &mut blocks, &is_outdated);
        }
    }
    for (index, card) in page.cards.iter().enumerate() {
        let id = format!("c{index}");
        if broken.contains(&Target::Card(card.id.clone())) {
            continue;
        }
        let project = card.reference.project();
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        let key = card.reference.key().as_str();
        if let CardRef::Entrance { .. } = card.reference {
            let Some(entrance) = graph.entrance(key) else {
                continue;
            };
            for (position, target) in entrance.targets.iter().enumerate() {
                let resolved = target
                    .target
                    .as_ref()
                    .filter(|_| target.resolution == Resolution::Resolved);
                let port = resolved
                    .and_then(|target_key| first_card.get(&(project, target_key.as_str())))
                    .and_then(|card_id| ports.get(card_id));
                if let Some(port) = port {
                    lines.push(format!("  {}", arrow(&id, false, "handler", port)));
                    continue;
                }
                let name = resolved
                    .and_then(|target_key| graph.qualified_name(target_key.as_str()))
                    .unwrap_or_else(|| target.text.to_string());
                let detail = match note(target.resolution, target.reason, target.module.as_ref()) {
                    Some(note) => format!("{}: {note}", target.resolution.as_str()),
                    None => target.resolution.as_str().to_owned(),
                };
                let external = format!("{id}_target{position}");
                lines.push(format!(
                    "  {}",
                    shape(&external, Shape::Box, &format!("{name} ({detail})"))
                ));
                lines.push(format!("  {}", arrow(&id, true, "handler", &external)));
            }
            continue;
        }
        if graph.block(key).is_none() {
            continue;
        }
        // The Flow node showing each occurrence edge, by (occurrence, sequence).
        let mut holders: FxHashMap<(&str, u32), u32> = FxHashMap::default();
        if let Some(flow) = flows.get(card.id.as_str()) {
            for node in &flow.nodes {
                for chip in &node.chips {
                    holders.entry((chip.source.as_str(), chip.seq)).or_insert(node.id);
                }
            }
        }
        // A Diagram-view card draws one arrow to each card it calls: its first call's or
        // construction's kind (else its first edge's), or "N calls", solid unless every edge is
        // dashed.
        let diagram = page.view_of(card) == View::Diagram;
        let mut merged: Vec<(&String, &'static str, usize, bool)> = Vec::new();
        for (source, edge) in graph.owned_edges(key) {
            let Some(target_card) = edge
                .target
                .as_ref()
                .filter(|_| edge.resolution == Resolution::Resolved)
                .and_then(|target| first_card.get(&(project, target.as_str())))
            else {
                continue;
            };
            let Some(port) = ports.get(target_card) else {
                continue;
            };
            let dashed = !matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs);
            if diagram {
                match merged.iter_mut().find(|(to, ..)| *to == port) {
                    Some((_, kind, calls, all_dashed)) => {
                        if *all_dashed && !dashed {
                            *kind = edge.kind.as_str();
                        }
                        *calls += 1;
                        *all_dashed &= dashed;
                    }
                    None => merged.push((port, edge.kind.as_str(), 1, dashed)),
                }
                continue;
            }
            let from = match holders.get(&(source.as_str(), edge.seq)) {
                Some(node) => format!("{id}_n{node}"),
                None => ports.get(card.id.as_str()).cloned().unwrap_or_else(|| id.clone()),
            };
            lines.push(format!("  {}", arrow(&from, dashed, edge.kind.as_str(), port)));
        }
        for (port, kind, calls, dashed) in merged {
            let label = if calls == 1 {
                kind.to_owned()
            } else {
                format!("{calls} calls")
            };
            lines.push(format!("  {}", arrow(&id, dashed, &label, port)));
        }
    }
    for link in &page.links {
        let (Some(from), Some(to)) = (ports.get(link.from.as_str()), ports.get(link.to.as_str())) else {
            continue;
        };
        let step = link
            .step
            .as_ref()
            .and_then(|step| step_node(page, &flows, &link.from, step));
        let outdated = is_outdated(&Target::Link(link.id.clone()));
        lines.push(format!(
            "  {}",
            connection(link, step.as_deref().unwrap_or(from), to, outdated)
        ));
    }
    for item in &health.items {
        lines.push(format!(
            "  %% {}: {}",
            health_kind(item.kind),
            comment(&item.message)
        ));
    }
    lines.push(CLASS_DEF.to_owned());
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_escaped_as_entities() {
        assert_eq!(escape("a<b> \"c\" #d|e"), "a#60;b#62; #34;c#34; #35;d#124;e");
        assert_eq!(escape("[x]{y}`z`\\"), "#91;x#93;#123;y#125;#96;z#96;#92;");
        assert_eq!(
            escape("one\r\ntwo\nthree\rfour"),
            "one<br/>two<br/>three<br/>four"
        );
        assert_eq!(comment("a\nb\u{2028}c\u{2029}d\re"), "a b c d e");
    }
}
