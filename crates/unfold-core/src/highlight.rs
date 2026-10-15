//! Syntax highlighting for the code panel.
//!
//! Language crates implement [`Highlighter`]; the workspace wires them into the reader, which never
//! parses source itself (docs/architecture.md#ownership, rule 4). A highlighter classifies byte spans of one
//! file's text into a small, closed set of [`TokenClass`]es. Text outside every span is plain.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::text::ByteRange;
use crate::vocab::Language;

/// What a highlighted span is. Each class serializes as its snake-case name, which the UI maps
/// to a theme color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TokenClass {
    /// A reserved word: `function`, `class`, `def`, `return`, `if`, ...
    Keyword,
    /// A line or block comment, including documentation comments.
    Comment,
    /// A string, template, or bytes literal, including its quotes.
    String,
    /// A numeric literal.
    Number,
    /// A literal constant: `true`, `false`, `null`, `undefined`, `None`, ...
    Constant,
    /// An operator: `=`, `+`, `=>`, `and`, ...
    Operator,
    /// Brackets, commas, colons, and other punctuation.
    Punctuation,
    /// A type name or annotation.
    Type,
    /// The name of a declared or called function.
    Function,
    /// A decorator, including its `@`.
    Decorator,
    /// A property or attribute name.
    Property,
    /// A parameter name.
    Parameter,
    /// Any other identifier.
    Variable,
}

impl TokenClass {
    /// Every class, in declaration order.
    pub const ALL: &'static [TokenClass] = &[
        Self::Keyword,
        Self::Comment,
        Self::String,
        Self::Number,
        Self::Constant,
        Self::Operator,
        Self::Punctuation,
        Self::Type,
        Self::Function,
        Self::Decorator,
        Self::Property,
        Self::Parameter,
        Self::Variable,
    ];

    /// The class's serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "number",
            Self::Constant => "constant",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Type => "type",
            Self::Function => "function",
            Self::Decorator => "decorator",
            Self::Property => "property",
            Self::Parameter => "parameter",
            Self::Variable => "variable",
        }
    }
}

impl fmt::Display for TokenClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One classified span of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HighlightSpan {
    /// The span's UTF-8 byte range in the highlighted text.
    pub range: ByteRange,
    /// What the span is.
    pub class: TokenClass,
}

/// Classifies one language's source text for display.
///
/// Highlighting is presentation only: it must tolerate text that does not parse (classifying
/// what it can and leaving the rest plain), must never panic, and must return well-formed spans
/// ([`Highlighter::highlight`]).
pub trait Highlighter: Send + Sync {
    /// The language this highlighter reads.
    fn language(&self) -> Language;

    /// The classified spans of `text`: nonempty, inside the text, on character boundaries,
    /// sorted by start, and non-overlapping.
    fn highlight(&self, text: &str) -> Vec<HighlightSpan>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, end: u32, class: TokenClass) -> HighlightSpan {
        HighlightSpan {
            range: ByteRange::new(start, end).unwrap(),
            class,
        }
    }

    #[test]
    fn classes_serialize_as_their_names() {
        for &class in TokenClass::ALL {
            let json = serde_json::to_string(&class).unwrap();
            assert_eq!(json, format!("\"{}\"", class.as_str()));
            assert_eq!(serde_json::from_str::<TokenClass>(&json).unwrap(), class);
        }
        assert_eq!(TokenClass::Keyword.to_string(), "keyword");
    }

    struct Words;

    impl Highlighter for Words {
        fn language(&self) -> Language {
            Language::Python
        }

        fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
            let mut spans = Vec::new();
            let mut offset = 0u32;
            for word in text.split(' ') {
                let len = u32::try_from(word.len()).unwrap();
                if word == "def" {
                    spans.push(span(offset, offset + len, TokenClass::Keyword));
                }
                offset += len + 1;
            }
            spans
        }
    }

    #[test]
    fn highlighters_are_object_safe() {
        let highlighter: &dyn Highlighter = &Words;
        let text = "def f def";
        let spans = highlighter.highlight(text);
        assert_eq!(highlighter.language(), Language::Python);
        assert_eq!(spans.len(), 2);
        assert!(crate::testkit::spans_are_well_formed(text, &spans));
    }
}
