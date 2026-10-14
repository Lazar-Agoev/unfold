use super::*;

fn error(text: &str) -> (String, u32, u32) {
    match Settings::parse(text) {
        Err(SettingsError::Invalid {
            message,
            line,
            column,
            ..
        }) => (message.into(), line, column),
        other => panic!("expected an invalid-settings error for {text:?}, got {other:?}"),
    }
}

fn assert_error(text: &str, line: u32, column: u32, needle: &str) {
    let (message, at_line, at_column) = error(text);
    assert!(message.contains(needle), "{message:?} should mention {needle:?}");
    assert_eq!((at_line, at_column), (line, column), "{message}");
}

const EXAMPLE: &str = r#"# Unfold settings for this repository. Commit this file to share them.
languages = ["typescript", "python"]

[scope]
include = []
exclude = ["**/generated/**"]

[[python.project]]
root = "py-playground"
source_roots = ["src"]
version = "3.12"

"#;

#[test]
fn the_documented_example_parses() {
    let settings = Settings::parse(EXAMPLE).unwrap();
    assert_eq!(
        settings.languages,
        Some(vec![Language::TypeScript, Language::Python])
    );
    assert_eq!(settings.scope.exclude(), ["**/generated/**".into()]);
    assert!(settings.scope.include().is_empty());

    let [project] = settings.python_projects.as_slice() else {
        panic!("one project")
    };
    assert_eq!(project.root.as_str(), "py-playground");
    let roots: Vec<&str> = project
        .source_roots
        .iter()
        .flatten()
        .map(FolderPath::as_str)
        .collect();
    assert_eq!(roots, ["py-playground/src"]);
    assert_eq!(project.version, PythonVersion::V3_12);
}

#[test]
fn an_absent_or_empty_file_means_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Settings::load(dir.path()).unwrap(), Settings::default());
    assert_eq!(Settings::parse("").unwrap(), Settings::default());
    assert_eq!(
        Settings::parse("# only a comment\n").unwrap(),
        Settings::default()
    );
    let defaults = Settings::default();
    assert!(defaults.language_enabled(Language::Python));
    assert!(defaults.language_enabled(Language::TypeScript));
    assert_eq!(
        defaults.python_version_for(&RepoPath::new("a.py").unwrap()),
        PythonVersion::V3_14
    );
}

#[test]
fn load_reads_the_settings_file_and_reports_encoding_errors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(SETTINGS_PATH), "languages = [\"python\"]\n").unwrap();
    let settings = Settings::load(dir.path()).unwrap();
    assert!(!settings.language_enabled(Language::TypeScript));
    std::fs::write(dir.path().join(SETTINGS_PATH), b"[scope]\n\xff").unwrap();
    assert!(matches!(
        Settings::load(dir.path()),
        Err(SettingsError::Encoding { offset: 8 })
    ));
}

#[test]
fn unknown_keys_are_rejected_with_their_position() {
    assert_error("surprise = true\n", 1, 1, "surprise");
    assert_error("version = 1\n", 1, 1, "version");
    assert_error("languages = [\"python\"]\n[scope]\ninclde = []\n", 3, 1, "inclde");
    assert_error(
        "[[python.project]]\nroot = \".\"\nsourceRoots = [\"src\"]\n",
        3,
        1,
        "sourceRoots",
    );
    assert_error("[python]\nversion = \"3.12\"\n", 2, 1, "version");
}

#[test]
fn malformed_toml_and_wrong_types_are_rejected() {
    let (message, line, _) = error("[scope]\nlanguages = [\"python\"\n");
    assert!(!message.is_empty());
    assert!(line >= 2);
    assert_error("[scope]\ninclude = \"src\"\n", 2, 11, "sequence");
    assert_error("[scope]\nexclude = [1]\n", 2, 12, "string");
}

#[test]
fn invalid_values_are_reported_precisely() {
    assert_error("languages = []\n", 1, 13, "at least one language");
    assert_error(
        "languages = [\"python\", \"rust\"]\n",
        1,
        24,
        "unknown language `rust`",
    );
    assert_error("[scope]\nexclude = [\"src/[a\"]\n", 2, 12, "not a valid glob");
    assert_error("[scope]\ninclude = [\"/abs\"]\n", 2, 12, "relative");
    assert_error("[scope]\ninclude = [\"\"]\n", 2, 12, "empty");
}

#[test]
fn evidence_files_are_confined_patterns() {
    let settings =
        Settings::parse("[evidence]\nfiles = [\"prisma/schema.prisma\", \"db/migrations/*.sql\"]\n").unwrap();
    assert_eq!(
        settings.evidence.patterns(),
        ["prisma/schema.prisma".into(), "db/migrations/*.sql".into()]
    );
    assert!(
        settings
            .evidence
            .contains(&RepoPath::new("db/migrations/001.sql").unwrap())
    );
    assert!(
        Settings::parse("[evidence]\n")
            .unwrap()
            .evidence
            .patterns()
            .is_empty()
    );
    assert_ne!(
        settings.hash(),
        Settings::default().hash(),
        "evidence patterns are settings"
    );
    assert_error("[evidence]\nfiles = [\"../secrets.sql\"]\n", 2, 10, "`..`");
    assert_error("[evidence]\nfiles = [\"/etc/hosts\"]\n", 2, 10, "relative");
    assert_error("[evidence]\nfiles = [\"db/[a\"]\n", 2, 10, "not a valid glob");
    assert_error("[evidence]\nfiles = [\"\"]\n", 2, 10, "empty");
    assert_error("[evidence]\nfiles = \"db/*.sql\"\n", 2, 9, "sequence");
    assert_error("[evidence]\npatterns = []\n", 2, 1, "patterns");
}

#[test]
fn python_projects_are_validated() {
    assert_error(
        "[[python.project]]\nversion = \"3.15\"\n",
        2,
        11,
        "unsupported Python grammar version \"3.15\"",
    );
    assert_error("[[python.project]]\nversion = 3.12\n", 2, 11, "string");
    assert_error(
        "[[python.project]]\nsource_roots = []\n",
        2,
        16,
        "at least one folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"../outside\"]\n",
        2,
        17,
        "inside the folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"/outside\"]\n",
        2,
        17,
        "relative",
    );
    assert_error(
        "[[python.project]]\nroot = \"../up\"\n",
        2,
        8,
        "inside the folder",
    );
    assert_error(
        "[[python.project]]\nsource_roots = [\"src\", \"./src/\"]\n",
        2,
        24,
        "duplicate Python source root `src`",
    );
    assert_error(
        "[[python.project]]\nroot = \"a\"\n[[python.project]]\nroot = \"a/\"\n",
        4,
        8,
        "two `[[python.project]]` tables",
    );
    let settings =
        Settings::parse("[[python.project]]\nversion = \"3.14\"\nsource_roots = [\".\"]\n").unwrap();
    assert_eq!(settings.python_projects[0].root, FolderPath::ROOT);
    assert_eq!(
        settings.python_projects[0].source_roots,
        Some(vec![FolderPath::ROOT])
    );
}

#[test]
fn errors_display_the_file_line_and_column() {
    let error = Settings::parse("\n\n  [scope]\n  include = [\"/abs\"]\n").unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("unfold.toml:4:14: "), "{text}");
}

#[test]
fn the_settings_hash_ignores_formatting_but_not_values() {
    let base = Settings::parse(EXAMPLE).unwrap().hash();
    let reformatted = EXAMPLE
        .replace("# Unfold settings", "# Changed comment")
        .replace("[scope]\n", "\n\n[scope]\n");
    assert_eq!(Settings::parse(&reformatted).unwrap().hash(), base);
    let changed = EXAMPLE.replace("version = \"3.12\"", "version = \"3.13\"");
    assert_ne!(Settings::parse(&changed).unwrap().hash(), base);
    assert_ne!(Settings::default().hash(), base);
    assert_eq!(
        Settings::default().hash(),
        Settings::parse("[scope]").unwrap().hash()
    );
    assert_eq!(base.to_string().len(), 64);
}

#[test]
fn python_hashes_follow_the_owning_project_only() {
    let text =
        "[[python.project]]\nroot = \"a\"\nversion = \"3.12\"\n\n[[python.project]]\nroot = \"a/inner\"\n";
    let settings = Settings::parse(text).unwrap();
    let path = |p: &str| RepoPath::new(p).unwrap();
    assert_eq!(
        settings
            .python_project_for(&path("a/x.py"))
            .unwrap()
            .root
            .as_str(),
        "a"
    );
    assert_eq!(
        settings
            .python_project_for(&path("a/inner/x.py"))
            .unwrap()
            .root
            .as_str(),
        "a/inner"
    );
    assert!(settings.python_project_for(&path("b/x.py")).is_none());
    assert_eq!(settings.python_version_for(&path("a/x.py")), PythonVersion::V3_12);
    assert_eq!(
        settings.python_version_for(&path("a/inner/x.py")),
        PythonVersion::V3_14
    );

    let changed = Settings::parse(&text.replace("3.12", "3.13")).unwrap();
    assert_ne!(
        changed.python_hash(&path("a/x.py")),
        settings.python_hash(&path("a/x.py"))
    );
    assert_eq!(
        changed.python_hash(&path("a/inner/x.py")),
        settings.python_hash(&path("a/inner/x.py"))
    );
    assert_eq!(
        changed.python_hash(&path("b/x.py")),
        Settings::default().python_hash(&path("b/x.py"))
    );
}

#[test]
fn removed_tables_are_unknown() {
    for (text, table) in [
        ("[project]\nname = \"api\"\n", "project"),
        ("[detectors]\ndisabled = []\n", "detectors"),
        ("[[application]]\nid = \"a\"\n", "application"),
        ("[[entrance]]\nid = \"a\"\n", "entrance"),
        ("[[entrance_override]]\nlabel = \"a\"\n", "entrance_override"),
    ] {
        let (message, line, _) = error(text);
        assert!(message.contains(table), "{message:?} should mention {table:?}");
        assert_eq!(line, 1, "{message}");
    }
}

#[test]
fn testdata_settings_parse() {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    for folder in [
        "fixtures/python/policy/single-root",
        "fixtures/python/policy/duplicate-roots",
    ] {
        let settings =
            Settings::load(&testdata.join(folder)).unwrap_or_else(|error| panic!("{folder}: {error}"));
        assert_eq!(settings.python_projects.len(), 1, "{folder}");
    }
}
