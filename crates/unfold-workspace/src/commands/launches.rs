//! `check_launch` and `launch_suggestions`: the launch editor's preview of a draft, and what
//! Add suggests for setting up launches.
//!
//! `check_launch` plans the draft with the engine's planner (the one compiles use) against the
//! inputs the current graph was linked from. Python targets and the draft's import path are
//! answered by linking Python again over the program the engine holds, which the engine thread
//! publishes after every update ([`crate::shared::LinkInputs`]): the linked program itself
//! borrows the engine's state and isn't kept. Until the engine has a graph, only the plan is
//! answered and `fromCurrentGraph` says so. Routes come from the stored catalog.

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{EntranceKind, Language, Reason};
use unfold_core::{Key, LaunchMode, RepoPath};
use unfold_engine::entrances::launches::plan_draft;
use unfold_engine::launch::{LaunchDraft, LaunchPlan, LaunchTarget, PythonContext};
use unfold_engine::settings::FolderPath;
use unfold_engine::{Linked, Outcome, ProjectInputs};
use unfold_reader::launches::start_kind;
use unfold_reader::notes::reason_text;

use super::entrances::{Catalog, ImportPathRow, LaunchStatus, unindexed_note};
use super::{ProjectArgs, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::settings_edit::LaunchTargetSpec;
use crate::workspace::Workspace;

/// How many newly resolving imports a preview names.
const IMPORT_EXAMPLES: usize = 3;
/// How many local-looking imports the suggestions name.
const LOCAL_LOOKING: usize = 3;

/// The arguments of `check_launch`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CheckLaunchArgs {
    /// The project.
    pub project: ProjectId,
    /// The launch as the editor holds it.
    pub draft: LaunchDraftSpec,
}

/// A process launch being edited.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LaunchDraftSpec {
    /// The `[[entrance]]` ID, when editing a saved launch.
    #[ts(optional)]
    pub id: Option<String>,
    /// The command line.
    #[ts(optional)]
    pub command: Option<String>,
    /// The working folder, project-relative (`.` when empty).
    pub cwd: String,
    /// The target set by hand.
    #[ts(optional)]
    pub target: Option<LaunchTargetSpec>,
    /// The import path set by hand, in order.
    #[ts(optional)]
    pub python_paths: Option<Vec<String>>,
    /// Declared environment variables.
    pub env: BTreeMap<String, String>,
}

/// What Unfold reads from a draft launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchCheck {
    /// The command's parts with their meanings ("What Unfold reads").
    pub parts: Vec<CommandPartView>,
    /// Why the launch can't be read, or doesn't add up.
    pub problem: Option<LaunchProblemView>,
    /// For `sh -c "…"`: what the quoted command would fill in.
    pub suggestion: Option<LaunchFill>,
    /// Whether the target is set by hand.
    pub target_set_by_hand: bool,
    /// The Python import path, in search order.
    pub import_paths: Vec<ImportPathRow>,
    /// What the launch starts.
    pub starts: Option<StartPreview>,
    /// Imports that would newly resolve to code in this repository.
    pub imports: Option<ImportGain>,
    /// The routes that would join the launch's application.
    pub routes: Option<RoutePreview>,
    /// Whether the preview comes from the graph as compiled (the project rests, or links
    /// without this draft), rather than linking with the draft.
    pub from_current_graph: bool,
}

/// A run of command words and what they mean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommandPartView {
    /// The words as written.
    pub text: String,
    /// What they mean.
    pub meaning: String,
    /// They don't change what code runs.
    pub irrelevant: bool,
}

/// Why a launch can't be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProblemView {
    /// A short status (`Can't read sh -c`).
    pub label: String,
    /// The full explanation.
    pub message: String,
}

/// What the quoted command of `sh -c "…"` fills in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchFill {
    /// The quoted command's meaningful words (`uvicorn dashboard_builder.app:app`).
    pub quoted: String,
    /// The target it starts.
    pub target: Option<LaunchTargetSpec>,
    /// The import path it gives, in order.
    pub python_paths: Vec<String>,
}

/// What a draft starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartPreview {
    /// The file.
    pub path: String,
    /// The binding or callable it names (`app`), if any.
    pub name: Option<String>,
    /// What it is (`A FastAPI application with 2 routes`).
    pub description: String,
    /// Whether Unfold can follow it into the code.
    pub status: LaunchStatus,
    /// Why not, when it can't.
    pub reason: Option<String>,
}

/// Imports a draft's import path would newly resolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportGain {
    /// How many ("18 more imports resolve to code in this repository").
    pub count: u32,
    /// The first few modules.
    pub examples: Vec<String>,
}

/// The routes a draft's application would serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RoutePreview {
    /// Unique route declarations.
    pub count: u32,
    /// The application that already serves them, by name.
    pub application: Option<String>,
    /// The router files other applications also mount.
    pub shared: Vec<SharedRouter>,
}

/// A router file several applications mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SharedRouter {
    /// The router file.
    pub path: String,
    /// How many of the routes come from it.
    pub routes: u32,
    /// The other applications that mount it, by name.
    pub also_in: Vec<String>,
}

/// What Add suggests for setting up launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSuggestions {
    /// Unique route declarations.
    pub routes: u32,
    /// Python imports that resolve to code in this repository, of all Python imports.
    pub imports: ImportStats,
    /// Unresolved imports that look like folders of this repository (`shared`, `api`, `utils`).
    pub local_looking: Vec<String>,
    /// FastAPI applications no launch serves.
    pub apps: Vec<SuggestedApp>,
}

/// Import resolution counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    /// Imports that resolve to code in this repository.
    pub resolved: u32,
    /// Every import.
    pub total: u32,
}

/// A FastAPI application no launch serves, with a launch that would.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedApp {
    /// Its file.
    pub path: String,
    /// The binding (`app`).
    pub attribute: String,
    /// The suggested working folder (`.` for the project's folder).
    pub folder: String,
    /// The suggested command (`uvicorn dashboard_builder.app:app`).
    pub command: String,
    /// Its unique route declarations.
    pub routes: u32,
}

/// `check_launch`: what Unfold reads from a draft launch. Runs and writes nothing.
pub fn check_launch(ctx: &Ctx<'_>, args: CheckLaunchArgs) -> CommandResult<LaunchCheck> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let spec = &args.draft;
    let folder = |text: &str| {
        FolderPath::parse(if text.is_empty() { "." } else { text })
            .map_err(|error| CommandError::invalid_args(format!("folder `{text}`: {error}")))
    };
    let cwd = folder(&spec.cwd)?;
    let target = spec.target.as_ref().map(launch_target).transpose()?;
    let python_paths = spec
        .python_paths
        .as_ref()
        .map(|paths| {
            paths
                .iter()
                .map(|path| folder(path))
                .collect::<CommandResult<Vec<_>>>()
        })
        .transpose()?;
    let draft = LaunchDraft {
        command: spec.command.as_deref(),
        cwd: &cwd,
        target: target.as_ref(),
        python_paths: python_paths.as_deref(),
        env: &spec.env,
    };
    let linked_from = workspace.linked_from();
    let plan = match &linked_from {
        Some(linked_from) => plan_draft(&draft, &linked_from.project),
        None => {
            let project = ProjectInputs {
                settings: workspace.settings().unwrap_or_default(),
                root: Some(workspace.root().to_owned()),
                ..ProjectInputs::default()
            };
            plan_draft(&draft, &project)
        }
    };
    let mut check = LaunchCheck {
        parts: plan
            .parts
            .iter()
            .map(|part| CommandPartView {
                text: part.text.to_string(),
                meaning: part.meaning.to_string(),
                irrelevant: part.irrelevant,
            })
            .collect(),
        problem: plan.problem.as_ref().map(|problem| LaunchProblemView {
            label: problem.label.to_string(),
            message: problem.message.to_string(),
        }),
        suggestion: plan.suggestion.as_ref().map(|suggestion| LaunchFill {
            quoted: suggestion.quoted.to_string(),
            target: suggestion.plan.target.as_ref().map(LaunchTargetSpec::from),
            python_paths: suggestion
                .plan
                .import_paths
                .iter()
                .map(|path| path.path.as_str().to_owned())
                .collect(),
        }),
        target_set_by_hand: plan.target_set_by_hand,
        import_paths: import_paths(&plan),
        starts: None,
        imports: None,
        routes: None,
        from_current_graph: linked_from.is_none(),
    };
    let (Some(linked_from), Some(target)) = (&linked_from, &plan.target) else {
        return Ok(check);
    };
    ctx.check_cancelled()?;
    let program = &linked_from.program;
    let python = workspace
        .language(Language::Python)
        .filter(|_| linked_from.project.settings.language_enabled(Language::Python));
    let linked = python.map(|python| python.link(program, &linked_from.project));
    let context = plan.python_context();
    let outcome = match (target, &linked, &context) {
        (LaunchTarget::File { path }, _, _) => file_outcome(&workspace, program, path),
        (_, Some(linked), Some(context)) => linked
            .launch_target(target, context)
            .map_or(Outcome::unresolved(Reason::NotIndexed), |resolution| {
                resolution.target
            }),
        _ => Outcome::unresolved(Reason::NotIndexed),
    };
    ctx.check_cancelled()?;
    if let (Some(linked), Some(context)) = (&linked, &context) {
        check.imports = import_gain(linked.as_ref(), context);
    }
    let catalog = match workspace.readers() {
        Ok(pool) => Some(Catalog::read(&pool.get()?)?),
        Err(_) => None,
    };
    let own = spec.id.as_deref().map(Key::configured_entrance).and_then(|key| {
        catalog
            .as_ref()?
            .entrances
            .iter()
            .position(|entrance| entrance.key == key)
    });
    let mode = plan.target.as_ref().map(mode);
    check.starts = Some(match outcome {
        Outcome::Resolved(block) => {
            let found = program.block(block);
            let path = program.file(block.file).map(|file| file.path.clone());
            let key = found.map(|found| found.key.clone());
            let routes = match (&catalog, &key, &path) {
                (Some(catalog), Some(key), Some(path)) => route_preview(catalog, key, path, own),
                _ => None,
            };
            let language = program
                .file(block.file)
                .map_or(Language::Python, |file| file.language);
            let routes_count = routes.as_ref().map_or(0, |routes| routes.count);
            let kind = start_kind(mode, language, routes_count > 0);
            check.routes = routes;
            StartPreview {
                path: path.map(|path| path.as_str().to_owned()).unwrap_or_default(),
                name: found
                    .filter(|_| block.local != 0)
                    .and_then(|found| found.name.as_deref())
                    .map(str::to_owned),
                description: describe(kind, routes_count),
                status: LaunchStatus::Ok,
                reason: None,
            }
        }
        Outcome::Unresolved { reason, .. } => {
            let (path, name) = written(target);
            let (status, reason) = match reason {
                Reason::NotIndexed => (
                    LaunchStatus::NotIndexed,
                    unindexed_note(
                        workspace.root(),
                        &linked_from.project.settings,
                        &workspace.options().engine.discover,
                        &unindexed_path(target, &plan),
                    ),
                ),
                Reason::NotFound => (LaunchStatus::NotFound, format!("{path} wasn't found")),
                other => (LaunchStatus::Unreadable, reason_text(other).to_owned()),
            };
            StartPreview {
                path,
                name,
                description: describe(start_kind(mode, Language::Python, false), 0),
                status,
                reason: Some(reason),
            }
        }
        Outcome::External(_) => StartPreview {
            path: written(target).0,
            name: None,
            description: describe(start_kind(mode, Language::Python, false), 0),
            status: LaunchStatus::NotFound,
            reason: Some("It is outside this repository".to_owned()),
        },
    });
    Ok(check)
}

/// A draft's target as the planner takes it.
fn launch_target(spec: &LaunchTargetSpec) -> CommandResult<LaunchTarget> {
    Ok(match spec {
        LaunchTargetSpec::File { path } => LaunchTarget::File {
            path: RepoPath::new(path)
                .map_err(|error| CommandError::invalid_args(format!("target path `{path}`: {error}")))?,
        },
        LaunchTargetSpec::PythonModule { module } => LaunchTarget::PythonModule {
            module: module.as_str().into(),
        },
        LaunchTargetSpec::PythonCallable { module, attribute } => LaunchTarget::PythonCallable {
            module: module.as_str().into(),
            attribute: attribute.as_str().into(),
        },
        LaunchTargetSpec::PythonAsgi {
            module,
            attribute,
            factory,
        } => LaunchTarget::PythonAsgi {
            module: module.as_str().into(),
            attribute: attribute.as_str().into(),
            factory: *factory,
        },
    })
}

fn mode(target: &LaunchTarget) -> LaunchMode {
    match target {
        LaunchTarget::File { .. } => LaunchMode::File,
        LaunchTarget::PythonModule { .. } => LaunchMode::PythonModule,
        LaunchTarget::PythonCallable { .. } => LaunchMode::PythonCallable,
        LaunchTarget::PythonAsgi { .. } => LaunchMode::PythonAsgi,
    }
}

fn import_paths(plan: &LaunchPlan) -> Vec<ImportPathRow> {
    plan.import_paths
        .iter()
        .map(|path| ImportPathRow {
            path: path.path.as_str().to_owned(),
            origin: path.origin.label().to_owned(),
        })
        .collect()
}

/// A launched file: its root when indexed, else whether it exists.
fn file_outcome(workspace: &Workspace, program: &unfold_engine::FactsSet, path: &RepoPath) -> Outcome {
    if let Some(file) = program.find(path.as_str())
        && program.facts(file).is_some()
    {
        return Outcome::Resolved(unfold_engine::BlockRef { file, local: 0 });
    }
    let exists = workspace
        .confine(path.as_str())
        .is_ok_and(|confined| confined.absolute().exists());
    Outcome::unresolved(if exists {
        Reason::NotIndexed
    } else {
        Reason::NotFound
    })
}

/// The file and binding a target names, as written.
fn written(target: &LaunchTarget) -> (String, Option<String>) {
    match target {
        LaunchTarget::File { path } => (path.to_string(), None),
        LaunchTarget::PythonModule { module } => (module.to_string(), None),
        LaunchTarget::PythonCallable { module, attribute }
        | LaunchTarget::PythonAsgi {
            module, attribute, ..
        } => (module.to_string(), Some(attribute.to_string())),
    }
}

/// The project path a target that isn't indexed names: its file, or its module under the first
/// import folder.
fn unindexed_path(target: &LaunchTarget, plan: &LaunchPlan) -> String {
    let (written, _) = written(target);
    if matches!(target, LaunchTarget::File { .. }) {
        return written;
    }
    let module = written.replace('.', "/");
    match plan
        .import_paths
        .first()
        .and_then(|path| path.path.as_repo_path())
    {
        Some(folder) => format!("{folder}/{module}"),
        None => module,
    }
}

/// `A FastAPI application with 2 routes`, `A Python module`.
fn describe(kind: &str, routes: u32) -> String {
    let article = if kind.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "An"
    } else {
        "A"
    };
    match routes {
        0 => format!("{article} {kind}"),
        1 => format!("{article} {kind} with 1 route"),
        many => format!("{article} {kind} with {many} routes"),
    }
}

/// The imports the context resolves to this repository that the graph doesn't.
fn import_gain(linked: &dyn Linked, context: &PythonContext) -> Option<ImportGain> {
    let preview = linked.context_preview(context)?;
    let gained = preview.resolved.saturating_sub(preview.resolved_now);
    (gained > 0).then(|| ImportGain {
        count: gained,
        examples: preview
            .newly_resolved
            .iter()
            .take(IMPORT_EXAMPLES)
            .map(|module| module.to_string())
            .collect(),
    })
}

/// The routes bound to the application at `key` (in `file`): how many, the application already
/// serving them (other than the draft's own, the entrance at `own`), and the router files other
/// applications also mount.
fn route_preview(catalog: &Catalog, key: &Key, file: &RepoPath, own: Option<usize>) -> Option<RoutePreview> {
    let routes: Vec<usize> = (0..catalog.entrances.len())
        .filter(|&position| {
            catalog.is_route(position) && catalog.entrances[position].binding.as_ref() == Some(key)
        })
        .collect();
    if routes.is_empty() {
        return None;
    }
    let own_apps: &[usize] = own.map_or(&[], |own| &catalog.apps_of[own]);
    let name = |app: usize| catalog.applications[app].name.to_string();
    let application = routes
        .iter()
        .flat_map(|&route| catalog.apps_of[route].iter().copied())
        .find(|app| !own_apps.contains(app))
        .map(name);
    let mut routers: BTreeMap<&str, (Vec<usize>, BTreeSet<String>)> = BTreeMap::new();
    for &route in &routes {
        let evidence = &catalog.entrances[route].evidence.path;
        if evidence == file {
            continue;
        }
        let (members, also_in) = routers.entry(evidence.as_str()).or_default();
        members.push(route);
        let registration = catalog.registration[route];
        for (twin, entrance) in catalog.entrances.iter().enumerate() {
            if catalog.registration[twin] == registration && entrance.binding.as_ref() != Some(key) {
                also_in.extend(catalog.apps_of[twin].iter().copied().map(name));
            }
        }
    }
    Some(RoutePreview {
        count: catalog.unique(routes.iter().copied()),
        application,
        shared: routers
            .into_iter()
            .filter(|(_, (_, also_in))| !also_in.is_empty())
            .map(|(path, (members, also_in))| SharedRouter {
                path: path.to_owned(),
                routes: catalog.unique(members),
                also_in: also_in.into_iter().collect(),
            })
            .collect(),
    })
}

/// `launch_suggestions`: route and import counts, the imports that look like this repository's
/// folders, and the FastAPI applications no launch serves. Served from the stored graph.
pub fn launch_suggestions(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<LaunchSuggestions> {
    let project = ctx.project(&args.project)?;
    let pool = project
        .readers()?
        .ok_or(crate::error::WorkspaceError::NotCompiled)?;
    let reader = pool.get()?;
    let catalog = Catalog::read(&reader)?;
    let summary = reader.import_summary(Language::Python)?;
    let files = reader.files()?;
    // Folder names that hold Python files, and Python module names.
    let mut local: BTreeSet<&str> = BTreeSet::new();
    for file in files.iter().filter(|file| file.language == Language::Python) {
        let path = file.path.as_str();
        let mut parts: Vec<&str> = path.split('/').collect();
        if let Some(name) = parts.pop() {
            local.insert(name.strip_suffix(".py").unwrap_or(name));
        }
        local.extend(parts);
    }
    let local_looking = summary
        .external
        .iter()
        .map(|(module, _)| &**module)
        .filter(|module| !module.starts_with("python:") && local.contains(module))
        .take(LOCAL_LOOKING)
        .map(str::to_owned)
        .collect();
    // Each route binding, with its routes, when no application serves any of them.
    let mut bindings: FxHashMap<&Key, Vec<usize>> = FxHashMap::default();
    for (position, entrance) in catalog.entrances.iter().enumerate() {
        if entrance.kind == EntranceKind::Http
            && let Some(binding) = &entrance.binding
        {
            bindings.entry(binding).or_default().push(position);
        }
    }
    let mut apps = Vec::new();
    for (binding, routes) in bindings {
        ctx.check_cancelled()?;
        if routes.iter().any(|&route| !catalog.apps_of[route].is_empty()) {
            continue;
        }
        let Some(located) = reader.block(binding.as_str())? else {
            continue;
        };
        let Some(attribute) = located.block.name.as_deref() else {
            continue;
        };
        let (folder, module) = launch_folder(located.path.as_str());
        apps.push(SuggestedApp {
            path: located.path.as_str().to_owned(),
            attribute: attribute.to_owned(),
            folder,
            command: format!("uvicorn {module}:{attribute}"),
            routes: catalog.unique(routes),
        });
    }
    apps.sort_by(|a, b| b.routes.cmp(&a.routes).then_with(|| a.path.cmp(&b.path)));
    Ok(LaunchSuggestions {
        routes: catalog.unique((0..catalog.entrances.len()).filter(|&position| catalog.is_route(position))),
        imports: ImportStats {
            resolved: count(usize::try_from(summary.resolved).unwrap_or(usize::MAX)),
            total: count(usize::try_from(summary.total).unwrap_or(usize::MAX)),
        },
        local_looking,
        apps,
    })
}

/// The folder a module file is launched from, and its dotted module there: climb from the
/// file's folder while folder names are Python identifiers; the first that isn't (or the
/// project's folder) is where the module path starts.
fn launch_folder(path: &str) -> (String, String) {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or_default();
    let stem = file.strip_suffix(".py").unwrap_or(file);
    let package = parts.iter().rev().take_while(|part| is_identifier(part)).count();
    let start = parts.len() - package;
    let folder = if start == 0 {
        ".".to_owned()
    } else {
        parts[..start].join("/")
    };
    let mut module: Vec<&str> = parts[start..].to_vec();
    if stem != "__init__" {
        module.push(stem);
    }
    (folder, module.join("."))
}

/// Whether `text` is a Python identifier.
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|next| next == '_' || next.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_folders_climb_while_folders_are_packages() {
        assert_eq!(
            launch_folder("data-pipelines/dashboard_builder/app.py"),
            ("data-pipelines".to_owned(), "dashboard_builder.app".to_owned())
        );
        assert_eq!(
            launch_folder("api/main.py"),
            (".".to_owned(), "api.main".to_owned())
        );
        assert_eq!(launch_folder("main.py"), (".".to_owned(), "main".to_owned()));
        assert_eq!(
            launch_folder("food-cost/api/__init__.py"),
            ("food-cost".to_owned(), "api".to_owned())
        );
    }

    #[test]
    fn descriptions_count_routes() {
        assert_eq!(
            describe("FastAPI application", 2),
            "A FastAPI application with 2 routes"
        );
        assert_eq!(describe("ASGI application", 0), "An ASGI application");
        assert_eq!(describe("Python module", 0), "A Python module");
    }
}
