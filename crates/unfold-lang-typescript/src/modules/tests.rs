use super::*;

fn entries(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    list.iter()
        .map(|(pattern, substitutions)| {
            (
                (*pattern).to_owned(),
                substitutions.iter().map(|item| (*item).to_owned()).collect(),
            )
        })
        .collect()
}

#[test]
fn exact_patterns_win_then_the_longest_prefix() {
    let paths = entries(&[
        ("*", &["any/*"]),
        ("@app/*", &["src/*"]),
        ("@app/ui/*", &["ui/*"]),
        ("@app/exact", &["exact.ts"]),
    ]);
    let found =
        |specifier| best_pattern(&paths, specifier).map(|(subs, star)| (subs[0].clone(), star.to_owned()));
    assert_eq!(found("@app/exact"), Some(("exact.ts".to_owned(), String::new())));
    assert_eq!(
        found("@app/ui/button"),
        Some(("ui/*".to_owned(), "button".to_owned()))
    );
    assert_eq!(found("@app/x"), Some(("src/*".to_owned(), "x".to_owned())));
    assert_eq!(found("lodash"), Some(("any/*".to_owned(), "lodash".to_owned())));
    let suffixed = entries(&[("*.css", &["styles/*.css"])]);
    assert_eq!(best_pattern(&suffixed, "a.css").map(|(_, star)| star), Some("a"));
    assert!(best_pattern(&suffixed, "a.ts").is_none());
}

#[test]
fn outcomes_become_module_targets() {
    let path = RepoPath::new("legacy/a.ts").expect("path");
    assert_eq!(
        Resolved::Indexed(FileId(3)).module_target(),
        ModuleTarget::Indexed(FileId(3))
    );
    assert_eq!(
        Resolved::Failed(FileId(3)).module_target(),
        ModuleTarget::NotIndexed
    );
    assert_eq!(
        Resolved::Unindexed { path, excluded: true }.module_target(),
        ModuleTarget::External(ExternalLocation::File("legacy/a.ts".into()))
    );
    assert_eq!(
        Resolved::Package("express".into()).module_target(),
        ModuleTarget::External(ExternalLocation::Package("express".into()))
    );
    assert_eq!(Resolved::NotFound.module_target(), ModuleTarget::NotFound);
    assert_eq!(Resolved::Failed(FileId(1)).file(), Some(FileId(1)));
    assert!(is_relative(".") && is_relative("../a") && !is_relative(".a") && !is_relative("a"));
}
