//! Structural keys: names for blocks and entrances that survive most edits.
//!
//! | Key part | Derivation |
//! | --- | --- |
//! | File root | `path#` |
//! | Named step | `kind:name`, with `%`, `/`, `#`, `[`, `]` percent-escaped and `[n]` for the n-th (n ≥ 1) earlier sibling of the same kind and name |
//! | Unnamed step | `kind[n]`: the zero-based position among siblings of that kind, named or not |
//! | Nested block | The parent key, then `/step` (no `/` directly after the file's `#`) |
//! | Entrance | `entrance:` plus a JSON array of detector, kind, method, trigger, and first target text, with `#n` for the n-th (n ≥ 1) earlier entrance with the same array |
//! | Configured entrance | `entrance:configured:` plus the `[[entrance]]` table's `id`, whatever else the table says |
//!
//! These rules reproduce the proof of concept's keys exactly, so boards and references carry
//! over unchanged.

use std::borrow::Borrow;
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::path::RepoPath;
use crate::vocab::{BlockKind, DetectorId, EntranceKind, HttpMethod};

/// A structural key. Cloning is cheap (a reference count).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct Key(Arc<str>);

impl Key {
    /// Wraps key text read from storage or an export. The text is not checked; the validator
    /// compares stored keys with freshly derived ones.
    pub fn from_text(text: &str) -> Self {
        Self(Arc::from(text))
    }

    /// The key of `path`'s file root: `path#`.
    pub fn file_root(path: &RepoPath) -> Self {
        let mut text = String::with_capacity(path.as_str().len() + 1);
        text.push_str(path.as_str());
        text.push('#');
        Self(Arc::from(text))
    }

    /// The key's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is an entrance key.
    pub fn is_entrance(&self) -> bool {
        self.0.starts_with(ENTRANCE_PREFIX)
    }

    /// The key of the configured entrance with `id`: `entrance:configured:<id>`.
    pub fn configured_entrance(id: &str) -> Self {
        Self(Arc::from(format!("{CONFIGURED_PREFIX}{id}")))
    }

    /// The ID of a configured entrance's key.
    pub fn configured_id(&self) -> Option<&str> {
        self.0.strip_prefix(CONFIGURED_PREFIX)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Key {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Key {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Ok(Self::from_text(&text))
    }
}

/// Appends `name` to `out` with the step's reserved characters (`%`, `/`, `#`, `[`, `]`)
/// percent-encoded in uppercase hex, as JavaScript's `encodeURIComponent` writes them.
pub fn escape_name(name: &str, out: &mut String) {
    for character in name.chars() {
        match character {
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            '#' => out.push_str("%23"),
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            _ => out.push(character),
        }
    }
}

/// Why a block's key cannot be derived.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The first block of a file must be its `file` root, with no parent.
    #[error("block 0 must be the file root with no parent")]
    MissingRoot,
    /// Only the first block of a file may be a `file` root, and every other block has a parent.
    #[error("block {local_id} is a second root")]
    SecondRoot {
        /// The offending block.
        local_id: u32,
    },
    /// A parent must come before its children in preorder.
    #[error("block {local_id} names parent {parent}, which does not precede it")]
    ParentNotBefore {
        /// The offending block.
        local_id: u32,
        /// Its claimed parent.
        parent: u32,
    },
}

/// Assigns keys to one file's blocks, given in preorder.
///
/// Sibling ordinals count only siblings already pushed, which in preorder are exactly the
/// earlier siblings.
#[derive(Debug)]
pub struct KeyBuilder {
    root: Key,
    keys: Vec<Key>,
    /// Siblings of each kind under each parent, named or not.
    kinds: FxHashMap<(u32, BlockKind), u32>,
    /// Siblings with the same kind and name, keyed by the step's ordinal-free key.
    names: FxHashMap<Key, u32>,
}

impl KeyBuilder {
    /// Starts a file at `path`.
    pub fn new(path: &RepoPath) -> Self {
        Self {
            root: Key::file_root(path),
            keys: Vec::new(),
            kinds: FxHashMap::default(),
            names: FxHashMap::default(),
        }
    }

    /// Derives the key of the next block in preorder. `name` is the block's name; an empty
    /// name counts as no name.
    pub fn push(
        &mut self,
        parent: Option<u32>,
        kind: BlockKind,
        name: Option<&str>,
    ) -> Result<Key, KeyError> {
        let local_id = u32::try_from(self.keys.len()).unwrap_or(u32::MAX);
        let key = match parent {
            None if local_id == 0 && kind == BlockKind::File => self.root.clone(),
            None if local_id == 0 => return Err(KeyError::MissingRoot),
            Some(_) if local_id == 0 => return Err(KeyError::MissingRoot),
            None => return Err(KeyError::SecondRoot { local_id }),
            Some(_) if kind == BlockKind::File => return Err(KeyError::SecondRoot { local_id }),
            Some(parent) if parent >= local_id => return Err(KeyError::ParentNotBefore { local_id, parent }),
            Some(parent) => self.child_key(parent, kind, name.filter(|name| !name.is_empty())),
        };
        self.keys.push(key.clone());
        Ok(key)
    }

    fn child_key(&mut self, parent: u32, kind: BlockKind, name: Option<&str>) -> Key {
        let parent_key = self.keys[parent as usize].as_str();
        let mut text = String::with_capacity(parent_key.len() + kind.as_str().len() + 8);
        text.push_str(parent_key);
        if !parent_key.ends_with('#') {
            text.push('/');
        }
        text.push_str(kind.as_str());
        let kind_count = self.kinds.entry((parent, kind)).or_insert(0);
        let kind_ordinal = *kind_count;
        *kind_count += 1;
        let Some(name) = name else {
            // Writing to a String cannot fail.
            let _ = write!(text, "[{kind_ordinal}]");
            return Key(Arc::from(text));
        };
        text.push(':');
        escape_name(name, &mut text);
        if let Some(count) = self.names.get_mut(text.as_str()) {
            let _ = write!(text, "[{count}]");
            *count += 1;
            Key(Arc::from(text))
        } else {
            let key = Key(Arc::from(text));
            self.names.insert(key.clone(), 1);
            key
        }
    }

    /// The keys derived so far, indexed by local ID.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Finishes the file and returns its keys, indexed by local ID.
    pub fn finish(self) -> Vec<Key> {
        self.keys
    }
}

const ENTRANCE_PREFIX: &str = "entrance:";
const CONFIGURED_PREFIX: &str = "entrance:configured:";

/// What an entrance contributes to its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntranceShape<'a> {
    /// The detector that produced the entrance.
    pub detector: DetectorId,
    /// The entrance's kind.
    pub kind: EntranceKind,
    /// The HTTP method, if any.
    pub method: Option<HttpMethod>,
    /// The trigger, if known.
    pub trigger: Option<&'a str>,
    /// The text of the entrance's first target, if it has one.
    pub first_target: Option<&'a str>,
    /// The ID of a configured entrance, which alone makes its key.
    pub configured: Option<&'a str>,
}

/// The ordinal-free key of one entrance: `entrance:configured:<id>` for a configured entrance,
/// otherwise `entrance:` and a compact JSON array of detector, kind, method, trigger, and first
/// target text, with absent values as empty strings.
pub fn entrance_key_base(shape: &EntranceShape<'_>) -> String {
    if let Some(id) = shape.configured {
        return format!("{CONFIGURED_PREFIX}{id}");
    }
    let parts = [
        shape.detector.as_str(),
        shape.kind.as_str(),
        shape.method.map_or("", HttpMethod::as_str),
        shape.trigger.unwrap_or(""),
        shape.first_target.unwrap_or(""),
    ];
    let mut text =
        String::with_capacity(ENTRANCE_PREFIX.len() + parts.iter().map(|p| p.len() + 3).sum::<usize>() + 2);
    text.push_str(ENTRANCE_PREFIX);
    text.push('[');
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        push_json_string(part, &mut text);
    }
    text.push(']');
    text
}

/// Derives the keys of a graph's entrances, given in their canonical order: the n-th (n ≥ 1)
/// entrance whose base repeats an earlier one gets `#n`.
pub fn entrance_keys<'a>(entrances: impl IntoIterator<Item = EntranceShape<'a>>) -> Vec<Key> {
    // Bases seen so far, keyed by the first entrance's key (which is the base itself), so a
    // unique base costs one allocation.
    let mut seen: FxHashMap<Key, u32> = FxHashMap::default();
    entrances
        .into_iter()
        .map(|shape| {
            let mut base = entrance_key_base(&shape);
            if let Some(count) = seen.get_mut(base.as_str()) {
                let _ = write!(base, "#{count}");
                *count += 1;
                Key(Arc::from(base))
            } else {
                let key = Key(Arc::from(base));
                seen.insert(key.clone(), 1);
                key
            }
        })
        .collect()
}

/// Appends `value` as a JSON string, escaped exactly as JavaScript's `JSON.stringify` escapes
/// it: `"` and `\` with a backslash, the short escapes for backspace, form feed, newline,
/// carriage return, and tab, and every other control character as `\u00xx` in lowercase hex.
/// Everything else, including non-ASCII, is written as is.
fn push_json_string(value: &str, out: &mut String) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            _ => out.push(character),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use BlockKind as K;

    /// What a block contributes to its key.
    #[derive(Debug, Clone, Copy)]
    struct BlockShape<'a> {
        parent: Option<u32>,
        kind: BlockKind,
        name: Option<&'a str>,
    }

    /// Derives the keys of one file's blocks, given in preorder.
    fn block_keys<'a>(
        path: &RepoPath,
        blocks: impl IntoIterator<Item = BlockShape<'a>>,
    ) -> Result<Vec<Key>, KeyError> {
        let mut builder = KeyBuilder::new(path);
        for block in blocks {
            builder.push(block.parent, block.kind, block.name)?;
        }
        Ok(builder.finish())
    }

    fn path(text: &str) -> RepoPath {
        RepoPath::new(text).unwrap()
    }

    fn shape(parent: Option<u32>, kind: BlockKind, name: &str) -> BlockShape<'_> {
        BlockShape {
            parent,
            kind,
            name: (!name.is_empty()).then_some(name),
        }
    }

    /// Blocks of `ts-playground/src/lib/validate.ts` from the proof of concept's frozen
    /// generation (local IDs are the generation's `b1171`... renumbered from zero), with the
    /// keys its reader derives.
    #[test]
    fn matches_the_proof_of_concept_on_validate_ts() {
        let blocks = [
            (None, K::File, ""),
            (Some(0), K::Function, "validateOrder"),
            (Some(1), K::If, ""),
            (Some(2), K::Throw, ""),
            (Some(3), K::New, ""),
            (Some(1), K::Variable, ""),
            (Some(5), K::Variable, "customerId"),
            (Some(5), K::Variable, "email"),
            (Some(1), K::If, ""),
            (Some(8), K::Call, ""),
            (Some(1), K::Variable, "validatedItems"),
            (Some(10), K::Call, ""),
            (Some(11), K::Function, ""),
            (Some(1), K::Return, ""),
            (Some(13), K::Call, ""),
            (Some(13), K::Call, ""),
        ];
        let keys = block_keys(
            &path("ts-playground/src/lib/validate.ts"),
            blocks
                .iter()
                .map(|&(parent, kind, name)| shape(parent, kind, name)),
        )
        .unwrap();
        let expected = [
            "ts-playground/src/lib/validate.ts#",
            "ts-playground/src/lib/validate.ts#function:validateOrder",
            "ts-playground/src/lib/validate.ts#function:validateOrder/if[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/if[0]/throw[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/if[0]/throw[0]/new[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable[0]/variable:customerId",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable[0]/variable:email",
            "ts-playground/src/lib/validate.ts#function:validateOrder/if[1]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/if[1]/call[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable:validatedItems",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable:validatedItems/call[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/variable:validatedItems/call[0]/function[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/return[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/return[0]/call[0]",
            "ts-playground/src/lib/validate.ts#function:validateOrder/return[0]/call[1]",
        ];
        let keys: Vec<&str> = keys.iter().map(Key::as_str).collect();
        assert_eq!(keys, expected);
    }

    /// Duplicate names in `py-playground/src/orders_playground/features/decorators.py`.
    #[test]
    fn numbers_duplicate_names_like_the_proof_of_concept() {
        let blocks = [
            (None, K::File, ""),
            (Some(0), K::Import, ""),
            (Some(0), K::Function, "normalize_sku"),
            (Some(0), K::Function, "stringify"),
            (Some(3), K::ExpressionStatement, ""),
            (Some(0), K::Function, "stringify"),
            (Some(0), K::Function, "stringify"),
            (Some(0), K::Class, "Named"),
        ];
        let keys = block_keys(
            &path("py-playground/src/orders_playground/features/decorators.py"),
            blocks
                .iter()
                .map(|&(parent, kind, name)| shape(parent, kind, name)),
        )
        .unwrap();
        let prefix = "py-playground/src/orders_playground/features/decorators.py#";
        let suffixes: Vec<&str> = keys.iter().map(|key| &key.as_str()[prefix.len()..]).collect();
        assert_eq!(
            suffixes,
            [
                "",
                "import[0]",
                "function:normalize_sku",
                "function:stringify",
                "function:stringify/expression_statement[0]",
                "function:stringify[1]",
                "function:stringify[2]",
                "class:Named"
            ]
        );
    }

    #[test]
    fn escapes_reserved_characters_like_the_proof_of_concept() {
        let mut builder = KeyBuilder::new(&path("py-playground/src/orders_playground/db/repository.py"));
        builder.push(None, K::File, None).unwrap();
        builder.push(Some(0), K::Class, Some("OrderRepository")).unwrap();
        builder.push(Some(1), K::Method, Some("insert")).unwrap();
        let key = builder
            .push(Some(2), K::Variable, Some("self.orders[order[\"id\"]]"))
            .unwrap();
        assert_eq!(
            key.as_str(),
            "py-playground/src/orders_playground/db/repository.py#class:OrderRepository/method:insert/variable:self.orders%5Border%5B\"id\"%5D%5D"
        );
        let mut out = String::new();
        escape_name("a%b/c#d[e]f:g é", &mut out);
        assert_eq!(out, "a%25b%2Fc%23d%5Be%5Df:g é");
    }

    #[test]
    fn unnamed_ordinals_count_named_siblings_of_the_same_kind() {
        let keys = block_keys(
            &path("a.ts"),
            [
                shape(None, K::File, ""),
                shape(Some(0), K::Function, "named"),
                shape(Some(0), K::Function, ""),
                shape(Some(0), K::Variable, ""),
                shape(Some(0), K::Function, "named"),
            ],
        )
        .unwrap();
        let keys: Vec<&str> = keys.iter().map(Key::as_str).collect();
        assert_eq!(
            keys,
            [
                "a.ts#",
                "a.ts#function:named",
                "a.ts#function[1]",
                "a.ts#variable[0]",
                "a.ts#function:named[1]"
            ]
        );
    }

    #[test]
    fn empty_names_count_as_unnamed() {
        let keys = block_keys(
            &path("a.ts"),
            [
                shape(None, K::File, ""),
                BlockShape {
                    parent: Some(0),
                    kind: K::If,
                    name: Some(""),
                },
            ],
        )
        .unwrap();
        assert_eq!(keys[1].as_str(), "a.ts#if[0]");
    }

    #[test]
    fn rejects_blocks_out_of_preorder() {
        let mut builder = KeyBuilder::new(&path("a.ts"));
        assert_eq!(
            builder.push(Some(0), K::Function, None),
            Err(KeyError::MissingRoot)
        );
        assert_eq!(builder.push(None, K::Function, None), Err(KeyError::MissingRoot));
        builder.push(None, K::File, None).unwrap();
        assert_eq!(
            builder.push(Some(1), K::If, None),
            Err(KeyError::ParentNotBefore {
                local_id: 1,
                parent: 1
            })
        );
        assert_eq!(
            builder.push(None, K::If, None),
            Err(KeyError::SecondRoot { local_id: 1 })
        );
        assert_eq!(
            builder.push(Some(0), K::File, None),
            Err(KeyError::SecondRoot { local_id: 1 })
        );
        assert_eq!(builder.keys().len(), 1);
    }

    #[test]
    fn entrance_keys_match_the_proof_of_concept() {
        let route = EntranceShape {
            detector: DetectorId::ExpressRoute,
            kind: EntranceKind::Http,
            method: Some(HttpMethod::Post),
            trigger: Some("/orders"),
            first_target: Some("requireUser"),
            configured: None,
        };
        let worker = EntranceShape {
            detector: DetectorId::BullmqWorker,
            kind: EntranceKind::Worker,
            method: None,
            trigger: None,
            first_target: Some("new URL(\"../../report-processor.js\", import.meta.url)"),
            configured: None,
        };
        let keys = entrance_keys([route, worker, route, route]);
        let keys: Vec<&str> = keys.iter().map(Key::as_str).collect();
        assert_eq!(
            keys,
            [
                r#"entrance:["express.route","http","POST","/orders","requireUser"]"#,
                r#"entrance:["bullmq.worker","worker","","","new URL(\"../../report-processor.js\", import.meta.url)"]"#,
                r#"entrance:["express.route","http","POST","/orders","requireUser"]#1"#,
                r#"entrance:["express.route","http","POST","/orders","requireUser"]#2"#,
            ]
        );
        assert!(Key::from_text(keys[0]).is_entrance());
    }

    #[test]
    fn configured_entrance_keys_are_their_id() {
        let shape = |trigger| EntranceShape {
            detector: DetectorId::Manual,
            kind: EntranceKind::Process,
            method: None,
            trigger,
            first_target: Some("src/server.ts"),
            configured: Some("api-start"),
        };
        let keys = entrance_keys([shape(Some("start")), shape(None)]);
        assert_eq!(keys[0].as_str(), "entrance:configured:api-start");
        assert_eq!(keys[0], Key::configured_entrance("api-start"));
        assert_eq!(keys[0].configured_id(), Some("api-start"));
        assert!(keys[0].is_entrance());
        assert_eq!(Key::from_text(r#"entrance:["manual"]"#).configured_id(), None);
    }

    #[test]
    fn entrance_json_escaping_matches_json_stringify() {
        // serde_json escapes strings the same way JSON.stringify does.
        let tricky = "tab\t nl\n cr\r bs\\ q\" \u{1} \u{1f} \u{7f} \u{2028} é 😀 \u{8}\u{c}";
        let base = entrance_key_base(&EntranceShape {
            detector: DetectorId::Manual,
            kind: EntranceKind::Event,
            method: None,
            trigger: Some(tricky),
            first_target: None,
            configured: None,
        });
        let expected = serde_json::to_string(&["manual", "event", "", tricky, ""]).unwrap();
        assert_eq!(base, format!("entrance:{expected}"));
    }
}
