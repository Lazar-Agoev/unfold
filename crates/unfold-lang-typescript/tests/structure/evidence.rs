//! Binding and evidence, case by case: every folder under
//! `testdata/fixtures/typescript-evidence/` is compiled with its configuration, validated, and its
//! links compared with the folder's `expected.links` golden (`UNFOLD_BLESS=1` rewrites them).
//!
//! The `poc-*` folders are the inline fixtures of the proof of concept's native binding,
//! classes, declared-type, narrowing, receiver, value, and reference tests; the others cover the
//! binder rules, the override census, overloads, script globals, and the fixed defects (C-2,
//! C-4, C-6, C-8). The folder's `README.md` lists where the goldens deliberately differ from the
//! proof of concept's native producer.

use std::fmt::Write as _;

use unfold_core::Graph;
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_lang_typescript::TypeScript;

use crate::support::{compile_with, folder_inputs, link_line, sources};

const FOLDER: &str = "fixtures/typescript-evidence";

/// One line per link: `from #seq kind `text` → outcome`, where the outcome is the target key,
/// the library or builtin module, or the reason.
pub fn link_lines(graph: &Graph) -> String {
    let mut out = String::new();
    for file in &graph.files {
        if let Some(failure) = &file.file.failure {
            let _ = writeln!(out, "{} failed: {failure}", file.file.path);
        }
        for link in &file.links {
            let from = &file.nodes[link.from as usize].key;
            let _ = writeln!(out, "{from} #{} {}", link.seq, link_line(link));
        }
    }
    out
}

#[test]
fn every_evidence_case_matches_its_golden() {
    let root = testdata_dir().join(FOLDER);
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .expect("read the evidence cases")
        .map(|entry| entry.expect("case entry").path())
        .filter(|path| path.is_dir())
        .collect();
    cases.sort();
    assert!(cases.len() >= 100, "found only {} cases", cases.len());
    for case in cases {
        let name = case
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let files = sources(&case);
        let compiled = compile_with(&TypeScript::new(), &files, &folder_inputs(&case));
        let violations: Vec<String> = compiled.violations.iter().map(ToString::to_string).collect();
        assert!(violations.is_empty(), "{name}: {}", violations.join("\n"));
        assert_golden(
            format!("{FOLDER}/{name}/expected.links"),
            &link_lines(&compiled.graph),
        );
    }
}
