//! The syntax mapping, case by case: each source in `testdata/fixtures/typescript-structure/`
//! is compiled and its blocks (key, role, text) and slots compared with the golden outline
//! beside it. Run with `UNFOLD_BLESS=1` to rewrite the goldens after an intended change.

use unfold_core::testkit::{assert_golden, testdata_dir};

use crate::support::outline;

const FOLDER: &str = "fixtures/typescript-structure";

fn check(name: &str) {
    let path = testdata_dir().join(FOLDER).join(format!("{name}.ts"));
    let source = std::fs::read_to_string(&path).expect("read the case");
    let mut text = outline("a.ts", &source).join("\n");
    text.push('\n');
    assert_golden(format!("{FOLDER}/{name}.outline"), &text);
}

#[test]
fn imports() {
    check("imports");
}

#[test]
fn classes() {
    check("classes");
}

#[test]
fn statements() {
    check("statements");
}

#[test]
fn destructuring() {
    check("destructuring");
}

#[test]
fn names() {
    check("names");
}

#[test]
fn calls() {
    check("calls");
}

#[test]
fn ambient() {
    check("ambient");
}
