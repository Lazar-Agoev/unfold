//! Determinism: the canonical export of a compiled folder is identical whatever the number of
//! threads and whatever order the files are supplied in (architecture §6.8). Extraction,
//! linking (whose tables are built in parallel), and edge resolution all run on the rayon pool
//! the test installs.

use unfold_core::canonical;
use unfold_core::testkit::testdata_dir;
use unfold_engine::ProjectInputs;
use unfold_lang_typescript::TypeScript;

use crate::support::{compile_with, folder_inputs, sources};

/// The canonical export of `files` compiled on a pool of `threads` threads.
fn export(threads: usize, files: &[(String, Vec<u8>)], inputs: &ProjectInputs) -> Vec<u8> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a thread pool");
    let graph = pool.install(|| compile_with(&TypeScript::new(), files, inputs).graph);
    let mut out = Vec::new();
    canonical::write(&graph, &mut out).expect("write the export");
    out
}

/// `files` in a scrambled but reproducible order: every third file first, then the rest
/// reversed.
fn scrambled(files: &[(String, Vec<u8>)]) -> Vec<(String, Vec<u8>)> {
    let mut first: Vec<_> = files.iter().step_by(3).cloned().collect();
    let mut rest: Vec<_> = files
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 3 != 0)
        .map(|(_, file)| file.clone())
        .collect();
    rest.reverse();
    first.append(&mut rest);
    first
}

#[test]
fn exports_are_identical_across_threads_and_input_orders() {
    for folder in [
        "playgrounds/ts-orders",
        "fixtures/typescript-evidence/scripts",
        "fixtures/typescript-evidence/c2-objects",
        "fixtures/typed-lines/ts",
    ] {
        let root = testdata_dir().join(folder);
        let files = sources(&root);
        let inputs = folder_inputs(&root);
        let one = export(1, &files, &inputs);
        let mut reversed = files.clone();
        reversed.reverse();
        assert!(
            one == export(8, &reversed, &inputs),
            "{folder}: 8 threads, reversed"
        );
        assert!(
            one == export(3, &scrambled(&files), &inputs),
            "{folder}: 3 threads, scrambled"
        );
    }
}
