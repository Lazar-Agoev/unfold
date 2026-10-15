//! File commands: opening a project's file in the person's editor. The code panel's model is
//! `code_view`.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::error::WorkspaceError;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::CONFIG_PATH;
use crate::projects::ProjectId;
use crate::state::AppSettings;
use crate::workspace::Workspace;

/// The arguments of `open_in_editor`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInEditorArgs {
    /// The project.
    pub project: ProjectId,
    /// The file, relative to the project's folder.
    pub path: String,
    /// The one-based line to open at.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub line: Option<u32>,
}

/// `open_in_editor`: runs the settings' editor command (without a shell), or opens the file in
/// the default text editor when none is set. Asked for the project's `unfold.toml` (the project menu's Open
/// `unfold.toml`), it first creates an empty one when the project has none.
///
/// The default text editor opens any file as text: handing the file to its default app could
/// run it (a `.py` file's default app can be a launcher, and `.command` files run when "opened"),
/// while Unfold never runs the code it indexes (docs/architecture.md#security).
pub fn open_in_editor(ctx: &Ctx<'_>, args: OpenInEditorArgs) -> CommandResult<()> {
    let workspace = ctx.live(&args.project)?;
    if args.path == CONFIG_PATH {
        create_config(&workspace)?;
    }
    let path = existing(&workspace, &args.path)?;
    if !path.is_file() {
        return Err(CommandError::invalid_args(format!(
            "`{}` is not a file",
            args.path
        )));
    }
    let settings = ctx.projects().app().settings();
    let argv = open_argv(&settings, path.as_os_str().to_string_lossy().as_ref(), args.line)?;
    spawn_detached(&argv)
}

/// The program and arguments that open `path` at `line`: the editor command when the settings
/// have one, else `/usr/bin/open -t`, which opens the default text editor.
fn open_argv(settings: &AppSettings, path: &str, line: Option<u32>) -> CommandResult<Vec<String>> {
    match settings
        .editor_command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        Some(template) => editor_argv(template, path, line),
        None => Ok(vec!["/usr/bin/open".to_owned(), "-t".to_owned(), path.to_owned()]),
    }
}

/// Creates an empty `unfold.toml` in the project's folder unless it has one. `create_new`
/// never follows a link, and confinement refuses one that leaves the folder.
fn create_config(workspace: &Workspace) -> CommandResult<()> {
    let config = workspace.confine(CONFIG_PATH)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config.absolute())
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(WorkspaceError::io(config.absolute(), error).into()),
    }
}

/// The absolute path of an existing file or folder inside the project's folder.
fn existing(workspace: &Workspace, path: &str) -> CommandResult<PathBuf> {
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
        .map_err(|error| spawn_error(program, &error))?;
    let _ = std::thread::Builder::new()
        .name("unfold-editor".to_owned())
        .spawn(move || child.wait());
    Ok(())
}

/// Why `program` didn't start. A program not found asks for its full path: an app started from
/// the Dock gets a short `PATH`, without what a shell profile adds.
fn spawn_error(program: &str, error: &std::io::Error) -> CommandError {
    if error.kind() == ErrorKind::NotFound {
        CommandError::new(
            "editor_not_found",
            format!(
                "cannot find `{program}`: give the editor command its full path (`which {program}` \
                 prints it), since an app started from the Dock doesn't get your shell's PATH"
            ),
        )
    } else {
        CommandError::new("io", format!("cannot start `{program}`: {error}"))
    }
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

    #[test]
    fn files_open_in_the_editor_command_or_text_edit() {
        let none = AppSettings::default();
        assert_eq!(
            open_argv(&none, "/r/a.py", Some(4)).expect("argv"),
            ["/usr/bin/open", "-t", "/r/a.py"]
        );
        let blank = AppSettings {
            editor_command: Some("  ".to_owned()),
        };
        assert_eq!(
            open_argv(&blank, "/r/a.py", None).expect("argv"),
            ["/usr/bin/open", "-t", "/r/a.py"]
        );
        let code = AppSettings {
            editor_command: Some("code -g {path}:{line}:{column}".to_owned()),
        };
        assert_eq!(
            open_argv(&code, "/r/a.py", Some(4)).expect("argv"),
            ["code", "-g", "/r/a.py:4:1"]
        );
    }

    #[test]
    fn a_missing_program_asks_for_its_full_path() {
        let missing = spawn_detached(&["unfold-no-such-editor".to_owned()]).expect_err("missing");
        assert_eq!(missing.code, "editor_not_found");
        assert!(missing.message.contains("full path"), "{}", missing.message);
        let denied = spawn_error("code", &std::io::Error::from(ErrorKind::PermissionDenied));
        assert_eq!(denied.code, "io");
    }
}
