//! TypeScript syntax highlighting for the Code view.
//!
//! Colors are lexical, as in the proof of concept's reader: the text is split into tokens and a
//! few neighbor rules pick a class. The tokens come from oxc's lexer (through the parser's token
//! stream, with comments from the parsed program); text the parser cannot get through, such as a
//! slice cut out of a class body, falls back to a small built-in scanner. Highlighting is a
//! reading aid, never a semantic claim.
//!
//! | Token | Class |
//! | --- | --- |
//! | Comment | `comment` |
//! | String, plain template, regular expression | `string` |
//! | Template head, middle, tail | `string`, with `${` and `}` as `keyword` |
//! | Number | `number` |
//! | Reserved word, not after `.` | `keyword` |
//! | Word before `(` or after `function` | `function` |
//! | Word after `.` or `?.` and not before another `.` | `property` |
//! | Word before `:` after `{` or `,` inside braces | `property` |
//! | Other TypeScript keyword | `keyword` |
//! | `PascalCase` word with a lowercase letter | `type` |
//!
//! Adjacent spans of the same class merge.

mod lex;

use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::text::ByteRange;
use unfold_core::vocab::Language;

use crate::scan::{Lexed, Lexeme, RESERVED};

/// Every other TypeScript keyword, contextual or strict-mode reserved.
const KEYWORDS: &[&str] = &[
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "abstract",
    "accessor",
    "as",
    "asserts",
    "assert",
    "any",
    "async",
    "await",
    "boolean",
    "constructor",
    "declare",
    "get",
    "infer",
    "intrinsic",
    "is",
    "keyof",
    "module",
    "namespace",
    "never",
    "out",
    "readonly",
    "require",
    "number",
    "object",
    "satisfies",
    "set",
    "string",
    "symbol",
    "type",
    "undefined",
    "unique",
    "unknown",
    "using",
    "from",
    "global",
    "bigint",
    "override",
    "of",
    "defer",
];

/// The TypeScript highlighter.
#[derive(Debug, Default, Clone, Copy)]
pub struct TypeScriptHighlighter;

impl Highlighter for TypeScriptHighlighter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
        let lexemes = lex::lex(text);
        classify(text, &lexemes)
    }
}

struct Spans {
    spans: Vec<HighlightSpan>,
}

impl Spans {
    fn push(&mut self, start: u32, end: u32, class: TokenClass) {
        if end <= start {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.class == class
            && last.range.end == start
        {
            last.range.end = end;
            return;
        }
        self.spans.push(HighlightSpan {
            range: ByteRange { start, end },
            class,
        });
    }
}

fn is_pascal_case(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && word.chars().all(|c| c.is_ascii_alphanumeric())
        && word.chars().skip(1).any(|c| c.is_ascii_lowercase())
}

fn classify(text: &str, lexemes: &[Lexeme]) -> Vec<HighlightSpan> {
    let mut out = Spans { spans: Vec::new() };
    let significant: Vec<usize> = (0..lexemes.len())
        .filter(|&index| lexemes[index].kind != Lexed::Comment)
        .collect();
    let mut position = 0usize;
    for (index, lexeme) in lexemes.iter().enumerate() {
        let (start, end) = (lexeme.start, lexeme.end);
        match lexeme.kind {
            Lexed::Comment => out.push(start, end, TokenClass::Comment),
            Lexed::String => out.push(start, end, TokenClass::String),
            Lexed::TemplateHead => {
                out.push(start, end.saturating_sub(2), TokenClass::String);
                out.push(end.saturating_sub(2), end, TokenClass::Keyword);
            }
            Lexed::TemplateMiddle => {
                out.push(start, start + 1, TokenClass::Keyword);
                out.push(start + 1, end.saturating_sub(2), TokenClass::String);
                out.push(end.saturating_sub(2), end, TokenClass::Keyword);
            }
            Lexed::TemplateTail => {
                out.push(start, start + 1, TokenClass::Keyword);
                out.push(start + 1, end, TokenClass::String);
            }
            Lexed::Number => out.push(start, end, TokenClass::Number),
            Lexed::Word => {
                // The significant lexemes before and after this one.
                while significant.get(position).is_some_and(|&at| at < index) {
                    position += 1;
                }
                let before = position
                    .checked_sub(1)
                    .and_then(|at| significant.get(at))
                    .map(|&at| &lexemes[at]);
                let after = significant.get(position + 1).map(|&at| &lexemes[at]);
                let word = text.get(start as usize..end as usize).unwrap_or_default();
                if let Some(class) = word_class(text, word, lexeme, before, after) {
                    out.push(start, end, class);
                }
            }
            Lexed::Punct => {}
        }
    }
    out.spans
}

fn word_class(
    text: &str,
    word: &str,
    lexeme: &Lexeme,
    before: Option<&Lexeme>,
    after: Option<&Lexeme>,
) -> Option<TokenClass> {
    let spelled = |lexeme: Option<&Lexeme>, spelling: &str| {
        lexeme.is_some_and(|found| {
            found.kind == Lexed::Punct && text.get(found.start as usize..found.end as usize) == Some(spelling)
        })
    };
    let word_is = |lexeme: Option<&Lexeme>, spelling: &str| {
        lexeme.is_some_and(|found| {
            found.kind == Lexed::Word && text.get(found.start as usize..found.end as usize) == Some(spelling)
        })
    };
    let member = spelled(before, ".") || spelled(before, "?.");
    let private = word.starts_with('#');
    if !member && !private && RESERVED.contains(&word) {
        return Some(TokenClass::Keyword);
    }
    if spelled(after, "(") || word_is(before, "function") {
        return Some(TokenClass::Function);
    }
    if member {
        return (!spelled(after, ".") && !spelled(after, "?.")).then_some(TokenClass::Property);
    }
    if spelled(after, ":") && (spelled(before, "{") || spelled(before, ",")) && lexeme.in_brace {
        return Some(TokenClass::Property);
    }
    if !private && KEYWORDS.contains(&word) {
        return Some(TokenClass::Keyword);
    }
    is_pascal_case(word).then_some(TokenClass::Type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unfold_core::highlight::spans_are_well_formed;

    fn classes(text: &str) -> Vec<(&str, TokenClass)> {
        TypeScriptHighlighter
            .highlight(text)
            .iter()
            .map(|span| {
                (
                    &text[span.range.start as usize..span.range.end as usize],
                    span.class,
                )
            })
            .collect()
    }

    #[test]
    fn classifies_like_the_proof_of_concept_reader() {
        let text = "// note\nexport async function load(id: string): Promise<Order> {\n  const rows = await db.query(`x${id}y`, /a+/g);\n  return { total: 1.5, rows };\n}\n";
        let found = classes(text);
        assert!(spans_are_well_formed(
            text,
            &TypeScriptHighlighter.highlight(text)
        ));
        for expected in [
            ("// note", TokenClass::Comment),
            ("export", TokenClass::Keyword),
            ("async", TokenClass::Keyword),
            ("function", TokenClass::Keyword),
            ("load", TokenClass::Function),
            ("string", TokenClass::Keyword),
            ("Promise", TokenClass::Type),
            ("Order", TokenClass::Type),
            ("const", TokenClass::Keyword),
            ("await", TokenClass::Keyword),
            ("query", TokenClass::Function),
            ("`x", TokenClass::String),
            ("${", TokenClass::Keyword),
            ("/a+/g", TokenClass::String),
            ("return", TokenClass::Keyword),
            ("total", TokenClass::Property),
            ("1.5", TokenClass::Number),
        ] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
        assert!(!found.iter().any(|(text, _)| *text == "rows" || *text == "db"));
    }

    #[test]
    fn templates_jsx_and_hashbangs() {
        let text = "#!/usr/bin/env node\nconst view = <Card title={`a${b}c${d}e`} />;\n";
        let found = classes(text);
        assert!(spans_are_well_formed(
            text,
            &TypeScriptHighlighter.highlight(text)
        ));
        for expected in [
            ("#!/usr/bin/env node", TokenClass::Comment),
            ("`a", TokenClass::String),
            ("${", TokenClass::Keyword),
            ("}", TokenClass::Keyword),
            ("c", TokenClass::String),
            ("e`", TokenClass::String),
            ("Card", TokenClass::Type),
        ] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
    }

    #[test]
    fn unparseable_slices_still_highlight() {
        let text = "  async load() { return this.items.map(item => item.id); }\n}";
        let found = classes(text);
        assert!(found.contains(&("load", TokenClass::Function)));
        assert!(found.contains(&("this", TokenClass::Keyword)));
        assert!(found.contains(&("id", TokenClass::Property)));
    }
}
