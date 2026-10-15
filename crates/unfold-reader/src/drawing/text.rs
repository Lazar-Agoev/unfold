//! Agent text (docs/reader.md#agent-text): the compiled tree for `read_code` ([`code_text`]) and
//! the drawn flow for `shape_card` results and `read_page` ([`flow_text`]). The MCP server prints
//! what these return.
//!
//! Both carry each block's full key at the end of its line, after `key=`, so the server can
//! turn the short label an agent writes into a key by looking it up, never by joining or
//! splitting key text. [`CodeText::labels`] holds the same lookup for the compiled tree.
//!
//! The drawn flow's marks: `▸` a project call (it opens in place), `□` another box (a named or
//! callback-taking library or unknown call), `·` a thin row, `?` an unknown call, `⊞`/`⊟` a
//! collapsed/open combined step, `⋯` a gap tick, `↳` a callback, `+ PROPOSED` a proposal.
//! Lanes are indented four spaces under their line; an inset is indented under its call and
//! ends with `( returns )`.

use std::fmt::Write as _;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use unfold_core::vocab::{CallRole, Flag, Form, Language, LinkKind, NodeKind, Outcome};
use unfold_core::{ByteRange, Key, LineBreaks, LinkTarget};

use super::lines::{FileText, candidates, doc_text};
use super::{
    CallBox, CombinedStep, DrawnFlow, ExitStep, ExitValue, Frame, GapTick, Lane, OtherCall, Route, RouteEnd,
    RouteKind, RouteState, Sequence, Shape, Step, ThinRow,
};
use crate::notes::module_text;
use crate::source::verify_file;
use crate::tree::FileTree;

/// The drawn flow as text: the shaped flow a person sees.
pub fn flow_text(flow: &DrawnFlow) -> String {
    let mut out = String::new();
    Printer::new(flow, false).flow(&mut out, 0);
    out
}

/// Writes one line: the text, the route state when it isn't reached, and the keys.
fn line(out: &mut String, indent: usize, text: &str, state: RouteState, keys: &[&Key]) {
    out.extend(std::iter::repeat_n(' ', indent));
    out.push_str(text);
    match state {
        RouteState::Reached => {}
        RouteState::Unreachable => out.push_str(" · never runs"),
        RouteState::Truncated => out.push_str(" · truncated"),
    }
    for (index, key) in keys.iter().enumerate() {
        out.push_str(if index == 0 { "   key=" } else { " " });
        out.push_str(key.as_str());
    }
    out.push('\n');
}

/// Names with the code they replace kept beside them, or the code alone: the parts given, in
/// order.
fn named(parts: &[Option<&str>]) -> String {
    parts.iter().flatten().copied().collect::<Vec<_>>().join(" · ")
}

fn plural(count: u32, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

fn question(outcome: Outcome) -> &'static str {
    if outcome == Outcome::Unknown { " ?" } else { "" }
}

/// A box's title: its mark, its name and callee, and its `?`.
fn box_title(call: &CallBox) -> String {
    let mark = if call.to.outcome == Outcome::Project {
        "▸"
    } else {
        "□"
    };
    let at = if call.role == Some(CallRole::Decorator) {
        "@"
    } else {
        ""
    };
    let callee = format!("{at}{}", call.callee);
    format!(
        "{mark} {}{}",
        named(&[call.name.as_deref(), Some(&callee)]),
        question(call.to.outcome)
    )
}

/// The calls listed on a box or inside a thin item, depth first.
fn listed(calls: &[OtherCall], out: &mut Vec<String>) {
    for call in calls {
        out.push(format!("{}{}", call.callee, question(call.to.outcome)));
        listed(&call.inside, out);
    }
}

fn plus(calls: &[OtherCall]) -> String {
    let mut names = Vec::new();
    listed(calls, &mut names);
    if names.is_empty() {
        String::new()
    } else {
        format!(" + {}", names.join(", "))
    }
}

fn box_text(call: &CallBox) -> String {
    let mut text = box_title(call);
    if let Some(binds) = &call.binds {
        let _ = write!(text, " → {binds}");
    }
    text.push_str(&plus(&call.calls));
    if call.awaited {
        text.push_str(" · AWAIT");
    }
    if call.sometimes {
        text.push_str(" · SOMETIMES");
    }
    if call.form == Form::New {
        text.push_str(" · new");
    }
    if let Some(feeds) = &call.feeds {
        let _ = write!(
            text,
            " · into {}(…)",
            named(&[feeds.name.as_deref(), Some(&feeds.callee)])
        );
    }
    text
}

fn thin_item(call: &OtherCall) -> String {
    let at = if call.role == Some(CallRole::Decorator) {
        "@"
    } else {
        ""
    };
    let mut text = format!("{at}{}{}", call.callee, question(call.to.outcome));
    if let Some(binds) = &call.binds {
        let _ = write!(text, " → {binds}");
    }
    text.push_str(&plus(&call.inside));
    text
}

fn thin_text(row: &ThinRow) -> String {
    let items: Vec<String> = row.calls.iter().map(thin_item).collect();
    let mut text = format!("· {}", items.join(", "));
    if row.calls.len() > 1 {
        let _ = write!(text, "   {} calls", row.calls.len());
        if row.unknown > 0 {
            let _ = write!(text, ", {} unknown", row.unknown);
        }
    }
    text
}

/// The boxes drawn on a header or test.
fn chips(calls: &[CallBox]) -> String {
    if calls.is_empty() {
        return String::new();
    }
    let titles: Vec<String> = calls.iter().map(box_title).collect();
    format!(" · {}", titles.join(", "))
}

/// The keys of the boxes drawn on a header or test, which their line carries.
fn chip_keys(calls: &[CallBox]) -> impl Iterator<Item = &Key> {
    calls.iter().map(|call| &call.key)
}

fn gap_text(gap: &GapTick) -> String {
    let mut text = format!("⋯ {}", plural(gap.count, "line"));
    if !gap.hidden.is_empty() {
        let labels: Vec<&str> = gap.hidden.iter().map(|hidden| hidden.label.as_str()).collect();
        let _ = write!(text, " · {}", labels.join(", "));
    }
    text
}

/// A lane's test as written.
fn test_text(lane: &Lane) -> Option<&str> {
    lane.test.as_ref().and_then(|test| test.text.as_deref())
}

/// Prints one flow (the card's, or an inset's).
struct Printer<'f> {
    flow: &'f DrawnFlow,
    from: FxHashMap<u32, Vec<&'f Route>>,
    inset: bool,
}

impl<'f> Printer<'f> {
    fn new(flow: &'f DrawnFlow, inset: bool) -> Self {
        let mut from: FxHashMap<u32, Vec<&Route>> = FxHashMap::default();
        for route in &flow.routes {
            from.entry(route.from).or_default().push(route);
        }
        Self { flow, from, inset }
    }

    fn routes(&self, id: u32) -> &[&'f Route] {
        self.from.get(&id).map_or(&[], Vec::as_slice)
    }

    fn flow(&self, out: &mut String, indent: usize) {
        let header = &self.flow.header;
        let title = header.name.as_deref().unwrap_or(&header.label);
        line(
            out,
            indent,
            &format!("( {title} )"),
            RouteState::Reached,
            &[&header.key],
        );
        if header.kind == NodeKind::Type {
            return;
        }
        if !header.defined_with.is_empty() {
            let calls: Vec<String> = header
                .defined_with
                .iter()
                .map(|call| match call.role {
                    Some(CallRole::Decorator) | None => thin_item(call),
                    Some(role) => format!("{} ({role})", thin_item(call)),
                })
                .collect();
            let keys: Vec<&Key> = header.defined_with.iter().map(|call| &call.key).collect();
            let text = format!("runs when defined · {}", calls.join(", "));
            line(out, indent + 2, &text, RouteState::Reached, &keys);
        }
        if !header.imports.is_empty() {
            let imports: Vec<&str> = header.imports.iter().map(|import| import.text.as_str()).collect();
            let text = format!("imports · {}", imports.join(", "));
            line(out, indent + 2, &text, RouteState::Reached, &[]);
        }
        self.sequence(out, &self.flow.steps, indent + 2);
        let end = if self.inset { "( returns )" } else { "( End )" };
        line(out, indent, end, RouteState::Reached, &[]);
    }

    fn sequence(&self, out: &mut String, sequence: &Sequence, indent: usize) {
        for step in &sequence.steps {
            if let Some(gap) = &step.gap {
                line(out, indent, &gap_text(gap), RouteState::Reached, &[]);
            }
            self.step(out, step, indent);
        }
        if let Some(gap) = &sequence.end_gap {
            line(out, indent, &gap_text(gap), RouteState::Reached, &[]);
        }
    }

    fn step(&self, out: &mut String, step: &Step, indent: usize) {
        match &step.shape {
            Shape::Call(call) => {
                line(out, indent, &box_text(call), step.state, &[&call.key]);
                self.below(out, call, indent + 4);
            }
            Shape::Thin(row) => {
                let keys: Vec<&Key> = row.calls.iter().map(|call| &call.key).collect();
                line(out, indent, &thin_text(row), step.state, &keys);
            }
            Shape::Decision(frame) => self.decision(out, step, frame, indent),
            Shape::Loop(frame) => self.looping(out, step, frame, indent),
            Shape::Try(frame) => self.trying(out, step, frame, indent),
            Shape::With(frame) => {
                let body = frame.lanes.first();
                let text = format!(
                    "WITH {}{}",
                    named(&[
                        frame.name.as_deref(),
                        body.and_then(|lane| lane.name.as_deref()),
                        frame.header.as_deref()
                    ]),
                    chips(&frame.header_calls)
                );
                let keys: Vec<&Key> = std::iter::once(&frame.key)
                    .chain(body.map(|lane| &lane.key))
                    .chain(chip_keys(&frame.header_calls))
                    .collect();
                line(out, indent, &text, step.state, &keys);
                self.insets(out, &frame.header_calls, indent + 4);
                if let Some(body) = body {
                    self.sequence(out, &body.steps, indent + 4);
                }
            }
            Shape::Exit(exit) => self.exit(out, step, exit, indent),
            Shape::Combined(combined) => self.combined(out, step, combined, indent),
            Shape::Proposed(proposed) => {
                line(
                    out,
                    indent,
                    &format!("+ PROPOSED {}", proposed.title),
                    step.state,
                    &[],
                );
            }
        }
    }

    /// A box's callbacks, with their insets, and its own inset.
    fn below(&self, out: &mut String, call: &CallBox, indent: usize) {
        for callback in &call.callbacks {
            let code = match &callback.signature {
                Some(signature) => format!("callback {signature}"),
                None => callback.label.clone(),
            };
            let text = format!("↳ {}", named(&[callback.name.as_deref(), Some(&code)]));
            line(out, indent, &text, RouteState::Reached, &[&callback.key]);
            if let Some(inset) = &callback.inset {
                Printer::new(inset, true).flow(out, indent + 4);
            }
        }
        if let Some(inset) = &call.inset {
            Printer::new(inset, true).flow(out, indent);
        }
    }

    fn insets(&self, out: &mut String, calls: &[CallBox], indent: usize) {
        for call in calls {
            self.below(out, call, indent);
        }
    }

    /// A lane's line: its words; its label, which is the names on the line and then their code;
    /// its boxes; then its steps.
    fn lane(
        &self,
        out: &mut String,
        lane: &Lane,
        words: &str,
        label: &[Option<&str>],
        keys: &[&Key],
        indent: usize,
    ) {
        let label = named(label);
        let calls = lane.test.as_ref().map_or(&[][..], |test| test.calls.as_slice());
        let mut text = words.to_owned();
        if !label.is_empty() {
            text.push(' ');
            text.push_str(&label);
        }
        text.push_str(&chips(calls));
        if lane.falls_through {
            text.push_str(" → falls into the next case");
        }
        let keys: Vec<&Key> = keys.iter().copied().chain(chip_keys(calls)).collect();
        line(out, indent, &text, lane.state, &keys);
        self.insets(out, calls, indent + 4);
        self.sequence(out, &lane.steps, indent + 4);
    }

    fn decision(&self, out: &mut String, step: &Step, frame: &Frame, indent: usize) {
        match frame.form {
            Form::Switch | Form::Match => {
                let word = if frame.form == Form::Switch {
                    "SWITCH"
                } else {
                    "MATCH"
                };
                let text = format!(
                    "{word} {}{}",
                    named(&[frame.name.as_deref(), frame.header.as_deref()]),
                    chips(&frame.header_calls)
                );
                let keys: Vec<&Key> = std::iter::once(&frame.key)
                    .chain(chip_keys(&frame.header_calls))
                    .collect();
                line(out, indent, &text, step.state, &keys);
                self.insets(out, &frame.header_calls, indent + 4);
                for lane in &frame.lanes {
                    let words = if lane.form == Form::Default {
                        "DEFAULT"
                    } else {
                        "CASE"
                    };
                    let label = [lane.name.as_deref(), test_text(lane)];
                    self.lane(out, lane, words, &label, &[&lane.key], indent);
                }
            }
            form => {
                let lanes_indent = if form == Form::Ternary {
                    let mut text = named(&[Some("TERNARY"), frame.name.as_deref()]);
                    if let Some(binds) = &frame.binds {
                        let _ = write!(text, " → {binds}");
                    }
                    line(out, indent, &text, step.state, &[&frame.key]);
                    indent + 4
                } else {
                    indent
                };
                for (index, lane) in frame.lanes.iter().enumerate() {
                    let words = match (lane.form, self.flow.header.language) {
                        (Form::Then, _) => "IF",
                        (Form::Elif, Language::Python) => "ELIF",
                        (Form::Elif, Language::TypeScript) => "ELSE IF",
                        _ => "ELSE",
                    };
                    // The first lane's line is the frame's too, so it carries the frame's name.
                    let (keys, frame_name) = if index == 0 && form != Form::Ternary {
                        (vec![&frame.key, &lane.key], frame.name.as_deref())
                    } else {
                        (vec![&lane.key], None)
                    };
                    let label = [frame_name, lane.name.as_deref(), test_text(lane)];
                    self.lane(out, lane, words, &label, &keys, lanes_indent);
                }
            }
        }
    }

    fn looping(&self, out: &mut String, step: &Step, frame: &Frame, indent: usize) {
        if frame.form == Form::Do {
            self.do_loop(out, step, frame, indent);
            return;
        }
        let word = if frame.form == Form::Comprehension {
            "COMPREHENSION"
        } else {
            "LOOP"
        };
        let body = frame.lanes.iter().find(|lane| lane.form == Form::Body);
        // The body's name goes on its `WHEN` line when it has a test, or else on this one.
        let body_name = body
            .filter(|lane| lane.test.is_none())
            .and_then(|lane| lane.name.as_deref());
        let label = named(&[frame.name.as_deref(), body_name, frame.header.as_deref()]);
        let mut text = format!("{word} {label}");
        if let Some(binds) = &frame.binds {
            let _ = write!(text, " → {binds}");
        }
        text.push_str(&chips(&frame.header_calls));
        let keys: Vec<&Key> = std::iter::once(&frame.key)
            .chain(body.map(|lane| &lane.key))
            .chain(chip_keys(&frame.header_calls))
            .collect();
        line(out, indent, &text, step.state, &keys);
        self.insets(out, &frame.header_calls, indent + 4);
        if let Some(body) = body {
            if let Some(test) = &body.test {
                let text = format!(
                    "WHEN {}{}",
                    named(&[body.name.as_deref(), test.text.as_deref()]),
                    chips(&test.calls)
                );
                let keys: Vec<&Key> = chip_keys(&test.calls).collect();
                line(out, indent + 4, &text, body.state, &keys);
                self.insets(out, &test.calls, indent + 8);
            }
            self.sequence(out, &body.steps, indent + 4);
        }
        if let Some(otherwise) = frame.lanes.iter().find(|lane| lane.form == Form::Else) {
            let label = [otherwise.name.as_deref()];
            self.lane(out, otherwise, "LOOP ELSE", &label, &[&otherwise.key], indent);
        }
    }

    /// A `do` loop: `DO`, its body, then its test as written, which runs after the body.
    fn do_loop(&self, out: &mut String, step: &Step, frame: &Frame, indent: usize) {
        let body = frame.lanes.iter().find(|lane| lane.form == Form::Body);
        let mut text = named(&[
            Some("DO"),
            frame.name.as_deref(),
            body.and_then(|lane| lane.name.as_deref()),
        ]);
        let keys: Vec<&Key> = std::iter::once(&frame.key)
            .chain(body.map(|lane| &lane.key))
            .collect();
        line(out, indent, &text, step.state, &keys);
        if let Some(body) = body {
            self.sequence(out, &body.steps, indent + 4);
        }
        text = "LOOP".to_owned();
        if let Some(header) = &frame.header {
            let _ = write!(text, " {header}");
        }
        text.push_str(&chips(&frame.header_calls));
        let keys: Vec<&Key> = chip_keys(&frame.header_calls).collect();
        line(out, indent, &text, step.state, &keys);
        self.insets(out, &frame.header_calls, indent + 4);
    }

    fn trying(&self, out: &mut String, step: &Step, frame: &Frame, indent: usize) {
        for lane in &frame.lanes {
            match lane.form {
                Form::Body => {
                    let text = named(&[Some("TRY"), frame.name.as_deref(), lane.name.as_deref()]);
                    line(out, indent, &text, step.state, &[&frame.key, &lane.key]);
                    self.sequence(out, &lane.steps, indent + 4);
                }
                Form::Except | Form::Catch => {
                    let label = [lane.name.as_deref(), test_text(lane)];
                    self.lane(out, lane, "ON ERROR →", &label, &[&lane.key], indent);
                }
                Form::Else => {
                    let label = [lane.name.as_deref()];
                    self.lane(out, lane, "ELSE", &label, &[&lane.key], indent);
                }
                _ => {
                    let label = [lane.name.as_deref()];
                    self.lane(out, lane, "FINALLY", &label, &[&lane.key], indent);
                }
            }
        }
    }

    /// Where an exit's (or a combined step's) routes go, in words.
    fn places(&self, id: u32) -> Vec<(RouteKind, String)> {
        let mut places: Vec<(RouteKind, String)> = Vec::new();
        for route in self.routes(id) {
            let place = match (route.kind, route.to) {
                (RouteKind::Return, _) if self.inset => "returns",
                (RouteKind::Return, _) => "End",
                (RouteKind::Raise, RouteEnd::Step(_)) => "the handlers",
                (RouteKind::Raise, _) => "the caller",
                (RouteKind::Break, _) if route.leaves == Some(Form::Switch) => "after the switch",
                (RouteKind::Break, _) => "after the loop",
                (RouteKind::Continue, _) => "next",
                (RouteKind::MayContinue, _) => "may continue",
                _ => continue,
            };
            let place = format!("{}{place}", "finally → ".repeat(route.through.len()));
            if !places.iter().any(|(_, known)| *known == place) {
                places.push((route.kind, place));
            }
        }
        places
    }

    fn exit(&self, out: &mut String, step: &Step, exit: &ExitStep, indent: usize) {
        let word = exit.form.as_str().to_uppercase();
        let value = exit.value.as_ref().map(|value| match value {
            ExitValue::Call(call) if call.to.outcome == Outcome::Project => box_title(call),
            ExitValue::Call(call) => format!(
                "{}{}",
                named(&[call.name.as_deref(), Some(&call.callee)]),
                question(call.to.outcome)
            ),
            ExitValue::Text { text } => text.clone(),
        });
        let mut text = word;
        let label = named(&[exit.name.as_deref(), value.as_deref()]);
        if !label.is_empty() {
            text.push(' ');
            text.push_str(&label);
        }
        let places: Vec<String> = self.places(step.id).into_iter().map(|(_, place)| place).collect();
        if !places.is_empty() {
            let _ = write!(text, " → {}", places.join(", "));
        }
        line(out, indent, &text, step.state, &[&exit.key]);
        if let Some(ExitValue::Call(call)) = &exit.value {
            self.below(out, call, indent + 4);
        }
    }

    fn combined(&self, out: &mut String, step: &Step, combined: &CombinedStep, indent: usize) {
        let mut parts = Vec::new();
        if combined.conditions > 0 {
            parts.push(plural(combined.conditions, "condition"));
        }
        if combined.loops > 0 {
            parts.push(plural(combined.loops, "loop"));
        }
        parts.extend(
            combined
                .calls
                .iter()
                .map(|call| call.name.clone().unwrap_or_else(|| call.callee.clone())),
        );
        if combined.library > 0 {
            parts.push(format!("{} library", combined.library));
        }
        if combined.unknown > 0 {
            parts.push(format!("{} unknown ?", combined.unknown));
        }
        for (kind, place) in self.places(step.id) {
            let verb = match kind {
                RouteKind::Return => "returns → ",
                RouteKind::Raise => "raises → ",
                RouteKind::Break => "breaks → ",
                RouteKind::Continue => "continues → ",
                _ => "",
            };
            parts.push(format!("{verb}{place}"));
        }
        let mark = if combined.collapsed { "⊞" } else { "⊟" };
        let mut text = format!("{mark} {}", combined.title);
        if !parts.is_empty() {
            let _ = write!(text, "   {}", parts.join(" · "));
        }
        let keys: Vec<&Key> = combined.members.iter().collect();
        line(out, indent, &text, step.state, &keys);
        if let Some(steps) = &combined.steps {
            self.sequence(out, steps, indent + 4);
        }
    }
}

/// The compiled tree of a definition, for `read_code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeText {
    /// The text: the definition's key, its kind, lines, and doc, then one line per block and
    /// per line of code that isn't a block.
    pub text: String,
    /// Each block's short label (relative to the definition, as agents write it) and full key.
    pub labels: Vec<CodeLabel>,
}

/// A block's short label and full key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeLabel {
    /// The label: the key's steps after the definition's, such as `try[0]/except[0]`.
    pub label: String,
    /// The block's key.
    pub key: Key,
}

/// `key`'s steps after `definition`'s ([`Key::steps_after`]): the short label agents read and
/// write.
fn short_label(definition: &Key, key: &Key) -> String {
    key.steps_after(definition).unwrap_or_else(|| key.to_string())
}

/// One line of the compiled tree.
struct Row {
    line: u32,
    label: String,
    description: String,
    key: Option<Key>,
}

/// The compiled tree of definition `definition` in `tree`, with source verified under `root`:
/// each block it owns (nested definitions as one line each), with what it calls, binds, or
/// tests, and the lines of code that aren't blocks (`⋯`).
pub fn code_text(tree: &FileTree, root: &Path, definition: u32) -> CodeText {
    let text = verify_file(
        root,
        tree.path(),
        tree.file().hash,
        LineBreaks::of(tree.language()),
    )
    .into_source()
    .map(|source| FileText::new(source, tree.language()));
    let text = text.as_ref();
    let node = tree.node(definition);
    let key = tree.key(definition);
    let mut out = format!(
        "{key}\n  {} · lines {}–{}",
        node.form, node.lines.start, node.lines.end
    );
    if let Some(doc) = text
        .zip(node.doc)
        .and_then(|(text, doc)| doc_text(text.slice(doc)?))
    {
        let _ = write!(out, " · doc \"{}\"", doc.lines().next().unwrap_or_default());
    }
    out.push_str("\n\n");

    let mut rows: Vec<Row> = tree
        .owned(definition)
        .map(|id| Row {
            line: tree.node(id).lines.start,
            label: short_label(key, tree.key(id)),
            description: describe(tree, text, key, id),
            key: Some(tree.key(id).clone()),
        })
        .collect();
    if let Some(text) = text {
        let mut covered = FxHashSet::default();
        for id in tree.owned(definition) {
            let block = tree.node(id);
            let mut cover = |range| {
                let span = text.lines(range);
                covered.extend(span.start..=span.end);
            };
            match block.kind() {
                NodeKind::Call | NodeKind::Exit => cover(block.stmt.unwrap_or(block.range)),
                kind if kind.is_definition() => cover(block.range),
                _ => {
                    let start = block.range.start;
                    cover(ByteRange { start, end: start });
                    if let Some(head) = block.head {
                        cover(head);
                    }
                }
            }
        }
        for line in candidates(tree, text, definition, definition) {
            if !covered.contains(&line) {
                rows.push(Row {
                    line,
                    label: format!("⋯ {}", text.line(line)),
                    description: "code".to_owned(),
                    key: None,
                });
            }
        }
    }
    rows.sort_by_key(|row| row.line);

    let line_width = rows
        .iter()
        .map(|row| row.line.to_string().len())
        .max()
        .unwrap_or(1);
    let label_width = rows
        .iter()
        .filter(|row| row.key.is_some())
        .map(|row| row.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut labels = Vec::new();
    for row in rows {
        let padding = label_width.saturating_sub(row.label.chars().count()) + 2;
        let _ = write!(
            out,
            "L{:<line_width$}  {}{}{}",
            row.line,
            row.label,
            " ".repeat(padding),
            row.description
        );
        if let Some(key) = row.key {
            let _ = write!(out, "   key={key}");
            labels.push(CodeLabel {
                label: row.label,
                key,
            });
        }
        out.push('\n');
    }
    CodeText { text: out, labels }
}

/// How the compiled tree describes block `id`: what it calls, binds, or tests.
fn describe(tree: &FileTree, text: Option<&FileText>, definition: &Key, id: u32) -> String {
    let node = tree.node(id);
    let written = |range| text.zip(range).and_then(|(text, range)| text.one_line(range));
    let mut parts: Vec<String> = Vec::new();
    match node.kind() {
        NodeKind::Call => {
            for link in tree.links(id) {
                let to = target_text(&link.to);
                match link.kind {
                    LinkKind::Calls => parts.push(to),
                    kind => parts.push(format!("{kind} {to}")),
                }
            }
            if node.form == Form::New {
                parts.push("new".to_owned());
            }
            if node.flags.contains(Flag::Awaited) {
                parts.push("await".to_owned());
            }
            if node.flags.contains(Flag::Conditional) {
                parts.push("sometimes".to_owned());
            }
            if let Some(binds) = &node.binds {
                parts.push(format!("binds {binds}"));
            }
            if node.flags.contains(Flag::ExitValue) {
                parts.push("exit value".to_owned());
            }
            match (node.role, node.r#for) {
                (Some(role), Some(owner)) => parts.push(format!("{role} for {}", tree.label(owner))),
                (Some(role), None) => parts.push(role.to_string()),
                (None, _) => {}
            }
        }
        NodeKind::Part => {
            let handler = matches!(node.form, Form::Except | Form::Catch);
            let word = if handler { "clause" } else { "test" };
            match (node.head, written(node.head)) {
                (Some(_), Some(head)) => parts.push(format!("{word} {head}")),
                (Some(_), None) => parts.push(word.to_owned()),
                (None, _) => parts.push(node.form.to_string()),
            }
            for link in tree.links(id) {
                parts.push(format!("{} {}", link.kind, target_text(&link.to)));
            }
            if node.flags.contains(Flag::FallsThrough) {
                parts.push("falls through".to_owned());
            }
            if node.flags.contains(Flag::Group) {
                parts.push("group".to_owned());
            }
        }
        NodeKind::Exit => {
            parts.push(node.form.to_string());
            if let Some(value) = written(node.value) {
                parts.push(format!("value {value}"));
            }
            if let Some(target) = node.target {
                parts.push(format!("leaves {}", short_label(definition, tree.key(target))));
            }
        }
        kind if kind.is_definition() => {
            parts.push(node.form.to_string());
            parts.push(format!("lines {}–{}", node.lines.start, node.lines.end));
        }
        _ => {
            parts.push(node.form.to_string());
            if let Some(header) = written(node.head) {
                parts.push(header);
            }
            if let Some(binds) = &node.binds {
                parts.push(format!("binds {binds}"));
            }
            if node.flags.contains(Flag::Async) {
                parts.push("async".to_owned());
            }
        }
    }
    if node.flags.contains(Flag::Truncated) {
        parts.push("truncated".to_owned());
    }
    parts.join(" · ")
}

/// Where a link goes, as the compiled tree writes it: `▸ key`, `library fastapi`, `builtin`,
/// `? unknown_receiver`.
fn target_text(target: &LinkTarget) -> String {
    match target {
        LinkTarget::Project(key) => format!("▸ {key}"),
        LinkTarget::Library(module) => format!("library {}", module_text(module)),
        LinkTarget::Builtin(_) => "builtin".to_owned(),
        LinkTarget::Unknown(reason) => format!("? {reason}"),
    }
}
