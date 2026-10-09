//! Parsing one file with Ruff's parser under the configured grammar version, and every reason a
//! file fails: a declared non-UTF-8 encoding, a syntax error, syntax newer than the configured
//! version, or a compile-time error the proof of concept's grammar reported (see
//! [`crate::checks`]).
//!
//! Failures never recover a partial graph: a file either parses completely or fails with a
//! message and the byte offset of its first problem.

use ruff_python_ast::{ModModule, PythonVersion as RuffVersion};
use ruff_python_parser::{Mode, ParseOptions, Parsed};
use ruff_text_size::TextSize;
use unfold_engine::SourceFailure;
use unfold_engine::settings::PythonVersion;

/// The parsed module with its tokens.
pub(crate) type Module = Parsed<ModModule>;

/// Ruff's spelling of a configured grammar version.
pub(crate) fn ruff_version(version: PythonVersion) -> RuffVersion {
    match version {
        PythonVersion::V3_12 => RuffVersion::PY312,
        PythonVersion::V3_13 => RuffVersion::PY313,
        PythonVersion::V3_14 => RuffVersion::PY314,
    }
}

/// A failure with a message and an optional byte offset.
pub(crate) fn failure(message: impl Into<Box<str>>, offset: Option<TextSize>) -> SourceFailure {
    SourceFailure {
        message: message.into(),
        offset: offset.map(|offset| offset.to_u32()),
    }
}

/// Parses `text` with the grammar of `version`. Ruff's parser always produces a tree, with
/// error recovery; [`syntax_error`] says whether the file failed. (Module mode always yields a
/// module; `None` only keeps that invariant out of the panic paths.)
pub(crate) fn parse_module(text: &str, version: PythonVersion) -> Option<Module> {
    let options = ParseOptions::from(Mode::Module).with_target_version(ruff_version(version));
    ruff_python_parser::parse_unchecked(text, options).try_into_module()
}

/// The failure for a tree with syntax errors or syntax its grammar version does not support:
/// the first problem by offset, with a count of the rest.
pub(crate) fn syntax_error(module: &Module) -> Option<SourceFailure> {
    let mut problems: Vec<(TextSize, String)> = module
        .errors()
        .iter()
        .map(|error| (error.location.start(), error.error.to_string()))
        .chain(
            module
                .unsupported_syntax_errors()
                .iter()
                .map(|error| (error.range.start(), error.to_string())),
        )
        .collect();
    if problems.is_empty() {
        return None;
    }
    problems.sort();
    let count = problems.len();
    let (offset, message) = problems.swap_remove(0);
    let message = match count {
        1 => message,
        _ => format!("{message} (and {} more problems)", count - 1),
    };
    Some(failure(message, Some(offset)))
}

/// Fails a file whose first or second line declares an encoding other than UTF-8 (PEP 263).
///
/// The declaration is a comment matching `coding[:=]\s*([-\w.]+)` on line 1, or on line 2 when
/// line 1 is blank or a comment, as CPython reads it. UTF-8 is accepted under the spellings
/// CPython normalizes to it (`utf-8`, `utf8`, `utf_8`, `UTF-8`, `utf-8-sig`, ...). A byte-order
/// mark is skipped and kept in the text.
pub(crate) fn check_encoding(text: &str) -> Result<(), SourceFailure> {
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut offset = text.len() - body.len();
    let mut rest = body;
    for line_number in 0..2 {
        let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let line = &rest[..end];
        if let Some(encoding) = cookie(line) {
            if is_utf8_name(encoding) {
                return Ok(());
            }
            let at = offset + line.find(encoding).unwrap_or(0);
            return Err(failure(
                format!("unsupported source encoding `{encoding}`: Python sources must be UTF-8"),
                u32::try_from(at).ok().map(TextSize::new),
            ));
        }
        let content = line.trim_start_matches([' ', '\t', '\x0c']);
        if line_number == 0 && !(content.is_empty() || content.starts_with('#')) {
            return Ok(());
        }
        let next = if rest[end..].starts_with("\r\n") {
            end + 2
        } else {
            (end + 1).min(rest.len())
        };
        offset += next;
        rest = &rest[next..];
    }
    Ok(())
}

/// The encoding named by an encoding declaration comment, if `line` is one.
fn cookie(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches([' ', '\t', '\x0c']);
    let comment = rest.strip_prefix('#')?;
    let at = comment.find("coding")?;
    let after = &comment[at + "coding".len()..];
    let after = after.strip_prefix([':', '='])?;
    let after = after.trim_start_matches([' ', '\t']);
    let end = after
        .find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(after.len());
    (end > 0).then(|| &after[..end])
}

/// Whether CPython reads `name` as UTF-8.
fn is_utf8_name(name: &str) -> bool {
    let normal = name.to_ascii_lowercase().replace('_', "-");
    normal == "utf-8" || normal == "utf8" || normal.starts_with("utf-8-") || normal.starts_with("utf8-")
}

/// Drops a syntax tree without overflowing the stack. Dropping recurses once per level of
/// nesting, so a tree `depth` levels deep gets a stack segment big enough for it.
pub(crate) fn release(module: Module, depth: usize) {
    // A generous bound on the stack one level of the tree's drop glue uses.
    const FRAME: usize = 512;
    const RED_ZONE: usize = 64 * 1024;
    let needed = depth.saturating_mul(FRAME).saturating_add(RED_ZONE);
    stacker::maybe_grow(needed, needed.saturating_add(RED_ZONE), move || drop(module));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding_error(text: &str) -> Option<SourceFailure> {
        check_encoding(text).err()
    }

    #[test]
    fn utf8_declarations_pass_and_others_fail() {
        for text in [
            "# coding: utf-8\nx = 1\n",
            "#!/usr/bin/env python\n# -*- coding: UTF_8 -*-\n",
            "# vim: set fileencoding=utf8 :\n",
            "\u{FEFF}# coding=utf-8-sig\n",
            "x = 1\n# coding: latin-1\n",
            "",
        ] {
            assert_eq!(encoding_error(text), None, "{text:?}");
        }
        let failed = encoding_error("# coding: latin-1\ndef f(): pass\n").unwrap();
        assert_eq!(failed.offset, Some(10));
        assert!(failed.message.contains("latin-1"));
        assert!(encoding_error("\n# coding: cp1252\n").is_some());
        assert!(encoding_error("#!/bin/python\r\n# coding: ascii\r\n").is_some());
        assert!(encoding_error("\u{FEFF}# coding: latin-1\n").is_some());
    }

    fn problem(text: &str, version: PythonVersion) -> Option<SourceFailure> {
        syntax_error(&parse_module(text, version).unwrap())
    }

    #[test]
    fn syntax_errors_fail_with_an_offset() {
        let failed = problem("def broken(:\n", PythonVersion::V3_14).unwrap();
        assert!(failed.offset.is_some());
        let failed = problem("value = t'hello'\n", PythonVersion::V3_12).unwrap();
        assert_eq!(failed.offset, Some(8));
        assert_eq!(problem("value = t'hello'\n", PythonVersion::V3_14), None);
    }
}
