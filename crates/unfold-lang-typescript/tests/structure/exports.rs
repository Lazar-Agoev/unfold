//! Export tables: named, default, and `export =` exports, merged declarations, re-export chains,
//! stars (explicit names win, conflicts are ambiguous, `default` is never re-exported), namespace
//! re-exports, type-only exports, external and excluded boundaries, cycles, and the top-level
//! declarations manual locators name.

use unfold_engine::evidence::{Candidate, ExternalLocation, Origin};
use unfold_engine::{FactsSet, Linked};
use unfold_lang_typescript::payload::Payload;

use crate::support::Project;

fn describe(program: &FactsSet, candidate: &Candidate) -> String {
    match (candidate.origin, &candidate.declaration, &candidate.location) {
        (Origin::Indexed, Some(declaration), _) => {
            let file = &program.files()[declaration.file.index()];
            let payload = Payload::from_bytes(&file.facts.as_ref().expect("facts").payload).expect("payload");
            let fact = &payload.module.declarations[declaration.index as usize];
            let augments = if candidate.augmentation { " (augments)" } else { "" };
            format!("{} {:?} {}{augments}", file.path, fact.kind, fact.name)
        }
        (origin, _, Some(ExternalLocation::Package(name))) => format!("{origin:?} package {name}"),
        (origin, _, Some(ExternalLocation::File(path))) => format!("{origin:?} file {path}"),
        (origin, _, location) => format!("{origin:?} {location:?}"),
    }
}

/// The runtime exports of `path`: each name with its candidates described.
fn exports(project: &Project, path: &str) -> Vec<(String, Vec<String>)> {
    project.link(|program, linked: &dyn Linked| {
        let file = program.find(path).expect("file in the program");
        linked
            .module_exports(file)
            .iter()
            .map(|export| {
                let candidates = export
                    .candidates
                    .iter()
                    .map(|candidate| describe(program, candidate))
                    .collect();
                (export.name.to_string(), candidates)
            })
            .collect()
    })
}

fn entry(name: &str, candidates: &[&str]) -> (String, Vec<String>) {
    (
        name.to_owned(),
        candidates
            .iter()
            .map(|candidate| (*candidate).to_owned())
            .collect(),
    )
}

#[test]
fn local_exports_name_their_declarations() {
    let project = Project::new().source(
        "a.ts",
        "export function f() {}\nexport namespace f { export const x = 1; }\nexport class C {}\nexport const k = 1, j = () => 2;\nconst hidden = 1;\nexport { hidden as shown, k as again };\nexport default function main() {}\nexport interface Shape {}\nexport type Alias = string;\nexport enum E { A }\nexport declare const ambient: number;\nnamespace TypesOnly { export interface I {} }\nexport { TypesOnly };\nexport type { C as TypeOfC };\n",
    );
    assert_eq!(
        exports(&project, "a.ts"),
        [
            entry("C", &["a.ts Class C"]),
            entry("E", &["a.ts Enum E"]),
            entry("again", &["a.ts Const k"]),
            entry("ambient", &["a.ts Const ambient"]),
            entry("default", &["a.ts Function main"]),
            entry(
                "f",
                &[
                    "a.ts Function f",
                    "a.ts Namespace { instantiated: true } f (augments)"
                ]
            ),
            entry("j", &["a.ts Const j"]),
            entry("k", &["a.ts Const k"]),
            entry("shown", &["a.ts Const hidden"]),
        ]
    );
}

#[test]
fn default_and_export_assignment_forms() {
    let project = Project::new()
        .source("arrow.ts", "export default () => 1;")
        .source("anonymous.ts", "export default class {}")
        .source("name.ts", "function run() {}\nexport default run;")
        .source("assigned.ts", "function handler() {}\nnamespace handler { export function extra() {} }\nexport = handler;")
        .source("user.ts", "import handler = require('./assigned');\nimport def from './assigned';\nexport { handler, def };");
    assert_eq!(
        exports(&project, "arrow.ts"),
        [entry("default", &["arrow.ts Expression "])]
    );
    assert_eq!(
        exports(&project, "anonymous.ts"),
        [entry("default", &["anonymous.ts Class "])]
    );
    assert_eq!(
        exports(&project, "name.ts"),
        [entry("default", &["name.ts Function run"])]
    );
    let assigned = [
        "assigned.ts Function handler",
        "assigned.ts Namespace { instantiated: true } handler (augments)",
    ];
    assert_eq!(exports(&project, "assigned.ts"), [entry("export=", &assigned)]);
    // `import x = require` takes `export =`; a missing `default` falls back to it.
    assert_eq!(
        exports(&project, "user.ts"),
        [entry("def", &assigned), entry("handler", &assigned)]
    );
    let strict = project.input(
        "tsconfig.json",
        r#"{"compilerOptions": {"allowSyntheticDefaultImports": false}}"#,
    );
    assert_eq!(
        exports(&strict, "user.ts"),
        [entry("def", &[]), entry("handler", &assigned)]
    );
}

#[test]
fn re_export_chains_and_stars_are_followed() {
    let project = Project::new()
        .source("c.ts", "export function deep() {}\nexport default function c() {}\nexport const shared = 1;")
        .source("b.ts", "export * from './c.js';\nexport { deep as renamed } from './c.js';\nexport const own = 1;")
        .source("d.ts", "export const shared = 2;\nexport const unique = 3;")
        .source("a.ts", "export * from './b.js';\nexport * from './d.js';\nexport * as ns from './c.js';\nexport { default as c } from './c.js';\nexport const unique = 'explicit';\nexport type * from './c.js';\nexport { type deep as typeOnly } from './c.js';\n");
    assert_eq!(
        exports(&project, "a.ts"),
        [
            entry("c", &["c.ts Function c"]),
            entry("deep", &["c.ts Function deep"]),
            entry("ns", &["c.ts Module "]),
            entry("own", &["b.ts Const own"]),
            entry("renamed", &["c.ts Function deep"]),
            // Two stars export different declarations: ambiguous, with both candidates.
            entry("shared", &["c.ts Const shared", "d.ts Const shared"]),
            // An explicit export wins over every star.
            entry("unique", &["a.ts Const unique"]),
        ]
    );
}

#[test]
fn boundaries_outside_the_program_are_kept() {
    let project = Project::new()
        .source("a.ts", "export { Router } from 'express';\nexport * as fs from 'node:fs';\nexport { x } from './excluded.js';\nexport { y } from './types.js';\nexport { z } from './broken.js';\nexport { w } from './missing.js';\nimport express from 'express';\nexport { express };\n")
        .source("broken.ts", "export const z = (")
        .unindexed("excluded.ts")
        .context("types.d.ts");
    assert_eq!(
        exports(&project, "a.ts"),
        [
            entry("Router", &["External package express"]),
            entry("express", &["External package express"]),
            entry("fs", &["External package node:fs"]),
            // A module that names nothing exports nothing under the name.
            entry("w", &[]),
            entry("x", &["Excluded file excluded.ts"]),
            entry("y", &["External file types.d.ts"]),
            entry("z", &["Excluded file broken.ts"]),
        ]
    );
}

#[test]
fn cycles_end_and_scripts_export_nothing() {
    let project = Project::new()
        .source(
            "a.ts",
            "export * from './b.js';\nexport const a = 1;\nexport { loop } from './b.js';",
        )
        .source(
            "b.ts",
            "export * from './a.js';\nexport const b = 1;\nexport { loop } from './a.js';",
        )
        .source(
            "self.ts",
            "export * from './self.js';\nexport { me as me } from './self.js';",
        )
        .source("script.ts", "const global = 1;");
    assert_eq!(
        exports(&project, "a.ts"),
        [
            entry("a", &["a.ts Const a"]),
            entry("b", &["b.ts Const b"]),
            entry("loop", &[])
        ]
    );
    assert_eq!(exports(&project, "self.ts"), [entry("me", &[])]);
    // Without module syntax (and no forced detection), a file is a script.
    let script = Project::new()
        .input(
            "tsconfig.json",
            r#"{"compilerOptions": {"module": "esnext", "moduleDetection": "auto"}}"#,
        )
        .source("script.ts", "const global = 1;")
        .source("user.ts", "import * as s from './script';\nexport { s };");
    assert_eq!(exports(&script, "script.ts"), []);
    assert_eq!(exports(&script, "user.ts"), [entry("s", &["script.ts Module "])]);
}

#[test]
fn locators_name_top_level_declarations() {
    let project = Project::new().source(
        "a.ts",
        "function shutdown() {}\nconst handler = async () => {};\nclass Job {}\nlet later = function () {};\nif (true) { var hoisted = () => 1; }\nexport default function () {}\ndeclare function ambient(): void;\n",
    );
    project.link(|program, linked| {
        let file = program.find("a.ts").expect("file");
        let block = |name: &str| -> Vec<Option<String>> {
            linked
                .locator_top_level(file, name)
                .into_iter()
                .map(|declaration| {
                    linked
                        .locator_concrete(declaration)
                        .map(|block| program.block(block).expect("block").key.as_str().to_owned())
                })
                .collect()
        };
        assert_eq!(block("shutdown"), [Some("a.ts#function:shutdown".to_owned())]);
        assert_eq!(
            block("handler"),
            [Some("a.ts#variable:handler/function:handler".to_owned())]
        );
        assert_eq!(block("Job"), [Some("a.ts#class:Job".to_owned())]);
        // Only a constant's initializer is a locator target, and only direct statements of
        // the file are top-level declarations (a hoisted `var` is not).
        assert_eq!(block("later"), [None]);
        assert!(block("hoisted").is_empty());
        assert_eq!(block("ambient"), [None]);
        assert!(block("missing").is_empty());
        let default = &linked.module_exports(file)[0];
        assert_eq!(&*default.name, "default");
        let target = default.candidates[0]
            .declaration
            .and_then(|declaration| linked.locator_concrete(declaration));
        assert_eq!(
            target.map(|block| program.block(block).expect("block").key.as_str().to_owned()),
            Some("a.ts#function:default".to_owned())
        );
    });
}
