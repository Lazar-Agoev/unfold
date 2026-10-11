//! The Python front end: the Ruff parser adapter, the binder, imports and source roots, C3
//! linearization and receivers, decorators, the Python entrance detectors (pyproject scripts,
//! FastAPI, Celery, APScheduler), and syntax highlighting for the reader.
//!
//! See `docs/languages/python.md`:
//!
//! - [`extract`]: parsing with Ruff under the configured grammar version (3.12–3.14), the
//!   compile-time checks the proof of concept's grammar made, block selection through the
//!   engine's shared selector with the Python mapping, the value table, call and import
//!   occurrences, the typed facts (signatures, parameters, members, call results, and the type
//!   names of annotations), the binder's per-file facts, the payload, and the independent
//!   census;
//! - [`link::PythonLinked`]: the whole-program binder (source roots and imports, the write
//!   census, reflection, C3 and receivers, decorators) and its answers to the shared backend;
//! - the entrance detectors (`detect`): `pyproject.script`, `fastapi.route`, `celery.task`, and
//!   `apscheduler.job`, which [`link::PythonLinked`] returns from `Linked::detectors`;
//! - [`PythonHighlighter`]: colors for the Code view from Ruff's lexer;
//! - [`Python`]: the [`Language`] implementation.

#![forbid(unsafe_code)]

mod analysis;
mod ast;
mod bind;
mod census;
mod detect;
mod extract;
mod highlight;
mod link;
mod mapping;
mod parse;
mod payload;
mod stdlib;
mod syntax;
mod tokens;
mod typed;
mod values;

use unfold_core::highlight::Highlighter;
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::settings::{Settings, SettingsHash, SettingsHasher};
use unfold_engine::{
    FactsSet, FileFacts, Language, LanguageId, Linked, Producer, ProjectInputs, SourceFailure,
};

pub use extract::extract;
pub use highlight::PythonHighlighter;

/// The front end's name, stored with the graph.
pub const PRODUCER_NAME: &str = "unfold-lang-python";

/// The front end's version: the crate, the pinned Ruff parser, and the payload format.
pub const PRODUCER_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+ruff.0.16.10+payload.5");

/// The Python language.
#[derive(Debug, Default, Clone, Copy)]
pub struct Python;

impl Python {
    /// The language.
    pub fn new() -> Self {
        Self
    }
}

impl Language for Python {
    fn id(&self) -> LanguageId {
        LanguageId::Python
    }

    fn producer(&self) -> Producer {
        Producer {
            name: PRODUCER_NAME,
            version: PRODUCER_VERSION,
        }
    }

    /// `.py` files, when Python is graphed.
    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        path.extension() == Some("py") && settings.language_enabled(LanguageId::Python)
    }

    /// Extraction reads only the grammar version of the file's Python project, which the
    /// project hash covers. The producer version is mixed in because the facts cache is keyed by
    /// the engine version, not the payload format this crate writes.
    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash {
        SettingsHasher::new(PRODUCER_NAME)
            .add(PRODUCER_VERSION)
            .add_bytes(settings.python_hash(path).as_bytes())
            .finish()
    }

    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure> {
        extract(file, settings.python_version_for(file.path()))
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        Box::new(link::PythonLinked::new(files, inputs))
    }

    fn highlighter(&self) -> &dyn Highlighter {
        &PythonHighlighter
    }
}
