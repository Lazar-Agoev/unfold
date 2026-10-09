//! Locations inside the Unfold repository.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

/// The repository root (xtask lives in `tools/xtask`).
pub(crate) fn root() -> &'static Path {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.ancestors().nth(2).unwrap_or(manifest_dir)
}

/// `testdata/`.
pub(crate) fn testdata_dir() -> PathBuf {
    root().join("testdata")
}

/// `testdata/reference/`.
pub(crate) fn reference_dir() -> PathBuf {
    testdata_dir().join("reference")
}

/// `path` relative to the repository root when it is inside it, for messages.
pub(crate) fn display(path: &Path) -> String {
    path.strip_prefix(root()).unwrap_or(path).display().to_string()
}

/// Writes the generated `text` to `path` unless the file already holds it; with `check`, fails
/// instead of writing when the file is missing or stale. `command` is the command that
/// regenerates the file, for the failure message. Returns whether the file was written.
pub(crate) fn check_or_write(path: &Path, text: &str, check: bool, command: &str) -> anyhow::Result<bool> {
    // Reading as bytes compares exactly, even when the file on disk is not UTF-8.
    let current = std::fs::read(path).ok();
    if current.as_deref() == Some(text.as_bytes()) {
        println!("{} is up to date", display(path));
        return Ok(false);
    }
    if check {
        bail!(
            "{} is stale: run `{command}` and commit the result",
            display(path)
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    println!("wrote {}", display(path));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_fails_on_stale_files_and_write_fixes_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("generated/index.ts");
        let command = "cargo xtask gen";
        assert!(
            check_or_write(&path, "export {};\n", true, command).is_err(),
            "missing file"
        );
        assert!(check_or_write(&path, "export {};\n", false, command).expect("write"));
        assert!(!check_or_write(&path, "export {};\n", true, command).expect("fresh"));
        assert!(
            check_or_write(&path, "export type A = 1;\n", true, command).is_err(),
            "stale file"
        );
    }
}
