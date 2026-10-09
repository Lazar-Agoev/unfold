//! SHA-256 content hashes.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// The SHA-256 of a file's original bytes. It displays and serializes (in self-describing
/// formats) as 64 lowercase hex digits, and as 32 raw bytes in compact formats.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct ContentHash([u8; 32]);

/// A string that is not 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected 64 lowercase hex digits, found `{0}`")]
pub struct HashParseError(pub Box<str>);

impl ContentHash {
    /// Hashes `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Wraps a digest computed elsewhere.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses 64 lowercase hex digits.
    pub fn from_hex(text: &str) -> Result<Self, HashParseError> {
        let error = || HashParseError(text.into());
        let digits = text.as_bytes();
        if digits.len() != 64 {
            return Err(error());
        }
        let mut bytes = [0u8; 32];
        let (pairs, _) = digits.as_chunks::<2>();
        for (byte, &[high, low]) in bytes.iter_mut().zip(pairs) {
            let high = hex_value(high).ok_or_else(error)?;
            let low = hex_value(low).ok_or_else(error)?;
            *byte = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

const fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut text = [0u8; 64];
        for (pair, byte) in text.as_chunks_mut::<2>().0.iter_mut().zip(self.0) {
            pair[0] = DIGITS[usize::from(byte >> 4)];
            pair[1] = DIGITS[usize::from(byte & 0x0f)];
        }
        // Every byte written above is an ASCII hex digit.
        f.write_str(std::str::from_utf8(&text).map_err(|_| fmt::Error)?)
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

impl FromStr for ContentHash {
    type Err = HashParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_hex(text)
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
            Self::from_hex(&text).map_err(serde::de::Error::custom)
        } else {
            <[u8; 32]>::deserialize(deserializer).map(Self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_vectors() {
        assert_eq!(
            ContentHash::of(b"").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            ContentHash::of(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hex_round_trips_and_rejects_bad_input() {
        let hash = ContentHash::of(b"unfold");
        assert_eq!(ContentHash::from_hex(&hash.to_string()), Ok(hash));
        assert!(ContentHash::from_hex("abc").is_err());
        assert!(ContentHash::from_hex(&hash.to_string().to_uppercase()).is_err());
        assert!(ContentHash::from_hex(&"g".repeat(64)).is_err());
    }

    #[test]
    fn serializes_as_hex_or_bytes() {
        let hash = ContentHash::of(b"x");
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, format!("\"{hash}\""));
        assert_eq!(serde_json::from_str::<ContentHash>(&json).unwrap(), hash);
        let packed = postcard::to_allocvec(&hash).unwrap();
        assert_eq!(packed.len(), 32);
        assert_eq!(postcard::from_bytes::<ContentHash>(&packed).unwrap(), hash);
    }
}
