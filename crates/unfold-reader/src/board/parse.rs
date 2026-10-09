//! Strict board parsing: the text must be one JSON object in the `unfold-board` version 1
//! shape. Unknown fields, duplicate keys, wrong types, and malformed values are rejected with
//! the JSON pointer of the offending value (`/pages/0/cards/2/ref`), so a person or agent
//! editing the file can find the problem. Parsing checks shape only; whether references exist
//! in the current graph is [`super::health`]'s job.
//!
//! Three framings share the page rules: a board file ([`parse`]), the changes copy of a
//! published board (`parse_changes`, which adds `publishes` and `baseRevision`), and the edit
//! the UI saves ([`parse_edit`]: only `title` and `pages`).

use std::fmt;
use std::str::FromStr;

use rustc_hash::FxHashSet;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use unfold_core::vocab::{BlockKind, EntranceKind};
use unfold_core::{ContentHash, Key};

use super::BoardError;
use super::files::Revision;
use super::model::{
    Author, Board, Card, CardRef, Description, FORMAT, Group, GuardEntry, LabelEntry, Link, Page,
    ProjectName, Signature, VERSION, View, is_folder_path, is_id,
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

/// The items of an optional array field (absent: none).
fn optional_array<'a>(map: &'a Map<String, Value>, name: &str, path: &Path) -> Parsed<&'a [Value]> {
    match map.get(name) {
        Some(value) => array(value, &path.join(name)),
        None => Ok(&[]),
    }
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

fn project(value: &Value, path: &Path) -> Parsed<ProjectName> {
    ProjectName::parse(string(value, path)?).ok_or_else(|| {
        path.fail(
            "must be a project's shared name: a letter or digit followed by up to 63 letters, \
             digits, `.`, `_`, or `-`",
        )
    })
}

fn optional_project(map: &Map<String, Value>, path: &Path) -> Parsed<Option<ProjectName>> {
    map.get("project")
        .map(|value| project(value, &path.join("project")))
        .transpose()
}

fn view(value: &Value, path: &Path) -> Parsed<View> {
    View::parse(string(value, path)?).ok_or_else(|| path.fail("must be \"code\" or \"flow\""))
}

fn signature(value: &Value, path: &Path) -> Parsed<Signature> {
    Signature::parse(string(value, path)?)
        .ok_or_else(|| path.fail("must be `sha256:` and 64 lowercase hex digits"))
}

/// The text, author, and signature fields of a description, label, or link label in `map`.
fn authored(map: &Map<String, Value>, path: &Path) -> Parsed<Description> {
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
    let map = object(value, path, &["block", "entrance", "project"], &[])?;
    let project = optional_project(map, path)?;
    match (map.get("block"), map.get("entrance")) {
        (Some(block), None) => Ok(CardRef::block(block_key(block, &path.join("block"))?, project)),
        (None, Some(entrance)) => {
            let entrance_path = path.join("entrance");
            let key = nonblank(entrance, &entrance_path)?;
            if !key.starts_with("entrance:") {
                return Err(entrance_path.fail("must be an entrance key (`entrance:[…]`)"));
            }
            Ok(CardRef::entrance(Key::from_text(key), project))
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
        id: id(field(map, "id"), &path.join("id"))?,
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
        id: id(field(map, "id"), &path.join("id"))?,
        card: id(field(map, "card"), &path.join("card"))?,
        title: nonblank(field(map, "title"), &path.join("title"))?.to_owned(),
        members,
        collapsed: boolean(field(map, "collapsed"), &path.join("collapsed"))?,
        description: map
            .get("description")
            .map(|value| description(value, &path.join("description")))
            .transpose()?,
    })
}

fn label(value: &Value, path: &Path) -> Parsed<LabelEntry> {
    let map = object(
        value,
        path,
        &["author", "key", "project", "signature", "text"],
        &["author", "key", "text"],
    )?;
    Ok(LabelEntry {
        project: optional_project(map, path)?,
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
        &["key", "kind", "owner", "project", "signature"],
        &["key", "kind", "owner", "signature"],
    )?;
    let kind_path = path.join("kind");
    let kind = string(field(map, "kind"), &kind_path)?;
    if !valid_guard_kind(kind) {
        return Err(kind_path.fail("must be a block kind or `entrance:<kind>`"));
    }
    Ok(GuardEntry {
        project: optional_project(map, path)?,
        key: Key::from_text(nonblank(field(map, "key"), &path.join("key"))?),
        kind: kind.to_owned(),
        owner: string(field(map, "owner"), &path.join("owner"))?.to_owned(),
        signature: signature(field(map, "signature"), &path.join("signature"))?,
    })
}

fn link(value: &Value, path: &Path) -> Parsed<Link> {
    let map = object(value, path, &["from", "id", "label", "to"], &["from", "id", "to"])?;
    Ok(Link {
        id: id(field(map, "id"), &path.join("id"))?,
        from: id(field(map, "from"), &path.join("from"))?,
        to: id(field(map, "to"), &path.join("to"))?,
        label: map
            .get("label")
            .map(|value| description(value, &path.join("label")))
            .transpose()?,
    })
}

fn page(value: &Value, path: &Path) -> Parsed<Page> {
    let map = object(
        value,
        path,
        &[
            "cards",
            "defaultView",
            "groups",
            "guards",
            "id",
            "labels",
            "links",
            "title",
        ],
        &["cards", "defaultView", "groups", "id", "title"],
    )?;
    let mut page = Page::new(
        &id(field(map, "id"), &path.join("id"))?,
        nonblank(field(map, "title"), &path.join("title"))?,
    );
    page.default_view = view(field(map, "defaultView"), &path.join("defaultView"))?;

    // Card, group, and link ids are unique together within the page.
    let mut ids: FxHashSet<String> = FxHashSet::default();
    let mut unique = |id: &str, path: &Path| {
        if ids.insert(id.to_owned()) {
            Ok(())
        } else {
            Err(path.join("id").fail(format!("duplicate id `{id}`")))
        }
    };
    let cards_path = path.join("cards");
    for (index, item) in array(field(map, "cards"), &cards_path)?.iter().enumerate() {
        let item_path = cards_path.join(index);
        let card = card(item, &item_path)?;
        unique(&card.id, &item_path)?;
        page.cards.push(card);
    }
    let groups_path = path.join("groups");
    for (index, item) in array(field(map, "groups"), &groups_path)?.iter().enumerate() {
        let item_path = groups_path.join(index);
        let group = group(item, &item_path)?;
        unique(&group.id, &item_path)?;
        if page.card(&group.card).is_none() {
            return Err(item_path
                .join("card")
                .fail(format!("refers to an unknown card `{}`", group.card)));
        }
        page.groups.push(group);
    }
    for (index, item) in optional_array(map, "links", path)?.iter().enumerate() {
        let item_path = path.join("links").join(index);
        let link = link(item, &item_path)?;
        unique(&link.id, &item_path)?;
        for (end, card) in [("from", &link.from), ("to", &link.to)] {
            if page.card(card).is_none() {
                return Err(item_path
                    .join(end)
                    .fail(format!("refers to an unknown card `{card}`")));
            }
        }
        if link.from == link.to {
            return Err(item_path.join("to").fail("must be another card than `from`"));
        }
        page.links.push(link);
    }

    let mut labeled = FxHashSet::default();
    for (index, item) in optional_array(map, "labels", path)?.iter().enumerate() {
        let item_path = path.join("labels").join(index);
        let entry = label(item, &item_path)?;
        if !labeled.insert((entry.project.clone(), entry.key.clone())) {
            return Err(item_path.fail(format!("repeats the label of `{}`", entry.key)));
        }
        page.labels.push(entry);
    }
    let mut guarded = FxHashSet::default();
    for (index, item) in optional_array(map, "guards", path)?.iter().enumerate() {
        let item_path = path.join("guards").join(index);
        let entry = guard(item, &item_path)?;
        if !guarded.insert((entry.project.clone(), entry.key.clone())) {
            return Err(item_path.fail(format!("repeats the guard of `{}`", entry.key)));
        }
        page.guards.push(entry);
    }
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

/// How a document is framed.
#[derive(Clone, Copy)]
enum Framing<'a> {
    /// A draft or published board file.
    File,
    /// The changes copy of published board `name`.
    Changes(&'a str),
}

/// A board file's document, and the base revision when it is a changes copy.
fn document(text: &str, framing: Framing<'_>) -> Parsed<(Board, Option<Revision>)> {
    let value = json(text)?;
    let root = Path::default();
    let common = ["folder", "format", "pages", "projects", "title", "version"];
    let required = ["format", "pages", "projects", "title", "version"];
    let map = match framing {
        Framing::File => object(&value, &root, &common, &required)?,
        Framing::Changes(_) => {
            let mut allowed = common.to_vec();
            allowed.extend(["baseRevision", "publishes"]);
            let mut needed = required.to_vec();
            needed.extend(["baseRevision", "publishes"]);
            object(&value, &root, &allowed, &needed)?
        }
    };
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
            if !is_folder_path(folder) {
                return Err(folder_path.fail(
                    "must be a folder path: 1 to 8 names joined by `/`, each 1 to 64 characters \
                     without leading or trailing spaces, control characters, or `\\`, and not \
                     `.` or `..`",
                ));
            }
            Some(folder.to_owned())
        }
    };
    let board = Board::with_pages(title, folder, pages(field(map, "pages"), &root.join("pages"))?);

    let projects_path = root.join("projects");
    let mut projects = Vec::new();
    for (index, item) in array(field(map, "projects"), &projects_path)?.iter().enumerate() {
        projects.push(project(item, &projects_path.join(index))?);
    }
    if projects != board.projects {
        let expected: Vec<String> = board.projects.iter().map(|name| format!("\"{name}\"")).collect();
        return Err(projects_path.fail(format!(
            "must list exactly the other projects the cards name, sorted, without repeats: [{}]",
            expected.join(", ")
        )));
    }

    let base = match framing {
        Framing::File => None,
        Framing::Changes(name) => {
            let publishes_path = root.join("publishes");
            let expected = publishes(name);
            if string(field(map, "publishes"), &publishes_path)? != expected {
                return Err(publishes_path.fail(format!(
                    "must be \"{expected}\", the published board this copy holds changes for"
                )));
            }
            let base_path = root.join("baseRevision");
            let base = ContentHash::from_hex(string(field(map, "baseRevision"), &base_path)?)
                .map_err(|_| base_path.fail("must be 64 lowercase hex digits"))?;
            Some(Revision(base))
        }
    };
    Ok((board, base))
}

/// The `publishes` value of the changes copy of published board `name`.
pub(crate) fn publishes(name: &str) -> String {
    format!("boards/{name}.json")
}

/// Parses board file text (a draft or a published board).
pub fn parse(text: &str) -> Result<Board, BoardError> {
    document(text, Framing::File).map(|(board, _)| board)
}

/// Parses the text of the changes copy of published board `name` (its file stem): the board
/// and the published revision the copy started from.
pub(crate) fn parse_changes(text: &str, name: &str) -> Result<(Board, Revision), BoardError> {
    let (board, base) = document(text, Framing::Changes(name))?;
    let base = base.ok_or_else(|| Path::default().fail("missing field `baseRevision`"))?;
    Ok((board, base))
}

/// Parses an edit the UI saves (`{title, pages}`, contract §6.5): the pages with the page
/// rules, `projects` derived from the cards, and `folder` from the caller (the folder is the
/// board's metadata, never part of an edit).
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
