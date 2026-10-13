//! The launch-command grammar: what module a `package.json` script starts, read from its text
//! and never executed (docs/graph-model.md#value-policy-and-reason-order, `docs/engine.md`).
//!
//! A script is a POSIX shell subset of `&&`/`;` steps. Steps may set environment variables
//! (leading assignments, `env`, `cross-env`), change directory (`cd`), reference other scripts
//! (`npm run`, `npm --prefix … run`, `yarn`, `pnpm`, `bun run`; nested up to eight levels, cycles
//! refused), or launch a file (`node`, `tsx`, `ts-node`, `ts-node-esm`, `nodemon`, `bun`). The
//! last step decides the outcome: `npm run build && node dist/server.js` launches
//! `dist/server.js`, which the TypeScript emit map later traces back to its source.
//!
//! Everything here is pure: package contents come in through [`PackageScripts`], and the result
//! is a [`LaunchCommand`] (a path and the directory it is relative to) or a [`Refusal`] carrying
//! the proof of concept's reason.
//!
//! Configured process commands use the same grammar and package lookup through [`plan_launch`],
//! which also reads Python and Uvicorn launches and yields a [`LaunchPlan`].

mod launcher;
mod path;
mod plan;
mod python;
mod shell;
mod target;

use std::fmt;

use launcher::{LAUNCHERS, Step, launcher};
pub use path::{LAUNCH_EXTENSIONS, launch_file};
pub use plan::{
    CommandPart, ImportPath, ImportPathOrigin, LaunchDraft, LaunchPlan, LaunchProblem, plan_launch,
};
use shell::{Word, is_assignment, steps};
pub use target::{LaunchTarget, PythonContext, PythonEntry};

/// How deep script references may nest: eight references below the enumerated script.
pub const MAX_SCRIPT_DEPTH: usize = 8;

/// How many script references one enumerated script may interpret in all. Depth and cycle
/// checks don't bound fan-out (each step may reference scripts that reference several more),
/// so without a budget a small `package.json` could take exponential time.
pub const MAX_SCRIPT_REFERENCES: usize = 1024;

/// A module launch: `path` exactly as written in the script, relative to `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LaunchCommand {
    /// The module path as written (the startup target's text), such as `dist/server.js`.
    pub path: Box<str>,
    /// The working directory the path is relative to: normally a folder-relative directory
    /// (`.` for the folder itself, `apps/api`), but `cd` can make it absolute or lead outside
    /// the folder (`../outside`); resolution rejects paths that end up outside the folder.
    pub cwd: Box<str>,
}

/// What a script starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptOutcome {
    /// Its last step launches a module.
    Launch(LaunchCommand),
    /// It is understood, but its last step launches no module (`vite`, `tsc`, `node --test`).
    NoLaunch,
    /// It uses something outside the grammar; the script produces no entrance, only a note.
    Unsupported(Refusal),
}

/// Why a script is outside the grammar, worded as in the proof of concept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Refusal {
    message: Box<str>,
}

impl Refusal {
    pub(crate) fn new(message: impl Into<Box<str>>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The reason.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// A script looked up in a package manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLookup<'a> {
    /// No such `package.json` among the selected configuration inputs.
    NoPackage,
    /// The package has no `scripts` object, or no own member with this name.
    Missing,
    /// The member exists but isn't a string.
    NotString,
    /// The script's text.
    Script(&'a str),
}

/// The selected `package.json` inputs a script may reference. Script references can't discover
/// more packages: `npm --prefix dir run x` must name a package that is already an input.
pub trait PackageScripts {
    /// Script `name` of the package whose manifest is at the folder-relative path `package`
    /// (such as `package.json` or `apps/api/package.json`).
    fn script(&self, package: &str, name: &str) -> ScriptLookup<'_>;
}

/// Interprets script `name` of the manifest at `package` (folder-relative, such as
/// `apps/api/package.json`). `root` is the open folder's absolute path with `/` separators; it
/// maps absolute `cd` and `npm --prefix` directories back to folder-relative packages.
pub fn interpret_script(
    packages: &dyn PackageScripts,
    root: &str,
    package: &str,
    name: &str,
) -> ScriptOutcome {
    let mut interpreter = Interpreter {
        packages,
        root: root.trim_end_matches('/'),
        active: Vec::new(),
        interpreted: 0,
    };
    match interpreter.script(package, name, 0) {
        Ok(Some(launch)) => ScriptOutcome::Launch(launch),
        Ok(None) => ScriptOutcome::NoLaunch,
        Err(refusal) => ScriptOutcome::Unsupported(refusal),
    }
}

/// Words that are shell syntax when written plainly.
const CONTROL_WORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in",
    "function", "select", "time", "!", "{", "}",
];

/// Builtins whose effect on the shell can't be modeled, refused even when quoted.
const SHELL_BUILTINS: &[&str] = &[
    "exec", "eval", "source", ".", "command", "builtin", "exit", "return", "break", "continue", "trap",
    "pushd", "popd", "alias", "unalias", "enable",
];

struct Interpreter<'a> {
    packages: &'a dyn PackageScripts,
    root: &'a str,
    /// The `(package, script)` references being interpreted, for cycle detection.
    active: Vec<(String, String)>,
    /// Scripts interpreted so far, against [`MAX_SCRIPT_REFERENCES`].
    interpreted: usize,
}

/// A step's script reference: the package and the child script name.
struct Reference<'w> {
    package: String,
    name: &'w Word,
    rest: &'w [Word],
}

impl Interpreter<'_> {
    fn script(&mut self, package: &str, name: &str, depth: usize) -> Result<Option<LaunchCommand>, Refusal> {
        if depth > MAX_SCRIPT_DEPTH {
            return Err(Refusal::new(format!(
                "Script reference depth exceeds {MAX_SCRIPT_DEPTH}"
            )));
        }
        self.interpreted += 1;
        if self.interpreted > MAX_SCRIPT_REFERENCES {
            return Err(Refusal::new(format!(
                "Script references exceed {MAX_SCRIPT_REFERENCES}"
            )));
        }
        if self.active.iter().any(|(p, n)| p == package && n == name) {
            return Err(Refusal::new(format!("Cyclic script reference {package}: {name}")));
        }
        let text = match self.packages.script(package, name) {
            ScriptLookup::NoPackage => {
                return Err(Refusal::new(format!("Package input is not selected: {package}")));
            }
            ScriptLookup::Missing | ScriptLookup::NotString => {
                return Err(Refusal::new(format!(
                    "Missing or non-string script {package}: {name}"
                )));
            }
            ScriptLookup::Script(text) => text,
        };
        self.active.push((package.to_owned(), name.to_owned()));
        let result = self.steps(package, text, depth);
        self.active.pop();
        result
    }

    fn steps(&mut self, package: &str, text: &str, depth: usize) -> Result<Option<LaunchCommand>, Refusal> {
        let mut cwd = dirname(package);
        let mut result = None;
        for step in steps(text)? {
            let Some(command) = command(&step)? else {
                result = None;
                continue;
            };
            if command.name == "cd" {
                cwd = change_directory(&cwd, command.args)?;
                result = None;
                continue;
            }
            result = match self.reference(command.name, command.args, package, &cwd)? {
                Some(reference) => self.follow(&reference, depth)?,
                None if LAUNCHERS.contains(&command.name) => {
                    match launcher(command.name, command.args, &cwd)? {
                        Step::Launch(launch, _) => Some(launch),
                        Step::Other => None,
                    }
                }
                // Ordinary preparatory commands are ignored; their effects aren't modeled.
                None => None,
            };
        }
        Ok(result)
    }

    /// Interprets the script a reference names, one level below `depth`.
    fn follow(&mut self, reference: &Reference<'_>, depth: usize) -> Result<Option<LaunchCommand>, Refusal> {
        let child = reference.name.literal("script name")?;
        if child.starts_with('-') {
            return Err(Refusal::new("Script reference options are unsupported"));
        }
        if reference.rest.first().is_some_and(|word| word.text != "--") {
            return Err(Refusal::new("Script arguments require --"));
        }
        self.script(&reference.package, child, depth + 1)
    }

    /// Recognizes a package-manager script reference.
    fn reference<'w>(
        &self,
        command: &str,
        args: &'w [Word],
        package: &str,
        cwd: &str,
    ) -> Result<Option<Reference<'w>>, Refusal> {
        let found = |package: String, at: usize, missing: &str| -> Result<Option<Reference<'w>>, Refusal> {
            match args.get(at) {
                Some(name) => Ok(Some(Reference {
                    package,
                    name,
                    rest: &args[at + 1..],
                })),
                None => Err(Refusal::new(missing.to_owned())),
            }
        };
        match command {
            "npm" => {
                let mut selected = package.to_owned();
                let mut offset = 0;
                if let Some(first) = args.first()
                    && first.literal("npm command")? == "--prefix"
                {
                    let directory = args
                        .get(1)
                        .ok_or_else(|| Refusal::new("Missing npm --prefix directory"))?
                        .literal("npm --prefix path")?;
                    if directory.is_empty() {
                        return Err(Refusal::new("Unsupported npm --prefix directory"));
                    }
                    selected = self.relative(&resolve(&resolve(cwd, directory), "package.json"));
                    offset = 2;
                    if args.get(offset).is_none_or(|word| word.text != "run") {
                        return Err(Refusal::new("npm --prefix requires run"));
                    }
                }
                let Some(verb) = args.get(offset) else {
                    return Ok(None);
                };
                match verb.literal("npm command")? {
                    "run" | "run-script" => found(selected, offset + 1, "Missing npm script name"),
                    "start" | "stop" | "restart" | "test" => {
                        found(selected, offset, "Missing npm script name")
                    }
                    verb if verb.starts_with('-') => Err(Refusal::new("Unsupported npm option")),
                    _ => Ok(None),
                }
            }
            "yarn" | "pnpm" => {
                let offset = usize::from(args.first().is_some_and(|word| word.text == "run"));
                found(
                    package.to_owned(),
                    offset,
                    &format!("Missing {command} script name"),
                )
            }
            "bun" if args.first().is_some_and(|word| word.text == "run") => {
                found(package.to_owned(), 1, "Missing bun script name")
            }
            _ => Ok(None),
        }
    }

    /// Maps an absolute path inside the folder back to a folder-relative one.
    fn relative(&self, path: &str) -> String {
        if self.root.is_empty() {
            return path.strip_prefix('/').unwrap_or(path).to_owned();
        }
        match path
            .strip_prefix(self.root)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            _ => path.to_owned(),
        }
    }
}

/// A step's command: the word after its leading assignments and any `env`/`cross-env` wrapper.
struct Command<'w> {
    /// The command's name.
    name: &'w str,
    /// The words before it: assignments, and the wrapper with its own assignments.
    prefix: &'w [Word],
    /// The words after it.
    args: &'w [Word],
}

/// Finds a step's command, refusing shell syntax and builtins whose effect can't be modeled.
/// `None` for a step that only sets variables.
fn command(step: &[Word]) -> Result<Option<Command<'_>>, Refusal> {
    let mut index = step.iter().take_while(|word| word.assignment).count();
    let Some(first) = step.get(index) else {
        return Ok(None);
    };
    let mut name = first.literal("command")?;
    if first.plain && CONTROL_WORDS.contains(&name) {
        return Err(Refusal::new(format!("Unsupported shell control word {name}")));
    }
    if SHELL_BUILTINS.contains(&name) {
        return Err(Refusal::new(format!("Unsupported shell builtin {name}")));
    }
    if name == "env" || name == "cross-env" {
        index += 1;
        index += step[index..]
            .iter()
            .take_while(|word| is_assignment(&word.text))
            .count();
        let Some(wrapped) = step.get(index) else {
            return Ok(None);
        };
        name = wrapped.literal("command")?;
        if name.starts_with('-') {
            return Err(Refusal::new("Environment wrapper options are unsupported"));
        }
        if name == "cd" || SHELL_BUILTINS.contains(&name) {
            return Err(Refusal::new(format!(
                "Environment wrapper cannot model shell builtin {name}"
            )));
        }
    }
    Ok(Some(Command {
        name,
        prefix: &step[..index],
        args: &step[index + 1..],
    }))
}

/// The working directory after `cd` with `args` from `cwd`.
fn change_directory(cwd: &str, args: &[Word]) -> Result<String, Refusal> {
    let [directory] = args else {
        return Err(Refusal::new("cd requires one literal directory"));
    };
    let directory = directory.literal("cd path")?;
    if directory.is_empty() || directory.starts_with('-') {
        return Err(Refusal::new("Unsupported cd directory"));
    }
    Ok(resolve(cwd, directory))
}

/// The directory of a `/`-separated path (`.` for a bare name).
fn dirname(path: &str) -> String {
    match path.rsplit_once('/') {
        Some(("", _)) => "/".to_owned(),
        Some((parent, _)) => normalize(parent),
        None => ".".to_owned(),
    }
}

/// `path` resolved against `base` (unless absolute), then normalized.
fn resolve(base: &str, path: &str) -> String {
    if path.starts_with('/') {
        normalize(path)
    } else {
        normalize(&format!("{base}/{path}"))
    }
}

/// POSIX lexical normalization: drops empty and `.` components and folds `..` (keeping leading
/// `..` in relative paths). The empty relative path is `.`; trailing slashes are dropped.
fn normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

impl fmt::Display for LaunchCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (from {})", self.path, self.cwd)
    }
}

#[cfg(test)]
mod tests;
