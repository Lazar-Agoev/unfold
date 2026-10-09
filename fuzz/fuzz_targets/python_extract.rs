//! Arbitrary bytes as a Python source: extraction under every supported grammar version either
//! succeeds or reports a failure, the facts survive the cache encoding, and highlighting produces
//! well-formed spans. Nothing may panic or overflow.

#![no_main]

use libfuzzer_sys::fuzz_target;
use unfold_core::highlight::{Highlighter, spans_are_well_formed};
use unfold_engine::settings::PythonVersion;
use unfold_fuzz::{check_facts_round_trip, snapshot};
use unfold_lang_python::{PythonHighlighter, extract};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Some(file) = snapshot("pkg/a.py", text) else {
        return;
    };
    for version in [PythonVersion::V3_12, PythonVersion::V3_13, PythonVersion::V3_14] {
        if let Ok(facts) = extract(&file, version) {
            check_facts_round_trip(&facts);
        }
    }
    let spans = PythonHighlighter.highlight(text);
    assert!(spans_are_well_formed(text, &spans), "malformed highlight spans");
});
