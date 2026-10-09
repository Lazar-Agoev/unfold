//! Deterministic board serialization: object keys sorted by UTF-16 code units, two-space
//! indentation, numbers and string escapes exactly as JavaScript's `JSON.stringify(value, null,
//! 2)` writes them, and a final newline. Pages keep their order; cards, groups, and links are
//! sorted by id; labels and guards by (`project` or `""`, `key`); `projects` is the sorted set
//! the cards name. One board has exactly one serialization, so a file written by Unfold, or by
//! an agent using `JSON.stringify` with sorted keys and arrays, reads back and writes out byte
//! for byte.

use std::cmp::Ordering;
use std::fmt::Write as _;

use super::files::Revision;
use super::model::{
    Board, Card, CardRef, Description, FORMAT, Group, GuardEntry, LabelEntry, Link, Page, ProjectName,
    VERSION,
};
use super::parse::publishes;

/// A JSON value to write.
enum Out {
    Text(String),
    Number(f64),
    Bool(bool),
    Array(Vec<Out>),
    Object(Vec<(&'static str, Out)>),
}

/// Writes `board` as its canonical file text (a draft or a published board).
pub fn serialize(board: &Board) -> String {
    finish(board_fields(board))
}

/// Writes `board` as the canonical text of the changes copy of published board `name` (its
/// file stem), started from published revision `base`.
pub fn serialize_changes(board: &Board, name: &str, base: Revision) -> String {
    let mut fields = board_fields(board);
    fields.push(("baseRevision", Out::Text(base.0.to_string())));
    fields.push(("publishes", Out::Text(publishes(name))));
    finish(fields)
}

fn finish(fields: Vec<(&'static str, Out)>) -> String {
    let mut out = String::new();
    write_value(&mut out, &Out::Object(fields), 0);
    out.push('\n');
    out
}

fn by_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// The order of labels and guards: by project (the board's own first), then key.
fn by_project_key(a: (Option<&ProjectName>, &str), b: (Option<&ProjectName>, &str)) -> Ordering {
    by_utf16(project_text(a.0), project_text(b.0)).then_with(|| by_utf16(a.1, b.1))
}

fn project_text(name: Option<&ProjectName>) -> &str {
    name.map_or("", ProjectName::as_str)
}

fn text(value: &str) -> Out {
    Out::Text(value.to_owned())
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

fn project(fields: &mut Vec<(&'static str, Out)>, project: Option<&ProjectName>) {
    if let Some(name) = project {
        fields.push(("project", text(name.as_str())));
    }
}

fn card(value: &Card) -> Out {
    let mut reference = vec![match &value.reference {
        CardRef::Block { block, .. } => ("block", text(block.as_str())),
        CardRef::Entrance { entrance, .. } => ("entrance", text(entrance.as_str())),
    }];
    project(&mut reference, value.reference.project());
    let mut fields = vec![(
        "at",
        Out::Array(value.at.iter().map(|&position| Out::Number(position)).collect()),
    )];
    if let Some(item) = &value.description {
        fields.push(("description", description(item)));
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
    project(&mut fields, value.project.as_ref());
    Out::Object(fields)
}

fn guard(value: &GuardEntry) -> Out {
    let mut fields = vec![
        ("key", text(value.key.as_str())),
        ("kind", text(&value.kind)),
        ("owner", text(&value.owner)),
        ("signature", Out::Text(value.signature.to_string())),
    ];
    project(&mut fields, value.project.as_ref());
    Out::Object(fields)
}

fn link(value: &Link) -> Out {
    let mut fields = vec![("from", text(&value.from)), ("id", text(&value.id))];
    if let Some(item) = &value.label {
        fields.push(("label", description(item)));
    }
    fields.push(("to", text(&value.to)));
    Out::Object(fields)
}

fn page(value: &Page) -> Out {
    let mut cards: Vec<&Card> = value.cards.iter().collect();
    cards.sort_by(|a, b| by_utf16(&a.id, &b.id));
    let mut groups: Vec<&Group> = value.groups.iter().collect();
    groups.sort_by(|a, b| by_utf16(&a.id, &b.id));
    let mut links: Vec<&Link> = value.links.iter().collect();
    links.sort_by(|a, b| by_utf16(&a.id, &b.id));
    let mut labels: Vec<&LabelEntry> = value.labels.iter().collect();
    labels.sort_by(|a, b| {
        by_project_key(
            (a.project.as_ref(), a.key.as_str()),
            (b.project.as_ref(), b.key.as_str()),
        )
    });
    let mut guards: Vec<&GuardEntry> = value.guards.iter().collect();
    guards.sort_by(|a, b| {
        by_project_key(
            (a.project.as_ref(), a.key.as_str()),
            (b.project.as_ref(), b.key.as_str()),
        )
    });
    Out::Object(vec![
        ("cards", Out::Array(cards.into_iter().map(card).collect())),
        ("defaultView", text(value.default_view.as_str())),
        ("groups", Out::Array(groups.into_iter().map(group).collect())),
        ("guards", Out::Array(guards.into_iter().map(guard).collect())),
        ("id", text(&value.id)),
        ("labels", Out::Array(labels.into_iter().map(label).collect())),
        ("links", Out::Array(links.into_iter().map(link).collect())),
        ("title", text(&value.title)),
    ])
}

fn board_fields(board: &Board) -> Vec<(&'static str, Out)> {
    let mut fields = Vec::new();
    if let Some(folder) = &board.folder {
        fields.push(("folder", text(folder)));
    }
    fields.extend([
        ("format", text(FORMAT)),
        ("pages", Out::Array(board.pages.iter().map(page).collect())),
        (
            "projects",
            Out::Array(
                board
                    .used_projects()
                    .iter()
                    .map(|name| text(name.as_str()))
                    .collect(),
            ),
        ),
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
