//! A small mixed program from arbitrary bytes (see [`unfold_fuzz::program_from_bytes`]): every
//! file is extracted, then the whole program is linked, resolved, detected, assembled, and
//! validated the way an update builds it. Nothing may panic or overflow, the backend may not
//! report an internal error, and the graph built from facts that extracted must validate.

#![no_main]

use libfuzzer_sys::fuzz_target;
use unfold_fuzz::{compile, program_from_bytes};

fuzz_target!(|data: &[u8]| {
    let program = program_from_bytes(data);
    if program.sources.is_empty() {
        return;
    }
    if let Err(problem) = compile(&program) {
        panic!("{problem}");
    }
});
