use super::*;

const COMMENTED: &str = "# Shared settings.\nlanguages = [\"typescript\"]\n\n[scope]\n# generated code\nexclude = [\"**/generated/**\"]\n\n# The API\n[[app]]\nname = \"Shop API\"  # shown on Home\nrun  = \"npm start\"\nentrances = [\n  # orders\n  { name = \"Place an order\", on = \"POST /orders\", code = \"src/a.ts#placeOrder\" },\n]\n";

fn edit(current: Option<&str>, edits: &[SettingsEdit]) -> EditedSettings {
    apply_edits(current, edits).unwrap_or_else(|error| panic!("{edits:?}: {error}"))
}

fn is_conflict(error: &EditError) -> bool {
    matches!(error, EditError::Conflict(_))
}

fn is_invalid_at_a_line(error: &EditError) -> bool {
    matches!(error, EditError::Invalid(SettingsError::Invalid { .. }))
}

fn set_entrance(app: &str, name: &str, on: Option<&str>, code: &[&str]) -> SettingsEdit {
    SettingsEdit::SetEntrance {
        app: app.into(),
        entrance: EntranceSpec {
            name: name.into(),
            on: on.map(Into::into),
            code: code.iter().map(|&key| key.into()).collect(),
            about: None,
        },
    }
}

fn remove(app: &str, entrance: Option<&str>) -> SettingsEdit {
    SettingsEdit::RemoveEntrance {
        app: app.into(),
        entrance: entrance.map(Into::into),
    }
}

#[test]
fn edits_keep_comments_and_formatting() {
    let edited = edit(
        Some(COMMENTED),
        &[SettingsEdit::ExcludeFromGraph {
            path: "src/legacy".into(),
        }],
    );
    assert!(
        edited
            .text
            .starts_with("# Shared settings.\nlanguages = [\"typescript\"]\n\n[scope]\n# generated code\n")
    );
    assert!(
        edited
            .text
            .contains("# The API\n[[app]]\nname = \"Shop API\"  # shown on Home\n")
    );
    assert_eq!(
        edited.file.settings.scope.exclude(),
        ["**/generated/**".into(), "src/legacy".into()]
    );
    let back = edit(
        Some(&edited.text),
        &[SettingsEdit::IncludeInGraph {
            path: "src/legacy".into(),
        }],
    );
    assert_eq!(back.file.settings.scope.exclude(), ["**/generated/**".into()]);
    assert_eq!(back.text, COMMENTED);
}

#[test]
fn edits_return_the_diff() {
    let edited = edit(
        Some(COMMENTED),
        &[SettingsEdit::AddEvidencePattern { pattern: "db".into() }],
    );
    assert_eq!(
        edited.diff,
        "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -13,3 +13,6 @@\n   # orders\n   { name = \"Place an order\", on = \"POST /orders\", code = \"src/a.ts#placeOrder\" },\n ]\n+\n+[evidence]\n+files = [\"db\"]\n"
    );
    let same = edit(
        Some(&edited.text),
        &[SettingsEdit::AddEvidencePattern { pattern: "db".into() }],
    );
    assert_eq!(same.diff, "", "an edit that changes nothing has no diff");
    let created = edit(None, &[SettingsEdit::GraphOnly { path: "src".into() }]);
    assert_eq!(
        created.diff,
        "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -0,0 +1,2 @@\n+[scope]\n+include = [\"src\"]\n"
    );
}

#[test]
fn scope_edits_follow_the_rules() {
    let only = edit(None, &[SettingsEdit::GraphOnly { path: "src".into() }]);
    assert_eq!(only.file.settings.scope.include(), ["src".into()]);
    let conflict = apply_edits(
        Some("[scope]\nexclude = [\"src/**\"]\n"),
        &[SettingsEdit::IncludeInGraph {
            path: "src/a.ts".into(),
        }],
    )
    .expect_err("conflict");
    assert!(is_conflict(&conflict), "{conflict}");
    let widened = edit(
        Some(&only.text),
        &[SettingsEdit::IncludeInGraph {
            path: "scripts".into(),
        }],
    );
    assert_eq!(
        widened.file.settings.scope.include(),
        ["src".into(), "scripts".into()]
    );
    let narrowed = edit(
        Some(&widened.text),
        &[SettingsEdit::ExcludeFromGraph { path: "src".into() }],
    );
    assert_eq!(narrowed.file.settings.scope.include(), ["scripts".into()]);
    assert_eq!(escape_glob("a[1]/*.ts"), "a\\[1\\]/\\*.ts");
}

#[test]
fn python_projects_are_set_and_removed() {
    let python = edit(
        None,
        &[SettingsEdit::SetPythonProject {
            project: PythonProjectSpec {
                root: "py".into(),
                source_roots: Some(vec!["src".into()]),
                version: Some("3.12".into()),
            },
        }],
    );
    assert_eq!(python.file.settings.python_projects.len(), 1);
    assert!(!python.text.contains("[python]\n"), "{}", python.text);
    let bad = apply_edits(
        Some(&python.text),
        &[SettingsEdit::SetPythonProject {
            project: PythonProjectSpec {
                root: "py".into(),
                version: Some("2.7".into()),
                ..PythonProjectSpec::default()
            },
        }],
    )
    .expect_err("unsupported grammar");
    assert!(is_invalid_at_a_line(&bad), "{bad}");
    let gone = edit(
        Some(&python.text),
        &[SettingsEdit::RemovePythonProject { root: "./py/".into() }],
    );
    assert!(gone.file.settings.python_projects.is_empty());
}

#[test]
fn scope_patterns_are_removed_as_written() {
    let text = "[scope]\n# keep me\ninclude = [\"src\"]\nexclude = [\"**/generated/**\", \"src/legacy\"]\n";
    let edited = edit(
        Some(text),
        &[SettingsEdit::RemoveScopePattern {
            list: ScopeList::Exclude,
            pattern: "**/generated/**".into(),
        }],
    );
    assert_eq!(edited.file.settings.scope.exclude(), ["src/legacy".into()]);
    assert!(edited.text.contains("# keep me"), "{}", edited.text);
    let emptied = edit(
        Some(&edited.text),
        &[SettingsEdit::RemoveScopePattern {
            list: ScopeList::Include,
            pattern: "src".into(),
        }],
    );
    assert!(emptied.file.settings.scope.include().is_empty());
    assert!(!emptied.text.contains("include"), "{}", emptied.text);
    let missing = apply_edits(
        Some(&emptied.text),
        &[SettingsEdit::RemoveScopePattern {
            list: ScopeList::Include,
            pattern: "src".into(),
        }],
    )
    .expect_err("not listed");
    assert!(is_conflict(&missing), "{missing}");
}

#[test]
fn evidence_patterns_are_added_once_and_removed_as_written() {
    let add = |pattern: &str| SettingsEdit::AddEvidencePattern {
        pattern: pattern.into(),
    };
    let remove = |pattern: &str| SettingsEdit::RemoveEvidencePattern {
        pattern: pattern.into(),
    };
    let added = edit(
        Some(COMMENTED),
        &[
            add("prisma/schema.prisma"),
            add("db/migrations/*.sql"),
            add("prisma/schema.prisma"),
        ],
    );
    assert!(added.text.starts_with(COMMENTED), "{}", added.text);
    assert_eq!(
        added.file.settings.evidence.patterns(),
        ["prisma/schema.prisma".into(), "db/migrations/*.sql".into()]
    );
    let commented = added.text.replace("[evidence]\n", "[evidence]\n# for boards\n");
    let removed = edit(Some(&commented), &[remove("prisma/schema.prisma")]);
    assert_eq!(
        removed.file.settings.evidence.patterns(),
        ["db/migrations/*.sql".into()]
    );
    assert!(removed.text.contains("# for boards"), "{}", removed.text);
    let emptied = edit(
        Some(&added.text),
        &[remove("prisma/schema.prisma"), remove("db/migrations/*.sql")],
    );
    assert_eq!(emptied.text, COMMENTED, "the emptied table goes");
    let missing = apply_edits(Some(COMMENTED), &[remove("db")]).expect_err("not listed");
    assert!(is_conflict(&missing), "{missing}");
    let outside = apply_edits(None, &[add("../secrets.sql")]).expect_err("confined");
    assert!(is_invalid_at_a_line(&outside), "{outside}");
    let edit: SettingsEdit =
        serde_json::from_str(r#"{"op": "add_evidence_pattern", "pattern": "db"}"#).expect("deserialize");
    assert_eq!(edit, add("db"));
}

#[test]
fn invalid_files_are_not_edited() {
    let error = apply_edits(
        Some("[scope]\nbogus = 2\n"),
        &[SettingsEdit::GraphOnly { path: "src".into() }],
    )
    .expect_err("invalid");
    assert!(
        matches!(
            error,
            EditError::Unparsable(SettingsError::Invalid { line: 2, .. })
        ),
        "{error}"
    );
    assert!(
        error.to_string().starts_with("fix unfold.toml first: "),
        "{error}"
    );
}

#[test]
fn set_app_adds_an_app_and_changes_only_its_fields() {
    let added = edit(
        Some(COMMENTED),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "Background jobs".into(),
                run: Some("celery -A shop.tasks worker".into()),
                code: Some("shop/tasks.py#".into()),
                about: None,
            },
        }],
    );
    assert_eq!(
        added.text,
        format!(
            "{COMMENTED}\n[[app]]\nname = \"Background jobs\"\nrun = \"celery -A shop.tasks worker\"\ncode = \"shop/tasks.py#\"\n"
        )
    );
    assert_eq!(added.file.apps.len(), 2);

    let changed = edit(
        Some(COMMENTED),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "shop api".into(),
                run: Some("npm run serve".into()),
                code: Some("src/server.ts#".into()),
                about: Some("The storefront's API.".into()),
            },
        }],
    );
    assert_eq!(
        changed.text,
        COMMENTED
            .replace("name = \"Shop API\"  #", "name = \"shop api\"  #")
            .replace(
                "run  = \"npm start\"\n",
                "run  = \"npm run serve\"\ncode = \"src/server.ts#\"\nabout = \"The storefront's API.\"\n"
            ),
        "the name addresses the app by its key; new fields go before `entrances`"
    );
    let app = &changed.file.apps[0];
    assert_eq!(app.key.as_ref().map(Key::as_str), Some("entrance:shop-api"));
    assert_eq!(app.entrances.len(), 1, "entrances stay");

    let cleared = edit(
        Some(&changed.text),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "shop api".into(),
                ..AppSpec::default()
            },
        }],
    );
    let app = &cleared.file.apps[0];
    assert_eq!(
        (app.run.as_ref(), app.code.as_ref(), app.about.as_ref()),
        (None, None, None)
    );

    let nameless = apply_edits(
        Some(COMMENTED),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "!!".into(),
                ..AppSpec::default()
            },
        }],
    )
    .expect_err("no key");
    assert!(is_conflict(&nameless), "{nameless}");
}

#[test]
fn empty_code_keys_are_refused() {
    let blank_app = apply_edits(
        Some(COMMENTED),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "Shop API".into(),
                code: Some(String::new()),
                ..AppSpec::default()
            },
        }],
    )
    .expect_err("an empty key");
    assert!(is_invalid_at_a_line(&blank_app), "{blank_app}");
    assert!(
        matches!(
            blank_app,
            EditError::Invalid(SettingsError::Invalid { line: 11, .. })
        ),
        "{blank_app}"
    );
    assert!(blank_app.to_string().contains("empty key"), "{blank_app}");

    for code in [&[""][..], &["src/a.ts#placeOrder", ""]] {
        let error = apply_edits(
            Some(COMMENTED),
            &[set_entrance("Shop API", "Place an order", None, code)],
        )
        .expect_err("an empty key");
        assert!(is_invalid_at_a_line(&error), "{code:?}: {error}");
    }
    let new_app =
        apply_edits(None, &[set_entrance("Jobs", "Nightly", None, &[""])]).expect_err("an empty key");
    assert!(is_invalid_at_a_line(&new_app), "{new_app}");

    let fixed = edit(
        Some(COMMENTED),
        &[
            set_entrance("Shop API", "Place an order", None, &[""]),
            set_entrance("Shop API", "Place an order", None, &["src/a.ts#placeOrder"]),
        ],
    );
    assert!(
        fixed.file.apps[0].entrances[0].is_valid(),
        "a later edit fixes it"
    );
}

#[test]
fn set_entrance_rewrites_one_line_or_adds_one() {
    let fixed = edit(
        Some(COMMENTED),
        &[set_entrance(
            "Shop API",
            "Place an order",
            Some("POST /orders"),
            &["src/orders.ts#placeOrder"],
        )],
    );
    assert_eq!(
        fixed.text,
        COMMENTED.replace("src/a.ts#placeOrder", "src/orders.ts#placeOrder")
    );
    assert_eq!(
        fixed.diff,
        "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -11,5 +11,5 @@\n run  = \"npm start\"\n entrances = [\n   # orders\n-  { name = \"Place an order\", on = \"POST /orders\", code = \"src/a.ts#placeOrder\" },\n+  { name = \"Place an order\", on = \"POST /orders\", code = \"src/orders.ts#placeOrder\" },\n ]\n"
    );

    let added = edit(
        Some(COMMENTED),
        &[set_entrance(
            "Shop API",
            "Cancel an order",
            None,
            &["src/auth.ts#requireUser", "src/a.ts#cancelOrder"],
        )],
    );
    assert_eq!(
        added.text,
        COMMENTED.replace(
            "placeOrder\" },\n]",
            "placeOrder\" },\n  { name = \"Cancel an order\", code = [\"src/auth.ts#requireUser\", \"src/a.ts#cancelOrder\"] },\n]"
        )
    );
    let cancel = &added.file.apps[0].entrances[1];
    assert_eq!(
        cancel.key.as_ref().map(Key::as_str),
        Some("entrance:shop-api/cancel-an-order")
    );
    assert_eq!(cancel.code.len(), 2);
}

#[test]
fn set_entrance_creates_the_app_when_it_is_new() {
    let created = edit(
        None,
        &[set_entrance(
            "Background jobs",
            "Send a receipt",
            Some("queue emails"),
            &["shop/tasks.py#send_receipt"],
        )],
    );
    assert_eq!(
        created.text,
        "[[app]]\nname = \"Background jobs\"\nentrances = [\n  { name = \"Send a receipt\", on = \"queue emails\", code = \"shop/tasks.py#send_receipt\" },\n]\n"
    );
    let second = edit(
        Some(&created.text),
        &[set_entrance(
            "Background jobs",
            "Sync inboxes",
            Some("every 5 minutes"),
            &["inbox/sync.py#sync_inbox"],
        )],
    );
    assert!(
        second.text.ends_with(
            "send_receipt\" },\n  { name = \"Sync inboxes\", on = \"every 5 minutes\", code = \"inbox/sync.py#sync_inbox\" },\n]\n"
        ),
        "{}",
        second.text
    );
    let into_commented = edit(
        Some(COMMENTED),
        &[set_entrance("Jobs", "Nightly", None, &["jobs.py#nightly"])],
    );
    assert!(
        into_commented
            .text
            .ends_with("]\n\n[[app]]\nname = \"Jobs\"\nentrances = [\n  { name = \"Nightly\", code = \"jobs.py#nightly\" },\n]\n"),
        "{}",
        into_commented.text
    );
    for bad in [
        set_entrance("Jobs", "", None, &["jobs.py#nightly"]),
        set_entrance("Jobs", "Nightly", None, &[]),
        set_entrance("--", "Nightly", None, &["jobs.py#nightly"]),
    ] {
        let error = apply_edits(None, &[bad]).expect_err("refused");
        assert!(is_conflict(&error), "{error}");
    }
}

#[test]
fn keys_with_double_quotes_are_written_in_single_quotes() {
    let key = "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]";
    let edited = edit(
        None,
        &[set_entrance("Orders API", "Place an order", None, &[key])],
    );
    assert!(
        edited
            .text
            .contains("code = 'src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]' }"),
        "{}",
        edited.text
    );
    let escaped = "src/a.ts#/call:r.get(\"say \\\"hi\\\"\")/callback[0]";
    let edited = edit(
        Some(&edited.text),
        &[SettingsEdit::SetApp {
            app: AppSpec {
                name: "Orders API".into(),
                code: Some(escaped.into()),
                ..AppSpec::default()
            },
        }],
    );
    assert!(
        edited
            .text
            .contains("code = 'src/a.ts#/call:r.get(\"say \\\"hi\\\"\")/callback[0]'"),
        "{}",
        edited.text
    );
    let app = &edited.file.apps[0];
    assert_eq!(app.code.as_ref().map(Key::as_str), Some(escaped));
    assert_eq!(app.entrances[0].code[0].as_str(), key);
}

#[test]
fn remove_entrance_removes_one_line_or_a_whole_app() {
    let two = edit(
        Some(COMMENTED),
        &[set_entrance("Shop API", "Refund", None, &["src/a.ts#refund"])],
    );
    let one = edit(Some(&two.text), &[remove("Shop API", Some("refund"))]);
    assert_eq!(one.text, COMMENTED);
    let none = edit(Some(COMMENTED), &[remove("Shop API", Some("Place an order"))]);
    assert!(!none.text.contains("entrances"), "{}", none.text);
    assert!(none.file.apps[0].entrances.is_empty());
    let gone = edit(Some(COMMENTED), &[remove("shop api", None)]);
    assert!(gone.file.apps.is_empty());
    assert!(!gone.text.contains("[[app]]"), "{}", gone.text);
    assert!(gone.text.starts_with("# Shared settings.\n"), "{}", gone.text);
    for missing in [remove("Shop API", Some("Refund")), remove("Jobs", None)] {
        let error = apply_edits(Some(COMMENTED), &[missing]).expect_err("not there");
        assert!(is_conflict(&error), "{error}");
    }
}

#[test]
fn a_byte_order_mark_stays() {
    let text = format!("\u{FEFF}{COMMENTED}");
    let edited = edit(Some(&text), &[remove("Shop API", None)]);
    assert!(
        edited.text.starts_with("\u{FEFF}# Shared settings.\n"),
        "{:?}",
        edited.text
    );
}

#[test]
fn entrance_edits_deserialize_from_ipc() {
    let edit: SettingsEdit = serde_json::from_str(
        r#"{"op": "set_entrance", "app": "Shop API", "entrance": {"name": "Place an order", "on": "POST /orders", "code": ["src/a.ts#placeOrder"]}}"#,
    )
    .expect("deserialize");
    assert_eq!(
        edit,
        set_entrance(
            "Shop API",
            "Place an order",
            Some("POST /orders"),
            &["src/a.ts#placeOrder"]
        )
    );
    let edit: SettingsEdit =
        serde_json::from_str(r#"{"op": "remove_entrance", "app": "Shop API"}"#).expect("deserialize");
    assert_eq!(edit, remove("Shop API", None));
    let edit: SettingsEdit =
        serde_json::from_str(r#"{"op": "set_app", "app": {"name": "Jobs", "run": "make jobs"}}"#)
            .expect("deserialize");
    assert_eq!(
        edit,
        SettingsEdit::SetApp {
            app: AppSpec {
                name: "Jobs".into(),
                run: Some("make jobs".into()),
                ..AppSpec::default()
            }
        }
    );
}
