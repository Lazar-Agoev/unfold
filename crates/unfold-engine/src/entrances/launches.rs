//! Launches: the plans of configured processes, the Python import contexts they give linking,
//! their targets once linked, and the launch records kept beside the graph.
//!
//! 1. Before linking, every configured process with a `command` or a `target` is planned
//!    ([`plan_launch`]); package references read the selected `package.json` inputs, as package
//!    scripts do. Each Python plan contributes its context to
//!    [`ProjectInputs::python_contexts`](crate::ProjectInputs::python_contexts), so the
//!    settings and inputs that shape the link basis also cover them.
//! 2. After linking, a planned file target resolves like a package script's launch path; a
//!    Python target resolves through [`Linked::launch_target`] with the launch's own context,
//!    which also gives the blocks the launch runs first (its seeds).
//! 3. After assembly, [`records`] describes every process entrance: a configured launch from its
//!    table and plan; a detected one (`package.script`, `package.bin`, `pyproject.script`) from
//!    its manifest member, with the presentation of the `[[entrance_override]]` that matches its
//!    detector, file, and pointer. An override that matches nothing is a problem.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{DetectorId, EntranceKind, Reason};
use unfold_core::{
    DeclaredApplication, DeclaredMember, Entrance, Launch, LaunchIssue, LaunchMode, LaunchPath, Launches,
    RepoPath,
};

use super::TargetCandidate;
use crate::language::{LanguageProblem, Linked, ProjectInput};
use crate::launch::{
    LaunchDraft, LaunchPlan, LaunchTarget, PackageScripts, PythonContext, ScriptLookup, plan_launch,
};
use crate::program::{BlockRef, FactsSet};
use crate::resolve::Outcome;
use crate::settings::{ManualTargets, Process, ProcessStart, SETTINGS_PATH, Settings};

/// The selected `package.json` inputs, as configured commands look scripts up.
struct Packages {
    manifests: FxHashMap<Box<str>, serde_json::Value>,
}

impl Packages {
    /// The readable `package.json` inputs outside `node_modules`.
    fn new(inputs: &BTreeMap<RepoPath, ProjectInput>) -> Self {
        let manifests = inputs
            .iter()
            .filter(|(path, _)| {
                path.file_name() == "package.json"
                    && !path.as_str().split('/').any(|part| part == "node_modules")
            })
            .filter_map(|(path, input)| {
                let value = serde_json::from_str(input.snapshot.text()).ok()?;
                Some((path.as_str().into(), value))
            })
            .collect();
        Self { manifests }
    }
}

impl PackageScripts for Packages {
    fn script(&self, package: &str, name: &str) -> ScriptLookup<'_> {
        let Some(manifest) = self.manifests.get(package) else {
            return ScriptLookup::NoPackage;
        };
        match manifest.get("scripts").and_then(|scripts| scripts.get(name)) {
            None => ScriptLookup::Missing,
            Some(script) => script
                .as_str()
                .map_or(ScriptLookup::NotString, ScriptLookup::Script),
        }
    }
}

/// The draft of a configured process, as the planner reads it.
fn draft<'a>(process: &'a Process, env: &'a BTreeMap<String, String>) -> LaunchDraft<'a> {
    let target = match &process.start {
        Some(ProcessStart::Target(target)) => Some(target),
        _ => None,
    };
    LaunchDraft {
        command: process.command.as_deref(),
        cwd: &process.cwd,
        target,
        python_paths: process.python_paths.as_deref(),
        env,
    }
}

/// The declared environment as the planner takes it.
fn environment(process: &Process) -> BTreeMap<String, String> {
    process
        .env
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The process of an `[[entrance]]` table that is planned: one with a `command` or a `target`.
fn planned(targets: &ManualTargets) -> Option<&Process> {
    match targets {
        ManualTargets::Process(process) if !matches!(process.start, Some(ProcessStart::Startup(_))) => {
            Some(process)
        }
        _ => None,
    }
}

/// The plan of each `[[entrance]]` table, aligned with [`Settings::entrances`]: `None` for
/// entrances other than processes and for a process started by a `startup` locator. `root` is
/// the open folder's absolute path with `/` separators.
pub fn plans(
    settings: &Settings,
    inputs: &BTreeMap<RepoPath, ProjectInput>,
    root: &str,
) -> Vec<Option<LaunchPlan>> {
    if !settings
        .entrances
        .iter()
        .any(|entrance| planned(&entrance.targets).is_some())
    {
        return vec![None; settings.entrances.len()];
    }
    let packages = Packages::new(inputs);
    settings
        .entrances
        .iter()
        .map(|entrance| {
            let process = planned(&entrance.targets)?;
            let env = environment(process);
            Some(plan_launch(&draft(process, &env), &packages, root))
        })
        .collect()
}

/// The Python import contexts of the plans, in settings order.
pub fn contexts(plans: &[Option<LaunchPlan>]) -> Vec<PythonContext> {
    plans
        .iter()
        .flatten()
        .filter_map(LaunchPlan::python_context)
        .collect()
}

/// A configured process's startup target and the blocks it runs first.
pub struct Start {
    /// The startup target.
    pub target: TargetCandidate,
    /// The blocks the launch runs before it, in order.
    pub seeds: Vec<BlockRef>,
}

/// The start of each `[[entrance]]` table that is a process, aligned with
/// [`Settings::entrances`]: its `startup` locator, or its plan's target resolved after linking.
pub fn starts(
    settings: &Settings,
    plans: &[Option<LaunchPlan>],
    linked: &[&dyn Linked],
) -> Vec<Option<Start>> {
    settings
        .entrances
        .iter()
        .zip(plans)
        .map(|(entrance, plan)| {
            let ManualTargets::Process(process) = &entrance.targets else {
                return None;
            };
            if let Some(ProcessStart::Startup(locator)) = &process.start {
                return Some(Start {
                    target: TargetCandidate::Locator(locator.clone()),
                    seeds: Vec::new(),
                });
            }
            let plan = plan.as_ref()?;
            Some(start(process, plan, linked))
        })
        .collect()
}

fn start(process: &Process, plan: &LaunchPlan, linked: &[&dyn Linked]) -> Start {
    let unresolved = |reason, text: &str| Start {
        target: TargetCandidate::Failure {
            reason,
            text: text.into(),
        },
        seeds: Vec::new(),
    };
    let Some(target) = &plan.target else {
        return unresolved(
            Reason::Unsupported,
            process.command.as_deref().unwrap_or_default(),
        );
    };
    let text = target_text(target);
    let context = match target {
        // A file resolves like a package script's launch path, Python scripts included.
        LaunchTarget::File { path } => {
            return Start {
                target: TargetCandidate::Launch {
                    path: path.as_str().into(),
                    cwd: ".".into(),
                },
                seeds: Vec::new(),
            };
        }
        _ => plan.python_context(),
    };
    let resolution = context.and_then(|context| {
        linked
            .iter()
            .find_map(|linked| linked.launch_target(target, &context))
    });
    let Some(resolution) = resolution else {
        // Python isn't graphed.
        return unresolved(Reason::NotIndexed, &text);
    };
    match resolution.target {
        Outcome::Resolved(block) => Start {
            target: TargetCandidate::Block {
                block,
                text: Some(text.into()),
            },
            seeds: resolution.seeds,
        },
        Outcome::Unresolved { reason, .. } => unresolved(reason, &text),
        Outcome::External(_) => unresolved(Reason::NotFound, &text),
    }
}

/// A launch target as `path`, `module`, or `module:attribute`.
pub(crate) fn target_text(target: &LaunchTarget) -> String {
    match target {
        LaunchTarget::File { path } => path.to_string(),
        LaunchTarget::PythonModule { module } => module.to_string(),
        LaunchTarget::PythonCallable { module, attribute }
        | LaunchTarget::PythonAsgi {
            module, attribute, ..
        } => format!("{module}:{attribute}"),
    }
}

/// What [`records`] reads.
pub(crate) struct RecordInput<'a> {
    pub(crate) settings: &'a Settings,
    pub(crate) plans: &'a [Option<LaunchPlan>],
    pub(crate) starts: &'a [Option<Start>],
    /// The configuration inputs, for the lines detected launches are declared on.
    pub(crate) inputs: &'a BTreeMap<RepoPath, ProjectInput>,
    pub(crate) program: &'a FactsSet,
    /// The module root each console script's callable is defined in, by its evidence path and
    /// pointer.
    pub(crate) script_modules: &'a FxHashMap<(RepoPath, Box<str>), BlockRef>,
}

/// The launch records of the assembled `entrances`, and the overrides that match no detected
/// launch as problems.
pub(crate) fn records(input: &RecordInput<'_>, entrances: &[Entrance]) -> (Launches, Vec<LanguageProblem>) {
    let settings = input.settings;
    let mut launches = Launches {
        applications: settings
            .applications
            .iter()
            .map(|application| DeclaredApplication {
                id: application.id.clone(),
                name: application.name.clone(),
                description: application.description.clone(),
            })
            .collect(),
        ..Launches::default()
    };
    let position: FxHashMap<&str, usize> = settings
        .entrances
        .iter()
        .enumerate()
        .map(|(index, entrance)| (&*entrance.id, index))
        .collect();
    let mut matched = vec![false; settings.overrides.len()];
    let key = |block: BlockRef| input.program.block(block).map(|block| block.key.clone());
    for entrance in entrances {
        if let Some(index) = entrance.key.configured_id().and_then(|id| position.get(id)) {
            let configured = &settings.entrances[*index];
            match &configured.targets {
                ManualTargets::Process(process) => {
                    let plan = input.plans[*index].as_ref();
                    let seeds = input.starts[*index]
                        .as_ref()
                        .map_or(&[][..], |start| &start.seeds[..]);
                    launches.launches.push(Launch {
                        entrance: entrance.key.clone(),
                        application: configured.application.clone(),
                        label: configured.label.clone().filter(|label| !label.is_empty()),
                        description: configured.description.clone(),
                        schedule: process.schedule.clone(),
                        folder: process.cwd.as_str().into(),
                        command: process.command.clone(),
                        mode: match plan {
                            Some(plan) => plan.target.as_ref().map(mode),
                            None => Some(LaunchMode::File),
                        },
                        line: configured.line,
                        target_set_by_hand: plan.is_none_or(|plan| plan.target_set_by_hand),
                        import_paths: plan.map_or_else(Vec::new, |plan| {
                            plan.import_paths
                                .iter()
                                .map(|path| LaunchPath {
                                    path: path.path.as_str().into(),
                                    origin: path.origin.label().into(),
                                })
                                .collect()
                        }),
                        problem: plan
                            .and_then(|plan| plan.problem.as_ref())
                            .map(|problem| LaunchIssue {
                                label: problem.label.clone(),
                                message: problem.message.clone(),
                            }),
                        seeds: seeds.iter().filter_map(|&block| key(block)).collect(),
                    });
                }
                ManualTargets::Handlers(_) => {
                    if let Some(application) = &configured.application {
                        launches.members.push(DeclaredMember {
                            entrance: entrance.key.clone(),
                            application: application.clone(),
                        });
                    }
                }
            }
            continue;
        }
        if entrance.kind != EntranceKind::Process {
            continue;
        }
        let pointer = entrance.evidence.pointer.as_deref().unwrap_or_default();
        let found = settings.overrides.iter().position(|candidate| {
            candidate.selector.detector == entrance.detector
                && candidate.selector.path == entrance.evidence.path
                && *candidate.selector.pointer == *pointer
        });
        let presentation = found.map(|index| {
            matched[index] = true;
            &settings.overrides[index]
        });
        let trigger = entrance.trigger.as_deref().unwrap_or_default();
        let (command, mode) = match entrance.detector {
            DetectorId::PackageScript => (format!("npm run {trigger}"), LaunchMode::File),
            DetectorId::PyprojectScript => (trigger.to_owned(), LaunchMode::PythonCallable),
            _ => (trigger.to_owned(), LaunchMode::File),
        };
        let path = &entrance.evidence.path;
        let seeds = input
            .script_modules
            .get(&(path.clone(), pointer.into()))
            .and_then(|&block| key(block));
        launches.launches.push(Launch {
            entrance: entrance.key.clone(),
            application: presentation.and_then(|found| found.application.clone()),
            label: presentation.and_then(|found| found.label.clone()),
            description: presentation.and_then(|found| found.description.clone()),
            schedule: presentation.and_then(|found| found.schedule.clone()),
            folder: path
                .parent()
                .filter(|parent| !parent.is_empty())
                .unwrap_or(".")
                .into(),
            command: Some(command.into()),
            mode: Some(mode),
            line: input.inputs.get(path).map_or(1, |input| {
                input.snapshot.line_index().line_of(entrance.evidence.range.start)
            }),
            target_set_by_hand: false,
            import_paths: Vec::new(),
            problem: None,
            seeds: seeds.into_iter().collect(),
        });
    }
    let problems = settings
        .overrides
        .iter()
        .zip(matched)
        .filter(|(_, matched)| !matched)
        .map(|(orphan, _)| LanguageProblem {
            path: RepoPath::new(SETTINGS_PATH).expect("the settings path is a valid folder-relative path"),
            message: format!(
                "The [[entrance_override]] on line {} matches no detected launch ({} {} {})",
                orphan.line, orphan.selector.detector, orphan.selector.path, orphan.selector.pointer
            )
            .into(),
            incomplete: false,
        })
        .collect();
    (launches, problems)
}

fn mode(target: &LaunchTarget) -> LaunchMode {
    match target {
        LaunchTarget::File { .. } => LaunchMode::File,
        LaunchTarget::PythonModule { .. } => LaunchMode::PythonModule,
        LaunchTarget::PythonCallable { .. } => LaunchMode::PythonCallable,
        LaunchTarget::PythonAsgi { .. } => LaunchMode::PythonAsgi,
    }
}
