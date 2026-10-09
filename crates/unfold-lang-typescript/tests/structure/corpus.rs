//! The proof of concept's 602-file TypeScript corpus (TypeScript 6.0.3's `src`), read from the
//! local proof-of-concept checkout named in `testdata/corpora.lock.toml`. Ignored by default:
//! run with `cargo test -p unfold-lang-typescript --release -- --ignored`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use unfold_core::testkit::testdata_dir;
use unfold_core::{RepoPath, Snapshot, canonical};
use unfold_engine::{Engine, EngineOptions, MemoryFactsCache, UpdateRequest};
use unfold_lang_typescript::{ExtractOptions, TypeScript, extract};

use crate::support::{compile, compile_with, folder_inputs, sources};

/// The corpus root: the proof of concept's repository joined with the corpus's local path.
fn corpus_root() -> PathBuf {
    let lock =
        std::fs::read_to_string(testdata_dir().join("corpora.lock.toml")).expect("read corpora.lock.toml");
    let lock: toml::Table = lock.parse().expect("parse corpora.lock.toml");
    let repository = lock["poc"]["repository"].as_str().expect("poc.repository");
    let corpora = lock["corpus"].as_array().expect("corpus entries");
    let corpus = corpora
        .iter()
        .find(|corpus| corpus["id"].as_str() == Some("typescript-6.0.3-src"))
        .expect("the typescript-6.0.3-src corpus");
    let local = corpus["poc_local_path"].as_str().expect("poc_local_path");
    PathBuf::from(repository).join(local)
}

#[test]
#[ignore = "reads the proof of concept's 602-file corpus from its local checkout"]
fn every_corpus_file_parses_and_validates() {
    let root = corpus_root();
    let files = sources(&root);
    assert_eq!(files.len(), 602, "the corpus selection changed");
    let bytes: usize = files.iter().map(|(_, bytes)| bytes.len()).sum();

    let snapshots: Vec<Snapshot> = files
        .iter()
        .map(|(path, bytes)| Snapshot::new(RepoPath::new(path).expect("path"), bytes.clone()).expect("UTF-8"))
        .collect();
    let started = Instant::now();
    let mut failures = Vec::new();
    for snapshot in &snapshots {
        if let Err(failure) = extract(snapshot, ExtractOptions::default()) {
            failures.push(format!("{}: {failure}", snapshot.path()));
        }
    }
    let extraction = started.elapsed();
    eprintln!(
        "extracted {} files ({bytes} bytes) in {:.2?} on one thread; {} failures",
        files.len(),
        extraction,
        failures.len()
    );
    assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));

    let started = Instant::now();
    let compiled = compile(&TypeScript::new(), &files).valid();
    let blocks: usize = compiled.graph.files.iter().map(|file| file.blocks.len()).sum();
    let slots: usize = compiled.graph.files.iter().map(|file| file.slots.len()).sum();
    eprintln!(
        "compiled and validated {blocks} blocks and {slots} slots in {:.2?}",
        started.elapsed()
    );
}

#[test]
#[ignore = "reads the proof of concept's 602-file corpus from its local checkout"]
fn corpus_exports_are_identical_across_threads_and_input_orders() {
    let root = corpus_root();
    let files = sources(&root);
    let inputs = folder_inputs(&root);
    let export = |threads: usize, files: &[(String, Vec<u8>)]| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("a thread pool");
        let graph = pool.install(|| compile_with(&TypeScript::new(), files, &inputs).valid().graph);
        let mut out = Vec::new();
        canonical::write(&graph, &mut out).expect("write the export");
        out
    };
    let one = export(1, &files);
    let mut reversed = files;
    reversed.reverse();
    assert!(
        one == export(8, &reversed),
        "1 thread in order vs 8 threads reversed"
    );
}

/// Copies the files under `from` into `to`.
fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read a folder") {
        let entry = entry.expect("a folder entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            std::fs::create_dir_all(&target).expect("create a folder");
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

/// The relink budget of a one-file edit: compiles a copy of the corpus through the engine, then
/// edits one file several times and prints each update's stages. Run optimized:
/// `cargo test -p unfold-lang-typescript --release --test structure corpus_update -- --ignored --nocapture`.
#[test]
#[ignore = "a benchmark over the proof of concept's 602-file corpus"]
fn corpus_update_latency() {
    let folder = tempfile::tempdir().expect("a temporary directory");
    copy_tree(&corpus_root(), folder.path());
    let mut engine = Engine::new(
        folder.path(),
        vec![Arc::new(TypeScript::new())],
        Arc::new(MemoryFactsCache::new()),
        EngineOptions::default(),
    )
    .expect("an engine");
    let compiled = engine.update(UpdateRequest::compile()).expect("the compile");
    eprintln!("compile:\n{}", compiled.stats);
    let edited = "compiler/utilities.ts";
    let original = std::fs::read_to_string(folder.path().join(edited)).expect("read the edited file");
    let mut totals: Vec<Duration> = Vec::new();
    for round in 0..7 {
        std::fs::write(
            folder.path().join(edited),
            format!("{original}\nexport function edited{round}() {{}}\n"),
        )
        .expect("edit the file");
        let path = RepoPath::new(edited).expect("a path");
        let outcome = engine
            .update(UpdateRequest::background(vec![path]).with_verify(unfold_engine::Verify::Contents))
            .expect("the update");
        assert!(outcome.stats.relinked);
        eprintln!(
            "update {round}: total {:.1?}; {:?}",
            outcome.stats.total, outcome.stats.stages
        );
        totals.push(outcome.stats.total);
    }
    totals.sort();
    eprintln!("median one-file update: {:.1?}", totals[totals.len() / 2]);
}
