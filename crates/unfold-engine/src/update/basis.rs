//! What a graph was linked from (docs/engine.md#project-lifecycle): kept with the stored graph so that the
//! reopen check can trust a restored graph without linking it again when nothing it was linked
//! from changed while the folder was closed, so that a settings file that stopped parsing
//! while it was closed leaves the last good settings and apps in effect, and so that a module
//! some `code` names is still recognized after it moved (docs/engine.md#resolving-code).

use serde::{Deserialize, Serialize};
use unfold_core::ContentHash;

use super::run::configuration;
use super::scan::parse_settings;
use super::state::{SettingsState, State};
use super::{Engine, Problem, ProblemKind};
use crate::ENGINE_VERSION;
use crate::entrances::Modules;
use crate::settings::SettingsHasher;

/// What a graph was linked from besides its files' facts, what linking found, the settings
/// file in effect, and the modules the apps name as last seen. Save it with the graph
/// ([`LinkBasis::encode`]) and hand it back to [`Engine::restore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkBasis {
    /// A fingerprint of everything but the files' facts that linking reads: the engine and
    /// front-end versions, the folder, the effective settings (not the settings file's bytes,
    /// so not its `[[app]]` tables), every configuration input (not the evidence files), and
    /// the unindexed and context files.
    fingerprint: ContentHash,
    /// The problems linking found, in order.
    problems: Vec<LinkProblem>,
    /// The text of `unfold.toml` the settings and apps came from: the last that parsed, or
    /// `None` for the defaults.
    settings: Option<Box<str>>,
    /// The top-level definitions of the modules some `code` names, as each was last seen, for
    /// the moved-module fix.
    modules: Modules,
}

/// One problem linking found ([`crate::Linked::problems`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LinkProblem {
    path: Box<str>,
    message: Box<str>,
    /// Whether it makes the graph incomplete.
    incomplete: bool,
}

impl LinkBasis {
    /// The basis of `state`'s graph, an update of `engine`.
    pub(super) fn of(engine: &Engine, state: &State) -> Self {
        Self {
            fingerprint: fingerprint(engine, state),
            problems: state
                .link_problems
                .iter()
                .map(|(problem, incomplete)| LinkProblem {
                    path: problem.path.clone(),
                    message: problem.message.clone(),
                    incomplete: *incomplete,
                })
                .collect(),
            settings: state.settings.source.as_ref().map(|source| source.text().into()),
            modules: state.modules.clone(),
        }
    }

    /// Whether `state`, an update of `engine`, sees exactly what this graph was linked from.
    pub(super) fn matches(&self, engine: &Engine, state: &State) -> bool {
        self.fingerprint == fingerprint(engine, state)
    }

    /// The problems linking found, as the state keeps them.
    pub(super) fn problems(&self) -> Vec<(Problem, bool)> {
        self.problems
            .iter()
            .map(|problem| {
                let listed = Problem {
                    kind: ProblemKind::Input,
                    path: problem.path.clone(),
                    message: problem.message.clone(),
                };
                (listed, problem.incomplete)
            })
            .collect()
    }

    /// The settings and apps in effect for the graph, parsed again from the text of
    /// `unfold.toml` they came from; the defaults when they didn't come from one.
    pub(super) fn settings(&self) -> SettingsState {
        self.settings
            .as_deref()
            .and_then(|text| parse_settings(text.as_bytes().to_vec(), None).ok())
            .unwrap_or_default()
    }

    /// The modules the apps named, as last seen.
    pub(super) fn modules(&self) -> Modules {
        self.modules.clone()
    }

    /// The text the store keeps.
    pub fn encode(&self) -> String {
        // Plain strings, booleans, a hash, and maps with string keys: serializing can't fail.
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Reads [`LinkBasis::encode`]'s text back; `None` when it isn't one (the graph is then
    /// linked again).
    pub fn decode(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

/// See [`LinkBasis::fingerprint`]. Discovery's sets and maps iterate in path order, so equal
/// inputs give equal fingerprints.
fn fingerprint(engine: &Engine, state: &State) -> ContentHash {
    let mut hasher = SettingsHasher::new("link-basis");
    hasher.add(ENGINE_VERSION).add(&engine.root_text);
    for language in &engine.languages {
        let producer = language.producer();
        hasher
            .add(language.id().as_str())
            .add(producer.name)
            .add(producer.version);
    }
    hasher.add_bytes(state.settings.settings.hash().as_bytes());
    for (path, input) in configuration(&state.inputs) {
        let content = input.content.as_ref().ok().map(|snapshot| snapshot.hash());
        hasher.add(path).add(&input.role).add(&content);
    }
    hasher.add(&state.unindexed).add(&state.context);
    ContentHash::from_bytes(*hasher.finish().as_bytes())
}
