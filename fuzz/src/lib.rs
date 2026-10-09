//! Shared harness for the fuzz targets: building a small mixed program from fuzzer bytes, and
//! compiling it through the same stages an engine update runs (extract, link, resolve, detect,
//! assemble, validate) without touching the file system.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use unfold_core::vocab::{FileState, InputRole, Language as LanguageId, Reason};
use unfold_core::{FileGraph, FileRecord, Graph, InputRecord, RepoPath, Snapshot};
use unfold_engine::edges::program_edges;
use unfold_engine::entrances::{TargetResolver, assemble, detect};
use unfold_engine::settings::Settings;
use unfold_engine::validate::validate_graph;
use unfold_engine::{
    BlockRef, FactsSet, FileFacts, Language, Linked, Outcome, ProgramFile, ProjectInput, ProjectInputs,
    Resolver,
};
use unfold_lang_python::Python;
use unfold_lang_typescript::TypeScript;

/// At most this many files come from one input.
const MAX_FILES: usize = 6;

/// A snapshot of `text` at `path`, or `None` when the snapshot refuses the text.
pub fn snapshot(path: &str, text: &str) -> Option<Snapshot> {
    Snapshot::from_text(RepoPath::new(path).ok()?, text).ok()
}

/// Asserts that `facts` survive the facts cache's encoding unchanged.
pub fn check_facts_round_trip(facts: &FileFacts) {
    let bytes = facts.to_bytes().expect("extracted facts encode");
    let decoded = FileFacts::from_bytes(&bytes).expect("encoded facts decode");
    assert_eq!(&decoded, facts, "facts changed through the cache encoding");
}

/// Sources and configuration inputs built from fuzzer bytes.
#[derive(Debug, Default)]
pub struct Program {
    /// Indexed sources by path.
    pub sources: BTreeMap<RepoPath, Snapshot>,
    /// Configuration inputs by path.
    pub inputs: BTreeMap<RepoPath, ProjectInput>,
}

/// Splits `data` at NUL bytes into at most [`MAX_FILES`] chunks. Each chunk's first byte picks
/// what the rest is: a TypeScript source (`f<i>.ts`, `f<i>.tsx`, `src/f<i>.mts`), a Python
/// source (`f<i>.py`, `pkg/f<i>.py`), or a configuration input (`package.json`,
/// `tsconfig.json`, `pyproject.toml`; the first of each wins). Chunks that aren't UTF-8 are
/// skipped.
pub fn program_from_bytes(data: &[u8]) -> Program {
    let mut program = Program::default();
    for (index, chunk) in data.split(|&byte| byte == 0).take(MAX_FILES).enumerate() {
        let Some((&selector, rest)) = chunk.split_first() else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(rest) else {
            continue;
        };
        let (path, role) = match selector % 8 {
            0 => (format!("f{index}.ts"), None),
            1 => (format!("f{index}.tsx"), None),
            2 => (format!("f{index}.py"), None),
            3 => (format!("pkg/f{index}.py"), None),
            4 => (format!("src/f{index}.mts"), None),
            5 => ("package.json".to_owned(), Some(InputRole::Package)),
            6 => ("tsconfig.json".to_owned(), Some(InputRole::Tsconfig)),
            _ => ("pyproject.toml".to_owned(), Some(InputRole::Pyproject)),
        };
        let Some(file) = snapshot(&path, text) else {
            continue;
        };
        let path = file.path().clone();
        match role {
            Some(role) => {
                program
                    .inputs
                    .entry(path)
                    .or_insert(ProjectInput { role, snapshot: file });
            }
            None => {
                program.sources.insert(path, file);
            }
        }
    }
    program
}

/// No launch target exists outside the program, and no unindexed file.
struct NoFolder;

impl TargetResolver for NoFolder {
    fn is_unindexed_file(&self, _path: &RepoPath) -> bool {
        false
    }

    fn launch(&self, _path: &str, _cwd: &str) -> Outcome {
        Outcome::unresolved(Reason::NotFound)
    }
}

/// Compiles `program` as an update would. Returns a description of anything the backend
/// reported as an internal error, and of every violation of the resulting graph.
pub fn compile(program: &Program) -> Result<Graph, String> {
    let settings = Settings::default();
    let project = ProjectInputs {
        settings: settings.clone(),
        inputs: program.inputs.clone(),
        ..ProjectInputs::default()
    };
    let languages: [&dyn Language; 2] = [&TypeScript::new(), &Python::new()];

    let mut records = BTreeMap::new();
    let mut files = Vec::new();
    for (path, snapshot) in &program.sources {
        let Some(language) = languages
            .iter()
            .find(|language| language.owns(path, &settings))
        else {
            continue;
        };
        let options = language
            .extraction_options(&[path], &settings, &project)
            .pop()
            .ok_or("no extraction options")?;
        let (facts, record) = match language.extract_with(snapshot, &settings, &options) {
            Ok(facts) => {
                check_facts_round_trip(&facts);
                let record = FileRecord {
                    path: path.clone(),
                    language: language.id(),
                    state: FileState::Current,
                    hash: Some(snapshot.hash()),
                    failure: None,
                };
                (Some(Arc::new(facts)), record)
            }
            Err(failure) => {
                let record = FileRecord {
                    path: path.clone(),
                    language: language.id(),
                    state: FileState::Failed,
                    hash: None,
                    failure: Some(failure.to_string().into()),
                };
                (None, record)
            }
        };
        records.insert(path.clone(), record);
        files.push(ProgramFile {
            path: path.clone(),
            language: language.id(),
            facts,
        });
    }
    let facts = FactsSet::new(files).map_err(|error| error.to_string())?;

    let linked: Vec<Box<dyn Linked + '_>> = languages
        .iter()
        .map(|language| language.link(&facts, &project))
        .collect();
    let linked_refs: Vec<&dyn Linked> = linked.iter().map(|linked| linked.as_ref()).collect();
    let resolvers: Vec<Resolver<'_>> = linked_refs
        .iter()
        .map(|linked| Resolver::new(&facts, *linked))
        .collect();
    let mut edges = vec![Vec::new(); facts.files().len()];
    for resolver in &resolvers {
        for (file, file_edges) in program_edges(resolver).map_err(|error| format!("resolve: {error}"))? {
            edges[file.index()] = file_edges;
        }
    }
    let candidates = resolvers
        .iter()
        .flat_map(|resolver| detect(resolver, &settings))
        .collect();
    let resolver_refs: Vec<&Resolver<'_>> = resolvers.iter().collect();
    let entrances = assemble(&facts, &resolver_refs, &NoFolder, candidates)
        .map_err(|error| format!("assemble: {error}"))?;

    let mut file_graphs = Vec::with_capacity(facts.files().len());
    for ((id, file), edges) in facts.iter().zip(edges) {
        let record = records[&file.path].clone();
        let Some(file_facts) = &file.facts else {
            file_graphs.push(FileGraph {
                file: record,
                blocks: Vec::new(),
                slots: Vec::new(),
                edges: Vec::new(),
            });
            continue;
        };
        let owner = linked_refs
            .iter()
            .find(|linked| linked.language() == file.language);
        let graph = file_facts
            .graph
            .linked_file_graph(record, edges, |local| {
                owner.is_some_and(|linked| linked.keeps_import(BlockRef { file: id, local }))
            })
            .map_err(|error| format!("conditional imports of {}: {error}", file.path))?;
        file_graphs.push(graph);
    }

    let mut read: BTreeSet<RepoPath> = linked_refs
        .iter()
        .flat_map(|linked| linked.inputs_read())
        .collect();
    read.extend(
        entrances
            .iter()
            .filter(|entrance| entrance.evidence.pointer.is_some())
            .map(|entrance| entrance.evidence.path.clone()),
    );
    let inputs = read
        .into_iter()
        .filter_map(|path| {
            let input = project.inputs.get(&path)?;
            Some(InputRecord {
                role: input.role,
                hash: input.snapshot.hash(),
                path,
            })
        })
        .collect();
    let mut graph = Graph {
        files: file_graphs,
        inputs,
        entrances,
    };
    graph.normalize();

    let mut snapshots = program.sources.clone();
    snapshots.extend(
        program
            .inputs
            .iter()
            .map(|(path, input)| (path.clone(), input.snapshot.clone())),
    );
    let violations = validate_graph(&graph, Some(&snapshots), &linked_refs);
    if violations.is_empty() {
        Ok(graph)
    } else {
        let listed: Vec<String> = violations.iter().map(ToString::to_string).collect();
        Err(format!("violations:\n{}", listed.join("\n")))
    }
}

/// Links and resolves decoded (possibly corrupt) facts as a one-file TypeScript program and as
/// a one-file Python program. Errors are fine; panics are not.
pub fn link_decoded(facts: FileFacts) {
    let facts = Arc::new(facts);
    let settings = Settings::default();
    let project = ProjectInputs::default();
    let typescript = TypeScript::new();
    let python = Python::new();
    let cases: [(&str, LanguageId, &dyn Language); 2] = [
        ("a.ts", LanguageId::TypeScript, &typescript),
        ("pkg/a.py", LanguageId::Python, &python),
    ];
    for (path, language_id, language) in cases {
        let Ok(path) = RepoPath::new(path) else {
            continue;
        };
        let Ok(program) = FactsSet::new([ProgramFile {
            path,
            language: language_id,
            facts: Some(Arc::clone(&facts)),
        }]) else {
            continue;
        };
        let linked = language.link(&program, &project);
        let resolver = Resolver::new(&program, linked.as_ref());
        let _edges = program_edges(&resolver);
        let candidates = detect(&resolver, &settings);
        let _entrances = assemble(&program, &[&resolver], &NoFolder, candidates);
    }
}
