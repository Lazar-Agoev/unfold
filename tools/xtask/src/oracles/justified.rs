//! `testdata/reference/oracles/justified.toml`: resolved edges the oracle does not confirm, each
//! with a written reason (such as decision C-8). An entry covers exactly one edge; an entry that
//! covers nothing is stale and fails the check, like a stale divergence.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use unfold_core::vocab::Language;

use super::compare::Disagreement;
use super::corpora::oracles_dir;

/// The parsed file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Justified {
    /// Every entry, in file order.
    #[serde(default)]
    pub justified: Vec<JustifiedEntry>,
}

/// One justified disagreement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JustifiedEntry {
    /// The oracle corpus.
    pub corpus: String,
    /// The language of the site.
    pub language: Language,
    /// The call or construction block.
    pub site: String,
    /// The argument position, for a reference.
    pub position: Option<u32>,
    /// The target Unfold resolves.
    pub target: String,
    /// Why Unfold's answer is right although the oracle does not give it.
    pub reason: Box<str>,
}

impl Justified {
    /// `testdata/reference/oracles/justified.toml`.
    pub(crate) fn default_path() -> PathBuf {
        oracles_dir().join("justified.toml")
    }

    /// Reads and checks the file at `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parses and checks the text: reasons are written and no edge is listed twice.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let justified: Self = toml::from_str(text)?;
        let mut seen = BTreeSet::new();
        for entry in &justified.justified {
            if entry.reason.trim().is_empty() {
                bail!("the entry for {} has no reason", entry.site);
            }
            if !seen.insert((&entry.corpus, entry.language, &entry.site, entry.position)) {
                bail!("{} (corpus `{}`) is justified twice", entry.site, entry.corpus);
            }
        }
        Ok(justified)
    }

    /// The entry covering `disagreement`, with its index.
    pub(crate) fn find(
        &self,
        corpus: &str,
        language: Language,
        disagreement: &Disagreement,
    ) -> Option<(usize, &JustifiedEntry)> {
        self.justified.iter().enumerate().find(|(_, entry)| {
            entry.corpus == corpus
                && entry.language == language
                && *entry.site == *disagreement.site
                && entry.position == disagreement.position
                && *entry.target == *disagreement.unfold
        })
    }

    /// The entries for `corpus` and `language` whose index is not in `used`.
    pub(crate) fn unused(
        &self,
        corpus: &str,
        language: Language,
        used: &BTreeSet<usize>,
    ) -> Vec<JustifiedEntry> {
        self.justified
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                entry.corpus == corpus && entry.language == language && !used.contains(index)
            })
            .map(|(_, entry)| entry.clone())
            .collect()
    }
}
