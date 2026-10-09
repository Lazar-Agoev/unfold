//! The Python front end's private per-file facts, cached with the language-neutral graph facts.
//!
//! It holds the census (token boundaries, call sites, module dependencies), the written form of
//! every runtime import (aligned with the file's
//! [`ImportFact`](unfold_engine::facts::ImportFact)s), which import resolution reads, and the
//! binder's facts ([`BindFacts`]). [`PAYLOAD_VERSION`] is part of the facts-cache key through the
//! producer version.

use serde::{Deserialize, Serialize};
use unfold_core::text::ByteRange;

use crate::bind::ir::{BindFacts, NodeId};
use crate::census::{CallSite, delta_decode, delta_encode};

/// The payload's format version.
pub const PAYLOAD_VERSION: u32 = 4;

/// Why a payload cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayloadError {
    /// The bytes are not a payload of this version.
    #[error("cannot decode the Python facts payload: {0}")]
    Decode(String),
    /// The payload could not be encoded.
    #[error("cannot encode the Python facts payload: {0}")]
    Encode(String),
}

/// One file's private facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    /// [`PAYLOAD_VERSION`] when written.
    pub version: u32,
    /// What validation compares the file's blocks with.
    pub census: CensusData,
    /// The written form of each runtime import, in block order.
    pub imports: Vec<ImportSyntax>,
    /// The binder's facts.
    pub bind: BindFacts,
}

/// The census, compactly: offsets are stored as gaps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CensusData {
    starts: Vec<u32>,
    ends: Vec<u32>,
    /// Call sites from the token scan (dynamic imports excluded), sorted by range.
    pub calls: Vec<CallSite>,
    /// The range of every runtime module dependency, sorted. Conditional child imports are not
    /// here: linking adds the ones it keeps.
    pub dependencies: Vec<ByteRange>,
}

impl CensusData {
    /// The census from sorted token starts and ends, call sites, and dependency ranges.
    pub(crate) fn new(
        starts: &[u32],
        ends: &[u32],
        calls: Vec<CallSite>,
        dependencies: Vec<ByteRange>,
    ) -> Self {
        Self {
            starts: delta_encode(starts),
            ends: delta_encode(ends),
            calls,
            dependencies,
        }
    }

    /// The sorted token starts.
    pub fn starts(&self) -> Vec<u32> {
        delta_decode(&self.starts)
    }

    /// The sorted token ends.
    pub fn ends(&self) -> Vec<u32> {
        delta_decode(&self.ends)
    }
}

/// How a runtime import is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportSyntax {
    /// One item of `import a.b` or `import a.b as c`.
    Module {
        /// The dotted module name (NFKC).
        module: Box<str>,
        /// The `as` name, if written (NFKC).
        alias: Option<Box<str>>,
    },
    /// `from <dots><module> import names`.
    From {
        /// The number of leading dots.
        level: u32,
        /// The module after the dots, if any (NFKC).
        module: Option<Box<str>>,
        /// The imported names with their `as` names; `*` for a star import.
        names: Vec<(Box<str>, Option<Box<str>>)>,
    },
    /// A proven `importlib.import_module("literal")`.
    Dynamic,
    /// A name of a `from` import that may import a child module (a conditional block).
    Child {
        /// The index (among this file's imports) of the `from` import it belongs to.
        from: u32,
        /// The imported name (NFKC).
        name: Box<str>,
        /// The name's declaration in the binder facts.
        node: NodeId,
    },
}

impl Payload {
    /// The compact encoding.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PayloadError> {
        postcard::to_allocvec(self).map_err(|error| PayloadError::Encode(error.to_string()))
    }

    /// Decodes a payload, rejecting other versions.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PayloadError> {
        let payload: Self =
            postcard::from_bytes(bytes).map_err(|error| PayloadError::Decode(error.to_string()))?;
        if payload.version != PAYLOAD_VERSION {
            return Err(PayloadError::Decode(format!(
                "payload version {} is not {PAYLOAD_VERSION}",
                payload.version
            )));
        }
        Ok(payload)
    }
}
