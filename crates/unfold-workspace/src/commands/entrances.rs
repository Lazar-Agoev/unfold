//! `entrances`: the apps and entrances of `unfold.toml` as written
//! (docs/designs/entrance-definitions.md), for Home's How it starts and the agents'
//! `list_entrances`.
//!
//! Apps come in file order, each with its entrances in file order, every field as written, its
//! lines in `unfold.toml`, and its problem when it has one (an entry with a problem stays out of
//! the graph and is listed anyway). Each `code` key carries its state (`found`, `missing`,
//! `not_analyzed`, `not_a_start`, `stale`) and, for a missing key with exactly one candidate,
//! its fix. Everything comes from what the project's engine last resolved, so the command wakes
//! the project. The same views, without states, describe the apps of `project_config`.

use serde::Serialize;
use ts_rs::TS;
use unfold_core::vocab::{InputRole, TargetState};
use unfold_core::{App, ByteRange, ContentHash, EntranceTarget, Graph, Key, LineIndex, RepoPath};
use unfold_engine::entrances::{FixKind, Fixes};
use unfold_engine::settings::{AppEntry, EntranceEntry, EntryProblem, EntryProblemKind};

use super::ProjectArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::workspace::settings_text;

/// The apps and entrances of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntrancesView {
    /// Whether the project's engine has read `unfold.toml`: `false` just after the project
    /// wakes and before its first Compile, with no apps listed; `entrances_changed` follows once
    /// it has.
    pub resolved: bool,
    /// The `[[app]]` tables, in file order.
    pub apps: Vec<AppView>,
}

/// One `[[app]]` table as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    /// Its key, `entrance:<slug>`; absent when its name makes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub key: Option<Key>,
    /// Its name; absent when the table has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// The command that starts it, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run: Option<String>,
    /// Where that command starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<EntranceCode>,
    /// One sentence for its card.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub about: Option<String>,
    /// What it handles, in file order.
    pub entrances: Vec<EntranceView>,
    /// The table's first and last line in `unfold.toml`; absent when the file changed since it
    /// was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lines: Option<[u32; 2]>,
    /// Why it stays out of the graph, with its entrances.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub problem: Option<EntryProblemView>,
}

/// One entrance of an app as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceView {
    /// Its key, `entrance:<app slug>/<entrance slug>`; absent when the names make none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub key: Option<Key>,
    /// Its name; absent when the entry has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// What starts it, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<String>,
    /// The blocks it starts at, in the order they run.
    pub code: Vec<EntranceCode>,
    /// One sentence for its card.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub about: Option<String>,
    /// The entry's first and last line in `unfold.toml`; absent when the file changed since it
    /// was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lines: Option<[u32; 2]>,
    /// Why it stays out of the graph.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub problem: Option<EntryProblemView>,
}

/// One `code` key as written, with what it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceCode {
    /// The block key.
    pub key: Key,
    /// What it names after the last compile; absent for an entry with a problem (it stays out
    /// of the graph) and in `project_config`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<TargetState>,
    /// The key to write instead, when the key is missing and exactly one candidate fits.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fix: Option<CodeFix>,
}

/// A one-candidate fix for a missing `code` key, offered and never applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CodeFix {
    /// Why it fits.
    pub kind: FixReason,
    /// The key to write instead.
    pub code: Key,
    /// The file of the block it names.
    pub path: RepoPath,
}

/// Why a [`CodeFix`] fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FixReason {
    /// The definition moved to another file.
    MovedDefinition,
    /// The literal of a route's registration call was edited.
    EditedRoute,
    /// The module moved.
    MovedModule,
}

impl From<FixKind> for FixReason {
    fn from(kind: FixKind) -> Self {
        match kind {
            FixKind::MovedDefinition => Self::MovedDefinition,
            FixKind::EditedRoute => Self::EditedRoute,
            FixKind::MovedModule => Self::MovedModule,
        }
    }
}

/// What is wrong with one app or entrance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryProblemView {
    /// What kind of problem.
    pub kind: EntryProblemReason,
    /// What to fix.
    pub message: String,
    /// The line of the field at fault (or of the entry); absent when the file changed since
    /// it was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub line: Option<u32>,
}

/// The kinds of [`EntryProblemView`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EntryProblemReason {
    /// A field an app or entrance doesn't have.
    UnknownField,
    /// A field whose value has the wrong type, or an entry that isn't a table.
    WrongType,
    /// No `name`.
    MissingName,
    /// An entrance with no `code`.
    MissingCode,
    /// A name with no letters or digits, which makes no key, or an empty `code` key.
    EmptyKey,
    /// Another app, or another entrance of the same app, has the same key.
    DuplicateKey,
}

impl From<EntryProblemKind> for EntryProblemReason {
    fn from(kind: EntryProblemKind) -> Self {
        match kind {
            EntryProblemKind::UnknownField => Self::UnknownField,
            EntryProblemKind::WrongType => Self::WrongType,
            EntryProblemKind::MissingName => Self::MissingName,
            EntryProblemKind::MissingCode => Self::MissingCode,
            EntryProblemKind::EmptyKey => Self::EmptyKey,
            EntryProblemKind::DuplicateKey => Self::DuplicateKey,
        }
    }
}

/// `entrances`: the apps and entrances as written, each `code` with its state and its fix.
/// Wakes the project: what it lists is what the project's engine last resolved.
pub fn entrances(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<EntrancesView> {
    let workspace = ctx.live(&args.project)?;
    let Some(resolved) = workspace.entrances() else {
        return Ok(EntrancesView {
            resolved: false,
            apps: Vec::new(),
        });
    };
    // Lines come from the file as it is now, only while it is the one the entries were read
    // from.
    let text = settings_text(workspace.root())?.filter(|text| read_from(&resolved.graph, text));
    let states = States {
        apps: &resolved.graph.apps,
        fixes: &resolved.fixes,
    };
    Ok(EntrancesView {
        resolved: true,
        apps: app_views(&resolved.entries, text.as_deref(), Some(&states)),
    })
}

/// Whether `text` is the settings file the graph's apps were resolved from.
fn read_from(graph: &Graph, text: &str) -> bool {
    graph
        .inputs
        .iter()
        .find(|input| input.role == InputRole::UnfoldConfig)
        .is_some_and(|input| input.hash == ContentHash::of(text.as_bytes()))
}

/// What the entries' `code` resolved to: the graph's apps (the valid entries) and the fixes.
pub(crate) struct States<'a> {
    pub(crate) apps: &'a [App],
    pub(crate) fixes: &'a Fixes,
}

/// The views of `entries`, with lines from `text` (the file they were read from, when known)
/// and the states of their code when `states` is given.
pub(crate) fn app_views(
    entries: &[AppEntry],
    text: Option<&str>,
    states: Option<&States<'_>>,
) -> Vec<AppView> {
    let lines = text.map(LineIndex::new);
    let span = |range: ByteRange| {
        lines.as_ref().map(|lines| {
            let span = lines.span(range);
            [span.start, span.end]
        })
    };
    let problem = |problem: &Option<EntryProblem>| {
        problem.as_ref().map(|problem| EntryProblemView {
            kind: problem.kind.into(),
            message: problem.message.to_string(),
            line: span(problem.span).map(|[start, _]| start),
        })
    };
    let text = |text: &Option<Box<str>>| text.as_deref().map(str::to_owned);
    let fixes = states.map(|states| states.fixes);
    entries
        .iter()
        .map(|entry| {
            // The graph holds the valid entries only, by their (unique) keys.
            let app = states
                .zip(entry.key.as_ref().filter(|_| entry.is_valid()))
                .and_then(|(states, key)| states.apps.iter().find(|app| app.key == *key));
            let entrance_view = |entrance: &EntranceEntry| {
                let found = app
                    .zip(entrance.key.as_ref().filter(|_| entrance.is_valid()))
                    .and_then(|(app, key)| app.entrances.iter().find(|found| found.key == *key));
                EntranceView {
                    key: entrance.key.clone(),
                    name: text(&entrance.name),
                    on: text(&entrance.on),
                    code: codes(
                        &entrance.code,
                        found.zip(fixes).map(|(found, fixes)| (&found.code[..], fixes)),
                    ),
                    about: text(&entrance.about),
                    lines: span(entrance.span),
                    problem: problem(&entrance.problem),
                }
            };
            AppView {
                key: entry.key.clone(),
                name: text(&entry.name),
                run: text(&entry.run),
                code: entry.code.as_ref().and_then(|code| {
                    let resolved = app.zip(fixes).map(|(app, fixes)| (app.code.as_slice(), fixes));
                    codes(std::slice::from_ref(code), resolved).pop()
                }),
                about: text(&entry.about),
                entrances: entry.entrances.iter().map(entrance_view).collect(),
                lines: span(entry.span),
                problem: problem(&entry.problem),
            }
        })
        .collect()
}

/// `keys` as written, with the states of `resolved` (the same keys, resolved, in order) and the
/// fixes of the missing ones; without states when the entry isn't resolved.
fn codes(keys: &[Key], resolved: Option<(&[EntranceTarget], &Fixes)>) -> Vec<EntranceCode> {
    keys.iter()
        .enumerate()
        .map(|(index, key)| {
            let target = resolved.and_then(|(targets, fixes)| {
                let target = targets.get(index).filter(|target| target.code == *key)?;
                Some((target, fixes))
            });
            EntranceCode {
                key: key.clone(),
                state: target.map(|(target, _)| target.state),
                fix: target
                    .and_then(|(target, fixes)| fixes.get(target))
                    .map(|fix| CodeFix {
                        kind: fix.kind.into(),
                        code: fix.code.clone(),
                        path: fix.path.clone(),
                    }),
            }
        })
        .collect()
}

/// The span in `unfold.toml` of the app or entrance with `key` among `entries` (the first, when
/// several share it).
pub(crate) fn span_of(entries: &[AppEntry], key: &str) -> Option<ByteRange> {
    entries.iter().find_map(|app| {
        if app.key.as_ref().is_some_and(|found| found.as_str() == key) {
            return Some(app.span);
        }
        app.entrances
            .iter()
            .find(|entrance| entrance.key.as_ref().is_some_and(|found| found.as_str() == key))
            .map(|entrance| entrance.span)
    })
}
