use super::*;

fn parts(path: &str) -> Vec<&str> {
    path.split('/').collect()
}

fn glob(spec: &str) -> Vec<String> {
    pattern(spec, true)
}

#[test]
fn wildcards_stay_in_one_component_and_skip_hidden_names() {
    assert!(wildcard(b"*.ts", b"a.ts"));
    assert!(wildcard(b"a*b*c", b"aXbYc"));
    assert!(wildcard(b"?.ts", b"a.ts"));
    assert!(!wildcard(b"?.ts", b"ab.ts"));
    assert!(wildcard(b"*", b""));
    assert!(!component("*.ts", ".a.ts"));
    assert!(component(".a.ts", ".a.ts"));
}

#[test]
fn patterns_match_folders_and_implicit_directories() {
    assert!(matches(&glob("src"), &parts("src/a/b.ts")));
    assert!(matches(&glob("src/**/*.ts"), &parts("src/b.ts")));
    assert!(matches(&glob("src/**/*.ts"), &parts("src/x/y/b.ts")));
    assert!(!matches(&glob("src/**/*.ts"), &parts("src/.x/b.ts")));
    assert!(!matches(&glob("src/*.ts"), &parts("src/x/b.ts")));
    assert!(matches(&pattern("dist", false), &parts("dist")));
    assert!(!matches(&pattern("dist", false), &parts("dist/a.ts")));
}

#[test]
fn outputs_change_extensions_beneath_the_out_dir() {
    assert_eq!(
        output_path("src/a.ts", "src", Some("dist"), false).as_deref(),
        Some("dist/a.js")
    );
    assert_eq!(
        output_path("src/a.tsx", "src", Some("dist"), true).as_deref(),
        Some("dist/a.jsx")
    );
    assert_eq!(
        output_path("src/a.tsx", "src", None, false).as_deref(),
        Some("src/a.js")
    );
    assert_eq!(output_path("a.cts", "", None, false).as_deref(), Some("a.cjs"));
    assert_eq!(output_path("lib/a.ts", "src", Some("dist"), false), None);
    assert!(emits("a.mts", false) && !emits("a.d.ts", true) && !emits("a.js", false) && emits("a.js", true));
}
