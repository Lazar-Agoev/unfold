//! Reopening with the store-backed facts cache (architecture §7.8): a folder whose files didn't
//! change reads and extracts nothing on reopen, whatever configuration shapes their facts
//! (TypeScript caches facts under each file's `tsconfig.json`-derived extraction options), and
//! a configuration edit made while the folder was closed re-extracts exactly the files it
//! touches.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use unfold_core::vocab::Language;
use unfold_workspace::Workspace;

use crate::common::{PATIENCE, Playground, RecordingSink, compile, copy_dir, settle};

/// `(read, from cache, extracted)` of the workspace's latest committed update.
fn last_files(workspace: &Workspace) -> (usize, usize, usize) {
    let log = workspace.index_log();
    let entry = log.last().expect("an update was committed");
    let line = entry
        .text
        .lines()
        .find_map(|line| line.strip_prefix("files: "))
        .expect("the files line");
    let count = |label: &str| -> usize {
        let start = line.find(label).expect("the label") + label.len();
        let digits: String = line[start..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().expect("a count")
    };
    (count("read "), count("from cache "), count("extracted "))
}

/// Opens the folder, waits for the reopen check, and returns `(read, from cache, extracted)`.
fn reopen(playground: &Playground, sink: &Arc<RecordingSink>) -> (Arc<Workspace>, (usize, usize, usize)) {
    let workspace = playground.open(sink, false);
    let deadline = Instant::now() + PATIENCE;
    while workspace.index_log().is_empty() {
        assert!(Instant::now() < deadline, "the reopen check never committed");
        std::thread::sleep(Duration::from_millis(10));
    }
    settle(&workspace);
    let files = last_files(&workspace);
    (workspace, files)
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

    let (workspace, files) = reopen(&playground, &sink);
    assert_eq!(files, (0, 0, 0), "no file is read or extracted again");
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
        ".unfold/config.toml",
        "version = 1\n\n[[python.project]]\nroot = \"py\"\nsource_roots = [\"src\"]\n",
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

    let (workspace, files) = reopen(&playground, &sink);
    assert_eq!(files, (0, 0, 0), "no file is read or extracted again");
    close(workspace);

    // A `tsconfig.json` edit while closed changes every TypeScript file's extraction options
    // and no Python file's settings.
    set_verbatim(&playground, false);
    let (workspace, files) = reopen(&playground, &sink);
    assert_eq!(files, (typescript, 0, typescript), "exactly the TypeScript files");
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

    let (workspace, files) = reopen(&playground, &sink);
    assert_eq!(files, (0, 0, 0));
    let entry = workspace.index_log().last().cloned().expect("the reopen check");
    assert!(entry.text.contains("graph unchanged"), "{}", entry.text);
    assert_eq!(
        (entry.commit_ms, entry.files_written),
        (0, 0),
        "nothing is committed"
    );
    assert_eq!(workspace.problems(), problems);
    assert_eq!(stored(&workspace), (compiled_at, graph.clone()));
    close(workspace);

    // A change only linking sees, made while closed, links again and commits.
    let config = std::fs::read_to_string(playground.path(".unfold/config.toml")).expect("settings");
    playground.write(
        ".unfold/config.toml",
        &format!("{config}\n[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"src/server.ts#shutdown\"\n"),
    );
    let (workspace, files) = reopen(&playground, &sink);
    assert_eq!(files.1 + files.2, 0, "no file's facts changed");
    let entry = workspace.index_log().last().cloned().expect("the reopen check");
    assert!(!entry.text.contains("graph unchanged"), "{}", entry.text);
    let (at, relinked) = stored(&workspace);
    assert!(at >= compiled_at);
    assert_ne!(relinked, graph, "the manual entrance is in the stored graph");
    close(workspace);
}
