//! Arbitrary bytes as a TypeScript source: extraction (as `.ts` and `.tsx`, with default and
//! legacy-target options) either succeeds or reports a failure, the facts survive the cache
//! encoding, and highlighting produces well-formed spans. Nothing may panic or overflow.

#![no_main]

use libfuzzer_sys::fuzz_target;
use unfold_core::highlight::{Highlighter, spans_are_well_formed};
use unfold_fuzz::{check_facts_round_trip, snapshot};
use unfold_lang_typescript::{BindingContext, ExtractOptions, TypeScriptHighlighter, extract};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let legacy = ExtractOptions {
        verbatim_module_syntax: true,
        binding: BindingContext {
            preserve_parameters: false,
            lower_optional_chains: true,
            lower_object_rest: true,
            standard_class_fields: false,
        },
    };
    for path in ["a.ts", "a.tsx"] {
        let Some(file) = snapshot(path, text) else {
            return;
        };
        for options in [ExtractOptions::default(), legacy] {
            if let Ok(facts) = extract(&file, options) {
                check_facts_round_trip(&facts);
            }
        }
    }
    let spans = TypeScriptHighlighter.highlight(text);
    assert!(spans_are_well_formed(text, &spans), "malformed highlight spans");
});
