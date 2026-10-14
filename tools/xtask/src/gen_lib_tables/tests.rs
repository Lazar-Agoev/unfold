use super::scan::Model;
use super::{Ecmascript, Header, is_ecmascript_file, is_library_file, reference_paths, render};

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
fn global_type_aliases_are_listed_through_namespaces() {
    let script = "type Partial<T> = { [P in keyof T]?: T[P] }; interface I {} \
                  declare namespace Intl { type LocalesArgument = string; interface Collator {} }";
    let module = "export type Hidden = 1; declare global { type Shown = 2; }";
    let model = model(&[script, module]);
    assert_eq!(
        model.library_type_aliases(),
        ["Intl.LocalesArgument", "Partial", "Shown"]
    );
    assert_eq!(model.library_types(), ["I", "Intl.Collator"]);
}

#[test]
fn files_and_references_are_recognized() {
    assert!(is_library_file("lib.es5.d.ts"));
    assert!(is_library_file("lib.dom.iterable.d.ts"));
    assert!(!is_library_file("lib.d.ts"));
    assert!(!is_library_file("typescript.d.ts"));
    assert!(is_ecmascript_file("lib.es5.d.ts"));
    assert!(is_ecmascript_file("lib.es2025.iterator.d.ts"));
    assert!(!is_ecmascript_file("lib.esnext.temporal.d.ts"));
    assert!(!is_ecmascript_file("lib.esnext.d.ts"));
    assert!(!is_ecmascript_file("lib.dom.d.ts"));
    assert!(!is_ecmascript_file("lib.decorators.d.ts"));
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
    let ecmascript = Ecmascript {
        files: 1,
        globals: vec!["JSON".into()],
        value_types: vec![("Object".into(), "ObjectConstructor".into())],
        types: vec!["Map".into(), "Iterable".into()],
        classes: vec!["Map".into(), "Array".into()],
        supertypes: vec![("AggregateError".into(), vec!["Error".into()])],
        methods: vec![("Array".into(), vec!["map".into(), "at".into()])],
    };
    let text = render(
        &header,
        &names,
        &["B".into(), "A".into()],
        &["fs".into()],
        &ecmascript,
    );
    assert!(text.contains("TypeScript 6.0.3 (`lib.*.d.ts`, 2 files) and @types/node 25.3.3"));
    assert!(text.contains("`lib.es*.d.ts` files alone (1 files)"));
    assert!(text.contains("    (\"Object\", \"ObjectConstructor\"),\n"));
    assert!(text.contains("    (\"Array\", &[\n        \"at\", \"map\",\n    ]),\n"));
    assert!(text.contains("ECMASCRIPT_CLASSES: &[&str] = &[\n    \"Array\", \"Map\",\n];"));
    assert!(text.contains("ECMASCRIPT_TYPES: &[&str] = &[\n    \"Iterable\", \"Map\",\n];"));
    assert!(text.contains("    (\"AggregateError\", &[\n        \"Error\",\n    ]),\n"));
    assert!(text.contains("pub(crate) const TYPESCRIPT_VERSION: &str = \"6.0.3\";"));
    assert!(text.contains("    \"A\", \"B\",\n"));
    assert!(text.lines().all(|line| line.len() <= 110));
    let first = text.find("\"name00\"").expect("present");
    let last = text.find("\"name39\"").expect("present");
    assert!(first < last);
}

#[test]
fn methods_merge_across_declarations_and_follow_extends() {
    let base = "interface IteratorObject<T> { map<U>(f: (v: T) => U): IteratorObject<U>; next(): T; \
                readonly size: number; [Symbol.iterator](): IteratorObject<T>; get x(): number; } \
                interface ArrayIterator<T> extends IteratorObject<T> { } \
                interface Array<T> { push(...items: T[]): number; length: number; } \
                interface ArrayConstructor { isArray(arg: any): arg is any[]; } \
                declare var Array: ArrayConstructor; declare var NaN: number; \
                declare var Inline: { f(): void }; \
                declare namespace Reflect { function ownKeys(target: object): PropertyKey[]; const x: number; } \
                declare namespace Intl { interface Options { a(): void; } interface Collator extends Options { compare(): number; } \
                interface Empty {} }";
    let more = "interface Array<T> { at(index: number): T | undefined; }";
    let model = model(&[base, more]);
    assert_eq!(
        model.methods().expect("no clashes"),
        [
            ("Array".to_owned(), vec!["at".to_owned(), "push".to_owned()]),
            ("ArrayConstructor".to_owned(), vec!["isArray".to_owned()]),
            (
                "ArrayIterator".to_owned(),
                vec!["map".to_owned(), "next".to_owned()]
            ),
            (
                "Intl.Collator".to_owned(),
                vec!["a".to_owned(), "compare".to_owned()]
            ),
            ("Intl.Options".to_owned(), vec!["a".to_owned()]),
            (
                "IteratorObject".to_owned(),
                vec!["map".to_owned(), "next".to_owned()]
            ),
            ("Reflect".to_owned(), vec!["ownKeys".to_owned()]),
        ]
    );
    assert_eq!(
        model.global_value_types(),
        [
            ("Array".to_owned(), "ArrayConstructor".to_owned()),
            ("Reflect".to_owned(), "Reflect".to_owned()),
        ]
    );
}

#[test]
fn a_namespace_and_an_interface_with_methods_cannot_share_a_name() {
    let file = "interface Clash { m(): void; } declare namespace Clash { function f(): void; }";
    assert!(model(&[file]).methods().is_err());
}

#[test]
fn class_types_construct_their_own_type_and_supertypes_are_transitive() {
    let file = "interface Map<K, V> { get(k: K): V; } \
                interface MapConstructor { readonly prototype: Map<any, any>; } \
                declare var Map: MapConstructor; \
                interface Array<T> {} interface ArrayConstructor { new (n?: number): any[]; } \
                declare var Array: ArrayConstructor; \
                interface Error {} interface ErrorConstructor { new (m?: string): Error; } \
                declare var Error: ErrorConstructor; \
                interface AggregateError extends Error {} \
                interface AggregateErrorConstructor extends ErrorConstructor { new (): AggregateError; } \
                declare var AggregateError: AggregateErrorConstructor; \
                interface Iterator<T> { next(): T; } interface IteratorConstructor { from(): unknown; } \
                declare var Iterator: IteratorConstructor; \
                interface ProxyConstructor { new <T>(target: T): T; } declare var Proxy: ProxyConstructor; \
                interface Wrapped {} declare var Wrapped: { new (): Wrapped }; \
                interface Deep extends AggregateError {} \
                declare namespace Intl { interface Collator {} \
                interface CollatorConstructor { new (): Collator; } var Collator: CollatorConstructor; }";
    let model = model(&[file]);
    // `Iterator` constructs nothing of its own, `Proxy` constructs its target, and `Wrapped`'s
    // type isn't written as a name; an `AggregateError` is constructed through its own signature.
    assert_eq!(
        model.classes(),
        ["AggregateError", "Array", "Error", "Intl.Collator", "Map"]
    );
    assert_eq!(
        model.supertypes(),
        [
            ("AggregateError".to_owned(), vec!["Error".to_owned()]),
            (
                "AggregateErrorConstructor".to_owned(),
                vec!["ErrorConstructor".to_owned()]
            ),
            (
                "Deep".to_owned(),
                vec!["AggregateError".to_owned(), "Error".to_owned()]
            ),
        ]
    );
}
