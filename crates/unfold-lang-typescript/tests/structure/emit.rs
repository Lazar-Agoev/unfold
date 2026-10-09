//! The emit map: outputs map back to sources through `rootDir`, `outDir`, and extension rules;
//! selectors decide membership; an output two sources produce is ambiguous; configurations that
//! emit nothing contribute nothing.

use unfold_core::RepoPath;
use unfold_engine::EmitSource;

use crate::support::Project;

fn source(project: &Project, output: &str) -> EmitSource {
    project.link(|_, linked| linked.emitted_source(&RepoPath::new(output).expect("path")))
}

fn from(path: &str) -> EmitSource {
    EmitSource::Source(RepoPath::new(path).expect("path"))
}

#[test]
fn outputs_map_through_root_dir_and_out_dir() {
    let project = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"rootDir": "src", "outDir": "dist", "jsx": "preserve"}, "include": ["src/**/*.ts", "src/**/*.tsx", "src/**/*.mts"]}"#,
        )
        .source("src/server.ts", "")
        .source("src/jobs/run.ts", "")
        .source("src/view.tsx", "")
        .source("src/esm.mts", "")
        .source("scripts/tool.ts", "")
        .context("src/types.d.ts");
    assert_eq!(source(&project, "dist/server.js"), from("src/server.ts"));
    assert_eq!(source(&project, "dist/jobs/run.js"), from("src/jobs/run.ts"));
    assert_eq!(source(&project, "dist/view.jsx"), from("src/view.tsx"));
    assert_eq!(source(&project, "dist/esm.mjs"), from("src/esm.mts"));
    // Not included, not emitted, or not JavaScript.
    assert_eq!(source(&project, "dist/tool.js"), EmitSource::Unknown);
    assert_eq!(source(&project, "dist/types.js"), EmitSource::Unknown);
    assert_eq!(source(&project, "dist/server.d.ts"), EmitSource::Unknown);
    assert_eq!(source(&project, "src/server.js"), EmitSource::Unknown);
}

#[test]
fn root_dir_defaults_to_the_configuration_folder() {
    let project = Project::new()
        .input("app/tsconfig.json", r#"{"compilerOptions": {"outDir": "build"}}"#)
        .source("app/src/main.ts", "")
        .source("app/build/old.ts", "")
        .source("app/node_modules_like/x.ts", "")
        .unindexed("app/excluded.ts");
    assert_eq!(source(&project, "app/build/src/main.js"), from("app/src/main.ts"));
    // The outDir is excluded by default.
    assert_eq!(source(&project, "app/build/build/old.js"), EmitSource::Unknown);
    // A source the index leaves out still emits (the launch then reports it `not_indexed`).
    assert_eq!(source(&project, "app/build/excluded.js"), from("app/excluded.ts"));
}

#[test]
fn outputs_two_sources_produce_are_ambiguous() {
    let project = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"outDir": "dist", "rootDir": "."}, "include": ["a", "b"]}"#,
        )
        .input(
            "b/tsconfig.json",
            r#"{"compilerOptions": {"outDir": "../dist/a", "rootDir": "."}}"#,
        )
        .source("a/x.ts", "")
        .source("a/y.mts", "")
        .source("b/x.ts", "")
        .source("b/only.ts", "");
    // `dist/a/x.js` comes from `a/x.ts` (root configuration) and from `b/x.ts` (b's).
    assert_eq!(source(&project, "dist/a/x.js"), EmitSource::Ambiguous);
    assert_eq!(source(&project, "dist/a/y.mjs"), from("a/y.mts"));
    assert_eq!(source(&project, "dist/b/only.js"), from("b/only.ts"));
    assert_eq!(source(&project, "dist/a/only.js"), from("b/only.ts"));
}

#[test]
fn configurations_that_emit_nothing_contribute_nothing() {
    for options in [r#"{"noEmit": true}"#, r#"{"emitDeclarationOnly": true}"#] {
        let config = format!(r#"{{"compilerOptions": {options}}}"#);
        let project = Project::new()
            .input("tsconfig.json", &config)
            .source("main.ts", "");
        assert_eq!(source(&project, "main.js"), EmitSource::Unknown, "{options}");
    }
    let bundle = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"outFile": "out/bundle.js"}}"#,
        )
        .source("a.ts", "")
        .source("b.ts", "");
    assert_eq!(source(&bundle, "out/bundle.js"), EmitSource::Ambiguous);
    let single = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"outFile": "out/bundle.js"}, "files": ["a.ts"]}"#,
        )
        .source("a.ts", "")
        .source("b.ts", "");
    assert_eq!(source(&single, "out/bundle.js"), from("a.ts"));
    // A file without a configuration emits beside itself only through a configuration.
    let none = Project::new().source("main.ts", "");
    assert_eq!(source(&none, "main.js"), EmitSource::Unknown);
}

#[test]
fn selectors_follow_typescripts_globs() {
    let project = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"outDir": "out"}, "include": ["src", "lib/*.ts", "tools/?.ts"], "exclude": ["src/**/*.test.ts", "src/skip"]}"#,
        )
        .source("src/a.ts", "")
        .source("src/a.test.ts", "")
        .source("src/skip/b.ts", "")
        .source("src/.hidden/c.ts", "")
        .source("lib/d.ts", "")
        .source("lib/deep/e.ts", "")
        .source("tools/f.ts", "")
        .source("tools/gg.ts", "");
    assert_eq!(source(&project, "out/src/a.js"), from("src/a.ts"));
    for output in [
        "out/src/a.test.js",
        "out/src/skip/b.js",
        "out/src/.hidden/c.js",
        "out/lib/deep/e.js",
        "out/tools/gg.js",
    ] {
        assert_eq!(source(&project, output), EmitSource::Unknown, "{output}");
    }
    assert_eq!(source(&project, "out/lib/d.js"), from("lib/d.ts"));
    assert_eq!(source(&project, "out/tools/f.js"), from("tools/f.ts"));
}
