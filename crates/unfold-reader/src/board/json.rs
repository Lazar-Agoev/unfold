//! Deterministic board serialization: object keys sorted by UTF-16 code units, two-space
//! indentation, numbers and string escapes exactly as JavaScript's `JSON.stringify(value, null,
//! 2)` writes them, and a final newline. One board has exactly one serialization, so a file
//! written by Unfold, by an agent using `JSON.stringify` with sorted keys, or by the proof of
//! concept reads back and writes out byte for byte.

use std::fmt::Write as _;

use super::model::{Board, Card, CardRef, Description, Group, Guard};

/// A JSON value to write.
enum Out {
    Text(String),
    Number(f64),
    Bool(bool),
    Array(Vec<Out>),
    Object(Vec<(&'static str, Out)>),
    /// An object whose keys come from data (labels, guards).
    Map(Vec<(String, Out)>),
}

/// Writes `board` as its canonical file text.
pub fn serialize(board: &Board) -> String {
    let mut out = String::new();
    write_value(&mut out, &board_value(board), 0);
    out.push('\n');
    out
}

fn by_utf16(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn text(value: &str) -> Out {
    Out::Text(value.to_owned())
}

fn description(value: &Description) -> Out {
    Out::Object(vec![
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
    ])
}

fn card(value: &Card) -> Out {
    let reference = match &value.reference {
        CardRef::Block(key) => ("block", text(key.as_str())),
        CardRef::Entrance(key) => ("entrance", text(key.as_str())),
    };
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
    fields.push(("ref", Out::Object(vec![reference])));
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

fn guard(value: &Guard) -> Out {
    Out::Object(vec![
        ("kind", text(&value.kind)),
        ("owner", text(&value.owner)),
        ("signature", Out::Text(value.signature.to_string())),
    ])
}

fn board_value(board: &Board) -> Out {
    let mut cards: Vec<&Card> = board.cards.iter().collect();
    cards.sort_by(|a, b| by_utf16(&a.id, &b.id));
    let mut groups: Vec<&Group> = board.groups.iter().collect();
    groups.sort_by(|a, b| by_utf16(&a.id, &b.id));
    Out::Object(vec![
        ("cards", Out::Array(cards.into_iter().map(card).collect())),
        ("defaultView", text(board.default_view.as_str())),
        ("format", text(super::model::FORMAT)),
        ("groups", Out::Array(groups.into_iter().map(group).collect())),
        (
            "guards",
            Out::Map(
                board
                    .guards
                    .iter()
                    .map(|(key, value)| (key.clone(), guard(value)))
                    .collect(),
            ),
        ),
        (
            "labels",
            Out::Map(
                board
                    .labels
                    .iter()
                    .map(|(key, value)| (key.clone(), description(value)))
                    .collect(),
            ),
        ),
        ("title", text(&board.title)),
        ("version", Out::Number(f64::from(super::model::VERSION))),
    ])
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
        Out::Map(entries) => {
            let mut fields: Vec<(&str, &Out)> =
                entries.iter().map(|(key, value)| (key.as_str(), value)).collect();
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
pub(crate) fn js_number(value: f64) -> String {
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
