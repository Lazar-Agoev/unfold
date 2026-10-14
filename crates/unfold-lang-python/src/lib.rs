//! The Python front end: the Ruff parser adapter, lowering to nodes, the binder, imports and
//! source roots, C3 linearization and receivers, decorators, and syntax highlighting for the
//! reader.
//!
//! - [`extract`]: parsing with Ruff under the configured grammar version (3.12–3.14), the
//!   compile-time checks the proof of concept's grammar made, lowering the parse tree straight
//!   to nodes (`lower`, one rule per syntax form), the value table and call facts, the imports
//!   and written type names that become links, the typed records (signatures, parameters, and
//!   members), the binder's per-file facts, the payload, and the independent census;
//! - `link::PythonLinked`: the whole-program binder (source roots and imports, the write
//!   census, reflection, C3 and receivers, decorators) and its answers to the shared backend;
//! - [`PythonHighlighter`]: colors for the code panel from Ruff's lexer;
//! - [`Python`]: the [`Language`] implementation.

#![forbid(unsafe_code)]

mod analysis;
mod ast;
mod bind;
mod census;
mod extract;
mod highlight;
mod link;
mod lower;
mod parse;
mod payload;
mod stdlib;
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
pub const PRODUCER_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+ruff.0.16.10+payload.7");

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
