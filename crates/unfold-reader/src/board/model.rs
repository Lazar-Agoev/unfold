//! The `unfold-board` version 1 document (architecture §9.1, decision U-22).
//!
//! A board holds authored choices only: references to blocks and entrances by structural key,
//! canvas positions, each card's saved view and fold, groups, descriptions, Flow labels, and the
//! guards that keep references from silently reattaching to different code. It never holds
//! copied source or compiled arrows.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use unfold_core::{ContentHash, Key};

/// The `format` value.
pub const FORMAT: &str = "unfold-board";

/// The only `version`.
pub const VERSION: u32 = 1;

/// The JSON Schema of the format.
pub const SCHEMA: &str = include_str!("../../schema/unfold-board.schema.json");

/// A card's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum View {
    /// The exact verified source.
    Code,
    /// Paths from written structure.
    Flow,
}

impl View {
    /// The serialized value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Flow => "flow",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "code" => Some(Self::Code),
            "flow" => Some(Self::Flow),
            _ => None,
        }
    }
}

/// Who wrote a description or label: `person:<name>` or `agent:<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
#[serde(into = "String")]
pub struct Author {
    kind: AuthorKind,
    name: Box<str>,
}

/// Whether a person or an agent wrote something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorKind {
    /// A person, named by their username.
    Person,
    /// An agent, named by itself.
    Agent,
}

impl Author {
    /// A person, by username.
    pub fn person(name: &str) -> Option<Self> {
        Self::new(AuthorKind::Person, name)
    }

    /// An agent, by name.
    pub fn agent(name: &str) -> Option<Self> {
        Self::new(AuthorKind::Agent, name)
    }

    fn new(kind: AuthorKind, name: &str) -> Option<Self> {
        (!name.trim().is_empty() && !name.chars().any(char::is_control)).then(|| Self {
            kind,
            name: name.into(),
        })
    }

    /// Parses `person:<name>` or `agent:<name>`.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(name) = text.strip_prefix("person:") {
            Self::person(name)
        } else {
            text.strip_prefix("agent:").and_then(Self::agent)
        }
    }

    /// Whether a person or an agent.
    pub fn kind(&self) -> AuthorKind {
        self.kind
    }

    /// The username or agent name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.kind {
            AuthorKind::Person => "person:",
            AuthorKind::Agent => "agent:",
        };
        write!(f, "{prefix}{}", self.name)
    }
}

impl From<Author> for String {
    fn from(author: Author) -> Self {
        author.to_string()
    }
}

/// `sha256:` and the hash of the exact selected source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
#[serde(into = "String")]
pub struct Signature(pub ContentHash);

impl Signature {
    /// The signature of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(ContentHash::of(bytes))
    }

    /// Parses `sha256:<64 lowercase hex digits>`.
    pub fn parse(text: &str) -> Option<Self> {
        text.strip_prefix("sha256:")
            .and_then(|hex| ContentHash::from_hex(hex).ok())
            .map(Self)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", self.0)
    }
}

impl From<Signature> for String {
    fn from(signature: Signature) -> Self {
        signature.to_string()
    }
}

/// A card or group description, or a Flow label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Description {
    /// The text; never blank.
    pub text: String,
    /// Who wrote it.
    pub author: Author,
    /// The signature of the source it describes; `None` until a save signs it from verified
    /// source (written as `""`).
    #[serde(serialize_with = "signature_or_empty")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub signature: Option<Signature>,
}

fn signature_or_empty<S: serde::Serializer>(
    signature: &Option<Signature>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match signature {
        Some(signature) => serializer.collect_str(signature),
        None => serializer.serialize_str(""),
    }
}

/// What a card shows: exactly one block or entrance, by structural key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum CardRef {
    /// A block (a definition or another indexed block).
    Block(Key),
    /// A declared entrance.
    Entrance(Key),
}

impl CardRef {
    /// The referenced key.
    pub fn key(&self) -> &Key {
        match self {
            Self::Block(key) | Self::Entrance(key) => key,
        }
    }

    /// The block key, for a block card.
    pub fn block(&self) -> Option<&Key> {
        match self {
            Self::Block(key) => Some(key),
            Self::Entrance(_) => None,
        }
    }
}

/// One card on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Unique among cards and groups.
    pub id: String,
    /// What it shows.
    #[serde(rename = "ref")]
    pub reference: CardRef,
    /// The canvas position: finite numbers.
    pub at: [f64; 2],
    /// The card's own Code/Flow switch; the board's default when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub view: Option<View>,
    /// Folded to its title (written only when true).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub folded: bool,
    /// The card's description.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<Description>,
}

/// A run of consecutive sibling statements inside one card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Unique among cards and groups.
    pub id: String,
    /// The card it belongs to.
    pub card: String,
    /// Its title on the rail.
    pub title: String,
    /// The member statements' keys, in source order.
    pub members: Vec<Key>,
    /// Whether the group is collapsed.
    pub collapsed: bool,
    /// The group's description.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<Description>,
}

/// What a reference was when it was saved, so it can reconnect safely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Guard {
    /// The block kind, or `entrance:<kind>`.
    pub kind: String,
    /// The owner's key for a block; empty for an entrance.
    pub owner: String,
    /// The signature of the referenced source.
    pub signature: Signature,
}

/// An `unfold-board` version 1 document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// The title; never blank.
    pub title: String,
    /// The view of cards without their own.
    pub default_view: View,
    /// The cards.
    pub cards: Vec<Card>,
    /// The groups.
    pub groups: Vec<Group>,
    /// Flow-node labels by block key.
    pub labels: BTreeMap<String, Description>,
    /// Guards by referenced key.
    pub guards: BTreeMap<String, Guard>,
}

impl Board {
    /// An empty board.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            default_view: View::Code,
            cards: Vec::new(),
            groups: Vec::new(),
            labels: BTreeMap::new(),
            guards: BTreeMap::new(),
        }
    }

    /// The card with `id`.
    pub fn card(&self, id: &str) -> Option<&Card> {
        self.cards.iter().find(|card| card.id == id)
    }

    /// The group with `id`.
    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }

    /// A card's effective view.
    pub fn view_of(&self, card: &Card) -> View {
        card.view.unwrap_or(self.default_view)
    }

    /// The first card that shows block `key`.
    pub fn card_for_block(&self, key: &str) -> Option<&Card> {
        self.cards
            .iter()
            .find(|card| card.reference.block().is_some_and(|block| block.as_str() == key))
    }

    /// Every key the board references (cards, group members, labels), in that order, without
    /// duplicates.
    pub fn references(&self) -> Vec<&str> {
        let mut seen = rustc_hash::FxHashSet::default();
        self.cards
            .iter()
            .map(|card| card.reference.key().as_str())
            .chain(
                self.groups
                    .iter()
                    .flat_map(|group| group.members.iter().map(Key::as_str)),
            )
            .chain(self.labels.keys().map(String::as_str))
            .filter(|key| seen.insert(*key))
            .collect()
    }

    /// The authored label of block `key`, when it has text.
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels
            .get(key)
            .map(|label| label.text.trim())
            .filter(|text| !text.is_empty())
    }
}

/// Whether a key is position-based: it contains an ordinal step (`if[0]`, `function:f[1]`),
/// which shifts when code is inserted before it.
pub fn is_ordinal(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut at = 0;
    while let Some(open) = bytes[at..].iter().position(|&byte| byte == b'[') {
        let start = at + open + 1;
        let digits = bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let close = start + digits;
        if digits > 0 && bytes.get(close) == Some(&b']') && matches!(bytes.get(close + 1), None | Some(b'/'))
        {
            return true;
        }
        at = start;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_and_signatures_parse_strictly() {
        assert_eq!(Author::parse("person:ada").unwrap().to_string(), "person:ada");
        assert_eq!(Author::parse("agent:codex").unwrap().kind(), AuthorKind::Agent);
        for bad in ["person:", "agent: ", "ada", "robot:x", "person:a\nb"] {
            assert!(Author::parse(bad).is_none(), "{bad}");
        }
        let signature = Signature::of(b"x");
        assert_eq!(Signature::parse(&signature.to_string()), Some(signature));
        assert!(Signature::parse("sha256:ABC").is_none());
        assert!(Signature::parse("md5:00").is_none());
    }

    #[test]
    fn ordinal_keys() {
        assert!(is_ordinal("a.ts#function:f/if[0]"));
        assert!(is_ordinal("a.ts#function:f[1]/return[0]"));
        assert!(is_ordinal("a.ts#function:f/if[12]/call[3]"));
        assert!(!is_ordinal("a.ts#function:f"));
        assert!(!is_ordinal("a.ts#variable:a%5B0%5D"));
        assert!(!is_ordinal("a.ts#function:f/variable:x[y]"));
    }
}
