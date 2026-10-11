//! The `unfold-board` version 1 document (docs/reader.md#boards, decisions board format, board references, cross-project boards).
//!
//! A board is a titled list of **pages**, optionally filed in a folder. Each page holds authored
//! choices only: references to blocks and entrances by structural key (in the board's own
//! project or in another project named by its shared name), canvas positions, each card's saved
//! view and fold, groups, descriptions, Flow labels, the guards that keep references from
//! silently reattaching to different code, links drawn between cards (connections, when they
//! have a kind), frames around items, resource cards, and notes. It never holds copied source or
//! compiled arrows.

use std::fmt;

use rustc_hash::FxHashSet;
use serde::Serialize;
use unfold_core::{ContentHash, Key, RepoPath};

/// The `format` value.
pub const FORMAT: &str = "unfold-board";

/// The only `version`.
pub const VERSION: u32 = 1;

/// The JSON Schema of the format.
pub const SCHEMA: &str = include_str!("../../schema/unfold-board.schema.json");

/// The id of the page a new board starts with.
const FIRST_PAGE: &str = "page-1";

/// The most characters a note holds.
pub const MAX_NOTE_CHARACTERS: usize = 400;

/// How deep frames nest: a frame in a frame in a frame.
pub const MAX_FRAME_DEPTH: usize = 3;

/// Declares a closed set of words of the format: an enum serialized as its words, with the words
/// in declaration order (for messages), `as_str`, and `parse`.
macro_rules! words {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident = $text:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        pub enum $name {
            $( $(#[$variant_meta])* #[serde(rename = $text)] $variant, )+
        }

        impl $name {
            /// Every word, in declaration order.
            pub(crate) const WORDS: &'static [&'static str] = &[$($text),+];

            /// The serialized word.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }

            /// The value `word` names.
            pub(crate) fn parse(word: &str) -> Option<Self> {
                match word {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

words! {
    /// A card's view.
    View {
        /// The exact verified source.
        Code = "code",
        /// Paths from written structure.
        Flow = "flow",
        /// One compact node: the kind's mark, the name, the location, and the description.
        Diagram = "diagram",
    }
}

words! {
    /// How a frame is drawn.
    FrameLook {
        /// An outlined box with a title bar: a unit of several functions, or a service.
        Box = "box",
        /// A tinted background with a large title and no border: an area that holds services.
        Zone = "zone",
    }
}

words! {
    /// What a frame is, as an icon.
    FrameIcon {
        /// An API.
        Api = "api",
        /// A worker.
        Worker = "worker",
        /// A schedule.
        Schedule = "schedule",
        /// A command-line tool.
        Cli = "cli",
        /// A web app.
        Web = "web",
        /// A library: shared code.
        Library = "library",
        /// A system.
        System = "system",
    }
}

words! {
    /// The way a frame lays its members out.
    FrameDirection {
        /// Left to right.
        Right = "right",
        /// Top to bottom.
        Down = "down",
    }
}

words! {
    /// What a resource card stands for.
    ResourceKind {
        /// A database table or collection.
        Table = "table",
        /// A queue or topic.
        Queue = "queue",
        /// A cache.
        Cache = "cache",
        /// Files or object storage.
        Storage = "storage",
        /// A service outside the workspace.
        External = "external",
        /// Who or what starts things: the edge of the diagram.
        Actor = "actor",
    }
}

words! {
    /// A mark on a table's column.
    ColumnKey {
        /// The primary key.
        Primary = "primary",
        /// A foreign key.
        Foreign = "foreign",
        /// A unique column.
        Unique = "unique",
    }
}

words! {
    /// How the two ends of a connection talk.
    LinkKind {
        /// An HTTP request: a client call to a route.
        Http = "http",
        /// Another request–response protocol: gRPC, tRPC, GraphQL.
        Rpc = "rpc",
        /// A job or message through a queue or topic.
        Queue = "queue",
        /// An event emitted and handled.
        Event = "event",
        /// Code that inserts, updates, or deletes.
        Writes = "writes",
        /// Code that queries or loads.
        Reads = "reads",
        /// Where a call the graph can't follow goes when the program runs.
        Calls = "calls",
        /// A relationship between two tables.
        Relates = "relates",
        /// Anything else (a link without a kind is one too).
        Other = "other",
    }
}

words! {
    /// How many rows of each table a `relates` connection joins.
    Cardinality {
        /// One to one.
        OneToOne = "one-to-one",
        /// One to many.
        OneToMany = "one-to-many",
        /// Many to one.
        ManyToOne = "many-to-one",
        /// Many to many.
        ManyToMany = "many-to-many",
    }
}

/// A project's shared name (`[project] name` in its `unfold.toml`, or its default):
/// a letter or digit, then up to 63 letters, digits, `.`, `_`, or `-`. Boards name other
/// projects by it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
#[serde(transparent)]
pub struct ProjectName(Box<str>);

impl ProjectName {
    /// The name, when `text` is a valid shared name.
    pub fn parse(text: &str) -> Option<Self> {
        is_slug(text, 64).then(|| Self(text.into()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `text` is a letter or digit followed by letters, digits, `.`, `_`, or `-`, with at
/// most `max` characters in all.
fn is_slug(text: &str, max: usize) -> bool {
    text.len() <= max
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

/// Whether `id` is a valid page or item id: a letter or digit, then up to 99 letters, digits,
/// `_`, `.`, or `-`.
pub(crate) fn is_id(id: &str) -> bool {
    is_slug(id, 100)
}

/// Whether `path` is a valid board folder path: 1 to 8 segments joined by `/`, each 1 to 64
/// characters without leading or trailing whitespace, control characters, `/`, or `\`, and not
/// `.` or `..`. Folder names compare exactly.
pub fn is_folder_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    (1..=8).contains(&segments.len())
        && segments.iter().all(|segment| {
            let length = segment.chars().count();
            (1..=64).contains(&length)
                && segment.trim() == *segment
                && !segment
                    .chars()
                    .any(|character| character.is_control() || character == '\\')
                && *segment != "."
                && *segment != ".."
        })
}

/// Who wrote something: `person:<name>` or `agent:<name>`.
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

/// A description (of a card, group, frame, resource, or connection), a Flow label's text, or a
/// drawn link's label.
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

/// What a card shows: exactly one block or entrance, by structural key, in the board's own
/// project (`project` absent) or in the project with that shared name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum CardRef {
    /// A block (a definition or another indexed block).
    Block {
        /// The block's key.
        block: Key,
        /// Its project; absent for the board's own.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        project: Option<ProjectName>,
    },
    /// A declared entrance.
    Entrance {
        /// The entrance's key.
        entrance: Key,
        /// Its project; absent for the board's own.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        project: Option<ProjectName>,
    },
}

impl CardRef {
    /// A block in `project` (`None`: the board's own).
    pub fn block(key: Key, project: Option<ProjectName>) -> Self {
        Self::Block { block: key, project }
    }

    /// An entrance in `project` (`None`: the board's own).
    pub fn entrance(key: Key, project: Option<ProjectName>) -> Self {
        Self::Entrance {
            entrance: key,
            project,
        }
    }

    /// The referenced key.
    pub fn key(&self) -> &Key {
        match self {
            Self::Block { block: key, .. } | Self::Entrance { entrance: key, .. } => key,
        }
    }

    /// The block key, for a block card.
    pub fn block_key(&self) -> Option<&Key> {
        match self {
            Self::Block { block, .. } => Some(block),
            Self::Entrance { .. } => None,
        }
    }

    /// The project; `None` for the board's own.
    pub fn project(&self) -> Option<&ProjectName> {
        match self {
            Self::Block { project, .. } | Self::Entrance { project, .. } => project.as_ref(),
        }
    }

    /// The same kind of reference to `key` in the same project.
    pub(crate) fn with_key(&self, key: Key) -> Self {
        match self {
            Self::Block { project, .. } => Self::block(key, project.clone()),
            Self::Entrance { project, .. } => Self::entrance(key, project.clone()),
        }
    }
}

/// One card on a page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Unique among the page's items.
    pub id: String,
    /// What it shows.
    #[serde(rename = "ref")]
    pub reference: CardRef,
    /// The canvas position: finite numbers.
    pub at: [f64; 2],
    /// The card's own view; the page's default when absent.
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

/// A run of consecutive sibling statements inside one block card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Unique among the page's items.
    pub id: String,
    /// The card it belongs to; its members are keys of that card's project.
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
    /// Who wrote the title; absent on groups titled before titles were attributed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub author: Option<Author>,
}

/// A Flow-node label: the authored name of a block inside a card, in that card's project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LabelEntry {
    /// The labeled block's project; absent for the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub project: Option<ProjectName>,
    /// The labeled block's key.
    pub key: Key,
    /// The text, author, and signature (of the block's source).
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub label: Description,
}

/// What a reference was when it was saved, so it can reconnect safely: one per referenced
/// (project, key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GuardEntry {
    /// The reference's project; absent for the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub project: Option<ProjectName>,
    /// The referenced key.
    pub key: Key,
    /// The block kind, or `entrance:<kind>`.
    pub kind: String,
    /// The owner's key for a block; empty for an entrance.
    pub owner: String,
    /// The signature of the referenced source.
    pub signature: Signature,
}

/// What a resource card or a connection cites, which Unfold can verify: a block or entrance key,
/// or a line range of an evidence file, in the board's own project (`project` absent) or in the
/// project with that shared name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum Anchor {
    /// A block key (`path#…`) or an entrance key (`entrance:[…]`).
    Key {
        /// The key.
        key: Key,
        /// Its project; absent for the board's own.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        project: Option<ProjectName>,
    },
    /// Lines of an evidence file.
    File {
        /// The file, relative to its project's folder.
        file: RepoPath,
        /// The first and last line, 1-based and inclusive.
        lines: [u32; 2],
        /// Its project; absent for the board's own.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        project: Option<ProjectName>,
    },
}

impl Anchor {
    /// The project; `None` for the board's own.
    pub fn project(&self) -> Option<&ProjectName> {
        match self {
            Self::Key { project, .. } | Self::File { project, .. } => project.as_ref(),
        }
    }

    /// The key, for a key anchor.
    pub fn key(&self) -> Option<&Key> {
        match self {
            Self::Key { key, .. } => Some(key),
            Self::File { .. } => None,
        }
    }
}

/// A link drawn between two items of a page (cross-project boards): the graph does not join code
/// across projects, so people and agents do. A link with a `kind` is a **connection**: a typed
/// line that says how its ends talk, with its evidence; one without is drawn as `other`, with an
/// optional signed label ("calls over HTTP").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// Unique among the page's items.
    pub id: String,
    /// The card, resource, or frame it starts at: the side that acts.
    pub from: String,
    /// The card, resource, or frame it points at; never `from`.
    pub to: String,
    /// Its label, signed over both ends' guard signatures (informational: never outdated).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<Description>,
    /// How its ends talk; absent for a drawn link (`other`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<LinkKind>,
    /// The statement or call inside the `from` card's block it leaves from (same project).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub step: Option<Key>,
    /// For `relates`: the column of the `from` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub from_column: Option<String>,
    /// For `relates`: the column of the `to` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub to_column: Option<String>,
    /// What travels: the route, the queue, or the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub channel: Option<String>,
    /// The type it carries: a type declaration or class key.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub carries: Option<Key>,
    /// The project of `carries`; absent for the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub carries_project: Option<ProjectName>,
    /// For `relates`: how many rows of each table it joins.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cardinality: Option<Cardinality>,
    /// The evidence for the receiving side.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub anchors: Vec<Anchor>,
    /// The evidence in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<Description>,
    /// Who drew it; every connection has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub author: Option<Author>,
    /// The signature of its step's and anchors' source; absent until a save signs it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub signature: Option<Signature>,
}

impl Link {
    /// A drawn link from `from` to `to`, without a label or a kind.
    pub fn new(id: &str, from: &str, to: &str) -> Self {
        Self {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            label: None,
            kind: None,
            step: None,
            from_column: None,
            to_column: None,
            channel: None,
            carries: None,
            carries_project: None,
            cardinality: None,
            anchors: Vec::new(),
            description: None,
            author: None,
            signature: None,
        }
    }

    /// Whether it is a `relates` connection.
    pub fn relates(&self) -> bool {
        self.kind == Some(LinkKind::Relates)
    }
}

/// What a page or a board references in one project: what [`super::BoardGraph::load`] reads
/// from that project's graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct References<'a> {
    /// Block and entrance keys, without duplicates: cards, group members, and labels, then the
    /// claims' keys (tables' types, connections' steps and carried types, and key anchors).
    pub keys: Vec<&'a str>,
    /// The types of tables backed by a type, whose fields a save checks marks against.
    pub tables: Vec<&'a str>,
    /// The evidence files anchors cite, without duplicates.
    pub files: Vec<&'a RepoPath>,
}

impl<'a> References<'a> {
    /// Adds what `other` references.
    fn append(&mut self, other: References<'a>) {
        self.keys.extend(other.keys);
        self.tables.extend(other.tables);
        self.files.extend(other.files);
    }

    /// Keeps the first of each repeated key, table, and file.
    fn dedup(mut self) -> Self {
        fn first<T: Copy + Eq + std::hash::Hash>(items: &mut Vec<T>) {
            let mut seen = FxHashSet::default();
            items.retain(|item| seen.insert(*item));
        }
        first(&mut self.keys);
        first(&mut self.tables);
        first(&mut self.files);
        self
    }
}

/// A named box around items of a page: a unit, a service, or a zone. It has no position of its
/// own: its box is its members' bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// Unique among the page's items.
    pub id: String,
    /// The title; never blank.
    pub title: String,
    /// The frame's description, signed over its member cards' guards.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<Description>,
    /// The ids of the cards, resources, notes, and frames it holds, in order: at least one,
    /// none in another frame.
    pub members: Vec<String>,
    /// How it is drawn.
    pub look: FrameLook,
    /// What it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub icon: Option<FrameIcon>,
    /// The way it lays its members out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub direction: Option<FrameDirection>,
    /// Collapsed to one node (written only when true).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub collapsed: bool,
    /// Who drew it.
    pub author: Author,
}

/// One column of a table card. A table backed by a type lists marks only (`name`, `key`,
/// `references`), naming fields of the type; the type gives the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Column {
    /// The column's name; unique in its table.
    pub name: String,
    /// Its type as written.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub written_type: Option<String>,
    /// Its mark: primary key, foreign key, or unique.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<ColumnKey>,
    /// What a foreign key references, such as `customers.id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub references: Option<String>,
    /// The column may be empty (written only when true).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub optional: bool,
}

/// A resource card: something the code talks to that isn't a function (a table, a queue, a
/// cache, storage, an outside service, or an actor), authored and anchored.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Resource {
    /// Unique among the page's items.
    pub id: String,
    /// What it stands for.
    pub kind: ResourceKind,
    /// Its technical name, such as `orders`; never blank.
    pub name: String,
    /// The project of `type`; absent for the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub project: Option<ProjectName>,
    /// For a table: the type declaration or class that declares its shape.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub type_key: Option<Key>,
    /// For a table: its columns, or a type-backed table's marks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<Column>,
    /// What it is anchored to.
    pub anchors: Vec<Anchor>,
    /// The canvas position: finite numbers.
    pub at: [f64; 2],
    /// Its description.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<Description>,
    /// Who added it.
    pub author: Author,
    /// The signature of its anchors' (and type's) source; absent until a save signs it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub signature: Option<Signature>,
}

/// A short piece of plain text on a page: free, or attached to a card, frame, resource, or link
/// and moving with it. Notes are about the page, not the code: never signed, no health.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Note {
    /// Unique among the page's items.
    pub id: String,
    /// The text: 1 to 400 characters, not blank.
    pub text: String,
    /// The canvas position; an offset from the attached item's top-left (a link's midpoint) when
    /// attached.
    pub at: [f64; 2],
    /// The card, frame, resource, or link it is attached to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub attach: Option<String>,
    /// Who wrote it.
    pub author: Author,
}

/// One page of a board.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// Unique in the board.
    pub id: String,
    /// The title; never blank.
    pub title: String,
    /// The view of cards without their own.
    pub default_view: View,
    /// The cards.
    pub cards: Vec<Card>,
    /// The groups.
    pub groups: Vec<Group>,
    /// Flow-node labels, one per (project, key).
    pub labels: Vec<LabelEntry>,
    /// Guards, one per referenced (project, key).
    pub guards: Vec<GuardEntry>,
    /// Drawn links and connections.
    pub links: Vec<Link>,
    /// Frames.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub frames: Vec<Frame>,
    /// Resource cards.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub resources: Vec<Resource>,
    /// Notes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub notes: Vec<Note>,
}

impl Page {
    /// An empty page.
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_owned(),
            title: title.to_owned(),
            default_view: View::Code,
            cards: Vec::new(),
            groups: Vec::new(),
            labels: Vec::new(),
            guards: Vec::new(),
            links: Vec::new(),
            frames: Vec::new(),
            resources: Vec::new(),
            notes: Vec::new(),
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

    /// The link with `id`.
    pub fn link(&self, id: &str) -> Option<&Link> {
        self.links.iter().find(|link| link.id == id)
    }

    /// The frame with `id`.
    pub fn frame(&self, id: &str) -> Option<&Frame> {
        self.frames.iter().find(|frame| frame.id == id)
    }

    /// The resource card with `id`.
    pub fn resource(&self, id: &str) -> Option<&Resource> {
        self.resources.iter().find(|resource| resource.id == id)
    }

    /// The note with `id`.
    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    /// The other projects the page's items name: its cards', resources' types', anchors', and
    /// carried types' projects, with repeats.
    pub fn projects(&self) -> impl Iterator<Item = &ProjectName> {
        let cards = self.cards.iter().filter_map(|card| card.reference.project());
        let resources = self.resources.iter().flat_map(|resource| {
            resource
                .project
                .iter()
                .chain(resource.anchors.iter().filter_map(Anchor::project))
        });
        let links = self.links.iter().flat_map(|link| {
            link.carries_project
                .iter()
                .chain(link.anchors.iter().filter_map(Anchor::project))
        });
        cards.chain(resources).chain(links)
    }

    /// A card's effective view.
    pub fn view_of(&self, card: &Card) -> View {
        card.view.unwrap_or(self.default_view)
    }

    /// The project of group `group`'s members: its card's project (`None`: the board's own,
    /// also when the card is gone).
    pub fn group_project(&self, group: &Group) -> Option<&ProjectName> {
        self.card(&group.card).and_then(|card| card.reference.project())
    }

    /// The label of (`project`, `key`).
    pub fn label_entry(&self, project: Option<&ProjectName>, key: &str) -> Option<&LabelEntry> {
        self.labels
            .iter()
            .find(|entry| entry.project.as_ref() == project && entry.key.as_str() == key)
    }

    /// The authored label text of block (`project`, `key`), when it has text.
    pub fn label(&self, project: Option<&ProjectName>, key: &str) -> Option<&str> {
        self.label_entry(project, key)
            .map(|entry| entry.label.text.trim())
            .filter(|text| !text.is_empty())
    }

    /// The guard of (`project`, `key`).
    pub fn guard(&self, project: Option<&ProjectName>, key: &str) -> Option<&GuardEntry> {
        self.guards
            .iter()
            .find(|entry| entry.project.as_ref() == project && entry.key.as_str() == key)
    }

    /// The project of connection `link`'s step: its `from` card's project (`None`: the board's
    /// own, also when the card is gone).
    pub fn step_project(&self, link: &Link) -> Option<&ProjectName> {
        self.card(&link.from).and_then(|card| card.reference.project())
    }

    /// Everything the page references in `project` (`None`: the board's own), without
    /// duplicates: the keys of cards, group members, and labels, in that order, then of the
    /// claims (tables' types, connections' steps and carried types, and key anchors); the types
    /// of tables backed by one; and the evidence files anchors cite.
    pub fn references(&self, project: Option<&ProjectName>) -> References<'_> {
        let mut references = References::default();
        references.keys.extend(
            self.cards
                .iter()
                .filter(|card| card.reference.project() == project)
                .map(|card| card.reference.key().as_str()),
        );
        references.keys.extend(
            self.groups
                .iter()
                .filter(|group| self.group_project(group) == project)
                .flat_map(|group| group.members.iter().map(Key::as_str)),
        );
        references.keys.extend(
            self.labels
                .iter()
                .filter(|entry| entry.project.as_ref() == project)
                .map(|entry| entry.key.as_str()),
        );
        for table in self
            .resources
            .iter()
            .filter(|resource| resource.project.as_ref() == project)
            .filter_map(|resource| resource.type_key.as_ref())
        {
            references.keys.push(table.as_str());
            references.tables.push(table.as_str());
        }
        for link in &self.links {
            if let Some(step) = link.step.as_ref().filter(|_| self.step_project(link) == project) {
                references.keys.push(step.as_str());
            }
            if let Some(carries) = link
                .carries
                .as_ref()
                .filter(|_| link.carries_project.as_ref() == project)
            {
                references.keys.push(carries.as_str());
            }
        }
        let anchors = self
            .resources
            .iter()
            .flat_map(|resource| &resource.anchors)
            .chain(self.links.iter().flat_map(|link| &link.anchors))
            .filter(|anchor| anchor.project() == project);
        for anchor in anchors {
            match anchor {
                Anchor::Key { key, .. } => references.keys.push(key.as_str()),
                Anchor::File { file, .. } => references.files.push(file),
            }
        }
        references.dedup()
    }
}

/// An `unfold-board` version 1 document (without the framing a changes copy adds).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// The title; never blank.
    pub title: String,
    /// The board folder it is filed in (a `/`-separated path); `None` at the top level.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub folder: Option<String>,
    /// The other projects its items name, sorted: [`Board::used_projects`]. The constructors,
    /// [`super::parse`], [`super::parse_edit`], and [`super::prepare_save`] set it; after
    /// editing items in place, call [`Board::refresh_projects`]. The writer always writes the
    /// derived set.
    pub projects: Vec<ProjectName>,
    /// The pages, in order; at least one.
    pub pages: Vec<Page>,
}

impl Board {
    /// A board with one empty page, `page-1` titled "Page 1".
    pub fn new(title: &str) -> Self {
        Self::with_pages(title, None, vec![Page::new(FIRST_PAGE, "Page 1")])
    }

    /// A board of `pages`, with its projects derived from them.
    pub fn with_pages(title: &str, folder: Option<String>, pages: Vec<Page>) -> Self {
        let mut board = Self {
            title: title.to_owned(),
            folder,
            projects: Vec::new(),
            pages,
        };
        board.refresh_projects();
        board
    }

    /// The page with `id`.
    pub fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    /// The page with `id`, to edit (call [`Board::refresh_projects`] after changing its items).
    pub fn page_mut(&mut self, id: &str) -> Option<&mut Page> {
        self.pages.iter_mut().find(|page| page.id == id)
    }

    /// The other projects every page's items name ([`Page::projects`]), sorted, without
    /// duplicates.
    pub fn used_projects(&self) -> Vec<ProjectName> {
        let mut projects: Vec<ProjectName> = self.pages.iter().flat_map(Page::projects).cloned().collect();
        projects.sort();
        projects.dedup();
        projects
    }

    /// Sets [`Board::projects`] from the items.
    pub fn refresh_projects(&mut self) {
        self.projects = self.used_projects();
    }

    /// Everything any page references in `project` (`None`: the board's own), without
    /// duplicates: what [`super::BoardGraph::load`] needs to check the whole board.
    pub fn references(&self, project: Option<&ProjectName>) -> References<'_> {
        let mut references = References::default();
        for page in &self.pages {
            references.append(page.references(project));
        }
        references.dedup()
    }
}

/// Whether a key is position-based: it contains an ordinal step (`if[0]`, `function:f[1]`),
/// which shifts when code is inserted before it.
pub(crate) fn is_ordinal(key: &str) -> bool {
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
        assert_eq!(Author::person("ada").unwrap().name(), "ada");
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

    #[test]
    fn names_ids_and_folders() {
        for good in ["storefront-api", "a", "A.b_c-9", &"x".repeat(64)] {
            assert!(ProjectName::parse(good).is_some(), "{good}");
        }
        for bad in ["", "-a", ".a", "a b", "a/b", "é", &"x".repeat(65)] {
            assert!(ProjectName::parse(bad).is_none(), "{bad}");
        }
        assert!(is_id("page-1") && is_id(&"p".repeat(100)) && is_id("a.b_c"));
        assert!(!is_id("") && !is_id("_a") && !is_id("a b") && !is_id(&"p".repeat(101)));
        for good in [
            "Checkout",
            "Checkout/Refunds",
            "a/b/c/d/e/f/g/h",
            "Café crème",
            &"x".repeat(64),
        ] {
            assert!(is_folder_path(good), "{good}");
        }
        for bad in [
            "",
            "/a",
            "a/",
            "a//b",
            " a",
            "a ",
            ".",
            "a/..",
            "a\\b",
            "a\tb",
            "a/b/c/d/e/f/g/h/i",
            &"x".repeat(65),
        ] {
            assert!(!is_folder_path(bad), "{bad:?}");
        }
    }
}
