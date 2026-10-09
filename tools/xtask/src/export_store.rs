//! `cargo xtask export-store <folder or graph.sqlite> [-o out.jsonl]`: the canonical export of the
//! graph a folder's workspace committed, read back from its database with
//! `unfold_store::Reader::graph()`. With `cargo xtask export` of an identical copy, the journeys
//! check that background updates ended equal to a fresh compile.
//!
//! The database is copied (with its write-ahead log) into a temporary folder first, so the live
//! file is never opened, even while the app has it open. A database written by another engine
//! version is refused rather than rebuilt.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use unfold_core::canonical;
use unfold_store::{Opened, Store, StoreConfig};
use unfold_workspace::{LOCAL_PATH, default_languages, store_version};

/// Arguments of `export-store`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// A compiled folder (its `.unfold/local/cache/graph.sqlite`) or a `graph.sqlite` file.
    source: PathBuf,
    /// Where to write the canonical export (default: standard output).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

/// The database file `source` names.
fn database(source: &Path) -> PathBuf {
    if source.is_dir() {
        source.join(LOCAL_PATH).join("cache").join("graph.sqlite")
    } else {
        source.to_path_buf()
    }
}

/// Runs `export-store`.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let live = database(&args.source);
    if !live.is_file() {
        bail!("no graph database at {}", live.display());
    }
    let scratch = tempfile::tempdir().context("cannot create a temporary folder")?;
    let copy = scratch.path().join("graph.sqlite");
    std::fs::copy(&live, &copy).with_context(|| format!("cannot copy {}", live.display()))?;
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", live.display()));
        if side.is_file() {
            std::fs::copy(&side, scratch.path().join(format!("graph.sqlite{suffix}")))
                .with_context(|| format!("cannot copy {}", side.display()))?;
        }
    }
    let config = StoreConfig::new(store_version(&default_languages()));
    let store = Store::open(&copy, &config).context("cannot open the copied database")?;
    if store.opened != Opened::Reused {
        bail!(
            "{} isn't a current graph for this engine ({:?})",
            live.display(),
            store.opened
        );
    }
    let graph = store.readers.get()?.graph()?;
    match &args.output {
        Some(path) => {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(path).with_context(|| format!("cannot create {}", path.display()))?,
            );
            canonical::write(&graph, &mut file)?;
            file.flush()?;
            eprintln!(
                "wrote {} ({} files, {} entrances)",
                path.display(),
                graph.files.len(),
                graph.entrances.len()
            );
        }
        None => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            canonical::write(&graph, &mut out)?;
            out.flush()?;
        }
    }
    Ok(())
}
