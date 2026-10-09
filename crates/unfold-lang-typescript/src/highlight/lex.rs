//! Tokens for highlighting: oxc's token stream and comments when the text parses, otherwise the
//! forgiving built-in scanner ([`crate::scan`]).

use oxc_allocator::Allocator;
use oxc_parser::{Kind, Parser, config::TokensParserConfig};
use oxc_span::SourceType;

use crate::scan::{Lexed, Lexeme, scan};

/// The lexemes of `text`, in order and non-overlapping.
pub(super) fn lex(text: &str) -> Vec<Lexeme> {
    let lexemes = crate::stack::on_parse_stack(text, || from_oxc(text))
        .ok()
        .flatten()
        .unwrap_or_else(|| scan(text));
    with_braces(text, lexemes)
}

/// oxc's tokens and comments, or `None` if the parser stopped early. The text is read as
/// TypeScript, or as TSX when only that reads it without errors.
fn from_oxc(text: &str) -> Option<Vec<Lexeme>> {
    let allocator = Allocator::default();
    let text = crate::extract::without_bom(text);
    let parse = |source_type| {
        Parser::new(&allocator, &text, source_type)
            .with_config(TokensParserConfig)
            .parse()
    };
    let mut parsed = parse(SourceType::ts());
    if !parsed.diagnostics.is_empty() {
        let tsx = parse(SourceType::tsx());
        if tsx.diagnostics.is_empty() {
            parsed = tsx;
        }
    }
    if parsed.fatal_error {
        return None;
    }
    let lexeme = |kind, start, end| Lexeme {
        kind,
        start,
        end,
        in_brace: false,
    };
    let mut tokens = parsed
        .tokens
        .iter()
        .filter(|token| token.start() < token.end())
        .map(|token| {
            let kind = match token.kind() {
                Kind::Str | Kind::NoSubstitutionTemplate | Kind::RegExp => Lexed::String,
                Kind::TemplateHead => Lexed::TemplateHead,
                Kind::TemplateMiddle => Lexed::TemplateMiddle,
                Kind::TemplateTail => Lexed::TemplateTail,
                Kind::PrivateIdentifier => Lexed::Word,
                kind if kind.is_number() => Lexed::Number,
                kind if kind.is_identifier_name() => Lexed::Word,
                _ => Lexed::Punct,
            };
            lexeme(kind, token.start(), token.end())
        });
    let hashbang = parsed.program.hashbang.as_ref().map(|hashbang| hashbang.span);
    let mut comments = hashbang
        .into_iter()
        .chain(parsed.program.comments.iter().map(|comment| comment.span))
        .map(|span| lexeme(Lexed::Comment, span.start, span.end));
    // Merge the two ordered streams.
    let mut merged = Vec::with_capacity(parsed.tokens.len() + parsed.program.comments.len());
    let (mut token, mut comment) = (tokens.next(), comments.next());
    loop {
        match (token, comment) {
            (Some(t), Some(c)) if c.start < t.start => {
                merged.push(c);
                comment = comments.next();
            }
            (Some(t), _) => {
                merged.push(t);
                token = tokens.next();
            }
            (None, Some(c)) => {
                merged.push(c);
                comment = comments.next();
            }
            (None, None) => break,
        }
    }
    let mut end = 0;
    merged.retain(|lexeme| {
        let keep = lexeme.start >= end;
        if keep {
            end = lexeme.end;
        }
        keep
    });
    Some(merged)
}

/// Marks which lexemes sit directly inside a `{` block (not a template or parentheses).
fn with_braces(text: &str, mut lexemes: Vec<Lexeme>) -> Vec<Lexeme> {
    #[derive(PartialEq)]
    enum Open {
        Block,
        Template,
        Paren,
    }
    let mut open: Vec<Open> = Vec::new();
    for lexeme in &mut lexemes {
        match lexeme.kind {
            Lexed::TemplateHead => open.push(Open::Template),
            Lexed::TemplateTail => {
                if open.last() == Some(&Open::Template) {
                    open.pop();
                }
            }
            Lexed::Punct => match text.get(lexeme.start as usize..lexeme.end as usize) {
                Some("(" | "[") => open.push(Open::Paren),
                Some(")" | "]") => {
                    if open.last() == Some(&Open::Paren) {
                        open.pop();
                    }
                }
                Some("{" | "${") => open.push(Open::Block),
                Some("}") => {
                    open.pop();
                }
                _ => {}
            },
            _ => {}
        }
        lexeme.in_brace = open.last() == Some(&Open::Block);
    }
    lexemes
}
