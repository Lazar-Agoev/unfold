//! Extraction, linking, and highlighting never panic: arbitrary strings, truncations of real
//! files, and pathologically deep nesting all end in facts or a reported failure (nesting past
//! the depth limit is a `truncated` node, and the graph stays valid), and long chains of
//! aliases, bases, and attributes link without exhausting the stack.

use crate::common::{compile_files, testdata};
use proptest::prelude::*;
use unfold_core::highlight::Highlighter;
use unfold_core::testkit::spans_are_well_formed;
use unfold_core::vocab::Flag;
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::settings::{PythonVersion, Settings};
use unfold_lang_python::PythonHighlighter;

fn exercise(text: &str) {
    let snapshot = Snapshot::from_text(RepoPath::new("fuzz.py").expect("a path"), text).expect("text");
    for version in [PythonVersion::V3_12, PythonVersion::V3_14] {
        if let Ok(facts) = unfold_lang_python::extract(&snapshot, version) {
            let bytes = facts.to_bytes().expect("facts encode");
            assert!(unfold_engine::FileFacts::from_bytes(&bytes).is_ok());
        }
    }
    let spans = PythonHighlighter.highlight(text);
    assert!(spans_are_well_formed(text, &spans), "{text:?}");
}

/// Python-flavored fragments, so random input reaches deep into the grammar.
fn fragment() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "def ",
        "class ",
        "lambda",
        ":",
        "(",
        ")",
        "[",
        "]",
        "{",
        "}",
        ",",
        ".",
        "=",
        ":=",
        "*",
        "**",
        "@",
        "->",
        "\n",
        "    ",
        "\t",
        "if ",
        "elif ",
        "else",
        "for ",
        " in ",
        "while ",
        "try",
        "except",
        "except*",
        "finally",
        "with ",
        " as ",
        "match ",
        "case ",
        "return",
        "yield",
        "raise",
        "import ",
        "from ",
        "type ",
        "async ",
        "await ",
        "x",
        "y1",
        "κ",
        "𐐀",
        "f\"",
        "t'",
        "{x}",
        "'",
        "\"",
        "\"\"\"",
        "#",
        "\\",
        "1",
        "0x1f",
        "1.5j",
        "TYPE_CHECKING",
        "property",
        "importlib.import_module(\"m\")",
        "Literal[",
        "Annotated[",
        " | ",
        "\"a.b\"",
        "global ",
        "nonlocal ",
        "del ",
        ";",
        "\r\n",
        "\u{FEFF}",
        "# coding: latin-1\n",
    ])
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_strings_never_panic(text in ".{0,200}") {
        exercise(&text);
    }

    #[test]
    fn python_fragments_never_panic(parts in prop::collection::vec(fragment(), 0..60)) {
        exercise(&parts.concat());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Whatever parses links into a valid graph.
    #[test]
    fn python_fragments_link_into_valid_graphs(
        first in prop::collection::vec(fragment(), 0..40),
        second in prop::collection::vec(fragment(), 0..40),
    ) {
        let first = first.concat();
        let second = second.concat();
        compile_files(&[("a.py", &first), ("pkg/__init__.py", &second), ("pkg/b.py", &first)], &Settings::default())
            .valid();
    }
}

#[test]
fn truncations_of_real_files_never_panic() {
    let root = testdata().join("playgrounds/py-orders/src/orders_playground");
    let mut files = Vec::new();
    for folder in [root.clone(), root.join("features"), root.join("services")] {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "py") {
                files.push(std::fs::read_to_string(path).unwrap());
            }
        }
    }
    files.sort();
    assert!(files.len() > 10);
    for text in &files {
        let cuts: Vec<usize> = text.char_indices().map(|(index, _)| index).step_by(5).collect();
        for cut in cuts {
            exercise(&text[..cut]);
        }
    }
}

/// Runs `body` on a thread with rayon's default stack size, where extraction runs.
fn on_small_stack(body: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(body)
        .expect("a thread")
        .join()
        .expect("no stack overflow or panic");
}

#[test]
fn deep_nesting_is_truncated_not_overflowed() {
    on_small_stack(|| {
        let depth = 20_000;
        let mut deep_ifs = String::new();
        for level in 0..1_500 {
            deep_ifs.push_str(&" ".repeat(level));
            deep_ifs.push_str("if x:\n");
        }
        deep_ifs.push_str(&" ".repeat(1_500));
        deep_ifs.push_str("pass\n");
        // Each case, and whether it nests blocks past the depth limit.
        let cases = [
            (format!("x = {}1\n", "-".repeat(depth)), false),
            (
                format!("x = {}1{}\n", "(".repeat(depth), ")".repeat(depth)),
                false,
            ),
            (
                format!("x = {}1{}\n", "[".repeat(depth), "]".repeat(depth)),
                false,
            ),
            (
                format!("x = {}1{}\n", "f(".repeat(depth / 4), ")".repeat(depth / 4)),
                true,
            ),
            (format!("x = {}1\n", "1 if 1 else ".repeat(depth)), false),
            (format!("x = {}1\n", "f() if 1 else ".repeat(depth / 4)), true),
            (format!("x = {}1\n", "lambda: ".repeat(depth)), true),
            (format!("x = {}1\n", "1**".repeat(depth)), false),
            (format!("x = a{}()\n", ".b".repeat(depth)), false),
            (
                format!("x = {}1{}\n", "1+(".repeat(depth), ")".repeat(depth)),
                false,
            ),
            (
                format!("x: {}int{}\n", "list[".repeat(depth), "]".repeat(depth)),
                false,
            ),
            (format!("x: {}int\n", "int | ".repeat(depth)), false),
            (
                format!(
                    "def f(x: a{}) -> \"a{}\": pass\n",
                    ".b".repeat(depth),
                    ".b".repeat(depth)
                ),
                false,
            ),
            (deep_ifs, true),
        ];
        for (text, truncated) in &cases {
            let spans = PythonHighlighter.highlight(text);
            assert!(spans_are_well_formed(text, &spans));
            let compiled = compile_files(&[("deep.py", text)], &Settings::default());
            if !compiled.failures.is_empty() {
                assert!(!truncated, "{:?}", compiled.failures);
                continue;
            }
            let compiled = compiled.valid();
            let file = compiled.file("deep.py");
            let marked = file
                .nodes
                .iter()
                .filter(|node| node.flags.contains(Flag::Truncated))
                .count();
            assert_eq!(marked > 0, *truncated, "{}", &text[..text.len().min(40)]);
        }
    });
}

#[test]
fn long_chains_link_without_overflowing() {
    on_small_stack(|| {
        let mut aliases = String::from("def f(): return 1\na0 = f\n");
        for index in 1..3_000 {
            aliases.push_str(&format!("a{index} = a{}\n", index - 1));
        }
        aliases.push_str("a2999()\n");
        let mut classes = String::from("class C0:\n    def m(self): return 1\n");
        for index in 1..1_000 {
            classes.push_str(&format!("class C{index}(C{}):\n    pass\n", index - 1));
        }
        classes.push_str("class Last(C999):\n    def run(self): return super().m()\nLast().m()\n");
        let attributes = format!(
            "import pkg\nx = pkg{}\nx()\ny: pkg{}\n",
            ".a".repeat(5_000),
            ".a".repeat(5_000)
        );
        // Star imports, and a type re-exported through every module.
        let modules = (0..300)
            .map(|index| {
                format!(
                    "from m{} import *\nfrom r{} import Cart\n__all__ = ['f']\nf = {index}\nc: Cart\n",
                    index + 1,
                    index + 1
                )
            })
            .collect::<Vec<_>>();
        let reexports = (0..300).map(|index| {
            let text = if index == 299 {
                "class Cart:\n    pass\n".to_owned()
            } else {
                format!(
                    "from r{} import Cart\n\n\ndef f(c: Cart) -> \"Cart\":\n    pass\n",
                    index + 1
                )
            };
            (format!("r{index}.py"), text)
        });
        let mut files: Vec<(String, String)> = vec![
            ("aliases.py".into(), aliases),
            ("classes.py".into(), classes),
            ("attributes.py".into(), attributes),
        ];
        files.extend(
            modules
                .into_iter()
                .enumerate()
                .map(|(index, text)| (format!("m{index}.py"), text)),
        );
        files.extend(reexports);
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        compile_files(&pairs, &Settings::default()).valid();
    });
}
