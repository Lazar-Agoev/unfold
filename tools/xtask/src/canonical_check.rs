//! `cargo xtask canonical-check <file.jsonl>`: the canonical export round trip, at the export's
//! own version (1 or 2).

use std::path::PathBuf;

use anyhow::{Context, bail};
use unfold_core::canonical;
use unfold_core::validate::Validator;

/// Arguments of `canonical-check`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The canonical export (JSON Lines) to check.
    file: PathBuf,
}

/// Reads the export, validates the graph's own invariants, writes it again, and compares bytes.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let bytes = std::fs::read(&args.file).with_context(|| format!("cannot read {}", args.file.display()))?;
    let text = String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", args.file.display()))?;
    let (graph, version) =
        canonical::read_versioned(&text).with_context(|| format!("cannot parse {}", args.file.display()))?;

    // A version-1 export predates typed records: it is validated and written back as one.
    let validator = if version < canonical::VERSION {
        Validator::new().without_typed_records()
    } else {
        Validator::new()
    };
    let violations = validator.validate(&graph);
    for violation in &violations {
        eprintln!("{violation}");
    }

    let mut rewritten = Vec::new();
    canonical::write_version(&graph, version, &mut rewritten)?;
    let rewritten = String::from_utf8(rewritten)?;
    let identical = rewritten == text;
    if !identical {
        let line = text.lines().zip(rewritten.lines()).position(|(a, b)| a != b);
        let line = line.unwrap_or_else(|| text.lines().count().min(rewritten.lines().count()));
        eprintln!(
            "round trip differs at line {}:\n  read:    {}\n  written: {}",
            line + 1,
            text.lines().nth(line).unwrap_or("<end of file>"),
            rewritten.lines().nth(line).unwrap_or("<end of file>"),
        );
    }

    if !violations.is_empty() || !identical {
        bail!(
            "{}: {} violation(s); round trip {}",
            args.file.display(),
            violations.len(),
            if identical { "identical" } else { "differs" }
        );
    }
    let blocks: usize = graph.files.iter().map(|file| file.blocks.len()).sum();
    let edges: usize = graph.files.iter().map(|file| file.edges.len()).sum();
    println!(
        "{}: valid, round trip identical ({} files, {} blocks, {} edges, {} entrances)",
        args.file.display(),
        graph.files.len(),
        blocks,
        edges,
        graph.entrances.len()
    );
    Ok(())
}
