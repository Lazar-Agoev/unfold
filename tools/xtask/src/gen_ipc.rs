//! `cargo xtask gen-ipc`: writes the TypeScript types of the command table (generated command types) to
//! `ui/src/ipc/generated/index.ts`; `--check` fails when that file is stale (CI).

use anyhow::{Result, anyhow};
use unfold_workspace::ipc::typescript::{OUTPUT_PATH, generate};

use crate::repo;

/// Options for `gen-ipc`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Fail if the generated file differs from what the Rust types produce, instead of writing.
    #[arg(long)]
    check: bool,
}

pub(crate) fn run(args: &Args) -> Result<()> {
    let text = generate().map_err(|error| anyhow!(error))?;
    repo::check_or_write(
        &repo::root().join(OUTPUT_PATH),
        &text,
        args.check,
        "cargo xtask gen-ipc",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_types_are_up_to_date() {
        let text = generate().expect("generate");
        repo::check_or_write(
            &repo::root().join(OUTPUT_PATH),
            &text,
            true,
            "cargo xtask gen-ipc",
        )
        .expect("run `cargo xtask gen-ipc` to regenerate ui/src/ipc/generated/index.ts");
    }
}
