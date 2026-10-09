//! The oracle file: `<corpus>.<language>.jsonl`, one JSON object per line. The first line is the
//! header; then one `call` row per call site the oracle saw (with the checker's target, or the
//! outcome without one) and one `reference` row per named argument the oracle resolved.
//! `tools/oracles/README.md` documents the fields.

use std::path::Path;

use anyhow::{Context, bail};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use unfold_core::Graph;
use unfold_core::vocab::Language;

use super::corpora::listing_sha256;

/// The `format` every oracle header names.
pub(crate) const FORMAT: &str = "unfold-oracle";

/// One line of an oracle file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub(crate) enum Line {
    /// The first line.
    Header(Box<Header>),
    /// A call site.
    Call(CallRow),
    /// A named argument the oracle resolved.
    Reference(ReferenceRow),
}

/// What produced the file, for which corpus bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    /// Always [`FORMAT`].
    pub format: String,
    /// Format version (1).
    pub version: u32,
    /// The corpus name.
    pub corpus: String,
    /// The language whose sites the file covers.
    pub language: Language,
    /// The oracle: `typescript` (the checker) or `pyright`.
    pub oracle: String,
    /// The oracle's exact version.
    pub oracle_version: String,
    /// [`corpus_sha256`] of the corpus the rows describe.
    pub corpus_sha256: String,
    /// SHA-256 of the canonical export used to name blocks.
    pub export_sha256: String,
    /// Source files of the language.
    pub files: usize,
    /// `call` rows.
    pub sites: usize,
    /// `call` rows with a target and no policy refusal: the recall denominator.
    pub resolved: usize,
    /// `reference` rows.
    pub references: usize,
    /// Unfold `call`/`new` blocks that match no oracle call site (TypeScript only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unfold_sites_without_checker_site: Option<usize>,
    /// Where imported rows came from (Python only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Provenance>,
}

/// The origin of rows imported from the proof of concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    /// The proof-of-concept commit the rows were saved at.
    pub poc_commit: String,
    /// The rows file, relative to the proof-of-concept repository.
    pub rows: String,
    /// SHA-256 of the rows file.
    pub rows_sha256: String,
    /// The proof of concept's aggregate hash of the sources the rows describe.
    pub poc_source_sha256: String,
    /// The Pyright commit.
    pub pyright_commit: String,
    /// The typeshed commit Pyright used.
    pub typeshed_commit: String,
}

/// What the oracle says about one call site.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CallRow {
    /// The Unfold `call`/`new` (or Python `import`) block at exactly the site's range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<Box<str>>,
    /// The file of a site with no Unfold block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<Box<str>>,
    /// The byte range of a site with no Unfold block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[u32; 2]>,
    /// The block the oracle says the site targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Box<str>>,
    /// A policy refusal beside the target (`overridden`, `unbound_this`): the target is still the
    /// oracle's, but the site leaves the recall denominator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Box<str>>,
    /// Why there is no target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Box<str>>,
    /// More about the outcome (an unmapped declaration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Box<str>>,
}

/// A named argument the oracle resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceRow {
    /// The call or construction block.
    pub site: Box<str>,
    /// The written argument position.
    pub position: u32,
    /// The block the argument names.
    pub target: Box<str>,
    /// A policy refusal beside the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Box<str>>,
}

/// A parsed and checked oracle file.
#[derive(Debug)]
pub(crate) struct OracleFile {
    /// The header.
    pub header: Header,
    /// Call rows in file order.
    pub calls: Vec<CallRow>,
    /// Reference rows in file order.
    pub references: Vec<ReferenceRow>,
}

impl OracleFile {
    /// Reads and checks the file at `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid oracle file {}", path.display()))
    }

    /// Parses oracle file text: a header first, then rows, each row well formed.
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        let mut lines = text.lines().enumerate();
        let header = match lines.next().map(|(_, line)| serde_json::from_str::<Line>(line)) {
            Some(Ok(Line::Header(header))) => *header,
            Some(Err(error)) => bail!("line 1: {error}"),
            _ => bail!("line 1 is not a header"),
        };
        if header.format != FORMAT || header.version != 1 {
            bail!("unsupported format {} version {}", header.format, header.version);
        }
        let mut calls = Vec::new();
        let mut references = Vec::new();
        let mut sites = rustc_hash::FxHashSet::default();
        let mut arguments = rustc_hash::FxHashSet::default();
        for (index, line) in lines {
            let number = index + 1;
            match serde_json::from_str::<Line>(line).with_context(|| format!("line {number}"))? {
                Line::Header(_) => bail!("line {number}: a second header"),
                Line::Call(row) => {
                    check_call(&row).with_context(|| format!("line {number}"))?;
                    if let Some(site) = &row.site
                        && !sites.insert(site.clone())
                    {
                        bail!("line {number}: site {site} appears twice");
                    }
                    calls.push(row);
                }
                Line::Reference(row) => {
                    if !arguments.insert((row.site.clone(), row.position)) {
                        bail!(
                            "line {number}: argument {} of {} appears twice",
                            row.position,
                            row.site
                        );
                    }
                    references.push(row);
                }
            }
        }
        Ok(Self {
            header,
            calls,
            references,
        })
    }

    /// The call rows by Unfold site key.
    pub(crate) fn calls_by_site(&self) -> FxHashMap<&str, &CallRow> {
        self.calls
            .iter()
            .filter_map(|row| row.site.as_deref().map(|site| (site, row)))
            .collect()
    }

    /// The reference rows by site key and argument position.
    pub(crate) fn references_by_argument(&self) -> FxHashMap<(&str, u32), &ReferenceRow> {
        self.references
            .iter()
            .map(|row| ((&*row.site, row.position), row))
            .collect()
    }

    /// Writes the file: the header, then every row, one JSON object per line.
    pub(crate) fn render(&self) -> anyhow::Result<String> {
        let mut out = serde_json::to_string(&Line::Header(Box::new(self.header.clone())))?;
        out.push('\n');
        for row in &self.calls {
            out.push_str(&serde_json::to_string(&Line::Call(row.clone()))?);
            out.push('\n');
        }
        for row in &self.references {
            out.push_str(&serde_json::to_string(&Line::Reference(row.clone()))?);
            out.push('\n');
        }
        Ok(out)
    }
}

fn check_call(row: &CallRow) -> anyhow::Result<()> {
    if row.site.is_some() == (row.file.is_some() || row.range.is_some()) {
        bail!("a call row has either `site` or both `file` and `range`");
    }
    if row.site.is_none() && (row.file.is_none() || row.range.is_none()) {
        bail!("a call row without `site` needs `file` and `range`");
    }
    if row.target.is_some() == row.outcome.is_some() {
        bail!("a call row has exactly one of `target` and `outcome`");
    }
    if row.refused.is_some() && row.target.is_none() {
        bail!("`refused` needs a `target`");
    }
    Ok(())
}

/// The corpus hash both oracles record: [`listing_sha256`] over the language's source files and
/// every configuration input of the graph. Fails when a source of the language did not compile,
/// because a failed file has no hash to compare.
pub(crate) fn corpus_sha256(graph: &Graph, language: Language) -> anyhow::Result<String> {
    let mut entries = Vec::new();
    for file in graph.files.iter().filter(|file| file.file.language == language) {
        let hash = file.file.hash.with_context(|| {
            format!(
                "{} did not compile ({})",
                file.file.path,
                file.file.failure.as_deref().unwrap_or("no failure recorded")
            )
        })?;
        entries.push((file.file.path.as_str(), hash));
    }
    entries.extend(graph.inputs.iter().map(|input| (input.path.as_str(), input.hash)));
    Ok(listing_sha256(entries))
}
