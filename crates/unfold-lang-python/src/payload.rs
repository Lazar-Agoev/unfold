//! The Python front end's private per-file facts, cached with the language-neutral graph facts.
//!
//! It holds the census's call sites, the written form of
//! every runtime import (aligned with the file's
//! [`ImportFact`](unfold_engine::facts::ImportFact)s), which import resolution reads, and the
//! binder's facts ([`BindFacts`]), whose `values` and `types` tables give each value and each
//! written type name its expression. Any change to this format, or to anything else that shapes
//! this crate's extracted facts, must bump [`PAYLOAD_VERSION`], which is part of the settings
//! hash and so of the facts-cache key. A cached payload linking can't [`read`](Payload::read) is
//! a cache miss too.

use serde::{Deserialize, Serialize};
use unfold_core::text::ByteRange;

use crate::bind::ir::{BindFacts, NodeId};

/// The version of Python extraction: the payload format and every rule that shapes facts.
pub const PAYLOAD_VERSION: u32 = 12;

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
    /// Call sites from the token scan, sorted, which validation compares the file's call nodes
    /// with; those inside a truncated block are left out.
    pub calls: Vec<ByteRange>,
    /// The written form of each runtime import, aligned with the file's import facts.
    pub imports: Vec<ImportSyntax>,
    /// The binder's facts.
    pub bind: BindFacts,
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
    /// A name of a `from` import that may import a child module (a conditional import).
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

    /// Decodes a payload linking can read: of this version, with consistent binder facts
    /// ([`BindFacts::is_consistent`]). Facts come back from the on-disk cache, where a damaged
    /// entry may still decode.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        Self::from_bytes(bytes)
            .ok()
            .filter(|payload| payload.bind.is_consistent())
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
