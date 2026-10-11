//! Planning a configured process launch: what its command starts, the import path it gives
//! Python, and a plain reading of each part of the command for the launch editor.
//!
//! A configured command uses the package-script grammar with a stricter composition rule: it
//! has exactly one launching step, the last. Earlier steps are preparatory and unmodeled, so
//! `npm run build && node dist/server.js` starts `dist/server.js`, but `node a.js && node b.js`
//! is refused. Besides the Node launchers and package references (a configured `npm run start`
//! selects `cwd/package.json`), it reads Python and Uvicorn launches ([`super::python`]).
//!
//! Planning is pure: commands come in as text, packages through [`PackageScripts`], and the host
//! environment is never read; only the declared `env` table counts.

use std::collections::BTreeMap;

use unfold_core::RepoPath;

use super::launcher::{LAUNCHERS, Step, launcher};
use super::python;
use super::shell::{Word, steps};
use super::{
    Interpreter, LaunchTarget, PackageScripts, PythonContext, PythonEntry, Refusal, change_directory,
    command, launch_file, resolve,
};
use crate::settings::FolderPath;

/// A configured process launch as written, before planning.
#[derive(Debug, Clone, Copy)]
pub struct LaunchDraft<'a> {
    /// The command as written, if any.
    pub command: Option<&'a str>,
    /// The project-relative working folder the command runs in.
    pub cwd: &'a FolderPath,
    /// The target set by hand, if any.
    pub target: Option<&'a LaunchTarget>,
    /// Import paths set by hand; they replace the inferred ones.
    pub python_paths: Option<&'a [FolderPath]>,
    /// The declared environment; only `PYTHONPATH` matters.
    pub env: &'a BTreeMap<String, String>,
}

/// What a configured launch starts, as far as Unfold can read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// The effective target: the command's, or the one set by hand. `None` when neither says,
    /// or when they disagree.
    pub target: Option<LaunchTarget>,
    /// Whether the target was set by hand.
    pub target_set_by_hand: bool,
    /// The Python import path, in search order, without repeats. Empty for other launches.
    pub import_paths: Vec<ImportPath>,
    /// The command's parts with their meanings, in written order. Empty when the command
    /// can't be read.
    pub parts: Vec<CommandPart>,
    /// Why the launch can't be read, or doesn't add up.
    pub problem: Option<LaunchProblem>,
    /// For `sh -c "…"`: the plan of the quoted command, which the editor can fill in.
    pub suggestion: Option<Box<Suggestion>>,
}

/// One folder on a launch's import path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPath {
    /// The project-relative folder.
    pub path: FolderPath,
    /// Why it is on the path.
    pub origin: ImportPathOrigin,
}

/// Why a folder is on a launch's import path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportPathOrigin {
    /// Uvicorn's `--app-dir`.
    AppDir,
    /// The working folder, as Uvicorn's default app folder or for a target set by hand.
    WorkingFolder,
    /// The working folder, which `python -m` puts on the path.
    ModuleWorkingFolder,
    /// The folder of the script Python runs.
    ScriptFolder,
    /// An entry of `PYTHONPATH`.
    PythonPath,
    /// Listed in `python_paths`.
    SetByHand,
}

impl ImportPathOrigin {
    /// How the editor labels the origin.
    pub fn label(self) -> &'static str {
        match self {
            Self::AppDir => "from --app-dir",
            Self::WorkingFolder => "the working folder",
            Self::ModuleWorkingFolder => "the working folder, for python -m",
            Self::ScriptFolder => "script folder",
            Self::PythonPath => "from PYTHONPATH",
            Self::SetByHand => "set by hand",
        }
    }
}

/// A run of command words and what they mean, for "What Unfold reads".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPart {
    /// The words as written.
    pub text: Box<str>,
    /// What they mean, in a short sentence without a final period.
    pub meaning: Box<str>,
    /// They don't change what code runs.
    pub irrelevant: bool,
}

/// Why a launch can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProblem {
    /// A short status, such as "Can't read sh -c".
    pub label: Box<str>,
    /// The full explanation.
    pub message: Box<str>,
}

/// The plan of the command quoted in `sh -c "…"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The quoted command's meaningful words, such as `uvicorn dashboard_builder.app:app`.
    pub quoted: Box<str>,
    /// The quoted command's plan, with its target and import path inferred.
    pub plan: LaunchPlan,
}

impl LaunchPlan {
    /// The import context of a Python launch: its entry and import path.
    pub fn python_context(&self) -> Option<PythonContext> {
        let entry = match self.target.as_ref()? {
            LaunchTarget::File { path } if is_python_file(path) => PythonEntry::Script(path.clone()),
            LaunchTarget::File { .. } => return None,
            LaunchTarget::PythonModule { module }
            | LaunchTarget::PythonCallable { module, .. }
            | LaunchTarget::PythonAsgi { module, .. } => PythonEntry::Module(module.clone()),
        };
        Some(PythonContext {
            entry,
            roots: self.import_paths.iter().map(|path| path.path.clone()).collect(),
        })
    }
}

/// Plans a configured launch. `root` is the open folder's absolute path with `/` separators; it
/// maps absolute paths in the command back into the project.
pub fn plan_launch(draft: &LaunchDraft<'_>, packages: &dyn PackageScripts, root: &str) -> LaunchPlan {
    let root = root.trim_end_matches('/');
    let command = draft.command.filter(|command| !command.trim().is_empty());
    let (parts, read) = match command {
        Some(command) => {
            let mut reader = Reader::new(packages, root, draft);
            let read = reader.command(command);
            // Nothing is read from a command outside the grammar.
            let outside = matches!(&read, Err(problem) if &*problem.label == CANT_READ_COMMAND);
            (if outside { Vec::new() } else { reader.parts.0 }, Some(read))
        }
        None => (Vec::new(), None),
    };
    let derived = |target: &LaunchTarget| target_paths(target, draft, root);
    let (target, import_paths, problem) = match (read, draft.target) {
        (None, None) => (None, Ok(Vec::new()), Some(nothing_to_start())),
        (None, Some(target)) => (Some(target.clone()), derived(target), None),
        (Some(Ok(launch)), None) => (Some(launch.target), Ok(launch.import_paths), None),
        (Some(Ok(launch)), Some(target)) if launch.target == *target => {
            (Some(launch.target), Ok(launch.import_paths), None)
        }
        (Some(Ok(launch)), Some(target)) => (None, Ok(Vec::new()), Some(disagree(&launch.target, target))),
        (Some(Err(problem)), None) => (None, Ok(Vec::new()), Some(problem)),
        (Some(Err(problem)), Some(target)) => (Some(target.clone()), derived(target), Some(problem)),
    };
    let (import_paths, problem) = match (draft.python_paths, import_paths) {
        (Some(paths), _) => (set_by_hand(paths), problem),
        (None, Ok(paths)) => (paths, problem),
        (None, Err(refusal)) => (Vec::new(), problem.or_else(|| Some(refusal.into()))),
    };
    LaunchPlan {
        target,
        target_set_by_hand: draft.target.is_some(),
        import_paths,
        parts,
        problem,
        suggestion: command.and_then(|command| suggest(command, draft, packages, root)),
    }
}

/// For a command that is only `sh -c "<command>"` (or `bash -c`), the plan of the quoted
/// command, when it can be read.
fn suggest(
    command: &str,
    draft: &LaunchDraft<'_>,
    packages: &dyn PackageScripts,
    root: &str,
) -> Option<Box<Suggestion>> {
    let steps = steps(command).ok()?;
    let [step] = steps.as_slice() else {
        return None;
    };
    let found = super::command(step).ok()??;
    let [flag, quoted] = found.args else {
        return None;
    };
    if !found.prefix.is_empty() || !is_shell(found.name) || flag.text != "-c" {
        return None;
    }
    let inner = LaunchDraft {
        command: Some(&quoted.text),
        target: None,
        python_paths: None,
        ..*draft
    };
    let plan = plan_launch(&inner, packages, root);
    if plan.problem.is_some() || plan.target.is_none() {
        return None;
    }
    let quoted = plan
        .parts
        .iter()
        .filter(|part| !part.irrelevant)
        .map(|part| &*part.text)
        .collect::<Vec<_>>()
        .join(" ");
    Some(Box::new(Suggestion {
        quoted: quoted.into(),
        plan,
    }))
}

/// What a readable command starts.
struct Launch {
    target: LaunchTarget,
    import_paths: Vec<ImportPath>,
}

/// Reads one configured command, step by step.
struct Reader<'a> {
    interpreter: Interpreter<'a>,
    place: Place<'a>,
    env: &'a BTreeMap<String, String>,
    parts: Parts,
}

impl<'a> Reader<'a> {
    fn new(packages: &'a dyn PackageScripts, root: &'a str, draft: &LaunchDraft<'a>) -> Self {
        Self {
            interpreter: Interpreter {
                packages,
                root,
                active: Vec::new(),
                interpreted: 0,
            },
            place: Place {
                root,
                cwd: draft.cwd.as_str().to_owned(),
            },
            env: draft.env,
            parts: Parts::default(),
        }
    }

    fn command(&mut self, text: &str) -> Result<Launch, LaunchProblem> {
        let steps = steps(text)?;
        let Some((last, preparatory)) = steps.split_last() else {
            return Err(nothing_to_start());
        };
        for step in preparatory {
            match command(step)? {
                Some(found) if found.name == "cd" => {
                    self.place.cwd = change_directory(&self.place.cwd, found.args)?;
                    let directory = &found.args[0].text;
                    self.parts
                        .add(step, format!("Moves into {directory} before the launch"));
                }
                Some(found) if self.launches(found.name, found.args)? => {
                    return Err(LaunchProblem {
                        label: "More than one launch".into(),
                        message: format!(
                            "Only the last step may start code, but {} starts code before it.",
                            written(step)
                        )
                        .into(),
                    });
                }
                _ => self.parts.irrelevant(step, RUNS_BEFORE),
            }
        }
        self.terminal(last)
    }

    /// Whether a preparatory step starts code.
    fn launches(&mut self, name: &str, args: &[Word]) -> Result<bool, Refusal> {
        let package = self.package();
        let reference = self
            .interpreter
            .reference(name, args, &package, &self.place.cwd)?;
        Ok(match reference {
            Some(reference) => self.interpreter.follow(&reference, 0)?.is_some(),
            None if LAUNCHERS.contains(&name) => {
                matches!(launcher(name, args, &self.place.cwd)?, Step::Launch(..))
            }
            None => python::is_launcher(name),
        })
    }

    /// Reads the last step, which must launch.
    fn terminal(&mut self, step: &[Word]) -> Result<Launch, LaunchProblem> {
        let Some(found) = command(step)? else {
            self.parts.irrelevant(step, ENVIRONMENT);
            return Err(starts_no_code());
        };
        let from_command = &step[found.prefix.len()..];
        if python::is_launcher(found.name) {
            let pythonpath = self.pythonpath(found.prefix)?;
            let (target, mut import_paths) =
                python::launch(&from_command[0], found.args, &self.place, &mut self.parts)?;
            if let Some(value) = pythonpath {
                import_paths.extend(pythonpath_entries(&self.place, &value)?);
            }
            return Ok(Launch {
                target,
                import_paths: deduplicated(import_paths),
            });
        }
        self.parts.irrelevant(found.prefix, ENVIRONMENT);
        if found.name == "cd" {
            return Err(starts_no_code());
        }
        if is_shell(found.name) && found.args.first().is_some_and(|flag| flag.text == "-c") {
            self.parts.add(
                from_command,
                "Runs a quoted shell command. Unfold doesn't look inside it",
            );
            return Err(LaunchProblem {
                label: format!("Can't read {} -c", found.name).into(),
                message: "Unfold doesn't look inside a quoted shell command, so nothing is read from it."
                    .into(),
            });
        }
        let package = self.package();
        let reference = self
            .interpreter
            .reference(found.name, found.args, &package, &self.place.cwd)?;
        if let Some(reference) = reference {
            let Some(launch) = self.interpreter.follow(&reference, 0)? else {
                return Err(starts_no_code());
            };
            let target = file(self.place.root, &launch.cwd, &launch.path)?;
            self.parts.add(
                from_command,
                format!(
                    "Runs the package script {}, which starts {}",
                    reference.name.text, launch.path
                ),
            );
            return Ok(Launch {
                target,
                import_paths: Vec::new(),
            });
        }
        if LAUNCHERS.contains(&found.name) {
            let Step::Launch(launch, at) = launcher(found.name, found.args, &self.place.cwd)? else {
                return Err(starts_no_code());
            };
            let target = file(self.place.root, &launch.cwd, &launch.path)?;
            let program = match found.name {
                "node" => "Node",
                "bun" => "Bun",
                other => other,
            };
            self.parts.add(&from_command[..=at], format!("Runs {program}"));
            self.parts
                .add(&found.args[at..=at], format!("Runs the file {}", launch.path));
            self.parts.irrelevant(&found.args[at + 1..], PROGRAM_ARGUMENTS);
            return Ok(Launch {
                target,
                import_paths: Vec::new(),
            });
        }
        Err(LaunchProblem {
            label: format!("Can't read {}", found.name).into(),
            message: format!(
                "Unfold doesn't know what {} starts. Set the target by hand to say what it runs.",
                found.name
            )
            .into(),
        })
    }

    /// The `PYTHONPATH` a Python step sees: its own last assignment, else the declared one.
    /// Adds the step's assignments to the parts.
    fn pythonpath(&mut self, prefix: &[Word]) -> Result<Option<String>, Refusal> {
        let mut value = None;
        for word in prefix {
            match word.text.strip_prefix("PYTHONPATH=") {
                Some(assigned) => {
                    if word.expansion {
                        return Err(Refusal::new("Shell expansion in PYTHONPATH"));
                    }
                    self.parts.add(
                        std::slice::from_ref(word),
                        format!("Adds {assigned} to the import path"),
                    );
                    value = Some(assigned.to_owned());
                }
                _ => self.parts.irrelevant(std::slice::from_ref(word), ENVIRONMENT),
            }
        }
        match value {
            // Python ignores an empty `PYTHONPATH`.
            Some(value) => Ok((!value.is_empty()).then_some(value)),
            None => Ok(declared_pythonpath(self.env)?.map(str::to_owned)),
        }
    }

    /// The manifest a package-manager command in the working folder selects.
    fn package(&self) -> String {
        self.interpreter
            .relative(&resolve(&self.place.cwd, "package.json"))
    }
}

/// Where a step runs: the open folder and the working directory, which `cd` may have made
/// absolute or moved outside the project.
pub(super) struct Place<'a> {
    root: &'a str,
    cwd: String,
}

impl Place<'_> {
    /// The project folder `directory` names from the working directory; empty is the working
    /// directory itself.
    pub(super) fn folder(&self, directory: &str) -> Result<FolderPath, Refusal> {
        let path = resolve(&self.cwd, if directory.is_empty() { "." } else { directory });
        let relative = match path.strip_prefix('/') {
            None => Some(path.as_str()),
            Some(_) => path.strip_prefix(self.root).and_then(|rest| match rest {
                "" => Some("."),
                rest => rest.strip_prefix('/'),
            }),
        };
        relative
            .and_then(|relative| FolderPath::parse(relative).ok())
            .ok_or_else(|| Refusal::new(format!("Folder {path} is outside the project")))
    }

    /// The project file `path` names from the working directory.
    pub(super) fn file(&self, path: &str) -> Result<RepoPath, Refusal> {
        launch_file(self.root, &self.cwd, path)
            .ok_or_else(|| Refusal::new(format!("{path} is outside the project")))
    }
}

/// The file target a launch names.
fn file(root: &str, cwd: &str, path: &str) -> Result<LaunchTarget, Refusal> {
    let place = Place {
        root,
        cwd: cwd.to_owned(),
    };
    Ok(LaunchTarget::File {
        path: place.file(path)?,
    })
}

/// A command's parts, merging neighboring irrelevant runs with the same meaning.
#[derive(Default)]
pub(super) struct Parts(Vec<CommandPart>);

impl Parts {
    /// Adds a part for `words`.
    pub(super) fn add<'w>(
        &mut self,
        words: impl IntoIterator<Item = &'w Word>,
        meaning: impl Into<Box<str>>,
    ) {
        let text = written(words);
        if !text.is_empty() {
            self.0.push(CommandPart {
                text: text.into(),
                meaning: meaning.into(),
                irrelevant: false,
            });
        }
    }

    /// Adds an irrelevant part for `words`, joining the previous part if it means the same.
    pub(super) fn irrelevant<'w>(&mut self, words: impl IntoIterator<Item = &'w Word>, meaning: &str) {
        let text = written(words);
        if text.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some(last) if last.irrelevant && &*last.meaning == meaning => {
                last.text = format!("{} {text}", last.text).into();
            }
            _ => self.0.push(CommandPart {
                text: text.into(),
                meaning: meaning.into(),
                irrelevant: true,
            }),
        }
    }
}

/// Words as written, separated by spaces.
fn written<'w>(words: impl IntoIterator<Item = &'w Word>) -> String {
    words
        .into_iter()
        .map(|word| word.raw.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The import path a target set by hand implies when the command doesn't say: what Python
/// would use for that kind of launch from the working folder, then the declared `PYTHONPATH`.
fn target_paths(
    target: &LaunchTarget,
    draft: &LaunchDraft<'_>,
    root: &str,
) -> Result<Vec<ImportPath>, Refusal> {
    let cwd = draft.cwd.clone();
    let first = match target {
        LaunchTarget::File { path } if is_python_file(path) => ImportPath {
            path: script_folder(path),
            origin: ImportPathOrigin::ScriptFolder,
        },
        LaunchTarget::File { .. } => return Ok(Vec::new()),
        LaunchTarget::PythonModule { .. } => ImportPath {
            path: cwd,
            origin: ImportPathOrigin::ModuleWorkingFolder,
        },
        LaunchTarget::PythonCallable { .. } | LaunchTarget::PythonAsgi { .. } => ImportPath {
            path: cwd,
            origin: ImportPathOrigin::WorkingFolder,
        },
    };
    let mut paths = vec![first];
    let place = Place {
        root,
        cwd: draft.cwd.as_str().to_owned(),
    };
    if let Some(value) = declared_pythonpath(draft.env)? {
        paths.extend(pythonpath_entries(&place, value)?);
    }
    Ok(deduplicated(paths))
}

/// The declared `PYTHONPATH`, unless it is empty (Python ignores an empty one). The table is
/// never expanded, so a variable in it can't be read.
fn declared_pythonpath(env: &BTreeMap<String, String>) -> Result<Option<&str>, Refusal> {
    match env.get("PYTHONPATH").map(String::as_str) {
        Some(value) if value.contains('$') => Err(Refusal::new("Shell expansion in declared PYTHONPATH")),
        value => Ok(value.filter(|value| !value.is_empty())),
    }
}

/// The folders a `PYTHONPATH` value names, resolved from the working directory; an empty entry
/// is the working directory.
fn pythonpath_entries(place: &Place<'_>, value: &str) -> Result<Vec<ImportPath>, Refusal> {
    value
        .split(':')
        .map(|entry| {
            Ok(ImportPath {
                path: place.folder(entry)?,
                origin: ImportPathOrigin::PythonPath,
            })
        })
        .collect()
}

fn set_by_hand(paths: &[FolderPath]) -> Vec<ImportPath> {
    deduplicated(
        paths
            .iter()
            .map(|path| ImportPath {
                path: path.clone(),
                origin: ImportPathOrigin::SetByHand,
            })
            .collect(),
    )
}

/// `paths` without later repeats of a folder.
fn deduplicated(paths: Vec<ImportPath>) -> Vec<ImportPath> {
    let mut kept: Vec<ImportPath> = Vec::with_capacity(paths.len());
    for path in paths {
        if !kept.iter().any(|seen| seen.path == path.path) {
            kept.push(path);
        }
    }
    kept
}

/// The folder holding a script, which Python puts first on the import path.
pub(super) fn script_folder(path: &RepoPath) -> FolderPath {
    FolderPath::parse(&super::dirname(path.as_str()))
        .expect("the folder of a path inside the project is a folder path")
}

fn is_python_file(path: &RepoPath) -> bool {
    path.as_str().ends_with(".py")
}

fn is_shell(name: &str) -> bool {
    name == "sh" || name == "bash"
}

/// How a target reads in a message.
fn describe(target: &LaunchTarget) -> String {
    match target {
        LaunchTarget::File { path } => path.to_string(),
        LaunchTarget::PythonModule { module } => format!("the module {module}"),
        LaunchTarget::PythonCallable { module, attribute } => format!("{module}:{attribute}"),
        LaunchTarget::PythonAsgi {
            module,
            attribute,
            factory,
        } => {
            let factory = if *factory { " as a factory" } else { "" };
            format!("{module}:{attribute}{factory}")
        }
    }
}

// What the parts and problems say.

/// The label of a command outside the grammar.
const CANT_READ_COMMAND: &str = "Can't read this command";
pub(super) const RUNS_BEFORE: &str = "Runs before the launch. It doesn't change the code";
pub(super) const ENVIRONMENT: &str = "Sets the environment. It doesn't change the code";
pub(super) const SERVER_OPTIONS: &str = "Server options. They don't change the code";
pub(super) const PROGRAM_ARGUMENTS: &str = "Arguments for the program. They don't change the code";

impl From<Refusal> for LaunchProblem {
    fn from(refusal: Refusal) -> Self {
        Self {
            label: CANT_READ_COMMAND.into(),
            message: refusal.message().into(),
        }
    }
}

fn nothing_to_start() -> LaunchProblem {
    LaunchProblem {
        label: "Nothing to start".into(),
        message: "Write the command that starts this process, or set its target by hand.".into(),
    }
}

fn starts_no_code() -> LaunchProblem {
    LaunchProblem {
        label: "Starts no code".into(),
        message: "The command's last step doesn't start a file or module.".into(),
    }
}

fn disagree(command: &LaunchTarget, target: &LaunchTarget) -> LaunchProblem {
    LaunchProblem {
        label: "Command and target disagree".into(),
        message: format!(
            "The command starts {}, but the target says {}.",
            describe(command),
            describe(target)
        )
        .into(),
    }
}

#[cfg(test)]
mod tests;
