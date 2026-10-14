//! The `py-orders` playground's goldens: a temporary copy of `testdata/playgrounds/py-orders`
//! compiled through the engine, exactly as the app compiles it, written as
//!
//! - `testdata/goldens/py-orders.jsonl`, the graph's canonical export; and
//! - `testdata/goldens/py-orders.entrances.txt`, the state of every `code` key of its
//!   `[[app]]` tables.
//!
//! The entrance file is UTF-8 text with one record per line and tab-separated fields; an
//! empty field is `-`. Each app is followed by its own `code`, then by its entrances, each
//! followed by its `code` keys in order:
//!
//! ```text
//! app<TAB><app key><TAB><run>
//! <TAB>code<TAB><key><TAB><state>[<TAB><fix kind><TAB><fix key>]
//! entrance<TAB><entrance key><TAB><on>
//! <TAB>code<TAB><key><TAB><state>[<TAB><fix kind><TAB><fix key>]
//! ```
//!
//! `state` is `found`, `missing`, `not_analyzed`, `not_a_start`, or `stale`; a missing key
//! with one candidate adds its fix: `moved_definition`, `edited_route`, or `moved_module`, and
//! the key to write instead. The playground keeps one deliberately missing key with a fix.
//!
//! `UNFOLD_BLESS=1` rewrites both; every change to them is reviewed by hand.

use std::fmt::Write as _;
use std::sync::Arc;

use unfold_core::canonical;
use unfold_core::testkit::assert_golden;
use unfold_core::{App, EntranceTarget, Graph};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::entrances::{FixKind, Fixes};
use unfold_engine::{Engine, EngineOptions, MemoryFactsCache, UpdateRequest};
use unfold_lang_python::Python;

use crate::common::{copy_folder, testdata};

/// The playground compiled through the engine on `threads` threads: its graph and the fixes of
/// its missing `code` keys.
fn compile_playground(threads: usize) -> (Arc<Graph>, Fixes) {
    let folder = tempfile::tempdir().expect("a temporary directory");
    copy_folder(&testdata().join("playgrounds/py-orders"), folder.path());
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
        vec![Arc::new(Python)],
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("an engine for the playground");
    let outcome = engine
        .update(UpdateRequest::compile())
        .expect("the playground compiles");
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
    (outcome.graph, engine.fixes().clone())
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

#[test]
fn the_playground_matches_its_goldens() {
    let (graph, fixes) = compile_playground(1);
    let export = canonical::to_string(&graph);
    let (again, _) = compile_playground(4);
    assert!(
        export == canonical::to_string(&again),
        "the export doesn't depend on the thread count"
    );
    assert_golden("goldens/py-orders.jsonl", &export);
    assert_golden(
        "goldens/py-orders.entrances.txt",
        &entrance_states(&graph.apps, &fixes),
    );
}
