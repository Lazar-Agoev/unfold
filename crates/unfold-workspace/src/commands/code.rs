//! `code_view`: the code panel's model for one selection (docs/reader.md#the-code-panels-model): a
//! block, lines of a file (with an optional mark), a field of a type card, a whole file, or an
//! app or entrance (its lines in `unfold.toml`), answered a window of lines at a time. The reader
//! builds the model from the committed graph and the source verified against it; the workspace
//! confines paths, finds an app's or entrance's span in `unfold.toml` among what the engine last
//! resolved, says whether that file parses as the engine last read it, and passes the languages'
//! highlighters and the sources verified before. Wakes the project.

use serde::Deserialize;
use ts_rs::TS;
use unfold_core::RepoPath;
use unfold_reader::code_view::{self, CodeRequest, CodeSources, CodeView, MAX_WINDOW, Target};

use super::entrances::span_of;
use super::key_of;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

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
    /// The view's lines to answer with, first and last, one-based: at most 400 (a page as the
    /// panel scrolls, or what `read_file` reads). Absent: a page of 200 from a little above the
    /// first line the selection marks.
    #[serde(default)]
    #[ts(optional)]
    pub window: Option<[u32; 2]>,
}

/// `code_view`: the code panel's model for one selection.
pub fn code_view(ctx: &Ctx<'_>, args: CodeViewArgs) -> CommandResult<CodeView> {
    if let Some(window) = args.window {
        check_lines(window)?;
        if window[1] - window[0] >= MAX_WINDOW {
            return Err(CommandError::invalid_args(format!(
                "a window is at most {MAX_WINDOW} lines"
            )));
        }
    }
    let workspace = ctx.live(&args.project)?;
    let Some(selected) = target(&workspace, args.target)? else {
        return Ok(CodeView::not_found());
    };
    let show = match args.show {
        Some(show) => target(&workspace, show)?,
        None => None,
    };
    let request = CodeRequest {
        selected,
        show,
        window: args.window,
    };
    let reader = workspace.readers()?.get()?;
    let highlighters = workspace.highlighters();
    let root = workspace.root();
    let settings_parse = workspace
        .entrances()
        .is_none_or(|resolved| resolved.settings_parse);
    let view = reader.snapshot(|reader| {
        let inputs = reader.inputs()?;
        let apps = reader.apps()?;
        let sources = CodeSources {
            facts: reader,
            root,
            inputs: &inputs,
            apps: &apps,
            settings_parse,
            highlighters: &highlighters,
            kept: workspace.kept_sources(),
        };
        Ok(code_view::code_view(&sources, &request))
    })?;
    Ok(view?)
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

/// One-based lines, in order.
fn check_lines([first, last]: [u32; 2]) -> CommandResult<()> {
    if first == 0 || last < first {
        return Err(CommandError::invalid_args("lines are one-based and in order"));
    }
    Ok(())
}
