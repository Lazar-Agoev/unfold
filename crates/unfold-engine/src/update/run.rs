//! One update from start to commit: the stages in order, building a new [`State`] beside the
//! engine's current one.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Instant;

use unfold_core::vocab::{FileState, InputRole};
use unfold_core::{ContentHash, Graph, InputRecord, RepoPath, Snapshot};

use super::build::{BuildInput, build};
use super::cache::{CacheEntry, FactsKey, NewCacheEntry, decode_facts};
use super::extract::{Extracted, Job, extract_all};
use super::read::{Counter, Read, read_all, read_file};
use super::scan::{self, Scan, stamp_of};
use super::state::{DiskState, Facts, FileEntry, InputEntry, SettingsState, State};
use super::{
    ChangeSummary, Changes, Engine, LinkBasis, Problem, ProblemKind, Stage, UpdateError, UpdateKind,
    UpdateOutcome, UpdateRequest, UpdateStats, Verify,
};
use crate::ENGINE_VERSION;
use crate::discovery::FileStamp;
use crate::entrances::launches;
use crate::language::{
    ExtractionOptions, Language, LanguageId, LanguageProblem, ProjectInput, ProjectInputs,
};
use crate::settings::SettingsHash;

/// How many times languages may ask for more inputs ([`Language::requested_inputs`]).
const INPUT_ROUNDS: usize = 16;

/// One running update.
pub(super) struct Run<'e, 'r> {
    engine: &'e Engine,
    request: &'r UpdateRequest<'r>,
    stats: UpdateStats,
    cache_problems: Vec<Problem>,
}

/// A source the snapshot stage looks at.
struct Pending<'s> {
    path: &'s RepoPath,
    language: &'s dyn Language,
    stamp: Option<FileStamp>,
    settings: SettingsHash,
    /// The extraction options, when they were computed for the settings hash; files that kept
    /// their previous hash get theirs only if they are extracted.
    options: Option<ExtractionOptions>,
    previous: Option<&'s FileEntry>,
}

impl<'e, 'r> Run<'e, 'r> {
    pub(super) fn new(engine: &'e Engine, request: &'r UpdateRequest<'r>, threads: usize) -> Self {
        Self {
            engine,
            request,
            stats: UpdateStats {
                kind: request.kind,
                threads,
                ..UpdateStats::default()
            },
            cache_problems: Vec::new(),
        }
    }

    pub(super) fn run(mut self) -> Result<(State, UpdateOutcome), UpdateError> {
        let started = Instant::now();
        let engine = self.engine;
        let request = self.request;
        let cancel = &request.cancel;
        let old = &engine.state;
        let rebuild = request.kind == UpdateKind::Rebuild;
        let empty = State::default();
        // What this update builds on: nothing for a rebuild, which drops last-good facts.
        let base = if rebuild { &empty } else { old };

        let stage = Instant::now();
        let changes = if base.built {
            &request.changes
        } else {
            &Changes::All
        };
        let settings = scan::load_settings(&engine.root, &base.settings, changes, request.verify);
        let settings_changed = settings.settings != base.settings.settings;
        let scan = scan::scan(engine, base, &settings.settings, settings_changed, changes)?;
        self.stats.full_discovery = scan.full;
        self.stats.stages.insert(Stage::Discover, stage.elapsed());
        cancel.check()?;

        let stage = Instant::now();
        // Inputs first: extraction options (and so settings hashes) can depend on them.
        let (project, inputs) = self.read_inputs(base, &settings, &scan)?;
        let input_problems = input_problems(engine, &project);
        let project_changed = settings_changed || inputs_differ(&base.inputs, &inputs);
        cancel.check()?;
        let pending = pending(engine, base, &scan, &project, project_changed);
        let (sources_to_read, kept) = self.split_reads(&pending, &scan);
        let paths: Vec<&RepoPath> = sources_to_read.iter().map(|pending| pending.path).collect();
        let counter = Counter::start(Stage::Snapshot, paths.len(), request.progress);
        let reads = read_all(&engine.root, &paths, cancel, &counter)?;
        self.stats.files_read = sources_to_read.len();
        self.stats.stages.insert(Stage::Snapshot, stage.elapsed());
        cancel.check()?;

        let stage = Instant::now();
        let mut files: BTreeMap<RepoPath, FileEntry> = kept;
        let mut snapshots: BTreeMap<RepoPath, Snapshot> = BTreeMap::new();
        let mut jobs: Vec<(&Pending<'_>, Snapshot)> = Vec::new();
        for (pending, read) in sources_to_read.into_iter().zip(reads) {
            match read {
                Read::Vanished => {}
                Read::Unreadable(message) => {
                    let disk = DiskState {
                        stamp: pending.stamp,
                        hash: None,
                    };
                    let facts = self.fall_back(pending, message);
                    files.insert(pending.path.clone(), entry(pending, disk, facts));
                }
                Read::Snapshot(snapshot) => {
                    let disk = DiskState {
                        stamp: pending.stamp,
                        hash: Some(snapshot.hash()),
                    };
                    snapshots.insert(pending.path.clone(), snapshot.clone());
                    match pending.previous {
                        // The same bytes under the same settings: nothing to extract.
                        Some(previous)
                            if previous.settings == pending.settings && previous.disk.hash == disk.hash =>
                        {
                            files.insert(
                                pending.path.clone(),
                                FileEntry {
                                    disk,
                                    ..previous.clone()
                                },
                            );
                        }
                        _ => jobs.push((pending, snapshot)),
                    }
                }
            }
        }
        self.extract(&mut files, &jobs, &settings, &project, rebuild)?;
        self.stats.stages.insert(Stage::Extract, stage.elapsed());
        cancel.check()?;

        let mut state = State {
            settings,
            files,
            inputs,
            unindexed: scan.unindexed,
            context: scan.context,
            discovery_problems: scan.problems.unwrap_or_else(|| base.discovery_problems.clone()),
            input_problems,
            link_problems: base.link_problems.clone(),
            discovered: true,
            built: true,
            // Relinked below unless the old graph was linked from what this update sees.
            linked: true,
            restored: None,
            graph: Arc::clone(&old.graph),
            launches: Arc::clone(&old.launches),
            project: Arc::default(),
            complete: false,
        };
        // Whether the old graph was linked from the context this update sees (everything but
        // the files' facts): by this engine, or, for a restored graph, by the session whose
        // basis the store kept, which also kept what linking found.
        let context_kept = if old.linked {
            !context_differs(old, &state)
        } else if let Some(basis) = old
            .restored
            .as_ref()
            .filter(|basis| basis.matches(engine, &state))
        {
            state.link_problems = basis.problems();
            true
        } else {
            false
        };
        state.complete = is_complete(&state);

        let relink = request.kind != UpdateKind::Background || !context_kept || files_differ(old, &state);
        if relink {
            // Only current files are checked against bytes: last-good facts (and entrance
            // evidence in them) come from earlier bytes than the ones just read.
            snapshots.retain(|path, _| {
                state
                    .files
                    .get(path)
                    .is_some_and(|entry| entry.facts.state() == FileState::Current)
            });
            for (path, input) in &project.inputs {
                snapshots
                    .entry(path.clone())
                    .or_insert_with(|| input.snapshot.clone());
            }
            for (path, input) in evidence(&state.inputs) {
                if let Ok(snapshot) = &input.content {
                    snapshots.entry(path.clone()).or_insert_with(|| snapshot.clone());
                }
            }
            let all_read = state.files.iter().all(|(path, entry)| match entry.facts.state() {
                FileState::Current => snapshots.contains_key(path),
                FileState::LastGood => false,
                FileState::Failed => true,
            });
            let input = BuildInput {
                engine,
                state: &state,
                project: &project,
                snapshots: &snapshots,
                all_read,
                cancel,
            };
            let built = build(&input, &mut self.stats)?;
            cancel.check()?;
            state.graph = Arc::new(built.graph);
            state.launches = Arc::new(built.launches);
            state.link_problems = link_problems(built.problems);
            state.complete = is_complete(&state);
        }
        // Evidence files shape nothing linking reads: a graph kept as it was takes their new
        // records as they are.
        let evidence_changed = !relink && {
            let records = evidence_records(&state.inputs);
            let changed = !state
                .graph
                .inputs
                .iter()
                .filter(|input| input.role == InputRole::Evidence)
                .eq(&records);
            if changed {
                state.graph = Arc::new(with_evidence(&state.graph, records));
            }
            changed
        };
        self.stats.relinked = relink;
        self.stats.files_removed = old
            .files
            .keys()
            .filter(|path| !state.files.contains_key(*path))
            .count();
        let summary = if relink {
            ChangeSummary::between(&old.graph, &state.graph)
        } else {
            ChangeSummary {
                inputs_changed: evidence_changed,
                ..ChangeSummary::default()
            }
        };
        self.stats.count_graph(&state.graph);
        self.stats.total = started.elapsed();
        let mut problems = standing_problems(&state);
        problems.append(&mut self.cache_problems);
        state.project = Arc::new(project);
        let outcome = UpdateOutcome {
            previous: Arc::clone(&old.graph),
            graph: Arc::clone(&state.graph),
            launches: Arc::clone(&state.launches),
            summary,
            stats: self.stats,
            problems,
            complete: state.complete,
            basis: LinkBasis::of(engine, &state),
        };
        Ok((state, outcome))
    }

    /// Splits sources into those to read and those kept as they are.
    fn split_reads<'p, 's>(
        &self,
        pending: &'p [Pending<'s>],
        scan: &Scan,
    ) -> (Vec<&'p Pending<'s>>, BTreeMap<RepoPath, FileEntry>) {
        let mut read = Vec::new();
        let mut kept = BTreeMap::new();
        for file in pending {
            let unchanged = file.previous.filter(|previous| {
                previous.settings == file.settings
                    && (!scan.check.contains(file.path)
                        || (self.request.verify == Verify::Stamps
                            && previous.disk.stamp.is_some()
                            && previous.disk.stamp == file.stamp))
            });
            match unchanged {
                Some(previous) => {
                    let mut entry = previous.clone();
                    entry.disk.stamp = file.stamp.or(previous.disk.stamp);
                    kept.insert(file.path.clone(), entry);
                }
                None => read.push(file),
            }
        }
        (read, kept)
    }

    /// The configuration inputs after the discover stage: the discovered ones (reading those
    /// that may have changed since `base`) and the ones languages request. Returns the project
    /// inputs languages see, and every input entry for the new state.
    pub(super) fn read_inputs(
        &mut self,
        base: &State,
        settings: &SettingsState,
        scan: &Scan,
    ) -> Result<(ProjectInputs, BTreeMap<RepoPath, InputEntry>), UpdateError> {
        let inputs_to_read = self.inputs_to_read(base, scan);
        let counter = Counter::start(Stage::Snapshot, inputs_to_read.len(), None);
        let reads = read_all(&self.engine.root, &inputs_to_read, &self.request.cancel, &counter)?;
        self.stats.inputs_read = inputs_to_read.len();
        let inputs = self.inputs(base, scan, &inputs_to_read, reads);
        Ok(self.project_inputs(base, settings, scan, inputs))
    }

    /// Discovered inputs that must be read.
    fn inputs_to_read<'s>(&self, base: &State, scan: &'s Scan) -> Vec<&'s RepoPath> {
        scan.inputs
            .iter()
            .filter(|(path, found)| {
                let previous = base
                    .inputs
                    .get(*path)
                    .filter(|previous| previous.role == found.role && !previous.requested);
                !previous.is_some_and(|previous| {
                    !scan.check.contains(path)
                        || (self.request.verify == Verify::Stamps
                            && previous.disk.stamp.is_some()
                            && previous.disk.stamp == found.stamp)
                })
            })
            .map(|(path, _)| path)
            .collect()
    }

    /// The discovered inputs after reading the changed ones.
    fn inputs(
        &self,
        base: &State,
        scan: &Scan,
        read: &[&RepoPath],
        reads: Vec<Read>,
    ) -> BTreeMap<RepoPath, InputEntry> {
        let mut fresh: BTreeMap<&RepoPath, Read> = read.iter().copied().zip(reads).collect();
        let mut inputs = BTreeMap::new();
        for (path, found) in &scan.inputs {
            let entry = match fresh.remove(path) {
                Some(Read::Vanished) => continue,
                Some(read) => input_entry(found.role, found.stamp, read, false),
                None => match base.inputs.get(path) {
                    Some(previous) => InputEntry {
                        disk: DiskState {
                            stamp: found.stamp.or(previous.disk.stamp),
                            ..previous.disk
                        },
                        ..previous.clone()
                    },
                    None => continue,
                },
            };
            inputs.insert(path.clone(), entry);
        }
        inputs
    }

    /// The project inputs languages link with, after the inputs they request are read, with the
    /// Python contexts of the configured launches; also returns every input entry for the new
    /// state.
    fn project_inputs(
        &mut self,
        base: &State,
        settings: &SettingsState,
        scan: &Scan,
        mut inputs: BTreeMap<RepoPath, InputEntry>,
    ) -> (ProjectInputs, BTreeMap<RepoPath, InputEntry>) {
        let mut project = ProjectInputs {
            settings: settings.settings.clone(),
            inputs: BTreeMap::new(),
            unindexed: scan.unindexed.iter().cloned().collect(),
            context: scan.context.iter().cloned().collect(),
            root: Some(self.engine.root.clone()),
            python_contexts: Vec::new(),
        };
        // Evidence files are never read by languages.
        for (path, input) in configuration(&inputs) {
            if let Ok(snapshot) = &input.content {
                let input = ProjectInput {
                    role: input.role,
                    snapshot: snapshot.clone(),
                };
                project.inputs.insert(path.clone(), input);
            }
        }
        if let Some(source) = &settings.source {
            let input = ProjectInput {
                role: InputRole::UnfoldConfig,
                snapshot: source.clone(),
            };
            project.inputs.insert(source.path().clone(), input);
        }
        let languages: Vec<&dyn Language> = self
            .engine
            .languages
            .iter()
            .filter(|language| settings.settings.language_enabled(language.id()))
            .map(|language| &**language)
            .collect();
        let mut asked: BTreeSet<RepoPath> = BTreeSet::new();
        for _ in 0..INPUT_ROUNDS {
            let mut requested: BTreeMap<RepoPath, InputRole> = BTreeMap::new();
            for language in &languages {
                for (path, role) in language.requested_inputs(&project) {
                    if !project.inputs.contains_key(&path) && !asked.contains(&path) {
                        requested.entry(path).or_insert(role);
                    }
                }
            }
            if requested.is_empty() {
                break;
            }
            for (path, role) in requested {
                asked.insert(path.clone());
                let Some(entry) = self.requested_input(base, &path, role) else {
                    continue;
                };
                if let Ok(snapshot) = &entry.content {
                    let input = ProjectInput {
                        role,
                        snapshot: snapshot.clone(),
                    };
                    project.inputs.insert(path.clone(), input);
                }
                inputs.insert(path, entry);
            }
        }
        let plans = launches::plans(&project.settings, &project.inputs, &self.engine.root_text);
        project.python_contexts = launches::contexts(&plans);
        (project, inputs)
    }

    /// One input a language asked for, or `None` if it doesn't exist.
    fn requested_input(&mut self, base: &State, path: &RepoPath, role: InputRole) -> Option<InputEntry> {
        let metadata = match std::fs::metadata(self.engine.root.join(path.as_str())) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return None,
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => {
                let read = Read::Unreadable(format!("cannot read the file: {error}").into());
                return Some(input_entry(role, None, read, true));
            }
        };
        let stamp = Some(stamp_of(&metadata));
        if self.request.verify == Verify::Stamps
            && let Some(previous) = base.inputs.get(path)
            && previous.requested
            && previous.role == role
            && previous.disk.stamp == stamp
        {
            return Some(previous.clone());
        }
        self.stats.inputs_read += 1;
        match read_file(&self.engine.root, path) {
            Read::Vanished => None,
            read => Some(input_entry(role, stamp, read, true)),
        }
    }

    /// Extracts `jobs` through the cache and records each file's new state in `files`.
    fn extract(
        &mut self,
        files: &mut BTreeMap<RepoPath, FileEntry>,
        jobs: &[(&Pending<'_>, Snapshot)],
        settings: &SettingsState,
        project: &ProjectInputs,
        rebuild: bool,
    ) -> Result<(), UpdateError> {
        let request = self.request;
        let options = job_options(jobs, project);
        let work: Vec<Job<'_>> = jobs
            .iter()
            .zip(&options)
            .map(|((pending, snapshot), options)| Job {
                path: pending.path,
                language: pending.language,
                snapshot,
                settings: pending.settings,
                options,
            })
            .collect();
        let cached = if rebuild || work.is_empty() {
            vec![None; work.len()]
        } else {
            self.cached(&work)
        };
        let counter = Counter::start(Stage::Extract, work.len(), request.progress);
        let results = extract_all(&work, cached, &settings.settings, &request.cancel, &counter)?;
        let fresh: Vec<NewCacheEntry<'_>> = work
            .iter()
            .zip(&results)
            .filter_map(|(job, result)| {
                let (status, bytes) = result.fresh.as_ref()?;
                Some(NewCacheEntry {
                    key: key(job),
                    status: *status,
                    bytes,
                })
            })
            .collect();
        if !fresh.is_empty()
            && let Err(error) = self.engine.cache.put(&fresh)
        {
            self.cache_problem(error.to_string());
        }
        drop(fresh);
        for ((pending, snapshot), result) in jobs.iter().zip(results) {
            if result.from_cache {
                self.stats.files_from_cache += 1;
            } else {
                self.stats.files_extracted += 1;
            }
            let disk = DiskState {
                stamp: pending.stamp,
                hash: Some(snapshot.hash()),
            };
            let facts = match result.extracted {
                Extracted::Facts(facts) => Facts::Current {
                    hash: snapshot.hash(),
                    facts,
                },
                Extracted::Failure(failure) => self.fall_back(pending, failure.to_string().into()),
            };
            files.insert(pending.path.clone(), entry(pending, disk, facts));
        }
        Ok(())
    }

    /// The cache's entries for `work`; a failing cache is a miss for every file.
    fn cached(&mut self, work: &[Job<'_>]) -> Vec<Option<CacheEntry>> {
        let keys: Vec<FactsKey<'_>> = work.iter().map(key).collect();
        match self.engine.cache.get(&keys) {
            Ok(found) if found.len() == keys.len() => found,
            Ok(found) => {
                self.cache_problem(format!(
                    "returned {} entries for {} keys",
                    found.len(),
                    keys.len()
                ));
                vec![None; work.len()]
            }
            Err(error) => {
                self.cache_problem(error.to_string());
                vec![None; work.len()]
            }
        }
    }

    /// The facts of a file whose bytes don't parse or can't be read (last-good facts): its last good
    /// facts under the same settings, from memory or the cache, or none at all.
    fn fall_back(&mut self, pending: &Pending<'_>, failure: Box<str>) -> Facts {
        if let Some((hash, facts)) = pending
            .previous
            .filter(|previous| previous.settings == pending.settings)
            .and_then(|previous| previous.facts.good())
        {
            return Facts::LastGood {
                hash,
                facts: Arc::clone(facts),
                failure,
            };
        }
        if self.request.kind != UpdateKind::Rebuild {
            match self
                .engine
                .cache
                .last_good(pending.path, ENGINE_VERSION, pending.settings)
            {
                Ok(Some((hash, bytes))) => {
                    if let Ok(facts) = decode_facts(&bytes) {
                        return Facts::LastGood {
                            hash,
                            facts: Arc::new(facts),
                            failure,
                        };
                    }
                }
                Ok(None) => {}
                Err(error) => self.cache_problem(error.to_string()),
            }
        }
        Facts::Failed { failure }
    }

    fn cache_problem(&mut self, message: String) {
        self.cache_problems.push(Problem {
            kind: ProblemKind::Cache,
            path: "".into(),
            message: message.into(),
        });
    }
}

/// Every source in scope with its language and settings hash, in path order. Settings hashes
/// (extraction options) are recomputed only for new files, or for every file when the settings
/// or the configuration inputs changed; each language computes them in one batch.
fn pending<'s>(
    engine: &'s Engine,
    base: &'s State,
    scan: &'s Scan,
    project: &ProjectInputs,
    project_changed: bool,
) -> Vec<Pending<'s>> {
    let mut pending: Vec<Pending<'s>> = Vec::with_capacity(scan.sources.len());
    let mut missing: BTreeMap<LanguageId, Vec<usize>> = BTreeMap::new();
    for (path, source) in &scan.sources {
        let Some(language) = engine.language(source.language) else {
            continue;
        };
        let previous = base
            .files
            .get(path)
            .filter(|entry| entry.language == source.language);
        let kept = previous
            .filter(|_| !project_changed)
            .map(|previous| previous.settings);
        if kept.is_none() {
            missing.entry(source.language).or_default().push(pending.len());
        }
        pending.push(Pending {
            path,
            language: &**language,
            stamp: source.stamp,
            settings: kept.unwrap_or_else(|| SettingsHash::from_bytes([0; 32])),
            options: None,
            previous,
        });
    }
    for indices in missing.values() {
        let language = pending[indices[0]].language;
        let paths: Vec<&RepoPath> = indices.iter().map(|&index| pending[index].path).collect();
        for (&index, options) in indices.iter().zip(options_of(language, &paths, project)) {
            pending[index].settings = options.hash;
            pending[index].options = Some(options);
        }
    }
    pending
}

/// The extraction options of `paths` (all owned by `language`), in the same order, in one
/// batch. A language that answers too few paths falls back to its plain settings hash for the
/// rest.
pub(super) fn options_of(
    language: &dyn Language,
    paths: &[&RepoPath],
    project: &ProjectInputs,
) -> Vec<ExtractionOptions> {
    let mut options = language.extraction_options(paths, &project.settings, project);
    debug_assert_eq!(options.len(), paths.len(), "one set of options per path");
    options.truncate(paths.len());
    for path in &paths[options.len()..] {
        options.push(ExtractionOptions::new(
            language.settings_hash(path, &project.settings),
        ));
    }
    options
}

/// The extraction options of each job: the ones computed with its settings hash, or, for a file
/// that kept its hash, computed now (in one batch per language).
fn job_options(jobs: &[(&Pending<'_>, Snapshot)], project: &ProjectInputs) -> Vec<ExtractionOptions> {
    let mut options: Vec<Option<ExtractionOptions>> =
        jobs.iter().map(|(pending, _)| pending.options.clone()).collect();
    let mut missing: BTreeMap<LanguageId, Vec<usize>> = BTreeMap::new();
    for (index, (pending, _)) in jobs.iter().enumerate() {
        if options[index].is_none() {
            missing.entry(pending.language.id()).or_default().push(index);
        }
    }
    for indices in missing.values() {
        let language = jobs[indices[0]].0.language;
        let paths: Vec<&RepoPath> = indices.iter().map(|&index| jobs[index].0.path).collect();
        let computed = language.extraction_options(&paths, &project.settings, project);
        for (&index, computed) in indices.iter().zip(computed) {
            // The hash was kept because neither settings nor inputs changed, and options
            // depend on nothing else (`Language::extraction_options`).
            debug_assert_eq!(
                computed.hash, jobs[index].0.settings,
                "options changed with a kept hash"
            );
            options[index] = Some(computed);
        }
    }
    options
        .into_iter()
        .zip(jobs)
        .map(|(options, (pending, _))| options.unwrap_or_else(|| ExtractionOptions::new(pending.settings)))
        .collect()
}

/// What the enabled languages find wrong with the configuration inputs, sorted.
fn input_problems(engine: &Engine, project: &ProjectInputs) -> Vec<Problem> {
    let mut problems: Vec<Problem> = engine
        .languages
        .iter()
        .filter(|language| project.settings.language_enabled(language.id()))
        .flat_map(|language| language.input_problems(project))
        .map(|problem| Problem {
            kind: ProblemKind::Input,
            path: problem.path.as_str().into(),
            message: problem.message,
        })
        .collect();
    problems.sort();
    problems.dedup();
    problems
}

/// The problems linking found, as sorted, distinct engine problems (with whether each makes
/// the graph incomplete). They are listed with the configuration-input problems.
fn link_problems(found: Vec<LanguageProblem>) -> Vec<(Problem, bool)> {
    let mut problems: Vec<(Problem, bool)> = found
        .into_iter()
        .map(|problem| {
            let listed = Problem {
                kind: ProblemKind::Input,
                path: problem.path.as_str().into(),
                message: problem.message,
            };
            (listed, problem.incomplete)
        })
        .collect();
    problems.sort();
    problems.dedup();
    problems
}

/// The configuration inputs of `inputs`: every one but the evidence files.
pub(super) fn configuration(
    inputs: &BTreeMap<RepoPath, InputEntry>,
) -> impl Iterator<Item = (&RepoPath, &InputEntry)> {
    inputs
        .iter()
        .filter(|(_, input)| input.role != InputRole::Evidence)
}

/// The evidence files of `inputs`.
fn evidence(inputs: &BTreeMap<RepoPath, InputEntry>) -> impl Iterator<Item = (&RepoPath, &InputEntry)> {
    inputs
        .iter()
        .filter(|(_, input)| input.role == InputRole::Evidence)
}

/// The graph records of the evidence files of `inputs` that could be read, in path order.
pub(super) fn evidence_records(inputs: &BTreeMap<RepoPath, InputEntry>) -> Vec<InputRecord> {
    evidence(inputs)
        .filter_map(|(path, input)| {
            let snapshot = input.content.as_ref().ok()?;
            Some(InputRecord {
                path: path.clone(),
                role: InputRole::Evidence,
                hash: snapshot.hash(),
            })
        })
        .collect()
}

/// `graph` with its evidence input records replaced by `records`.
fn with_evidence(graph: &Graph, records: Vec<InputRecord>) -> Graph {
    let mut inputs: Vec<InputRecord> = graph
        .inputs
        .iter()
        .filter(|input| input.role != InputRole::Evidence)
        .cloned()
        .chain(records)
        .collect();
    inputs.sort_by(|a, b| a.path.cmp(&b.path));
    Graph {
        files: graph.files.clone(),
        inputs,
        entrances: graph.entrances.clone(),
    }
}

/// The content of an input as linking sees it: its role and its bytes' hash.
fn fingerprint(entry: &InputEntry) -> (InputRole, Option<ContentHash>) {
    (entry.role, entry.content.as_ref().ok().map(Snapshot::hash))
}

/// Whether two sets of configuration inputs differ in path, role, or content (evidence files
/// don't count).
fn inputs_differ(old: &BTreeMap<RepoPath, InputEntry>, new: &BTreeMap<RepoPath, InputEntry>) -> bool {
    !configuration(old)
        .map(|(path, entry)| (path, fingerprint(entry)))
        .eq(configuration(new).map(|(path, entry)| (path, fingerprint(entry))))
}

fn key<'a>(job: &Job<'a>) -> FactsKey<'a> {
    FactsKey {
        path: job.path,
        hash: job.snapshot.hash(),
        engine: ENGINE_VERSION,
        settings: job.settings,
    }
}

fn entry(pending: &Pending<'_>, disk: DiskState, facts: Facts) -> FileEntry {
    FileEntry {
        language: pending.language.id(),
        settings: pending.settings,
        disk,
        facts,
    }
}

fn input_entry(role: InputRole, stamp: Option<FileStamp>, read: Read, requested: bool) -> InputEntry {
    let (hash, content) = match read {
        Read::Snapshot(snapshot) => (Some(snapshot.hash()), Ok(snapshot)),
        Read::Unreadable(message) => (None, Err(message)),
        Read::Vanished => (None, Err("the file disappeared".into())),
    };
    InputEntry {
        role,
        disk: DiskState { stamp, hash },
        content,
        requested,
    }
}

/// Whether anything but the files that shapes the graph differs between two states: the
/// settings, the settings file, the configuration inputs, or the unindexed or context files
/// (what [`LinkBasis`] fingerprints). Evidence files shape nothing linking reads.
fn context_differs(old: &State, new: &State) -> bool {
    let source_hash = |state: &State| state.settings.source.as_ref().map(Snapshot::hash);
    old.settings.settings != new.settings.settings
        || source_hash(old) != source_hash(new)
        || old.unindexed != new.unindexed
        || old.context != new.context
        || inputs_differ(&old.inputs, &new.inputs)
}

/// Whether the indexed files, their languages, or their facts differ between two states.
fn files_differ(old: &State, new: &State) -> bool {
    old.files.len() != new.files.len()
        || old
            .files
            .iter()
            .zip(&new.files)
            .any(|((a, x), (b, y))| a != b || x.language != y.language || !x.facts.same_as(&y.facts))
}

fn is_complete(state: &State) -> bool {
    state.settings.problem.is_none()
        && state.discovery_problems.is_empty()
        && state.input_problems.is_empty()
        && state.link_problems.iter().all(|(_, incomplete)| !incomplete)
        && configuration(&state.inputs).all(|(_, input)| input.content.is_ok())
        && state
            .files
            .values()
            .all(|entry| entry.facts.state() != FileState::Failed)
}

/// The settings, discovery, and input problems of a state.
pub(super) fn standing_problems(state: &State) -> Vec<Problem> {
    let mut problems: Vec<Problem> = state.settings.problem.iter().cloned().collect();
    problems.extend(state.discovery_problems.iter().cloned());
    problems.extend(state.input_problems.iter().cloned());
    problems.extend(state.link_problems.iter().map(|(problem, _)| problem.clone()));
    problems.extend(state.inputs.iter().filter_map(|(path, input)| {
        let message = input.content.as_ref().err()?;
        Some(Problem {
            kind: ProblemKind::Input,
            path: path.as_str().into(),
            message: message.clone(),
        })
    }));
    problems
}
