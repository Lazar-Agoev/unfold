//! `import_deploy_files` on a copy of `testdata/fixtures/deploy-files`: a root `render.yaml` and
//! a nested `data-pipelines/render.yaml` with their Dockerfiles, read into draft launches.

use serde_json::{Value, json};

use crate::common::{Harness, fixture, options};

/// A host over a copy of the fixture, not compiled: the import needs no graph.
fn harness() -> Harness {
    let mut harness = Harness::empty(options(false));
    let (project, root) = harness.add_copy(&fixture("deploy-files"), "deploy-files");
    harness.project = project;
    harness.root = root;
    harness
}

fn import(harness: &Harness) -> Value {
    harness.ok(1, "import_deploy_files", json!({ "project": harness.project }))
}

#[test]
fn drafts_a_launch_per_service_from_render_yaml_and_dockerfiles() {
    let harness = harness();
    let import = import(&harness);
    assert_eq!(
        import["files"],
        json!(["render.yaml", "data-pipelines/render.yaml"])
    );
    let rows: Vec<Value> = import["launches"]
        .as_array()
        .expect("launches")
        .iter()
        .map(|launch| {
            json!([
                launch["id"],
                launch["application"]["id"],
                launch["command"],
                launch["cwd"],
                launch["copiedFrom"],
            ])
        })
        .collect();
    let workers = "data-pipelines/Dockerfile.workers, CMD";
    let commands = "data-pipelines/render.yaml, dockerCommand";
    assert_eq!(
        rows,
        [
            json!([
                "dashboard-builder",
                "dashboard-builder",
                "sh -c \"uvicorn dashboard_builder.app:app --host 0.0.0.0 --port ${PORT:-8010}\"",
                "data-pipelines",
                "data-pipelines/Dockerfile.dashboard-builder, CMD"
            ]),
            json!([
                "dashboard-builder-export",
                "dashboard-builder",
                "python -m dashboard_builder.export --from-env",
                "data-pipelines",
                "render.yaml, dockerCommand"
            ]),
            json!([
                "review-poller",
                "review-poller",
                "python3 review-workers/run_poller.py --all-businesses",
                "data-pipelines",
                commands
            ]),
            json!([
                "review-worker",
                "review-worker",
                "python3 review-workers/run_worker.py",
                "data-pipelines",
                workers
            ]),
            json!([
                "demo-sync-2-daily",
                "demo-sync-2-daily",
                "python data-pipelines/demo-sync-2/sync.py",
                ".",
                "data-pipelines/render.yaml, startCommand"
            ]),
            json!([
                "token-refresh-daily",
                "token-refresh-daily",
                "python3 -m token_refresh.run",
                "data-pipelines",
                commands
            ]),
            json!([
                "invoice-hitl-api",
                "invoice-hitl-api",
                "uvicorn api.main:app --host 0.0.0.0 --port 8000",
                "data-pipelines",
                "data-pipelines/Dockerfile, CMD"
            ]),
            json!([
                "recipe-cost-api",
                "recipe-cost-api",
                "python3 -m uvicorn api:app --app-dir food-cost --host 0.0.0.0 --port $PORT",
                "data-pipelines",
                commands
            ]),
        ]
    );

    let launches = &import["launches"];
    assert_eq!(
        launches[0],
        json!({
            "service": "dashboard-builder",
            "serviceType": "web",
            "application": {
                "id": "dashboard-builder",
                "name": "Dashboard builder",
                "description": "Query service: /analyst/* called only by the web app's server with the shared token.",
            },
            "id": "dashboard-builder",
            "label": "Web",
            "command": "sh -c \"uvicorn dashboard_builder.app:app --host 0.0.0.0 --port ${PORT:-8010}\"",
            "cwd": "data-pipelines",
            "schedule": null,
            "description": null,
            "env": { "PYTHONPATH": "." },
            "copiedFrom": "data-pipelines/Dockerfile.dashboard-builder, CMD",
            "note": null,
            "renderFile": "render.yaml",
        })
    );
    let export = &launches[1];
    assert_eq!(export["label"], "Cron");
    assert_eq!(export["schedule"], "0 10 * * *");
    assert_eq!(export["application"], launches[0]["application"]);
    assert_eq!(
        export["description"],
        "Nightly export: one parquet per analyst table per business, published to the bucket. Runs at 10:00 UTC. Same image, different command."
    );
    // The owning service's comment describes its application, not the launch.
    let token = &launches[5];
    assert_eq!(token["description"], Value::Null);
    assert_eq!(
        token["application"]["description"],
        "Rotates OAuth tokens before they expire. One schedule serves every connected merchant."
    );
    assert_eq!(token["schedule"], "30 7 * * *");
    assert_eq!(launches[6]["application"]["name"], "Invoice hitl API");
    assert_eq!(launches[6]["env"], json!({ "PYTHONPATH": "." }));
    assert_eq!(launches[3]["env"], json!({}));
}

#[test]
fn render_yaml_files_follow_discovery_ignore_rules() {
    let harness = harness();
    harness.write(".ignore", "legacy/\n");
    harness.write(
        "legacy/render.yaml",
        "services:\n  - type: worker\n    name: old\n    env: python\n    startCommand: python old.py\n",
    );
    harness.write(
        "jobs/render.yaml",
        "services:\n  - type: worker\n    name: nightly\n    runtime: docker\n",
    );
    let import = import(&harness);
    assert_eq!(
        import["files"],
        json!(["render.yaml", "data-pipelines/render.yaml", "jobs/render.yaml"])
    );
    let nightly = import["launches"]
        .as_array()
        .expect("launches")
        .iter()
        .find(|launch| launch["id"] == "nightly")
        .expect("nightly");
    assert_eq!(nightly["command"], Value::Null);
    assert_eq!(
        nightly["note"],
        "No Dockerfile found at Dockerfile or jobs/Dockerfile"
    );
}
