//! The `[[entrance]]` tables of a folder's `.unfold/config.toml`, with their byte ranges.
//!
//! PoC manual entrances were declared in `codegraph.json` (evidence: a JSON array element with
//! pointer `/entrypoints/N`). In Unfold they are `[[entrance]]` tables, so the importer
//! re-derives their evidence from the folder's settings file: the range runs from the table's
//! `[[` through the end of its last value (token-exact: no trailing comment or whitespace), and
//! the pointer is `/entrance/N`.

use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde::Deserialize;
use toml::Spanned;
use unfold_core::ByteRange;

/// One `[[entrance]]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManualTable {
    /// From the `[[` of its header through the end of its last value.
    pub range: ByteRange,
    /// `kind`.
    pub kind: String,
    /// `trigger`, if written.
    pub trigger: Option<String>,
    /// `method`, if written.
    pub method: Option<String>,
    /// `label`, if written.
    pub label: Option<String>,
    /// `handler` or `startup`, as one or several locators.
    pub targets: Vec<String>,
}

#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    entrance: Vec<BTreeMap<String, Spanned<toml::Value>>>,
}

/// The `[[entrance]]` tables of `text`, in order.
pub(crate) fn manual_tables(text: &str) -> anyhow::Result<Vec<ManualTable>> {
    let config: Config = toml::from_str(text).context("invalid settings file")?;
    let headers = entrance_headers(text);
    if headers.len() != config.entrance.len() {
        bail!(
            "found {} `[[entrance]]` headers but {} parsed entrance tables",
            headers.len(),
            config.entrance.len()
        );
    }
    let mut tables = Vec::with_capacity(headers.len());
    for (index, (header, table)) in headers.iter().zip(&config.entrance).enumerate() {
        let first = table.values().map(|value| value.span().start).min();
        let last = table.values().map(|value| value.span().end).max();
        let (Some(first), Some(last)) = (first, last) else {
            bail!("`[[entrance]]` table {index} is empty");
        };
        let next_header = headers.get(index + 1).copied().unwrap_or(usize::MAX);
        if first < *header || last > next_header {
            bail!("`[[entrance]]` table {index} does not follow its header");
        }
        let start = u32::try_from(*header).context("settings file too large")?;
        let end = u32::try_from(last).context("settings file too large")?;
        let string = |key: &str| -> anyhow::Result<Option<String>> {
            match table.get(key).map(|value| value.get_ref()) {
                None => Ok(None),
                Some(toml::Value::String(text)) => Ok(Some(text.clone())),
                Some(_) => bail!("`[[entrance]]` table {index}: `{key}` is not a string"),
            }
        };
        let targets = match table
            .get("handler")
            .or_else(|| table.get("startup"))
            .map(|v| v.get_ref())
        {
            Some(toml::Value::String(text)) => vec![text.clone()],
            Some(toml::Value::Array(items)) => items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .with_context(|| format!("`[[entrance]]` table {index}: a handler is not a string"))?,
            _ => bail!("`[[entrance]]` table {index} has no handler or startup"),
        };
        tables.push(ManualTable {
            range: ByteRange::new(start, end)?,
            kind: string("kind")?.with_context(|| format!("`[[entrance]]` table {index} has no kind"))?,
            trigger: string("trigger")?,
            method: string("method")?,
            label: string("label")?,
            targets,
        });
    }
    Ok(tables)
}

/// The byte offset of the `[[` of every `[[entrance]]` header line.
fn entrance_headers(text: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let rest = &line[indent..];
        if let Some(inner) = rest.strip_prefix("[[")
            && let Some(close) = inner.find("]]")
            && inner[..close].trim() == "entrance"
        {
            offsets.push(line_start + indent);
        }
        line_start += line.len();
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tables_and_their_token_exact_ranges() {
        let text = "version = 1\n\n# A comment.\n[[entrance]]\nkind = \"command\"\ntrigger = \"import\"\n\
                    handler = \"src/a.ts#run\"  # trailing comment\n\n  [[ entrance ]]\nkind = \"http\"\n\
                    method = \"GET\"\nlabel = \"Health\"\nhandler = [\"src/a.ts:3\", \"src/b.ts\"]\n";
        let tables = manual_tables(text).unwrap();
        assert_eq!(tables.len(), 2);
        let first = &tables[0];
        let slice = &text[first.range.as_usize()];
        assert!(slice.starts_with("[[entrance]]\nkind"));
        assert!(slice.ends_with("handler = \"src/a.ts#run\""));
        assert_eq!(first.trigger.as_deref(), Some("import"));
        assert_eq!(first.targets, ["src/a.ts#run"]);
        let second = &tables[1];
        let slice = &text[second.range.as_usize()];
        assert!(slice.starts_with("[[ entrance ]]"));
        assert!(slice.ends_with("\"src/b.ts\"]"));
        assert_eq!(
            (second.method.as_deref(), second.label.as_deref()),
            (Some("GET"), Some("Health"))
        );
        assert_eq!(second.targets, ["src/a.ts:3", "src/b.ts"]);
        assert!(second.trigger.is_none());
    }

    #[test]
    fn rejects_tables_without_handlers() {
        assert!(manual_tables("[[entrance]]\nkind = \"command\"\n").is_err());
        assert!(manual_tables("version = 1\n").unwrap().is_empty());
    }
}
