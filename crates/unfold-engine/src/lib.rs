//! The language-neutral engine: the `Language` and `Linked` traits that language front ends
//! implement, discovery and scope, the settings model, and the shared backend (block selection,
//! roles, names, slots, the value policy, reason order, module labels, provenance, edges, argument
//! pairing, type links, entrance assembly, locators, and the launch grammar). `update()` is the single entry point that turns
//! file changes into a new in-memory graph, with the per-file facts cache, relinking, and diffs.

#![forbid(unsafe_code)]

pub mod cancel;
pub mod discovery;
pub mod edges;
pub mod entrances;
pub mod error;
pub mod evidence;
pub mod facts;
pub mod labels;
pub mod language;
pub mod launch;
pub mod locator;
pub mod pairing;
pub mod program;
pub mod provenance;
pub mod resolve;
pub mod select;
pub mod settings;
pub mod update;
pub mod validate;

pub use cancel::{CancelToken, Cancelled};
pub use error::BackendError;
pub use facts::{ENGINE_VERSION, FileFacts, GraphFacts, Invocation};
pub use language::{
    ContextPreview, EmitSource, ExtractionOptions, InputProblem, Language, LanguageId, LanguageProblem,
    LaunchResolution, Linked, Producer, ProjectInput, ProjectInputs, SourceFailure,
};
pub use program::{BlockRef, CallRef, DeclRef, FactsSet, FileId, ProgramFile, TypeRef, ValueRef};
pub use resolve::{Outcome, Resolver};
pub use update::{
    ChangeSummary, Changes, Engine, EngineOptions, FactsCache, LinkBasis, MemoryFactsCache, PreparedUpdate,
    Problem, ProblemKind, Progress, Stage, UpdateError, UpdateKind, UpdateOutcome, UpdateRequest,
    UpdateStats, Verify,
};
