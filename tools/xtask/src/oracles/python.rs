//! The Python oracle: the proof of concept's saved Pyright 1.1.407 evaluator rows, converted onto
//! Unfold's nodes. Each row is one call site with Pyright's canonical targets; a site's target
//! counts only when there is exactly one and it is indexed, as the proof of concept's
//! `comparePythonCalls` counted it (`compilers/codegraph/test/python-reference.ts`). Offsets go from
//! UTF-16 to UTF-8 bytes through each file's text. A site is named by the Unfold call node with
//! exactly its range, and a target by the Unfold definition with exactly its range.

use std::ops::RangeInclusive;
use std::path::Path;

use anyhow::{Context, bail};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use unfold_core::text::Utf16Range;
use unfold_core::vocab::{Language, NodeKind};
use unfold_core::{ByteRange, ContentHash, Graph, RepoPath, Snapshot, Utf16Index, canonical};

use super::corpora::{Lock, OracleCorpus, PathMapping, PyrightRows};
use super::file::{CallRow, FORMAT, Header, OracleFile, Provenance, ReferenceRow, corpus_sha256};

/// The Pyright version the saved rows were produced with.
pub(crate) const PYRIGHT_VERSION: &str = "1.1.407";

#[derive(Deserialize)]
struct RowsFile {
    provenance: RowsProvenance,
    reference: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowsProvenance {
    version: String,
    commit: String,
    typeshed_commit: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    file_path: String,
    start: u32,
    end: u32,
    canonical: Canonical,
    #[serde(default)]
    arguments: Vec<Argument>,
}

#[derive(Deserialize)]
struct Canonical {
    targets: Vec<Target>,
    policy: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    file_path: String,
    start: u32,
    end: u32,
    decorated_start: Option<u32>,
    kind: String,
    indexed: bool,
}

#[derive(Deserialize)]
struct Argument {
    position: u32,
    eligible: bool,
    canonical: Canonical,
}

impl Canonical {
    /// The one indexed target, when there is exactly one.
    fn single_indexed(&self) -> Option<&Target> {
        match self.targets.as_slice() {
            [target] if target.indexed => Some(target),
            _ => None,
        }
    }
}

/// Converts a half-open UTF-16 range to bytes.
fn utf16_to_bytes(index: &Utf16Index, start: u32, end: u32) -> anyhow::Result<ByteRange> {
    index
        .range_to_utf8(Utf16Range { start, end })
        .with_context(|| format!("UTF-16 range [{start}, {end})"))
}

/// Maps a rows path onto the corpus folder: the longest matching prefix wins.
fn map_path(paths: &[PathMapping], path: &str) -> Option<String> {
    paths
        .iter()
        .filter(|mapping| path.starts_with(&mapping.poc))
        .max_by_key(|mapping| mapping.poc.len())
        .map(|mapping| format!("{}{}", mapping.unfold, &path[mapping.poc.len()..]))
}

/// The proof of concept's aggregate source hash (`verify-python.ts`): SHA-256 of
/// `JSON.stringify([{filePath, sha256}, …])` sorted by its paths, which are the rows' paths.
fn poc_source_sha256(graph: &Graph, paths: &[PathMapping]) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Source {
        file_path: String,
        sha256: String,
    }
    let mut sources = Vec::new();
    for file in graph
        .files
        .iter()
        .filter(|file| file.file.language == Language::Python)
    {
        let path = file.file.path.as_str();
        let mapping = paths
            .iter()
            .filter(|mapping| path.starts_with(&mapping.unfold))
            .max_by_key(|mapping| mapping.unfold.len())
            .with_context(|| format!("{path} is outside the rows' path mapping"))?;
        let hash = file
            .file
            .hash
            .with_context(|| format!("{path} did not compile"))?;
        sources.push(Source {
            file_path: format!("{}{}", mapping.poc, &path[mapping.unfold.len()..]),
            sha256: hash.to_string(),
        });
    }
    sources.sort_by(|a, b| a.file_path.as_bytes().cmp(b.file_path.as_bytes()));
    Ok(ContentHash::of(serde_json::to_string(&sources)?.as_bytes()).to_string())
}

/// A node's start, kind, and key.
type Placed<'a> = (u32, NodeKind, &'a str);

/// The Python nodes of the graph by file and end.
struct Nodes<'a> {
    by_end: FxHashMap<(&'a str, u32), Vec<Placed<'a>>>,
}

impl<'a> Nodes<'a> {
    fn new(graph: &'a Graph) -> Self {
        let mut by_end: FxHashMap<_, Vec<_>> = FxHashMap::default();
        for file in graph
            .files
            .iter()
            .filter(|file| file.file.language == Language::Python)
        {
            for node in &file.nodes {
                by_end
                    .entry((file.file.path.as_str(), node.range.end))
                    .or_default()
                    .push((node.range.start, node.kind(), node.key.as_str()));
            }
        }
        Self { by_end }
    }

    /// The key of the node of `kind` that ends at `end` and starts in `starts`.
    fn find(&self, path: &str, starts: RangeInclusive<u32>, end: u32, kind: NodeKind) -> Option<&'a str> {
        self.by_end
            .get(&(path, end))?
            .iter()
            .find(|(start, found, _)| *found == kind && starts.contains(start))
            .map(|(_, _, key)| *key)
    }
}

/// Converts the saved rows for `corpus` (Python) into an oracle file. `folder` is the compiled
/// copy of the corpus (texts for the offset conversion); `graph` its compile.
pub(crate) fn import(
    corpus: &OracleCorpus,
    rows: &PyrightRows,
    poc: &Path,
    lock: &Lock,
    folder: &Path,
    graph: &Graph,
) -> anyhow::Result<OracleFile> {
    let rows_path = poc.join(&rows.rows);
    let bytes = std::fs::read(&rows_path).with_context(|| format!("cannot read {}", rows_path.display()))?;
    let saved: RowsFile =
        serde_json::from_slice(&bytes).with_context(|| format!("cannot parse {}", rows_path.display()))?;
    if saved.provenance.version != PYRIGHT_VERSION {
        bail!(
            "{} comes from Pyright {}, not {PYRIGHT_VERSION}",
            rows.rows,
            saved.provenance.version
        );
    }
    let expected = lock
        .entry(&rows.lock)
        .and_then(|entry| entry.poc_source_sha256.clone())
        .with_context(|| {
            format!(
                "testdata/corpora.lock.toml has no poc_source_sha256 for `{}`",
                rows.lock
            )
        })?;
    let actual = poc_source_sha256(graph, &rows.paths)?;
    if actual != expected {
        bail!(
            "corpus `{}` is not the source the rows describe: aggregate {actual}, the rows were computed for {expected}",
            corpus.name
        );
    }

    let nodes = Nodes::new(graph);
    let mut texts: FxHashMap<String, Utf16Index> = FxHashMap::default();
    let mut convert = |path: &str, start: u32, end: u32| -> anyhow::Result<(u32, u32)> {
        if !texts.contains_key(path) {
            let bytes = std::fs::read(folder.join(path)).with_context(|| format!("cannot read {path}"))?;
            let snapshot = Snapshot::new(RepoPath::new(path)?, bytes)?;
            texts.insert(path.to_owned(), snapshot.utf16_index());
        }
        let range = utf16_to_bytes(&texts[path], start, end).with_context(|| format!("in {path}"))?;
        Ok((range.start, range.end))
    };
    let mut calls = Vec::new();
    let mut references = Vec::new();
    for row in &saved.reference {
        let path = map_path(&rows.paths, &row.file_path)
            .with_context(|| format!("{} is outside the corpus's path mapping", row.file_path))?;
        let (start, end) = convert(&path, row.start, row.end)?;
        let site = nodes.find(&path, start..=start, end, NodeKind::Call);
        let mut call = CallRow::default();
        match site {
            Some(site) => call.site = Some(site.into()),
            None => {
                call.file = Some(path.as_str().into());
                call.range = Some([start, end]);
            }
        }
        match row.canonical.single_indexed() {
            Some(target) => match target_key(target, &rows.paths, &nodes, &mut convert)? {
                Ok(key) => call.target = Some(key.into()),
                Err(detail) => {
                    call.outcome = Some("unmapped".into());
                    call.detail = Some(detail.into());
                }
            },
            None if row.canonical.targets.len() == 1 => call.outcome = Some("external".into()),
            None => call.outcome = Some(row.canonical.policy.as_str().into()),
        }
        for argument in row.arguments.iter().filter(|argument| argument.eligible) {
            let Some(target) = argument.canonical.single_indexed() else {
                continue;
            };
            let Some(site) = site else {
                bail!("{path}:{start}: a resolved argument at a site with no Unfold call");
            };
            match target_key(target, &rows.paths, &nodes, &mut convert)? {
                Ok(key) => references.push((
                    (path.clone(), start, end),
                    ReferenceRow {
                        site: site.into(),
                        position: argument.position,
                        target: key.into(),
                        refused: None,
                    },
                )),
                Err(detail) => bail!("argument {} of {site}: {detail}", argument.position),
            }
        }
        calls.push(((path, start, end), call));
    }
    calls.sort_by(|a, b| order(&a.0, &b.0));
    references.sort_by(|a, b| order(&a.0, &b.0).then(a.1.position.cmp(&b.1.position)));

    let mut export = Vec::new();
    canonical::write(graph, &mut export)?;
    let calls: Vec<CallRow> = calls.into_iter().map(|(_, row)| row).collect();
    let header = Header {
        format: FORMAT.to_owned(),
        version: 1,
        corpus: corpus.name.clone(),
        language: Language::Python,
        oracle: "pyright".to_owned(),
        oracle_version: PYRIGHT_VERSION.to_owned(),
        corpus_sha256: corpus_sha256(graph, Language::Python)?,
        export_sha256: ContentHash::of(&export).to_string(),
        files: graph
            .files
            .iter()
            .filter(|file| file.file.language == Language::Python)
            .count(),
        sites: calls.len(),
        resolved: calls.iter().filter(|row| row.target.is_some()).count(),
        references: references.len(),
        unfold_sites_without_checker_site: None,
        source: Some(Provenance {
            poc_commit: lock.poc.commit.clone().unwrap_or_default(),
            rows: rows.rows.clone(),
            rows_sha256: ContentHash::of(&bytes).to_string(),
            poc_source_sha256: expected,
            pyright_commit: saved.provenance.commit,
            typeshed_commit: saved.provenance.typeshed_commit,
        }),
    };
    Ok(OracleFile {
        header,
        calls,
        references: references.into_iter().map(|(_, row)| row).collect(),
    })
}

fn order(a: &(String, u32, u32), b: &(String, u32, u32)) -> std::cmp::Ordering {
    a.0.as_bytes()
        .cmp(b.0.as_bytes())
        .then(a.1.cmp(&b.1))
        .then(a.2.cmp(&b.2))
}

/// The Unfold definition a Pyright target denotes: a class for a class and a function for a
/// function, ending where the target ends and starting between the target's statement (its first
/// decorator, or `async`) and its `def` or `class` keyword. An Unfold definition starts after its
/// decorators and at `async`. `Err` describes a target with no such node.
fn target_key<'a>(
    target: &Target,
    paths: &[PathMapping],
    nodes: &Nodes<'a>,
    convert: &mut impl FnMut(&str, u32, u32) -> anyhow::Result<(u32, u32)>,
) -> anyhow::Result<Result<&'a str, String>> {
    let path = map_path(paths, &target.file_path)
        .with_context(|| format!("target {} is outside the corpus's path mapping", target.file_path))?;
    let (first, end) = convert(&path, target.decorated_start.unwrap_or(target.start), target.end)?;
    let (start, _) = convert(&path, target.start, target.end)?;
    let kind = if target.kind == "class" {
        NodeKind::Class
    } else {
        NodeKind::Function
    };
    let key = nodes.find(&path, first..=start, end, kind);
    Ok(key.ok_or_else(|| format!("{} {path}:{start}-{end} is no Unfold definition", target.kind)))
}
