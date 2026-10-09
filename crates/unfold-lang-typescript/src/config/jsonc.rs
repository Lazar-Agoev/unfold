//! JSON with comments, as `tsconfig.json` is written: `//` and `/* */` comments and trailing
//! commas are allowed, and a leading byte-order mark is ignored. Everything else must be JSON.
//! Duplicate keys keep the last value, as TypeScript does.

use serde_json::Value;

/// Parses JSONC text into a JSON value.
pub(crate) fn parse(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let plain = strip(text)?;
    serde_json::from_str(&plain).map_err(|error| error.to_string())
}

/// The text with comments and trailing commas replaced by spaces, so error positions still
/// point at the written text.
fn strip(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    // Where the last comma written outside a string is, while only whitespace and comments
    // have followed it.
    let mut pending_comma: Option<usize> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' => {
                pending_comma = None;
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                let end = (index + 1).min(bytes.len());
                out.extend_from_slice(&bytes[start..end]);
                index = end;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    out.push(b' ');
                    index += 1;
                }
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let Some(close) = text[index + 2..].find("*/") else {
                    return Err("unterminated comment".to_owned());
                };
                let end = index + 2 + close + 2;
                out.extend(
                    bytes[index..end]
                        .iter()
                        .map(|&b| if b == b'\n' { b'\n' } else { b' ' }),
                );
                index = end;
                continue;
            }
            b',' => pending_comma = Some(out.len()),
            b'}' | b']' => {
                if let Some(comma) = pending_comma.take() {
                    out[comma] = b' ';
                }
            }
            b' ' | b'\t' | b'\n' | b'\r' => {}
            _ => pending_comma = None,
        }
        out.push(byte);
        index += 1;
    }
    // Only ASCII bytes were replaced (with ASCII), so the bytes are still UTF-8.
    String::from_utf8(out).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_comments_trailing_commas_and_a_byte_order_mark() {
        let text = "\u{feff}{\n  // line\n  \"a\": [1, 2,], /* block, with a comma */\n  \"b\": \"x // not a comment\",\n  \"c\": {\"d\": \"\\\"\",},\n}";
        let value = parse(text).expect("parses");
        assert_eq!(value["a"], serde_json::json!([1, 2]));
        assert_eq!(value["b"], "x // not a comment");
        assert_eq!(value["c"]["d"], "\"");
        assert_eq!(parse("{\"a\": 1, \"a\": 2}").expect("parses")["a"], 2);
    }

    #[test]
    fn rejects_what_is_not_jsonc() {
        assert!(parse("{\"a\": }").is_err());
        assert!(parse("{ /* open").is_err());
        assert!(parse("{\"a\": 'x'}").is_err());
        assert!(parse("").is_err());
    }
}
