//! `code_view`: the code panel's model for one selection (docs/designs/viewing-code.md): a
//! block, lines of a file (with an optional mark), a field of a type card, a whole file, or an
//! app or entrance (its lines in `unfold.toml`). The reader builds the model from the committed
//! graph and the source verified against it; the workspace confines paths, finds an app's or
//! entrance's span in `unfold.toml` among what the engine last resolved, and passes the
//! languages' highlighters. Wakes the project.

use serde::Deserialize;
use ts_rs::TS;
use unfold_core::RepoPath;
use unfold_reader::code_view::{self, CodeRequest, CodeSources, CodeState, CodeView, Target};

use super::entrances::span_of;
use super::key_of;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// The most lines a `lines` target spans.
pub const MAX_LINES: u32 = 100_000;
/// The longest title of a `lines` target, in characters.
pub const MAX_TITLE: usize = 400;

/// What the code panel opens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CodeTarget {
    /// A block: a module, definition, control block, part, call, or exit.
    Block {
        /// The block's key.
        key: String,
    },
    /// Lines of a file: a combined step, a row of other calls, a gap tick, or the lines a
    /// resource or connection cites (which can be lines of an evidence file).
    Lines {
        /// The file, relative to the project's folder.
        path: String,
        /// The first and last line, one-based and inclusive.
        lines: [u32; 2],
        /// The lines to mark, for a citation.
        #[serde(default)]
        #[ts(optional)]
        mark: Option<[u32; 2]>,
        /// A combined step's title, which labels it in the path.
        #[serde(default)]
        #[ts(optional)]
        title: Option<String>,
    },
    /// A written member of a class or type card, shown as its line.
    Field {
        /// The class or type.
        key: String,
        /// The member's index among its written members.
        index: u32,
    },
    /// A whole file: a source file, an evidence file, or any other file in the folder.
    File {
        /// The file, relative to the project's folder.
        path: String,
    },
    /// An app or an entrance: its lines in `unfold.toml`.
    Start {
        /// The app's or entrance's key.
        key: String,
    },
}

/// The arguments of `code_view`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeViewArgs {
    /// The project.
    pub project: ProjectId,
    /// The selection: it heads the path, and it stays marked.
    pub target: CodeTarget,
    /// What to show instead of the selection's own lines, when the path widened the view: a
    /// block on the selection's path, the file, or the app of a selected entrance.
    #[serde(default)]
    #[ts(optional)]
    pub show: Option<CodeTarget>,
}

/// `code_view`: the code panel's model for one selection.
pub fn code_view(ctx: &Ctx<'_>, args: CodeViewArgs) -> CommandResult<CodeView> {
    let workspace = ctx.live(&args.project)?;
    let Some(selected) = target(&workspace, args.target)? else {
        return Ok(not_found());
    };
    let show = match args.show {
        Some(show) => target(&workspace, show)?,
        None => None,
    };
    let request = CodeRequest { selected, show };
    let reader = workspace.readers()?.get()?;
    let highlighters = workspace.highlighters();
    let root = workspace.root();
    let view = reader.snapshot(|reader| {
        let inputs = reader.inputs()?;
        let apps = reader.apps()?;
        let sources = CodeSources {
            facts: reader,
            root,
            inputs: &inputs,
            apps: &apps,
            highlighters: &highlighters,
        };
        Ok(code_view::code_view(&sources, &request))
    })?;
    Ok(view?)
}

/// The panel's model when the selection names nothing: an app or entrance key no table of
/// `unfold.toml` has.
fn not_found() -> CodeView {
    CodeView {
        file: None,
        state: CodeState::NotFound,
        lines: Vec::new(),
        mark: None,
        path: Vec::new(),
        definitions: Vec::new(),
    }
}

/// The reader's target for `target`, with its path confined and its key checked; `None` for an
/// app or entrance key no table has.
fn target(workspace: &Workspace, target: CodeTarget) -> CommandResult<Option<Target>> {
    Ok(Some(match target {
        CodeTarget::Block { key } => Target::Block(key_of(&key)?),
        CodeTarget::Lines {
            path,
            lines,
            mark,
            title,
        } => {
            check_lines(lines)?;
            if let Some(mark) = mark {
                check_lines(mark)?;
            }
            if title
                .as_ref()
                .is_some_and(|title| title.chars().count() > MAX_TITLE)
            {
                return Err(CommandError::invalid_args(format!(
                    "a title is at most {MAX_TITLE} characters"
                )));
            }
            Target::Lines {
                path: file(workspace, &path)?,
                lines,
                mark,
                title,
            }
        }
        CodeTarget::Field { key, index } => Target::Field {
            key: key_of(&key)?,
            index,
        },
        CodeTarget::File { path } => Target::File(file(workspace, &path)?),
        CodeTarget::Start { key } => {
            let key = key_of(&key)?;
            let span = workspace
                .entrances()
                .and_then(|resolved| span_of(&resolved.entries, key.as_str()));
            match span {
                Some(span) => Target::Start { key, span },
                None => return Ok(None),
            }
        }
    }))
}

/// A file path inside the project's folder.
fn file(workspace: &Workspace, path: &str) -> CommandResult<RepoPath> {
    workspace
        .confine(path)?
        .relative()
        .cloned()
        .ok_or_else(|| CommandError::invalid_args("the path names the project's folder, not a file"))
}

/// One-based, ordered lines spanning at most [`MAX_LINES`].
fn check_lines([first, last]: [u32; 2]) -> CommandResult<()> {
    if first == 0 || last < first || last - first >= MAX_LINES {
        return Err(CommandError::invalid_args(format!(
            "lines are one-based, in order, and span at most {MAX_LINES} lines"
        )));
    }
    Ok(())
}
