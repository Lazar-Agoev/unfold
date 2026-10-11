//! `cargo xtask reference import`: converts the frozen PoC generations under
//! `testdata/reference/poc/` into Unfold's canonical export under
//! `testdata/reference/canonical/`, one file per corpus in `testdata/reference/corpora.toml`.
//!
//! The conversion is deterministic: importing again gives identical bytes (`--check` verifies
//! that the committed files are up to date without writing).

pub(crate) mod convert;
pub(crate) mod corpora;
mod poc;
mod settings;

use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Subcommand;
use unfold_core::canonical;

use self::convert::{Counts, FileSource};
use self::corpora::{Corpora, Corpus};
use self::poc::Generation;
use crate::repo;

/// Arguments of `reference`.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert every corpus in testdata/reference/corpora.toml into canonical/<corpus>.jsonl.
    Import(ImportArgs),
}

#[derive(clap::Args)]
struct ImportArgs {
    /// The PoC repository (graph-compiler), read only for files the testdata folders lack.
    #[arg(long)]
    poc: Option<PathBuf>,
    /// Fail if any canonical file is out of date instead of writing it.
    #[arg(long)]
    check: bool,
    /// Import only these corpora (default: all).
    corpora: Vec<String>,
}

/// Runs `reference <command>`.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    match &args.command {
        Command::Import(import) => run_import(import),
    }
}

fn run_import(args: &ImportArgs) -> anyhow::Result<()> {
    let corpora = Corpora::load(&Corpora::default_path())?;
    let selected: Vec<&Corpus> = if args.corpora.is_empty() {
        corpora.corpus.iter().collect()
    } else {
        args.corpora
            .iter()
            .map(|name| corpora.get(name))
            .collect::<anyhow::Result<_>>()?
    };
    let mut stale = Vec::new();
    for corpus in selected {
        let (text, counts) =
            import(corpus, args.poc.as_deref()).with_context(|| format!("corpus `{}`", corpus.name))?;
        let output = corpus.canonical_path();
        let current = std::fs::read(&output).ok();
        let unchanged = current.as_deref() == Some(text.as_bytes());
        if args.check {
            if !unchanged {
                stale.push(corpus.name.as_str());
            }
        } else if !unchanged {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            std::fs::write(&output, &text).with_context(|| format!("cannot write {}", output.display()))?;
        }
        println!(
            "{}: {} files, {} blocks, {} slots, {} edges, {} inputs, {} entrances, {} targets -> {} ({})",
            corpus.name,
            counts.files,
            counts.blocks,
            counts.slots,
            counts.edges,
            counts.inputs,
            counts.entrances,
            counts.targets,
            repo::display(&output),
            match (unchanged, args.check) {
                (true, _) => "unchanged",
                (false, true) => "out of date",
                (false, false) => "written",
            }
        );
    }
    if !stale.is_empty() {
        bail!(
            "out of date: {}; run `cargo xtask reference import`",
            stale.join(", ")
        );
    }
    Ok(())
}

/// Converts one corpus and returns its canonical export (version 1) and record counts.
pub(crate) fn import(corpus: &Corpus, poc: Option<&std::path::Path>) -> anyhow::Result<(String, Counts)> {
    let generation = Generation::read(&corpus.raw_dir())?;
    let source = FileSource {
        folder: corpus.folder_dir(),
        poc_root: poc.map(|poc| poc.join(&corpus.poc_root)),
    };
    let converted = convert::convert(corpus, &generation, &source)?;
    // The references stay version 1: the proof of concept has no typed records.
    let mut text = Vec::new();
    canonical::write_version(&converted.graph, 1, &mut text)?;
    Ok((String::from_utf8(text)?, converted.counts))
}

#[cfg(test)]
mod tests;
