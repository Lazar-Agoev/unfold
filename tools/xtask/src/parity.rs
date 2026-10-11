//! `cargo xtask parity <corpus> [--actual <file.jsonl>]`: the reference gate
//! ([architecture §16](../../../docs/architecture.md#16-testing)). Compares an actual canonical
//! export with `testdata/reference/canonical/<corpus>.jsonl`, then checks the differences
//! against the divergence ledger ([`crate::ledger`]): it fails on any unlisted difference and on
//! any stale ledger entry.
//!
//! # The compile step
//!
//! Without `--actual`, the command compiles the corpus folder itself:
//!
//! 1. [`Corpus::folder_dir`] is copied into a `tempfile` directory ([`copy_folder`]). Nothing
//!    compiles in place, so nothing but blessing ever writes into `testdata/`.
//! 2. The copy is compiled with the engine's `update()`, registering the language crates
//!    `Corpus::languages` names. The folder carries the corpus's settings
//!    (`.unfold/config.toml`), so no other options are needed.
//! 3. The resulting graph goes to [`check_differences`] and [`Ledger::check`]. Paths in the
//!    graph are relative to the folder, which is what the reference uses.
//!
//! `--actual <file.jsonl>` checks a canonical export produced elsewhere instead.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use serde_json::json;
use unfold_core::{Graph, canonical};

use crate::canonical_diff::{read_graph, scopes};
use crate::compare::{self, Scope, Summary};
use crate::compile::{compile_with, copy_folder, languages};
use crate::ledger::{self, Ledger, Outcome};
use crate::reference::corpora::{Corpora, Corpus};

/// Arguments of `parity`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The corpus name from testdata/reference/corpora.toml.
    corpus: String,
    /// A canonical export to check instead of compiling the corpus folder.
    #[arg(long)]
    actual: Option<PathBuf>,
    /// The record types to compare (default: all); ledger entries outside the scope are ignored.
    #[arg(long, value_delimiter = ',', value_enum)]
    scope: Vec<Scope>,
    /// The divergence ledger (default: testdata/reference/divergences.toml).
    #[arg(long)]
    ledger: Option<PathBuf>,
    /// The most unlisted differences listed (0 lists all).
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Print a JSON summary instead of the human report.
    #[arg(long)]
    json: bool,
}

/// Runs `parity`.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let corpora = Corpora::load(&Corpora::default_path())?;
    let corpus = corpora.get(&args.corpus)?;
    let names: BTreeSet<&str> = corpora.corpus.iter().map(|corpus| corpus.name.as_str()).collect();
    let ledger = Ledger::load(&args.ledger.clone().unwrap_or_else(Ledger::default_path), &names)?;
    let actual = match &args.actual {
        Some(path) => read_graph(path)?,
        None => compile_corpus(corpus)?,
    };
    let scopes = scopes(&args.scope);
    let expected = read_graph(&corpus.canonical_path())
        .context("run `cargo xtask reference import` to create the reference")?;
    let differences = check_differences(corpus, &expected, &actual, &scopes)?;
    let outcome = ledger.check(&corpus.name, &scopes, &differences);
    report(corpus, &differences, &outcome, args);
    if !outcome.passed() {
        bail!(
            "parity failed for `{}`: {} unlisted difference(s), {} stale divergence(s)",
            corpus.name,
            outcome.unlisted.len(),
            outcome.stale.len()
        );
    }
    Ok(())
}

/// Compiles a temporary copy of the corpus folder with the languages it indexes.
fn compile_corpus(corpus: &Corpus) -> anyhow::Result<Graph> {
    let copy = tempfile::tempdir().context("cannot create a temporary folder")?;
    copy_folder(&corpus.folder_dir(), copy.path())?;
    let languages = languages()
        .into_iter()
        .filter(|language| corpus.languages.contains(&language.id()))
        .collect();
    let outcome = compile_with(copy.path(), languages, 0)?;
    if !outcome.complete {
        eprintln!(
            "note: the compiled graph is incomplete ({} problem(s))",
            outcome.problems.len()
        );
    }
    Ok(Arc::unwrap_or_clone(outcome.graph))
}

/// The differences between the corpus reference and `actual`'s version-1 view
/// ([`canonical::version_one`]: no typed records or type declaration blocks). Fails for a corpus
/// that is only a comparison aid.
pub(crate) fn check_differences(
    corpus: &Corpus,
    expected: &Graph,
    actual: &Graph,
    scopes: &[Scope],
) -> anyhow::Result<Vec<compare::Difference>> {
    if !corpus.parity {
        bail!(
            "corpus `{}` is a comparison aid (parity = false), not a parity target; use `cargo xtask canonical-diff`",
            corpus.name
        );
    }
    // The references are version 1: compare only what they record.
    compare::diff(expected, &canonical::version_one(actual), scopes)
}

fn report(corpus: &Corpus, differences: &[compare::Difference], outcome: &Outcome<'_, '_>, args: &Args) {
    let summary = Summary::of_differences(differences);
    let unlisted = Summary::of(
        outcome
            .unlisted
            .iter()
            .map(|unit| (unit.difference.id.record, unit.difference.change.category())),
    );
    if args.json {
        let value = json!({
            "corpus": corpus.name,
            "passed": outcome.passed(),
            "differences": summary.to_json(),
            "listed": outcome.listed.len(),
            "unlisted": unlisted.to_json(),
            "stale": outcome.stale.len(),
        });
        println!("{value}");
        return;
    }
    println!(
        "parity {}: {} differing record(s) ({}); {} listed, {} unlisted, {} stale",
        corpus.name,
        summary.total(),
        summary.line(),
        outcome.listed.len(),
        outcome.unlisted.len(),
        outcome.stale.len()
    );
    if !outcome.unlisted.is_empty() {
        println!("unlisted differences:");
        let shown = if args.limit == 0 {
            outcome.unlisted.len()
        } else {
            args.limit.min(outcome.unlisted.len())
        };
        for unit in &outcome.unlisted[..shown] {
            println!("  {}", ledger::describe_unit(unit));
        }
        if shown < outcome.unlisted.len() {
            println!("  ... and {} more", outcome.unlisted.len() - shown);
        }
    }
    if !outcome.stale.is_empty() {
        println!("stale divergences (no longer occur):");
        for (number, entry) in &outcome.stale {
            println!("  {}", ledger::describe_entry(*number, entry));
        }
    }
}
