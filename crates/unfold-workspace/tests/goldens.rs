//! The goldens that need both languages, from temporary copies compiled through the engine with
//! every language the workspace registers, exactly as the app compiles them:
//!
//! - the mixed playground (`testdata/playgrounds`, `ts-orders` and `py-orders` together):
//!   `testdata/goldens/mixed-playground.jsonl`, the graph's canonical export, and
//!   `testdata/goldens/mixed-playground.entrances.txt`, the state of every `code` key of its
//!   `[[app]]` tables;
//! - the typed-lines fixture (`testdata/fixtures/typed-lines`): `testdata/goldens/typed-lines.jsonl`,
//!   its canonical export, and `testdata/goldens/typed-lines.flows.txt`, every definition drawn
//!   with no overlay and printed with the reader's `flow_text`.
//!
//! The formats are the per-language goldens' (`crates/unfold-lang-*/tests/*/goldens.rs`, and
//! the reader's playground flows). The entrance file is UTF-8 text with one record per line and
//! tab-separated fields; an empty field is `-`. Each app is followed by its own `code`, then by
//! its entrances, each followed by its `code` keys in order:
//!
//! ```text
//! app<TAB><app key><TAB><run>
//! <TAB>code<TAB><key><TAB><state>[<TAB><fix kind><TAB><fix key>]
//! entrance<TAB><entrance key><TAB><on>
//! <TAB>code<TAB><key><TAB><state>[<TAB><fix kind><TAB><fix key>]
//! ```
//!
//! The flows file lists each definition in file and preorder order as `=== <key>`, then its
//! drawn flow, then a blank line.
//!
//! `UNFOLD_BLESS=1` rewrites them; every change to them is reviewed by hand.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_core::{App, EntranceTarget, FileGraph, Graph, canonical};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::entrances::{FixKind, Fixes};
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, UpdateRequest};
use unfold_reader::drawing::{Card, Overlay, draw, flow_text};
use unfold_reader::{FileTree, Trees};
use unfold_workspace::default_languages;

/// A compiled copy: the graph, the fixes of its missing `code` keys, and the copy its sources
/// are verified in.
struct Compiled {
    graph: Arc<Graph>,
    fixes: Fixes,
    folder: TempDir,
}

/// Copies `from` into `to`, without `.unfold/local/`.
fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create folder");
    for entry in std::fs::read_dir(from).expect("read folder") {
        let entry = entry.expect("folder entry");
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            if source.ends_with(".unfold/local") {
                continue;
            }
            copy_folder(&source, &target);
        } else {
            std::fs::copy(&source, &target).expect("copy file");
        }
    }
}

/// `testdata/<relative>` copied and compiled through the engine on `threads` threads, with no
/// problems in the update or in `unfold.toml`.
fn compile(relative: &str, threads: usize) -> Compiled {
    let folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&testdata_dir().join(relative), folder.path());
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: threads,
        ..EngineOptions::default()
    };
    let mut engine = Engine::new(
        folder.path(),
        default_languages(),
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("an engine for the folder");
    let outcome = engine
        .update(UpdateRequest::compile())
        .expect("the folder compiles");
    assert!(outcome.complete, "{relative} compiles completely");
    assert!(outcome.problems.is_empty(), "problems: {:?}", outcome.problems);
    let problems: Vec<_> = engine
        .app_entries()
        .iter()
        .flat_map(|app| {
            app.problem.iter().chain(
                app.entrances
                    .iter()
                    .filter_map(|entrance| entrance.problem.as_ref()),
            )
        })
        .collect();
    assert!(problems.is_empty(), "entry problems: {problems:?}");
    Compiled {
        graph: outcome.graph,
        fixes: engine.fixes().clone(),
        folder,
    }
}

/// The canonical export of `relative`, checked not to depend on the thread count.
fn export(relative: &str) -> (Compiled, String) {
    let compiled = compile(relative, 1);
    let export = canonical::to_string(&compiled.graph);
    assert!(
        export == canonical::to_string(&compile(relative, 4).graph),
        "the export of {relative} doesn't depend on the thread count"
    );
    (compiled, export)
}

/// The entrance file's lines for `apps` (see the module documentation).
fn entrance_states(apps: &[App], fixes: &Fixes) -> String {
    let code = |out: &mut String, target: &EntranceTarget| {
        let _ = write!(out, "\tcode\t{}\t{}", target.code, target.state);
        if let Some(fix) = fixes.get(target) {
            let kind = match fix.kind {
                FixKind::MovedDefinition => "moved_definition",
                FixKind::EditedRoute => "edited_route",
                FixKind::MovedModule => "moved_module",
            };
            let _ = write!(out, "\t{kind}\t{}", fix.code);
        }
        out.push('\n');
    };
    let mut out = String::new();
    for app in apps {
        let _ = writeln!(out, "app\t{}\t{}", app.key, app.run.as_deref().unwrap_or("-"));
        if let Some(target) = &app.code {
            code(&mut out, target);
        }
        for entrance in &app.entrances {
            let _ = writeln!(
                out,
                "entrance\t{}\t{}",
                entrance.key,
                entrance.on.as_deref().unwrap_or("-")
            );
            for target in &entrance.code {
                code(&mut out, target);
            }
        }
    }
    out
}

/// Every definition of the compiled graph, drawn with no overlay and printed as agent text.
fn flows(compiled: &Compiled) -> String {
    let mut trees = Trees::new();
    for file in &compiled.graph.files {
        trees.insert(FileTree::new(FileGraph::clone(file)));
    }
    let overlay = Overlay::default();
    let mut out = String::new();
    for file in &compiled.graph.files {
        for node in file.nodes.iter().filter(|node| node.kind().is_definition()) {
            let card = Card {
                definition: node.key.clone(),
                open: Vec::new(),
                show_all_calls: false,
            };
            let flow = draw(&trees, compiled.folder.path(), &overlay, &card).expect("a definition draws");
            out.push_str("=== ");
            out.push_str(node.key.as_str());
            out.push('\n');
            out.push_str(&flow_text(&flow));
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_mixed_playground_matches_its_goldens() {
    let (compiled, export) = export("playgrounds");
    assert_golden("goldens/mixed-playground.jsonl", &export);
    assert_golden(
        "goldens/mixed-playground.entrances.txt",
        &entrance_states(&compiled.graph.apps, &compiled.fixes),
    );
}

#[test]
fn the_typed_lines_fixture_matches_its_goldens() {
    let (compiled, export) = export("fixtures/typed-lines");
    assert_golden("goldens/typed-lines.jsonl", &export);
    assert_golden("goldens/typed-lines.flows.txt", &flows(&compiled));
}
