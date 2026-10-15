//! The link, resolve, and validate stages: the whole program from its per-file facts to a
//! validated graph, in memory, without parsing or reading anything.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;

use unfold_core::validate::{Subject, Violation};
use unfold_core::vocab::FileState;
use unfold_core::{FileGraph, FileTypes, Graph, Input, RepoPath, Snapshot};

use super::run::evidence_records;
use super::state::State;
use super::{Engine, Stage, UpdateError, UpdateStats};
use crate::cancel::CancelToken;
use crate::language::{Language, LanguageProblem, Linked, ProjectInputs};
use crate::links::{FileLinks, program_links};
use crate::program::{FactsSet, ProgramFile};
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
    /// graph can be checked against its bytes in one pass.
    pub(crate) all_read: bool,
    pub(crate) cancel: &'a CancelToken,
}

/// What the build stages produce: the validated graph and the problems linking found.
pub(crate) struct Built {
    pub(crate) graph: Graph,
    /// Every language's [`Linked::problems`], in language order.
    pub(crate) problems: Vec<LanguageProblem>,
}

/// Links, resolves, and validates; times each stage into `stats`.
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
    let mut links: Vec<FileLinks> = vec![FileLinks::default(); program.files().len()];
    for linked in &linked {
        let resolver = Resolver::new(&program, linked.as_ref());
        let resolved = program_links(&resolver, || cancel.check().map_err(UpdateError::from))?;
        for (file, file_links) in resolved {
            links[file.index()] = file_links;
        }
    }
    stats.stages.insert(Stage::Resolve, started.elapsed());
    cancel.check()?;

    // File IDs follow path order, as does the state's map, so the two line up.
    let entries: Vec<_> = state.files.values().collect();
    let files: Vec<Arc<FileGraph>> = program
        .files()
        .par_iter()
        .zip(entries)
        .zip(links)
        .map(|((file, entry), links)| {
            cancel.check()?;
            let record = entry.record(&file.path);
            Ok(Arc::new(match &file.facts {
                Some(facts) => facts.graph.file_graph(record, links),
                None => FileGraph {
                    file: record,
                    nodes: Vec::new(),
                    links: Vec::new(),
                    types: FileTypes::default(),
                },
            }))
        })
        .collect::<Result<_, UpdateError>>()?;
    let linked_refs: Vec<&dyn Linked> = linked.iter().map(|linked| linked.as_ref()).collect();
    // The apps are resolved against the validated graph, and checked as they are attached.
    let mut graph = Graph {
        files,
        inputs: graph_inputs(state, project, &linked_refs),
        apps: Vec::new(),
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

/// The inputs that shaped the graph: the settings file when settings came from it and what each
/// language says it read (paths that aren't among the project inputs are ignored); then every
/// evidence file that could be read.
fn graph_inputs(state: &State, project: &ProjectInputs, linked: &[&dyn Linked]) -> Vec<Input> {
    let mut paths: BTreeSet<RepoPath> = linked.iter().flat_map(|linked| linked.inputs_read()).collect();
    if let Some(source) = &state.settings.source {
        debug_assert_eq!(source.path().as_str(), SETTINGS_PATH);
        paths.insert(source.path().clone());
    }
    paths
        .into_iter()
        .filter_map(|path| {
            let input = project.inputs.get(&path)?;
            Some(Input {
                role: input.role,
                hash: input.snapshot.hash(),
                path,
            })
        })
        .chain(evidence_records(&state.inputs))
        .collect()
}

/// Every violation of `graph`.
///
/// The structural, cross-file, and census checks always cover the whole graph. The
/// checks against bytes need each file's snapshot; files that weren't read this update kept
/// facts that were checked against their bytes when they were extracted, so only the files
/// read now (and every input) are checked against snapshots, in a second pass over just
/// those files whose findings on files, nodes, and inputs are kept (their links and typed
/// records may name nodes of files outside the pass).
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
        apps: Vec::new(),
    };
    violations.extend(
        validate_graph(&read, Some(snapshots), &[])
            .into_iter()
            .filter(|violation| {
                matches!(
                    violation.subject,
                    Subject::File(_) | Subject::Node(_) | Subject::Input(_)
                )
            }),
    );
    violations.sort();
    violations.dedup();
    violations
}
