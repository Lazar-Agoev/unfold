//! Half-open byte ranges and one-based line spans.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A range whose start is after its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("range start {start} is after its end {end}")]
pub struct RangeError {
    /// The rejected start.
    pub start: u32,
    /// The rejected end.
    pub end: u32,
}

/// A zero-based, half-open range of UTF-8 byte offsets, `[start, end)`.
///
/// Ranges order by start, then end. They serialize as a two-element array `[start, end]`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteRange {
    /// The first included byte.
    pub start: u32,
    /// The byte immediately after the last included byte.
    pub end: u32,
}

impl ByteRange {
    /// A range from `start` to `end`, if `start <= end`.
    pub const fn new(start: u32, end: u32) -> Result<Self, RangeError> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(RangeError { start, end })
        }
    }

    /// The number of bytes in the range.
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the range has no bytes.
    pub const fn is_empty(self) -> bool {
        self.end <= self.start
    }

    /// Whether `other` lies wholly inside this range (equal ranges contain each other).
    pub const fn contains(self, other: ByteRange) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The range as `usize` bounds, for slicing.
    pub const fn as_usize(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

impl fmt::Debug for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {})", self.start, self.end)
    }
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {})", self.start, self.end)
    }
}

impl Serialize for ByteRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.start, self.end).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ByteRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (start, end) = <(u32, u32)>::deserialize(deserializer)?;
        Self::new(start, end).map_err(serde::de::Error::custom)
    }
}

/// The one-based lines a range starts and ends on. For a nonempty range, `end` is the line of
/// its last byte; for an empty range, both are the line of its start.
///
/// Spans serialize as a two-element array `[start, end]`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineSpan {
    /// The line containing the range's start.
    pub start: u32,
    /// The line containing the range's last byte.
    pub end: u32,
}

impl LineSpan {
    /// Whether this span lies wholly inside `outer`.
    pub const fn within(self, outer: LineSpan) -> bool {
        outer.start <= self.start && self.end <= outer.end
    }
}

impl fmt::Debug for LineSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lines {}-{}", self.start, self.end)
    }
}

impl Serialize for LineSpan {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.start, self.end).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LineSpan {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (start, end) = <(u32, u32)>::deserialize(deserializer)?;
        Ok(Self { start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_containment() {
        let outer = ByteRange::new(10, 50).unwrap();
        assert_eq!(outer.len(), 40);
        assert!(outer.contains(ByteRange::new(10, 50).unwrap()));
        assert!(outer.contains(ByteRange::new(20, 30).unwrap()));
        assert!(!outer.contains(ByteRange::new(5, 30).unwrap()));
        assert!(ByteRange::new(3, 3).unwrap().is_empty());
        assert_eq!(ByteRange::new(4, 3), Err(RangeError { start: 4, end: 3 }));
    }

    #[test]
    fn serializes_as_a_pair() {
        let range = ByteRange::new(512, 1044).unwrap();
        assert_eq!(serde_json::to_string(&range).unwrap(), "[512,1044]");
        assert_eq!(serde_json::from_str::<ByteRange>("[512,1044]").unwrap(), range);
        assert!(serde_json::from_str::<ByteRange>("[9,1]").is_err());
        let lines = LineSpan { start: 25, end: 35 };
        assert_eq!(serde_json::to_string(&lines).unwrap(), "[25,35]");
    }
}
