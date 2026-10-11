//! Configured launches end to end (`testdata/fixtures/configured-entrances`, shaped like a
//! data-pipelines monorepo): each launch planned, resolved with its own import path, grouped
//! into applications by category, with routes two applications mount counted once; overrides
//! of detected launches; `check_launch` previews and `launch_suggestions`.

use serde_json::{Value, json};

use crate::common::Harness;

/// The overview's applications, by name.
fn applications(overview: &Value) -> Vec<Value> {
    overview["categories"]
        .as_array()
        .expect("categories")
        .iter()
        .flat_map(|category| {
            category["applications"]
                .as_array()
                .expect("applications")
                .iter()
                .map(|application| {
                    let mut application = application.clone();
                    application["category"] = category["category"].clone();
                    application
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn application<'a>(all: &'a [Value], name: &str) -> &'a Value {
    all.iter()
        .find(|application| application["name"] == name)
        .unwrap_or_else(|| panic!("no application {name} in {all:?}"))
}

#[test]
fn configured_launches_group_into_applications_by_category() {
    let harness = Harness::compiled_fixture("configured-entrances");
    let overview = harness.ok(1, "entrances", json!({ "project": harness.project }));
    assert_eq!(
        overview["counts"],
        json!({ "applications": 8, "launches": 8, "routes": 7, "entrances": 16 })
    );
    assert_eq!(overview["unreached"], 1, "the admin tools app no launch serves");
    let order: Vec<&str> = overview["categories"]
        .as_array()
        .expect("categories")
        .iter()
        .filter_map(|category| category["category"].as_str())
        .collect();
    assert_eq!(order, ["needs_look", "services", "workers", "scheduled"]);
    let all = applications(&overview);

    // uvicorn module:attr from a working folder: the routes bound to that app, mounts included.
    let api = application(&all, "Invoice HITL API");
    assert_eq!(api["category"], "services");
    assert_eq!(api["origin"], "declared");
    assert_eq!(api["routes"], 4);
    assert_eq!(api["entrances"], 5);
    let start = &api["launches"][0];
    assert_eq!(start["label"], "Start API");
    assert_eq!(start["folder"], "data-pipelines");
    assert_eq!(
        start["command"],
        "uvicorn api.main:app --host 0.0.0.0 --port 8000"
    );
    assert_eq!(start["status"], "ok");
    assert_eq!(
        start["starts"],
        json!({ "path": "data-pipelines/api/main.py", "name": "app", "kind": "FastAPI application" })
    );
    assert_eq!(
        start["importPaths"],
        json!([{ "path": "data-pipelines", "origin": "the working folder" }])
    );
    assert_eq!(start["routes"], 4);
    assert_eq!(start["declaredIn"], json!({ "path": "unfold.toml", "line": 12 }));
    assert_eq!(start["editable"], "configured");
    assert_eq!(start["id"], "invoice-hitl-api");
    assert!(start["reaches"].as_u64().is_some_and(|reaches| reaches > 0));

    // `python -m uvicorn … --app-dir`: the app folder first, then the working folder.
    let recipe = all
        .iter()
        .find(|application| application["launches"][0]["id"] == "recipe-cost-api")
        .expect("the recipe cost launch");
    assert_eq!(recipe["launches"][0]["folder"], "configured-entrances");
    assert_eq!(
        recipe["launches"][0]["importPaths"],
        json!([
            { "path": "food-cost", "origin": "from --app-dir" },
            { "path": ".", "origin": "the working folder, for python -m" }
        ])
    );
    assert_eq!(recipe["routes"], 1);

    // sh -c can't be read: Needs a look, unless its target is set by hand.
    let unreadable = application(&all, "dashboard-builder");
    assert_eq!(unreadable["category"], "needs_look");
    let launch = &unreadable["launches"][0];
    assert_eq!(launch["status"], "unreadable");
    assert_eq!(launch["problem"], "Can't read sh -c");
    assert_eq!(launch["starts"], Value::Null);
    let fixed = all
        .iter()
        .find(|application| application["launches"][0]["id"] == "dashboard-builder-fixed")
        .expect("the launch with its target set by hand");
    assert_eq!(fixed["category"], "services");
    assert_eq!(fixed["launches"][0]["targetSetByHand"], true);
    assert_eq!(fixed["launches"][0]["status"], "ok");
    // Two applications mount one router: each has its routes, which count once in all.
    assert_eq!(fixed["routes"], 3);
    assert_eq!(fixed["alsoIn"], json!(["Invoice HITL API"]));
    assert_eq!(api["alsoIn"], json!([fixed["name"]]));

    // A script, scheduled; a module; code that isn't indexed.
    let token = application(&all, "Token refresh");
    assert_eq!(token["category"], "scheduled");
    let launch = &token["launches"][0];
    assert_eq!(launch["schedule"], "30 7 * * *");
    assert_eq!(launch["scheduleGloss"], "daily at 07:30");
    assert_eq!(launch["starts"]["kind"], "Python script");
    assert_eq!(
        launch["importPaths"],
        json!([{ "path": "data-pipelines/jobs", "origin": "script folder" }])
    );
    let worker = application(&all, "main");
    assert_eq!(worker["category"], "workers");
    assert_eq!(worker["launches"][0]["starts"]["kind"], "Python module");
    let eval = application(&all, "purchase-ingestion-eval");
    assert_eq!(eval["launches"][0]["status"], "not_indexed");
    assert_eq!(
        eval["launches"][0]["note"],
        "evals/ is in .gitignore, so this code isn't indexed"
    );
}

#[test]
fn overrides_present_detected_launches_and_orphans_are_problems() {
    let harness = Harness::compiled_fixture("configured-entrances");
    let overview = harness.ok(1, "entrances", json!({ "project": harness.project }));
    let all = applications(&overview);
    // The console script joins the declared application with the override's presentation.
    let billing = application(&all, "Billing");
    assert_eq!(billing["category"], "scheduled");
    assert_eq!(billing["entrances"], 2, "the launch and the declared command");
    let review = &billing["launches"][0];
    assert_eq!(review["label"], "Review poller");
    assert_eq!(review["command"], "review");
    assert_eq!(review["scheduleGloss"], "4 times a day");
    assert_eq!(review["editable"], "override");
    assert_eq!(
        review["match"],
        json!({ "detector": "pyproject.script", "path": "pyproject.toml", "pointer": "/project/scripts/review" })
    );
    assert_eq!(
        review["declaredIn"],
        json!({ "path": "pyproject.toml", "line": 6 })
    );
    assert_eq!(
        review["starts"],
        json!({ "path": "worker/poll.py", "name": "run", "kind": "Python function" })
    );
    let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
    let messages: Vec<&str> = problems
        .as_array()
        .expect("problems")
        .iter()
        .filter_map(|problem| problem["message"].as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("matches no detected launch")
                && message.contains("/project/scripts/missing")),
        "{problems}"
    );
}

#[test]
fn an_application_scope_sections_its_rows_and_pages_them() {
    let harness = Harness::compiled_fixture("configured-entrances");
    let scope = |args: Value| {
        let mut args = args;
        args["project"] = json!(harness.project);
        args["application"] = json!("invoice-hitl-api");
        harness.ok(1, "entrances", args)
    };
    let all = scope(json!({}));
    let sections: Vec<(&str, &Value, &Value)> = all["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .map(|section| {
            (
                section["title"].as_str().expect("title"),
                &section["mountedFrom"],
                &section["alsoIn"],
            )
        })
        .collect();
    let dashboard = json!("data-pipelines/dashboard_builder/router.py");
    let fixed = json!(["app#app"]);
    assert_eq!(
        sections,
        [
            ("Launch", &Value::Null, &json!([])),
            ("/dashboard-builder", &dashboard, &fixed),
            ("/runs", &Value::Null, &json!([])),
        ]
    );
    let rows: Vec<(&str, &str)> = all["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            (
                row["title"].as_str().expect("title"),
                row["handler"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Start API", ""),
            ("POST /dashboard-builder/explore", "explore"),
            ("GET /dashboard-builder/model/{model}", "model"),
            ("POST /runs", "create_run"),
            ("GET /runs/{run_id}", "get_run"),
        ]
    );
    assert_eq!(all["rows"][1]["alsoIn"], fixed);
    assert_eq!(all["rows"][1]["line"], 11);
    assert_eq!(all["rows"][0]["id"], "invoice-hitl-api");

    let first = scope(json!({ "limit": 2 }));
    assert_eq!(first["total"], 5);
    assert_eq!(first["rows"].as_array().map(Vec::len), Some(2));
    let second = scope(json!({ "limit": 2, "cursor": first["nextCursor"] }));
    assert_eq!(second["rows"][0], all["rows"][2]);
    let runs = scope(json!({ "filter": "RUN", "limit": 1 }));
    assert_eq!(runs["total"], 2, "the filter applies before paging");
    assert_eq!(runs["rows"][0]["title"], "POST /runs");
    assert_eq!(runs["sections"].as_array().map(Vec::len), Some(1));
    let unreached = harness.ok(
        1,
        "entrances",
        json!({ "project": harness.project, "unreached": true }),
    );
    assert_eq!(unreached["rows"][0]["title"], "GET /admin/stats");
}

#[test]
fn check_launch_previews_a_draft_against_the_current_graph() {
    let harness = Harness::compiled_fixture("configured-entrances");
    let check = |draft: Value| {
        harness.ok(
            1,
            "check_launch",
            json!({ "project": harness.project, "draft": draft }),
        )
    };

    let quoted = check(json!({
        "cwd": "data-pipelines",
        "command": "sh -c \"uvicorn dashboard_builder.app:app --port $PORT\"",
        "env": {}
    }));
    assert_eq!(quoted["problem"]["label"], "Can't read sh -c");
    assert_eq!(quoted["starts"], Value::Null);
    assert_eq!(
        quoted["suggestion"],
        json!({
            "quoted": "uvicorn dashboard_builder.app:app",
            "target": { "type": "python_asgi", "module": "dashboard_builder.app", "attribute": "app", "factory": false },
            "pythonPaths": ["data-pipelines"]
        })
    );

    let draft = check(json!({
        "cwd": "data-pipelines",
        "command": "uvicorn dashboard_builder.app:app --port $PORT",
        "env": {}
    }));
    assert_eq!(draft["fromCurrentGraph"], false);
    assert_eq!(draft["problem"], Value::Null);
    let meanings: Vec<&str> = draft["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter_map(|part| part["meaning"].as_str())
        .collect();
    assert!(
        meanings.contains(&"Server options. They don't change the code"),
        "{meanings:?}"
    );
    assert_eq!(
        draft["starts"],
        json!({
            "path": "data-pipelines/dashboard_builder/app.py",
            "name": "app",
            "description": "A FastAPI application with 3 routes",
            "status": "ok",
            "reason": null
        })
    );
    assert_eq!(draft["routes"]["count"], 3);
    assert_eq!(
        draft["routes"]["shared"],
        json!([{ "path": "data-pipelines/dashboard_builder/router.py", "routes": 2, "alsoIn": ["Invoice HITL API"] }])
    );

    // An import path that finds code the graph resolves nowhere else.
    let module = check(json!({ "cwd": "data-pipelines", "command": "python -m jobs.backfill", "env": {} }));
    assert_eq!(module["starts"]["description"], "A Python module");
    assert_eq!(module["imports"]["count"], 2);
    assert_eq!(module["imports"]["examples"], json!(["shared", "shared.tokens"]));

    let missing = check(json!({ "cwd": ".", "command": "python -m evals.nothing", "env": {} }));
    assert_eq!(missing["starts"]["status"], "not_found");
    let ignored = check(json!({ "cwd": ".", "command": "python -m evals.run", "env": {} }));
    assert_eq!(ignored["starts"]["status"], "not_indexed");
    assert_eq!(
        ignored["starts"]["reason"],
        "evals/ is in .gitignore, so this code isn't indexed"
    );
    assert_eq!(
        harness.code(
            1,
            "check_launch",
            json!({ "project": harness.project, "draft": { "cwd": "../up", "env": {} } })
        ),
        "invalid_args"
    );
}

#[test]
fn launch_suggestions_name_the_apps_no_launch_serves() {
    let harness = Harness::compiled_fixture("configured-entrances");
    let suggestions = harness.ok(1, "launch_suggestions", json!({ "project": harness.project }));
    assert_eq!(suggestions["routes"], 7);
    let imports = &suggestions["imports"];
    assert!(
        imports["resolved"].as_u64() < imports["total"].as_u64(),
        "{imports}"
    );
    assert_eq!(suggestions["localLooking"], json!(["shared"]));
    assert_eq!(
        suggestions["apps"],
        json!([{
            "path": "data-pipelines/admin_tools/server.py",
            "attribute": "app",
            "folder": "data-pipelines",
            "command": "uvicorn admin_tools.server:app",
            "routes": 1
        }])
    );
}
