//! `testdata/reference/corpora.toml`: which frozen generation describes which testdata folder,
//! and how its paths map onto that folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use unfold_core::vocab::Language;

use crate::repo;

/// The parsed `corpora.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Corpora {
    /// Every corpus, in file order.
    pub corpus: Vec<Corpus>,
}

/// One reference corpus.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Corpus {
    /// The corpus name; the reference is `canonical/<name>.jsonl`.
    pub name: String,
    /// The raw generation, relative to `testdata/reference/`.
    pub raw: String,
    /// The folder Unfold opens, relative to `testdata/`.
    pub folder: String,
    /// Where the PoC built the generation, relative to the PoC repository root.
    pub poc_root: String,
    /// The languages the generation indexes.
    pub languages: Vec<Language>,
    /// Whether Unfold is expected to reproduce this corpus.
    #[serde(default = "default_true")]
    pub parity: bool,
    /// Prefix mappings from PoC paths to folder paths.
    pub paths: Vec<PathMapping>,
    /// PoC settings files that became the folder's `unfold.toml`.
    #[serde(default)]
    pub settings: Vec<SettingsSource>,
}

/// One path-prefix mapping.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathMapping {
    /// The PoC prefix (empty matches every path).
    pub poc: String,
    /// What replaces it.
    pub unfold: String,
}

/// One PoC `codegraph.json` that became part of `unfold.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsSource {
    /// The PoC path of the `codegraph.json`.
    pub poc: String,
    /// The index of the `[[entrance]]` table holding its first manual entrance.
    pub first_entrance: usize,
}

const fn default_true() -> bool {
    true
}

impl Corpora {
    /// The default location, `testdata/reference/corpora.toml`.
    pub(crate) fn default_path() -> PathBuf {
        repo::reference_dir().join("corpora.toml")
    }

    /// Reads and checks the file at `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parses and checks corpora from TOML text.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let corpora: Self = toml::from_str(text)?;
        let mut names = std::collections::BTreeSet::new();
        for corpus in &corpora.corpus {
            if corpus.name.is_empty() || corpus.name.contains(['/', '\\']) {
                bail!("corpus name `{}` must be a nonempty file name", corpus.name);
            }
            if !names.insert(corpus.name.as_str()) {
                bail!("corpus `{}` is listed twice", corpus.name);
            }
            if corpus.paths.is_empty() {
                bail!("corpus `{}` has no path mappings", corpus.name);
            }
            if corpus.languages.is_empty() {
                bail!("corpus `{}` lists no languages", corpus.name);
            }
        }
        Ok(corpora)
    }

    /// The corpus called `name`.
    pub(crate) fn get(&self, name: &str) -> anyhow::Result<&Corpus> {
        self.corpus
            .iter()
            .find(|corpus| corpus.name == name)
            .with_context(|| {
                let known: Vec<&str> = self.corpus.iter().map(|corpus| corpus.name.as_str()).collect();
                format!("unknown corpus `{name}`; known corpora: {}", known.join(", "))
            })
    }
}

impl Corpus {
    /// The raw generation's directory.
    pub(crate) fn raw_dir(&self) -> PathBuf {
        repo::reference_dir().join(&self.raw)
    }

    /// The testdata folder Unfold opens.
    pub(crate) fn folder_dir(&self) -> PathBuf {
        repo::testdata_dir().join(&self.folder)
    }

    /// The canonical reference file.
    pub(crate) fn canonical_path(&self) -> PathBuf {
        repo::reference_dir()
            .join("canonical")
            .join(format!("{}.jsonl", self.name))
    }

    /// The folder path for a PoC path, using the longest matching prefix.
    pub(crate) fn map_path(&self, poc_path: &str) -> Option<String> {
        self.paths
            .iter()
            .filter(|mapping| poc_path.starts_with(mapping.poc.as_str()))
            .max_by_key(|mapping| mapping.poc.len())
            .map(|mapping| format!("{}{}", mapping.unfold, &poc_path[mapping.poc.len()..]))
    }

    /// The settings source a PoC path names, if it is one of the replaced `codegraph.json` files.
    pub(crate) fn settings_source(&self, poc_path: &str) -> Option<&SettingsSource> {
        self.settings.iter().find(|source| source.poc == poc_path)
    }
}

impl SettingsSource {
    /// The directory the settings file's locators are relative to, with a trailing `/` (or
    /// empty for the root).
    pub(crate) fn locator_base(&self) -> &str {
        self.poc.rfind('/').map_or("", |slash| &self.poc[..=slash])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [[corpus]]
        name = "mixed"
        raw = "poc/mixed"
        folder = "playgrounds"
        poc_root = "."
        languages = ["typescript", "python"]
        paths = [
          { poc = "ts-playground/", unfold = "ts-orders/" },
          { poc = "ts-playground/src/special/", unfold = "elsewhere/" },
          { poc = "py-playground/", unfold = "py-orders/" },
        ]
        settings = [{ poc = "py-playground/codegraph.json", first_entrance = 5 }]
    "#;

    #[test]
    fn maps_paths_by_longest_prefix() {
        let corpora = Corpora::parse(SAMPLE).unwrap();
        let corpus = corpora.get("mixed").unwrap();
        assert!(corpus.parity);
        assert_eq!(
            corpus.map_path("ts-playground/src/a.ts").unwrap(),
            "ts-orders/src/a.ts"
        );
        assert_eq!(
            corpus.map_path("ts-playground/src/special/b.ts").unwrap(),
            "elsewhere/b.ts"
        );
        assert_eq!(
            corpus.map_path("py-playground/pyproject.toml").unwrap(),
            "py-orders/pyproject.toml"
        );
        assert!(corpus.map_path("other/c.ts").is_none());
        let settings = corpus.settings_source("py-playground/codegraph.json").unwrap();
        assert_eq!(settings.locator_base(), "py-playground/");
        assert!(corpora.get("absent").is_err());
    }

    #[test]
    fn rejects_duplicates_and_unknown_fields() {
        let twice = format!("{SAMPLE}\n{}", SAMPLE.replace("[[corpus]]", "[[corpus]]\n"));
        assert!(Corpora::parse(&twice).is_err());
        assert!(Corpora::parse(&SAMPLE.replace("poc_root", "root")).is_err());
    }

    #[test]
    fn the_repository_file_is_valid() {
        let corpora = Corpora::load(&Corpora::default_path()).unwrap();
        for corpus in &corpora.corpus {
            assert!(
                corpus.raw_dir().join("manifest.json").is_file(),
                "{}",
                corpus.name
            );
            assert!(corpus.folder_dir().is_dir(), "{}", corpus.name);
        }
    }
}
