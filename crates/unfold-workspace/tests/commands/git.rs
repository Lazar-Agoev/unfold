//! Branches and worktrees from fake `.git` folders and files (worktree metadata): the row's branch,
//! one row per repository, `git_info`, and switching a row to another worktree.

use std::path::Path;

use serde_json::{Value, json};
use unfold_workspace::ProjectId;

use crate::common::{Harness, copy_dir, is_event, options, playground};

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

/// A repository at `<base>/repo` on `main` with a linked worktree at `<base>/wt` on `feature`,
/// each holding a copy of `ts-orders` in `app/`.
fn repository(base: &Path) {
    write(&base.join("repo/.git/HEAD"), "ref: refs/heads/main\n");
    let meta = base.join("repo/.git/worktrees/wt");
    write(&meta.join("HEAD"), "ref: refs/heads/feature\n");
    write(&meta.join("commondir"), "../..\n");
    write(
        &meta.join("gitdir"),
        &format!("{}\n", base.join("wt/.git").display()),
    );
    write(&base.join("wt/.git"), &format!("gitdir: {}\n", meta.display()));
    copy_dir(&playground("ts-orders"), &base.join("repo/app"));
    copy_dir(&playground("ts-orders"), &base.join("wt/app"));
}

/// A host whose first project is `<base>/repo/app`.
fn harness() -> Harness {
    let mut harness = Harness::empty(options(false));
    repository(&harness.base);
    let added = harness
        .trusted("add_project", json!({ "path": harness.base.join("repo/app") }))
        .expect("add");
    harness.project = added["project"]["id"].as_str().expect("id").to_owned();
    harness.root = harness.base.join("repo/app");
    harness
}

fn path(harness: &Harness, relative: &str) -> String {
    harness.base.join(relative).to_str().expect("utf-8").to_owned()
}

#[test]
fn a_row_shows_its_branch_and_a_repository_has_one_row() {
    let harness = harness();
    let row = harness.row(&harness.project);
    assert_eq!(
        row["git"],
        json!({ "branch": "main", "detached": null, "worktrees": 2 })
    );
    assert_eq!(
        row["name"], "app",
        "a subfolder project is named after its folder"
    );
    // Another worktree of the repository, at the same subpath, is the same row.
    let other = harness
        .trusted("add_project", json!({ "path": harness.base.join("wt/app") }))
        .expect("add");
    assert_eq!(other["status"], "exists");
    assert_eq!(other["project"]["id"], harness.project);
    // The repository root is another project; every worktree of it shares the main one's name.
    let root = harness
        .trusted("add_project", json!({ "path": harness.base.join("wt") }))
        .expect("add");
    assert_eq!(root["status"], "added");
    assert_eq!(root["project"]["name"], "repo");
    assert_eq!(root["project"]["git"]["branch"], "feature");
    let again = harness
        .trusted("add_project", json!({ "path": harness.base.join("repo") }))
        .expect("add");
    assert_eq!(again["status"], "exists");
    assert_eq!(again["project"]["id"], root["project"]["id"]);
}

#[test]
fn git_info_lists_worktrees_and_reads_head_every_time() {
    let harness = harness();
    let info = harness.ok(1, "git_info", json!({ "project": harness.project }));
    assert_eq!(info["branch"], "main");
    assert_eq!(
        info["worktrees"],
        json!([
            {
                "root": path(&harness, "repo"), "path": path(&harness, "repo/app"),
                "branch": "main", "detached": null, "main": true, "current": true, "available": true,
            },
            {
                "root": path(&harness, "wt"), "path": path(&harness, "wt/app"),
                "branch": "feature", "detached": null, "main": false, "current": false, "available": true,
            },
        ])
    );
    // A checkout in the main worktree shows at once: nothing is cached.
    write(
        &harness.base.join("repo/.git/HEAD"),
        "0123456789abcdef0123456789abcdef01234567\n",
    );
    let info = harness.ok(1, "git_info", json!({ "project": harness.project }));
    assert_eq!(info["branch"], Value::Null);
    assert_eq!(info["detached"], "0123456");
    assert_eq!(harness.row(&harness.project)["git"]["detached"], "0123456");
    // A worktree whose project folder is missing is listed, unavailable.
    std::fs::remove_dir_all(harness.base.join("wt/app")).expect("remove");
    let info = harness.ok(1, "git_info", json!({ "project": harness.project }));
    assert_eq!(info["worktrees"][1]["available"], false);
}

#[test]
fn switching_points_the_row_at_another_worktree() {
    let harness = harness();
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    assert_eq!(
        harness.code(
            1,
            "switch_worktree",
            json!({ "project": harness.project, "path": path(&harness, "elsewhere") })
        ),
        "not_a_worktree"
    );
    // A board held by a window keeps the row where it is.
    let id: ProjectId = serde_json::from_value(json!(harness.project)).expect("id");
    let projects = harness.host.projects().expect("initialized");
    let claims = projects.handle(&id).expect("project");
    claims
        .claims()
        .claim(".unfold/local/drafts/board-1.json", 2)
        .expect("claim");
    let target = json!({ "project": harness.project, "path": path(&harness, "wt/app") });
    assert_eq!(harness.code(1, "switch_worktree", target.clone()), "boards_open");
    harness.host.detach_window(2).expect("detach");

    let mark = harness.sink.mark();
    let row = harness.ok(1, "switch_worktree", target);
    assert_eq!(row["id"], harness.project);
    assert_eq!(row["path"], path(&harness, "wt/app"));
    assert_eq!(row["git"]["branch"], "feature");
    assert_eq!(row["live"], true, "a selected project reopens on the new folder");
    let messages = harness.sink.all();
    let after = &messages[mark..];
    let reset = after
        .iter()
        .position(|message| is_event(message, "project_reset", &harness.project))
        .expect("project_reset");
    let changed = after
        .iter()
        .position(|message| message["event"]["type"] == "workspace_changed")
        .expect("workspace_changed");
    assert!(reset < changed);
    assert!(
        after[changed..]
            .iter()
            .any(|message| is_event(message, "project_state", &harness.project))
    );
    // The new folder was never compiled: its graph is the new folder's.
    assert_eq!(row["state"], json!({"kind": "not_compiled"}));
    let info = harness.ok(1, "git_info", json!({ "project": harness.project }));
    assert_eq!(info["worktrees"][1]["current"], true);
    assert!(!harness.base.join("wt/app/.unfold/local").exists());
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["path"], path(&harness, "wt/app"));
}

#[test]
fn switching_a_resting_project_leaves_it_resting() {
    let harness = harness();
    let row = harness.ok(
        1,
        "switch_worktree",
        json!({ "project": harness.project, "path": path(&harness, "wt/app") }),
    );
    assert_eq!(row["live"], false);
    assert_eq!(row["state"]["kind"], "resting");
    // And back again.
    let row = harness.ok(
        1,
        "switch_worktree",
        json!({ "project": harness.project, "path": path(&harness, "repo/app") }),
    );
    assert_eq!(row["path"], path(&harness, "repo/app"));
}
