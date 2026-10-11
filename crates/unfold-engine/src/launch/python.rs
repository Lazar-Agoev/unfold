//! Python launchers in configured commands: `python`, `python3`, and `python3.N` running a
//! script or a module (`-m`), and Uvicorn serving an ASGI application (`uvicorn module:app` or
//! `python -m uvicorn module:app`).
//!
//! Option arity is pinned: an unknown option could take the next word as its value and turn it
//! into a false target, so it makes the command unreadable. Expansions are allowed only where
//! they can't change the code: in server option values and in the program's own arguments.

use super::plan::{
    ImportPath, ImportPathOrigin, PROGRAM_ARGUMENTS, Parts, Place, SERVER_OPTIONS, script_folder,
};
use super::shell::Word;
use super::{LaunchTarget, Refusal};

/// Interpreter options accepted before the script or module. None of them changes the import
/// path; `-E`, `-I`, and `-P` do, and they are unsupported with every other option.
const PYTHON_OPTIONS: &[&str] = &["-u", "-B", "-O", "-OO"];

/// Uvicorn options that take a value and don't change the code.
const SERVER_VALUES: &[&str] = &[
    "--host",
    "--port",
    "--workers",
    "--log-level",
    "--forwarded-allow-ips",
    "--root-path",
    "--timeout-keep-alive",
];

/// Uvicorn flags that don't change the code.
const SERVER_FLAGS: &[&str] = &["--reload", "--proxy-headers"];

/// Whether `name` is a Python launcher: `python`, `python3`, `python3.N`, or `uvicorn`.
pub(super) fn is_launcher(name: &str) -> bool {
    let minor = name.strip_prefix("python3.");
    matches!(name, "python" | "python3" | "uvicorn")
        || minor.is_some_and(|minor| !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit()))
}

/// Reads a Python launch: its target and the import path the launcher sets up, before
/// `PYTHONPATH`. `command` is one of [`is_launcher`]'s names.
pub(super) fn launch(
    command: &Word,
    args: &[Word],
    place: &Place<'_>,
    parts: &mut Parts,
) -> Result<(LaunchTarget, Vec<ImportPath>), Refusal> {
    if command.text == "uvicorn" {
        parts.add([command], "Runs Uvicorn");
        return uvicorn(args, place, parts, false);
    }
    let options = args
        .iter()
        .take_while(|word| PYTHON_OPTIONS.contains(&word.text.as_str()))
        .count();
    let launcher = std::iter::once(command).chain(&args[..options]);
    let Some(first) = args.get(options) else {
        return Err(Refusal::new(format!(
            "{} needs a script or -m module",
            command.text
        )));
    };
    let text = first.literal("Python script or option")?;
    if text == "-m" {
        let word = args
            .get(options + 1)
            .ok_or_else(|| Refusal::new("Missing module for -m"))?;
        let module = word.literal("python -m module")?;
        if module == "uvicorn" {
            parts.add(launcher.chain([first, word]), "Runs Uvicorn through Python");
            return uvicorn(&args[options + 2..], place, parts, true);
        }
        if !is_dotted(module) {
            return Err(Refusal::new(format!("Unsupported module name {module}")));
        }
        parts.add(launcher, "Runs Python");
        parts.add([first, word], format!("Runs the module {module}"));
        parts.irrelevant(&args[options + 2..], PROGRAM_ARGUMENTS);
        let target = LaunchTarget::PythonModule {
            module: module.into(),
        };
        let paths = vec![ImportPath {
            path: place.folder("")?,
            origin: ImportPathOrigin::ModuleWorkingFolder,
        }];
        return Ok((target, paths));
    }
    if text.starts_with('-') {
        return Err(Refusal::new(format!("Unsupported python option {text}")));
    }
    if text.is_empty() {
        return Err(Refusal::new("Empty script path"));
    }
    let path = place.file(text)?;
    parts.add(launcher, "Runs Python");
    parts.add([first], format!("Runs the script {text}"));
    parts.irrelevant(&args[options + 1..], PROGRAM_ARGUMENTS);
    let paths = vec![ImportPath {
        path: script_folder(&path),
        origin: ImportPathOrigin::ScriptFolder,
    }];
    Ok((LaunchTarget::File { path }, paths))
}

/// Reads Uvicorn's arguments: one `module:attribute` application, `--app-dir`, `--factory`,
/// and the pinned server options.
fn uvicorn(
    args: &[Word],
    place: &Place<'_>,
    parts: &mut Parts,
    through_python: bool,
) -> Result<(LaunchTarget, Vec<ImportPath>), Refusal> {
    let mut application = None;
    let mut app_dir = None;
    let mut factory = false;
    let mut index = 0;
    while let Some(word) = args.get(index) {
        // Server option values may expand; everything else must be literal.
        let text = word.text.as_str();
        index += 1;
        if !text.starts_with('-') {
            let text = word.literal("Uvicorn application")?;
            if application.is_some() {
                return Err(Refusal::new("Uvicorn takes one application"));
            }
            let (module, attribute) = text
                .split_once(':')
                .filter(|(module, attribute)| is_dotted(module) && is_identifier(attribute))
                .ok_or_else(|| Refusal::new(format!("Unsupported Uvicorn application {text}")))?;
            parts.add([word], format!("Serves {attribute} from the module {module}"));
            application = Some((module, attribute));
            continue;
        }
        let (option, inline) = match text.split_once('=') {
            Some((option, value)) => (option, Some(value)),
            None => (text, None),
        };
        let start = index - 1;
        if option == "--app-dir" || SERVER_VALUES.contains(&option) {
            if inline.is_none() {
                if index == args.len() {
                    return Err(Refusal::new(format!("Missing value for {option}")));
                }
                index += 1;
            }
        } else if inline.is_some() || !(option == "--factory" || SERVER_FLAGS.contains(&option)) {
            return Err(Refusal::new(format!("Unsupported uvicorn option {option}")));
        }
        let words = &args[start..index];
        match option {
            "--app-dir" => {
                let directory = match inline {
                    Some(value) => word.literal("--app-dir").map(|_| value)?,
                    None => words[1].literal("--app-dir")?,
                };
                app_dir = Some(place.folder(directory)?);
                parts.add(words, format!("Puts {directory} first on the import path"));
            }
            "--factory" => {
                factory = true;
                parts.add(words, "Treats the application as a factory that builds it");
            }
            _ => parts.irrelevant(words, SERVER_OPTIONS),
        }
    }
    let (module, attribute) = application.ok_or_else(|| Refusal::new("Missing Uvicorn application"))?;
    let mut paths = vec![match app_dir {
        Some(path) => ImportPath {
            path,
            origin: ImportPathOrigin::AppDir,
        },
        // Uvicorn's default `--app-dir` is the working directory.
        None => ImportPath {
            path: place.folder("")?,
            origin: ImportPathOrigin::WorkingFolder,
        },
    }];
    if through_python {
        paths.push(ImportPath {
            path: place.folder("")?,
            origin: ImportPathOrigin::ModuleWorkingFolder,
        });
    }
    let target = LaunchTarget::PythonAsgi {
        module: module.into(),
        attribute: attribute.into(),
        factory,
    };
    Ok((target, paths))
}

/// A dotted module name: identifiers separated by `.`.
fn is_dotted(name: &str) -> bool {
    name.split('.').all(is_identifier)
}

/// A Python identifier.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}
