//! Evidence files (`[evidence] files`): hashed and recorded as inputs with the role `evidence`,
//! never parsed, never read by linking, so changing one never relinks.

use std::sync::Arc;

use unfold_core::vocab::InputRole;
use unfold_core::{ContentHash, Graph, Input};
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{UpdateOutcome, UpdateRequest};

use super::{Counting, Folder, background, canonical, compile, engine, engine_with, template};

const SETTINGS: &str = "[evidence]\nfiles = [\"db\", \"openapi.yaml\"]\n";

fn project(folder: &Folder) {
    folder.write("main.ts", &template(1, 0));
    folder.write("lib/util.ts", &template(0, 1));
    folder.write("unfold.toml", SETTINGS);
    folder.write("db/schema.sql", "create table orders (id text primary key);\n");
}

/// The evidence inputs of a graph, as (path, hash).
fn evidence(graph: &Graph) -> Vec<(&str, ContentHash)> {
    graph
        .inputs
        .iter()
        .filter(|input| input.role == InputRole::Evidence)
        .map(|Input { path, hash, .. }| (path.as_str(), *hash))
        .collect()
}

fn hash(text: &str) -> ContentHash {
    ContentHash::of(text.as_bytes())
}

fn relinked(outcome: &UpdateOutcome) -> bool {
    outcome.stats.relinked
}

#[test]
fn evidence_files_are_hashed_inputs_and_changing_one_never_relinks() {
    let folder = Folder::new();
    project(&folder);
    let mut engine = engine(&folder);
    let first = compile(&mut engine);
    assert!(first.complete);
    assert_eq!(
        evidence(&first.graph),
        [(
            "db/schema.sql",
            hash("create table orders (id text primary key);\n")
        )]
    );
    assert!(
        first.graph.file("db/schema.sql").is_none(),
        "evidence is never a source"
    );

    let edited = "create table orders (id text primary key, customer_id text);\n";
    folder.write("db/schema.sql", edited);
    let changed = background(&mut engine, &["db/schema.sql"]);
    assert!(!relinked(&changed), "evidence shapes nothing linking reads");
    assert!(changed.summary.inputs_changed);
    assert!(changed.summary.changed_files.is_empty() && changed.summary.changed_keys.is_empty());
    assert_eq!(evidence(&changed.graph), [("db/schema.sql", hash(edited))]);
    assert_eq!(changed.graph.files, first.graph.files);

    folder.write("openapi.yaml", "openapi: 3.1.0\n");
    let added = background(&mut engine, &["openapi.yaml"]);
    assert!(!relinked(&added));
    assert_eq!(
        evidence(&added.graph),
        [
            ("db/schema.sql", hash(edited)),
            ("openapi.yaml", hash("openapi: 3.1.0\n"))
        ]
    );

    folder.remove("db/schema.sql");
    let removed = background(&mut engine, &["db/schema.sql"]);
    assert!(!relinked(&removed));
    assert_eq!(
        evidence(&removed.graph),
        [("openapi.yaml", hash("openapi: 3.1.0\n"))]
    );

    // The same graph a fresh compile makes.
    let mut fresh = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2);
    assert_eq!(canonical(&removed), canonical(&compile(&mut fresh)));

    // A pattern is a setting: dropping it drops the file's record.
    folder.write("unfold.toml", "[evidence]\nfiles = [\"db\"]\n");
    let unlisted = background(&mut engine, &["unfold.toml"]);
    assert!(evidence(&unlisted.graph).is_empty());
    assert!(unlisted.summary.inputs_changed);
}

/// An evidence file changed while the folder was closed: the reopen check keeps the restored
/// graph (nothing linking reads changed) and takes the file's new hash.
#[test]
fn a_restored_graph_takes_new_evidence_without_linking_again() {
    let folder = Folder::new();
    project(&folder);
    let cache = Arc::new(MemoryFactsCache::new());
    let mut engine = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    let first = compile(&mut engine);
    let graph: Graph = (*first.graph).clone();
    let disks: Vec<_> = graph
        .files
        .iter()
        .filter_map(|file| Some((file.file.path.clone(), engine.disk_state(&file.file.path)?)))
        .collect();
    folder.write("db/schema.sql", "create table customers (id text);\n");

    let mut reopened = engine_with(&folder, Counting::new(), Arc::clone(&cache), 2);
    reopened
        .restore(graph, disks, Some(first.basis.clone()))
        .expect("restore");
    let outcome = reopened.update(UpdateRequest::reopen()).expect("reopen check");
    assert!(!relinked(&outcome), "the restored graph is kept");
    assert!(outcome.summary.inputs_changed);
    assert_eq!(
        evidence(&outcome.graph),
        [("db/schema.sql", hash("create table customers (id text);\n"))]
    );
    assert_eq!(
        outcome.basis, first.basis,
        "evidence isn't part of what linking read"
    );
    let mut fresh = engine_with(&folder, Counting::new(), Arc::new(MemoryFactsCache::new()), 2);
    assert_eq!(canonical(&outcome), canonical(&compile(&mut fresh)));
}

/// Linking reads no `[evidence]` pattern, and patterns match alike in any order: editing
/// evidence patterns, or reordering any pattern list, never relinks. A real scope change does.
#[test]
fn only_a_scope_change_relinks() {
    let folder = Folder::new();
    project(&folder);
    folder.write("docs/api.md", "# API\n");
    folder.write("vendor/lib.ts", &template(2, 0));
    let settings = |exclude: &str, evidence: &str| {
        format!("[scope]\nexclude = [{exclude}]\n\n[evidence]\nfiles = [{evidence}]\n")
    };
    folder.write("unfold.toml", &settings("\"vendor\", \"gen\"", "\"db\""));
    let mut engine = engine(&folder);
    let first = compile(&mut engine);
    let settings_hash = engine.settings().hash();

    folder.write(
        "unfold.toml",
        &settings("\"vendor\", \"gen\"", "\"db\", \"docs\""),
    );
    let evidence_edit = background(&mut engine, &["unfold.toml"]);
    assert!(!relinked(&evidence_edit), "an evidence pattern edit");
    assert_eq!(
        evidence(&evidence_edit.graph),
        [
            (
                "db/schema.sql",
                hash("create table orders (id text primary key);\n")
            ),
            ("docs/api.md", hash("# API\n"))
        ],
        "the new pattern's files are found"
    );
    assert_eq!(engine.settings().hash(), settings_hash);

    folder.write(
        "unfold.toml",
        &settings("\"gen\", \"vendor\"", "\"docs\", \"db\""),
    );
    let reordered = background(&mut engine, &["unfold.toml"]);
    assert!(!relinked(&reordered), "reordered patterns");
    assert!(!reordered.stats.full_discovery, "the settings are the same");
    assert_eq!(engine.settings().hash(), settings_hash);
    assert_eq!(reordered.graph.files, first.graph.files);

    folder.write("unfold.toml", &settings("\"gen\"", "\"docs\", \"db\""));
    let scoped = background(&mut engine, &["unfold.toml"]);
    assert!(relinked(&scoped), "a scope change");
    assert_ne!(engine.settings().hash(), settings_hash);
    assert!(scoped.graph.file("vendor/lib.ts").is_some());
}
