//! Converting one raw PoC generation into an Unfold [`Graph`].
//!
//! Conversion rules (also listed in `testdata/reference/README.md`):
//!
//! - Every path goes through the corpus's prefix mapping; keys are derived from mapped paths.
//! - Every source and input is read from the corpus's testdata folder (or, failing that, from
//!   the PoC when `--poc` is given) and must have the manifest's SHA-256.
//! - UTF-16 offsets become UTF-8 byte offsets through each file's [`Utf16Index`]. Lines are
//!   unit-independent and kept.
//! - Each file's block tree is rebuilt from `parent_id`; local IDs follow the CSV preorder and
//!   keys come from core's [`KeyBuilder`]. Edge `seq` is the edge's position among its source's
//!   rows. Block, edge, and target IDs become keys.
//! - The PoC `codegraph.json` inputs listed as settings sources become one `.unfold/config.toml`
//!   input (role `unfold-config`, hashed from the folder). Each manual entrance's evidence
//!   becomes its `[[entrance]]` table there ([`super::settings`]); its locator target text is
//!   made relative to the folder and checked against the table.
//! - Module labels that are repository paths are mapped like other paths.
//! - Entrance keys are re-derived after sorting ([`Graph::normalize`]).
//! - The result must pass core's validator against every snapshot, and its record counts must
//!   equal the CSV row counts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, bail, ensure};
use rustc_hash::FxHashMap;
use unfold_core::text::Utf16Range;
use unfold_core::vocab::{FileState, InputRole};
use unfold_core::{
    Block, ByteRange, ContentHash, Edge, Entrance, EntranceTarget, Evidence, FileGraph, FileRecord, Graph,
    InputRecord, Key, KeyBuilder, LineSpan, ModuleLabel, RepoPath, Slot, Snapshot, Utf16Index, Validator,
};

use super::corpora::Corpus;
use super::poc::{EntranceRow, Generation, TargetRow};
use super::settings::{ManualTable, manual_tables};

/// Where Unfold keeps the settings that replace the PoC's `codegraph.json` files.
pub(crate) const SETTINGS_PATH: &str = ".unfold/config.toml";

/// The converted graph and the number of records of each type.
#[derive(Debug)]
pub(crate) struct Converted {
    pub graph: Graph,
    pub counts: Counts,
}

/// Record counts, compared between the CSV tables and the converted graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Counts {
    pub files: usize,
    pub blocks: usize,
    pub slots: usize,
    pub edges: usize,
    pub inputs: usize,
    pub entrances: usize,
    pub targets: usize,
}

impl Counts {
    /// The counts of a graph's records.
    pub(crate) fn of(graph: &Graph) -> Self {
        Self {
            files: graph.files.len(),
            blocks: graph.files.iter().map(|file| file.blocks.len()).sum(),
            slots: graph.files.iter().map(|file| file.slots.len()).sum(),
            edges: graph.files.iter().map(|file| file.edges.len()).sum(),
            inputs: graph.inputs.len(),
            entrances: graph.entrances.len(),
            targets: graph
                .entrances
                .iter()
                .map(|entrance| entrance.targets.len())
                .sum(),
        }
    }
}

/// Finds the bytes of the files a generation describes.
#[derive(Debug, Clone)]
pub(crate) struct FileSource {
    /// The testdata folder Unfold opens.
    pub folder: PathBuf,
    /// The PoC directory the generation was built in, when `--poc` is given.
    pub poc_root: Option<PathBuf>,
}

impl FileSource {
    /// Reads `mapped` from the folder, or `poc_path` from the PoC if the folder lacks it, and
    /// checks the bytes against `sha256` when given.
    fn snapshot(&self, mapped: &RepoPath, poc_path: &str, sha256: Option<&str>) -> anyhow::Result<Snapshot> {
        let in_folder = self.folder.join(mapped.as_str());
        let location = match &self.poc_root {
            Some(poc_root) if !in_folder.is_file() => poc_root.join(poc_path),
            _ => in_folder,
        };
        let bytes =
            std::fs::read(&location).with_context(|| format!("cannot read {}", location.display()))?;
        let snapshot = Snapshot::new(mapped.clone(), bytes)
            .with_context(|| format!("cannot decode {}", location.display()))?;
        if let Some(sha256) = sha256 {
            let expected = ContentHash::from_hex(sha256)?;
            ensure!(
                snapshot.hash() == expected,
                "{} has SHA-256 {}, but the manifest records {expected} for `{poc_path}`",
                location.display(),
                snapshot.hash()
            );
        }
        Ok(snapshot)
    }
}

/// A file being rebuilt.
struct FileBuild {
    record: FileRecord,
    utf16: Utf16Index,
    keys: KeyBuilder,
    blocks: Vec<Block>,
    slots: Vec<Slot>,
    edges: Vec<Edge>,
}

/// Where a PoC block went: its file's index and its local ID.
#[derive(Clone, Copy)]
struct At {
    file: usize,
    local: u32,
}

/// Converts `generation` for `corpus`.
pub(crate) fn convert(
    corpus: &Corpus,
    generation: &Generation,
    source: &FileSource,
) -> anyhow::Result<Converted> {
    Converter::new(corpus, generation, source)?.run()
}

struct Converter<'a> {
    corpus: &'a Corpus,
    generation: &'a Generation,
    files: Vec<FileBuild>,
    file_index: FxHashMap<&'a str, usize>,
    blocks: FxHashMap<&'a str, At>,
    inputs: Vec<InputRecord>,
    /// Snapshots of every source and input, by mapped path, for evidence offsets and validation.
    snapshots: BTreeMap<RepoPath, Snapshot>,
    /// Input snapshots' offset indexes, by PoC path.
    input_utf16: FxHashMap<&'a str, (RepoPath, Utf16Index)>,
    manual: Vec<ManualTable>,
}

impl<'a> Converter<'a> {
    fn new(corpus: &'a Corpus, generation: &'a Generation, source: &FileSource) -> anyhow::Result<Self> {
        let mut converter = Self {
            corpus,
            generation,
            files: Vec::with_capacity(generation.manifest.files.len()),
            file_index: FxHashMap::default(),
            blocks: FxHashMap::default(),
            inputs: Vec::new(),
            snapshots: BTreeMap::new(),
            input_utf16: FxHashMap::default(),
            manual: Vec::new(),
        };
        converter.load_files(source)?;
        converter.load_inputs(source)?;
        Ok(converter)
    }

    fn map_path(&self, poc_path: &str) -> anyhow::Result<RepoPath> {
        let mapped = self
            .corpus
            .map_path(poc_path)
            .with_context(|| format!("no path mapping matches `{poc_path}`"))?;
        RepoPath::new(&mapped).with_context(|| format!("`{poc_path}` maps to an invalid path `{mapped}`"))
    }

    fn load_files(&mut self, source: &FileSource) -> anyhow::Result<()> {
        for (poc_path, entry) in &self.generation.manifest.files {
            ensure!(
                self.corpus.languages.contains(&entry.language),
                "`{poc_path}` is {}, which the corpus does not list",
                entry.language
            );
            let path = self.map_path(poc_path)?;
            let snapshot = source.snapshot(&path, poc_path, Some(&entry.sha256))?;
            self.file_index.insert(poc_path, self.files.len());
            self.files.push(FileBuild {
                record: FileRecord {
                    path: path.clone(),
                    language: entry.language,
                    state: FileState::Current,
                    hash: Some(snapshot.hash()),
                    failure: None,
                },
                utf16: snapshot.utf16_index(),
                keys: KeyBuilder::new(&path),
                blocks: Vec::new(),
                slots: Vec::new(),
                edges: Vec::new(),
            });
            self.snapshots.insert(path, snapshot);
        }
        Ok(())
    }

    fn load_inputs(&mut self, source: &FileSource) -> anyhow::Result<()> {
        let mut settings_inputs = 0;
        for (poc_path, entry) in &self.generation.manifest.inputs {
            if self.corpus.settings_source(poc_path).is_some() {
                settings_inputs += 1;
                continue;
            }
            let path = self.map_path(poc_path)?;
            let role = match path.file_name() {
                "package.json" => InputRole::Package,
                "pyproject.toml" => InputRole::Pyproject,
                _ => bail!(
                    "input `{poc_path}` is neither a package.json, a pyproject.toml, nor a settings source"
                ),
            };
            let snapshot = source.snapshot(&path, poc_path, Some(&entry.sha256))?;
            self.inputs.push(InputRecord {
                path: path.clone(),
                role,
                hash: snapshot.hash(),
            });
            self.input_utf16
                .insert(poc_path, (path.clone(), snapshot.utf16_index()));
            self.snapshots.insert(path, snapshot);
        }
        for settings in &self.corpus.settings {
            ensure!(
                self.generation.manifest.inputs.contains_key(&settings.poc),
                "settings source `{}` is not an input of the generation",
                settings.poc
            );
        }
        if settings_inputs > 0 {
            let path = RepoPath::new(SETTINGS_PATH)?;
            // Unfold's settings file has no PoC counterpart with the same bytes, so its hash is
            // taken as found.
            let snapshot = source.snapshot(&path, SETTINGS_PATH, None)?;
            self.manual = manual_tables(snapshot.text())
                .with_context(|| format!("{}", source.folder.join(SETTINGS_PATH).display()))?;
            self.inputs.push(InputRecord {
                path: path.clone(),
                role: InputRole::UnfoldConfig,
                hash: snapshot.hash(),
            });
            self.snapshots.insert(path, snapshot);
        }
        Ok(())
    }

    fn run(mut self) -> anyhow::Result<Converted> {
        self.convert_blocks()?;
        self.convert_slots()?;
        self.convert_edges()?;
        let entrances = self.convert_entrances()?;
        let mut graph = Graph {
            files: self
                .files
                .into_iter()
                .map(|file| FileGraph {
                    file: file.record,
                    blocks: file.blocks,
                    slots: file.slots,
                    edges: file.edges,
                })
                .collect(),
            inputs: self.inputs,
            entrances,
        };
        graph.normalize();

        let violations = Validator::new().with_sources(&self.snapshots).validate(&graph);
        if !violations.is_empty() {
            let shown: Vec<String> = violations.iter().take(20).map(ToString::to_string).collect();
            bail!(
                "the converted graph has {} violation(s):\n  {}",
                violations.len(),
                shown.join("\n  ")
            );
        }

        let generation = self.generation;
        let counts = Counts::of(&graph);
        let settings_inputs = generation
            .manifest
            .inputs
            .keys()
            .filter(|path| self.corpus.settings_source(path).is_some())
            .count();
        let expected = Counts {
            files: generation.manifest.files.len(),
            blocks: generation.blocks.len(),
            slots: generation.slots.len(),
            edges: generation.edges.len(),
            // The settings sources collapse into one `.unfold/config.toml`.
            inputs: generation.manifest.inputs.len() - settings_inputs + usize::from(settings_inputs > 0),
            entrances: generation.entrances.len(),
            targets: generation.targets.len(),
        };
        ensure!(
            counts == expected,
            "record counts {counts:?} differ from the CSV rows {expected:?}"
        );
        Ok(Converted { graph, counts })
    }

    fn block(&self, id: &str) -> anyhow::Result<At> {
        self.blocks
            .get(id)
            .copied()
            .with_context(|| format!("unknown block `{id}`"))
    }

    fn key(&self, id: &str) -> anyhow::Result<Key> {
        let at = self.block(id)?;
        Ok(self.files[at.file].blocks[at.local as usize].key.clone())
    }

    fn convert_blocks(&mut self) -> anyhow::Result<()> {
        for row in &self.generation.blocks {
            let context = || format!("block {}", row.id);
            let file_index = *self.file_index.get(&*row.file_path).with_context(|| {
                format!(
                    "block {} is in `{}`, which the manifest lacks",
                    row.id, row.file_path
                )
            })?;
            let parent = match row.parent_id.as_deref() {
                None => None,
                Some(parent_id) => {
                    let parent = self.block(parent_id).with_context(context)?;
                    ensure!(
                        parent.file == file_index,
                        "block {} has a parent in another file",
                        row.id
                    );
                    Some(parent.local)
                }
            };
            let file = &mut self.files[file_index];
            let local = u32::try_from(file.blocks.len()).context("too many blocks")?;
            let kind = row.kind.parse().with_context(context)?;
            let name = row.name.as_deref();
            let key = file.keys.push(parent, kind, name).with_context(context)?;
            file.blocks.push(Block {
                local_id: local,
                parent,
                kind,
                role: row
                    .role
                    .as_deref()
                    .map(str::parse)
                    .transpose()
                    .with_context(context)?,
                name: name.map(Box::from),
                range: utf16_to_bytes(&file.utf16, row.start_offset, row.end_offset).with_context(context)?,
                lines: LineSpan {
                    start: row.start_line,
                    end: row.end_line,
                },
                key,
            });
            ensure!(
                self.blocks
                    .insert(
                        &row.id,
                        At {
                            file: file_index,
                            local
                        }
                    )
                    .is_none(),
                "block ID {} repeats",
                row.id
            );
        }
        Ok(())
    }

    fn convert_slots(&mut self) -> anyhow::Result<()> {
        for row in &self.generation.slots {
            let context = || format!("slot {} of block {}", row.slot, row.block_id);
            let at = self.block(&row.block_id).with_context(context)?;
            let file = &mut self.files[at.file];
            file.slots.push(Slot {
                block: at.local,
                slot: row.slot.parse().with_context(context)?,
                index: row.index,
                range: utf16_to_bytes(&file.utf16, row.start_offset, row.end_offset).with_context(context)?,
            });
        }
        Ok(())
    }

    fn convert_edges(&mut self) -> anyhow::Result<()> {
        let mut seqs: FxHashMap<&str, u32> = FxHashMap::default();
        for row in &self.generation.edges {
            let context = || format!("edge from {} to `{}`", row.source_id, row.target_text);
            let at = self.block(&row.source_id).with_context(context)?;
            let seq = seqs.entry(&row.source_id).or_insert(0);
            let edge = Edge {
                source: at.local,
                seq: *seq,
                kind: row.kind.parse().with_context(context)?,
                target: row
                    .target_id
                    .as_deref()
                    .map(|id| self.key(id))
                    .transpose()
                    .with_context(context)?,
                text: row.target_text.clone(),
                resolution: row.resolution.parse().with_context(context)?,
                reason: row
                    .reason
                    .as_deref()
                    .map(str::parse)
                    .transpose()
                    .with_context(context)?,
                module: row
                    .module
                    .as_deref()
                    .map(|label| self.map_module(label))
                    .transpose()?,
                position: row.position,
            };
            *seq += 1;
            self.files[at.file].edges.push(edge);
        }
        Ok(())
    }

    /// Maps a module label's repository-path labels; package, built-in, and Python labels are
    /// kept.
    fn map_module(&self, label: &str) -> anyhow::Result<ModuleLabel> {
        // Only labels under a nonempty mapped prefix are paths; an empty prefix matches every
        // label, including `express`.
        let map_one = |one: &str| -> String {
            let paths = &self.corpus.paths;
            let is_path = paths.iter().any(|m| !m.poc.is_empty() && one.starts_with(&m.poc));
            is_path
                .then(|| self.corpus.map_path(one))
                .flatten()
                .unwrap_or_else(|| one.to_owned())
        };
        if label.starts_with('[') {
            let labels: Vec<String> =
                serde_json::from_str(label).with_context(|| format!("invalid module label set `{label}`"))?;
            let mapped: Vec<String> = labels.iter().map(|one| map_one(one)).collect();
            ModuleLabel::from_set(mapped.iter().map(String::as_str))
                .with_context(|| format!("invalid module label set `{label}`"))
        } else {
            ModuleLabel::single(&map_one(label)).with_context(|| format!("invalid module label `{label}`"))
        }
    }

    fn convert_entrances(&self) -> anyhow::Result<Vec<Entrance>> {
        let mut targets: FxHashMap<&str, Vec<&TargetRow>> = FxHashMap::default();
        for row in &self.generation.targets {
            targets.entry(&row.entrypoint_id).or_default().push(row);
        }
        let mut entrances = Vec::with_capacity(self.generation.entrances.len());
        for row in &self.generation.entrances {
            let mut rows = targets.remove(&*row.id).unwrap_or_default();
            rows.sort_by_key(|target| target.position);
            let entrance = self
                .convert_entrance(row, &rows)
                .with_context(|| format!("entrance {}", row.id))?;
            entrances.push(entrance);
        }
        if let Some(orphan) = targets.keys().min() {
            bail!("targets name entrance `{orphan}`, which does not exist");
        }
        Ok(entrances)
    }

    fn convert_entrance(&self, row: &EntranceRow, rows: &[&TargetRow]) -> anyhow::Result<Entrance> {
        let settings = self.corpus.settings_source(&row.evidence_file_path);
        let mut targets = Vec::with_capacity(rows.len());
        for target in rows {
            let text = match settings {
                // A manual locator is relative to its settings file; Unfold's is relative to
                // the folder.
                Some(settings) => self
                    .map_path(&format!("{}{}", settings.locator_base(), target.target_text))?
                    .as_str()
                    .into(),
                None => target.target_text.clone(),
            };
            targets.push(EntranceTarget {
                position: target.position,
                role: target.role.parse()?,
                target: target.target_id.as_deref().map(|id| self.key(id)).transpose()?,
                text,
                resolution: target.resolution.parse()?,
                reason: target.reason.as_deref().map(str::parse).transpose()?,
                module: target
                    .module
                    .as_deref()
                    .map(|label| self.map_module(label))
                    .transpose()?,
            });
        }
        let manual = settings
            .map(|settings| self.manual_table(row, settings.first_entrance))
            .transpose()?;
        let evidence = match manual {
            Some((index, table)) => Evidence {
                path: RepoPath::new(SETTINGS_PATH)?,
                range: table.range,
                block: None,
                pointer: Some(format!("/entrance/{index}").into()),
            },
            None => self.source_evidence(row)?,
        };
        let entrance = Entrance {
            // Derived by `Graph::normalize` once every entrance is in canonical order.
            key: Key::from_text(""),
            kind: row.kind.parse()?,
            label: row.label.clone(),
            method: row.method.as_deref().map(str::parse).transpose()?,
            trigger: row.trigger.clone(),
            trigger_status: row.trigger_status.parse()?,
            detector: row.detector.parse()?,
            evidence,
            targets,
        };
        if let Some((index, table)) = manual {
            check_manual_table(index, table, &entrance)?;
        }
        Ok(entrance)
    }

    /// The `[[entrance]]` table (and its index) in `.unfold/config.toml` that replaces a manual
    /// entrance's `codegraph.json` element `/entrypoints/<n>`.
    fn manual_table(
        &self,
        row: &EntranceRow,
        first_entrance: usize,
    ) -> anyhow::Result<(usize, &ManualTable)> {
        let pointer = row.evidence_pointer.as_deref().unwrap_or("");
        let ordinal: usize = pointer
            .strip_prefix("/entrypoints/")
            .and_then(|digits| digits.parse().ok())
            .with_context(|| format!("manual evidence pointer `{pointer}` is not /entrypoints/<n>"))?;
        let index = first_entrance + ordinal;
        let table = self
            .manual
            .get(index)
            .with_context(|| format!("{SETTINGS_PATH} has no [[entrance]] table {index}"))?;
        Ok((index, table))
    }

    /// The evidence of a detected entrance: a source block, or a `package.json` or
    /// `pyproject.toml` range with its pointer.
    fn source_evidence(&self, row: &EntranceRow) -> anyhow::Result<Evidence> {
        let poc_path = &*row.evidence_file_path;
        let (path, utf16) = match (self.file_index.get(poc_path), self.input_utf16.get(poc_path)) {
            (Some(&file), _) => (&self.files[file].record.path, &self.files[file].utf16),
            (None, Some((path, utf16))) => (path, utf16),
            (None, None) => bail!("evidence file `{poc_path}` is neither a source nor an input"),
        };
        Ok(Evidence {
            path: path.clone(),
            range: utf16_to_bytes(utf16, row.evidence_start_offset, row.evidence_end_offset)?,
            block: row
                .evidence_block_id
                .as_deref()
                .map(|id| self.key(id))
                .transpose()?,
            pointer: row.evidence_pointer.clone(),
        })
    }
}

/// Fails unless the `[[entrance]]` table declares `entrance`: same kind, trigger, method, label,
/// and locators.
fn check_manual_table(index: usize, table: &ManualTable, entrance: &Entrance) -> anyhow::Result<()> {
    let declared: Vec<&str> = entrance.targets.iter().map(|target| &*target.text).collect();
    ensure!(
        table.kind == entrance.kind.as_str()
            && table.trigger.as_deref() == entrance.trigger.as_deref()
            && table.method.as_deref() == entrance.method.map(|method| method.as_str())
            && table.label.as_deref() == entrance.label.as_deref()
            && table.targets == declared,
        "{SETTINGS_PATH} [[entrance]] table {index} ({} {:?} {:?}) does not declare this entrance ({} {:?} {declared:?})",
        table.kind,
        table.trigger,
        table.targets,
        entrance.kind,
        entrance.trigger,
    );
    Ok(())
}

/// Converts a half-open UTF-16 range to bytes.
pub(crate) fn utf16_to_bytes(index: &Utf16Index, start: u32, end: u32) -> anyhow::Result<ByteRange> {
    index
        .range_to_utf8(Utf16Range { start, end })
        .with_context(|| format!("UTF-16 range [{start}, {end})"))
}
