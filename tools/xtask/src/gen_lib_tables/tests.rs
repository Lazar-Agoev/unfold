use super::scan::Model;
use super::{Header, is_library_file, reference_paths, render};

fn model(files: &[&str]) -> Model {
    let mut model = Model::default();
    for (index, text) in files.iter().enumerate() {
        model
            .add_file(&format!("file{index}.d.ts"), text)
            .expect("parses");
    }
    model
}

fn globals(files: &[&str]) -> Vec<String> {
    let mut names = model(files).global_values();
    names.retain(|name| name != "globalThis" && name != "undefined");
    names
}

#[test]
fn scripts_declare_globals_and_modules_only_augment() {
    let script = "declare var a: number; declare let b, c; declare function f(): void; \
                  declare class C {} declare enum E { X } declare const enum K { Y } \
                  interface I {} type T = number;";
    assert_eq!(globals(&[script]), ["C", "E", "K", "a", "b", "c", "f"]);
    let module = "export declare var hidden: number; declare global { var shown: number; \
                  function also(): void; interface Seen {} }";
    assert_eq!(globals(&[module]), ["also", "shown"]);
    assert_eq!(model(&[module]).library_types(), ["Seen"]);
    // Every name is there once, with the checker's intrinsics.
    let all = model(&[script]).global_values();
    assert!(all.contains(&"globalThis".to_owned()) && all.contains(&"undefined".to_owned()));
}

#[test]
fn ambient_modules_are_listed_and_their_global_blocks_merge() {
    let file = "declare module \"fs\" { export function read(): void; global { var Buffer: unknown; \
                namespace NodeJS { interface ReadStream {} } } } \
                declare module \"node:fs\" { export * from \"fs\"; }";
    let model = model(&[file, "declare namespace NodeJS { interface Process {} }"]);
    assert_eq!(model.global_values(), ["Buffer", "globalThis", "undefined"]);
    assert_eq!(model.library_types(), ["NodeJS.Process", "NodeJS.ReadStream"]);
    assert_eq!(
        model.ambient_modules.iter().collect::<Vec<_>>(),
        ["fs", "node:fs"]
    );
}

#[test]
fn namespaces_are_values_only_when_instantiated() {
    let file = "declare namespace Types { interface A {} type B = A; namespace Inner { interface C {} } } \
                declare namespace Values { function f(): void; } \
                declare namespace Nested.Deep { const x: number; } \
                declare namespace Enums { const enum E { A } } \
                declare namespace Exports { export { Local }; type Local = number; }";
    assert_eq!(globals(&[file]), ["Enums", "Nested", "Values"]);
    // A name exported from an enclosing block is looked up there.
    let outer =
        "declare module \"m\" { type Encoding = string; global { namespace G { export { Encoding }; } } }";
    assert!(globals(&[outer]).is_empty());
    // An unknown name could be a value.
    assert_eq!(globals(&["declare namespace U { export { missing }; }"]), ["U"]);
}

#[test]
fn library_types_follow_implicit_and_explicit_exports() {
    let file = "declare namespace Intl { interface Collator {} class Locale {} namespace Deep { interface X {} } } \
                declare namespace Explicit { export interface Shown {} interface Hidden {} export {}; } \
                declare function Merged(): void; declare namespace Merged { interface Options {} } \
                interface Merged {} type Alias = { a: 1 };";
    assert_eq!(
        model(&[file]).library_types(),
        [
            "Explicit.Shown",
            "Intl.Collator",
            "Intl.Deep.X",
            "Intl.Locale",
            "Merged",
            "Merged.Options"
        ]
    );
}

#[test]
fn files_and_references_are_recognized() {
    assert!(is_library_file("lib.es5.d.ts"));
    assert!(is_library_file("lib.dom.iterable.d.ts"));
    assert!(!is_library_file("lib.d.ts"));
    assert!(!is_library_file("typescript.d.ts"));
    let text = "/// <reference path=\"globals.d.ts\" />\n///<reference path='fs/promises.d.ts'/>\n\
                /// <reference lib=\"es2020\" />\n// /// <reference path=\"no.d.ts\" />\n";
    assert_eq!(reference_paths(text), ["globals.d.ts", "fs/promises.d.ts"]);
}

#[test]
fn rendering_is_sorted_compact_and_versioned() {
    let header = Header {
        typescript_version: "6.0.3",
        library_files: 2,
        node_version: "25.3.3",
        digest: "abc",
    };
    let names: Vec<String> = (0..40).rev().map(|index| format!("name{index:02}")).collect();
    let text = render(&header, &names, &["B".into(), "A".into()], &["fs".into()]);
    assert!(text.contains("TypeScript 6.0.3 (`lib.*.d.ts`, 2 files) and @types/node 25.3.3"));
    assert!(text.contains("pub(crate) const TYPESCRIPT_VERSION: &str = \"6.0.3\";"));
    assert!(text.contains("    \"A\", \"B\",\n"));
    assert!(text.lines().all(|line| line.len() <= 110));
    let first = text.find("\"name00\"").expect("present");
    let last = text.find("\"name39\"").expect("present");
    assert!(first < last);
}
