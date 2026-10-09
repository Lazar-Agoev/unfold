//! Strict board parsing: the text must be one JSON object in the `unfold-board` version 1
//! shape. Unknown fields, duplicate keys, wrong types, and malformed values are rejected with
//! the JSON pointer of the offending value (`/cards/2/ref`), so a person or agent editing the
//! file can find the problem. Parsing checks shape only; whether references exist in the
//! current graph is [`super::check`]'s job.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use rustc_hash::FxHashSet;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use unfold_core::Key;
use unfold_core::vocab::{BlockKind, EntranceKind};

use super::BoardError;
use super::model::{
    Author, Board, Card, CardRef, Description, FORMAT, Group, Guard, Signature, VERSION, View,
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
struct Path(String);

impl Path {
    fn join(&self, part: impl fmt::Display) -> Self {
        let part = part.to_string().replace('~', "~0").replace('/', "~1");
        Self(format!("{}/{part}", self.0))
    }

    fn fail(&self, message: impl Into<String>) -> BoardError {
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

fn boolean(value: &Value, path: &Path) -> Parsed<bool> {
    value.as_bool().ok_or_else(|| path.fail("must be true or false"))
}

fn array<'a>(value: &'a Value, path: &Path) -> Parsed<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| path.fail("must be an array"))
}

fn view(value: &Value, path: &Path) -> Parsed<View> {
    View::parse(string(value, path)?).ok_or_else(|| path.fail("must be \"code\" or \"flow\""))
}

fn signature(value: &Value, path: &Path) -> Parsed<Signature> {
    Signature::parse(string(value, path)?)
        .ok_or_else(|| path.fail("must be `sha256:` and 64 lowercase hex digits"))
}

fn description(value: &Value, path: &Path) -> Parsed<Description> {
    let map = object(value, path, &["author", "signature", "text"], &["author", "text"])?;
    let text = nonblank(field(map, "text"), &path.join("text"))?;
    let author_path = path.join("author");
    let author = Author::parse(string(field(map, "author"), &author_path)?)
        .ok_or_else(|| author_path.fail("must be `person:<name>` or `agent:<name>`"))?;
    let signature = match map.get("signature") {
        None => None,
        Some(Value::String(empty)) if empty.is_empty() => None,
        Some(value) => Some(signature(value, &path.join("signature"))?),
    };
    Ok(Description {
        text: text.to_owned(),
        author,
        signature,
    })
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
        (Some(block), None) => Ok(CardRef::Block(block_key(block, &path.join("block"))?)),
        (None, Some(entrance)) => {
            let entrance_path = path.join("entrance");
            let key = nonblank(entrance, &entrance_path)?;
            if !key.starts_with("entrance:") {
                return Err(entrance_path.fail("must be an entrance key (`entrance:[…]`)"));
            }
            Ok(CardRef::Entrance(Key::from_text(key)))
        }
        _ => Err(path.fail("must have exactly one of `block` or `entrance`")),
    }
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

fn card(value: &Value, path: &Path) -> Parsed<Card> {
    let map = object(
        value,
        path,
        &["at", "description", "folded", "id", "ref", "view"],
        &["at", "id", "ref"],
    )?;
    Ok(Card {
        id: nonblank(field(map, "id"), &path.join("id"))?.to_owned(),
        reference: reference(field(map, "ref"), &path.join("ref"))?,
        at: position(field(map, "at"), &path.join("at"))?,
        view: map
            .get("view")
            .map(|value| view(value, &path.join("view")))
            .transpose()?,
        folded: map
            .get("folded")
            .map(|value| boolean(value, &path.join("folded")))
            .transpose()?
            .unwrap_or(false),
        description: map
            .get("description")
            .map(|value| description(value, &path.join("description")))
            .transpose()?,
    })
}

fn group(value: &Value, path: &Path) -> Parsed<Group> {
    let map = object(
        value,
        path,
        &["card", "collapsed", "description", "id", "members", "title"],
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
        id: nonblank(field(map, "id"), &path.join("id"))?.to_owned(),
        card: nonblank(field(map, "card"), &path.join("card"))?.to_owned(),
        title: nonblank(field(map, "title"), &path.join("title"))?.to_owned(),
        members,
        collapsed: boolean(field(map, "collapsed"), &path.join("collapsed"))?,
        description: map
            .get("description")
            .map(|value| description(value, &path.join("description")))
            .transpose()?,
    })
}

/// Whether `kind` is a block kind or `entrance:<entrance kind>`.
pub(crate) fn valid_guard_kind(kind: &str) -> bool {
    match kind.strip_prefix("entrance:") {
        Some(entrance) => EntranceKind::from_str(entrance).is_ok(),
        None => BlockKind::from_str(kind).is_ok(),
    }
}

fn guard(value: &Value, path: &Path) -> Parsed<Guard> {
    let map = object(
        value,
        path,
        &["kind", "owner", "signature"],
        &["kind", "owner", "signature"],
    )?;
    let kind_path = path.join("kind");
    let kind = string(field(map, "kind"), &kind_path)?;
    if !valid_guard_kind(kind) {
        return Err(kind_path.fail("must be a block kind or `entrance:<kind>`"));
    }
    Ok(Guard {
        kind: kind.to_owned(),
        owner: string(field(map, "owner"), &path.join("owner"))?.to_owned(),
        signature: signature(field(map, "signature"), &path.join("signature"))?,
    })
}

/// Parses board file text.
pub fn parse(text: &str) -> Result<Board, BoardError> {
    let Strict(value) = serde_json::from_str(text).map_err(|error| BoardError::Syntax {
        line: error.line(),
        column: error.column(),
        message: error.to_string(),
    })?;
    let root = Path::default();
    let map = object(
        &value,
        &root,
        &[
            "cards",
            "defaultView",
            "format",
            "groups",
            "guards",
            "labels",
            "title",
            "version",
        ],
        &["cards", "defaultView", "format", "groups", "title", "version"],
    )?;
    if field(map, "format").as_str() != Some(FORMAT) {
        return Err(root.join("format").fail(format!("must be \"{FORMAT}\"")));
    }
    if field(map, "version").as_u64() != Some(u64::from(VERSION)) {
        return Err(root
            .join("version")
            .fail(format!("unsupported version; expected {VERSION}")));
    }
    let title = nonblank(field(map, "title"), &root.join("title"))?.to_owned();
    let default_view = view(field(map, "defaultView"), &root.join("defaultView"))?;

    let mut ids: FxHashSet<String> = FxHashSet::default();
    let cards_path = root.join("cards");
    let mut cards = Vec::new();
    for (index, item) in array(field(map, "cards"), &cards_path)?.iter().enumerate() {
        let path = cards_path.join(index);
        let card = card(item, &path)?;
        if !ids.insert(card.id.clone()) {
            return Err(path.join("id").fail(format!("duplicate id `{}`", card.id)));
        }
        cards.push(card);
    }
    let card_ids: FxHashSet<&str> = cards.iter().map(|card| card.id.as_str()).collect();
    let groups_path = root.join("groups");
    let mut groups = Vec::new();
    for (index, item) in array(field(map, "groups"), &groups_path)?.iter().enumerate() {
        let path = groups_path.join(index);
        let group = group(item, &path)?;
        if !ids.insert(group.id.clone()) {
            return Err(path.join("id").fail(format!("duplicate id `{}`", group.id)));
        }
        if !card_ids.contains(group.card.as_str()) {
            return Err(path
                .join("card")
                .fail(format!("refers to an unknown card `{}`", group.card)));
        }
        groups.push(group);
    }

    let mut labels = BTreeMap::new();
    if let Some(value) = map.get("labels") {
        let path = root.join("labels");
        let Value::Object(entries) = value else {
            return Err(path.fail("must be an object of labels by block key"));
        };
        for (key, item) in entries {
            let item_path = path.join(key);
            block_key(&Value::String(key.clone()), &item_path)?;
            labels.insert(key.clone(), description(item, &item_path)?);
        }
    }
    let mut guards = BTreeMap::new();
    if let Some(value) = map.get("guards") {
        let path = root.join("guards");
        let Value::Object(entries) = value else {
            return Err(path.fail("must be an object of guards by key"));
        };
        for (key, item) in entries {
            let item_path = path.join(key);
            if key.trim().is_empty() {
                return Err(item_path.fail("must be keyed by a block or entrance key"));
            }
            guards.insert(key.clone(), guard(item, &item_path)?);
        }
    }
    Ok(Board {
        title,
        default_view,
        cards,
        groups,
        labels,
        guards,
    })
}
