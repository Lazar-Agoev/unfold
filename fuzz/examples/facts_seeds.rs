//! Writes seeds for the `facts_decode` target: the encoded facts of every playground source, so
//! mutations start from well-formed cache entries (and their language payloads) instead of noise.
//!
//! Run from `fuzz/`: `cargo +nightly run --release --example facts_seeds`.

use std::path::Path;

use unfold_engine::settings::PythonVersion;
use unfold_fuzz::snapshot;
use unfold_lang_typescript::ExtractOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let playgrounds = manifest.join("../testdata/playgrounds");
    let out = manifest.join("corpus/facts_decode");
    std::fs::create_dir_all(&out)?;
    let mut pending = vec![playgrounds];
    let mut written = 0;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.to_string_lossy().into_owned();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let facts = if name.ends_with(".ts") {
                snapshot("a.ts", &text)
                    .and_then(|file| unfold_lang_typescript::extract(&file, ExtractOptions::default()).ok())
            } else if name.ends_with(".py") {
                snapshot("pkg/a.py", &text)
                    .and_then(|file| unfold_lang_python::extract(&file, PythonVersion::V3_14).ok())
            } else {
                None
            };
            if let Some(facts) = facts {
                std::fs::write(out.join(format!("seed-{written}")), facts.to_bytes()?)?;
                written += 1;
            }
        }
    }
    println!("wrote {written} seeds to {}", out.display());
    Ok(())
}
