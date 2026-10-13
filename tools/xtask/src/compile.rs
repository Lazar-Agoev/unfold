//! `cargo xtask compile <folder>` and `cargo xtask export <folder> [-o out.jsonl]`: a headless
//! compile of one folder with the real language front ends, through the engine's `update()`
//! and an in-memory facts cache. The engine reads the folder and writes nothing into it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use unfold_core::canonical;
use unfold_core::vocab::FileState;
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, Language, UpdateOutcome, UpdateRequest};

/// Arguments of `compile`.
#[derive(clap::Args)]
pub(crate) struct CompileArgs {
    /// The folder to compile.
    folder: PathBuf,
    /// Compile threads (default: every core but one).
    #[arg(long, default_value_t = 0)]
    threads: usize,
    /// Also list every failed or last-good file with its failure.
    #[arg(long)]
    failures: bool,
}

/// Arguments of `export`.
#[derive(clap::Args)]
pub(crate) struct ExportArgs {
    /// The folder to compile.
    folder: PathBuf,
    /// Where to write the canonical export (default: standard output).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// Every language front end Unfold ships, as the workspace registers them.
pub(crate) fn languages() -> Vec<Arc<dyn Language>> {
    vec![
        Arc::new(unfold_lang_typescript::TypeScript::new()),
        Arc::new(unfold_lang_python::Python::new()),
    ]
}

/// Compiles `folder` once, from nothing, with every language.
pub(crate) fn compile_folder(folder: &Path, threads: usize) -> anyhow::Result<UpdateOutcome> {
    compile_with(folder, languages(), threads)
}

/// Compiles `folder` once, from nothing, with `languages`.
pub(crate) fn compile_with(
    folder: &Path,
    languages: Vec<Arc<dyn Language>>,
    threads: usize,
) -> anyhow::Result<UpdateOutcome> {
    let options = EngineOptions {
        compile_threads: threads,
        ..EngineOptions::default()
    };
    let mut engine = Engine::new(folder, languages, Arc::new(MemoryFactsCache::new()), options)
        .with_context(|| format!("cannot open {}", folder.display()))?;
    engine
        .update(UpdateRequest::compile())
        .with_context(|| format!("compiling {} failed", folder.display()))
}

/// Runs `compile`: prints the update's statistics and any problems.
pub(crate) fn run_compile(args: &CompileArgs) -> anyhow::Result<()> {
    let outcome = compile_folder(&args.folder, args.threads)?;
    print!("{}", outcome.stats);
    println!(
        "graph: {}",
        if outcome.complete {
            "complete"
        } else {
            "incomplete"
        }
    );
    for problem in &outcome.problems {
        println!("problem: {problem}");
    }
    let failing: Vec<_> = outcome
        .graph
        .files
        .iter()
        .filter(|file| file.file.state != FileState::Current)
        .collect();
    if args.failures {
        for file in &failing {
            println!(
                "{} {}: {}",
                file.file.state,
                file.file.path,
                file.file.failure.as_deref().unwrap_or("")
            );
        }
    } else if !failing.is_empty() {
        println!("{} file(s) don't parse; --failures lists them", failing.len());
    }
    Ok(())
}

/// Runs `export`: writes the canonical export.
pub(crate) fn run_export(args: &ExportArgs) -> anyhow::Result<()> {
    let outcome = compile_folder(&args.folder, 0)?;
    match &args.output {
        Some(path) => {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(path).with_context(|| format!("cannot create {}", path.display()))?,
            );
            canonical::write(&outcome.graph, &mut file)?;
            file.flush()?;
            eprintln!(
                "wrote {} ({} files, {} entrances)",
                path.display(),
                outcome.graph.files.len(),
                outcome.graph.entrances.len()
            );
        }
        None => {
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            canonical::write(&outcome.graph, &mut out)?;
            out.flush()?;
        }
    }
    Ok(())
}
