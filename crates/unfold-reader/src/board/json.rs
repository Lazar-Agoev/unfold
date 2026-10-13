//! Deterministic board serialization: object keys sorted by UTF-16 code units, two-space
//! indentation, numbers and string escapes exactly as JavaScript's `JSON.stringify(value, null,
//! 2)` writes them, and a final newline. Pages keep their order; cards, groups, links, frames,
//! resources, notes, and proposals are sorted by id; labels and guards by key. Optional fields
//! are omitted when absent,
//! and a card's `expanded` and the page arrays `frames`, `resources`, `notes`, and `proposals`
//! when empty, so boards without them read as they always did. One board has exactly one serialization, so a file written by Unfold, or by
//! an agent using `JSON.stringify` with sorted keys and arrays, reads back and writes out byte
//! for byte.

use std::cmp::Ordering;
use std::fmt::Write as _;

use super::model::{
    Anchor, Board, Card, CardRef, Column, Description, FORMAT, Frame, Group, GuardEntry, LabelEntry, Link,
    Note, Page, Proposal, Resource, VERSION,
};

/// A JSON value to write.
enum Out {
    Text(String),
    Number(f64),
    Bool(bool),
    Array(Vec<Out>),
    Object(Vec<(&'static str, Out)>),
}

/// Writes `board` as its canonical file text.
pub fn serialize(board: &Board) -> String {
    let mut out = String::new();
    write_value(&mut out, &Out::Object(board_fields(board)), 0);
    out.push('\n');
    out
}

fn by_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn text(value: &str) -> Out {
    Out::Text(value.to_owned())
}

fn position(at: [f64; 2]) -> Out {
    Out::Array(at.iter().map(|&number| Out::Number(number)).collect())
}

/// Adds `name` when `value` is present, written by `write`.
fn optional<T>(
    fields: &mut Vec<(&'static str, Out)>,
    name: &'static str,
    value: Option<T>,
    write: impl FnOnce(T) -> Out,
) {
    if let Some(value) = value {
        fields.push((name, write(value)));
    }
}

/// The fields of a description, label, or link label.
fn authored(value: &Description) -> Vec<(&'static str, Out)> {
    vec![
        ("author", Out::Text(value.author.to_string())),
        (
            "signature",
            Out::Text(
                value
                    .signature
                    .map(|signature| signature.to_string())
                    .unwrap_or_default(),
            ),
        ),
        ("text", text(&value.text)),
    ]
}

fn description(value: &Description) -> Out {
    Out::Object(authored(value))
}

fn card(value: &Card) -> Out {
    let reference = vec![match &value.reference {
        CardRef::Block { block } => ("block", text(block.as_str())),
        CardRef::Entrance { entrance } => ("entrance", text(entrance.as_str())),
    }];
    let mut fields = vec![("at", position(value.at))];
    if let Some(item) = &value.description {
        fields.push(("description", description(item)));
    }
    if !value.expanded.is_empty() {
        let paths = value
            .expanded
            .iter()
            .map(|path| Out::Array(path.iter().map(|key| text(key.as_str())).collect()));
        fields.push(("expanded", Out::Array(paths.collect())));
    }
    if value.folded {
        fields.push(("folded", Out::Bool(true)));
    }
    fields.push(("id", text(&value.id)));
    fields.push(("ref", Out::Object(reference)));
    if let Some(view) = value.view {
        fields.push(("view", text(view.as_str())));
    }
    Out::Object(fields)
}

fn group(value: &Group) -> Out {
    let mut fields = vec![
        ("card", text(&value.card)),
        ("collapsed", Out::Bool(value.collapsed)),
    ];
    if let Some(item) = &value.description {
        fields.push(("description", description(item)));
    }
    optional(&mut fields, "author", value.author.as_ref(), |author| {
        Out::Text(author.to_string())
    });
    fields.push(("id", text(&value.id)));
    fields.push((
        "members",
        Out::Array(value.members.iter().map(|key| text(key.as_str())).collect()),
    ));
    fields.push(("title", text(&value.title)));
    Out::Object(fields)
}

fn label(value: &LabelEntry) -> Out {
    let mut fields = authored(&value.label);
    fields.push(("key", text(value.key.as_str())));
    Out::Object(fields)
}

fn guard(value: &GuardEntry) -> Out {
    Out::Object(vec![
        ("key", text(value.key.as_str())),
        ("kind", text(&value.kind)),
        ("owner", text(&value.owner)),
        ("signature", Out::Text(value.signature.to_string())),
    ])
}

fn anchor(value: &Anchor) -> Out {
    Out::Object(match value {
        Anchor::Key { key } => vec![("key", text(key.as_str()))],
        Anchor::File { file, lines } => vec![
            ("file", text(file.as_str())),
            (
                "lines",
                Out::Array(lines.iter().map(|&line| Out::Number(f64::from(line))).collect()),
            ),
        ],
    })
}

fn anchors(values: &[Anchor]) -> Out {
    Out::Array(values.iter().map(anchor).collect())
}

fn link(value: &Link) -> Out {
    let mut fields = vec![("from", text(&value.from)), ("id", text(&value.id))];
    if !value.anchors.is_empty() {
        fields.push(("anchors", anchors(&value.anchors)));
    }
    optional(&mut fields, "author", value.author.as_ref(), |author| {
        Out::Text(author.to_string())
    });
    optional(&mut fields, "cardinality", value.cardinality, |cardinality| {
        text(cardinality.as_str())
    });
    optional(&mut fields, "carries", value.carries.as_ref(), |key| {
        text(key.as_str())
    });
    optional(&mut fields, "channel", value.channel.as_deref(), text);
    optional(
        &mut fields,
        "description",
        value.description.as_ref(),
        description,
    );
    optional(&mut fields, "fromColumn", value.from_column.as_deref(), text);
    optional(&mut fields, "kind", value.kind, |kind| text(kind.as_str()));
    optional(&mut fields, "label", value.label.as_ref(), description);
    optional(&mut fields, "signature", value.signature, |signature| {
        Out::Text(signature.to_string())
    });
    optional(&mut fields, "step", value.step.as_ref(), |key| text(key.as_str()));
    fields.push(("to", text(&value.to)));
    optional(&mut fields, "toColumn", value.to_column.as_deref(), text);
    Out::Object(fields)
}

fn frame(value: &Frame) -> Out {
    let mut fields = vec![("author", Out::Text(value.author.to_string()))];
    if value.collapsed {
        fields.push(("collapsed", Out::Bool(true)));
    }
    optional(
        &mut fields,
        "description",
        value.description.as_ref(),
        description,
    );
    optional(&mut fields, "direction", value.direction, |direction| {
        text(direction.as_str())
    });
    optional(&mut fields, "icon", value.icon, |icon| text(icon.as_str()));
    fields.push(("id", text(&value.id)));
    fields.push(("look", text(value.look.as_str())));
    fields.push((
        "members",
        Out::Array(value.members.iter().map(|member| text(member)).collect()),
    ));
    fields.push(("title", text(&value.title)));
    Out::Object(fields)
}

fn column(value: &Column) -> Out {
    let mut fields = Vec::new();
    optional(&mut fields, "key", value.key, |key| text(key.as_str()));
    fields.push(("name", text(&value.name)));
    if value.optional {
        fields.push(("optional", Out::Bool(true)));
    }
    optional(&mut fields, "references", value.references.as_deref(), text);
    optional(&mut fields, "type", value.written_type.as_deref(), text);
    Out::Object(fields)
}

fn resource(value: &Resource) -> Out {
    let mut fields = vec![
        ("anchors", anchors(&value.anchors)),
        ("at", position(value.at)),
        ("author", Out::Text(value.author.to_string())),
    ];
    if !value.columns.is_empty() {
        fields.push(("columns", Out::Array(value.columns.iter().map(column).collect())));
    }
    optional(
        &mut fields,
        "description",
        value.description.as_ref(),
        description,
    );
    fields.push(("id", text(&value.id)));
    fields.push(("kind", text(value.kind.as_str())));
    fields.push(("name", text(&value.name)));
    optional(&mut fields, "signature", value.signature, |signature| {
        Out::Text(signature.to_string())
    });
    optional(&mut fields, "type", value.type_key.as_ref(), |key| {
        text(key.as_str())
    });
    Out::Object(fields)
}

fn note(value: &Note) -> Out {
    let mut fields = vec![("at", position(value.at))];
    optional(&mut fields, "attach", value.attach.as_deref(), text);
    fields.push(("author", Out::Text(value.author.to_string())));
    fields.push(("id", text(&value.id)));
    fields.push(("text", text(&value.text)));
    Out::Object(fields)
}

fn proposal(value: &Proposal) -> Out {
    let mut fields = vec![
        ("after", text(value.after.as_str())),
        ("author", Out::Text(value.author.to_string())),
        ("card", text(&value.card)),
        ("id", text(&value.id)),
    ];
    optional(&mut fields, "path", value.path, |path| text(path.as_str()));
    optional(&mut fields, "reason", value.reason.as_deref(), text);
    fields.push(("title", text(&value.title)));
    Out::Object(fields)
}

/// `items` sorted by id, written by `write`.
fn by_id<T>(items: &[T], id: impl Fn(&T) -> &str, write: impl Fn(&T) -> Out) -> Out {
    let mut sorted: Vec<&T> = items.iter().collect();
    sorted.sort_by(|a, b| by_utf16(id(a), id(b)));
    Out::Array(sorted.into_iter().map(write).collect())
}

fn page(value: &Page) -> Out {
    let mut labels: Vec<&LabelEntry> = value.labels.iter().collect();
    labels.sort_by(|a, b| by_utf16(a.key.as_str(), b.key.as_str()));
    let mut guards: Vec<&GuardEntry> = value.guards.iter().collect();
    guards.sort_by(|a, b| by_utf16(a.key.as_str(), b.key.as_str()));
    let mut fields = vec![
        ("cards", by_id(&value.cards, |item| &item.id, card)),
        ("defaultView", text(value.default_view.as_str())),
        ("groups", by_id(&value.groups, |item| &item.id, group)),
        ("guards", Out::Array(guards.into_iter().map(guard).collect())),
        ("id", text(&value.id)),
        ("labels", Out::Array(labels.into_iter().map(label).collect())),
        ("links", by_id(&value.links, |item| &item.id, link)),
        ("title", text(&value.title)),
    ];
    if !value.frames.is_empty() {
        fields.push(("frames", by_id(&value.frames, |item| &item.id, frame)));
    }
    if !value.resources.is_empty() {
        fields.push(("resources", by_id(&value.resources, |item| &item.id, resource)));
    }
    if !value.notes.is_empty() {
        fields.push(("notes", by_id(&value.notes, |item| &item.id, note)));
    }
    if !value.proposals.is_empty() {
        fields.push(("proposals", by_id(&value.proposals, |item| &item.id, proposal)));
    }
    Out::Object(fields)
}

fn board_fields(board: &Board) -> Vec<(&'static str, Out)> {
    let mut fields = Vec::new();
    if let Some(folder) = &board.folder {
        fields.push(("folder", text(folder)));
    }
    fields.extend([
        ("format", text(FORMAT)),
        ("pages", Out::Array(board.pages.iter().map(page).collect())),
        ("title", text(&board.title)),
        ("version", Out::Number(f64::from(VERSION))),
    ]);
    fields
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, value: &str) {
    // serde_json escapes exactly what JSON.stringify escapes for valid Unicode text: `"`, `\`,
    // and control characters (`\b`, `\f`, `\n`, `\r`, `\t`, others as lowercase `\u00xx`).
    match serde_json::to_string(value) {
        Ok(escaped) => out.push_str(&escaped),
        Err(_) => out.push_str("\"\""),
    }
}

fn write_value(out: &mut String, value: &Out, depth: usize) {
    match value {
        Out::Text(value) => write_string(out, value),
        Out::Number(value) => out.push_str(&js_number(*value)),
        Out::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Out::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                out.push_str(if index == 0 { "\n" } else { ",\n" });
                indent(out, depth + 1);
                write_value(out, item, depth + 1);
            }
            out.push('\n');
            indent(out, depth);
            out.push(']');
        }
        Out::Object(fields) => {
            let mut fields: Vec<(&str, &Out)> = fields.iter().map(|(key, value)| (*key, value)).collect();
            fields.sort_by(|a, b| by_utf16(a.0, b.0));
            write_object(out, &fields, depth);
        }
    }
}

fn write_object(out: &mut String, fields: &[(&str, &Out)], depth: usize) {
    if fields.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    for (index, (key, value)) in fields.iter().enumerate() {
        out.push_str(if index == 0 { "\n" } else { ",\n" });
        indent(out, depth + 1);
        write_string(out, key);
        out.push_str(": ");
        write_value(out, value, depth + 1);
    }
    out.push('\n');
    indent(out, depth);
    out.push('}');
}

/// A finite number as JavaScript's `Number.prototype.toString` writes it: the shortest digits
/// that round-trip, in plain notation for exponents from -7 to 20 and `1.5e+21` style beyond.
fn js_number(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    // Rust's `{:e}` gives the shortest round-trip digits: `-1.2345e3`.
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let n = exponent + 1;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(&digits);
        for _ in 0..n - k {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        let split = usize::try_from(n).unwrap_or(0);
        out.push_str(&digits[..split]);
        out.push('.');
        out.push_str(&digits[split..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..-n {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(out, "e{}{}", if n > 0 { "+" } else { "-" }, (n - 1).abs());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_javascript() {
        let cases = [
            (0.0, "0"),
            (-0.0, "0"),
            (-104.0, "-104"),
            (12.5, "12.5"),
            (0.1, "0.1"),
            (1e21, "1e+21"),
            (123_456_789_012_345_680_000.0, "123456789012345680000"),
            (1e-7, "1e-7"),
            (0.000_001, "0.000001"),
            (1.5e-10, "1.5e-10"),
            (-2.25e30, "-2.25e+30"),
            (1_000_000.0, "1000000"),
        ];
        for (value, expected) in cases {
            assert_eq!(js_number(value), expected, "{value}");
        }
    }
}
