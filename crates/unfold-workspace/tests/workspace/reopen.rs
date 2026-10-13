//! Reopening with the store-backed facts cache (docs/engine.md#project-lifecycle): a folder whose files didn't
//! change extracts nothing on reopen, whatever configuration shapes their facts
//! (TypeScript caches facts under each file's `tsconfig.json`-derived extraction options), and
//! a configuration edit made while the folder was closed re-extracts exactly the files it
//! touches.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use unfold_core::vocab::Language;
use unfold_workspace::Workspace;

use crate::common::{Playground, RecordingSink, compile, copy_dir, settle};

/// Opens the folder, waits for the reopen check, and returns how many files it extracted.
fn reopen(playground: &Playground, sink: &Arc<RecordingSink>) -> (Arc<Workspace>, usize) {
    let extracted = Arc::new(AtomicUsize::new(0));
    let workspace = playground.open_counted(sink, &extracted);
    settle(&workspace);
    (workspace, extracted.load(Ordering::Relaxed))
}

/// Sets `verbatimModuleSyntax` in the playground's `tsconfig.json`, an option that shapes
/// extracted facts (unlike the playground's own options, which match the defaults).
fn set_verbatim(playground: &Playground, on: bool) {
    let path = playground.path("tsconfig.json");
    let text = std::fs::read_to_string(&path).expect("tsconfig");
    let (from, to) = if on {
        (
            "\"strict\": true,",
            "\"strict\": true,\n    \"verbatimModuleSyntax\": true,",
        )
    } else {
        (
            "\"verbatimModuleSyntax\": true,",
            "\"verbatimModuleSyntax\": false,",
        )
    };
    assert!(text.contains(from), "the tsconfig has {from}");
    playground.write("tsconfig.json", &text.replace(from, to));
}

fn close(workspace: Arc<Workspace>) {
    workspace.close();
    drop(workspace);
}

#[test]
fn reopening_an_unchanged_typescript_folder_extracts_nothing() {
    let playground = Playground::new("ts-orders", true);
    set_verbatim(&playground, true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    close(workspace);

    let (workspace, extracted) = reopen(&playground, &sink);
    assert_eq!(extracted, 0, "no file is extracted again");
    close(workspace);
}

#[test]
fn reopening_a_mixed_folder_re_extracts_only_what_its_configuration_touches() {
    let playground = Playground::new("ts-orders", false);
    // The mixed folder: the TypeScript playground at the root (its `tsconfig.json` applies to
    // every TypeScript file) and the Python playground's sources beside it.
    let python = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/playgrounds/py-orders/src");
    copy_dir(&python, &playground.path("py/src"));
    playground.write(
        "unfold.toml",
        "[[python.project]]\nroot = \"py\"\nsource_roots = [\"src\"]\n",
    );
    set_verbatim(&playground, true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let reader = workspace.readers().expect("readers").get().expect("reader");
    let rows = reader.files().expect("files");
    drop(reader);
    let typescript = rows
        .iter()
        .filter(|row| row.language == Language::TypeScript)
        .count();
    assert!(
        typescript > 10 && typescript < rows.len(),
        "both languages are indexed"
    );
    close(workspace);

    let (workspace, extracted) = reopen(&playground, &sink);
    assert_eq!(extracted, 0, "no file is extracted again");
    close(workspace);

    // A `tsconfig.json` edit while closed changes every TypeScript file's extraction options
    // and no Python file's settings.
    set_verbatim(&playground, false);
    let (workspace, extracted) = reopen(&playground, &sink);
    assert_eq!(extracted, typescript, "exactly the TypeScript files");
    close(workspace);
}

/// The stored graph's compile time and the canonical text of the graph readers see.
fn stored(workspace: &Workspace) -> (i64, String) {
    let reader = workspace.readers().expect("readers").get().expect("reader");
    let meta = reader.meta().expect("meta").graph.expect("a graph");
    let graph = reader.graph().expect("graph");
    (meta.compiled_at_ms, unfold_core::canonical::to_string(&graph))
}

#[test]
fn reopening_an_unchanged_folder_neither_links_nor_commits() {
    let playground = Playground::new("ts-orders", true);
    // What linking finds (a package manifest it can't read) survives a reopen that doesn't link.
    playground.write("tools/package.json", "[]\n");
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let problems = workspace.problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.path == "tools/package.json"),
        "{problems:?}"
    );
    let (compiled_at, graph) = stored(&workspace);
    close(workspace);

    let (workspace, extracted) = reopen(&playground, &sink);
    assert_eq!(extracted, 0);
    assert_eq!(workspace.problems(), problems);
    assert_eq!(
        stored(&workspace),
        (compiled_at, graph.clone()),
        "nothing is committed"
    );
    close(workspace);

    // A change only linking sees, made while closed, links again and commits.
    let config = std::fs::read_to_string(playground.path("unfold.toml")).expect("settings");
    playground.write(
        "unfold.toml",
        &format!("{config}\n[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"src/server.ts#shutdown\"\n"),
    );
    let (workspace, extracted) = reopen(&playground, &sink);
    assert_eq!(extracted, 0, "no file's facts changed");
    let (at, relinked) = stored(&workspace);
    assert!(at >= compiled_at);
    assert_ne!(relinked, graph, "the manual entrance is in the stored graph");
    close(workspace);
}
