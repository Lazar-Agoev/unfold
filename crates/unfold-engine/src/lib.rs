//! The language-neutral engine: the `Language` and `Linked` traits that language front ends
//! implement, discovery and scope, the settings model, and the shared backend (the per-file
//! facts, the value policy and its reason order, module labels, provenance, links and their
//! outcomes, and argument pairing). `update()` is the single entry point that turns file changes
//! into a new in-memory graph, with the per-file facts cache, relinking, diffs, and the apps and
//! entrances `unfold.toml` declares, resolved against the graph.

#![forbid(unsafe_code)]

pub mod cancel;
pub mod discovery;
pub mod entrances;
pub mod error;
pub mod evidence;
pub mod facts;
pub mod labels;
pub mod language;
pub mod links;
pub mod pairing;
pub mod program;
pub mod provenance;
pub mod resolve;
pub mod settings;
pub mod update;
pub mod validate;

pub use cancel::{CancelToken, Cancelled};
pub use error::BackendError;
pub use facts::{ENGINE_VERSION, FileFacts, GraphFacts};
pub use language::{
    ExtractionOptions, InputProblem, Language, LanguageId, LanguageProblem, Linked, Producer, ProjectInput,
    ProjectInputs, SourceFailure,
};
pub use program::{CallRef, DeclRef, FactsSet, FileId, ImportRef, NodeRef, ProgramFile, TypeRef, ValueRef};
pub use resolve::{Outcome, Resolver};
pub use update::{
    ChangeSummary, Changes, Engine, EngineOptions, EntrancesError, EntrancesOutcome, FactsCache, LinkBasis,
    MemoryFactsCache, PreparedUpdate, Problem, ProblemKind, Progress, Stage, UpdateError, UpdateKind,
    UpdateOutcome, UpdateRequest, UpdateStats, Verify,
};
