use unfold_core::{RepoPath, Snapshot};

use super::*;
use crate::extract::{ExtractOptions, extract};
use crate::payload::Payload;

/// Files by ID (their index), resolving `./name` to the file called `name.ts`, and anything
/// else to a package.
struct Files(Vec<(&'static str, ModuleFacts)>);

impl Files {
    fn new(sources: &[(&'static str, &str)]) -> Self {
        Self(
            sources
                .iter()
                .map(|(path, source)| {
                    let snapshot =
                        Snapshot::new(RepoPath::new(path).expect("path"), source.as_bytes().to_vec())
                            .expect("text");
                    let facts = extract(&snapshot, ExtractOptions::default()).expect("extracts");
                    (
                        *path,
                        Payload::from_bytes(&facts.payload).expect("payload").module,
                    )
                })
                .collect(),
        )
    }

    fn table(&self) -> ExportTable {
        let files: Vec<Option<ModuleFile<'_>>> = self
            .0
            .iter()
            .map(|(_, facts)| {
                Some(ModuleFile {
                    facts,
                    is_module: true,
                    synthetic_default: true,
                    callee_synthetic_default: true,
                    symbols: Vec::new(),
                })
            })
            .collect();
        ExportTable::build(&files, self)
    }
}

impl Specifiers for Files {
    fn resolve(&self, _file: FileId, specifier: &str, _kind: SpecifierKind) -> Resolved {
        match specifier.strip_prefix("./") {
            Some(name) => self
                .0
                .iter()
                .position(|(path, _)| path.strip_suffix(".ts") == Some(name))
                .map_or(Resolved::NotFound, |index| {
                    Resolved::Indexed(FileId(index as u32))
                }),
            None => Resolved::Package(crate::labels::package_name(specifier).into()),
        }
    }

    fn path(&self, file: FileId) -> Option<&str> {
        self.0.get(file.index()).map(|(path, _)| *path)
    }
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
    let (a, b, c) = (FileId(0), FileId(1), FileId(2));
    let run = table.export(a, "run").expect("run").clone();
    assert_eq!(run.declarations, [DeclRef { file: a, index: 1 }]);
    assert!(table.export(a, "Shape").is_some_and(|shape| shape.type_only));
    // `import * as a`: the module, through its file declaration.
    let namespace = table.binding(b, 0).expect("binding").clone();
    assert_eq!(namespace.namespace, Some(a));
    assert_eq!(namespace.declarations, [DeclRef { file: a, index: 0 }]);
    assert_eq!(table.member(&namespace, "run"), run);
    assert!(table.member(&namespace, "missing").is_empty());
    assert_eq!(table.binding(b, 1).expect("binding"), &run);
    // A package import is external, and so are its members.
    let express = table.binding(b, 2).expect("binding").clone();
    assert_eq!(express.external, [ExternalLocation::Package("express".into())]);
    assert_eq!(table.member(&express, "Router").external, express.external);
    assert!(table.binding(b, 3).expect("binding").type_only);
    // `export =`: members come from what it assigns (here, nothing named `inner` is exported
    // through the function itself, so the member is empty).
    let assigned = table.export(c, "export=").expect("export=");
    assert_eq!(assigned.declarations.len(), 2);
    assert_eq!(table.exports(c).len(), 1);
    assert!(table.export(c, "default").is_none());
}
