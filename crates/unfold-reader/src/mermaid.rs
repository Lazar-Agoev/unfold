//! Mermaid export (architecture §10.5, decision U-27): a board, an entrance, or one Flow as a
//! Mermaid flowchart.
//!
//! - Each card becomes a node (an entrance, or a Code-view card as its name and location) or a
//!   subgraph (a Flow-view card, with its groups as nested subgraphs).
//! - Arrows between cards come from the graph: solid for calls and constructions, dashed for
//!   imports and passed references; an entrance points at its handlers.
//! - Node labels are exactly the app's: the board's authored label, or the fallback name from
//!   the code, never exact source. An outdated description or label is marked
//!   `(description outdated)`; references held for review and broken groups are drawn as
//!   opaque "needs review" nodes instead of being applied to changed code.
//! - Output is deterministic: it depends only on the board, the graph, and verified sources.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{EdgeKind, Resolution};
use unfold_core::{Entrance, Key};

use crate::board::{Board, BoardGraph, Card, CardRef, HealthKind, Target, View, health};
use crate::flow::{FlowContext, FlowModel, FlowNode, FlowNodeKind};
use crate::notes::note;

const CLASS_DEF: &str = "  classDef opaque fill:#fff4dc,stroke:#a66b10,stroke-dasharray:4 3";
const OUTDATED: &str = " (description outdated)";

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
    let connector = if dashed { "-.->" } else { "-->" };
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

/// The board an entrance export draws: the entrance card and a Flow card for each distinct
/// resolved target, in registration order.
pub fn entrance_board(graph: &BoardGraph, key: &str) -> Option<Board> {
    let entrance = graph.entrance(key)?;
    let mut board = Board::new(&entrance_title(entrance));
    board.cards.push(Card {
        id: "entrance".to_owned(),
        reference: CardRef::Entrance(entrance.key.clone()),
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
        if board.card_for_block(target_key.as_str()).is_some() {
            continue;
        }
        let row = u32::try_from(index).unwrap_or(u32::MAX);
        board.cards.push(Card {
            id: format!("handler-{}", index + 1),
            reference: CardRef::Block(target_key.clone()),
            at: [420.0, f64::from(row) * 180.0],
            view: Some(View::Flow),
            folded: false,
            description: None,
        });
    }
    Some(board)
}

/// An entrance and its handlers as a diagram.
pub fn entrance(graph: &BoardGraph, key: &str) -> Option<String> {
    entrance_board(graph, key).map(|board| self::board(&board, graph))
}

/// Labels for Flows inside a board export: authored labels (marked when outdated, dropped when
/// held for review) and target names from the graph.
struct Context<'a> {
    labels: FxHashMap<&'a str, String>,
    graph: &'a BoardGraph,
}

impl FlowContext for Context<'_> {
    fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }

    fn target_name(&self, key: &str) -> Option<String> {
        self.graph.qualified_name(key)
    }
}

fn with_description(
    title: String,
    description: Option<&crate::board::Description>,
    outdated: bool,
) -> String {
    match description {
        Some(description) => {
            let mark = if outdated { OUTDATED } else { "" };
            format!("{title}\n{}{mark}", description.text)
        }
        None => title,
    }
}

/// A whole board as a diagram.
pub fn board(board: &Board, graph: &BoardGraph) -> String {
    let health = health(board, graph);
    let broken: FxHashSet<&Target> = health
        .items
        .iter()
        .filter(|item| item.kind != HealthKind::Outdated)
        .map(|item| &item.target)
        .collect();
    let is_outdated = |target: &Target| health.has(HealthKind::Outdated, target);
    let mut lines = vec![
        "flowchart LR".to_owned(),
        "  %% Paths from written structure; call arrows come from the index.".to_owned(),
    ];
    let mut labels = FxHashMap::default();
    for (key, label) in &board.labels {
        let target = Target::Label(key.clone());
        if broken.contains(&target) || label.text.trim().is_empty() {
            continue;
        }
        let text = if is_outdated(&target) {
            format!("{}{OUTDATED}", label.text)
        } else {
            label.text.clone()
        };
        labels.insert(key.as_str(), text);
    }
    let context = Context { labels, graph };
    let mut first_card: FxHashMap<&str, &str> = FxHashMap::default();
    let mut ports: FxHashMap<&str, String> = FxHashMap::default();
    let mut flows: FxHashMap<&str, FlowModel> = FxHashMap::default();
    for (index, card) in board.cards.iter().enumerate() {
        let id = format!("c{index}");
        let target = Target::Card(card.id.clone());
        let key = card.reference.key().as_str();
        if broken.contains(&target) {
            lines.push(format!(
                "  {}",
                shape(&id, Shape::Box, &format!("Reference needs review: {key}"))
            ));
            lines.push(format!("  class {id} opaque"));
            ports.insert(&card.id, id);
            continue;
        }
        if let CardRef::Entrance(_) = card.reference {
            let title = graph
                .entrance(key)
                .map_or_else(|| format!("Missing entrance: {key}"), entrance_title);
            lines.push(format!("  {}", shape(&id, Shape::Stadium, &title)));
            ports.insert(&card.id, id);
            continue;
        }
        first_card.entry(key).or_insert(&card.id);
        let Some(block) = graph.block(key) else {
            lines.push(format!(
                "  {}",
                shape(&id, Shape::Box, &format!("Missing definition: {key}"))
            ));
            lines.push(format!("  class {id} opaque"));
            ports.insert(&card.id, id);
            continue;
        };
        let name = graph.qualified_name(key).unwrap_or_else(|| key.to_owned());
        let flow = (board.view_of(card) == View::Flow)
            .then(|| graph.flow(key, &context))
            .flatten();
        let Some(flow) = flow else {
            let location = format!(
                "{name} · {}:{}",
                block.file().file.path,
                block.block().lines.start
            );
            let label = with_description(location, card.description.as_ref(), is_outdated(&target));
            lines.push(format!("  {}", shape(&id, Shape::Box, &label)));
            ports.insert(&card.id, id);
            continue;
        };
        ports.insert(&card.id, format!("{id}_n{}", flow.start));
        let title = with_description(name, card.description.as_ref(), is_outdated(&target));
        lines.push(format!("  subgraph {id}_card[\"{}\"]", escape(&title)));
        lines.push("    direction TB".to_owned());
        let card_groups: Vec<_> = board
            .groups
            .iter()
            .filter(|group| group.card == card.id)
            .collect();
        let groups: Vec<_> = card_groups
            .iter()
            .filter(|group| !broken.contains(&Target::Group(group.id.clone())))
            .collect();
        let grouped: FxHashSet<&Key> = groups.iter().flat_map(|group| group.members.iter()).collect();
        let prefix = format!("{id}_");
        for node in &flow.nodes {
            if node.key.as_ref().is_none_or(|key| !grouped.contains(key)) {
                lines.push(format!("    {}", node_line(&prefix, node)));
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
            lines.push(format!("    subgraph {id}_g{index}[\"{}\"]", escape(&title)));
            for node in members {
                lines.push(format!("      {}", node_line(&prefix, node)));
            }
            lines.push("    end".to_owned());
        }
        for route in &flow.routes {
            let (from, to) = (
                format!("{prefix}n{}", route.from),
                format!("{prefix}n{}", route.to),
            );
            lines.push(format!(
                "    {}",
                arrow(&from, route.kind.dashed(), route.kind.label(), &to)
            ));
        }
        for node in flow.nodes.iter().filter(|node| node.kind == FlowNodeKind::Opaque) {
            lines.push(format!("    class {prefix}n{} opaque", node.id));
        }
        let broken_groups = card_groups
            .iter()
            .filter(|group| broken.contains(&Target::Group(group.id.clone())));
        for (index, group) in broken_groups.enumerate() {
            let review = format!("{id}_review{index}");
            lines.push(format!(
                "    {}",
                shape(
                    &review,
                    Shape::Box,
                    &format!("Group needs review: {}", group.title)
                )
            ));
            lines.push(format!("    class {review} opaque"));
        }
        for warning in &flow.warnings {
            lines.push(format!("    %% {}", comment(warning.message)));
        }
        lines.push("  end".to_owned());
        flows.insert(&card.id, flow);
    }
    for (index, card) in board.cards.iter().enumerate() {
        let id = format!("c{index}");
        if broken.contains(&Target::Card(card.id.clone())) {
            continue;
        }
        let key = card.reference.key().as_str();
        if let CardRef::Entrance(_) = card.reference {
            let Some(entrance) = graph.entrance(key) else {
                continue;
            };
            for (position, target) in entrance.targets.iter().enumerate() {
                let resolved = target
                    .target
                    .as_ref()
                    .filter(|_| target.resolution == Resolution::Resolved);
                let port = resolved
                    .and_then(|target_key| first_card.get(target_key.as_str()))
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
        for (source, edge) in graph.owned_edges(key) {
            let Some(target_card) = edge
                .target
                .as_ref()
                .filter(|_| edge.resolution == Resolution::Resolved)
                .and_then(|target| first_card.get(target.as_str()))
            else {
                continue;
            };
            let Some(port) = ports.get(target_card) else {
                continue;
            };
            let from = match holders.get(&(source.as_str(), edge.seq)) {
                Some(node) => format!("{id}_n{node}"),
                None => ports.get(card.id.as_str()).cloned().unwrap_or_else(|| id.clone()),
            };
            let dashed = !matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs);
            lines.push(format!("  {}", arrow(&from, dashed, edge.kind.as_str(), port)));
        }
    }
    for item in &health.items {
        let kind = match item.kind {
            HealthKind::Missing => "missing",
            HealthKind::Outdated => "outdated",
            HealthKind::BrokenGroup => "brokenGroup",
            HealthKind::Review => "review",
        };
        lines.push(format!("  %% {kind}: {}", comment(&item.message)));
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
