//! `cargo xtask canonical-diff <expected.jsonl> <actual.jsonl>`: categorized differences between
//! two canonical exports, keyed by structural key (see [`crate::compare`]).
//!
//! Exits non-zero when the exports differ. `--json` prints only the machine-readable summary
//! (counts per record type and category).

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use unfold_core::{Graph, canonical};

use crate::compare::{self, Scope, Summary};

/// Arguments of `canonical-diff`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The expected export (for example a reference under testdata/reference/canonical).
    expected: PathBuf,
    /// The actual export.
    actual: PathBuf,
    /// The record types to compare (default: all); `entrances` includes their targets.
    #[arg(long, value_delimiter = ',', value_enum)]
    scope: Vec<Scope>,
    /// The most differences listed per record type and category (0 lists all).
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Print the summary as JSON instead of listing differences.
    #[arg(long)]
    json: bool,
}

/// Reads a canonical export.
pub(crate) fn read_graph(path: &Path) -> anyhow::Result<Graph> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    canonical::read(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// The scopes to compare: the given ones, or all.
pub(crate) fn scopes(given: &[Scope]) -> Vec<Scope> {
    if given.is_empty() {
        Scope::ALL.to_vec()
    } else {
        given.to_vec()
    }
}

/// Runs `canonical-diff`.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let expected = read_graph(&args.expected)?;
    let actual = read_graph(&args.actual)?;
    let differences = compare::diff(&expected, &actual, &scopes(&args.scope))?;
    let summary = Summary::of_differences(&differences);
    if args.json {
        println!("{}", summary.to_json());
    } else {
        let listed: Vec<_> = differences.iter().collect();
        print!("{}", compare::render_differences(&listed, args.limit));
        println!("differences: {} ({})", summary.total(), summary.line());
    }
    if !differences.is_empty() {
        bail!("{} and {} differ", args.expected.display(), args.actual.display());
    }
    Ok(())
}
