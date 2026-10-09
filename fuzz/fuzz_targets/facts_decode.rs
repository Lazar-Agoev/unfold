//! Arbitrary bytes as a facts-cache entry: decoding either fails or yields facts whose
//! invariants hold, and linking and resolving decoded facts (as a TypeScript file and as a
//! Python file) must not panic or overflow. A corrupt entry on disk is reported, never trusted
//! into a crash.

#![no_main]

use libfuzzer_sys::fuzz_target;
use unfold_engine::FileFacts;
use unfold_fuzz::link_decoded;

fuzz_target!(|data: &[u8]| {
    let Ok(facts) = FileFacts::from_bytes(data) else {
        return;
    };
    link_decoded(facts);
});
