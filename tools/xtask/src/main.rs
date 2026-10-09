//! `cargo xtask`: developer and CI tasks for the Unfold repository. Never shipped.

#![forbid(unsafe_code)]
// A command-line tool reports on standard output and standard error.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod canonical_check;
mod canonical_diff;
mod compare;
mod compile;
mod export_store;
mod gen_ipc;
mod gen_lib_tables;
mod ledger;
mod notices;
mod oracles;
mod parity;
mod reference;
mod repo;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cargo xtask", about = "Developer tasks for the Unfold repository")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a canonical export, validate it, write it again, and fail unless the bytes are
    /// identical.
    CanonicalCheck(canonical_check::Args),
    /// Compare two canonical exports record by record, keyed by structural key.
    CanonicalDiff(canonical_diff::Args),
    /// Compile a folder headlessly and print what the Index log would show.
    Compile(compile::CompileArgs),
    /// Compile a folder headlessly and write its canonical export.
    Export(compile::ExportArgs),
    /// Write the canonical export of the graph a folder's workspace committed (read from its
    /// graph database), to compare background updates with a fresh `export`.
    ExportStore(export_store::Args),
    /// Write the command table's TypeScript types to ui/src/ipc/generated/ (`--check` fails when
    /// they are stale).
    GenIpc(gen_ipc::Args),
    /// Generate the TypeScript front end's tables of known globals, library types, and Node
    /// built-in modules from TypeScript's `lib.*.d.ts` files and `@types/node`.
    GenLibTables(gen_lib_tables::Args),
    /// Write THIRD_PARTY_NOTICES.md from the shipped Rust crates and npm packages (`--check`
    /// fails when it is stale).
    Notices(notices::Args),
    /// The precision and recall gates: compare compiled corpora with the cached TypeScript
    /// checker and Pyright oracle outputs (`check`), or regenerate them (`refresh`).
    Oracles(oracles::Args),
    /// Compile a reference corpus folder (or read `--actual`) and compare it with the reference
    /// and the divergence ledger.
    Parity(parity::Args),
    /// Work with the frozen proof-of-concept reference under testdata/reference.
    Reference(reference::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::CanonicalCheck(args) => canonical_check::run(&args),
        Command::CanonicalDiff(args) => canonical_diff::run(&args),
        Command::Compile(args) => compile::run_compile(&args),
        Command::Export(args) => compile::run_export(&args),
        Command::ExportStore(args) => export_store::run(&args),
        Command::GenIpc(args) => gen_ipc::run(&args),
        Command::GenLibTables(args) => gen_lib_tables::run(&args),
        Command::Notices(args) => notices::run(&args),
        Command::Oracles(args) => oracles::run(&args),
        Command::Parity(args) => parity::run(&args),
        Command::Reference(args) => reference::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
