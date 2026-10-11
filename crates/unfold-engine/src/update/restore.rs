//! Reopening a folder (docs/engine.md#project-lifecycle): rebuilding the in-memory program from the stored
//! graph and the facts cache, without parsing, so the next update is fast.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::vocab::FileState;
use unfold_core::{FileRecord, Graph, RepoPath};

use super::cache::{CacheStatus, FactsKey, decode_facts};
use super::run::{Run, options_of};
use super::scan::{load_settings, scan};
use super::state::{DiskState, Facts, FileEntry, InputEntry, SettingsState, State};
use super::{Changes, Engine, LinkBasis, UpdateError, UpdateKind, UpdateRequest, Verify};
use crate::ENGINE_VERSION;
use crate::language::{Language, LanguageId, ProjectInputs};
use crate::settings::SettingsHash;

/// What [`Engine::restore`] loaded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreStats {
    /// Files in the stored graph.
    pub files: usize,
    /// Files whose facts came back from the cache (or that are `failed` and need none).
    pub restored: usize,
    /// Files the cache had no usable facts for; the next update reads and extracts them.
    pub missing: usize,
}

impl Engine {
    /// Restores the state of a previous session: `graph` is the stored graph, `disks` the
    /// stored disk state of its files, and `basis` what the graph was linked from
    /// ([`super::UpdateOutcome::basis`]), if it was kept. Each file's facts come from the cache
    /// by the hash its facts came from and its settings hash, which the extraction options give
    /// ([`Language::extraction_options`]), so the configuration inputs are read first, as an
    /// update reads them; files the cache can't supply are left for the next update. Follow
    /// with [`super::UpdateRequest::reopen`], which checks every file (and input) by size and
    /// modification time and hashes only those that differ, and links the graph again only when
    /// something changed or `basis` doesn't match.
    pub fn restore(
        &mut self,
        graph: Graph,
        disks: impl IntoIterator<Item = (RepoPath, DiskState)>,
        basis: Option<LinkBasis>,
    ) -> Result<RestoreStats, UpdateError> {
        let disks: FxHashMap<RepoPath, DiskState> = disks.into_iter().collect();
        let pool = self.pool(UpdateKind::Background)?;
        let this = &*self;
        let (state, stats) = pool.install(|| this.restored(graph, &disks, basis));
        self.set_state(state);
        Ok(stats)
    }

    fn restored(
        &self,
        graph: Graph,
        disks: &FxHashMap<RepoPath, DiskState>,
        basis: Option<LinkBasis>,
    ) -> (State, RestoreStats) {
        let settings = load_settings(
            &self.root,
            &SettingsState::default(),
            &Changes::All,
            Verify::Contents,
        );
        let (project, inputs) = self.restored_inputs(&settings);
        let mut stats = RestoreStats {
            files: graph.files.len(),
            ..RestoreStats::default()
        };
        let records: Vec<(&FileRecord, &dyn Language)> = graph
            .files
            .iter()
            .filter_map(|file| Some((&file.file, &**self.language(file.file.language)?)))
            .collect();
        let hashes = settings_hashes(&records, &project);
        let mut wanted = Vec::new();
        let mut files = BTreeMap::new();
        for ((record, _), settings) in records.iter().zip(hashes) {
            let entry = FileEntry {
                language: record.language,
                settings,
                disk: disks.get(&record.path).copied().unwrap_or_default(),
                facts: Facts::Failed {
                    failure: record.failure.clone().unwrap_or_else(|| "no facts".into()),
                },
            };
            match (record.state, record.hash) {
                (FileState::Failed, _) => {
                    files.insert(record.path.clone(), entry);
                }
                (state, Some(hash)) => wanted.push((*record, state, hash, entry)),
                (_, None) => {}
            }
        }
        let keys: Vec<FactsKey<'_>> = wanted
            .iter()
            .map(|(record, _, hash, entry)| FactsKey {
                path: &record.path,
                hash: *hash,
                engine: ENGINE_VERSION,
                settings: entry.settings,
            })
            .collect();
        // A failing cache restores nothing; the next update extracts instead.
        let cached = self.cache.get(&keys).unwrap_or_default();
        let decoded: Vec<_> = cached
            .into_par_iter()
            .map(|entry| {
                let entry = entry.filter(|entry| entry.status == CacheStatus::Parsed)?;
                decode_facts(&entry.bytes).ok().map(Arc::new)
            })
            .collect();
        for ((record, state, hash, mut entry), facts) in wanted.into_iter().zip(decoded) {
            let Some(facts) = facts else {
                continue;
            };
            entry.facts = match (state, &record.failure) {
                (FileState::LastGood, Some(failure)) => Facts::LastGood {
                    hash,
                    facts,
                    failure: failure.clone(),
                },
                _ => Facts::Current { hash, facts },
            };
            files.insert(record.path.clone(), entry);
        }
        stats.restored = files.len();
        stats.missing = stats.files - stats.restored;
        let complete = settings.problem.is_none()
            && graph
                .files
                .iter()
                .all(|file| file.file.state != FileState::Failed);
        let state = State {
            settings,
            files,
            // Kept with their stamps, so the reopen check reads only inputs that changed and
            // keeps every file's settings hash when none did.
            inputs,
            built: true,
            restored: basis,
            complete,
            graph: Arc::new(graph),
            ..State::default()
        };
        (state, stats)
    }

    /// The configuration inputs on disk, discovered and read as an update would. Restoring
    /// is best effort: when discovery fails, there are none, and files whose settings hash
    /// depends on inputs miss the cache until the next update reads them.
    fn restored_inputs(&self, settings: &SettingsState) -> (ProjectInputs, BTreeMap<RepoPath, InputEntry>) {
        let request = UpdateRequest::reopen();
        let empty = State::default();
        let mut run = Run::new(self, &request, 1);
        scan(self, &empty, &settings.settings, false, &Changes::All)
            .and_then(|scan| run.read_inputs(&empty, settings, &scan))
            .unwrap_or_else(|_| {
                let project = ProjectInputs {
                    settings: settings.settings.clone(),
                    root: Some(self.root.clone()),
                    ..ProjectInputs::default()
                };
                (project, BTreeMap::new())
            })
    }
}

/// Each record's settings hash from its extraction options, in order, one batch per language.
fn settings_hashes(records: &[(&FileRecord, &dyn Language)], project: &ProjectInputs) -> Vec<SettingsHash> {
    let mut hashes = vec![SettingsHash::from_bytes([0; 32]); records.len()];
    let mut batches: BTreeMap<LanguageId, Vec<usize>> = BTreeMap::new();
    for (index, (record, _)) in records.iter().enumerate() {
        batches.entry(record.language).or_default().push(index);
    }
    for indices in batches.values() {
        let language = records[indices[0]].1;
        let paths: Vec<&RepoPath> = indices.iter().map(|&index| &records[index].0.path).collect();
        for (&index, options) in indices.iter().zip(options_of(language, &paths, project)) {
            hashes[index] = options.hash;
        }
    }
    hashes
}
