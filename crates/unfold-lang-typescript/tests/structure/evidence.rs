//! Binding and evidence, case by case: every folder under
//! `testdata/fixtures/typescript-evidence/` is compiled with its configuration, validated, and its
//! call, construction, reference, and import edges compared with the folder's `expected.edges`
//! golden (`UNFOLD_BLESS=1` rewrites them).
//!
//! The `poc-*` folders are the inline fixtures of the proof of concept's native binding,
//! classes, declared-type, narrowing, receiver, value, and reference tests; the others cover the
//! binder rules, the override census, overloads, script globals, and the fixed defects (C-2,
//! C-4, C-6, C-8). Every golden was checked against the proof of concept's native producer:
//! they agree edge for edge except where the folder's `README.md` lists a fixed defect.

use std::fmt::Write as _;

use unfold_core::Graph;
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_lang_typescript::TypeScript;

use crate::support::{compile_with, folder_inputs, sources};

const FOLDER: &str = "fixtures/typescript-evidence";

/// One line per edge: `source #seq kind text -> outcome`, where the outcome is the target
/// key, the external module, or the reason (with a parameter position).
pub fn edge_lines(graph: &Graph) -> String {
    let mut out = String::new();
    for file in &graph.files {
        if let Some(failure) = &file.file.failure {
            let _ = writeln!(out, "{} failed: {failure}", file.file.path);
        }
        for edge in &file.edges {
            let source = &file.blocks[edge.source as usize].key;
            let outcome = match (&edge.target, &edge.module, edge.reason) {
                (Some(target), _, _) => target.as_str().to_owned(),
                (_, Some(module), _) => format!("external {}", module.as_str()),
                (_, _, Some(reason)) => match edge.position {
                    Some(position) => format!("{reason} @{position}"),
                    None => reason.to_string(),
                },
                _ => edge.resolution.to_string(),
            };
            let position = match (edge.target.is_some(), edge.position) {
                (true, Some(position)) => format!(" @{position}"),
                _ => String::new(),
            };
            let _ = writeln!(
                out,
                "{source} #{} {} `{}` -> {outcome}{position}",
                edge.seq, edge.kind, edge.text
            );
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
            format!("{FOLDER}/{name}/expected.edges"),
            &edge_lines(&compiled.graph),
        );
    }
}
