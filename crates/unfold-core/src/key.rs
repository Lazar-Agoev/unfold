//! Structural keys: one readable name for every block, app, and entrance, which survives most
//! edits (`docs/designs/diagram-blocks.md`, "Keys"; `docs/designs/entrance-definitions.md`,
//! "Keys").
//!
//! ```text
//! key    = path "#"                               a module
//!        | path "#" names [ "/" steps ]           a named definition, and the blocks inside it
//!        | path "#/" steps                        the blocks of a module's own code
//!        | "entrance:" app [ "/" entrance ]       an app's start, or one of its entrances
//! names  = name [ "[" n "]" ] { "." name [ "[" n "]" ] }
//! steps  = step { "/" step }
//! step   = control "[" n "]" | part | exit "[" n "]"
//!        | "call:" callee [ "(" literal ")" ] [ "[" n "]" ]
//!        | inline "[" n "]" [ "." names ]
//! ```
//!
//! | Block | Step |
//! | --- | --- |
//! | Named definition | its name after `#` (in a module) or after its enclosing definition's key and `.`, wherever it sits; `[n]` for the n-th (n ≥ 1) repeat of that key |
//! | Control block, exit | its form and its ordinal among the parent's children of that form: `if[0]`, `return[1]` |
//! | Part | `then`, `else`, `default`, `body`, `catch`, `finally`; `elif[n]`, `case[n]`, `except[n]` |
//! | Inline function | `callback[n]` in a call's arguments, `lambda[n]` anywhere else |
//! | Call | `call:` and its normalized callee ([`Callee`]); its first argument as a double-quoted literal when it is a string and the call takes a callback; `[n]` for the n-th (n ≥ 1) repeat of that step under the same parent |
//!
//! Names and callees percent-encode `%`, `/`, `#`, `[`, and `]`. A literal is read up to its
//! closing quote, so only `"` and `\` are escaped in it, with a backslash.
//!
//! This module is the only place keys are made ([`KeyBuilder`], [`Key::app`],
//! [`Key::entrance`]) and the only place they are read ([`Key::parse`], and [`Key::rebase`] for
//! moving the keys inside a renamed block). Everything else treats a key as opaque text and
//! gets kinds, parents, and files from records.

use std::borrow::Borrow;
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::path::RepoPath;
use crate::vocab::{Form, Inline, NodeKind};

/// A structural key. Cloning is cheap (a reference count).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct Key(Arc<str>);

impl Key {
    /// Wraps key text read from storage, an export, or `unfold.toml`. The text is not checked;
    /// [`Key::parse`] reads it.
    pub fn from_text(text: &str) -> Self {
        Self(Arc::from(text))
    }

    /// The key of `path`'s module: `path#`.
    pub fn module(path: &RepoPath) -> Self {
        let mut text = String::with_capacity(path.as_str().len() + 1);
        text.push_str(path.as_str());
        text.push('#');
        Self(Arc::from(text))
    }

    /// The key of the app named `name`: `entrance:<slug>`.
    pub fn app(name: &str) -> Result<Self, SlugError> {
        Ok(Self(Arc::from(format!("{ENTRANCE_PREFIX}{}", slug(name)?))))
    }

    /// The key of the entrance named `entrance` in the app named `app`:
    /// `entrance:<app slug>/<entrance slug>`.
    pub fn entrance(app: &str, entrance: &str) -> Result<Self, SlugError> {
        let text = format!("{ENTRANCE_PREFIX}{}/{}", slug(app)?, slug(entrance)?);
        Ok(Self(Arc::from(text)))
    }

    /// The key's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reads the key's structure.
    pub fn parse(&self) -> Result<ParsedKey, KeyParseError> {
        ParsedKey::parse(&self.0)
    }

    /// This key after block `from` became block `to`: `to` when this key is `from`, and for a
    /// block inside `from`, the same steps and names under `to` (`a.ts#f/if[0]/call:g`, from
    /// `a.ts#f` to `b.ts#h`, is `b.ts#h/if[0]/call:g`).
    ///
    /// Inside is read from the keys' structure, never their text: `a.ts#format` is not inside
    /// `a.ts#f`, nor `a.ts#f[1]` inside `a.ts#f`, but the nested definition `a.ts#f.g` is.
    /// `None` when this key is neither `from` nor inside it, when any of the three is not a
    /// block key, or when the steps can't follow `to` (a name after a control block or a call).
    pub fn rebase(&self, from: &Key, to: &Key) -> Option<Key> {
        let (Ok(ParsedKey::Block(this)), Ok(ParsedKey::Block(from)), Ok(ParsedKey::Block(to))) =
            (self.parse(), from.parse(), to.parse())
        else {
            return None;
        };
        if this.path != from.path {
            return None;
        }
        let inside = this.segments.strip_prefix(from.segments.as_slice())?;
        let name_after_step = matches!(inside.first(), Some(Segment::Name { .. }))
            && matches!(
                to.segments.last(),
                Some(Segment::Form { .. } | Segment::Call { .. })
            );
        if name_after_step {
            return None;
        }
        let mut segments = to.segments;
        segments.extend_from_slice(inside);
        Some(
            BlockKey {
                path: to.path,
                segments,
            }
            .to_key(),
        )
    }

    /// This key's steps and names after block `outer`'s, as the short label agents read and
    /// write: `try[0]/except[0]` in `a.py#f` for `a.py#f/try[0]/except[0]`, `inner/call:g` for
    /// `a.py#f.inner/call:g`. Empty for `outer` itself.
    ///
    /// Inside is read from the keys' structure, as [`Key::rebase`] reads it. `None` when this
    /// key is not `outer` or inside it, or when either is not a block key.
    pub fn steps_after(&self, outer: &Key) -> Option<String> {
        let (Ok(ParsedKey::Block(this)), Ok(ParsedKey::Block(outer))) = (self.parse(), outer.parse()) else {
            return None;
        };
        if this.path != outer.path {
            return None;
        }
        let inside = this.segments.strip_prefix(outer.segments.as_slice())?;
        let mut label = String::new();
        for (index, segment) in inside.iter().enumerate() {
            match segment {
                _ if index == 0 => {}
                Segment::Name { .. } => label.push('.'),
                _ => label.push('/'),
            }
            // Writing to a String cannot fail.
            let _ = write!(label, "{segment}");
        }
        Some(label)
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

const ENTRANCE_PREFIX: &str = "entrance:";
const CALL_PREFIX: &str = "call:";

/// A name with no letters or digits, which can't make an app or entrance key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("name `{0}` has no letters or digits")]
pub struct SlugError(pub Box<str>);

/// The slug of an app or entrance name: lowercased, every run of characters other than letters
/// and digits replaced by `-`, and leading and trailing `-` dropped.
pub fn slug(name: &str) -> Result<String, SlugError> {
    let mut slug = String::with_capacity(name.len());
    let mut gap = false;
    for character in name.to_lowercase().chars() {
        if !character.is_alphanumeric() {
            gap = true;
            continue;
        }
        if gap && !slug.is_empty() {
            slug.push('-');
        }
        gap = false;
        slug.push(character);
    }
    if slug.is_empty() {
        return Err(SlugError(name.into()));
    }
    Ok(slug)
}

/// A call's callee as the key and the node's name write it, built from the written expression
/// with each language's syntax already read:
///
/// | Written | Callee |
/// | --- | --- |
/// | an identifier, a member name | itself, members after `.` (`?.` is written `.`) |
/// | arguments | `()` |
/// | a subscript | `[]` |
/// | a string literal | `""` |
/// | any other expression | `(…)` |
///
/// Front ends drop `await`, parentheses, TypeScript's `!`, and type casts before building it.
/// So `res.status(500).json(…)` is `Callee::name("res").member("status").call().member("json")`,
/// and `(row.get("x") or "").strip()` is `Callee::expression().member("strip")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Callee(String);

impl Callee {
    /// The most characters a callee keeps; a longer one is cut.
    pub const MAX_CHARS: usize = 80;

    /// A callee that starts with the name `name`.
    pub fn name(name: &str) -> Self {
        Self(name.to_owned())
    }

    /// A callee that starts with a string literal: `""`.
    pub fn string() -> Self {
        Self("\"\"".to_owned())
    }

    /// A callee that starts with any other expression: `(…)`.
    pub fn expression() -> Self {
        Self("(…)".to_owned())
    }

    /// Adds a member access: `.name`.
    #[must_use]
    pub fn member(mut self, name: &str) -> Self {
        self.0.push('.');
        self.0.push_str(name);
        self
    }

    /// Adds a call: `()`.
    #[must_use]
    pub fn call(mut self) -> Self {
        self.0.push_str("()");
        self
    }

    /// Adds a subscript: `[]`.
    #[must_use]
    pub fn index(mut self) -> Self {
        self.0.push_str("[]");
        self
    }

    /// The callee as the node's name records it: unescaped, and cut at
    /// [`Callee::MAX_CHARS`] characters.
    pub fn text(&self) -> &str {
        match self.0.char_indices().nth(Self::MAX_CHARS) {
            Some((cut, _)) => &self.0[..cut],
            None => &self.0,
        }
    }
}

/// What a node contributes to its key, given to [`KeyBuilder::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Child<'a> {
    /// A named definition: a class, a type, or a function with a name. A function bound by a
    /// declaration or a simple assignment takes that name (`const handler = …` is `handler`),
    /// an object-literal method or class property its property name qualified by its object
    /// (`handlers.onCreate`), and an anonymous default export `default`.
    Definition(&'a str),
    /// An unnamed function, keyed by where it is written.
    Inline(Inline),
    /// A call.
    Call {
        /// Its normalized callee.
        callee: &'a Callee,
        /// The value of its first argument, when that argument is a string literal.
        first_literal: Option<&'a str>,
        /// Whether any function is written in its arguments ([`Inline::Callback`]). Only then
        /// does the first literal join the key.
        takes_callback: bool,
    },
    /// A control block, a part, or an exit, keyed by its form.
    Form(Form),
}

/// Why a node's key can't be derived.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// A parent must be pushed before its children.
    #[error("parent {parent} has not been pushed")]
    UnknownParent {
        /// The claimed parent's local ID.
        parent: u32,
    },
    /// Definitions are keyed by name or as inline functions, and calls by their callee.
    #[error("`{0}` is keyed by a name or a callee, not by its form")]
    NotAStep(Form),
    /// A part without an ordinal appears once under its parent.
    #[error("a second `{0}` part under one parent")]
    RepeatedPart(Form),
    /// Definitions and callees have nonempty names.
    #[error("an empty name or callee")]
    EmptyName,
}

/// What a node's step is counted with: steps under one parent with the same counter take
/// ordinals 0, 1, 2, … in preorder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Counter {
    Form(Form),
    Inline(Inline),
    /// A call step without its repeat ordinal.
    Call(Box<str>),
}

/// Derives the keys of one file's nodes, pushed in preorder. Local ID 0 is the module.
///
/// A front end lowering its tree pushes each node under its parent's local ID as it creates
/// it; the builder numbers repeats and ordinals among the siblings already pushed, which in
/// preorder are exactly the earlier ones.
///
/// ```
/// use unfold_core::key::{Callee, Child, KeyBuilder};
/// use unfold_core::vocab::Form;
/// use unfold_core::RepoPath;
///
/// let mut keys = KeyBuilder::new(&RepoPath::new("shop/service.py").unwrap());
/// keys.push(0, Child::Definition("place_order")).unwrap(); // 1
/// keys.push(1, Child::Form(Form::Try)).unwrap(); // 2
/// keys.push(2, Child::Form(Form::Body)).unwrap(); // 3
/// let charge = Callee::name("charge_card");
/// let call = Child::Call { callee: &charge, first_literal: None, takes_callback: false };
/// let key = keys.push(3, call).unwrap();
/// assert_eq!(key.as_str(), "shop/service.py#place_order/try[0]/body/call:charge_card");
/// ```
#[derive(Debug)]
pub struct KeyBuilder {
    keys: Vec<Key>,
    /// For each node, the nearest definition at or above it, whose key the names of the named
    /// definitions under it extend.
    scopes: Vec<u32>,
    /// Steps pushed under each parent, by counter.
    steps: FxHashMap<(u32, Counter), u32>,
    /// Named definitions pushed, by their key without a repeat ordinal.
    names: FxHashMap<Box<str>, u32>,
}

impl KeyBuilder {
    /// Starts the file at `path`, with its module as local ID 0.
    pub fn new(path: &RepoPath) -> Self {
        Self {
            keys: vec![Key::module(path)],
            scopes: vec![0],
            steps: FxHashMap::default(),
            names: FxHashMap::default(),
        }
    }

    /// Derives the key of the next node in preorder, a child of `parent`. Its local ID is the
    /// number of nodes pushed before it, the module included. On an error nothing is pushed.
    pub fn push(&mut self, parent: u32, child: Child<'_>) -> Result<Key, KeyError> {
        let parent_index = parent as usize;
        if parent_index >= self.keys.len() {
            return Err(KeyError::UnknownParent { parent });
        }
        let local_id = u32::try_from(self.keys.len()).unwrap_or(u32::MAX);
        let (key, scope) = match child {
            Child::Definition(name) => (self.definition(parent_index, name)?, local_id),
            Child::Inline(inline) => {
                let ordinal = self.next(parent, Counter::Inline(inline));
                let step = Segment::Inline { inline, ordinal };
                (self.step_key(parent_index, &step), local_id)
            }
            Child::Call {
                callee,
                first_literal,
                takes_callback,
            } => {
                let callee = callee.text();
                if callee.is_empty() {
                    return Err(KeyError::EmptyName);
                }
                let literal = first_literal.filter(|_| takes_callback);
                let mut base = String::new();
                write_call(&mut base, callee, literal);
                let repeat = self.next(parent, Counter::Call(base.into()));
                let step = Segment::Call {
                    callee: callee.into(),
                    literal: literal.map(Into::into),
                    repeat: (repeat > 0).then_some(repeat),
                };
                (self.step_key(parent_index, &step), self.scopes[parent_index])
            }
            Child::Form(form) => {
                let numbering = Numbering::of(form).ok_or(KeyError::NotAStep(form))?;
                if numbering == Numbering::Once && self.steps.contains_key(&(parent, Counter::Form(form))) {
                    return Err(KeyError::RepeatedPart(form));
                }
                let ordinal = self.next(parent, Counter::Form(form));
                let ordinal = (numbering == Numbering::Ordinal).then_some(ordinal);
                let step = Segment::Form { form, ordinal };
                (self.step_key(parent_index, &step), self.scopes[parent_index])
            }
        };
        self.keys.push(key.clone());
        self.scopes.push(scope);
        Ok(key)
    }

    /// The keys derived so far, indexed by local ID.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Finishes the file and returns its keys, indexed by local ID.
    pub fn finish(self) -> Vec<Key> {
        self.keys
    }

    fn definition(&mut self, parent: usize, name: &str) -> Result<Key, KeyError> {
        if name.is_empty() {
            return Err(KeyError::EmptyName);
        }
        let scope = self.scopes[parent];
        let mut text = self.keys[scope as usize].as_str().to_owned();
        if scope != 0 {
            text.push('.');
        }
        escape(name, &mut text);
        if let Some(count) = self.names.get_mut(text.as_str()) {
            let _ = write!(text, "[{count}]");
            *count += 1;
        } else {
            self.names.insert(text.as_str().into(), 1);
        }
        Ok(Key(Arc::from(text)))
    }

    /// The number of earlier steps under `parent` with `counter`, counting this one.
    fn next(&mut self, parent: u32, counter: Counter) -> u32 {
        let count = self.steps.entry((parent, counter)).or_insert(0);
        let ordinal = *count;
        *count += 1;
        ordinal
    }

    fn step_key(&self, parent: usize, step: &Segment) -> Key {
        let mut text = self.keys[parent].as_str().to_owned();
        // Writing to a String cannot fail.
        let _ = write!(text, "/{step}");
        Key(Arc::from(text))
    }
}

/// Whether a step of a form carries an ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numbering {
    /// Always: control blocks, exits, and the parts that repeat (`elif`, `case`, `except`).
    Ordinal,
    /// Never, because it appears at most once under its parent: the other parts.
    Once,
}

impl Numbering {
    /// How a form's step is numbered, or `None` for a form that isn't keyed by its form.
    const fn of(form: Form) -> Option<Self> {
        match form.kind() {
            NodeKind::Part => match form {
                Form::Elif | Form::Case | Form::Except => Some(Self::Ordinal),
                _ => Some(Self::Once),
            },
            kind if kind.is_control() => Some(Self::Ordinal),
            NodeKind::Exit => Some(Self::Ordinal),
            _ => None,
        }
    }
}

/// One step of a block key, or one name in its `names`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A named definition's name, unescaped, and its repeat ordinal (n ≥ 1).
    Name {
        /// The name.
        name: Box<str>,
        /// The ordinal of a repeated name, absent for the first.
        repeat: Option<u32>,
    },
    /// A control block, part, or exit.
    Form {
        /// Its form.
        form: Form,
        /// Its ordinal among siblings of that form; absent for the parts that appear once.
        ordinal: Option<u32>,
    },
    /// A call.
    Call {
        /// The normalized callee, unescaped.
        callee: Box<str>,
        /// Its first argument's value, when the key carries it.
        literal: Option<Box<str>>,
        /// The ordinal of a repeated step (n ≥ 1), absent for the first.
        repeat: Option<u32>,
    },
    /// An unnamed function.
    Inline {
        /// Where it is written.
        inline: Inline,
        /// Its ordinal among siblings written the same way.
        ordinal: u32,
    },
}

impl fmt::Display for Segment {
    /// Writes the segment without its leading separator.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::new();
        match self {
            Self::Name { name, repeat } => {
                escape(name, &mut text);
                write_ordinal(&mut text, *repeat);
            }
            Self::Form { form, ordinal } => {
                text.push_str(form.as_str());
                write_ordinal(&mut text, *ordinal);
            }
            Self::Call {
                callee,
                literal,
                repeat,
            } => {
                write_call(&mut text, callee, literal.as_deref());
                write_ordinal(&mut text, *repeat);
            }
            Self::Inline { inline, ordinal } => {
                text.push_str(inline.as_str());
                write_ordinal(&mut text, Some(*ordinal));
            }
        }
        f.write_str(&text)
    }
}

/// Appends `call:`, the escaped callee, and the quoted literal, if any.
fn write_call(out: &mut String, callee: &str, literal: Option<&str>) {
    out.push_str(CALL_PREFIX);
    escape(callee, out);
    if let Some(literal) = literal {
        out.push_str("(\"");
        for character in literal.chars() {
            if matches!(character, '"' | '\\') {
                out.push('\\');
            }
            out.push(character);
        }
        out.push_str("\")");
    }
}

fn write_ordinal(out: &mut String, ordinal: Option<u32>) {
    if let Some(ordinal) = ordinal {
        // Writing to a String cannot fail.
        let _ = write!(out, "[{ordinal}]");
    }
}

/// Appends `text` with the reserved characters (`%`, `/`, `#`, `[`, `]`) percent-encoded in
/// uppercase hex.
fn escape(text: &str, out: &mut String) {
    for character in text.chars() {
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

/// Reverses [`escape`]; `None` for a reserved character written raw or an unknown escape.
fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(['%', '/', '#', '[', ']']) {
        out.push_str(&rest[..index]);
        let escaped = rest[index..].get(..3)?;
        out.push(match escaped {
            "%25" => '%',
            "%2F" => '/',
            "%23" => '#',
            "%5B" => '[',
            "%5D" => ']',
            _ => return None,
        });
        rest = &rest[index + 3..];
    }
    out.push_str(rest);
    Some(out)
}

/// A key's structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedKey {
    /// A block's key.
    Block(BlockKey),
    /// An app's key: `entrance:<app>`.
    App {
        /// The app's slug.
        app: Box<str>,
    },
    /// An entrance's key: `entrance:<app>/<entrance>`.
    Entrance {
        /// The app's slug.
        app: Box<str>,
        /// The entrance's slug.
        entrance: Box<str>,
    },
}

impl ParsedKey {
    fn parse(text: &str) -> Result<Self, KeyParseError> {
        let error = |problem| KeyParseError {
            key: text.into(),
            problem,
        };
        if let Some(rest) = text
            .strip_prefix(ENTRANCE_PREFIX)
            .filter(|rest| !rest.contains('#'))
        {
            let is_slug = |part: &str| slug(part).is_ok_and(|slug| slug == part);
            return match rest.split_once('/') {
                None if is_slug(rest) => Ok(Self::App { app: rest.into() }),
                Some((app, entrance)) if is_slug(app) && is_slug(entrance) => Ok(Self::Entrance {
                    app: app.into(),
                    entrance: entrance.into(),
                }),
                _ => Err(error("an app or entrance name is not a slug")),
            };
        }
        // A path may itself contain `#`, so the path ends at the first `#` after which the
        // rest reads as a key.
        let mut problem = "it has no `#` after a file path";
        for (index, _) in text.match_indices('#') {
            let Ok(path) = RepoPath::new(&text[..index]) else {
                continue;
            };
            match Steps::new(&text[index + 1..]).segments() {
                Ok(segments) => return Ok(Self::Block(BlockKey { path, segments })),
                Err(first) => problem = first,
            }
        }
        Err(error(problem))
    }
}

/// A block key's file and the segments after its `#`, in order: the names of a named
/// definition, then steps, with an inline function's names following it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockKey {
    path: RepoPath,
    segments: Vec<Segment>,
}

impl BlockKey {
    /// The block's file.
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// The segments after `#`; empty for a module.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The key as text, the same text it was read from.
    pub fn to_key(&self) -> Key {
        Key(Arc::from(self.to_string()))
    }
}

impl fmt::Display for BlockKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.path)?;
        for (index, segment) in self.segments.iter().enumerate() {
            match segment {
                Segment::Name { .. } if index == 0 => {}
                Segment::Name { .. } => f.write_str(".")?,
                _ => f.write_str("/")?,
            }
            write!(f, "{segment}")?;
        }
        Ok(())
    }
}

/// Key text that does not follow the key grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{key}` is not a key: {problem}")]
pub struct KeyParseError {
    /// The rejected text.
    pub key: Box<str>,
    /// What is wrong with it.
    pub problem: &'static str,
}

/// A reader over the text after a block key's `#`.
struct Steps<'a> {
    rest: &'a str,
}

impl<'a> Steps<'a> {
    fn new(text: &'a str) -> Self {
        Self { rest: text }
    }

    fn segments(mut self) -> Result<Vec<Segment>, &'static str> {
        let mut segments = Vec::new();
        if self.rest.is_empty() {
            return Ok(segments);
        }
        if !self.rest.starts_with('/') {
            self.names(&mut segments)?;
        }
        while let Some(rest) = self.rest.strip_prefix('/') {
            self.rest = rest;
            let step = self.step()?;
            let inline = matches!(step, Segment::Inline { .. });
            segments.push(step);
            if inline && self.rest.starts_with('.') {
                self.rest = &self.rest[1..];
                self.names(&mut segments)?;
            }
        }
        if !self.rest.is_empty() {
            return Err("a step or name ends with unexpected text");
        }
        if segments.is_empty() {
            return Err("`#/` is followed by no step");
        }
        Ok(segments)
    }

    /// Reads `name { "." name }`, each with an optional repeat ordinal.
    fn names(&mut self, segments: &mut Vec<Segment>) -> Result<(), &'static str> {
        loop {
            let end = self.rest.find(['.', '/', '[']).unwrap_or(self.rest.len());
            let (raw, rest) = self.rest.split_at(end);
            if raw.is_empty() {
                return Err("a name is empty");
            }
            let name = unescape(raw).ok_or("a name has an unescaped reserved character")?;
            self.rest = rest;
            let repeat = self.repeat()?;
            segments.push(Segment::Name {
                name: name.into(),
                repeat,
            });
            match self.rest.strip_prefix('.') {
                Some(rest) => self.rest = rest,
                None => return Ok(()),
            }
        }
    }

    fn step(&mut self) -> Result<Segment, &'static str> {
        if let Some(rest) = self.rest.strip_prefix(CALL_PREFIX) {
            self.rest = rest;
            return self.call();
        }
        let end = self.rest.find(['[', '/', '.']).unwrap_or(self.rest.len());
        let (word, rest) = self.rest.split_at(end);
        self.rest = rest;
        if let Ok(inline) = word.parse::<Inline>() {
            let ordinal = self.ordinal()?.ok_or("an inline function has no ordinal")?;
            return Ok(Segment::Inline { inline, ordinal });
        }
        let form = word.parse::<Form>().map_err(|_| "a step is not a known form")?;
        let numbering = Numbering::of(form).ok_or("a definition or call form is written as a step")?;
        let ordinal = self.ordinal()?;
        match (numbering, ordinal) {
            (Numbering::Ordinal, None) => Err("a control block, exit, or repeating part has no ordinal"),
            (Numbering::Once, Some(_)) => Err("a part that appears once has an ordinal"),
            _ => Ok(Segment::Form { form, ordinal }),
        }
    }

    fn call(&mut self) -> Result<Segment, &'static str> {
        let end = [self.rest.find('/'), self.rest.find('['), self.rest.find("(\"")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(self.rest.len());
        let (raw, rest) = self.rest.split_at(end);
        if raw.is_empty() {
            return Err("a callee is empty");
        }
        let callee = unescape(raw).ok_or("a callee has an unescaped reserved character")?;
        self.rest = rest;
        let literal = match self.rest.strip_prefix("(\"") {
            Some(rest) => {
                self.rest = rest;
                Some(self.literal()?)
            }
            None => None,
        };
        let repeat = self.repeat()?;
        Ok(Segment::Call {
            callee: callee.into(),
            literal: literal.map(Into::into),
            repeat,
        })
    }

    /// Reads a literal's characters after `("`, through its closing `")`.
    fn literal(&mut self) -> Result<String, &'static str> {
        let mut literal = String::new();
        let mut characters = self.rest.char_indices();
        while let Some((index, character)) = characters.next() {
            match character {
                '\\' => match characters.next() {
                    Some((_, escaped @ ('"' | '\\'))) => literal.push(escaped),
                    _ => return Err("a literal has an escape other than `\\\"` or `\\\\`"),
                },
                '"' => {
                    let rest = &self.rest[index + 1..];
                    self.rest = rest
                        .strip_prefix(')')
                        .ok_or("a literal's quote is not followed by `)`")?;
                    return Ok(literal);
                }
                _ => literal.push(character),
            }
        }
        Err("a literal has no closing quote")
    }

    /// Reads an optional repeat ordinal, which is never `[0]`.
    fn repeat(&mut self) -> Result<Option<u32>, &'static str> {
        match self.ordinal()? {
            Some(0) => Err("a repeat ordinal is 0"),
            repeat => Ok(repeat),
        }
    }

    /// Reads an optional `[n]`: decimal digits without leading zeros.
    fn ordinal(&mut self) -> Result<Option<u32>, &'static str> {
        let Some(rest) = self.rest.strip_prefix('[') else {
            return Ok(None);
        };
        let (digits, rest) = rest.split_once(']').ok_or("an ordinal has no `]`")?;
        let canonical = !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && (digits == "0" || !digits.starts_with('0'));
        let ordinal = digits
            .parse()
            .ok()
            .filter(|_| canonical)
            .ok_or("an ordinal is not a plain number")?;
        self.rest = rest;
        Ok(Some(ordinal))
    }
}

#[cfg(test)]
mod tests;
