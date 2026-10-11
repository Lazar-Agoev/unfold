//! What a launch starts, and the Python import context it runs in.
//!
//! A [`LaunchTarget`] is symbolic: it names a file, module, or binding before languages link.
//! Configured `target` tables and planned commands both produce one; linking resolves it later.
//! A [`PythonContext`] is the import path one Python launch gives every file it reaches.

use serde::Serialize;
use unfold_core::RepoPath;

use crate::settings::FolderPath;

/// A symbolic launch target: what a process starts, before it is resolved to a block.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaunchTarget {
    /// A file launched as a script, relative to the open folder.
    File {
        /// The file.
        path: RepoPath,
    },
    /// A module run as `python -m`: the module, or a package's `__main__`.
    PythonModule {
        /// The dotted module name.
        module: Box<str>,
    },
    /// A callable imported from a module and invoked, as a console script does.
    PythonCallable {
        /// The dotted module name.
        module: Box<str>,
        /// The callable's name in the module.
        attribute: Box<str>,
    },
    /// An ASGI application served from a module (`uvicorn module:attribute`).
    PythonAsgi {
        /// The dotted module name.
        module: Box<str>,
        /// The application's name in the module.
        attribute: Box<str>,
        /// Whether the attribute is a factory that returns the application.
        factory: bool,
    },
}

/// The import context of one Python launch: where it starts and its ordered import path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PythonContext {
    /// Where the launch starts.
    pub entry: PythonEntry,
    /// The import path, in search order, relative to the open folder.
    pub roots: Vec<FolderPath>,
}

/// How a Python launch starts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PythonEntry {
    /// A script file run directly; it has no package.
    Script(RepoPath),
    /// A module imported by name (`python -m`, Uvicorn, a callable), with its package identity.
    Module(Box<str>),
}
