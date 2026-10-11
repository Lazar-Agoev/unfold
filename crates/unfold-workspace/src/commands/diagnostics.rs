//! `diagnostics`: the redacted summary **Copy Diagnostics** puts on the clipboard for bug reports
//! (docs/architecture.md#security).
//!
//! The summary is built only from versions, platform names, vocabulary words (languages, file
//! states, detector IDs, problem kinds, watch modes), and counts. It never holds source, file
//! contents, or paths: not even folder-relative ones, which name a person's code. As a second
//! line of defense, the finished text goes through a [`Redactor`] that replaces the project's
//! path, the home folder, and the username with placeholders, should any of them ever slip in.
//! The app part is always there; the project part covers the project named, in full while it is
//! live and only its last known state while it rests. The UI adds what only it knows (the app,
//! Electron, and Chromium versions).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::FileState;

use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::options::default_languages;
use crate::problems::ProblemCategory;
use crate::projects::ProjectId;
use crate::status::{GraphState, ProjectState, WatchMode};
use crate::workspace::Workspace;

/// What `diagnostics` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// The summary, one fact per line, already redacted.
    pub text: String,
}

/// The arguments of `diagnostics`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsArgs {
    /// The project to describe as well, if any.
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<ProjectId>,
}

/// `diagnostics`: versions, platform, and, for the project named, counts and states (files by
/// language and state, graph completeness, the last update's statistics, problems by kind,
/// watching, fallback location, lock; only its last known state while it rests), with no source
/// and no paths. Never wakes the project.
pub fn diagnostics(ctx: &Ctx<'_>, args: DiagnosticsArgs) -> CommandResult<DiagnosticsReport> {
    let mut text = String::new();
    write_app(&mut text);
    let mut redactor = Redactor::default();
    match &args.project {
        Some(id) => {
            let project = ctx.project(id)?;
            redactor.path(&project.root().to_string_lossy(), "<folder>");
            match project.live() {
                Some(workspace) => write_project(&mut text, &workspace)?,
                None => write_resting(&mut text, &project.state()),
            }
        }
        None => text.push_str("\nProject: none\n"),
    }
    if let Some(home) = home_dir() {
        redactor.path(&home, "<home>");
    }
    if let Some(user) = username() {
        redactor.word(&user, "<user>");
    }
    Ok(DiagnosticsReport {
        text: redactor.apply(&text),
    })
}

/// The username from the environment, as `app_info` reports it.
pub(crate) fn username() -> Option<String> {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.trim().is_empty()))
}

fn home_dir() -> Option<String> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| value.len() > 1))
}

fn write_app(text: &mut String) {
    let mut producers: Vec<String> = default_languages()
        .iter()
        .map(|language| {
            let producer = language.producer();
            format!("{}={}@{}", language.id(), producer.name, producer.version)
        })
        .collect();
    producers.sort();
    let _ = writeln!(text, "Unfold diagnostics");
    let _ = writeln!(text, "Engine build: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(text, "Engine version: {}", unfold_engine::ENGINE_VERSION);
    let _ = writeln!(text, "Producers: {}", producers.join(", "));
    let _ = writeln!(
        text,
        "OS: {} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    );
}

/// A resting project's part: only the state it had when it went to rest (its kind: a failure's
/// message could name a path).
fn write_resting(text: &mut String, state: &ProjectState) {
    let _ = writeln!(text, "\nProject (resting)");
    let line = match state {
        ProjectState::Resting { last, .. } => format!("last known state {}", project_state_name(last)),
        other => project_state_name(other).to_owned(),
    };
    let _ = writeln!(text, "  State: {line}");
}

/// A row state's kind in words.
fn project_state_name(state: &ProjectState) -> &'static str {
    match state {
        ProjectState::Current => "current",
        ProjectState::Compiling { .. } => "compiling",
        ProjectState::Updating { .. } => "updating",
        ProjectState::Problems { .. } => "problems",
        ProjectState::Paused { .. } => "background updates paused",
        ProjectState::NotCompiled => "not compiled",
        ProjectState::Failed { .. } => "last compile failed",
        ProjectState::Resting { .. } => "resting",
        ProjectState::Unavailable { .. } => "unavailable",
    }
}

fn write_project(text: &mut String, workspace: &Workspace) -> CommandResult<()> {
    let status = workspace.status();
    let layout = workspace.layout();
    let _ = writeln!(text, "\nProject");
    let _ = writeln!(
        text,
        "  Graph: {} · {}",
        state_name(&status.state),
        if status.complete { "complete" } else { "incomplete" }
    );
    match workspace.readers() {
        Ok(pool) => {
            let rows = pool.get()?.files()?;
            let mut languages: BTreeMap<String, u32> = BTreeMap::new();
            let mut states: BTreeMap<String, u32> = BTreeMap::new();
            for row in &rows {
                *languages.entry(row.language.to_string()).or_default() += 1;
                *states.entry(file_state_name(row.state).to_owned()).or_default() += 1;
            }
            let _ = writeln!(text, "  Files: {}", rows.len());
            let _ = writeln!(text, "  Files by language: {}", counts(&languages));
            let _ = writeln!(text, "  File states: {}", counts(&states));
        }
        Err(_) => {
            let _ = writeln!(text, "  Files: not compiled");
        }
    }
    let _ = writeln!(
        text,
        "  Background updates: {}",
        if status.background_updates { "on" } else { "paused" }
    );
    let _ = writeln!(text, "  Watching: {}", watch_name(status.watching));
    let _ = writeln!(
        text,
        "  Fallback location in use: {}",
        if status.fallback.is_some() { "yes" } else { "no" }
    );
    let _ = writeln!(
        text,
        "  Folder lock: {}",
        if layout.is_compiled() {
            "held by this app"
        } else {
            "not taken (never compiled here)"
        }
    );
    write_settings(text, workspace)?;
    write_problems(text, workspace);
    write_last_update(text, workspace);
    Ok(())
}

fn write_settings(text: &mut String, workspace: &Workspace) -> CommandResult<()> {
    let exists = workspace.settings_text()?.is_some();
    let line = match workspace.settings() {
        Ok(settings) => {
            let disabled: Vec<String> = settings
                .disabled_detectors
                .iter()
                .map(ToString::to_string)
                .collect();
            format!(
                "{} · {} manual entrances · {} Python projects · {} include and {} exclude patterns · detectors off: {}",
                if exists { "valid" } else { "defaults (no file)" },
                settings.entrances.len(),
                settings.python_projects.len(),
                settings.scope.include().len(),
                settings.scope.exclude().len(),
                if disabled.is_empty() {
                    "none".to_owned()
                } else {
                    disabled.join(", ")
                }
            )
        }
        Err(crate::error::WorkspaceError::InvalidSettings {
            line: Some(line),
            column: Some(column),
            ..
        }) => format!("invalid at line {line}, column {column}"),
        Err(_) => "invalid".to_owned(),
    };
    let _ = writeln!(text, "  Settings: {line}");
    Ok(())
}

fn write_problems(text: &mut String, workspace: &Workspace) {
    let problems = workspace.problems();
    let mut by_kind: BTreeMap<String, u32> = BTreeMap::new();
    for problem in &problems {
        *by_kind
            .entry(category_name(problem.category).to_owned())
            .or_default() += 1;
    }
    let _ = writeln!(
        text,
        "  Problems: {}{}",
        problems.len(),
        if by_kind.is_empty() {
            String::new()
        } else {
            format!(" ({})", counts(&by_kind))
        }
    );
}

fn write_last_update(text: &mut String, workspace: &Workspace) {
    let log = workspace.index_log();
    let Some(last) = log.last() else {
        let _ = writeln!(text, "  Last update: none this session");
        return;
    };
    let _ = writeln!(
        text,
        "  Last update: {} · {} files · update {} ms · commit {} ms · wrote {} files and {} edges · removed {} files",
        last.action,
        last.files,
        last.update_ms,
        last.commit_ms,
        last.files_written,
        last.edges_written,
        last.files_removed
    );
    let _ = writeln!(text, "  Updates this session: {}", log.len());
    // The engine's statistics are counts by vocabulary word, one fact per line.
    for line in last.text.lines().filter(|line| !line.trim().is_empty()) {
        let _ = writeln!(text, "    {line}");
    }
}

fn counts(map: &BTreeMap<String, u32>) -> String {
    if map.is_empty() {
        return "none".to_owned();
    }
    map.iter()
        .map(|(name, count)| format!("{name} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The state's kind only: a failure's message could name a path.
fn state_name(state: &GraphState) -> &'static str {
    match state {
        GraphState::NotCompiled => "not compiled",
        GraphState::Current { .. } => "current",
        GraphState::Compiling { .. } => "compiling",
        GraphState::Updating { .. } => "updating",
        GraphState::DoesntParse { .. } => "some files don't parse",
        GraphState::Paused { .. } => "background updates paused",
        GraphState::Failed { .. } => "last compile failed",
    }
}

fn file_state_name(state: FileState) -> &'static str {
    match state {
        FileState::Current => "current",
        FileState::LastGood => "last_good",
        FileState::Failed => "failed",
    }
}

fn watch_name(mode: WatchMode) -> &'static str {
    match mode {
        WatchMode::Events => "events",
        WatchMode::Scanning => "scan",
        WatchMode::Off => "off",
    }
}

fn category_name(category: ProblemCategory) -> &'static str {
    match category {
        ProblemCategory::Settings => "settings",
        ProblemCategory::CompileFailed => "compile_failed",
        ProblemCategory::DoesntParse => "doesnt_parse",
        ProblemCategory::CantRead => "cant_read",
        ProblemCategory::Discovery => "discovery",
        ProblemCategory::Input => "input",
        ProblemCategory::Cache => "cache",
        ProblemCategory::Board => "board",
    }
}

/// Replaces private strings with placeholders: paths (and their other-separator spelling)
/// anywhere, and words (the username) only where they stand alone.
#[derive(Debug, Default)]
pub(crate) struct Redactor {
    paths: Vec<(String, &'static str)>,
    words: Vec<(String, &'static str)>,
}

impl Redactor {
    /// The shortest username that is redacted: shorter ones would hit ordinary words.
    const MIN_WORD: usize = 3;

    /// Redacts `path` wherever it appears, spelled with `/` or `\`.
    pub(crate) fn path(&mut self, path: &str, placeholder: &'static str) {
        let trimmed = path.trim_end_matches(['/', '\\']);
        if trimmed.len() < 2 {
            return;
        }
        for spelling in [trimmed.replace('\\', "/"), trimmed.replace('/', "\\")] {
            if !self.paths.iter().any(|(known, _)| *known == spelling) {
                self.paths.push((spelling, placeholder));
            }
        }
    }

    /// Redacts `word` where it isn't part of a longer word.
    pub(crate) fn word(&mut self, word: &str, placeholder: &'static str) {
        if word.chars().count() >= Self::MIN_WORD {
            self.words.push((word.to_owned(), placeholder));
        }
    }

    /// The text with every registered string replaced; longer paths first, so a folder inside
    /// the home folder becomes `<folder>`, not `<home>/…`.
    pub(crate) fn apply(&self, text: &str) -> String {
        let mut paths: Vec<&(String, &'static str)> = self.paths.iter().collect();
        paths.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        let mut out = text.to_owned();
        for (path, placeholder) in paths {
            out = out.replace(path.as_str(), placeholder);
        }
        for (word, placeholder) in &self.words {
            out = replace_word(&out, word, placeholder);
        }
        out
    }
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
}

/// Replaces standalone occurrences of `word` (case-insensitively for ASCII).
fn replace_word(text: &str, word: &str, placeholder: &str) -> String {
    let lower_text = text.to_ascii_lowercase();
    let lower_word = word.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = lower_text[from..].find(&lower_word) {
        let start = from + found;
        let end = start + lower_word.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char) {
            out.push_str(&text[copied..start]);
            out.push_str(placeholder);
            copied = end;
        }
        from = end;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_paths_longest_first_and_usernames_as_words() {
        let mut redactor = Redactor::default();
        redactor.path("/Users/ada", "<home>");
        redactor.path("/Users/ada/code/orders-service/", "<folder>");
        redactor.word("ada", "<user>");
        let text = "open /Users/ada/code/orders-service/src/a.ts\nlogs in /Users/ada/Library\nby ada; adam and Ada.x stay\nC:\\Users\\ada\\x";
        let redacted = redactor.apply(text);
        assert_eq!(
            redacted,
            "open <folder>/src/a.ts\nlogs in <home>/Library\nby <user>; adam and Ada.x stay\nC:<home>\\x"
        );
    }

    #[test]
    fn short_names_and_roots_are_not_redacted() {
        let mut redactor = Redactor::default();
        redactor.path("/", "<home>");
        redactor.word("on", "<user>");
        assert_eq!(redactor.apply("updates: on /tmp"), "updates: on /tmp");
    }

    #[test]
    fn counts_read_as_name_count_pairs() {
        let mut map = BTreeMap::new();
        assert_eq!(counts(&map), "none");
        map.insert("typescript".to_owned(), 34);
        map.insert("python".to_owned(), 50);
        assert_eq!(counts(&map), "python 50, typescript 34");
    }
}
