//! The little of a Dockerfile the import reads: the final stage's `WORKDIR`, its `COPY`s into the
//! working folder, `ENV PYTHONPATH`, and `CMD`/`ENTRYPOINT` in exec (JSON array) or shell form.
//! Lines ending in `\` continue on the next; comment lines are skipped, also inside a
//! continuation. Nothing else (build arguments, variable substitution, other stages) is read.

use std::collections::BTreeMap;

/// What the final stage of a Dockerfile says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Dockerfile {
    /// The working folder in the image, absolute; `None` when no `WORKDIR` is set.
    workdir: Option<String>,
    /// Every single-source local `COPY` as (source, destination), in order.
    copies: Vec<(String, String)>,
    /// The last `ENV PYTHONPATH`.
    python_path: Option<String>,
    entrypoint: Option<Form>,
    cmd: Option<Form>,
}

/// How `CMD` or `ENTRYPOINT` is written.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Form {
    /// `["sh", "-c", "…"]`.
    Exec(Vec<String>),
    /// `python app.py`.
    Shell(String),
}

/// The command a Dockerfile starts, and the instructions it came from (`CMD`, `ENTRYPOINT`, or
/// `ENTRYPOINT and CMD`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartCommand {
    pub command: String,
    pub source: &'static str,
}

impl Dockerfile {
    /// Reads `text`.
    pub fn parse(text: &str) -> Self {
        let mut file = Self::default();
        for line in logical_lines(text) {
            let (instruction, rest) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
            let rest = rest.trim();
            match instruction.to_ascii_uppercase().as_str() {
                // Each stage starts over; only the last one runs.
                "FROM" => file = Self::default(),
                "WORKDIR" if !rest.is_empty() => {
                    let base = file.workdir.as_deref().unwrap_or("/");
                    file.workdir = Some(absolute(base, rest));
                }
                "COPY" => {
                    if let Some(copy) = copy_args(rest) {
                        file.copies.push(copy);
                    }
                }
                "ENV" => {
                    if let Some(value) = env_value(rest, "PYTHONPATH") {
                        file.python_path = Some(value);
                    }
                }
                "CMD" => file.cmd = Some(Form::parse(rest)),
                "ENTRYPOINT" => file.entrypoint = Some(Form::parse(rest)),
                _ => {}
            }
        }
        file
    }

    /// The command the image starts: `ENTRYPOINT` then `CMD`, exec words joined with POSIX shell
    /// quoting. A shell-form `ENTRYPOINT` ignores `CMD`, as Docker does.
    pub fn start_command(&self) -> Option<StartCommand> {
        let (command, source) = match (&self.entrypoint, &self.cmd) {
            (None, None) => return None,
            (None, Some(cmd)) => (cmd.text(), "CMD"),
            (Some(entrypoint @ Form::Shell(_)), _) | (Some(entrypoint), None) => {
                (entrypoint.text(), "ENTRYPOINT")
            }
            (Some(entrypoint), Some(cmd)) => (
                format!("{} {}", entrypoint.text(), cmd.text()),
                "ENTRYPOINT and CMD",
            ),
        };
        Some(StartCommand { command, source })
    }

    /// The repository folder that becomes `WORKDIR`, with `context` the build context. A
    /// `COPY <dir> .` into the working folder names it; failing that, a `COPY a/b/x ./x` names
    /// `a/b`. Copies of files (`is_file`) and globs say nothing. When no copy says, or they
    /// disagree, it is the build context.
    pub fn working_folder(&self, context: &str, is_file: impl Fn(&str) -> bool) -> String {
        let Some(workdir) = self.workdir.as_deref() else {
            return context.to_owned();
        };
        let mut whole = Vec::new();
        let mut beside = Vec::new();
        for (source, destination) in &self.copies {
            let Some(source) = repo_path(context, source) else {
                continue;
            };
            if is_file(&source) {
                continue;
            }
            let destination = absolute(workdir, destination);
            if destination == workdir {
                whole.push(source);
            } else if parent(&destination) == workdir
                && base_name(&destination) == base_name(&source)
                && source != "."
            {
                beside.push(parent(&source).to_owned());
            }
        }
        let candidates = if whole.is_empty() { beside } else { whole };
        match candidates.split_first() {
            Some((first, rest)) if rest.iter().all(|other| other == first) => first.clone(),
            _ => context.to_owned(),
        }
    }

    /// `PYTHONPATH` relative to the working folder: the entries that are `WORKDIR` (`.`) or
    /// under it, in order. Entries elsewhere in the image are dropped.
    pub fn env(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        let (Some(workdir), Some(value)) = (self.workdir.as_deref(), self.python_path.as_deref()) else {
            return env;
        };
        let mut entries: Vec<&str> = Vec::new();
        for entry in value.split(':').map(|entry| entry.trim_end_matches('/')) {
            let relative = if entry == workdir {
                Some(".")
            } else {
                entry
                    .strip_prefix(workdir)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .filter(|rest| !rest.is_empty())
            };
            if let Some(relative) = relative
                && !entries.contains(&relative)
            {
                entries.push(relative);
            }
        }
        if !entries.is_empty() {
            env.insert("PYTHONPATH".to_owned(), entries.join(":"));
        }
        env
    }
}

impl Form {
    fn parse(rest: &str) -> Self {
        if rest.starts_with('[')
            && let Ok(words) = serde_json::from_str::<Vec<String>>(rest)
        {
            return Self::Exec(words);
        }
        Self::Shell(rest.to_owned())
    }

    fn text(&self) -> String {
        match self {
            Self::Exec(words) => shell_join(words),
            Self::Shell(text) => text.clone(),
        }
    }
}

/// The instructions of `text`, one per line, continuations joined and comments dropped.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || (trimmed.is_empty() && !current.is_empty()) {
            continue;
        }
        match trimmed.strip_suffix('\\') {
            Some(start) => {
                current.push_str(start);
                current.push(' ');
            }
            None => {
                current.push_str(trimmed);
                let line = std::mem::take(&mut current);
                if !line.trim().is_empty() {
                    lines.push(line.trim().to_owned());
                }
            }
        }
    }
    if !current.trim().is_empty() {
        lines.push(current.trim().to_owned());
    }
    lines
}

/// A `COPY` with one local source: (source, destination). Copies from another stage or image
/// (`--from`), with several sources, or of globs are `None`.
fn copy_args(rest: &str) -> Option<(String, String)> {
    let words: Vec<String> = if rest.starts_with('[') {
        serde_json::from_str(rest).ok()?
    } else {
        rest.split_whitespace().map(str::to_owned).collect()
    };
    let mut paths = Vec::new();
    for word in words {
        if word.starts_with("--from") {
            return None;
        }
        if !word.starts_with("--") {
            paths.push(word);
        }
    }
    let [source, destination] = <[String; 2]>::try_from(paths).ok()?;
    if source.contains(['*', '?', '[']) {
        return None;
    }
    Some((source, destination))
}

/// The value `ENV` gives `name`, in either form: `ENV NAME=value OTHER="x y"` or `ENV NAME value`.
fn env_value(rest: &str, name: &str) -> Option<String> {
    let words = shell_words(rest);
    if !words.first().is_some_and(|word| word.contains('=')) {
        // `ENV NAME value`: the rest of the line is the value.
        let value = rest.strip_prefix(name)?;
        return value
            .starts_with(char::is_whitespace)
            .then(|| shell_words(value).join(" "));
    }
    words.iter().rev().find_map(|word| {
        let (key, value) = word.split_once('=')?;
        (key == name).then(|| value.to_owned())
    })
}

/// Splits on unquoted whitespace and removes double and single quotes.
fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (Some(open), _) if c == open => quote = None,
            (None | Some('"'), '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
                started = true;
            }
            (None, _) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Joins exec-form words into one POSIX shell command. A word of only safe characters stays
/// bare; one with whitespace and no `"`, `\`, or backtick goes in double quotes (as people write
/// `sh -c "…"`); any other goes in single quotes.
pub(super) fn shell_join(words: &[String]) -> String {
    words.iter().map(|word| quote(word)).collect::<Vec<_>>().join(" ")
}

fn quote(word: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !word.is_empty() && word.chars().all(safe) {
        word.to_owned()
    } else if word.contains(char::is_whitespace) && !word.contains(['"', '\\', '`']) {
        format!("\"{word}\"")
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// `path` inside the image, absolute: relative to `base` unless it starts with `/`; `.` and `..`
/// resolved, no trailing `/`.
fn absolute(base: &str, path: &str) -> String {
    let joined = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("{base}/{path}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

/// A repository path from `base` (a repository folder, `.` for the root) and `path` relative to
/// it (a leading `/` or `./` is dropped): `/`-separated, `.` for the root, `None` when it leaves
/// the repository.
pub(super) fn repo_path(base: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in base.split('/').chain(path.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    })
}

/// The folder holding `path` (`.` for a top-level repository path, `/` for a top-level image
/// path).
pub(super) fn parent(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) => "/",
        Some((parent, _)) => parent,
        None => ".",
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DASHBOARD: &str = r#"# Analyst sandbox
FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PYTHONPATH=/app \
    DASHBOARD_BUILDER_ENV=production

WORKDIR /app

COPY data-pipelines/requirements-dashboard-builder.txt ./requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY data-pipelines/dashboard_builder ./dashboard_builder

# Web service: uvicorn on $PORT.
CMD ["sh", "-c", "uvicorn dashboard_builder.app:app --host 0.0.0.0 --port ${PORT:-8010}"]
"#;

    fn no_files(_: &str) -> bool {
        false
    }

    #[test]
    fn reads_the_final_stage() {
        let file = Dockerfile::parse(DASHBOARD);
        assert_eq!(file.workdir.as_deref(), Some("/app"));
        assert_eq!(file.python_path.as_deref(), Some("/app"));
        assert_eq!(
            file.copies,
            [
                (
                    "data-pipelines/requirements-dashboard-builder.txt".to_owned(),
                    "./requirements.txt".to_owned()
                ),
                (
                    "data-pipelines/dashboard_builder".to_owned(),
                    "./dashboard_builder".to_owned()
                ),
            ]
        );
        let start = file.start_command().expect("a command");
        assert_eq!(
            start.command,
            r#"sh -c "uvicorn dashboard_builder.app:app --host 0.0.0.0 --port ${PORT:-8010}""#
        );
        assert_eq!(start.source, "CMD");

        let staged = Dockerfile::parse(
            "FROM node AS build\nWORKDIR /src\nCMD [\"npm\", \"run\", \"build\"]\n\nFROM python\nWORKDIR srv\nWORKDIR api\nCOPY --from=build /src/dist ./dist\nCMD python main.py\n",
        );
        assert_eq!(staged.workdir.as_deref(), Some("/srv/api"));
        assert!(staged.copies.is_empty(), "copies from another stage say nothing");
        assert_eq!(
            staged.start_command().map(|start| start.command).as_deref(),
            Some("python main.py")
        );
    }

    #[test]
    fn joins_continuations_and_skips_comments_inside_them() {
        let file = Dockerfile::parse(
            "FROM x\nCMD [\"python3\", \\\n# the script\n     \"review-workers/run_worker.py\"]\n",
        );
        assert_eq!(
            file.start_command().map(|start| start.command).as_deref(),
            Some("python3 review-workers/run_worker.py")
        );
    }

    #[test]
    fn combines_entrypoint_and_cmd() {
        let both = Dockerfile::parse("ENTRYPOINT [\"python\", \"-m\"]\nCMD [\"app.main\"]\n");
        assert_eq!(
            both.start_command(),
            Some(StartCommand {
                command: "python -m app.main".to_owned(),
                source: "ENTRYPOINT and CMD"
            })
        );
        let shell = Dockerfile::parse("ENTRYPOINT ./run.sh --serve\nCMD [\"ignored\"]\n");
        assert_eq!(
            shell.start_command(),
            Some(StartCommand {
                command: "./run.sh --serve".to_owned(),
                source: "ENTRYPOINT"
            })
        );
        assert_eq!(Dockerfile::parse("FROM x\nRUN true\n").start_command(), None);
    }

    #[test]
    fn quotes_exec_words_for_a_posix_shell() {
        let join = |words: &[&str]| shell_join(&words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>());
        assert_eq!(
            join(&["uvicorn", "api.main:app", "--port", "8000"]),
            "uvicorn api.main:app --port 8000"
        );
        assert_eq!(join(&["sh", "-c", "a b $X"]), r#"sh -c "a b $X""#);
        assert_eq!(join(&["echo", "$PORT"]), "echo '$PORT'");
        assert_eq!(join(&["echo", r#"say "hi" now"#]), r#"echo 'say "hi" now'"#);
        assert_eq!(join(&["echo", "it's"]), r"echo 'it'\''s'");
        assert_eq!(join(&["echo", ""]), "echo ''");
    }

    #[test]
    fn the_working_folder_comes_from_copies_into_workdir() {
        let workers = Dockerfile::parse(
            "WORKDIR /app\nCOPY data-pipelines/requirements-api.txt ./requirements.txt\nCOPY data-pipelines .\nCOPY tests ./tests\n",
        );
        assert_eq!(workers.working_folder(".", no_files), "data-pipelines");

        let dashboard = Dockerfile::parse(DASHBOARD);
        assert_eq!(dashboard.working_folder(".", no_files), "data-pipelines");

        let absolute = Dockerfile::parse("WORKDIR /srv\nCOPY ./services/api/ /srv/\n");
        assert_eq!(absolute.working_folder(".", no_files), "services/api");

        let in_context = Dockerfile::parse("WORKDIR /app\nCOPY . .\n");
        assert_eq!(in_context.working_folder("backend", no_files), "backend");

        let file_copy = Dockerfile::parse("WORKDIR /app\nCOPY a/b/requirements.txt ./requirements.txt\n");
        let is_file = |path: &str| path == "a/b/requirements.txt";
        assert_eq!(
            file_copy.working_folder(".", is_file),
            ".",
            "a file copy says nothing"
        );

        let disagree = Dockerfile::parse("WORKDIR /app\nCOPY a/x ./x\nCOPY b/y ./y\n");
        assert_eq!(disagree.working_folder("ctx", no_files), "ctx");

        let no_workdir = Dockerfile::parse("COPY src .\n");
        assert_eq!(no_workdir.working_folder(".", no_files), ".");
    }

    #[test]
    fn python_path_is_relative_to_the_working_folder() {
        let env = |text: &str| Dockerfile::parse(text).env();
        assert_eq!(
            env(DASHBOARD),
            BTreeMap::from([("PYTHONPATH".to_owned(), ".".to_owned())])
        );
        assert_eq!(
            env("WORKDIR /app\nENV PYTHONPATH=/app/src:/opt/lib:$PYTHONPATH:/app/\n"),
            BTreeMap::from([("PYTHONPATH".to_owned(), "src:.".to_owned())])
        );
        assert_eq!(
            env("WORKDIR /app\nENV PYTHONPATH /app/src\n"),
            BTreeMap::from([("PYTHONPATH".to_owned(), "src".to_owned())])
        );
        assert!(env("WORKDIR /app\nENV PYTHONPATH=\"/opt/x\"\n").is_empty());
        assert!(
            env("ENV PYTHONPATH=/app\n").is_empty(),
            "no WORKDIR, nothing to be relative to"
        );
    }

    #[test]
    fn repository_paths_are_normalized() {
        assert_eq!(
            repo_path(".", "./data-pipelines/Dockerfile").as_deref(),
            Some("data-pipelines/Dockerfile")
        );
        assert_eq!(repo_path("a", "./").as_deref(), Some("a"));
        assert_eq!(repo_path(".", ".").as_deref(), Some("."));
        assert_eq!(repo_path("a", "../b").as_deref(), Some("b"));
        assert_eq!(repo_path(".", "../b"), None);
        assert_eq!(parent("data-pipelines/render.yaml"), "data-pipelines");
        assert_eq!(parent("render.yaml"), ".");
    }
}
