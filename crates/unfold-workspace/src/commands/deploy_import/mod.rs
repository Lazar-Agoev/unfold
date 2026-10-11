//! `import_deploy_files`: a one-time, reviewed import of Render deploy files into draft launches.
//! Unfold reads every `render.yaml` discovery sees (ignore files and `[scope]` excludes apply, as
//! for `package.json` and `pyproject.toml`) and the Dockerfiles they name, and returns one draft
//! per web, worker, and cron service. It never runs anything and writes nothing: the UI opens each
//! draft in the launch editor, and saving it is the normal edit of `unfold.toml`.
//!
//! A Docker service's command is its `dockerCommand`, else its Dockerfile's `ENTRYPOINT` and
//! `CMD`; its working folder is the repository folder that becomes `WORKDIR`, and an
//! `ENV PYTHONPATH` inside `WORKDIR` becomes the draft's `PYTHONPATH` ([`dockerfile`]). A native
//! service's command is its `startCommand`, run in its `rootDir`. A service named
//! `<other service>-<suffix>` joins that service's application (`dashboard-builder-export` joins
//! `dashboard-builder`); the comment block right above a service is its description.

mod dockerfile;

use std::collections::BTreeMap;

use saphyr::{LoadableYamlNode, MarkedYaml};
use serde::Serialize;
use ts_rs::TS;
use unfold_engine::settings::ScopeStatus;

use self::dockerfile::{Dockerfile, parent, repo_path};
use super::ProjectArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::workspace::Workspace;

/// The most `render.yaml` files one import reads.
pub const MAX_RENDER_FILES: usize = 64;
/// The largest `render.yaml` or Dockerfile read, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Name words written in capitals in an application's name.
const ACRONYMS: &[&str] = &["api", "sdk", "ui", "cli", "id", "url"];

/// What the deploy files say, as draft launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeployImport {
    /// The `render.yaml` files read, by path.
    pub files: Vec<String>,
    /// One draft per web, worker, and cron service, in file then document order.
    pub launches: Vec<ImportedLaunch>,
}

/// A Render service type the import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    /// `type: web`.
    Web,
    /// `type: worker`.
    Worker,
    /// `type: cron`.
    Cron,
}

/// The draft application a launch joins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DraftApplication {
    /// The owning service's name.
    pub id: String,
    /// The owning service's name in words (`invoice-hitl-api` → `Invoice hitl API`).
    pub name: String,
    /// The owning service's description.
    pub description: Option<String>,
}

/// One service as a draft launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportedLaunch {
    /// The service's name.
    pub service: String,
    /// The service's type.
    pub service_type: ServiceType,
    /// The application it joins.
    pub application: DraftApplication,
    /// The launch's ID: the service's name.
    pub id: String,
    /// The launch's name: the service type (`Web`, `Worker`, `Cron`).
    pub label: String,
    /// The command as deployed; `None` when the deploy files don't say (see `note`).
    pub command: Option<String>,
    /// The working folder, project-relative (`.` for the project's folder).
    pub cwd: String,
    /// The cron schedule as written.
    pub schedule: Option<String>,
    /// The comment block above the service: its first paragraph.
    pub description: Option<String>,
    /// Environment the command needs to find its code (`PYTHONPATH`).
    pub env: BTreeMap<String, String>,
    /// Where the command came from (`data-pipelines/Dockerfile.workers, CMD`,
    /// `render.yaml, dockerCommand`).
    pub copied_from: Option<String>,
    /// What the import couldn't find (`No Dockerfile found at Dockerfile`).
    pub note: Option<String>,
    /// The `render.yaml` that declares the service.
    pub render_file: String,
}

/// `import_deploy_files`: the project's Render services as draft launches. See the
/// [module documentation](self).
pub fn import_deploy_files(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<DeployImport> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let settings = workspace.settings().unwrap_or_default();
    let files = project
        .extras()
        .file_set(workspace.root(), &workspace.options().engine.discover)?;
    let render_files: Vec<&str> = files
        .paths
        .iter()
        .map(AsRef::as_ref)
        .filter(|path| {
            (*path == "render.yaml" || path.ends_with("/render.yaml"))
                && settings.scope.status_of(path) != ScopeStatus::Excluded
        })
        .take(MAX_RENDER_FILES)
        .collect();
    let is_file = |path: &str| files.paths.binary_search_by(|probe| (**probe).cmp(path)).is_ok();
    Ok(import(&render_files, is_file, |path| read_text(&workspace, path)))
}

/// A project file's text: `None` when it is missing, outside the folder, larger than
/// [`MAX_FILE_BYTES`], or not UTF-8.
fn read_text(workspace: &Workspace, path: &str) -> Option<String> {
    let confined = workspace.confine(path).ok()?;
    let metadata = std::fs::metadata(confined.absolute()).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    String::from_utf8(std::fs::read(confined.absolute()).ok()?).ok()
}

/// The drafts of `render_files`, reading files with `read`; `is_file` tells a copied file from
/// a folder.
fn import(
    render_files: &[&str],
    is_file: impl Fn(&str) -> bool,
    read: impl Fn(&str) -> Option<String>,
) -> DeployImport {
    let mut files = Vec::new();
    let mut services = Vec::new();
    for &path in render_files {
        let Some(found) = read(path).and_then(|text| services_of(&text)) else {
            continue;
        };
        files.push(path.to_owned());
        services.extend(found.into_iter().map(|service| (path, service)));
    }
    let names: Vec<&str> = services
        .iter()
        .map(|(_, service)| service.name.as_str())
        .collect();
    let launches = services
        .iter()
        .map(|(path, service)| {
            let owner = owner(&service.name, &names);
            let application = DraftApplication {
                id: owner.to_owned(),
                name: humanize(owner),
                description: services
                    .iter()
                    .find(|(_, other)| other.name == owner)
                    .and_then(|(_, other)| other.description.clone()),
            };
            draft(path, service, application, &is_file, &read)
        })
        .collect();
    DeployImport { files, launches }
}

/// One service of a `render.yaml`, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Service {
    kind: ServiceType,
    name: String,
    /// `runtime` (or the older `env`) is `docker`.
    docker: bool,
    root_dir: Option<String>,
    dockerfile_path: Option<String>,
    docker_context: Option<String>,
    docker_command: Option<String>,
    start_command: Option<String>,
    schedule: Option<String>,
    description: Option<String>,
}

/// The web, worker, and cron services of a `render.yaml`, in order; `None` when it isn't YAML.
/// Other types (`pserv`, `keyvalue`, …) and static sites are skipped.
fn services_of(text: &str) -> Option<Vec<Service>> {
    let documents = MarkedYaml::load_from_str(text).ok()?;
    let Some(services) = documents
        .first()
        .and_then(|document| document.data.as_mapping_get("services"))
        .and_then(|services| services.data.as_vec())
    else {
        return Some(Vec::new());
    };
    let lines: Vec<&str> = text.lines().collect();
    Some(
        services
            .iter()
            .filter_map(|item| {
                let field = |key: &str| {
                    let value = item.data.as_mapping_get(key)?.data.as_str()?.trim();
                    (!value.is_empty()).then(|| value.to_owned())
                };
                let kind = match field("type")?.as_str() {
                    "web" => ServiceType::Web,
                    "worker" => ServiceType::Worker,
                    "cron" => ServiceType::Cron,
                    _ => return None,
                };
                let runtime = field("runtime").or_else(|| field("env"));
                if runtime.as_deref() == Some("static") {
                    return None;
                }
                Some(Service {
                    kind,
                    name: field("name")?,
                    docker: runtime.as_deref() == Some("docker"),
                    root_dir: field("rootDir"),
                    dockerfile_path: field("dockerfilePath"),
                    docker_context: field("dockerContext"),
                    docker_command: field("dockerCommand"),
                    start_command: field("startCommand"),
                    schedule: field("schedule"),
                    description: comment_above(&lines, item.span.start.line()),
                })
            })
            .collect(),
    )
}

/// The first paragraph of the comment block right above one-based `line`, at the same
/// indentation, with `#` stripped and its lines joined with spaces.
fn comment_above(lines: &[&str], line: usize) -> Option<String> {
    let index = line.checked_sub(1)?;
    let indent = |text: &str| text.len() - text.trim_start().len();
    let item_indent = indent(lines.get(index)?);
    let mut block = Vec::new();
    for above in lines[..index].iter().rev() {
        let Some(comment) = above.trim_start().strip_prefix('#') else {
            break;
        };
        if indent(above) != item_indent {
            break;
        }
        block.push(comment.trim());
    }
    block.reverse();
    let paragraph: Vec<&str> = block
        .into_iter()
        .skip_while(|text| text.is_empty())
        .take_while(|text| !text.is_empty())
        .collect();
    (!paragraph.is_empty()).then(|| paragraph.join(" "))
}

/// The service whose application `name` joins: the longest other service name that `name`
/// extends with `-<suffix>`, followed to its own owner; `name` itself when there is none.
fn owner<'a>(name: &'a str, names: &[&'a str]) -> &'a str {
    let mut owner = name;
    while let Some(longer) = names
        .iter()
        .filter(|other| {
            owner
                .strip_prefix(**other)
                .is_some_and(|rest| rest.len() > 1 && rest.starts_with('-'))
        })
        .max_by_key(|other| other.len())
    {
        owner = longer;
    }
    owner
}

/// A service name in words: `-` and `_` become spaces, the first letter is a capital, and
/// acronyms are capitals (`invoice-hitl-api` → `Invoice hitl API`).
fn humanize(name: &str) -> String {
    let words: Vec<String> = name
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            if ACRONYMS.contains(&word.to_ascii_lowercase().as_str()) {
                word.to_ascii_uppercase()
            } else {
                word.to_owned()
            }
        })
        .collect();
    let text = words.join(" ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The draft launch of `service`, declared in `render_file`.
fn draft(
    render_file: &str,
    service: &Service,
    application: DraftApplication,
    is_file: impl Fn(&str) -> bool,
    read: impl Fn(&str) -> Option<String>,
) -> ImportedLaunch {
    let mut launch = ImportedLaunch {
        service: service.name.clone(),
        service_type: service.kind,
        application,
        id: service.name.clone(),
        label: match service.kind {
            ServiceType::Web => "Web",
            ServiceType::Worker => "Worker",
            ServiceType::Cron => "Cron",
        }
        .to_owned(),
        command: None,
        cwd: ".".to_owned(),
        schedule: service.schedule.clone(),
        description: service.description.clone(),
        env: BTreeMap::new(),
        copied_from: None,
        note: None,
        render_file: render_file.to_owned(),
    };
    let root_dir = service.root_dir.as_deref().and_then(|dir| repo_path(".", dir));
    if !service.docker {
        launch.cwd = root_dir.unwrap_or_else(|| ".".to_owned());
        match &service.start_command {
            Some(command) => {
                launch.command = Some(command.clone());
                launch.copied_from = Some(format!("{render_file}, startCommand"));
            }
            None => launch.note = Some(format!("No startCommand in {render_file}")),
        }
        return launch;
    }
    let context = service
        .docker_context
        .as_deref()
        .and_then(|context| repo_path(".", context))
        .unwrap_or_else(|| ".".to_owned());
    // Render's paths are relative to the repository's root.
    let mut candidates: Vec<String> = Vec::new();
    let paths = match &service.dockerfile_path {
        Some(path) => vec![repo_path(".", path)],
        None => vec![
            root_dir.and_then(|dir| repo_path(&dir, "Dockerfile")),
            Some("Dockerfile".to_owned()),
            repo_path(parent(render_file), "Dockerfile"),
        ],
    };
    for path in paths.into_iter().flatten() {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    let found = candidates
        .iter()
        .find_map(|path| read(path).map(|text| (path, Dockerfile::parse(&text))));
    match found {
        Some((path, dockerfile)) => {
            launch.cwd = dockerfile.working_folder(&context, is_file);
            launch.env = dockerfile.env();
            match dockerfile.start_command() {
                Some(start) => {
                    launch.command = Some(start.command);
                    launch.copied_from = Some(format!("{path}, {}", start.source));
                }
                None if service.docker_command.is_none() => {
                    launch.note = Some(format!("No CMD or ENTRYPOINT in {path}"));
                }
                None => {}
            }
        }
        None => {
            launch.cwd = context;
            launch.note = Some(format!("No Dockerfile found at {}", candidates.join(" or ")));
        }
    }
    if let Some(command) = &service.docker_command {
        launch.command = Some(command.clone());
        launch.copied_from = Some(format!("{render_file}, dockerCommand"));
    }
    launch
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENDER: &str = r#"# Blueprint notes, not about any one service.

services:
  # Query service: /analyst/* called only by pathway-web's server
  # with the shared token.
  #
  # Deploys on purpose.
  - type: web
    name: dashboard-builder
    runtime: docker
    dockerfilePath: ./data-pipelines/Dockerfile.dashboard-builder
    dockerContext: .
    envVars:
      # Shared with Vercel.
      - key: TOKEN
        sync: false

  # Nightly export.
  - type: cron
    name: dashboard-builder-export
    runtime: docker
    schedule: "0 10 * * *"
    dockerfilePath: ./data-pipelines/Dockerfile.dashboard-builder
    dockerCommand: python -m dashboard_builder.export --from-env

  - type: pserv
    name: private
  - type: web
    name: docs
    runtime: static
  - type: cron
    # Names the existing service.
    name: demo-sync-daily
    env: python
    schedule: "0 8 * * *"
    rootDir: jobs/
    buildCommand: pip install -r requirements.txt
    startCommand: python sync.py
"#;

    #[test]
    fn reads_web_worker_and_cron_services_with_their_comments() {
        let services = services_of(RENDER).expect("YAML");
        let names: Vec<&str> = services.iter().map(|service| service.name.as_str()).collect();
        assert_eq!(
            names,
            ["dashboard-builder", "dashboard-builder-export", "demo-sync-daily"]
        );
        assert_eq!(
            services[0].description.as_deref(),
            Some("Query service: /analyst/* called only by pathway-web's server with the shared token.")
        );
        assert_eq!(services[1].description.as_deref(), Some("Nightly export."));
        assert_eq!(
            services[2].description, None,
            "a comment inside the item isn't above it"
        );
        assert!(services[0].docker && !services[2].docker);
        assert_eq!(services[1].schedule.as_deref(), Some("0 10 * * *"));
        assert_eq!(services_of("services: [").map(|found| found.len()), None);
        assert_eq!(services_of("databases: []").map(|found| found.len()), Some(0));
    }

    #[test]
    fn groups_suffixed_services_under_their_owner() {
        let names = ["api", "api-export", "api-export-daily", "apiary", "worker"];
        assert_eq!(owner("api-export", &names), "api");
        assert_eq!(owner("api-export-daily", &names), "api");
        assert_eq!(owner("apiary", &names), "apiary");
        assert_eq!(owner("worker", &names), "worker");
        assert_eq!(owner("worker-", &names), "worker-");
    }

    #[test]
    fn names_applications_in_words() {
        assert_eq!(humanize("invoice-hitl-api"), "Invoice hitl API");
        assert_eq!(humanize("dashboard-builder"), "Dashboard builder");
        assert_eq!(humanize("menu_manager_sdk"), "Menu manager SDK");
        assert_eq!(humanize("ui-url-id"), "UI URL ID");
        assert_eq!(humanize("demo-sync-2-daily"), "Demo sync 2 daily");
    }

    #[test]
    fn drafts_launches_from_render_yaml_and_dockerfiles() {
        let files = BTreeMap::from([
            ("render.yaml", RENDER),
            (
                "data-pipelines/Dockerfile.dashboard-builder",
                "FROM python\nENV PYTHONPATH=/app\nWORKDIR /app\nCOPY data-pipelines/dashboard_builder ./dashboard_builder\nCMD [\"sh\", \"-c\", \"uvicorn dashboard_builder.app:app --port ${PORT:-8010}\"]\n",
            ),
        ]);
        let read = |path: &str| files.get(path).map(|text| (*text).to_owned());
        let import = import(&["render.yaml"], |_| false, read);
        assert_eq!(import.files, ["render.yaml"]);
        let [web, export, demo] = import.launches.as_slice() else {
            panic!("three launches: {:?}", import.launches);
        };
        assert_eq!(
            web.command.as_deref(),
            Some(r#"sh -c "uvicorn dashboard_builder.app:app --port ${PORT:-8010}""#)
        );
        assert_eq!(web.cwd, "data-pipelines");
        assert_eq!(
            web.env,
            BTreeMap::from([("PYTHONPATH".to_owned(), ".".to_owned())])
        );
        assert_eq!(
            web.copied_from.as_deref(),
            Some("data-pipelines/Dockerfile.dashboard-builder, CMD")
        );
        assert_eq!(web.label, "Web");
        assert_eq!(web.application.id, "dashboard-builder");
        assert_eq!(web.application.name, "Dashboard builder");

        assert_eq!(export.application, web.application);
        assert_eq!(export.id, "dashboard-builder-export");
        assert_eq!(export.label, "Cron");
        assert_eq!(
            export.command.as_deref(),
            Some("python -m dashboard_builder.export --from-env")
        );
        assert_eq!(export.copied_from.as_deref(), Some("render.yaml, dockerCommand"));
        assert_eq!(export.cwd, "data-pipelines");
        assert_eq!(export.schedule.as_deref(), Some("0 10 * * *"));

        assert_eq!(demo.command.as_deref(), Some("python sync.py"));
        assert_eq!(demo.cwd, "jobs");
        assert_eq!(demo.copied_from.as_deref(), Some("render.yaml, startCommand"));
        assert!(demo.env.is_empty());
    }

    #[test]
    fn a_missing_dockerfile_leaves_the_command_empty_with_a_note() {
        let render =
            "services:\n  - type: worker\n    name: poller\n    runtime: docker\n    rootDir: poller\n";
        let read = |path: &str| (path == "jobs/render.yaml").then(|| render.to_owned());
        let import = import(&["jobs/render.yaml"], |_| false, read);
        let [poller] = import.launches.as_slice() else {
            panic!("one launch");
        };
        assert_eq!(poller.command, None);
        assert_eq!(poller.cwd, ".");
        assert_eq!(
            poller.note.as_deref(),
            Some("No Dockerfile found at poller/Dockerfile or Dockerfile or jobs/Dockerfile")
        );
    }
}
