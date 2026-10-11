//! The TypeScript front end: the oxc adapter, project configuration (`tsconfig.json` chains),
//! module resolution, export tables, binding and evidence, the emit map, module labels, generated
//! library tables, and syntax highlighting for the reader (`docs/languages/typescript.md`).
//!
//! - [`extract`]: strict parsing with oxc, block selection through the engine's shared selector
//!   with the TypeScript mapping (kinds, roles, names, slots), the value table, call and import
//!   occurrences, typed facts (signatures, parameters, members, call results, and written type
//!   names), module-level facts, the file's binding (declarations, symbols, and the expressions
//!   evidence evaluates), the payload, and the independent census;
//! - [`TypeScript`]: the [`Language`] implementation. Each file's extraction options come from
//!   its nearest `tsconfig.json` ([`TypeScript::extraction_options`]); linking
//!   ([`link::TypeScriptLinked`]) resolves modules and exports, answers callee, value, type,
//!   and declaration evidence (evidence levels 1–4 and constant object members), class shapes,
//!   and locators, and spells module labels;
//! - [`tables`]: the known globals, library types, global type aliases, and Node built-ins of the
//!   pinned TypeScript and `@types/node` declarations;
//! - [`TypeScriptHighlighter`]: lexical colors for the Code view.

#![forbid(unsafe_code)]

mod bind;
mod build;
pub mod census;
mod config;
mod detect;
mod emit;
mod evidence;
pub mod exports;
mod extract;
mod highlight;
mod imports;
pub mod labels;
pub mod link;
mod mapping;
mod module_syntax;
pub mod modules;
mod names;
mod number;
mod paths;
pub mod payload;
mod scan;
mod sites;
mod stack;
mod syntax;
pub mod tables;
mod typed;
mod values;

use std::sync::{Arc, Mutex};

use unfold_core::highlight::Highlighter;
use unfold_core::vocab::InputRole;
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::settings::{Settings, SettingsHash, SettingsHasher};
use unfold_engine::{
    ExtractionOptions, FactsSet, FileFacts, InputProblem, Language, LanguageId, Linked, Producer,
    ProjectInputs, SourceFailure,
};

pub use config::BindingContext;
pub use emit::EmitMap;
pub use extract::{ExtractOptions, extract, is_typescript_source};
pub use highlight::TypeScriptHighlighter;

use config::ConfigSet;

/// The front end's name, stored with the graph.
pub const PRODUCER_NAME: &str = "unfold-lang-typescript";

/// The TypeScript language.
#[derive(Debug, Default)]
pub struct TypeScript {
    options: ExtractOptions,
    highlighter: TypeScriptHighlighter,
    /// The configurations parsed from the last set of inputs, keyed by their fingerprint.
    configs: Mutex<Option<(SettingsHash, Arc<ConfigSet>)>>,
}

impl Clone for TypeScript {
    fn clone(&self) -> Self {
        Self::with_options(self.options)
    }
}

impl TypeScript {
    /// The language with default extraction options.
    pub fn new() -> Self {
        Self::default()
    }

    /// The language extracting with `options` wherever a file's `tsconfig.json` doesn't say
    /// otherwise (and for files without one).
    pub fn with_options(options: ExtractOptions) -> Self {
        Self {
            options,
            highlighter: TypeScriptHighlighter,
            configs: Mutex::new(None),
        }
    }

    /// The default extraction options.
    pub fn options(&self) -> ExtractOptions {
        self.options
    }

    /// The configurations of `inputs`, parsed once per distinct set of configuration inputs.
    fn configs(&self, inputs: &ProjectInputs) -> Arc<ConfigSet> {
        let mut hasher = SettingsHasher::new("typescript.configs");
        for (path, input) in &inputs.inputs {
            if matches!(input.role, InputRole::Tsconfig | InputRole::Package) {
                hasher
                    .add(path.as_str())
                    .add(&input.role.as_str())
                    .add_bytes(input.snapshot.hash().as_bytes());
            }
        }
        let key = hasher.finish();
        // A poisoned lock only means another thread panicked while parsing; parse again.
        let mut cached = self
            .configs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((found, configs)) = cached.as_ref()
            && *found == key
        {
            return Arc::clone(configs);
        }
        let configs = Arc::new(ConfigSet::new(inputs));
        *cached = Some((key, Arc::clone(&configs)));
        configs
    }

    fn hash(options: &ExtractOptions) -> SettingsHash {
        SettingsHasher::new("typescript.extract")
            .add(&payload::PAYLOAD_VERSION)
            .add(options)
            .finish()
    }
}

impl Language for TypeScript {
    fn id(&self) -> LanguageId {
        LanguageId::TypeScript
    }

    fn producer(&self) -> Producer {
        Producer {
            name: PRODUCER_NAME,
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        settings.language_enabled(LanguageId::TypeScript) && is_typescript_source(path.as_str())
    }

    fn settings_hash(&self, _path: &RepoPath, _settings: &Settings) -> SettingsHash {
        Self::hash(&self.options)
    }

    fn extract(&self, file: &Snapshot, _settings: &Settings) -> Result<FileFacts, SourceFailure> {
        extract(file, self.options)
    }

    /// Each file's `verbatimModuleSyntax` and target-dependent binding semantics come from its
    /// nearest `tsconfig.json`; they are the only configuration that shapes extracted facts
    /// (module detection and JSX mode apply when linking), so a configuration change re-extracts
    /// exactly the files whose values changed.
    fn extraction_options(
        &self,
        paths: &[&RepoPath],
        _settings: &Settings,
        inputs: &ProjectInputs,
    ) -> Vec<ExtractionOptions> {
        let configs = self.configs(inputs);
        let mut variants: rustc_hash::FxHashMap<ExtractOptions, (SettingsHash, Arc<ExtractOptions>)> =
            rustc_hash::FxHashMap::default();
        paths
            .iter()
            .map(|path| {
                let config = configs.nearest(path);
                let options = ExtractOptions {
                    verbatim_module_syntax: config
                        .options
                        .verbatim_module_syntax
                        .unwrap_or(self.options.verbatim_module_syntax),
                    binding: if config.path.is_some() {
                        config.binding_context()
                    } else {
                        self.options.binding
                    },
                };
                let (hash, options) = variants
                    .entry(options)
                    .or_insert_with(|| (Self::hash(&options), Arc::new(options)));
                ExtractionOptions::with(*hash, Arc::clone(options) as Arc<dyn std::any::Any + Send + Sync>)
            })
            .collect()
    }

    fn extract_with(
        &self,
        file: &Snapshot,
        _settings: &Settings,
        options: &ExtractionOptions,
    ) -> Result<FileFacts, SourceFailure> {
        extract(
            file,
            options.get::<ExtractOptions>().copied().unwrap_or(self.options),
        )
    }

    fn input_problems(&self, inputs: &ProjectInputs) -> Vec<InputProblem> {
        self.configs(inputs).problems().to_vec()
    }

    fn requested_inputs(&self, inputs: &ProjectInputs) -> Vec<(RepoPath, InputRole)> {
        self.configs(inputs)
            .requests()
            .iter()
            .map(|path| (path.clone(), InputRole::Tsconfig))
            .collect()
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        Box::new(link::TypeScriptLinked::new(files, inputs, self.configs(inputs)))
    }

    fn highlighter(&self) -> &dyn Highlighter {
        &self.highlighter
    }
}
