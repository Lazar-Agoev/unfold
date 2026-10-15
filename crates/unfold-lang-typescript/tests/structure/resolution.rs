//! Module resolution through `imports` links: relative paths and extension rules, packages and
//! Node built-ins, `paths` and `baseUrl`, in-repository packages and their `exports`, and the
//! choice between `not_found`, `not_indexed`, and a path label.

use unfold_core::vocab::{LinkKind, Outcome, Reason};
use unfold_core::{LinkTarget, ModuleLabel};

use crate::support::{Project, outcome};

const MODULE: &str = r#"{"type": "module"}"#;

#[track_caller]
fn assert_resolved(project: &Project, path: &str, text: &str, target: &str) {
    let compiled = project.compile();
    let link = compiled.import(path, text);
    assert_eq!(
        link.to,
        LinkTarget::Project(unfold_core::Key::from_text(&format!("{target}#"))),
        "{text}"
    );
}

#[track_caller]
fn assert_unresolved(project: &Project, path: &str, text: &str, reason: Reason) {
    let compiled = project.compile();
    let link = compiled.import(path, text);
    assert_eq!(link.to, LinkTarget::Unknown(reason), "{text}");
}

#[track_caller]
fn assert_external(project: &Project, path: &str, text: &str, module: &str) {
    let compiled = project.compile();
    let link = compiled.import(path, text);
    assert_eq!(
        link.to,
        LinkTarget::Library(ModuleLabel::single(module).expect("a label")),
        "{text}"
    );
}

#[test]
fn relative_imports_follow_typescripts_extension_rules() {
    let project = Project::new()
        .source(
            "src/a.ts",
            "import './b.js';\nimport './b';\nimport './c.mjs';\nimport './dir';\nimport './b.ts';\nimport '../src/b.js';\n",
        )
        .source("src/b.ts", "export const b = 1;")
        .source("src/c.mts", "export const c = 1;")
        .source("src/dir/index.ts", "export {};");
    // CommonJS mode (no package `"type"`): extensions and `index` are probed.
    assert_resolved(&project, "src/a.ts", "./b.js", "src/b.ts");
    assert_resolved(&project, "src/a.ts", "./b", "src/b.ts");
    assert_resolved(&project, "src/a.ts", "./c.mjs", "src/c.mts");
    assert_resolved(&project, "src/a.ts", "./dir", "src/dir/index.ts");
    assert_resolved(&project, "src/a.ts", "./b.ts", "src/b.ts");
    assert_resolved(&project, "src/a.ts", "../src/b.js", "src/b.ts");

    // ESM mode under NodeNext needs the extension, and never probes `index`.
    let esm = project.input("package.json", MODULE);
    assert_resolved(&esm, "src/a.ts", "./b.js", "src/b.ts");
    assert_unresolved(&esm, "src/a.ts", "./b", Reason::NotFound);
    assert_unresolved(&esm, "src/a.ts", "./dir", Reason::NotFound);
    assert_resolved(&esm, "src/a.ts", "./b.ts", "src/b.ts");
}

#[test]
fn import_kinds_pick_the_resolution_mode() {
    let project = Project::new()
        .input("package.json", MODULE)
        .source(
            "a.ts",
            "import b = require('./b');\nconst c = import('./b');\nimport('./b.js');\n",
        )
        .source("b.ts", "export = 1;")
        .source("lib/c.cts", "import './d';")
        .source("lib/d.ts", "");
    // `import x = require` is CommonJS even in an ESM package; a dynamic import is ESM.
    assert_resolved(&project, "a.ts", "./b", "b.ts");
    let compiled = project.compile();
    let dynamic: Vec<_> = compiled
        .file("a.ts")
        .links
        .iter()
        .filter(|link| &*link.text == "./b" || &*link.text == "./b.js")
        .map(|link| (link.text.to_string(), link.to.outcome()))
        .collect();
    assert_eq!(
        dynamic,
        [
            ("./b".to_owned(), Outcome::Project),
            ("./b".to_owned(), Outcome::Unknown),
            ("./b.js".to_owned(), Outcome::Project),
        ]
    );
    // A `.cts` file is CommonJS whatever the package says.
    assert_resolved(&project, "lib/c.cts", "./d", "lib/d.ts");
}

#[test]
fn dynamic_imports_follow_the_extension_rules_of_static_ones() {
    let project = Project::new()
        .source(
            "src/a.ts",
            "import { hello } from './names';\nimport 'dual';\nexport async function main() {\n  const { hello: again } = await import('./names');\n  again();\n  await import('./names.ts');\n  await import('./names.js');\n  await import('./dir');\n  await import('dual');\n}\n",
        )
        .source("src/names.ts", "export function hello() {}")
        .source("src/dir/index.ts", "export {};")
        .input(
            "packages/dual/package.json",
            r#"{"name": "dual", "exports": {"import": "./esm.ts", "require": "./cjs.ts"}}"#,
        )
        .source("packages/dual/esm.ts", "export {};")
        .source("packages/dual/cjs.ts", "export {};");
    let imports = |project: &Project| -> Vec<String> {
        let compiled = project.compile();
        compiled
            .file("src/a.ts")
            .links
            .iter()
            .filter(|link| link.kind == LinkKind::Imports)
            .map(|link| format!("{} {}", link.text, outcome(&link.to)))
            .collect()
    };
    // Without a configuration the file is CommonJS: a dynamic import probes extensions and
    // `index` as its static imports do, with the conditions of an ESM import, and a binding
    // destructured from it reads the module.
    assert_eq!(
        imports(&project),
        [
            "./names project src/names.ts#",
            "dual project packages/dual/cjs.ts#",
            "./names project src/names.ts#",
            "./names.ts project src/names.ts#",
            "./names.js project src/names.ts#",
            "./dir project src/dir/index.ts#",
            "dual project packages/dual/esm.ts#",
        ]
    );
    let compiled = project.compile();
    let again = compiled
        .file("src/a.ts")
        .links
        .iter()
        .find(|link| link.kind == LinkKind::Calls && &*link.text == "again")
        .expect("the call to `again`");
    assert_eq!(outcome(&again.to), "project src/names.ts#hello");
    // In an ESM package both need the extension.
    assert_eq!(
        imports(&project.input("package.json", MODULE)),
        [
            "./names unknown not_found",
            "dual project packages/dual/esm.ts#",
            "./names unknown not_found",
            "./names.ts project src/names.ts#",
            "./names.js project src/names.ts#",
            "./dir unknown not_found",
            "dual project packages/dual/esm.ts#",
        ]
    );
}

#[test]
fn missing_unindexed_and_failed_targets_are_told_apart() {
    let project = Project::new()
        .source(
            "a.ts",
            "import './missing.js';\nimport './legacy.js';\nimport './excluded';\nimport './types';\nimport './broken';\nimport './gone/x';\nimport '/abs/x';\n",
        )
        .source("broken.ts", "function (")
        .unindexed("legacy.js")
        .unindexed("excluded.ts")
        .context("types.d.ts");
    assert_unresolved(&project, "a.ts", "./missing.js", Reason::NotFound);
    // JavaScript isn't a module without `allowJs`, but the file exists.
    assert_unresolved(&project, "a.ts", "./legacy.js", Reason::NotIndexed);
    // A source the scope excludes, or a declaration file, is labeled with its path.
    assert_external(&project, "a.ts", "./excluded", "excluded.ts");
    assert_external(&project, "a.ts", "./types", "types.d.ts");
    // A file that never parsed has no nodes to import.
    assert_unresolved(&project, "a.ts", "./broken", Reason::NotIndexed);
    assert_unresolved(&project, "a.ts", "./gone/x", Reason::NotFound);
    assert_unresolved(&project, "a.ts", "/abs/x", Reason::NotFound);
    // With `allowJs`, JavaScript resolves (and is labeled with its path).
    let js = project.input("tsconfig.json", r#"{"compilerOptions": {"allowJs": true}}"#);
    assert_external(&js, "a.ts", "./legacy.js", "legacy.js");
}

#[test]
fn packages_and_builtins_are_external_without_installation() {
    let project = Project::new().source(
        "a.ts",
        "import 'express';\nimport 'express/lib/router';\nimport '@scope/pkg/sub';\nimport 'fs';\nimport 'node:fs/promises';\nimport 'fs/promises';\nimport 'node:test';\nimport '#internal';\nimport '[odd]';\n",
    );
    assert_external(&project, "a.ts", "express", "express");
    assert_external(&project, "a.ts", "express/lib/router", "express");
    assert_external(&project, "a.ts", "@scope/pkg/sub", "@scope/pkg");
    assert_external(&project, "a.ts", "fs", "node:fs");
    assert_external(&project, "a.ts", "node:fs/promises", "node:fs/promises");
    assert_external(&project, "a.ts", "fs/promises", "node:fs/promises");
    assert_external(&project, "a.ts", "node:test", "node:test");
    assert_external(&project, "a.ts", "#internal", "#internal");
    // No package can be named so its label reads as a list of labels.
    assert_unresolved(&project, "a.ts", "[odd]", Reason::NotFound);
}

#[test]
fn paths_and_base_url_map_bare_specifiers_first() {
    let project = Project::new()
        .input(
            "tsconfig.json",
            r#"{
                // Comments and trailing commas are fine.
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": {"@app/*": ["src/*", "fallback/*"], "exact": ["src/exact.ts"], "@app/special/*": ["special/*"],},
                },
            }"#,
        )
        .source(
            "src/main.ts",
            "import '@app/util';\nimport '@app/other';\nimport '@app/missing';\nimport 'exact';\nimport '@app/special/x';\nimport 'src/util';\nimport 'lodash';\n",
        )
        .source("src/util.ts", "")
        .source("src/exact.ts", "")
        .source("fallback/other.ts", "")
        .source("special/x.ts", "");
    assert_resolved(&project, "src/main.ts", "@app/util", "src/util.ts");
    // The next substitution is tried when the first names nothing.
    assert_resolved(&project, "src/main.ts", "@app/other", "fallback/other.ts");
    // A matched pattern that names nothing is not a package.
    assert_unresolved(&project, "src/main.ts", "@app/missing", Reason::NotFound);
    assert_resolved(&project, "src/main.ts", "exact", "src/exact.ts");
    // The longest matching prefix wins.
    assert_resolved(&project, "src/main.ts", "@app/special/x", "special/x.ts");
    assert_resolved(&project, "src/main.ts", "src/util", "src/util.ts");
    assert_external(&project, "src/main.ts", "lodash", "lodash");
}

#[test]
fn paths_without_base_url_resolve_from_the_configuration_that_wrote_them() {
    let project = Project::new()
        .input(
            "configs/base.json",
            r#"{"compilerOptions": {"paths": {"~/*": ["../src/*"]}}}"#,
        )
        .input("app/tsconfig.json", r#"{"extends": "../configs/base.json"}"#)
        .source("app/main.ts", "import '~/lib';")
        .source("src/lib.ts", "");
    assert_resolved(&project, "app/main.ts", "~/lib", "src/lib.ts");
}

#[test]
fn in_repository_packages_resolve_by_name_through_their_exports() {
    let project = Project::new()
        .input(
            "packages/lib/package.json",
            r##"{"name": "@me/lib", "exports": {".": "./src/index.ts", "./sub": {"types": "./src/sub.ts", "default": "./dist/sub.js"}}, "imports": {"#own": "./src/own.ts"}}"##,
        )
        .input("packages/plain/package.json", r#"{"name": "plain", "types": "./src/main.ts"}"#)
        .source("packages/lib/src/index.ts", "import '#own';\nimport '@me/lib/sub';")
        .source("packages/lib/src/sub.ts", "")
        .source("packages/lib/src/own.ts", "")
        .source("packages/plain/src/main.ts", "")
        .source("packages/plain/src/extra.ts", "")
        .source("app/main.ts", "import '@me/lib';\nimport '@me/lib/sub';\nimport '@me/lib/hidden';\nimport 'plain';\nimport 'plain/src/extra';\n");
    assert_resolved(&project, "app/main.ts", "@me/lib", "packages/lib/src/index.ts");
    assert_resolved(&project, "app/main.ts", "@me/lib/sub", "packages/lib/src/sub.ts");
    // A subpath the package doesn't export stays the package's.
    assert_external(&project, "app/main.ts", "@me/lib/hidden", "@me/lib");
    assert_resolved(&project, "app/main.ts", "plain", "packages/plain/src/main.ts");
    assert_resolved(
        &project,
        "app/main.ts",
        "plain/src/extra",
        "packages/plain/src/extra.ts",
    );
    // A package's `imports` and its references to itself.
    assert_resolved(
        &project,
        "packages/lib/src/index.ts",
        "#own",
        "packages/lib/src/own.ts",
    );
    assert_resolved(
        &project,
        "packages/lib/src/index.ts",
        "@me/lib/sub",
        "packages/lib/src/sub.ts",
    );
}

#[test]
fn type_only_and_empty_imports_are_erased_unless_verbatim() {
    let source = "import {} from './b';\nimport { type T } from './b';\nimport type { U } from './b';\nexport {} from './b';\nexport { type V } from './b';\nimport { w } from './b';\n";
    let plain = Project::new()
        .source("a.ts", source)
        .source("b.ts", "export const w = 1;");
    let compiled = plain.compile();
    // C-12: only the value import is a runtime dependency.
    assert_eq!(compiled.import_count("a.ts"), 1);
    let verbatim = plain.input(
        "tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    let compiled = verbatim.compile();
    // With `verbatimModuleSyntax`, named forms stay; `import type` never does.
    assert_eq!(compiled.import_count("a.ts"), 5);
}
