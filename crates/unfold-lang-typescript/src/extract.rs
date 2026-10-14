//! Per-file extraction: parse strictly with oxc, take the independent census, lower the tree to
//! nodes (with the value table, call and import facts, and typed records), bind it, and record
//! the payload.
//!
//! Parsing is strict: a file with any syntax diagnostic fails with the first diagnostic's message
//! and byte offset, and a recovered tree is never used. Declaration files (`.d.ts`, `.d.mts`,
//! `.d.cts`) are not extracted.
//!
//! Each file is parsed on a thread with a large enough stack ([`crate::stack`]), and a file nested
//! deeper than the census's limit fails instead of overflowing.

use std::borrow::Cow;

use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser, config::TokensParserConfig};
use oxc_span::SourceType;
use serde::{Deserialize, Serialize};
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::SourceFailure;
use unfold_engine::facts::{FileFacts, GraphFacts, TypeRefFact};

use crate::bind;
use crate::census::{self, CensusRecord};
use crate::config::BindingContext;
use crate::lower;
use crate::module_syntax;
use crate::payload::Payload;
use crate::stack::{self, StackError};

/// Inputs that shape a file's facts besides its bytes. Every field is part of the settings hash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtractOptions {
    /// TypeScript's `verbatimModuleSyntax`: named imports and re-exports whose specifiers are all
    /// type-only (or empty) stay runtime dependencies instead of being erased.
    pub verbatim_module_syntax: bool,
    /// The target-dependent binding semantics of the file's configuration (which body
    /// variables a parameter list can see).
    pub binding: BindingContext,
}

/// Whether `path` is a TypeScript source this front end indexes: `.ts`, `.tsx`, `.mts`, or
/// `.cts`, but never a declaration file.
pub fn is_typescript_source(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let declaration = [".d.ts", ".d.mts", ".d.cts"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    let source = [".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix));
    source && !declaration
}

/// The oxc source type for a TypeScript source path. `.ts` and `.tsx` files are modules when
/// they use module syntax and scripts otherwise.
pub(crate) fn source_type(path: &RepoPath) -> Option<SourceType> {
    let name = path.as_str();
    if !is_typescript_source(name) {
        return None;
    }
    let ts = SourceType::ts();
    Some(if name.ends_with(".tsx") {
        SourceType::tsx()
    } else if name.ends_with(".mts") {
        ts.with_module(true)
    } else if name.ends_with(".cts") {
        ts.with_commonjs(true)
    } else {
        ts
    })
}

/// The text oxc parses. A leading byte-order mark, and a shebang line right after it (which
/// TypeScript accepts but oxc does not), become spaces, so every offset is unchanged.
pub(crate) fn without_bom(text: &str) -> Cow<'_, str> {
    let Some(rest) = text.strip_prefix('\u{feff}') else {
        return Cow::Borrowed(text);
    };
    let shebang = if rest.starts_with("#!") {
        rest.find(['\n', '\r', '\u{2028}', '\u{2029}'])
            .unwrap_or(rest.len())
    } else {
        0
    };
    let blank = '\u{feff}'.len_utf8() + shebang;
    Cow::Owned(format!("{}{}", " ".repeat(blank), &text[blank..]))
}

fn failure(message: impl Into<Box<str>>, offset: Option<u32>) -> SourceFailure {
    SourceFailure {
        message: message.into(),
        offset,
    }
}

/// Extracts one file's facts.
pub fn extract(file: &Snapshot, options: ExtractOptions) -> Result<FileFacts, SourceFailure> {
    let Some(source_type) = source_type(file.path()) else {
        return Err(failure(
            "not a TypeScript source file (declaration files are not indexed)",
            None,
        ));
    };
    match stack::on_parse_stack(file.text(), || extract_here(file, source_type, options)) {
        Ok(result) => result,
        Err(StackError::TooDeep) => Err(failure(
            format!("syntax is nested more than {} levels deep", stack::MAX_NESTING),
            None,
        )),
        Err(StackError::Thread) => Err(failure("the TypeScript parser could not run on this file", None)),
    }
}

/// Parses `file` strictly on the current thread and returns the allocator-free results.
fn extract_here(
    file: &Snapshot,
    source_type: SourceType,
    options: ExtractOptions,
) -> Result<FileFacts, SourceFailure> {
    let allocator = Allocator::default();
    let text = file.text();
    let parse_options = ParseOptions {
        preserve_parens: true,
        ..ParseOptions::default()
    };
    let parse_text = without_bom(text);
    let parsed = Parser::new(&allocator, &parse_text, source_type)
        .with_options(parse_options)
        .with_config(TokensParserConfig)
        .parse();
    if let Some(diagnostic) = parsed.diagnostics.first() {
        let labels = &diagnostic.labels;
        let offset = labels
            .iter()
            .find(|label| label.primary())
            .or_else(|| labels.first())
            .map(|label| label.offset());
        return Err(failure(diagnostic.message.to_string(), offset));
    }
    if parsed.fatal_error {
        return Err(failure("the file could not be parsed", None));
    }
    let mut census = CensusRecord::take(
        &parsed.program,
        &parsed.tokens,
        file.len(),
        options.verbatim_module_syntax,
    )
    .map_err(|_| {
        failure(
            format!("syntax is nested more than {} levels deep", census::MAX_DEPTH),
            None,
        )
    })?;
    let mut lowered = lower::lower(
        &parsed.program,
        file,
        &parsed.tokens,
        options.verbatim_module_syntax,
    )
    .map_err(|error| failure(format!("cannot key a node: {error}"), None))?;
    census.cut(&lowered.cuts);
    let jsx = source_type.is_jsx() && module_syntax::uses_jsx(&parsed.program);
    let module = module_syntax::collect(&parsed.program, parsed.module_record.has_module_syntax, jsx);
    let input = bind::Input {
        program: &parsed.program,
        nodes: &lowered.definitions,
        module: &module,
        context: options.binding,
    };
    let linked = std::mem::take(&mut lowered.linked);
    let (binding, type_sites) = bind::bind(&input, &lowered.values, linked);
    let type_refs = type_sites
        .iter()
        .map(|site| {
            let name = text
                .get(site.range.start as usize..site.range.end as usize)
                .unwrap_or_default();
            TypeRefFact::new(
                lowered.owner(site.range),
                site.range,
                name,
                site.qualified,
                site.usage,
            )
        })
        .collect();
    let payload = Payload {
        imports: lowered.import_kinds,
        module,
        census,
        binding,
    };
    let graph = GraphFacts::new(
        lowered.nodes,
        lowered.values.finish(),
        lowered.calls,
        lowered.imports,
        type_refs,
        lowered.types,
    )
    .map_err(|error| failure(error.to_string(), None))?;
    let payload = payload
        .to_bytes()
        .map_err(|error| failure(format!("cannot encode TypeScript facts: {error}"), None))?;
    Ok(FileFacts::new(graph, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_typescript_sources_but_not_declarations() {
        for path in ["a.ts", "src/a.tsx", "a.mts", "a.cts", "a.d.tsx"] {
            assert!(is_typescript_source(path), "{path}");
        }
        for path in ["a.d.ts", "x/a.d.mts", "a.d.cts", ".ts", "a.js", "a.ts.map"] {
            assert!(!is_typescript_source(path), "{path}");
        }
    }
}
