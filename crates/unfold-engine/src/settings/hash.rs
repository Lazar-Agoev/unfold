//! Settings fingerprints for cache keys.

use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// A SHA-256 fingerprint of some settings. The per-file facts cache is keyed by path, content
/// hash, engine version, and the settings hash the file's language computes, so equal hashes
/// mean the settings that matter to that file are equal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
pub struct SettingsHash([u8; 32]);

impl SettingsHash {
    /// The raw digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Wraps a digest computed elsewhere, such as one read back from the cache.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Display for SettingsHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for SettingsHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SettingsHash({self})")
    }
}

/// Builds a [`SettingsHash`] from a domain name and any number of serializable parts.
///
/// Each part is written as length-prefixed canonical JSON. Settings types serialize with a fixed
/// field order and sorted collections, so the bytes depend only on the values. The domain keeps
/// hashes of different kinds of settings apart even when their values serialize alike.
pub struct SettingsHasher {
    digest: Sha256,
}

impl SettingsHasher {
    /// Starts a hash for `domain`, such as `"python.project"`.
    pub fn new(domain: &str) -> Self {
        let mut hasher = Self {
            digest: Sha256::new(),
        };
        hasher.write(domain.as_bytes());
        hasher
    }

    /// Adds one part.
    pub fn add<T: Serialize + ?Sized>(&mut self, part: &T) -> &mut Self {
        // Serializing plain data (strings, numbers, sequences, string-keyed structs) to JSON
        // cannot fail; a failure would mean a settings type with a non-string map key.
        let bytes = serde_json::to_vec(part).expect("settings parts serialize to JSON");
        self.write(&bytes);
        self
    }

    /// Adds raw bytes, such as a configuration input's content hash.
    pub fn add_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.write(bytes);
        self
    }

    /// The finished hash.
    pub fn finish(&self) -> SettingsHash {
        SettingsHash(self.digest.clone().finalize().into())
    }

    fn write(&mut self, bytes: &[u8]) {
        self.digest.update((bytes.len() as u64).to_le_bytes());
        self.digest.update(bytes);
    }
}
