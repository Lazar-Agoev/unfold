//! `testdata/reference/oracles/corpora.toml`: the corpora the oracle gates cover, where their
//! sources come from, and (for Python) which saved proof-of-concept Pyright rows describe them.
//! Lock corpora are pinned in `testdata/corpora.lock.toml` and found outside the repository.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use unfold_core::vocab::Language;
use unfold_core::{ContentHash, Graph};

use crate::compile::{compile_with, languages};
use crate::reference::corpora::PathMapping;
use crate::repo;

/// The environment variable naming a folder that holds lock corpora as `<lock id>/`.
pub(crate) const CORPORA_DIR_VAR: &str = "UNFOLD_CORPORA_DIR";

/// The parsed `corpora.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OracleCorpora {
    /// Every corpus, in file order.
    pub corpus: Vec<OracleCorpus>,
}

/// One corpus under the oracle gates.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OracleCorpus {
    /// The corpus name; its oracle files are `<name>.<language>.jsonl`.
    pub name: String,
    /// A folder under `testdata/`, for corpora kept in the repository.
    pub folder: Option<String>,
    /// A `testdata/corpora.lock.toml` id, for corpora pinned rather than copied.
    pub lock: Option<String>,
    /// The languages compiled and checked.
    pub languages: Vec<Language>,
    /// Whether the oracle files are committed under `testdata/reference/oracles/` (default
    /// true). Uncommitted ones live in `target/oracles/` and are regenerated where the corpus is.
    #[serde(default = "default_true")]
    pub committed: bool,
    /// `unfold.toml` text for a corpus that has none of its own (lock corpora).
    pub settings: Option<String>,
    /// For Python: the saved proof-of-concept Pyright rows this corpus's oracle comes from.
    pub pyright: Option<PyrightRows>,
}

/// Where a corpus's saved Pyright rows are and how their paths map.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PyrightRows {
    /// The rows file, relative to the proof-of-concept repository.
    pub rows: String,
    /// The `testdata/corpora.lock.toml` id whose `poc_source_sha256` the rows were computed for.
    pub lock: String,
    /// Prefix mappings from the rows' paths to the corpus folder's paths.
    pub paths: Vec<PathMapping>,
}

const fn default_true() -> bool {
    true
}

impl OracleCorpora {
    /// `testdata/reference/oracles/corpora.toml`.
    pub(crate) fn default_path() -> PathBuf {
        oracles_dir().join("corpora.toml")
    }

    /// Reads and checks the file at `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parses and checks `corpora.toml` text.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let corpora: Self = toml::from_str(text)?;
        let mut names = std::collections::BTreeSet::new();
        for corpus in &corpora.corpus {
            if !names.insert(corpus.name.as_str()) {
                bail!("corpus `{}` is listed twice", corpus.name);
            }
            if corpus.folder.is_some() == corpus.lock.is_some() {
                bail!(
                    "corpus `{}` needs exactly one of `folder` and `lock`",
                    corpus.name
                );
            }
            if corpus.languages.is_empty() {
                bail!("corpus `{}` lists no languages", corpus.name);
            }
            if corpus.languages.contains(&Language::Python) != corpus.pyright.is_some() {
                bail!(
                    "corpus `{}`: `pyright` rows go with exactly the Python corpora",
                    corpus.name
                );
            }
        }
        Ok(corpora)
    }

    /// The corpora named in `names` (all of them when empty), in file order.
    pub(crate) fn select(&self, names: &[String]) -> anyhow::Result<Vec<&OracleCorpus>> {
        if names.is_empty() {
            return Ok(self.corpus.iter().collect());
        }
        names
            .iter()
            .map(|name| {
                self.corpus
                    .iter()
                    .find(|corpus| &corpus.name == name)
                    .with_context(|| {
                        format!(
                            "no oracle corpus `{name}` in {}",
                            repo::display(&Self::default_path())
                        )
                    })
            })
            .collect()
    }
}

/// `testdata/reference/oracles/`.
pub(crate) fn oracles_dir() -> PathBuf {
    repo::reference_dir().join("oracles")
}

/// Where uncommitted oracle files go: `target/oracles/` at the repository root.
pub(crate) fn uncommitted_dir() -> PathBuf {
    repo::root().join("target").join("oracles")
}

impl OracleCorpus {
    /// The oracle file for `language`.
    pub(crate) fn oracle_path(&self, language: Language) -> PathBuf {
        let dir = if self.committed {
            oracles_dir()
        } else {
            uncommitted_dir()
        };
        dir.join(format!("{}.{}.jsonl", self.name, language.as_str()))
    }

    /// Where the corpus's sources are, or why they are not available here.
    pub(crate) fn locate(&self, lock: &Lock) -> Result<PathBuf, String> {
        if let Some(folder) = &self.folder {
            return Ok(repo::testdata_dir().join(folder));
        }
        let id = self.lock.as_deref().unwrap_or_default();
        let entry = lock
            .entry(id)
            .ok_or_else(|| format!("`{id}` is not in testdata/corpora.lock.toml"))?;
        if let Some(dir) = std::env::var_os(CORPORA_DIR_VAR) {
            let candidate = PathBuf::from(dir).join(id);
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
        if let (Some(repository), Some(local)) = (&lock.poc.repository, &entry.poc_local_path) {
            let candidate = Path::new(repository).join(local);
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
        Err(format!(
            "corpus `{}` is not available: put lock corpus `{id}` at ${CORPORA_DIR_VAR}/{id}",
            self.name
        ))
    }

    /// Copies the corpus into `into` (adding its settings when it has none) and compiles the
    /// copy with the corpus's languages. Nothing is written where the corpus lives.
    pub(crate) fn compile_copy(&self, source: &Path, into: &Path) -> anyhow::Result<Graph> {
        copy_corpus(source, into)?;
        if let Some(settings) = &self.settings {
            let config = into.join("unfold.toml");
            if config.exists() {
                bail!(
                    "corpus `{}` has its own unfold.toml and `settings` too",
                    self.name
                );
            }
            std::fs::write(&config, settings)
                .with_context(|| format!("cannot write {}", config.display()))?;
        }
        let selected = languages()
            .into_iter()
            .filter(|language| self.languages.contains(&language.id()))
            .collect();
        let outcome = compile_with(into, selected, 0)?;
        Ok(std::sync::Arc::unwrap_or_clone(outcome.graph))
    }

    /// Checks a lock corpus's compiled files against the lock's pins (`listing_sha256` or
    /// per-file `fingerprints`), so a different checkout is never measured by mistake.
    pub(crate) fn verify_pins(&self, lock: &Lock, graph: &Graph) -> anyhow::Result<()> {
        let Some(entry) = self.lock.as_deref().and_then(|id| lock.entry(id)) else {
            return Ok(());
        };
        let files: Vec<(&str, ContentHash)> = graph
            .files
            .iter()
            .filter(|file| self.languages.contains(&file.file.language))
            .map(|file| {
                file.file
                    .hash
                    .map(|hash| (file.file.path.as_str(), hash))
                    .with_context(|| format!("{} did not compile", file.file.path))
            })
            .collect::<anyhow::Result<_>>()?;
        if let Some(expected) = entry.listing_sha256.as_deref().filter(|value| !value.is_empty()) {
            let actual = listing_sha256(files.iter().copied());
            if actual != expected {
                bail!(
                    "corpus `{}` is not the pinned `{}`: its listing hash is {actual}, the lock pins {expected}",
                    self.name,
                    entry.id
                );
            }
        }
        if !entry.fingerprints.is_empty() {
            let actual: std::collections::BTreeMap<&str, String> = files
                .iter()
                .map(|(path, hash)| (*path, hash.to_string()))
                .collect();
            let expected: std::collections::BTreeMap<&str, String> = entry
                .fingerprints
                .iter()
                .map(|(path, hash)| (path.as_str(), hash.clone()))
                .collect();
            if actual != expected {
                bail!(
                    "corpus `{}` does not match the fingerprints pinned for `{}`",
                    self.name,
                    entry.id
                );
            }
        }
        Ok(())
    }
}

/// Copies `from` into `to` byte for byte, without `.unfold/local/` and without symbolic links.
/// Unfold never follows a linked folder, and a downloaded archive may hold links that point
/// outside it (the FastAPI template links an agent skills folder), so links are left behind.
fn copy_corpus(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    let entries = std::fs::read_dir(from).with_context(|| format!("cannot read {}", from.display()))?;
    for entry in entries {
        let entry = entry?;
        let kind = entry.file_type()?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            if entry.file_name() == "local" && from.file_name().is_some_and(|name| name == ".unfold") {
                continue;
            }
            copy_corpus(&source, &target)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &target).with_context(|| format!("cannot copy {}", source.display()))?;
        }
    }
    Ok(())
}

/// SHA-256 of a `sha256sum`-style listing: `"<hash>  <path>\n"` per file, sorted by path bytes.
pub(crate) fn listing_sha256<'a>(entries: impl IntoIterator<Item = (&'a str, ContentHash)>) -> String {
    let mut lines: Vec<(&str, ContentHash)> = entries.into_iter().collect();
    lines.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut text = String::new();
    for (path, hash) in lines {
        text.push_str(&format!("{hash}  {path}\n"));
    }
    ContentHash::of(text.as_bytes()).to_string()
}

/// The parts of `testdata/corpora.lock.toml` the oracles read.
#[derive(Debug, Deserialize)]
pub(crate) struct Lock {
    /// The proof of concept's local checkout.
    pub poc: LockPoc,
    /// Every pinned corpus.
    pub corpus: Vec<LockEntry>,
}

/// `[poc]` in the lock.
#[derive(Debug, Deserialize)]
pub(crate) struct LockPoc {
    /// The local proof-of-concept repository.
    pub repository: Option<String>,
    /// The commit its outputs were frozen at.
    pub commit: Option<String>,
}

/// One `[[corpus]]` in the lock.
#[derive(Debug, Deserialize)]
pub(crate) struct LockEntry {
    /// The corpus id.
    pub id: String,
    /// The corpus root inside the proof-of-concept repository, when it has one.
    pub poc_local_path: Option<String>,
    /// SHA-256 of the corpus listing.
    pub listing_sha256: Option<String>,
    /// The proof of concept's aggregate source hash.
    pub poc_source_sha256: Option<String>,
    /// Per-file SHA-256 pins.
    #[serde(default)]
    pub fingerprints: std::collections::BTreeMap<String, String>,
}

impl Lock {
    /// `testdata/corpora.lock.toml`.
    pub(crate) fn load() -> anyhow::Result<Self> {
        let path = repo::testdata_dir().join("corpora.lock.toml");
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
    }

    /// The entry with `id`.
    pub(crate) fn entry(&self, id: &str) -> Option<&LockEntry> {
        self.corpus.iter().find(|entry| entry.id == id)
    }
}
