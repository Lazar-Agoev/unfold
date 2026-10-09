//! Grammar versions and failures: syntax newer than the configured version, encodings, syntax
//! errors, and the compile-time errors the proof of concept's grammar reported.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use common::{compile_files, testdata};
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::settings::{PythonVersion, Settings};
use unfold_engine::{Language, SourceFailure};
use unfold_lang_python::Python;

fn extract(text: &str, version: PythonVersion) -> Result<(), SourceFailure> {
    let snapshot = Snapshot::from_text(RepoPath::new("main.py").unwrap(), text).unwrap();
    unfold_lang_python::extract(&snapshot, version).map(|_| ())
}

/// Which of 3.12, 3.13, and 3.14 accept `text`.
fn accepted_by(text: &str) -> Vec<&'static str> {
    PythonVersion::ALL
        .iter()
        .filter(|version| extract(text, **version).is_ok())
        .map(|version| version.as_str())
        .collect()
}

#[test]
fn syntax_newer_than_the_configured_version_fails() {
    let all = ["3.12", "3.13", "3.14"];
    // PEP 695 type parameters and `type` aliases: 3.12.
    assert_eq!(
        accepted_by("def first[T](items: list[T]) -> T: return items[0]\n"),
        all
    );
    assert_eq!(
        accepted_by("class Box[T]: pass\ntype Pair[T] = tuple[T, T]\n"),
        all
    );
    // PEP 701 f-strings: 3.12.
    assert_eq!(accepted_by("name = f\"{\"nested\"}\"\n"), all);
    // PEP 696 type parameter defaults: 3.13.
    assert_eq!(accepted_by("def make[T = int](): pass\n"), ["3.13", "3.14"]);
    assert_eq!(accepted_by("class Box[T = str]: pass\n"), ["3.13", "3.14"]);
    // PEP 758 unparenthesized `except` and PEP 750 template strings: 3.14.
    assert_eq!(
        accepted_by("try:\n    pass\nexcept ValueError, TypeError:\n    pass\n"),
        ["3.14"]
    );
    assert_eq!(accepted_by("greeting = t\"hello {name}\"\n"), ["3.14"]);
    // Python 3.15 syntax fails under every supported version.
    assert_eq!(accepted_by("lazy import json\n"), Vec::<&str>::new());
    // PEP 765: the proof of concept's 3.14 grammar rejected leaving `finally` with a transfer.
    let finally_return = "def f():\n    try:\n        pass\n    finally:\n        return 1\n";
    assert_eq!(accepted_by(finally_return), ["3.12", "3.13"]);
    let finally_break = "for x in y:\n    try:\n        pass\n    finally:\n        break\n";
    assert_eq!(accepted_by(finally_break), ["3.12", "3.13"]);
    let inner_loop =
        "def f():\n    try:\n        pass\n    finally:\n        for x in y:\n            break\n";
    assert_eq!(accepted_by(inner_loop), all);
}

#[test]
fn the_configured_project_version_applies_per_file() {
    let settings = Settings::parse(
        "version = 1\n[[python.project]]\nroot = \"old\"\nversion = \"3.12\"\n[[python.project]]\nroot = \"new\"\n",
    )
    .unwrap();
    let text = "def make[T = int](): pass\n";
    let compiled = compile_files(
        &[("old/a.py", text), ("new/a.py", text), ("loose.py", text)],
        &settings,
    )
    .valid();
    let failed: Vec<&str> = compiled.failures.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(failed, ["old/a.py"]);
    let python = Python;
    let old = RepoPath::new("old/a.py").unwrap();
    let new = RepoPath::new("new/a.py").unwrap();
    assert_ne!(
        python.settings_hash(&old, &settings),
        python.settings_hash(&new, &settings)
    );
}

#[test]
fn syntax_errors_fail_with_a_message_and_offset() {
    let failure = extract("def broken(:\n    pass\n", PythonVersion::V3_14).unwrap_err();
    assert!(failure.offset.is_some(), "{failure}");
    let failure = extract("x = (1,\ny = 2\n", PythonVersion::V3_14).unwrap_err();
    assert!(!failure.message.is_empty());
    let failure = extract("value = t'x'\n", PythonVersion::V3_13).unwrap_err();
    assert_eq!(failure.offset, Some(8));
    assert!(failure.message.contains("3.13"), "{failure}");
}

#[test]
fn compile_time_errors_the_proof_of_concept_reported_fail() {
    let failing = [
        ("return 1\n", "'return' outside function"),
        ("class C:\n    return 1\n", "'return' outside function"),
        ("break\n", "'break' outside loop"),
        (
            "while x:\n    def f():\n        continue\n",
            "'continue' outside loop",
        ),
        (
            "for x in y:\n    pass\nelse:\n    break\n",
            "'break' outside loop",
        ),
        (
            "for x in y:\n    try:\n        pass\n    except* E:\n        break\n",
            "except* block",
        ),
        ("def f(a, a): pass\n", "duplicate argument 'a'"),
        ("f = lambda x, x: x\n", "duplicate argument 'x'"),
        (
            "try:\n    pass\nexcept:\n    pass\nexcept E:\n    pass\n",
            "must be last",
        ),
        (
            "match v:\n    case x:\n        pass\n    case 1:\n        pass\n",
            "irrefutable",
        ),
        (
            "match v:\n    case [a, a]:\n        pass\n",
            "multiple assignments to name 'a'",
        ),
        (
            "match v:\n    case [*a, *b]:\n        pass\n",
            "multiple starred names",
        ),
        ("match v:\n    case [a] | (b):\n        pass\n", "different names"),
        ("match v:\n    case _ | 1:\n        pass\n", "irrefutable"),
    ];
    for (text, message) in failing {
        let failure = extract(text, PythonVersion::V3_14).expect_err(text);
        assert!(failure.message.contains(message), "{text:?}: {failure}");
        assert!(failure.offset.is_some(), "{text:?}");
    }
    let passing = [
        "for x in y:\n    try:\n        pass\n    except* E:\n        for z in w:\n            break\n",
        "match v:\n    case x if x:\n        pass\n    case 1:\n        pass\n    case _:\n        pass\n",
        "match v:\n    case [a] | (a):\n        pass\n",
        "def f(*args, **kwargs):\n    yield args\n",
        "nonlocal_name = 1\nawait_name = 2\n",
    ];
    for text in passing {
        assert_eq!(extract(text, PythonVersion::V3_14), Ok(()), "{text:?}");
    }
}

#[test]
fn encodings_other_than_utf8_fail_and_bom_is_kept() {
    let latin1 =
        std::fs::read_to_string(testdata().join("fixtures/python/policy/encoding/latin1_cookie.py")).unwrap();
    let failure = extract(&latin1, PythonVersion::V3_14).unwrap_err();
    assert!(failure.message.contains("latin-1"), "{failure}");
    let utf8 =
        std::fs::read_to_string(testdata().join("fixtures/python/policy/encoding/utf8_cookie.py")).unwrap();
    assert_eq!(extract(&utf8, PythonVersion::V3_14), Ok(()));
    let compiled = compile_files(&[("bom.py", "\u{FEFF}def f(): pass\n")], &Settings::default()).valid();
    let root = &compiled.file("bom.py").blocks[0];
    assert_eq!(root.range.start, 0);
    assert_eq!(compiled.file("bom.py").blocks[1].range.start, 3);
}

/// All 23 policy fixtures: 22 compile and validate, the Latin-1 declaration fails.
#[test]
fn policy_fixtures_compile_with_their_read_failure() {
    let root = testdata().join("fixtures/python/policy");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    assert_eq!(files.len(), 23);
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let compiled = compile_files(&pairs, &Settings::default()).valid();
    let failed: Vec<&str> = compiled.failures.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(failed, ["encoding/latin1_cookie.py"]);
    let header_calls = compiled
        .file("headers.py")
        .blocks
        .iter()
        .filter(|block| {
            block.kind == unfold_core::vocab::BlockKind::Call
                && block.role == Some(unfold_core::vocab::Role::Header)
        })
        .count();
    assert!(header_calls >= 8, "{header_calls}");
    let kinds: Vec<_> = compiled
        .file("decorators.py")
        .blocks
        .iter()
        .map(|block| block.kind)
        .collect();
    assert!(kinds.contains(&unfold_core::vocab::BlockKind::Getter));
    assert!(kinds.contains(&unfold_core::vocab::BlockKind::Setter));
    let imports = |path: &str| {
        compiled
            .file(path)
            .edges
            .iter()
            .filter(|edge| edge.kind == unfold_core::vocab::EdgeKind::Imports)
            .count()
    };
    assert_eq!(imports("imports/multiple.py"), 3);
    assert_eq!(
        imports("imports/guards.py"),
        2,
        "typing and the unguarded `.literal_exports`"
    );
    assert_eq!(
        imports("imports/dynamic_imports.py"),
        4,
        "two imports and two proven dynamic imports"
    );
}

fn collect(root: &Path, folder: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|extension| extension == "py") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, std::fs::read_to_string(&path).unwrap()));
        }
    }
}
