//! File commands (architecture §11.3 Files): one tree level with its quiet marks, a file's
//! definition outline, and opening a file in the person's editor or file manager.

use std::path::Path;
use std::process::{Command, Stdio};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::RepoPath;
use unfold_core::vocab::{BlockKind, EntranceKind, FileState, Language};
use unfold_engine::discovery::{FileClass, IgnoreRules, classify, is_default_excluded};
use unfold_engine::settings::{ScopeStatus, Settings};

use super::folders::SkipKind;
use super::{PathArgs, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::UNFOLD_DIR;
use crate::workspace::Workspace;

/// The most entries one `list_dir` returns.
pub const MAX_ENTRIES: usize = 10_000;

/// What a tree entry is on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A folder.
    Folder,
    /// A symbolic link (never followed).
    Symlink,
}

/// One entry of a folder, with the facts behind its quiet marks. The `●` (on the open board)
/// mark is the UI's: it knows its boards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// The file or folder name.
    pub name: String,
    /// The folder-relative path.
    pub path: String,
    /// File, folder, or link.
    pub kind: EntryKind,
    /// The source language, for a file Unfold would graph.
    pub language: Option<Language>,
    /// The file's state in the graph (`current`, `last_good` shows the amber "doesn't parse"
    /// warning, `failed`), when it is in the graph.
    pub state: Option<FileState>,
    /// Changed on disk and not in the graph yet (for a folder: something inside it).
    pub pending: bool,
    /// Non-process entrances declared in the file (`→N`).
    pub entrances: u32,
    /// Launches declared in the file (`N launches` on `package.json` and `pyproject.toml`).
    pub launches: u32,
    /// Why the entry isn't graphed: ignored (`ignored`, `default`), or a source-like file that
    /// isn't indexed (`excluded`, `not_included`, `language_off`, `javascript`).
    pub skipped: Option<SkipKind>,
    /// Whether this is the folder's `.unfold/` (shared settings and boards).
    pub unfold: bool,
}

/// One level of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// The listed folder (`""` for the open folder itself).
    pub path: String,
    /// Folders first, then files, each by name ignoring case.
    pub entries: Vec<DirEntry>,
    /// The folder has more than [`MAX_ENTRIES`] entries; the rest are left out.
    pub truncated: bool,
}

/// The graph facts `list_dir` attaches to files, when the folder is compiled.
struct GraphFacts {
    reader: Option<unfold_store::Reader>,
    /// `(entrances, launches)` by evidence path, loaded on first use (one query for the level).
    declared: Option<FxHashMap<String, (u32, u32)>>,
}

impl GraphFacts {
    fn new(workspace: &Workspace) -> CommandResult<Self> {
        let reader = match workspace.readers() {
            Ok(pool) => Some(pool.get()?),
            Err(_) => None,
        };
        Ok(Self {
            reader,
            declared: None,
        })
    }

    fn state(&self, path: &str) -> CommandResult<Option<FileState>> {
        Ok(match &self.reader {
            Some(reader) => reader.file(path)?.map(|row| row.state),
            None => None,
        })
    }

    /// `(entrances, launches)` declared in the file.
    fn entrances(&mut self, path: &str) -> CommandResult<(u32, u32)> {
        let Some(reader) = &self.reader else {
            return Ok((0, 0));
        };
        if self.declared.is_none() {
            let mut declared: FxHashMap<String, (u32, u32)> = FxHashMap::default();
            for entrance in reader.entrances()? {
                let counts = declared
                    .entry(entrance.evidence.path.as_str().to_owned())
                    .or_default();
                if entrance.kind == EntranceKind::Process {
                    counts.1 += 1;
                } else {
                    counts.0 += 1;
                }
            }
            self.declared = Some(declared);
        }
        Ok(self
            .declared
            .as_ref()
            .and_then(|declared| declared.get(path).copied())
            .unwrap_or_default())
    }
}

/// `list_dir`: one level of the folder tree with its marks.
pub fn list_dir(ctx: &Ctx<'_>, args: PathArgs) -> CommandResult<DirListing> {
    let workspace = ctx.workspace()?;
    let folder = workspace.confine(&args.path)?;
    let read = std::fs::read_dir(folder.absolute())
        .map_err(|error| CommandError::not_found(format!("cannot list `{}`: {error}", args.path)))?;
    let settings = workspace.settings().unwrap_or_default();
    let mut ignore = IgnoreRules::new(workspace.root(), &workspace.options().engine.discover);
    let mut facts = GraphFacts::new(&workspace)?;
    let pending = workspace.pending_under(folder.as_str());
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in read {
        let Ok(entry) = entry else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        let path = if folder.as_str().is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", folder.as_str())
        };
        let Ok(relative) = RepoPath::new(&path) else {
            continue;
        };
        let kind = match entry.file_type() {
            Ok(kind) if kind.is_symlink() => EntryKind::Symlink,
            Ok(kind) if kind.is_dir() => EntryKind::Folder,
            _ => EntryKind::File,
        };
        let mut item = DirEntry {
            name,
            path,
            kind,
            language: None,
            state: None,
            pending: false,
            entrances: 0,
            launches: 0,
            skipped: None,
            unfold: relative.as_str() == UNFOLD_DIR,
        };
        ctx.check_cancelled()?;
        mark(&mut item, &relative, &settings, &mut ignore, &mut facts, &pending)?;
        entries.push(item);
    }
    entries.sort_by_cached_key(|entry| {
        (
            entry.kind != EntryKind::Folder,
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    });
    Ok(DirListing {
        path: folder.as_str().to_owned(),
        entries,
        truncated,
    })
}

/// Fills in an entry's marks.
fn mark(
    item: &mut DirEntry,
    relative: &RepoPath,
    settings: &Settings,
    ignore: &mut IgnoreRules,
    facts: &mut GraphFacts,
    pending: &[String],
) -> CommandResult<()> {
    let is_folder = item.kind == EntryKind::Folder;
    item.pending = pending.iter().any(|path| {
        path == relative.as_str()
            || (is_folder
                && path
                    .strip_prefix(relative.as_str())
                    .is_some_and(|rest| rest.starts_with('/')))
    });
    if item.unfold {
        return Ok(());
    }
    if ignore.is_ignored(relative, is_folder) {
        item.skipped = Some(if is_default_excluded(relative.as_str()) {
            SkipKind::Default
        } else {
            SkipKind::Ignored
        });
        return Ok(());
    }
    if is_folder {
        item.skipped = folder_scope(relative, settings);
        return Ok(());
    }
    if item.kind == EntryKind::Symlink {
        item.skipped = Some(SkipKind::Symlink);
        return Ok(());
    }
    match classify(relative, settings) {
        FileClass::Source(language) => {
            item.language = Some(language);
            item.state = facts.state(relative.as_str())?;
            (item.entrances, item.launches) = facts.entrances(relative.as_str())?;
        }
        FileClass::Unindexed(reason) => item.skipped = Some(reason.into()),
        FileClass::Input(_, _) => {
            (item.entrances, item.launches) = facts.entrances(relative.as_str())?;
        }
        FileClass::Context(_, _) | FileClass::Other => {}
    }
    Ok(())
}

/// Whether `[scope]` leaves a whole folder out: excluded, or outside every `include` pattern
/// with none of them inside it.
fn folder_scope(folder: &RepoPath, settings: &Settings) -> Option<SkipKind> {
    match settings.scope.status(folder) {
        ScopeStatus::InScope => None,
        ScopeStatus::Excluded => Some(SkipKind::Excluded),
        ScopeStatus::NotIncluded => {
            let prefix = format!("{}/", folder.as_str());
            let holds_included = settings
                .scope
                .include()
                .iter()
                .any(|pattern| pattern.starts_with(&prefix) || pattern.starts_with("**"));
            (!holds_included).then_some(SkipKind::NotIncluded)
        }
    }
}

/// One definition in a file's outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntry {
    /// The definition's key (open its card with it).
    pub key: String,
    /// Its block kind.
    pub kind: BlockKind,
    /// Its display label.
    pub name: String,
    /// Its qualified name.
    pub qualified_name: String,
    /// Its first line.
    pub start_line: u32,
    /// Its last line.
    pub end_line: u32,
    /// The definitions written inside it (a class's methods), in source order.
    pub children: Vec<OutlineEntry>,
}

/// A file's definitions in source order, nested by where they are written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileOutline {
    /// The file.
    pub path: String,
    /// The file's language, when it is in the graph.
    pub language: Option<Language>,
    /// The file's state in the graph.
    pub state: Option<FileState>,
    /// The top-level definitions.
    pub items: Vec<OutlineEntry>,
}

/// Outlines nest at most this deep; deeper definitions are listed under the deepest one.
const MAX_OUTLINE_DEPTH: usize = 32;

/// `file_outline`: the definitions of an indexed file, classes holding their methods.
pub fn file_outline(ctx: &Ctx<'_>, args: PathArgs) -> CommandResult<FileOutline> {
    let workspace = ctx.workspace()?;
    let path = workspace.confine(&args.path)?;
    let reader = workspace.readers()?.get()?;
    let Some(file) = reader.file_graph(path.as_str())? else {
        return Ok(FileOutline {
            path: path.as_str().to_owned(),
            language: None,
            state: None,
            items: Vec::new(),
        });
    };
    let rows = reader.definitions_in(path.as_str())?;
    // Definitions in preorder; each attaches to the nearest enclosing definition.
    let mut slots: Vec<Option<u32>> = vec![None; file.blocks.len()];
    let mut nodes: Vec<(Option<usize>, OutlineEntry)> = Vec::with_capacity(rows.len());
    let mut depth: Vec<usize> = Vec::with_capacity(rows.len());
    for row in &rows {
        let Some(block) = file.blocks.get(row.local_id as usize) else {
            continue;
        };
        let mut parent = block.parent;
        let mut enclosing = None;
        while let Some(local) = parent {
            if let Some(index) = slots.get(local as usize).copied().flatten() {
                enclosing = Some(index as usize);
                break;
            }
            parent = file.blocks.get(local as usize).and_then(|block| block.parent);
        }
        let mut level = enclosing.map_or(0, |index| depth[index] + 1);
        while level > MAX_OUTLINE_DEPTH {
            enclosing = enclosing.and_then(|index| nodes[index].0);
            level -= 1;
        }
        slots[row.local_id as usize] = Some(count(nodes.len()));
        depth.push(level);
        nodes.push((
            enclosing,
            OutlineEntry {
                key: row.key.as_str().to_owned(),
                kind: row.kind,
                name: row.label.to_string(),
                qualified_name: row.qualified_name.to_string(),
                start_line: block.lines.start,
                end_line: block.lines.end,
                children: Vec::new(),
            },
        ));
    }
    // Children come after their parents in preorder, so attach from the end.
    let mut items = Vec::new();
    while let Some((parent, mut entry)) = nodes.pop() {
        entry.children.reverse();
        match parent {
            Some(index) => nodes[index].1.children.push(entry),
            None => items.push(entry),
        }
    }
    items.reverse();
    Ok(FileOutline {
        path: path.as_str().to_owned(),
        language: Some(file.file.language),
        state: Some(file.file.state),
        items,
    })
}

/// The arguments of `open_in_editor`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInEditorArgs {
    /// The folder-relative file.
    pub path: String,
    /// The one-based line to open at.
    #[serde(default)]
    #[ts(optional)]
    pub line: Option<u32>,
}

/// How a file was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OpenedWith {
    /// The editor command from the preferences.
    Editor,
    /// The system's text editor (never the file's default app, which could run it).
    DefaultApp,
}

/// `open_in_editor`: runs the preferences' editor command (without a shell), or opens the file
/// as text in the system's text editor when none is set.
///
/// The fallback never hands the file to its default app: a `.py` file's default app can be a
/// launcher that runs it, and `.command`, `.bat`, or `.js` files run when "opened", while Unfold
/// never runs the code it indexes (architecture §13).
pub fn open_in_editor(ctx: &Ctx<'_>, args: OpenInEditorArgs) -> CommandResult<OpenedWith> {
    let workspace = ctx.workspace()?;
    let path = existing(&workspace, &args.path)?;
    if !path.is_file() {
        return Err(CommandError::invalid_args(format!(
            "`{}` is not a file",
            args.path
        )));
    }
    let preferences = ctx.registry().app().preferences();
    match preferences
        .editor_command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        Some(template) => {
            let argv = editor_argv(template, path.as_os_str().to_string_lossy().as_ref(), args.line)?;
            spawn_detached(&argv)?;
            Ok(OpenedWith::Editor)
        }
        None => {
            open_as_text(&path)?;
            Ok(OpenedWith::DefaultApp)
        }
    }
}

/// Opens a file in the system's text editor: `open -t` on macOS, Notepad on Windows.
#[cfg(any(target_os = "macos", windows))]
fn open_as_text(path: &Path) -> CommandResult<()> {
    let path = path.as_os_str().to_string_lossy().into_owned();
    #[cfg(target_os = "macos")]
    let argv = ["/usr/bin/open".to_owned(), "-t".to_owned(), path];
    #[cfg(windows)]
    let argv = ["notepad.exe".to_owned(), path];
    spawn_detached(&argv)
}

/// Opens a file with the desktop's handler for its type, unless opening could run it (an
/// executable file or a `.desktop` launcher): there is no system text editor to name here.
#[cfg(not(any(target_os = "macos", windows)))]
fn open_as_text(path: &Path) -> CommandResult<()> {
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    };
    #[cfg(not(unix))]
    let executable = false;
    let launcher = path.extension().is_some_and(|extension| extension == "desktop");
    if executable || launcher {
        return Err(CommandError::new(
            "no_editor",
            "opening this file could run it; set an editor command in Preferences",
        ));
    }
    opener::open(path).map_err(|error| CommandError::new("io", error.to_string()))
}

/// `reveal_in_finder`: shows the file or folder in the system's file manager.
pub fn reveal_in_finder(ctx: &Ctx<'_>, args: PathArgs) -> CommandResult<()> {
    let workspace = ctx.workspace()?;
    let path = existing(&workspace, &args.path)?;
    opener::reveal(&path).map_err(|error| CommandError::new("io", error.to_string()))
}

/// The absolute path of an existing file or folder inside the workspace.
fn existing(workspace: &Workspace, path: &str) -> CommandResult<std::path::PathBuf> {
    let confined = workspace.confine(path)?;
    if !confined.absolute().exists() {
        return Err(CommandError::not_found(format!("`{path}` does not exist")));
    }
    Ok(confined.absolute().to_path_buf())
}

/// The editor's argument vector: the template split into words (quotes group words), with
/// `{path}`, `{line}`, and `{column}` replaced; the path is appended when the template doesn't
/// name it.
pub fn editor_argv(template: &str, path: &str, line: Option<u32>) -> CommandResult<Vec<String>> {
    let words = split_words(template)?;
    if words.is_empty() {
        return Err(CommandError::invalid_args("the editor command is empty"));
    }
    let line = line.unwrap_or(1).max(1).to_string();
    let mut named = false;
    let mut argv: Vec<String> = words
        .iter()
        .map(|word| {
            let (text, has_path) = substitute(word, path, &line);
            named |= has_path;
            text
        })
        .collect();
    if !named {
        argv.push(path.to_owned());
    }
    Ok(argv)
}

/// Replaces `{path}`, `{line}`, and `{column}` in one pass, so text substituted for one
/// placeholder (a path containing `{line}`) is never substituted again; whether `{path}` occurred.
fn substitute(word: &str, path: &str, line: &str) -> (String, bool) {
    let mut text = String::with_capacity(word.len() + path.len());
    let mut rest = word;
    let mut has_path = false;
    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let tail = &rest[open..];
        let (value, length) = if tail.starts_with("{path}") {
            has_path = true;
            (path, "{path}".len())
        } else if tail.starts_with("{line}") {
            (line, "{line}".len())
        } else if tail.starts_with("{column}") {
            ("1", "{column}".len())
        } else {
            ("{", 1)
        };
        text.push_str(value);
        rest = &tail[length..];
    }
    text.push_str(rest);
    (text, has_path)
}

/// Splits a command line into words: whitespace separates, single and double quotes group, and
/// a backslash escapes the next character outside single quotes.
fn split_words(text: &str) -> CommandResult<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (Some('\''), other) => word.push(other),
            (_, '\\') => {
                in_word = true;
                if let Some(next) = characters.next() {
                    word.push(next);
                }
            }
            (Some(_), other) => word.push(other),
            (None, '"' | '\'') => {
                in_word = true;
                quote = Some(character);
            }
            (None, space) if space.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, other) => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if quote.is_some() {
        return Err(CommandError::invalid_args(
            "the editor command has an unclosed quote",
        ));
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// Starts a program without a shell and without waiting for it; a thread reaps it.
fn spawn_detached(argv: &[String]) -> CommandResult<()> {
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| CommandError::invalid_args("the editor command is empty"))?;
    let mut child = Command::new(Path::new(program))
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| CommandError::new("io", format!("cannot start `{program}`: {error}")))?;
    let _ = std::thread::Builder::new()
        .name("unfold-editor".to_owned())
        .spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_commands_split_without_a_shell() {
        assert_eq!(
            editor_argv("code -g {path}:{line}", "/r/a b.ts", Some(12)).expect("argv"),
            ["code", "-g", "/r/a b.ts:12"]
        );
        assert_eq!(
            editor_argv("\"/Applications/My Editor.app/x\" --wait", "/r/a.ts", None).expect("argv"),
            ["/Applications/My Editor.app/x", "--wait", "/r/a.ts"]
        );
        assert_eq!(
            editor_argv("subl '{path}:{line}:{column}'", "/r/a.ts", None).expect("argv"),
            ["subl", "/r/a.ts:1:1"]
        );
        assert_eq!(
            editor_argv("vim; rm -rf x", "/r/a.ts", None).expect("argv"),
            ["vim;", "rm", "-rf", "x", "/r/a.ts"],
            "no shell: `;` is just text"
        );
        assert_eq!(
            editor_argv("ed {path}:{line}", "/r/{line}.ts", Some(3)).expect("argv"),
            ["ed", "/r/{line}.ts:3"],
            "a placeholder inside the path is left alone"
        );
        assert!(editor_argv("  ", "/r/a.ts", None).is_err());
        assert!(editor_argv("code \"unclosed", "/r/a.ts", None).is_err());
    }
}
