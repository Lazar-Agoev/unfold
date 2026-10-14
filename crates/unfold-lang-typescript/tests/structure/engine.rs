//! End to end through the engine: a `tsconfig.json` change re-extracts exactly the files whose
//! options changed and is a graph input, and an incremental update after edits, deletions, and
//! additions gives the graph a full compile of the same folder gives.

use std::sync::Arc;

use unfold_core::vocab::LinkKind;
use unfold_core::{Graph, RepoPath, canonical};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, Language, UpdateRequest};
use unfold_lang_typescript::TypeScript;

fn engine(root: &std::path::Path) -> Engine {
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        ..EngineOptions::default()
    };
    let language: Arc<dyn Language> = Arc::new(TypeScript::new());
    Engine::new(root, vec![language], Arc::new(MemoryFactsCache::new()), options).expect("engine")
}

fn write(root: &std::path::Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("folders");
    std::fs::write(full, text).expect("write");
}

#[test]
fn a_tsconfig_change_re_extracts_exactly_the_files_it_affects() {
    let folder = tempfile::tempdir().expect("temporary folder");
    let root = folder.path();
    write(root, "tsconfig.json", "{}");
    write(root, "strict/tsconfig.json", r#"{"extends": "../tsconfig.json"}"#);
    write(root, "a.ts", "import {} from './b';\nexport const a = 1;");
    write(root, "b.ts", "export const b = 1;");
    write(root, "strict/c.ts", "import {} from '../b';");
    write(root, "strict/deep/d.ts", "import {} from '../../b';");
    let mut engine = engine(root);
    let imports = |graph: &Graph| -> usize {
        graph
            .files
            .iter()
            .flat_map(|file| &file.links)
            .filter(|link| link.kind == LinkKind::Imports)
            .count()
    };
    let first = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(first.stats.files_extracted, 4);
    assert_eq!(imports(&first.graph), 0, "empty imports are erased");

    // The nested configuration turns on verbatimModuleSyntax: only its two files change.
    write(
        root,
        "strict/tsconfig.json",
        r#"{"extends": "../tsconfig.json", "compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    let changed = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(changed.stats.files_extracted, 2);
    assert_eq!(imports(&changed.graph), 2);

    // Through the chain: the root configuration turns it on for everything; only `a.ts` and
    // `b.ts` change (the strict files already had it).
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    let root_changed = engine.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(root_changed.stats.files_extracted, 2);
    assert_eq!(imports(&root_changed.graph), 3);
    let inputs: Vec<&str> = root_changed
        .graph
        .inputs
        .iter()
        .map(|input| input.path.as_str())
        .collect();
    assert_eq!(inputs, ["strict/tsconfig.json", "tsconfig.json"]);

    // A missing base is a problem, not a failure.
    write(
        root,
        "strict/tsconfig.json",
        r#"{"extends": "@tsconfig/missing"}"#,
    );
    let broken = engine.update(UpdateRequest::compile()).expect("compile");
    assert!(!broken.complete);
    assert_eq!(broken.problems.len(), 1, "{:?}", broken.problems);
    assert_eq!(&*broken.problems[0].path, "strict/tsconfig.json");
}

const SERVICE: &str = "import { loadOrder, saveOrder } from './store';\n\nexport async function placeOrder(id: string) {\n  const order = await loadOrder(id);\n  switch (order.status) {\n    case 'paid':\n      return order;\n    case 'cancelled':\n      throw new Error('cancelled');\n  }\n  await saveOrder(order);\n  return order;\n}\n";

const STORE: &str = "export interface Order { id: string; status: string }\n\nexport async function loadOrder(id: string): Promise<Order> {\n  return { id, status: 'new' };\n}\n\nexport async function saveOrder(order: Order) {\n  console.log(order);\n}\n";

const ROUTES: &str = "import { Router } from 'express';\nimport { placeOrder } from './service';\n\nexport const ordersRouter = Router();\n\nordersRouter.post('/orders/:id', async (req, res) => {\n  try {\n    res.json(await placeOrder(req.params.id));\n  } catch (error) {\n    res.status(500).json({ error: String(error) });\n  }\n});\n";

#[test]
fn an_incremental_update_equals_a_full_compile() {
    let folder = tempfile::tempdir().expect("temporary folder");
    let root = folder.path();
    write(root, "src/service.ts", SERVICE);
    write(root, "src/store.ts", STORE);
    write(root, "src/routes.ts", ROUTES);
    write(root, "src/old.ts", "export function old() { return 1; }\n");
    let mut incremental = engine(root);
    let first = incremental.update(UpdateRequest::compile()).expect("compile");
    assert_eq!(first.stats.files_extracted, 4);

    // Edit a body (keys after it move), delete a file, add one that imports the others, and
    // add a function a call resolves to.
    write(
        root,
        "src/service.ts",
        &SERVICE.replace(
            "  await saveOrder(order);\n",
            "  if (order.id) {\n    await saveOrder(order);\n  }\n",
        ),
    );
    std::fs::remove_file(root.join("src/old.ts")).expect("delete");
    write(
        root,
        "src/cli.ts",
        "import { placeOrder } from './service';\nvoid placeOrder(process.argv[2]);\n",
    );
    write(
        root,
        "src/store.ts",
        &format!("{STORE}\nexport function audit(order: Order) {{\n  return saveOrder(order);\n}}\n"),
    );
    let paths = ["src/service.ts", "src/old.ts", "src/cli.ts", "src/store.ts"]
        .map(|path| RepoPath::new(path).expect("path"))
        .to_vec();
    let updated = incremental
        .update(UpdateRequest::background(paths))
        .expect("update");
    assert_eq!(updated.stats.files_extracted, 3);

    let full = engine(root).update(UpdateRequest::compile()).expect("compile");
    let export = |graph: &Graph| canonical::to_string(graph);
    let expected = export(&full.graph);
    assert_eq!(export(&updated.graph), expected);
    assert!(expected.contains("\"path\":\"src/cli.ts\""));
    assert!(!expected.contains("src/old.ts"));
}
