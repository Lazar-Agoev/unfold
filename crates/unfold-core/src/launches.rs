//! What the engine records about launches beside the graph (docs/reader.md#derived-views): the
//! declared applications, each launch's plan and presentation, and the applications configured
//! entrances declare. The store keeps it and the reader's entrance catalog reads it; it is not
//! part of the graph or its canonical export.

use serde::{Deserialize, Serialize};

use crate::key::Key;

/// The launch records of one compile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launches {
    /// The `[[application]]` tables, in file order.
    pub applications: Vec<DeclaredApplication>,
    /// One record per process entrance, in the graph's entrance order.
    pub launches: Vec<Launch>,
    /// Configured entrances other than processes that name an application, in the graph's
    /// entrance order.
    pub members: Vec<DeclaredMember>,
}

/// An `[[application]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredApplication {
    /// Its ID.
    pub id: Box<str>,
    /// Its name.
    pub name: Box<str>,
    /// Its description.
    pub description: Option<Box<str>>,
}

/// A configured entrance that names its application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredMember {
    /// The entrance's key.
    pub entrance: Key,
    /// The application's ID.
    pub application: Box<str>,
}

/// One launch (a process entrance): how it is shown, what Unfold read from it, and what it runs
/// besides its startup target. Detected launches take their presentation from a matching
/// `[[entrance_override]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    /// The process entrance's key.
    pub entrance: Key,
    /// The declared application's ID.
    pub application: Option<Box<str>>,
    /// The display label.
    pub label: Option<Box<str>>,
    /// The description.
    pub description: Option<Box<str>>,
    /// The schedule, as written.
    pub schedule: Option<Box<str>>,
    /// The project-relative folder it runs in (`.` for the project's folder).
    pub folder: Box<str>,
    /// The command: as written for a configured launch; `npm run start`, the bin, or the console
    /// script for a detected one. `None` when a configured launch has no command.
    pub command: Option<Box<str>>,
    /// How it starts its code, when known.
    pub mode: Option<LaunchMode>,
    /// The one-based line where it is declared, in its evidence file.
    pub line: u32,
    /// Whether its target was set by hand.
    pub target_set_by_hand: bool,
    /// Its Python import path, in search order.
    pub import_paths: Vec<LaunchPath>,
    /// Why its command can't be read, or doesn't add up.
    pub problem: Option<LaunchIssue>,
    /// Blocks it runs besides its startup target, in order: the packages a module launch
    /// initializes and the module of a callable or application target.
    pub seeds: Vec<Key>,
}

/// How a launch starts its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    /// It runs a file.
    File,
    /// It runs a Python module (`python -m`).
    PythonModule,
    /// It calls a Python callable (a console script).
    PythonCallable,
    /// It serves a Python ASGI application.
    PythonAsgi,
}

/// One folder on a launch's import path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPath {
    /// The project-relative folder.
    pub path: Box<str>,
    /// Why it is there, in words (`from --app-dir`).
    pub origin: Box<str>,
}

/// Why a launch can't be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchIssue {
    /// A short status (`Can't read sh -c`).
    pub label: Box<str>,
    /// The full explanation.
    pub message: Box<str>,
}
