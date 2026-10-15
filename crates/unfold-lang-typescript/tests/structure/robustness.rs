//! Extraction, linking, and highlighting never panic or overflow: arbitrary text, truncations of
//! real files, and pathologically deep nesting either extract or fail with a message; blocks
//! nested past the depth limit are truncated into a valid graph; and deep chains and cycles link
//! into a valid graph.

use proptest::prelude::*;
use unfold_core::highlight::Highlighter;
use unfold_core::testkit::{spans_are_well_formed, testdata_dir};
use unfold_core::vocab::{Flag, Form};
use unfold_core::{RepoPath, Snapshot};
use unfold_lang_typescript::{ExtractOptions, TypeScript, TypeScriptHighlighter, extract};

use crate::support::{compile, compile_source, sources};

fn snapshot(path: &str, text: &str) -> Snapshot {
    Snapshot::from_text(RepoPath::new(path).expect("valid path"), text).expect("valid text")
}

/// Extracts and highlights `text` as `.ts` and `.tsx`; neither may panic.
fn exercise(text: &str) {
    for path in ["a.ts", "a.tsx"] {
        // Success or a reported failure are both fine; a panic or overflow is not.
        let _facts = extract(&snapshot(path, text), ExtractOptions::default());
    }
    let spans = TypeScriptHighlighter.highlight(text);
    assert!(spans_are_well_formed(text, &spans), "{spans:?}");
}

#[test]
fn deep_nesting_extracts_or_fails_cleanly() {
    let shapes: [(&str, &str, &str); 12] = [
        ("(", "", ")"),
        ("[", "", "]"),
        ("{a:", "", "}"),
        ("!", "", ""),
        ("x=", "", ""),
        ("a?b:", "", ""),
        ("()=>", "", ""),
        ("f(", "", ")"),
        ("if(a){", "", "}"),
        ("function f(){", "", "}"),
        ("`${", "", "}`"),
        ("a.b", "", ""),
    ];
    for depth in [100, 5_000, 50_000] {
        for (open, middle, close) in shapes {
            let text = format!("{}0{middle}{}", open.repeat(depth), close.repeat(depth));
            exercise(&text);
        }
        exercise(&format!(
            "let x: {}number{};",
            "Array<".repeat(depth),
            ">".repeat(depth)
        ));
        exercise(&format!("if (a) {{}}{}", " else if (a) {}".repeat(depth)));
        exercise(&format!("x = 1{};", " + 1".repeat(depth)));
    }
}

/// Files over a megabyte are parsed after estimating how deeply they nest, and many sibling
/// statements are flat, not deep: thousands of typed functions, and one function holding
/// thousands of `if` statements, written with or without semicolons, all compile.
#[test]
fn large_flat_files_compile() {
    let functions: String = (0..9_000)
        .map(|index| {
            format!(
                "export function handler{index}(order: Order, count: number): Result {{ if (count > {index}) {{ return save(order); }} return skip(order); }}\n"
            )
        })
        .collect();
    let functions = format!(
        "interface Order {{}}\ninterface Result {{}}\ndeclare function save(o: Order): Result;\ndeclare function skip(o: Order): Result;\n{functions}"
    );
    let branches: String = (0..20_000)
        .map(|index| format!("  if (count > {index}) {{ work(count, {index}); }}\n"))
        .collect();
    let branches = format!(
        "declare function work(a: number, b: number): void;\nexport function many(count: number) {{\n{branches}}}\n"
    );
    let unterminated: String = (0..30_000)
        .map(|index| format!("export const value{index}: number = compute({index}, \"{index}\")\n"))
        .collect();
    let unterminated = format!("declare function compute(a: number, b: string): number;\n{unterminated}");
    for (path, source) in [
        ("functions.ts", &functions),
        ("branches.ts", &branches),
        ("unterminated.ts", &unterminated),
    ] {
        assert!(source.len() > 800_000, "{path} is {} bytes", source.len());
        compile_source(path, source).valid();
    }
    assert!(functions.len() > 1 << 20);
}

/// Blocks nested past the depth limit are left out, and the block they would sit in is marked
/// `truncated`: the graph stays valid, because the census leaves out the calls that were cut.
#[test]
fn nesting_past_the_depth_limit_is_truncated() {
    let depth = 300;
    let source = format!("{}work();{}\n", "if (a) {".repeat(depth), "}".repeat(depth));
    let compiled = compile_source("deep.ts", &source).valid();
    let nodes = &compiled.file("deep.ts").nodes;
    let truncated: Vec<_> = nodes
        .iter()
        .filter(|node| node.flags.contains(Flag::Truncated))
        .collect();
    assert_eq!(truncated.len(), 1);
    assert_eq!(truncated[0].form, Form::Then);
    assert!(
        nodes.iter().all(|node| node.form != Form::Call),
        "the call is past the limit"
    );
    // A long call chain nests each call in the next one's receiver.
    let chain = format!("start(){};\n", ".then()".repeat(400));
    let compiled = compile_source("chain.ts", &chain).valid();
    let nodes = &compiled.file("chain.ts").nodes;
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.flags.contains(Flag::Truncated))
            .count(),
        1
    );
    assert!(nodes.len() < 300);
}

/// A call that takes a callback is keyed by its first literal only while the callback fits
/// under the depth limit: one cut there leaves the key without it, and the graph valid.
#[test]
fn a_call_whose_callback_is_cut_drops_its_literal() {
    // 255 functions put the call at the limit, and its callback one past it.
    let depth = 255;
    let source = format!(
        "{}on(\"ready\", () => run());{}\n",
        "function f() {".repeat(depth),
        "}".repeat(depth)
    );
    let compiled = compile_source("deep.ts", &source).valid();
    let nodes = &compiled.file("deep.ts").nodes;
    let call = nodes
        .iter()
        .find(|node| node.form == Form::Call)
        .expect("the call fits");
    assert!(call.key.to_string().ends_with("/call:on"), "{}", call.key);
    assert!(call.flags.contains(Flag::Truncated));
    assert!(nodes.iter().all(|node| node.form != Form::Lambda));
}

#[test]
fn truncations_of_real_files_never_panic() {
    let root = testdata_dir().join("playgrounds/ts-orders");
    for (path, bytes) in sources(&root).into_iter().take(12) {
        let text = String::from_utf8(bytes).expect("UTF-8 source");
        let mut cuts: Vec<usize> = (0..text.len()).step_by(37).collect();
        cuts.push(text.len());
        for cut in cuts.into_iter().filter(|&cut| text.is_char_boundary(cut)) {
            let prefix = &text[..cut];
            let suffix = &text[cut..];
            exercise(prefix);
            exercise(suffix);
        }
        assert!(!path.is_empty());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_text_never_panics(text in "\\PC{0,200}") {
        exercise(&text);
    }

    #[test]
    fn arbitrary_syntax_soup_never_panics(
        parts in proptest::collection::vec(
            prop_oneof![
                Just("function"), Just("class"), Just("("), Just(")"), Just("{"), Just("}"), Just("["),
                Just("]"), Just("=>"), Just("`"), Just("${"), Just("'"), Just("\""), Just("/"), Just("/*"),
                Just("*/"), Just("//"), Just("\n"), Just("@"), Just("<"), Just(">"), Just("x"), Just("new"),
                Just("import"), Just("export"), Just("const"), Just("="), Just(","), Just(";"), Just(":"),
                Just("?."), Just("#p"), Just("1e"), Just("\\u{41}"), Just("é"), Just("😀"), Just(" "),
            ],
            0..60,
        )
    ) {
        exercise(&parts.concat());
    }
}

/// Linking and evidence stay bounded on deep chains and cycles: every program compiles into a
/// valid graph (deeply nested values simply resolve nothing) without overflowing.
#[test]
fn deep_chains_and_cycles_link_without_overflow() {
    let members = format!(
        "export const x = {{ a: 1 }};\n(x as any){}.f();\n",
        ".a".repeat(2_000)
    );
    let calls = format!(
        "export function f(): any {{ return f; }}\nf(){};\n",
        "()".repeat(900)
    );
    let mut classes = String::from("export class C0 { m() {} }\n");
    for index in 1..300 {
        classes.push_str(&format!("class C{index} extends C{} {{}}\n", index - 1));
    }
    classes.push_str("new C299().m();\nfunction use(c: C299) { c.m(); }\n");
    let cycles = "class A extends B { m() { super.m(); this.n(); } }\nclass B extends A { n() { super.n(); } }\nconst a: any = a.b;\na.c();\nconst o = { f() { o.f(); } };\no.f();\nexport {};\n";
    for (path, source) in [
        ("members.ts", members.as_str()),
        ("calls.ts", calls.as_str()),
        ("classes.ts", classes.as_str()),
        ("cycles.ts", cycles),
    ] {
        compile_source(path, source).valid();
    }
    let modules = [
        (
            "a.ts",
            "export * from './b.js';\nexport { y as x } from './b.js';\nimport { z } from './b.js';\nz();\n",
        ),
        (
            "b.ts",
            "export * from './a.js';\nexport { x as y, x as z } from './a.js';\nimport * as a from './a.js';\na.x();\n",
        ),
    ];
    let files: Vec<(String, Vec<u8>)> = modules
        .iter()
        .map(|(path, source)| ((*path).to_owned(), source.as_bytes().to_vec()))
        .collect();
    compile(&TypeScript::new(), &files).valid();
}

/// `export =` that assigns its own module, directly or through another module, links into a
/// valid graph: following the assignment for a member stops at the repeated module instead of
/// recursing until the stack overflows, and the member stays unresolved.
#[test]
fn export_equals_cycles_link_without_overflow() {
    let programs: [&[(&str, &str)]; 3] = [
        &[("a.ts", "import me = require(\"./a\");\nexport = me;\nme.foo();\n")],
        &[
            ("a.ts", "import b = require(\"./b\");\nexport = b;\n"),
            ("b.ts", "import a = require(\"./a\");\nexport = a;\na.foo();\n"),
        ],
        &[
            ("a.ts", "import me = require(\"./a\");\nexport = me;\n"),
            ("b.ts", "import { foo } from \"./a\";\nfoo();\n"),
        ],
    ];
    for program in programs {
        let files: Vec<(String, Vec<u8>)> = program
            .iter()
            .map(|(path, source)| ((*path).to_owned(), source.as_bytes().to_vec()))
            .collect();
        let compiled = compile(&TypeScript::new(), &files).valid();
        let calls: Vec<_> = compiled
            .graph
            .files
            .iter()
            .flat_map(|file| &file.links)
            .filter(|link| link.kind == unfold_core::vocab::LinkKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1, "{program:?}");
        assert_eq!(
            calls[0].to.outcome(),
            unfold_core::vocab::Outcome::Unknown,
            "{program:?}"
        );
    }
}
