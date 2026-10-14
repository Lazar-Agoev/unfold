//! A verified file's text as the drawing reads it: slices on one line, line spans, and which
//! lines are code a gap tick counts.
//!
//! A line is **code** unless it is blank, a comment, only closing punctuation (`});`), a
//! TypeScript `break;` alone (the end of a `switch` case), or part of an import statement
//! (imports are links, listed in the module's header).

use rustc_hash::FxHashSet;
use unfold_core::vocab::{Language, NodeKind};
use unfold_core::{ByteRange, LineSpan, Node};

use crate::source::Source;
use crate::tree::FileTree;

/// The most characters a value or code line keeps on one line.
const MAX_CHARS: usize = 80;

/// A verified file's text, with its code lines found.
#[derive(Debug)]
pub(super) struct FileText {
    source: Source,
    /// `code[line - 1]`: whether one-based `line` is code.
    code: Vec<bool>,
}

impl FileText {
    pub(super) fn new(source: Source, language: Language) -> Self {
        let mut code = Vec::new();
        let mut import_depth: Option<i32> = None;
        for line in source.text().lines() {
            let trimmed = line.trim();
            if let Some(depth) = import_depth.as_mut() {
                *depth += bracket_balance(trimmed);
                if *depth <= 0 {
                    import_depth = None;
                }
                code.push(false);
                continue;
            }
            if starts_import(trimmed, language) {
                let depth = bracket_balance(trimmed);
                import_depth = (depth > 0).then_some(depth);
                code.push(false);
                continue;
            }
            code.push(
                !trimmed.is_empty()
                    && !is_comment(trimmed, language)
                    && !is_closing(trimmed)
                    && !is_case_end(trimmed, language),
            );
        }
        Self { source, code }
    }

    /// The text of `range`, on one line (whitespace runs become one space).
    pub(super) fn one_line(&self, range: ByteRange) -> Option<String> {
        self.source.slice(range).ok().map(one_line)
    }

    /// The text of `range` as written.
    pub(super) fn slice(&self, range: ByteRange) -> Option<&str> {
        self.source.slice(range).ok()
    }

    /// The lines `range` starts and ends on.
    pub(super) fn lines(&self, range: ByteRange) -> LineSpan {
        self.source.lines().span(range)
    }

    /// Whether one-based `line` is code.
    pub(super) fn is_code(&self, line: u32) -> bool {
        line.checked_sub(1)
            .and_then(|index| self.code.get(index as usize))
            .copied()
            .unwrap_or(false)
    }

    /// One-based `line`, trimmed and cut to one short line.
    pub(super) fn line(&self, line: u32) -> String {
        let text = self.source.text();
        let index = self.source.lines();
        let start = index.line_start(line).unwrap_or(index.len()) as usize;
        let end = index.line_start(line + 1).map_or(text.len(), |end| end as usize);
        cut(text.get(start..end).unwrap_or_default().trim())
    }
}

/// The code lines of `container`'s body (a definition's or a part's) that a gap tick may count:
/// its body's lines, without the lines that draw as structure (where the container, its
/// control block, and that block's parts start, and their heads), the doc, and the definitions
/// written inside (with what runs when they are defined), which are cards of their own.
/// `definition` is the definition being drawn.
pub(super) fn candidates(tree: &FileTree, text: &FileText, definition: u32, container: u32) -> Vec<u32> {
    let node = tree.node(container);
    let Some(body) = node.body else {
        return Vec::new();
    };
    let mut skip = FxHashSet::default();
    let mut skip_range = |range: ByteRange| {
        let span = text.lines(range);
        skip.extend(span.start..=span.end);
    };
    let mut skip_structure = |node: &Node| {
        // A part whose range is its suite starts on its first statement, which is not structure.
        if node.body.is_none_or(|body| node.range.start < body.start) {
            skip_range(ByteRange {
                start: node.range.start,
                end: node.range.start,
            });
        }
        if let Some(head) = node.head {
            skip_range(head);
        }
    };
    if node.kind() != NodeKind::Module {
        skip_structure(node);
    }
    if node.kind() == NodeKind::Part
        && let Some(control) = node.parent
    {
        skip_structure(tree.node(control));
        for &part in tree.children(control) {
            if tree.kind(part) == NodeKind::Part {
                skip_structure(tree.node(part));
            }
        }
    }
    if let Some(doc) = node.doc {
        skip_range(doc);
    }
    for id in tree.subtree(container).skip(1) {
        let inner = tree.node(id);
        let defined = inner.kind().is_definition() && tree.owner(id) == Some(definition);
        if defined || inner.r#for.is_some() {
            skip_range(inner.range);
        }
    }
    let span = text.lines(body);
    (span.start..=span.end)
        .filter(|line| !skip.contains(line) && text.is_code(*line))
        .collect()
}

/// `text` on one line: every run of whitespace becomes one space, cut to [`MAX_CHARS`].
pub(super) fn one_line(text: &str) -> String {
    cut(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// `text` cut to [`MAX_CHARS`] characters, ending with `…` when cut.
fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn starts_import(line: &str, language: Language) -> bool {
    let word = |word: &str| {
        line.strip_prefix(word)
            .is_some_and(|rest| rest.starts_with([' ', '{', '*']))
    };
    match language {
        Language::Python => word("import") || word("from"),
        Language::TypeScript => word("import") || (word("export") && line.contains(" from ")),
    }
}

fn is_comment(line: &str, language: Language) -> bool {
    match language {
        Language::Python => line.starts_with('#'),
        Language::TypeScript => line.starts_with("//") || line.starts_with("/*") || line.starts_with('*'),
    }
}

/// A TypeScript `break` that ends a `switch` case: the case's structure, not an exit (an exit
/// `break` is a block, whose statement covers its line).
fn is_case_end(line: &str, language: Language) -> bool {
    language == Language::TypeScript && matches!(line, "break;" | "break")
}

fn is_closing(line: &str) -> bool {
    line.chars()
        .all(|character| matches!(character, ')' | ']' | '}' | ';' | ',' | ' '))
}

/// Opening brackets minus closing brackets on `line`.
fn bracket_balance(line: &str) -> i32 {
    line.chars()
        .map(|character| match character {
            '(' | '[' | '{' => 1,
            ')' | ']' | '}' => -1,
            _ => 0,
        })
        .sum()
}

/// A Python docstring's or a JSDoc's text: its quotes or comment marks removed, each line
/// trimmed, blank lines at either end dropped.
pub(super) fn doc_text(doc: &str) -> Option<String> {
    let inner = if let Some(rest) = doc.strip_prefix("/**") {
        rest.strip_suffix("*/").unwrap_or(rest)
    } else {
        let unprefixed = doc.trim_start_matches(['r', 'R', 'u', 'U', 'b', 'B', 'f', 'F']);
        ["\"\"\"", "'''", "\"", "'"]
            .iter()
            .find_map(|quote| unprefixed.strip_prefix(quote)?.strip_suffix(quote))
            .unwrap_or(unprefixed)
    };
    let lines: Vec<&str> = inner
        .lines()
        .map(|line| {
            let line = line.trim();
            line.strip_prefix('*').map_or(line, str::trim_start)
        })
        .collect();
    let first = lines.iter().position(|line| !line.is_empty())?;
    let last = lines.iter().rposition(|line| !line.is_empty())?;
    Some(lines[first..=last].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_lose_their_quotes_and_marks() {
        assert_eq!(
            doc_text("\"\"\"Charge an order and save it.\"\"\"").as_deref(),
            Some("Charge an order and save it.")
        );
        assert_eq!(
            doc_text("/**\n * An order.\n *\n * Sent by the API.\n */").as_deref(),
            Some("An order.\n\nSent by the API.")
        );
        assert_eq!(doc_text("r'''\n  Raw.\n'''").as_deref(), Some("Raw."));
        assert_eq!(doc_text("\"\"\"\"\"\""), None);
    }

    #[test]
    fn values_fit_on_one_short_line() {
        assert_eq!(one_line("{\n  id,\n  total: 0 }"), "{ id, total: 0 }");
        assert_eq!(one_line(&"x".repeat(90)).chars().count(), MAX_CHARS + 1);
    }

    #[test]
    fn import_and_punctuation_lines_are_not_code() {
        assert!(starts_import(
            "from shop.store import load_order",
            Language::Python
        ));
        assert!(starts_import(
            "import { Router } from \"express\";",
            Language::TypeScript
        ));
        assert!(starts_import("export { a } from \"./a\";", Language::TypeScript));
        assert!(!starts_import("important = 1", Language::Python));
        assert!(!starts_import("export const a = 1;", Language::TypeScript));
        assert!(is_closing("});"));
        assert!(!is_closing("} else {"));
        assert!(is_comment("# note", Language::Python));
        assert!(is_comment(" * jsdoc".trim(), Language::TypeScript));
        assert_eq!(bracket_balance("from x import ("), 1);
    }
}
