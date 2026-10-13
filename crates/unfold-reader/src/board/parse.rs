//! Strict board parsing: the text must be one JSON object in the `unfold-board` version 1
//! shape. Unknown fields, duplicate keys, wrong types, and malformed values are rejected with
//! the JSON pointer of the offending value (`/pages/0/cards/2/ref`), so a person or agent
//! editing the file can find the problem. Parsing checks shape and the references between the
//! items of one page ([`super::items`]); whether keys exist in the current graph is
//! [`super::health`]'s job.
//!
//! Two framings share the page rules: a board file ([`parse`]) and the edit the UI saves
//! ([`parse_edit`]: only `title` and `pages`).

use std::fmt;
use std::str::FromStr;

use rustc_hash::FxHashSet;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use unfold_core::vocab::{BlockKind, EntranceKind};
use unfold_core::{Key, RepoPath};

use super::BoardError;
use super::items::check_items;
use super::model::{
    Anchor, Author, Board, Card, CardRef, Cardinality, Column, ColumnKey, Description, FORMAT, Frame,
    FrameDirection, FrameIcon, FrameLook, Group, GuardEntry, LabelEntry, Link, LinkKind, MAX_EXPANDED_CALLS,
    MAX_EXPANSION_DEPTH, MAX_NOTE_CHARACTERS, MAX_PROPOSAL_REASON, MAX_PROPOSAL_TITLE, MAX_PROPOSALS, Note,
    Page, Proposal, ProposalPath, Resource, ResourceKind, Signature, VERSION, View, is_folder_name, is_id,
};

/// A JSON value read with duplicate keys rejected.
struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(Strict)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(Strict(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let Strict(value) = map.next_value()?;
            if object.contains_key(&key) {
                return Err(de::Error::custom(format_args!("duplicate key `{key}`")));
            }
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

/// A JSON pointer being built while walking the document.
#[derive(Debug, Clone, Default)]
pub(super) struct Path(String);

impl Path {
    /// The pointer of `part` inside this value.
    pub(super) fn join(&self, part: impl fmt::Display) -> Self {
        let part = part.to_string().replace('~', "~0").replace('/', "~1");
        Self(format!("{}/{part}", self.0))
    }

    /// The error that the value at this pointer breaks the format, and why.
    pub(super) fn fail(&self, message: impl Into<String>) -> BoardError {
        BoardError::Invalid {
            path: if self.0.is_empty() {
                "/".to_owned()
            } else {
                self.0.clone()
            },
            message: message.into(),
        }
    }
}

type Parsed<T> = Result<T, BoardError>;

fn object<'a>(
    value: &'a Value,
    path: &Path,
    allowed: &[&str],
    required: &[&str],
) -> Parsed<&'a Map<String, Value>> {
    let Value::Object(map) = value else {
        return Err(path.fail("must be an object"));
    };
    if let Some(unknown) = map.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(path.join(unknown).fail("unknown field"));
    }
    if let Some(missing) = required.iter().find(|key| !map.contains_key(**key)) {
        return Err(path.fail(format!("missing field `{missing}`")));
    }
    Ok(map)
}

fn field<'a>(map: &'a Map<String, Value>, name: &str) -> &'a Value {
    map.get(name).unwrap_or(&Value::Null)
}

fn string<'a>(value: &'a Value, path: &Path) -> Parsed<&'a str> {
    value.as_str().ok_or_else(|| path.fail("must be a string"))
}

fn nonblank<'a>(value: &'a Value, path: &Path) -> Parsed<&'a str> {
    let text = string(value, path)?;
    if text.trim().is_empty() {
        return Err(path.fail("must not be empty"));
    }
    Ok(text)
}

/// A string that isn't blank, owned.
fn owned_text(value: &Value, path: &Path) -> Parsed<String> {
    nonblank(value, path).map(str::to_owned)
}

/// A string that isn't blank, of at most `max` characters, owned.
fn limited_text(value: &Value, path: &Path, max: usize) -> Parsed<String> {
    let text = nonblank(value, path)?;
    if text.chars().count() > max {
        return Err(path.fail(format!("must be at most {max} characters")));
    }
    Ok(text.to_owned())
}

fn boolean(value: &Value, path: &Path) -> Parsed<bool> {
    value.as_bool().ok_or_else(|| path.fail("must be true or false"))
}

fn array<'a>(value: &'a Value, path: &Path) -> Parsed<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| path.fail("must be an array"))
}

/// The items of an optional array field (absent: none).
fn optional_array<'a>(map: &'a Map<String, Value>, name: &str, path: &Path) -> Parsed<&'a [Value]> {
    match map.get(name) {
        Some(value) => array(value, &path.join(name)),
        None => Ok(&[]),
    }
}

/// Optional field `name` of `map`, read with `read` (absent: `None`).
fn optional<T>(
    map: &Map<String, Value>,
    name: &str,
    path: &Path,
    read: impl FnOnce(&Value, &Path) -> Parsed<T>,
) -> Parsed<Option<T>> {
    map.get(name)
        .map(|value| read(value, &path.join(name)))
        .transpose()
}

/// `"a" or "b"`, or `"a", "b", or "c"`: the words a value may be, for messages.
fn choices(words: &[&str]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("\"{word}\"")).collect();
    match quoted.as_slice() {
        [one] => one.clone(),
        [first, second] => format!("{first} or {second}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
        [] => String::new(),
    }
}

/// One of a closed set of words (`words`), read with `parse`.
fn word<T>(value: &Value, path: &Path, parse: fn(&str) -> Option<T>, words: &[&str]) -> Parsed<T> {
    parse(string(value, path)?).ok_or_else(|| path.fail(format!("must be {}", choices(words))))
}

fn id(value: &Value, path: &Path) -> Parsed<String> {
    let text = string(value, path)?;
    if !is_id(text) {
        return Err(
            path.fail("must be a letter or digit followed by up to 99 letters, digits, `_`, `.`, or `-`")
        );
    }
    Ok(text.to_owned())
}

fn view(value: &Value, path: &Path) -> Parsed<View> {
    word(value, path, View::parse, View::WORDS)
}

fn signature(value: &Value, path: &Path) -> Parsed<Signature> {
    Signature::parse(string(value, path)?)
        .ok_or_else(|| path.fail("must be `sha256:` and 64 lowercase hex digits"))
}

/// The optional `signature` of `map`: absent or `""` (not signed yet) is `None`.
fn optional_signature(map: &Map<String, Value>, path: &Path) -> Parsed<Option<Signature>> {
    match map.get("signature") {
        None => Ok(None),
        Some(Value::String(empty)) if empty.is_empty() => Ok(None),
        Some(value) => signature(value, &path.join("signature")).map(Some),
    }
}

fn author(value: &Value, path: &Path) -> Parsed<Author> {
    Author::parse(string(value, path)?).ok_or_else(|| path.fail("must be `person:<name>` or `agent:<name>`"))
}

/// The text, author, and signature fields of a description, label, or link label in `map`.
fn authored(map: &Map<String, Value>, path: &Path) -> Parsed<Description> {
    let text = nonblank(field(map, "text"), &path.join("text"))?;
    Ok(Description {
        text: text.to_owned(),
        author: author(field(map, "author"), &path.join("author"))?,
        signature: optional_signature(map, path)?,
    })
}

fn description(value: &Value, path: &Path) -> Parsed<Description> {
    let map = object(value, path, &["author", "signature", "text"], &["author", "text"])?;
    authored(map, path)
}

fn block_key(value: &Value, path: &Path) -> Parsed<Key> {
    let key = nonblank(value, path)?;
    if key.starts_with("entrance:") || !key.contains('#') {
        return Err(path.fail("must be a block key (`path#…`)"));
    }
    Ok(Key::from_text(key))
}

fn reference(value: &Value, path: &Path) -> Parsed<CardRef> {
    let map = object(value, path, &["block", "entrance"], &[])?;
    match (map.get("block"), map.get("entrance")) {
        (Some(block), None) => Ok(CardRef::block(block_key(block, &path.join("block"))?)),
        (None, Some(entrance)) => {
            let entrance_path = path.join("entrance");
            let key = nonblank(entrance, &entrance_path)?;
            if !key.starts_with("entrance:") {
                return Err(entrance_path.fail("must be an entrance key (`entrance:[…]`)"));
            }
            Ok(CardRef::entrance(Key::from_text(key)))
        }
        _ => Err(path.fail("must have exactly one of `block` or `entrance`")),
    }
}

/// A block key or an entrance key.
fn any_key(value: &Value, path: &Path) -> Parsed<Key> {
    let key = nonblank(value, path)?;
    if !key.starts_with("entrance:") && !key.contains('#') {
        return Err(path.fail("must be a block key (`path#…`) or an entrance key (`entrance:[…]`)"));
    }
    Ok(Key::from_text(key))
}

fn position(value: &Value, path: &Path) -> Parsed<[f64; 2]> {
    match array(value, path)? {
        [x, y] => {
            let number = |value: &Value, index: usize| {
                value
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| path.join(index).fail("must be a finite number"))
            };
            Ok([number(x, 0)?, number(y, 1)?])
        }
        _ => Err(path.fail("must be two numbers")),
    }
}

/// A card's calls expanded in place: at most [`MAX_EXPANDED_CALLS`] different paths of 1 to
/// [`MAX_EXPANSION_DEPTH`] block keys each.
fn expanded(value: &Value, path: &Path) -> Parsed<Vec<Vec<Key>>> {
    let items = array(value, path)?;
    if items.len() > MAX_EXPANDED_CALLS {
        return Err(path.fail(format!("must hold at most {MAX_EXPANDED_CALLS} expanded calls")));
    }
    let mut paths: Vec<Vec<Key>> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_path = path.join(index);
        let keys = array(item, &item_path)?;
        if keys.is_empty() || keys.len() > MAX_EXPANSION_DEPTH {
            return Err(item_path.fail(format!(
                "must list 1 to {MAX_EXPANSION_DEPTH} keys: a call or callback of the card's block, \
                 then of each callee expanded inside it, outermost first"
            )));
        }
        let keys = keys
            .iter()
            .enumerate()
            .map(|(level, key)| block_key(key, &item_path.join(level)))
            .collect::<Parsed<Vec<Key>>>()?;
        if paths.contains(&keys) {
            return Err(item_path.fail("repeats an earlier expanded call"));
        }
        paths.push(keys);
    }
    Ok(paths)
}

fn card(value: &Value, path: &Path) -> Parsed<Card> {
    let map = object(
        value,
        path,
        &["at", "description", "expanded", "folded", "id", "ref", "view"],
        &["at", "id", "ref"],
    )?;
    let card = Card {
        id: id(field(map, "id"), &path.join("id"))?,
        reference: reference(field(map, "ref"), &path.join("ref"))?,
        at: position(field(map, "at"), &path.join("at"))?,
        view: optional(map, "view", path, view)?,
        folded: optional(map, "folded", path, boolean)?.unwrap_or(false),
        description: optional(map, "description", path, description)?,
        expanded: optional(map, "expanded", path, expanded)?.unwrap_or_default(),
    };
    if !card.expanded.is_empty() && card.reference.block_key().is_none() {
        return Err(path.join("expanded").fail(
            "only a block card expands calls in place; an entrance card's targets open as their own cards",
        ));
    }
    Ok(card)
}

fn group(value: &Value, path: &Path) -> Parsed<Group> {
    let map = object(
        value,
        path,
        &[
            "author",
            "card",
            "collapsed",
            "description",
            "id",
            "members",
            "title",
        ],
        &["card", "collapsed", "id", "members", "title"],
    )?;
    let members_path = path.join("members");
    let items = array(field(map, "members"), &members_path)?;
    if items.is_empty() {
        return Err(members_path.fail("must list at least one block key"));
    }
    let mut seen = FxHashSet::default();
    let mut members = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_path = members_path.join(index);
        let key = block_key(item, &item_path)?;
        if !seen.insert(key.clone()) {
            return Err(item_path.fail("repeats an earlier member"));
        }
        members.push(key);
    }
    Ok(Group {
        id: id(field(map, "id"), &path.join("id"))?,
        card: id(field(map, "card"), &path.join("card"))?,
        title: nonblank(field(map, "title"), &path.join("title"))?.to_owned(),
        members,
        collapsed: boolean(field(map, "collapsed"), &path.join("collapsed"))?,
        description: optional(map, "description", path, description)?,
        author: optional(map, "author", path, author)?,
    })
}

fn label(value: &Value, path: &Path) -> Parsed<LabelEntry> {
    let map = object(
        value,
        path,
        &["author", "key", "signature", "text"],
        &["author", "key", "text"],
    )?;
    Ok(LabelEntry {
        key: block_key(field(map, "key"), &path.join("key"))?,
        label: authored(map, path)?,
    })
}

/// Whether `kind` is a block kind or `entrance:<entrance kind>`.
fn valid_guard_kind(kind: &str) -> bool {
    match kind.strip_prefix("entrance:") {
        Some(entrance) => EntranceKind::from_str(entrance).is_ok(),
        None => BlockKind::from_str(kind).is_ok(),
    }
}

fn guard(value: &Value, path: &Path) -> Parsed<GuardEntry> {
    let map = object(
        value,
        path,
        &["key", "kind", "owner", "signature"],
        &["key", "kind", "owner", "signature"],
    )?;
    let kind_path = path.join("kind");
    let kind = string(field(map, "kind"), &kind_path)?;
    if !valid_guard_kind(kind) {
        return Err(kind_path.fail("must be a block kind or `entrance:<kind>`"));
    }
    Ok(GuardEntry {
        key: Key::from_text(nonblank(field(map, "key"), &path.join("key"))?),
        kind: kind.to_owned(),
        owner: string(field(map, "owner"), &path.join("owner"))?.to_owned(),
        signature: signature(field(map, "signature"), &path.join("signature"))?,
    })
}

fn anchor(value: &Value, path: &Path) -> Parsed<Anchor> {
    let map = object(value, path, &["file", "key", "lines"], &[])?;
    match (map.get("key"), map.get("file")) {
        (Some(key), None) => {
            if map.contains_key("lines") {
                return Err(path.join("lines").fail("only a file anchor has lines"));
            }
            Ok(Anchor::Key {
                key: any_key(key, &path.join("key"))?,
            })
        }
        (None, Some(file)) => {
            let file_path = path.join("file");
            let file = RepoPath::new(string(file, &file_path)?).map_err(|_| {
                file_path.fail("must be a path relative to the project's folder, with `/` separators and no `.` or `..` parts")
            })?;
            let lines = map
                .get("lines")
                .ok_or_else(|| path.fail("missing field `lines`"))?;
            Ok(Anchor::File {
                file,
                lines: line_range(lines, &path.join("lines"))?,
            })
        }
        _ => Err(path.fail("must have exactly one of `key` or `file`")),
    }
}

/// `[first, last]`: 1-based, inclusive line numbers.
fn line_range(value: &Value, path: &Path) -> Parsed<[u32; 2]> {
    let [first, last] = array(value, path)? else {
        return Err(path.fail("must be two line numbers: the first and the last"));
    };
    let number = |value: &Value, index: usize| {
        value
            .as_u64()
            .filter(|line| *line >= 1)
            .and_then(|line| u32::try_from(line).ok())
            .ok_or_else(|| path.join(index).fail("must be a line number (1 or more)"))
    };
    let (first, last) = (number(first, 0)?, number(last, 1)?);
    if last < first {
        return Err(path.join(1).fail("must not come before the first line"));
    }
    Ok([first, last])
}

fn anchors(value: &Value, path: &Path) -> Parsed<Vec<Anchor>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| anchor(item, &path.join(index)))
        .collect()
}

fn link(value: &Value, path: &Path) -> Parsed<Link> {
    let map = object(
        value,
        path,
        &[
            "anchors",
            "author",
            "cardinality",
            "carries",
            "channel",
            "description",
            "from",
            "fromColumn",
            "id",
            "kind",
            "label",
            "signature",
            "step",
            "to",
            "toColumn",
        ],
        &["from", "id", "to"],
    )?;
    let mut link = Link::new(
        &id(field(map, "id"), &path.join("id"))?,
        &id(field(map, "from"), &path.join("from"))?,
        &id(field(map, "to"), &path.join("to"))?,
    );
    link.label = optional(map, "label", path, description)?;
    link.kind = optional(map, "kind", path, |value, path| {
        word(value, path, LinkKind::parse, LinkKind::WORDS)
    })?;
    if !link.relates()
        && let Some(name) = ["fromColumn", "toColumn", "cardinality"]
            .into_iter()
            .find(|name| map.contains_key(*name))
    {
        return Err(path
            .join(name)
            .fail("only a `relates` connection joins columns and has a cardinality"));
    }
    link.author = optional(map, "author", path, author)?;
    if link.kind.is_some() && link.author.is_none() {
        return Err(path.fail("missing field `author`: a connection (a link with `kind`) names who drew it"));
    }
    link.step = optional(map, "step", path, block_key)?;
    link.from_column = optional(map, "fromColumn", path, owned_text)?;
    link.to_column = optional(map, "toColumn", path, owned_text)?;
    link.channel = optional(map, "channel", path, owned_text)?;
    link.carries = optional(map, "carries", path, block_key)?;
    link.cardinality = optional(map, "cardinality", path, |value, path| {
        word(value, path, Cardinality::parse, Cardinality::WORDS)
    })?;
    link.anchors = optional(map, "anchors", path, anchors)?.unwrap_or_default();
    link.description = optional(map, "description", path, description)?;
    link.signature = optional_signature(map, path)?;
    // An empty `anchors` is the default, as everywhere.
    if link.kind.is_none()
        && let Some(name) = ["carries", "channel", "description", "signature", "step"]
            .into_iter()
            .find(|name| map.contains_key(*name))
            .or_else(|| (!link.anchors.is_empty()).then_some("anchors"))
    {
        return Err(path.join(name).fail(
            "only a connection (a link with `kind`) cites code and has a description; a drawn link has a label",
        ));
    }
    Ok(link)
}

fn frame(value: &Value, path: &Path) -> Parsed<Frame> {
    let map = object(
        value,
        path,
        &[
            "author",
            "collapsed",
            "description",
            "direction",
            "icon",
            "id",
            "look",
            "members",
            "title",
        ],
        &["author", "id", "look", "members", "title"],
    )?;
    let frame_id = id(field(map, "id"), &path.join("id"))?;
    let members_path = path.join("members");
    let items = array(field(map, "members"), &members_path)?;
    if items.is_empty() {
        return Err(members_path.fail("must list at least one item"));
    }
    let mut seen = FxHashSet::default();
    let mut members = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_path = members_path.join(index);
        let member = id(item, &item_path)?;
        if !seen.insert(member.clone()) {
            return Err(item_path.fail("repeats an earlier member"));
        }
        members.push(member);
    }
    Ok(Frame {
        id: frame_id,
        title: nonblank(field(map, "title"), &path.join("title"))?.to_owned(),
        description: optional(map, "description", path, description)?,
        members,
        look: word(
            field(map, "look"),
            &path.join("look"),
            FrameLook::parse,
            FrameLook::WORDS,
        )?,
        icon: optional(map, "icon", path, |value, path| {
            word(value, path, FrameIcon::parse, FrameIcon::WORDS)
        })?,
        direction: optional(map, "direction", path, |value, path| {
            word(value, path, FrameDirection::parse, FrameDirection::WORDS)
        })?,
        collapsed: optional(map, "collapsed", path, boolean)?.unwrap_or(false),
        author: author(field(map, "author"), &path.join("author"))?,
    })
}

/// A column of a table; `marks_only` for a table backed by a type.
fn column(value: &Value, path: &Path, marks_only: bool) -> Parsed<Column> {
    let map = object(
        value,
        path,
        &["key", "name", "optional", "references", "type"],
        &["name"],
    )?;
    if marks_only
        && let Some(name) = ["type", "optional"]
            .into_iter()
            .find(|name| map.contains_key(*name))
    {
        return Err(path.join(name).fail(
            "a table backed by a type lists marks only (`name`, `key`, `references`): the type gives the rest",
        ));
    }
    let column = Column {
        name: nonblank(field(map, "name"), &path.join("name"))?.to_owned(),
        written_type: optional(map, "type", path, owned_text)?,
        key: optional(map, "key", path, |value, path| {
            word(value, path, ColumnKey::parse, ColumnKey::WORDS)
        })?,
        references: optional(map, "references", path, owned_text)?,
        optional: optional(map, "optional", path, boolean)?.unwrap_or(false),
    };
    if column.references.is_some() && column.key != Some(ColumnKey::Foreign) {
        return Err(path
            .join("references")
            .fail("only a foreign key (`\"key\": \"foreign\"`) references another table"));
    }
    Ok(column)
}

fn resource(value: &Value, path: &Path) -> Parsed<Resource> {
    let map = object(
        value,
        path,
        &[
            "anchors",
            "at",
            "author",
            "columns",
            "description",
            "id",
            "kind",
            "name",
            "signature",
            "type",
        ],
        &["anchors", "at", "author", "id", "kind", "name"],
    )?;
    let resource_id = id(field(map, "id"), &path.join("id"))?;
    let kind = word(
        field(map, "kind"),
        &path.join("kind"),
        ResourceKind::parse,
        ResourceKind::WORDS,
    )?;
    let table = kind == ResourceKind::Table;
    if !table
        && let Some(name) = ["type", "columns"]
            .into_iter()
            .find(|name| map.contains_key(*name))
    {
        return Err(path.join(name).fail("only a table has a type and columns"));
    }
    let type_key = optional(map, "type", path, block_key)?;
    let columns_path = path.join("columns");
    let mut columns: Vec<Column> = Vec::new();
    for (index, item) in optional_array(map, "columns", path)?.iter().enumerate() {
        let item_path = columns_path.join(index);
        let column = column(item, &item_path, type_key.is_some())?;
        if columns.iter().any(|known| known.name == column.name) {
            return Err(item_path
                .join("name")
                .fail(format!("repeats column `{}`", column.name)));
        }
        columns.push(column);
    }
    Ok(Resource {
        id: resource_id,
        kind,
        name: nonblank(field(map, "name"), &path.join("name"))?.to_owned(),
        type_key,
        columns,
        anchors: anchors(field(map, "anchors"), &path.join("anchors"))?,
        at: position(field(map, "at"), &path.join("at"))?,
        description: optional(map, "description", path, description)?,
        author: author(field(map, "author"), &path.join("author"))?,
        signature: optional_signature(map, path)?,
    })
}

fn note(value: &Value, path: &Path) -> Parsed<Note> {
    let map = object(
        value,
        path,
        &["at", "attach", "author", "id", "text"],
        &["at", "author", "id", "text"],
    )?;
    Ok(Note {
        id: id(field(map, "id"), &path.join("id"))?,
        text: limited_text(field(map, "text"), &path.join("text"), MAX_NOTE_CHARACTERS)?,
        at: position(field(map, "at"), &path.join("at"))?,
        attach: optional(map, "attach", path, id)?,
        author: author(field(map, "author"), &path.join("author"))?,
    })
}

fn proposal(value: &Value, path: &Path) -> Parsed<Proposal> {
    let map = object(
        value,
        path,
        &["after", "author", "card", "id", "path", "reason", "title"],
        &["after", "author", "card", "id", "title"],
    )?;
    let proposal_id = id(field(map, "id"), &path.join("id"))?;
    let card = id(field(map, "card"), &path.join("card"))?;
    let after = block_key(field(map, "after"), &path.join("after"))?;
    let title_path = path.join("title");
    let title = limited_text(field(map, "title"), &title_path, MAX_PROPOSAL_TITLE)?;
    if title.chars().any(char::is_control) {
        return Err(title_path.fail("must be one line, without control characters"));
    }
    Ok(Proposal {
        id: proposal_id,
        card,
        after,
        path: optional(map, "path", path, |value, path| {
            word(value, path, ProposalPath::parse, ProposalPath::WORDS)
        })?,
        title,
        reason: optional(map, "reason", path, |value, path| {
            limited_text(value, path, MAX_PROPOSAL_REASON)
        })?,
        author: author(field(map, "author"), &path.join("author"))?,
    })
}

/// The items of optional array `name` of `map`, each read with `read`.
fn items<T>(
    map: &Map<String, Value>,
    name: &str,
    path: &Path,
    read: fn(&Value, &Path) -> Parsed<T>,
) -> Parsed<Vec<T>> {
    let items_path = path.join(name);
    optional_array(map, name, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| read(item, &items_path.join(index)))
        .collect()
}

fn page(value: &Value, path: &Path) -> Parsed<Page> {
    let map = object(
        value,
        path,
        &[
            "cards",
            "defaultView",
            "frames",
            "groups",
            "guards",
            "id",
            "labels",
            "links",
            "notes",
            "proposals",
            "resources",
            "title",
        ],
        &["cards", "defaultView", "groups", "id", "title"],
    )?;
    let mut page = Page::new(
        &id(field(map, "id"), &path.join("id"))?,
        nonblank(field(map, "title"), &path.join("title"))?,
    );
    page.default_view = view(field(map, "defaultView"), &path.join("defaultView"))?;
    // `cards` and `groups` are required (checked above); the other arrays may be absent.
    page.cards = items(map, "cards", path, card)?;
    page.groups = items(map, "groups", path, group)?;
    page.frames = items(map, "frames", path, frame)?;
    page.resources = items(map, "resources", path, resource)?;
    page.links = items(map, "links", path, link)?;
    page.notes = items(map, "notes", path, note)?;
    if optional_array(map, "proposals", path)?.len() > MAX_PROPOSALS {
        return Err(path
            .join("proposals")
            .fail(format!("must hold at most {MAX_PROPOSALS} proposed steps")));
    }
    page.proposals = items(map, "proposals", path, proposal)?;

    let mut labeled = FxHashSet::default();
    for (index, item) in optional_array(map, "labels", path)?.iter().enumerate() {
        let item_path = path.join("labels").join(index);
        let entry = label(item, &item_path)?;
        if !labeled.insert(entry.key.clone()) {
            return Err(item_path.fail(format!("repeats the label of `{}`", entry.key)));
        }
        page.labels.push(entry);
    }
    let mut guarded = FxHashSet::default();
    for (index, item) in optional_array(map, "guards", path)?.iter().enumerate() {
        let item_path = path.join("guards").join(index);
        let entry = guard(item, &item_path)?;
        if !guarded.insert(entry.key.clone()) {
            return Err(item_path.fail(format!("repeats the guard of `{}`", entry.key)));
        }
        page.guards.push(entry);
    }
    check_items(&page, path)?;
    Ok(page)
}

fn pages(value: &Value, path: &Path) -> Parsed<Vec<Page>> {
    let items = array(value, path)?;
    if items.is_empty() {
        return Err(path.fail("must hold at least one page"));
    }
    let mut ids = FxHashSet::default();
    let mut pages = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_path = path.join(index);
        let page = page(item, &item_path)?;
        if !ids.insert(page.id.clone()) {
            return Err(item_path
                .join("id")
                .fail(format!("duplicate page id `{}`", page.id)));
        }
        pages.push(page);
    }
    Ok(pages)
}

/// Reads JSON text with duplicate keys refused.
fn json(text: &str) -> Parsed<Value> {
    let Strict(value) = serde_json::from_str(text).map_err(|error| BoardError::Syntax {
        line: error.line(),
        column: error.column(),
        message: error.to_string(),
    })?;
    Ok(value)
}

/// Parses board file text.
pub fn parse(text: &str) -> Result<Board, BoardError> {
    let value = json(text)?;
    let root = Path::default();
    let map = object(
        &value,
        &root,
        &["folder", "format", "pages", "title", "version"],
        &["format", "pages", "title", "version"],
    )?;
    if field(map, "format").as_str() != Some(FORMAT) {
        return Err(root.join("format").fail(format!("must be \"{FORMAT}\"")));
    }
    if field(map, "version").as_u64() != Some(u64::from(VERSION)) {
        return Err(root
            .join("version")
            .fail(format!("unsupported version; expected {VERSION}")));
    }
    let title = nonblank(field(map, "title"), &root.join("title"))?;
    let folder = match map.get("folder") {
        None => None,
        Some(value) => {
            let folder_path = root.join("folder");
            let folder = string(value, &folder_path)?;
            if !is_folder_name(folder) {
                return Err(folder_path.fail(
                    "must be a folder name: 1 to 64 characters without `/`, `\\`, control \
                     characters, or leading or trailing spaces, and not `.` or `..`",
                ));
            }
            Some(folder.to_owned())
        }
    };
    Ok(Board::with_pages(
        title,
        folder,
        pages(field(map, "pages"), &root.join("pages"))?,
    ))
}

/// Parses an edit the UI saves (`{title, pages}`): the pages with the page rules, and `folder`
/// from the caller (the folder is the board's metadata, never part of an edit).
pub fn parse_edit(text: &str, folder: Option<String>) -> Result<Board, BoardError> {
    let value = json(text)?;
    let root = Path::default();
    let map = object(&value, &root, &["pages", "title"], &["pages", "title"])?;
    let title = nonblank(field(map, "title"), &root.join("title"))?;
    Ok(Board::with_pages(
        title,
        folder,
        pages(field(map, "pages"), &root.join("pages"))?,
    ))
}
