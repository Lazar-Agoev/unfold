//! `testdata/reference/oracles/floors.toml`: the recall floor per corpus and language. A floor
//! records the matched and resolved counts it was set from; recall passes when it is at least that
//! ratio, compared exactly. Floors only rise: when recall is higher, the check says so and the
//! floor is raised by hand.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use unfold_core::vocab::Language;

use super::compare::Ratio;
use super::corpora::oracles_dir;

/// The parsed file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Floors {
    /// Every floor, in file order.
    #[serde(default)]
    pub floor: Vec<Floor>,
}

/// One recall floor.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Floor {
    /// The oracle corpus.
    pub corpus: String,
    /// The language.
    pub language: Language,
    /// Sites Unfold resolved to the oracle's target when the floor was set.
    pub matched: usize,
    /// Sites the oracle resolved when the floor was set.
    pub resolved: usize,
}

impl Floor {
    /// The floor as a ratio.
    pub(crate) fn ratio(&self) -> Ratio {
        Ratio {
            part: self.matched,
            total: self.resolved,
        }
    }
}

impl Floors {
    /// `testdata/reference/oracles/floors.toml`.
    pub(crate) fn default_path() -> PathBuf {
        oracles_dir().join("floors.toml")
    }

    /// Reads and checks the file at `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parses and checks the text: one floor per corpus and language, matched never above resolved.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let floors: Self = toml::from_str(text)?;
        let mut seen = std::collections::BTreeSet::new();
        for floor in &floors.floor {
            if !seen.insert((&floor.corpus, floor.language)) {
                bail!("corpus `{}` has two {} floors", floor.corpus, floor.language);
            }
            if floor.matched > floor.resolved {
                bail!(
                    "corpus `{}`: matched {} exceeds resolved {}",
                    floor.corpus,
                    floor.matched,
                    floor.resolved
                );
            }
        }
        Ok(floors)
    }

    /// The floor for `corpus` and `language`.
    pub(crate) fn get(&self, corpus: &str, language: Language) -> Option<&Floor> {
        self.floor
            .iter()
            .find(|floor| floor.corpus == corpus && floor.language == language)
    }
}
