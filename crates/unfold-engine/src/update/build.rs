//! The link, resolve, detect, and validate stages: the whole program from its per-file facts
//! to a validated graph, in memory, without parsing or reading anything.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use rayon::prelude::*;

use unfold_core::validate::{Subject, Violation};
use unfold_core::vocab::FileState;
use unfold_core::{Edge, Entrance, FileGraph, Graph, InputRecord, RepoPath, Snapshot};

use super::state::State;
use super::targets::FolderTargets;
use super::{Engine, Stage, UpdateError, UpdateStats};
use crate::cancel::CancelToken;
use crate::edges::program_edges_checked;
use crate::entrances::{self, manual_candidates};
use crate::error::BackendError;
use crate::language::{Language, LanguageProblem, Linked, ProjectInputs};
use crate::program::{BlockRef, FactsSet, FileId, ProgramFile};
use crate::resolve::Resolver;
use crate::settings::SETTINGS_PATH;
use crate::validate::validate_graph;

/// What the build stages read.
pub(crate) struct BuildInput<'a> {
    pub(crate) engine: &'a Engine,
    /// The new state; its graph is not used.
    pub(crate) state: &'a State,
    pub(crate) project: &'a ProjectInputs,
    /// Every file and input read this update, plus every input and the settings file.
    pub(crate) snapshots: &'a BTreeMap<RepoPath, Snapshot>,
    /// Whether `snapshots` holds every current file and no file is `last_good`, so the whole
    /// graph (entrance evidence included) can be checked against its bytes in one pass.
    pub(crate) all_read: bool,
    pub(crate) cancel: &'a CancelToken,
}

/// What the build stages produce: the validated graph and the problems linking found.
pub(crate) struct Built {
    pub(crate) graph: Graph,
    /// Every language's [`Linked::problems`], in language order.
    pub(crate) problems: Vec<LanguageProblem>,
}

/// Links, resolves, detects, and validates; times each stage into `stats`.
pub(crate) fn build(input: &BuildInput<'_>, stats: &mut UpdateStats) -> Result<Built, UpdateError> {
    let BuildInput {
        engine,
        state,
        project,
        cancel,
        ..
    } = *input;
    let program = FactsSet::new(state.files.iter().map(|(path, entry)| ProgramFile {
        path: path.clone(),
        language: entry.language,
        facts: entry.facts.program(),
    }))?;
    let settings = &project.settings;
    let languages: Vec<&dyn Language> = engine
        .languages
        .iter()
        .filter(|language| settings.language_enabled(language.id()))
        .map(|language| &**language)
        .collect();

    let started = Instant::now();
    // Languages link independently (each over its own files), so they link in parallel; the
    // results stay in language order.
    let linked: Vec<Box<dyn Linked + '_>> = languages
        .par_iter()
        .map(|language| {
            cancel.check()?;
            Ok(language.link(&program, project))
        })
        .collect::<Result<_, UpdateError>>()?;
    stats.stages.insert(Stage::Link, started.elapsed());
    cancel.check()?;

    let started = Instant::now();
    let resolvers: Vec<Resolver<'_>> = linked
        .iter()
        .map(|linked| Resolver::new(&program, linked.as_ref()))
        .collect();
    let mut edges: Vec<Vec<Edge>> = vec![Vec::new(); program.files().len()];
    for resolver in &resolvers {
        let resolved = program_edges_checked(resolver, || cancel.check().map_err(UpdateError::from))?;
        for (file, file_edges) in resolved {
            edges[file.index()] = file_edges;
        }
    }
    stats.stages.insert(Stage::Resolve, started.elapsed());
    cancel.check()?;

    let started = Instant::now();
    // Detection order: manual declarations, then each language's detectors in language order.
    let mut candidates = if state.settings.source.is_some() {
        manual_candidates(settings)
    } else {
        Vec::new()
    };
    for resolver in &resolvers {
        cancel.check()?;
        candidates.extend(entrances::detect(resolver, settings));
    }
    let linked_refs: Vec<&dyn Linked> = linked.iter().map(|linked| linked.as_ref()).collect();
    let targets = FolderTargets {
        root: &engine.root,
        root_text: &engine.root_text,
        program: &program,
        unindexed: &state.unindexed,
        linked: &linked_refs,
    };
    let resolver_refs: Vec<&Resolver<'_>> = resolvers.iter().collect();
    let entrances = entrances::assemble(&program, &resolver_refs, &targets, candidates)?;
    stats.stages.insert(Stage::Detect, started.elapsed());
    cancel.check()?;

    // File IDs follow path order, as does the state's map, so the two line up. Conditional
    // import blocks the owning language doesn't keep are dropped here, after resolution.
    let entries: Vec<_> = state.files.values().collect();
    let files: Vec<FileGraph> = program
        .files()
        .par_iter()
        .zip(entries)
        .zip(edges)
        .enumerate()
        .map(|(index, ((file, entry), edges))| {
            cancel.check()?;
            let id = FileId(index as u32);
            let record = entry.record(&file.path);
            let Some(facts) = &file.facts else {
                return Ok(FileGraph {
                    file: record,
                    blocks: Vec::new(),
                    slots: Vec::new(),
                    edges: Vec::new(),
                });
            };
            if facts.graph.conditional().is_empty() {
                return Ok(facts.graph.file_graph(record, edges));
            }
            let owner = linked_refs
                .iter()
                .find(|linked| linked.language() == file.language);
            facts
                .graph
                .linked_file_graph(record, edges, |local| {
                    owner.is_some_and(|linked| linked.keeps_import(BlockRef { file: id, local }))
                })
                .map_err(|error| {
                    UpdateError::from(BackendError::Facts {
                        path: file.path.clone(),
                        message: error.to_string().into(),
                    })
                })
        })
        .collect::<Result<_, UpdateError>>()?;
    let inputs = graph_inputs(state, project, &linked_refs, &entrances);
    let mut graph = Graph {
        files,
        inputs,
        entrances,
    };
    graph.normalize();
    cancel.check()?;

    let started = Instant::now();
    let violations = validate(&graph, input.snapshots, input.all_read, &linked_refs);
    stats.stages.insert(Stage::Validate, started.elapsed());
    cancel.check()?;
    if !violations.is_empty() {
        return Err(UpdateError::Validation(violations));
    }
    let problems = linked_refs.iter().flat_map(|linked| linked.problems()).collect();
    Ok(Built { graph, problems })
}

/// The inputs that shaped the graph: the settings file when settings came from it, what each
/// language says it read, and every input an entrance cites as evidence. Paths that aren't
/// among the project inputs are ignored.
fn graph_inputs(
    state: &State,
    project: &ProjectInputs,
    linked: &[&dyn Linked],
    entrances: &[Entrance],
) -> Vec<InputRecord> {
    let mut paths: BTreeSet<RepoPath> = linked.iter().flat_map(|linked| linked.inputs_read()).collect();
    paths.extend(
        entrances
            .iter()
            .filter(|entrance| entrance.evidence.pointer.is_some())
            .map(|entrance| entrance.evidence.path.clone()),
    );
    if let Some(source) = &state.settings.source {
        debug_assert_eq!(source.path().as_str(), SETTINGS_PATH);
        paths.insert(source.path().clone());
    }
    paths
        .into_iter()
        .filter_map(|path| {
            let input = project.inputs.get(&path)?;
            Some(InputRecord {
                role: input.role,
                hash: input.snapshot.hash(),
                path,
            })
        })
        .collect()
}

/// Every violation of `graph`.
///
/// The structural, cross-file, entrance, and census checks always cover the whole graph. The
/// checks against bytes need each file's snapshot; files that weren't read this update kept
/// facts that were checked against their bytes when they were extracted, so only the files
/// read now (and every input) are checked against snapshots, in a second pass over just
/// those files whose source-related findings are kept.
fn validate(
    graph: &Graph,
    snapshots: &BTreeMap<RepoPath, Snapshot>,
    all_read: bool,
    linked: &[&dyn Linked],
) -> Vec<Violation> {
    if all_read {
        return validate_graph(graph, Some(snapshots), linked);
    }
    let mut violations = validate_graph(graph, None, linked);
    let read = Graph {
        files: graph
            .files
            .iter()
            .filter(|file| file.file.state == FileState::Current && snapshots.contains_key(&file.file.path))
            .cloned()
            .collect(),
        inputs: graph.inputs.clone(),
        entrances: Vec::new(),
    };
    violations.extend(
        validate_graph(&read, Some(snapshots), &[])
            .into_iter()
            .filter(|violation| {
                matches!(
                    violation.subject,
                    Subject::File(_) | Subject::Block(_) | Subject::Slot { .. } | Subject::Input(_)
                )
            }),
    );
    violations.sort();
    violations.dedup();
    violations
}
