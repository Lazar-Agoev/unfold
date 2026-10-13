//! `cargo xtask oracles check|refresh`: the precision and recall gates
//! ([docs/testing.md#choose-checks-for-a-local-change](../../../../docs/testing.md#choose-checks-for-a-local-change),
//! [testing policy](../../../../docs/testing.md#precision-and-recall-gates)).
//!
//! - `check` compiles each corpus in `testdata/reference/oracles/corpora.toml` from a temporary
//!   copy (or reads `--actual`) and compares it with the cached oracle files: every resolved call,
//!   construction, and named reference must name the oracle's target unless `justified.toml`
//!   explains why not, and recall must stay at or above `floors.toml`. It needs no Node: the
//!   oracle files are cached, keyed by a hash of the corpus bytes.
//! - `refresh` regenerates the oracle files: TypeScript through the developer-only Node script
//!   `tools/oracles/typescript-oracle.mjs` (the published `typescript` package), Python by
//!   converting the proof of concept's saved Pyright rows ([`python`]).

mod compare;
mod corpora;
mod file;
mod floors;
mod justified;
mod python;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{Context, bail};
use clap::Subcommand;
use unfold_core::vocab::Language;
use unfold_core::{Graph, canonical};

use self::compare::{Comparison, compare};
use self::corpora::{Lock, OracleCorpora, OracleCorpus};
use self::file::{OracleFile, corpus_sha256};
use self::floors::Floors;
use self::justified::Justified;
use crate::repo;

/// Arguments of `oracles`.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare compiled corpora with the cached oracle outputs (precision and recall gates).
    Check(CheckArgs),
    /// Regenerate the oracle outputs (TypeScript needs Node and `pnpm install --filter
    /// @unfold/oracles`; Python reads the proof of concept's saved Pyright rows).
    Refresh(RefreshArgs),
}

#[derive(clap::Args)]
struct CheckArgs {
    /// Corpora to check (default: every corpus; lock corpora that are not available are skipped).
    corpora: Vec<String>,
    /// A canonical export to check instead of compiling (exactly one corpus).
    #[arg(long)]
    actual: Option<PathBuf>,
    /// Fail instead of skipping a corpus whose sources are not available.
    #[arg(long)]
    strict: bool,
    /// The most disagreements listed per corpus (0 lists all).
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(clap::Args)]
struct RefreshArgs {
    /// Corpora to refresh (default: every available corpus).
    corpora: Vec<String>,
    /// Refresh only this language's oracle files.
    #[arg(long, value_parser = ["typescript", "python"])]
    language: Option<String>,
    /// The proof-of-concept repository holding the saved Pyright rows (default: the lock's).
    #[arg(long)]
    poc: Option<PathBuf>,
}

/// Runs `oracles <command>`.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    match &args.command {
        Command::Check(check) => run_check(check),
        Command::Refresh(refresh) => run_refresh(refresh),
    }
}

fn run_check(args: &CheckArgs) -> anyhow::Result<()> {
    let corpora = OracleCorpora::load(&OracleCorpora::default_path())?;
    let selected = corpora.select(&args.corpora)?;
    if args.actual.is_some() && selected.len() != 1 {
        bail!("--actual checks exactly one corpus");
    }
    let lock = Lock::load()?;
    let justified = Justified::load(&Justified::default_path())?;
    let floors = Floors::load(&Floors::default_path())?;
    let mut failures = Vec::new();
    for corpus in selected {
        let source = match &args.actual {
            Some(_) => None,
            None => match corpus.locate(&lock) {
                Ok(source) => Some(source),
                Err(why) if corpus.lock.is_some() && !args.strict && args.corpora.is_empty() => {
                    println!("skipped {}: {why}", corpus.name);
                    continue;
                }
                Err(why) => {
                    failures.push(why);
                    continue;
                }
            },
        };
        if let Err(error) = check_corpus(
            corpus,
            source.as_deref(),
            args,
            &lock,
            &justified,
            &floors,
            &mut failures,
        ) {
            failures.push(format!("{}: {error:#}", corpus.name));
        }
    }
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("FAILED {failure}");
        }
        bail!("{} oracle gate failure(s)", failures.len());
    }
    Ok(())
}

/// Reads a canonical export.
fn read_graph(path: &Path) -> anyhow::Result<Graph> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    canonical::read(&text).with_context(|| format!("cannot parse {}", path.display()))
}

fn check_corpus(
    corpus: &OracleCorpus,
    source: Option<&Path>,
    args: &CheckArgs,
    lock: &Lock,
    justified: &Justified,
    floors: &Floors,
    failures: &mut Vec<String>,
) -> anyhow::Result<()> {
    let graph = match (source, &args.actual) {
        (_, Some(actual)) => read_graph(actual)?,
        (Some(source), None) => {
            let copy = tempfile::tempdir().context("cannot create a temporary folder")?;
            let graph = corpus.compile_copy(source, copy.path())?;
            corpus.verify_pins(lock, &graph)?;
            graph
        }
        (None, None) => bail!("no sources and no --actual"),
    };
    for &language in &corpus.languages {
        let path = corpus.oracle_path(language);
        if !path.is_file() {
            failures.push(format!(
                "{} ({language}): no oracle file {}: run `cargo xtask oracles refresh {}`",
                corpus.name,
                repo::display(&path),
                corpus.name
            ));
            continue;
        }
        let oracle = OracleFile::load(&path)?;
        if oracle.header.corpus != corpus.name || oracle.header.language != language {
            bail!(
                "{} describes {} ({}), not {} ({language})",
                repo::display(&path),
                oracle.header.corpus,
                oracle.header.language,
                corpus.name
            );
        }
        let hash = corpus_sha256(&graph, language)?;
        if hash != oracle.header.corpus_sha256 {
            failures.push(format!(
                "{} ({language}): stale oracle — the corpus changed since {} was generated; rerun `cargo xtask oracles refresh {}`",
                corpus.name,
                repo::display(&path),
                corpus.name
            ));
            continue;
        }
        let comparison = compare(&corpus.name, language, &graph, &oracle, justified);
        let floor = floors.get(&corpus.name, language);
        report(corpus, language, &oracle, &comparison, floor, args.limit);
        if !comparison.precise() {
            failures.push(format!(
                "{} ({language}): precision: {} unjustified disagreement(s), {} stale justification(s)",
                corpus.name,
                comparison.unjustified.len(),
                comparison.stale.len()
            ));
        }
        match floor {
            None => failures.push(format!(
                "{} ({language}): no recall floor in {}; record matched = {}, resolved = {}",
                corpus.name,
                repo::display(&Floors::default_path()),
                comparison.recall.part,
                comparison.recall.total
            )),
            Some(floor) if !comparison.recall.at_least(floor.ratio()) => failures.push(format!(
                "{} ({language}): recall {} is below the floor {}",
                corpus.name,
                comparison.recall,
                floor.ratio()
            )),
            Some(_) => {}
        }
    }
    Ok(())
}

fn report(
    corpus: &OracleCorpus,
    language: Language,
    oracle: &OracleFile,
    comparison: &Comparison,
    floor: Option<&floors::Floor>,
    limit: usize,
) {
    println!(
        "{} {language} ({} {}): precision {} ({} justified; agreeing calls {}, references {}); recall {}{}; reference recall {}",
        corpus.name,
        oracle.header.oracle,
        oracle.header.oracle_version,
        comparison.precision(),
        comparison.justified.len(),
        comparison.calls,
        comparison.references,
        comparison.recall,
        floor.map_or_else(String::new, |floor| match &floor.poc {
            Some(poc) => format!(", floor {} (proof of concept: {poc})", floor.ratio()),
            None => format!(", floor {}", floor.ratio()),
        }),
        comparison.reference_recall,
    );
    if let Some(floor) = floor
        && comparison.recall.above(floor.ratio())
    {
        println!(
            "  recall is above the floor: raise it in {} to matched = {}, resolved = {}",
            repo::display(&Floors::default_path()),
            comparison.recall.part,
            comparison.recall.total
        );
    }
    for (disagreement, reason) in &comparison.justified {
        println!("  justified: {disagreement}\n    because: {reason}");
    }
    let shown = if limit == 0 {
        comparison.unjustified.len()
    } else {
        limit.min(comparison.unjustified.len())
    };
    for disagreement in &comparison.unjustified[..shown] {
        println!("  DISAGREES: {disagreement}");
    }
    if shown < comparison.unjustified.len() {
        println!("  ... and {} more", comparison.unjustified.len() - shown);
    }
    for entry in &comparison.stale {
        println!(
            "  STALE justification (no longer occurs): {} -> {}",
            entry.site, entry.target
        );
    }
}

fn run_refresh(args: &RefreshArgs) -> anyhow::Result<()> {
    let corpora = OracleCorpora::load(&OracleCorpora::default_path())?;
    let selected = corpora.select(&args.corpora)?;
    let lock = Lock::load()?;
    let wanted = |language: Language| {
        args.language
            .as_deref()
            .is_none_or(|name| name == language.as_str())
    };
    for corpus in selected {
        let source = match corpus.locate(&lock) {
            Ok(source) => source,
            Err(why) if args.corpora.is_empty() => {
                println!("skipped {}: {why}", corpus.name);
                continue;
            }
            Err(why) => bail!(why),
        };
        let work = tempfile::tempdir().context("cannot create a temporary folder")?;
        let copy = work.path().join("corpus");
        let graph = corpus.compile_copy(&source, &copy)?;
        corpus.verify_pins(&lock, &graph)?;
        for &language in corpus.languages.iter().filter(|&&language| wanted(language)) {
            let out = corpus.oracle_path(language);
            match language {
                Language::TypeScript => refresh_typescript(corpus, &copy, &graph, work.path(), &out)?,
                Language::Python => {
                    let rows = corpus
                        .pyright
                        .as_ref()
                        .context("a Python corpus without `pyright` rows")?;
                    let poc = match &args.poc {
                        Some(poc) => poc.clone(),
                        None => PathBuf::from(
                            lock.poc
                                .repository
                                .as_deref()
                                .context("the lock names no PoC repository; pass --poc")?,
                        ),
                    };
                    let oracle = python::import(corpus, rows, &poc, &lock, &copy, &graph)?;
                    repo::check_or_write(&out, &oracle.render()?, false, "cargo xtask oracles refresh")?;
                }
            }
            let oracle = OracleFile::load(&out)?;
            println!(
                "{} {language}: {} call sites, {} resolved by the oracle, {} named-argument targets",
                corpus.name, oracle.header.sites, oracle.header.resolved, oracle.header.references
            );
        }
    }
    Ok(())
}

/// Runs the Node checker oracle over the compiled copy and its canonical export.
fn refresh_typescript(
    corpus: &OracleCorpus,
    copy: &Path,
    graph: &Graph,
    work: &Path,
    out: &Path,
) -> anyhow::Result<()> {
    let oracles = repo::root().join("tools").join("oracles");
    if !oracles.join("node_modules").join("typescript").is_dir() {
        bail!("the TypeScript oracle needs its packages: run `pnpm install --filter @unfold/oracles`");
    }
    let export = work.join("export.jsonl");
    let mut writer = std::io::BufWriter::new(
        std::fs::File::create(&export).with_context(|| format!("cannot create {}", export.display()))?,
    );
    canonical::write(graph, &mut writer)?;
    std::io::Write::flush(&mut writer)?;
    drop(writer);
    let status = Process::new("node")
        .arg("--max-old-space-size=8192")
        .arg(oracles.join("typescript-oracle.mjs"))
        .arg("--folder")
        .arg(copy)
        .arg("--export")
        .arg(&export)
        .arg("--corpus")
        .arg(&corpus.name)
        .arg("--out")
        .arg(out)
        .status()
        .context("cannot run `node` (the TypeScript oracle is a developer-only Node tool)")?;
    if !status.success() {
        bail!("the TypeScript oracle failed for `{}` ({status})", corpus.name);
    }
    Ok(())
}
