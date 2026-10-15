//! The Problems list (problem reporting, docs/engine.md#project-lifecycle).
//!
//! Problems lists only real problems:
//!
//! - files that don't parse or can't be read, with the position of the error;
//! - settings errors, with line and column (an unsupported Python grammar version is one: the
//!   settings parser rejects it at its position);
//! - discovery, configuration-input, and cache problems the engine reports;
//! - a compile that failed outright (the previous graph is kept);
//! - board health, which the board commands set ([`crate::Workspace::set_board_problems`]).
//!
//! Board health is listed but doesn't make the project's state: the row's amber dot and Home's
//! state line count the other problems only ([`project_problem_count`]); boards that need review
//! show in Home's Health line.
//!
//! Unresolved calls are facts about the code, not problems. Language problems the linker
//! reports (`Linked::problems`, such as Python modules that two source roots both provide)
//! arrive as input problems among the engine's standing problems.

use std::path::Path;

use serde::Serialize;
use unfold_core::vocab::FileState;
use unfold_core::{Graph, LineBreaks, LineIndex};
use unfold_engine::settings::{SettingsError, SettingsFile};
use unfold_engine::{Problem, ProblemKind};

/// What kind of problem a [`ProblemItem`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ProblemCategory {
    /// The settings file can't be read or is invalid.
    Settings,
    /// The last compile or update failed.
    CompileFailed,
    /// A file doesn't parse.
    DoesntParse,
    /// A file can't be read or isn't UTF-8.
    CantRead,
    /// Discovery couldn't read or use part of the folder.
    Discovery,
    /// A configuration input can't be read.
    Input,
    /// The facts cache failed (the update went on without it).
    Cache,
    /// A board's health check found something.
    Board,
}

/// One entry in the Problems list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProblemItem {
    /// What kind of problem.
    pub category: ProblemCategory,
    /// The folder-relative path involved, or empty.
    pub path: String,
    /// The one-based line, when known.
    pub line: Option<u32>,
    /// The one-based column in characters, when known.
    pub column: Option<u32>,
    /// What went wrong.
    pub message: String,
    /// For a file: `last_good` (the graph shows its last version that parsed) or `failed`.
    pub file_state: Option<FileState>,
}

impl ProblemItem {
    /// A problem with no file position.
    pub fn new(category: ProblemCategory, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category,
            path: path.into(),
            line: None,
            column: None,
            message: message.into(),
            file_state: None,
        }
    }
}

/// The problems of one folder, by source.
#[derive(Debug, Clone, Default)]
pub(crate) struct Problems {
    /// From the last committed graph and the engine's standing problems.
    pub(crate) graph: Vec<ProblemItem>,
    /// The last compile failure, until a compile or update succeeds.
    pub(crate) compile: Option<ProblemItem>,
    /// Board health, from the board commands.
    pub(crate) boards: Vec<ProblemItem>,
}

impl Problems {
    /// Every problem, sorted by category, path, and position.
    pub(crate) fn all(&self) -> Vec<ProblemItem> {
        let mut all: Vec<ProblemItem> = self
            .compile
            .iter()
            .chain(&self.graph)
            .chain(&self.boards)
            .cloned()
            .collect();
        all.sort();
        all.dedup();
        all
    }
}

/// How many of `problems` make the project's `problems` state (crates/unfold-workspace/src/status.rs): graph and
/// settings problems and a failed compile, not board health.
pub(crate) fn project_problem_count(problems: &[ProblemItem]) -> u32 {
    let count = problems
        .iter()
        .filter(|problem| problem.category != ProblemCategory::Board)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The problems a committed graph and the engine's standing problems describe. Files that don't
/// parse are read again (they are few) to place the error and to tell a syntax error from an
/// unreadable file.
pub(crate) fn graph_problems(root: &Path, graph: &Graph, standing: &[Problem]) -> Vec<ProblemItem> {
    let mut items: Vec<ProblemItem> = graph
        .files
        .iter()
        .filter(|file| file.file.state != FileState::Current)
        .map(|file| {
            let record = &file.file;
            let failure = record.failure.as_deref().unwrap_or("doesn't parse");
            let breaks = LineBreaks::of(record.language);
            file_problem(root, record.path.as_str(), failure, record.state, breaks)
        })
        .collect();
    items.extend(standing.iter().map(|problem| engine_problem(root, problem)));
    items.sort();
    items
}

/// Splits the engine's `"<message> (at byte N)"` failure text.
fn split_offset(failure: &str) -> (&str, Option<u32>) {
    if let Some(head) = failure.strip_suffix(')')
        && let Some(index) = head.rfind(" (at byte ")
        && let Ok(offset) = head[index + " (at byte ".len()..].parse()
    {
        return (&head[..index], Some(offset));
    }
    (failure, None)
}

/// The problem of a source file that isn't current, placed in its text, whose lines end at
/// `breaks`.
fn file_problem(root: &Path, path: &str, failure: &str, state: FileState, breaks: LineBreaks) -> ProblemItem {
    let (message, offset) = split_offset(failure);
    let text = std::fs::read(root.join(path))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let mut item = ProblemItem {
        file_state: Some(state),
        ..ProblemItem::new(
            if text.is_some() {
                ProblemCategory::DoesntParse
            } else {
                ProblemCategory::CantRead
            },
            path,
            message,
        )
    };
    if let (Some(text), Some(offset)) = (text, offset) {
        let (line, column) = position(&text, offset, breaks);
        item.line = Some(line);
        item.column = Some(column);
    }
    item
}

/// The one-based line and character column of byte `offset` in `text`, whose lines end at
/// `breaks`.
fn position(text: &str, offset: u32, breaks: LineBreaks) -> (u32, u32) {
    let lines = LineIndex::new(text, breaks);
    let line = lines.line_of(offset);
    let start = lines.line_start(line).unwrap_or(0) as usize;
    let end = (offset as usize).min(text.len());
    let column = text.get(start..end).map_or(0, |prefix| prefix.chars().count());
    (line, u32::try_from(column).unwrap_or(u32::MAX).saturating_add(1))
}

fn engine_problem(root: &Path, problem: &Problem) -> ProblemItem {
    let category = match problem.kind {
        ProblemKind::Settings => ProblemCategory::Settings,
        ProblemKind::Discovery => ProblemCategory::Discovery,
        ProblemKind::Input => ProblemCategory::Input,
        ProblemKind::Cache => ProblemCategory::Cache,
    };
    let mut item = ProblemItem::new(category, &*problem.path, &*problem.message);
    if category == ProblemCategory::Settings
        && let Err(SettingsError::Invalid {
            message,
            line,
            column,
            ..
        }) = SettingsFile::load(root)
    {
        item.path = unfold_engine::settings::SETTINGS_PATH.to_owned();
        item.message = message.into();
        item.line = Some(line);
        item.column = Some(column);
    }
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_failure_offset() {
        assert_eq!(
            split_offset("Unexpected token (at byte 12)"),
            ("Unexpected token", Some(12))
        );
        assert_eq!(
            split_offset("cannot read the file: denied"),
            ("cannot read the file: denied", None)
        );
        assert_eq!(split_offset("odd (at byte x)"), ("odd (at byte x)", None));
    }

    #[test]
    fn places_offsets_by_line_and_character() {
        let text = "ab\ncdé f\n";
        assert_eq!(position(text, 0, LineBreaks::Newlines), (1, 1));
        assert_eq!(position(text, 3, LineBreaks::Newlines), (2, 1));
        // `f` follows a two-byte character.
        assert_eq!(position(text, 8, LineBreaks::Newlines), (2, 5));
        // U+2028 ends a TypeScript line, not a Python one.
        let text = "s = \"\u{2028}\"\nx";
        assert_eq!(position(text, 10, LineBreaks::Newlines), (2, 1));
        assert_eq!(position(text, 10, LineBreaks::EcmaScript), (3, 1));
    }

    #[test]
    fn reads_failed_files_to_place_and_classify_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("bad.ts"), "let x = ;\n").expect("write");
        std::fs::write(dir.path().join("bin.ts"), [0xff, 0xfe]).expect("write");
        let parse = file_problem(
            dir.path(),
            "bad.ts",
            "Unexpected token (at byte 8)",
            FileState::Failed,
            LineBreaks::EcmaScript,
        );
        assert_eq!(parse.category, ProblemCategory::DoesntParse);
        assert_eq!((parse.line, parse.column), (Some(1), Some(9)));
        assert_eq!(parse.message, "Unexpected token");
        let read = file_problem(
            dir.path(),
            "bin.ts",
            "invalid UTF-8 at byte 0",
            FileState::Failed,
            LineBreaks::EcmaScript,
        );
        assert_eq!(read.category, ProblemCategory::CantRead);
        assert_eq!(read.line, None);
    }

    #[test]
    fn settings_problems_carry_their_position() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("unfold.toml"), "[scope]\nbogus = true\n").expect("write");
        let problem = Problem {
            kind: ProblemKind::Settings,
            path: "unfold.toml".into(),
            message: "invalid".into(),
        };
        let item = engine_problem(dir.path(), &problem);
        assert_eq!(item.category, ProblemCategory::Settings);
        assert_eq!(item.line, Some(2));
        assert!(item.column.is_some());
    }

    #[test]
    fn board_health_doesnt_count_toward_the_project_state() {
        let problems = Problems {
            graph: vec![ProblemItem::new(ProblemCategory::DoesntParse, "a.ts", "bad")],
            compile: Some(ProblemItem::new(ProblemCategory::CompileFailed, "", "boom")),
            boards: vec![
                ProblemItem::new(ProblemCategory::Board, ".unfold/boards/a.json", "outdated"),
                ProblemItem::new(ProblemCategory::Board, ".unfold/boards/b.json", "missing"),
            ],
        };
        let all = problems.all();
        assert_eq!(all.len(), 4);
        assert_eq!(project_problem_count(&all), 2);
        let boards_only = Problems {
            boards: problems.boards,
            ..Problems::default()
        };
        assert_eq!(project_problem_count(&boards_only.all()), 0);
    }
}
