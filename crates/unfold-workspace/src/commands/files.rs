//! File commands: opening a project's file in the person's editor or file
//! manager. The source a peek sheet shows is `file_view`.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ProjectPathArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// The arguments of `open_in_editor`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInEditorArgs {
    /// The project.
    pub project: ProjectId,
    /// The file, relative to the project's folder.
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
    let workspace = ctx.live(&args.project)?;
    let path = existing(&workspace, &args.path)?;
    if !path.is_file() {
        return Err(CommandError::invalid_args(format!(
            "`{}` is not a file",
            args.path
        )));
    }
    let preferences = ctx.projects().app().preferences();
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

/// `reveal_in_finder`: shows the file or folder in the system's file manager (`""` for the
/// project's folder).
pub fn reveal_in_finder(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<()> {
    let workspace = ctx.live(&args.project)?;
    let path = existing(&workspace, &args.path)?;
    opener::reveal(&path).map_err(|error| CommandError::new("io", error.to_string()))
}

/// The absolute path of an existing file or folder inside the project's folder.
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
