use unfold_core::{RepoPath, Snapshot};

use super::*;
use crate::extract::{ExtractOptions, extract};
use crate::payload::Payload;

/// Files by ID (their index). `./name` and `./name.js` resolve to the file called `name.ts`, or
/// to a boundary the test lists; anything else is a package.
struct Files {
    files: Vec<(&'static str, ModuleFacts)>,
    /// Specifier → what it resolves to, for the boundaries a test sets up.
    boundaries: Vec<(&'static str, Resolved)>,
    synthetic_default: bool,
}

impl Files {
    fn new(sources: &[(&'static str, &str)]) -> Self {
        let files = sources
            .iter()
            .map(|(path, source)| {
                let snapshot = Snapshot::new(RepoPath::new(path).expect("path"), source.as_bytes().to_vec())
                    .expect("text");
                let module = match extract(&snapshot, ExtractOptions::default()) {
                    Ok(facts) => Payload::from_bytes(&facts.payload).expect("payload").module,
                    Err(_) => ModuleFacts::default(),
                };
                (*path, module)
            })
            .collect();
        Self {
            files,
            boundaries: Vec::new(),
            synthetic_default: true,
        }
    }

    fn boundary(mut self, specifier: &'static str, resolved: Resolved) -> Self {
        self.boundaries.push((specifier, resolved));
        self
    }

    fn table(&self) -> ExportTable {
        let files: Vec<Option<ModuleFile<'_>>> = self
            .files
            .iter()
            .map(|(_, facts)| {
                Some(ModuleFile {
                    facts,
                    is_module: facts.module_syntax,
                    synthetic_default: self.synthetic_default,
                    symbols: Vec::new(),
                })
            })
            .collect();
        ExportTable::build(&files, self)
    }

    fn id(&self, path: &str) -> FileId {
        let index = self
            .files
            .iter()
            .position(|(file, _)| *file == path)
            .expect("a file of the test");
        FileId(index as u32)
    }

    /// A target, described: its declarations (`path Kind name`), the module it names,
    /// external and excluded locations, and whether it is ambiguous.
    fn describe(&self, target: &ExportTarget) -> Vec<String> {
        let mut described: Vec<String> = target
            .declarations
            .iter()
            .map(|declaration| {
                let (path, facts) = &self.files[declaration.file.index()];
                let fact = &facts.declarations[declaration.index as usize];
                format!("{path} {:?} {}", fact.kind, fact.name)
            })
            .collect();
        if let Some(module) = target.namespace {
            described.push(format!("module {}", self.files[module.index()].0));
        }
        for location in &target.external {
            described.push(format!("external {location:?}"));
        }
        for location in &target.excluded {
            described.push(format!("excluded {location:?}"));
        }
        if target.ambiguous {
            described.push("ambiguous".to_owned());
        }
        described
    }

    /// Every export name of `path` followed as a value, with its target described.
    fn exports(&self, table: &ExportTable, path: &str) -> Vec<(String, Vec<String>)> {
        let mut names: Vec<(String, Vec<String>)> = table.value_names[self.id(path).index()]
            .iter()
            .map(|(name, target)| (name.to_string(), self.describe(target)))
            .collect();
        names.sort();
        names
    }
}

impl Specifiers for Files {
    fn resolve(&self, _file: FileId, specifier: &str, _kind: SpecifierKind) -> Resolved {
        if let Some((_, resolved)) = self.boundaries.iter().find(|(written, _)| *written == specifier) {
            return resolved.clone();
        }
        match specifier.strip_prefix("./") {
            Some(name) => {
                let name = name.strip_suffix(".js").unwrap_or(name);
                self.files
                    .iter()
                    .position(|(path, _)| path.strip_suffix(".ts") == Some(name))
                    .map_or(Resolved::NotFound, |index| {
                        Resolved::Indexed(FileId(index as u32))
                    })
            }
            None => Resolved::Package(crate::labels::package_name(specifier).into()),
        }
    }

    fn path(&self, file: FileId) -> Option<&str> {
        self.files.get(file.index()).map(|(path, _)| *path)
    }
}

fn entry(name: &str, described: &[&str]) -> (String, Vec<String>) {
    (
        name.to_owned(),
        described.iter().map(|item| (*item).to_owned()).collect(),
    )
}

#[test]
fn bindings_and_members_follow_modules() {
    let files = Files::new(&[
        (
            "a.ts",
            "export function run() {}\nexport const value = 1;\nexport interface Shape {}",
        ),
        (
            "b.ts",
            "import * as a from './a';\nimport { run } from './a';\nimport express from 'express';\nimport type { Shape } from './a';\nexport { a };",
        ),
        (
            "c.ts",
            "function f() {}\nnamespace f { export const inner = 1; }\nexport = f;",
        ),
    ]);
    let table = files.table();
    let (a, b) = (files.id("a.ts"), files.id("b.ts"));
    let run = table.value_member(&ExportTarget::module(a), "run");
    assert_eq!(run.declarations, [DeclRef { file: a, index: 1 }]);
    // `import * as a`: the module.
    let namespace = table.value_binding(b, 0).expect("binding").clone();
    assert_eq!(namespace.namespace, Some(a));
    assert_eq!(table.value_member(&namespace, "run"), run);
    assert!(table.value_member(&namespace, "missing").is_empty());
    assert_eq!(table.value_binding(b, 1).expect("binding"), &run);
    // A package import is external, and so are its members.
    let express = table.value_binding(b, 2).expect("binding").clone();
    assert_eq!(express.external, [ExternalLocation::Package("express".into())]);
    assert_eq!(table.value_member(&express, "Router").external, express.external);
    // A type-only import names nothing as a value, and the type it names as a type.
    assert!(table.value_binding(b, 3).expect("binding").is_empty());
    assert_eq!(
        files.describe(table.type_binding(b, 3).expect("binding")),
        ["a.ts Interface Shape"]
    );
    // `export =`: what it assigns, the function and the namespace merged with it.
    assert_eq!(
        files.exports(&table, "c.ts"),
        [entry(
            "export=",
            &["c.ts Function f", "c.ts Namespace { instantiated: true } f"]
        )]
    );
}

#[test]
fn local_exports_name_their_declarations() {
    let files = Files::new(&[(
        "a.ts",
        "export function f() {}\nexport namespace f { export const x = 1; }\nexport class C {}\nexport const k = 1, j = () => 2;\nconst hidden = 1;\nexport { hidden as shown, k as again };\nexport default function main() {}\nexport interface Shape {}\nexport type Alias = string;\nexport enum E { A }\nexport declare const ambient: number;\nexport type { C as TypeOfC };\n",
    )]);
    let table = files.table();
    assert_eq!(
        files.exports(&table, "a.ts"),
        [
            entry("Alias", &["a.ts TypeAlias Alias"]),
            entry("C", &["a.ts Class C"]),
            entry("E", &["a.ts Enum E"]),
            entry("Shape", &["a.ts Interface Shape"]),
            // A type-only export names nothing as a value.
            entry("TypeOfC", &[]),
            entry("again", &["a.ts Const k"]),
            entry("ambient", &["a.ts Const ambient"]),
            entry("default", &["a.ts Function main"]),
            entry(
                "f",
                &["a.ts Function f", "a.ts Namespace { instantiated: true } f"]
            ),
            entry("j", &["a.ts Const j"]),
            entry("k", &["a.ts Const k"]),
            entry("shown", &["a.ts Const hidden"]),
        ]
    );
}

#[test]
fn default_and_export_assignment_forms() {
    let files = Files::new(&[
        ("arrow.ts", "export default () => 1;"),
        ("anonymous.ts", "export default class {}"),
        ("name.ts", "function run() {}\nexport default run;"),
        (
            "assigned.ts",
            "function handler() {}\nnamespace handler { export function extra() {} }\nexport = handler;",
        ),
        (
            "user.ts",
            "import handler = require('./assigned');\nimport def from './assigned';\nexport { handler, def };",
        ),
    ]);
    let table = files.table();
    assert_eq!(
        files.exports(&table, "arrow.ts"),
        [entry("default", &["arrow.ts Expression "])]
    );
    assert_eq!(
        files.exports(&table, "anonymous.ts"),
        [entry("default", &["anonymous.ts Class "])]
    );
    assert_eq!(
        files.exports(&table, "name.ts"),
        [entry("default", &["name.ts Function run"])]
    );
    let assigned = [
        "assigned.ts Function handler",
        "assigned.ts Namespace { instantiated: true } handler",
    ];
    assert_eq!(
        files.exports(&table, "assigned.ts"),
        [entry("export=", &assigned)]
    );
    // `import x = require` takes `export =`; a missing `default` falls back to it.
    assert_eq!(
        files.exports(&table, "user.ts"),
        [entry("def", &assigned), entry("handler", &assigned)]
    );
    let strict = Files {
        synthetic_default: false,
        ..files
    };
    let table = strict.table();
    assert_eq!(
        strict.exports(&table, "user.ts"),
        [entry("def", &[]), entry("handler", &assigned)]
    );
}

#[test]
fn re_export_chains_and_stars_are_followed() {
    let files = Files::new(&[
        (
            "c.ts",
            "export function deep() {}\nexport default function c() {}\nexport const shared = 1;",
        ),
        (
            "b.ts",
            "export * from './c.js';\nexport { deep as renamed } from './c.js';\nexport const own = 1;",
        ),
        ("d.ts", "export const shared = 2;\nexport const unique = 3;"),
        (
            "a.ts",
            "export * from './b.js';\nexport * from './d.js';\nexport * as ns from './c.js';\nexport { default as c } from './c.js';\nexport const unique = 'explicit';\nexport type * from './c.js';\nexport { type deep as typeOnly } from './c.js';\n",
        ),
    ]);
    let table = files.table();
    assert_eq!(
        files.exports(&table, "a.ts"),
        [
            entry("c", &["c.ts Function c"]),
            entry("deep", &["c.ts Function deep"]),
            entry("ns", &["module c.ts"]),
            entry("own", &["b.ts Const own"]),
            entry("renamed", &["c.ts Function deep"]),
            // Two stars export different declarations: ambiguous, with both.
            entry("shared", &["c.ts Const shared", "d.ts Const shared", "ambiguous"]),
            entry("typeOnly", &[]),
            // An explicit export wins over every star.
            entry("unique", &["a.ts Const unique"]),
        ]
    );
}

#[test]
fn boundaries_outside_the_program_are_kept() {
    let excluded = Resolved::Unindexed {
        path: RepoPath::new("excluded.ts").expect("path"),
        excluded: true,
    };
    let declarations = Resolved::Unindexed {
        path: RepoPath::new("types.d.ts").expect("path"),
        excluded: false,
    };
    let files = Files::new(&[
        (
            "a.ts",
            "export { Router } from 'express';\nexport * as fs from 'node:fs';\nexport { x } from './excluded.js';\nexport { y } from './types.js';\nexport { z } from './broken.js';\nexport { w } from './missing.js';\nimport express from 'express';\nexport { express };\n",
        ),
        ("broken.ts", "export const z = ("),
    ])
    .boundary("./excluded.js", excluded)
    .boundary("./types.js", declarations)
    .boundary("./broken.js", Resolved::Failed(FileId(1)));
    let table = files.table();
    assert_eq!(
        files.exports(&table, "a.ts"),
        [
            entry("Router", &["external Package(\"express\")"]),
            entry("express", &["external Package(\"express\")"]),
            entry("fs", &["external Package(\"node:fs\")"]),
            // A module that names nothing exports nothing under the name.
            entry("w", &[]),
            entry("x", &["excluded File(\"excluded.ts\")"]),
            entry("y", &["external File(\"types.d.ts\")"]),
            entry("z", &["excluded File(\"broken.ts\")"]),
        ]
    );
}

#[test]
fn cycles_end_and_scripts_export_nothing() {
    let files = Files::new(&[
        (
            "a.ts",
            "export * from './b.js';\nexport const a = 1;\nexport { loop } from './b.js';",
        ),
        (
            "b.ts",
            "export * from './a.js';\nexport const b = 1;\nexport { loop } from './a.js';",
        ),
        (
            "self.ts",
            "export * from './self.js';\nexport { me as me } from './self.js';",
        ),
        ("script.ts", "const global = 1;"),
    ]);
    let table = files.table();
    assert_eq!(
        files.exports(&table, "a.ts"),
        [
            entry("a", &["a.ts Const a"]),
            entry("b", &["b.ts Const b"]),
            entry("loop", &[])
        ]
    );
    assert_eq!(files.exports(&table, "self.ts"), [entry("me", &[])]);
    // Without module syntax a file is a script, which exports nothing.
    assert_eq!(files.exports(&table, "script.ts"), []);
}
