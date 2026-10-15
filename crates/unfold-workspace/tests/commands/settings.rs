//! Settings: the app settings (`settings.json`: the editor command only, and opening a
//! project's `unfold.toml` with it), `app_info`, and a project's `unfold.toml` (`project_config`,
//! and `update_project_config`'s edits with their diff; an edit of only `[[app]]` tables resolves
//! the entrances without compiling).

use serde_json::{Value, json};

use crate::common::Harness;

impl Harness {
    /// Saves the app settings as the main process does for the Settings dialog.
    fn set_settings(&self, settings: Value) -> Result<Value, unfold_workspace::CommandError> {
        self.trusted("set_settings", json!({ "settings": settings }))
    }
}

#[test]
fn settings_round_trip_through_settings_json() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(harness.ok(1, "get_settings", json!({})), json!({}));
    let settings = json!({ "editorCommand": "code -g {path}:{line}" });
    assert_eq!(harness.set_settings(settings.clone()).expect("saved"), settings);
    assert_eq!(harness.ok(1, "get_settings", json!({})), settings);
    let text = std::fs::read_to_string(harness.base.join("app/settings.json")).expect("settings.json");
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), settings);
    // Clearing the command leaves nothing in the file.
    harness.set_settings(json!({})).expect("cleared");
    assert_eq!(harness.ok(1, "get_settings", json!({})), json!({}));
    // A window can't set the program `open_in_editor` runs.
    assert_eq!(
        harness.code(1, "set_settings", json!({ "settings": settings })),
        "untrusted"
    );
    assert_eq!(harness.ok(1, "get_settings", json!({})), json!({}));
}

#[test]
fn rejected_settings_are_not_saved() {
    let harness = Harness::new("ts-orders", false);
    for settings in [
        json!({ "editorCommand": "code\u{0}" }),
        json!({ "theme": "dark" }),
        json!({ "editorCommand": "code", "threadLimit": 2 }),
    ] {
        assert_eq!(
            harness.set_settings(settings.clone()).expect_err("refused").code,
            "invalid_args",
            "{settings}"
        );
    }
    assert!(!harness.base.join("app/settings.json").exists());
}

#[test]
fn opening_unfold_toml_creates_an_empty_one_first() {
    let harness = Harness::new("ts-orders", false);
    // An editor command that does nothing, so the test opens no app.
    harness
        .set_settings(json!({ "editorCommand": "/usr/bin/true" }))
        .expect("saved");
    let config = harness.root.join("unfold.toml");
    std::fs::remove_file(&config).expect("remove unfold.toml");
    let open = json!({ "project": harness.project, "path": "unfold.toml" });
    assert_eq!(harness.ok(1, "open_in_editor", open.clone()), Value::Null);
    assert_eq!(std::fs::read_to_string(&config).expect("created"), "");
    // An existing file is opened as it is.
    std::fs::write(&config, "languages = [\"typescript\"]\n").expect("write");
    harness.ok(1, "open_in_editor", open);
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        "languages = [\"typescript\"]\n"
    );
    // Any other missing file is not created.
    assert_eq!(
        harness.code(
            1,
            "open_in_editor",
            json!({ "project": harness.project, "path": "src/missing.ts" })
        ),
        "not_found"
    );
    assert!(!harness.root.join("src/missing.ts").exists());
}

#[test]
fn app_info_names_the_signer_and_the_engine() {
    let harness = Harness::new("ts-orders", false);
    let info = harness.ok(1, "app_info", json!({}));
    assert!(info["username"].as_str().is_some_and(|name| !name.is_empty()));
    assert!(
        info["engineVersion"]
            .as_str()
            .is_some_and(|version| !version.is_empty())
    );
}

#[test]
fn project_config_reads_settings_and_apps_without_waking() {
    let harness = Harness::with("ts-orders", crate::common::restless());
    crate::common::eventually("the project rests", || {
        harness.row(&harness.project)["live"] == false
    });
    let config = harness.ok(1, "project_config", json!({ "project": harness.project }));
    assert_eq!(config["exists"], true);
    assert!(
        config["text"]
            .as_str()
            .is_some_and(|text| text.contains("[[app]]"))
    );
    let settings = &config["settings"];
    assert_eq!(settings["include"], json!([]));
    assert_eq!(settings["evidence"], json!([]));
    let apps = settings["apps"].as_array().expect("apps");
    assert_eq!(apps.len(), 7);
    assert_eq!(apps[0]["name"], "Server");
    assert_eq!(
        apps[0]["code"],
        json!({ "key": "src/server.ts#" }),
        "as written, without states"
    );
    assert!(apps[0]["lines"].is_array());
    assert_eq!(
        harness.row(&harness.project)["live"],
        false,
        "reading didn't wake it"
    );

    // A file that doesn't parse is reported with its position.
    harness.write("unfold.toml", "[scope]\nbogus = 1\n");
    let broken = harness.ok(1, "project_config", json!({ "project": harness.project }));
    assert_eq!(broken["settings"], Value::Null);
    assert_eq!(broken["error"]["line"], 2);
}

#[test]
fn config_edits_return_their_diff_and_app_edits_resolve_without_compiling() {
    let harness = Harness::watched("ts-orders");
    let project = harness.project.clone();
    let edit = |edits: Value| {
        harness.ok(
            1,
            "update_project_config",
            json!({ "project": project, "edits": edits }),
        )
    };
    let updated_at =
        || harness.ok(1, "project_summary", json!({ "project": project }))["updatedAtMs"].clone();
    let before = updated_at();

    // An app-only edit: entrances are resolved again before it returns, and nothing compiles.
    let mark = harness.sink.mark();
    let updated = edit(json!([
        { "op": "set_app", "app": { "name": "Mailer", "run": "npm run mailer", "code": "src/queues/email-worker.ts#" } },
        { "op": "set_entrance", "app": "Mailer", "entrance": {
            "name": "Send an email", "on": "queue emails",
            "code": ["src/queues/email-worker.ts#/call:Worker(\"emails\")/callback[0]"]
        } },
        { "op": "set_entrance", "app": "Server", "entrance": {
            "name": "Admin stats", "on": "GET /admin/stats", "code": ["src/routes/admin.ts#adminStats"]
        } },
    ]));
    let diff = updated["diff"].as_str().expect("diff");
    assert!(diff.contains("+name = \"Mailer\""), "{diff}");
    assert!(diff.contains("-  { name = \"Admin stats\""), "{diff}");
    let apps = updated["config"]["settings"]["apps"].as_array().expect("apps");
    assert_eq!(apps.last().expect("the new app")["name"], "Mailer");
    let changed = harness.sink.events(mark, "entrances_changed");
    assert_eq!(
        changed,
        [json!({ "type": "entrances_changed", "project": project })]
    );
    assert_eq!(harness.sink.events(mark, "config_changed").len(), 1);
    let apps = harness.apps();
    let mailer = apps.last().expect("Mailer");
    assert_eq!(mailer["code"]["state"], "found");
    assert_eq!(mailer["entrances"][0]["code"][0]["state"], "found");
    let admin = &apps[0]["entrances"][7];
    assert_eq!(admin["code"][0]["state"], "found");
    assert!(admin["code"][0].get("fix").is_none());

    // Removing an entrance and an app.
    let removed = edit(json!([
        { "op": "remove_entrance", "app": "Mailer", "entrance": "Send an email" },
        { "op": "remove_entrance", "app": "Mailer" },
    ]));
    assert!(
        removed["diff"]
            .as_str()
            .expect("diff")
            .contains("-name = \"Mailer\"")
    );
    assert_eq!(harness.apps().len(), 7);
    // The watcher sees the writes and their temporary files; none is an outside change.
    harness.drain_watcher();
    for kind in ["graph_updated", "files_changed", "compile_progress"] {
        let events = harness.sink.events(mark, kind);
        assert!(events.is_empty(), "app edits never compile: {events:?}");
    }
    let states = harness.sink.events(mark, "project_state");
    assert!(states.is_empty(), "no update ran: {states:?}");
    assert_eq!(updated_at(), before, "the graph's commit time is kept");

    // The store holds what the graph and its apps came from: an unrelated change doesn't commit
    // the graph again.
    let mark = harness.sink.mark();
    harness.write("README.md", "# Changed\n");
    crate::common::eventually("README.md is checked", || {
        let states = harness.sink.events(mark, "project_state");
        states.len() >= 2
            && states
                .last()
                .is_some_and(|event| event["state"]["kind"] != "updating")
    });
    assert_eq!(updated_at(), before, "nothing to commit");

    // A scope edit goes through a background update.
    let mark = harness.sink.mark();
    let scoped = edit(json!([{ "op": "exclude_from_graph", "path": "src/lib" }]));
    assert_eq!(scoped["config"]["settings"]["exclude"], json!(["src/lib"]));
    assert_eq!(scoped["entries"], json!([]), "no app or entrance edit");
    crate::common::eventually("the update lands", || {
        !harness.sink.events(mark, "graph_updated").is_empty()
    });

    // An edit that changes nothing writes nothing and has no diff.
    let same = edit(json!([{ "op": "exclude_from_graph", "path": "src/lib" }]));
    assert_eq!(same["diff"], "");

    // Refused edits write nothing.
    let before = std::fs::read_to_string(harness.root.join("unfold.toml")).expect("read");
    for (edits, code) in [
        (
            json!([{ "op": "remove_entrance", "app": "Nowhere" }]),
            "settings_conflict",
        ),
        (
            json!([{ "op": "set_entrance", "app": "Server", "entrance": { "name": "X", "code": [] } }]),
            "settings_conflict",
        ),
        (json!([{ "op": "set_detector", "entrance": {} }]), "invalid_args"),
    ] {
        assert_eq!(
            harness.code(
                1,
                "update_project_config",
                json!({ "project": project, "edits": edits })
            ),
            code,
            "{edits}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(harness.root.join("unfold.toml")).expect("read"),
        before
    );
}

#[test]
fn entrance_edits_keep_what_they_leave_out_address_by_name_or_key_and_say_what_they_did() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let config = || std::fs::read_to_string(harness.root.join("unfold.toml")).expect("read");
    let edit = |edits: Value| {
        harness.ok(
            1,
            "update_project_config",
            json!({ "project": project, "edits": edits }),
        )
    };
    let refused = |edits: Value| {
        harness
            .call(
                1,
                "update_project_config",
                json!({ "project": project, "edits": edits }),
            )
            .expect_err("refused")
    };
    let before = config();

    // Names that make an entry's key, and its key, address it: only `about` is written, and the
    // app and entrance keep their names and every other field.
    let changed = edit(json!([
        { "op": "set_app", "app": { "name": "Server!", "about": "The HTTP API." } },
        { "op": "set_entrance", "app": "server", "entrance": {
            "name": "entrance:server/status", "about": "Says the API is up."
        } },
    ]));
    assert_eq!(
        changed["entries"],
        json!([
            { "change": "changed", "app": "Server", "newApp": false },
            { "change": "changed", "app": "Server", "entrance": "Status", "newApp": false },
        ])
    );
    let expected = before
        .replacen(
            "code = \"src/server.ts#\"\nentrances",
            "code = \"src/server.ts#\"\nabout = \"The HTTP API.\"\nentrances",
            1,
        )
        .replace(
            "{ name = \"Status\", on = \"GET /v1/status\", code = \"src/server.ts#status\" }",
            "{ name = \"Status\", on = \"GET /v1/status\", code = \"src/server.ts#status\", about = \"Says the API is up.\" }",
        );
    assert_eq!(config(), expected);

    // Null removes a field; the rest stays.
    edit(json!([{ "op": "set_entrance", "app": "Server", "entrance": { "name": "status", "on": null } }]));
    assert!(
        config().contains(
            "{ name = \"Status\", code = \"src/server.ts#status\", about = \"Says the API is up.\" }"
        ),
        "{}",
        config()
    );

    // Add adds one line, creating the app, and refuses a key the app has.
    let added = edit(json!([{ "op": "add_entrance", "app": "Mailer", "entrance": {
        "name": "Send an email", "code": ["src/queues/email-worker.ts#"]
    } }]));
    assert_eq!(
        added["entries"],
        json!([{ "change": "added", "app": "Mailer", "entrance": "Send an email", "newApp": true }])
    );
    let after = config();
    let taken = refused(json!([{ "op": "add_entrance", "app": "Server", "entrance": {
        "name": "status!", "code": ["src/server.ts#status"]
    } }]));
    assert_eq!(taken.code, "settings_conflict");
    assert_eq!(
        taken.message,
        "Server already has the entrance “Status”; give this one another name"
    );

    // A new entrance needs code; an app or entrance the file doesn't have is named in the refusal.
    let codeless =
        refused(json!([{ "op": "set_entrance", "app": "Server", "entrance": { "name": "Refund" } }]));
    assert_eq!(
        codeless.message,
        "“Refund” is a new entrance of Server, and a new entrance needs code: the block it starts at"
    );
    let unknown = refused(json!([{ "op": "remove_entrance", "app": "Dev server", "entrance": "Reload" }]));
    assert_eq!(
        unknown.message,
        "the app Dev server has no entrance “Reload”. Its entrances: none"
    );
    assert_eq!(config(), after, "refused edits write nothing");
}
