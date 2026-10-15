//! The graph's closed vocabularies and the per-kind rules other crates share.
//!
//! Every vocabulary is a fieldless enum whose serialized form is the exact string the graph
//! model uses (`type_alias`, `unknown_receiver`, `not_a_start`, ...). In self-describing formats
//! (JSON, the canonical export) values serialize as those strings; in compact formats (postcard,
//! the facts cache) they serialize as their variant index.
//!
//! docs/graph-model.md describes every node and link value.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A string that is not part of a vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {vocabulary} `{value}`")]
pub struct UnknownValue {
    /// The vocabulary's name, such as `node form`.
    pub vocabulary: &'static str,
    /// The rejected input.
    pub value: Box<str>,
}

macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident, $label:literal {
            $( $(#[$vmeta:meta])* $variant:ident = $text:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        pub enum $name {
            $( $(#[$vmeta])* #[serde(rename = $text)] $variant, )+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The value's vocabulary string.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text,)+
                }
            }
        }

        impl FromStr for $name {
            type Err = UnknownValue;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok($name::$variant),)+
                    _ => Err(UnknownValue { vocabulary: $label, value: value.into() }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

vocabulary! {
    /// A source language Unfold indexes.
    Language, "language" {
        TypeScript = "typescript",
        Python = "python",
    }
}

vocabulary! {
    /// Whether a file's facts come from the bytes on disk.
    FileState, "file state" {
        /// The facts come from the bytes on disk.
        Current = "current",
        /// The file no longer parses; the facts come from the last version that did.
        LastGood = "last_good",
        /// No version of the file has parsed; it has no nodes.
        Failed = "failed",
    }
}

vocabulary! {
    /// What a fingerprinted input is: a configuration file that shaped the graph, or an
    /// evidence file boards cite.
    InputRole, "input role" {
        UnfoldConfig = "unfold-config",
        Tsconfig = "tsconfig",
        Package = "package",
        Pyproject = "pyproject",
        /// A file `[evidence] files` lists: hashed, never parsed, and never read by linking.
        Evidence = "evidence",
    }
}

vocabulary! {
    /// What a block is: one of the five kinds of block, with definitions split by what they
    /// define. A node's kind follows from its [`Form`].
    NodeKind, "node kind" {
        /// A file's own code.
        Module = "module",
        Class = "class",
        /// Any function: declared, a method, an accessor, or written inline.
        Function = "function",
        /// An interface, a type alias, or an enum. It has no body.
        Type = "type",
        /// Code that chooses which of its parts run.
        Decision = "decision",
        /// Code that repeats its body.
        Loop = "loop",
        /// Code that protects its body.
        Try = "try",
        /// Code that runs its body inside a context.
        With = "with",
        /// One lane of a control block.
        Part = "part",
        /// A place where this code runs other code.
        Call = "call",
        /// A place where a path leaves.
        Exit = "exit",
    }
}

vocabulary! {
    /// A block's written form. Every form belongs to exactly one [`NodeKind`]
    /// ([`Form::kind`]), so a node stores only its form and an impossible kind and form pair
    /// can't be written down.
    Form, "node form" {
        Module = "module",
        Class = "class",
        Function = "function",
        Method = "method",
        Constructor = "constructor",
        /// Python's `@property`, a TypeScript `get` accessor.
        Getter = "getter",
        /// Python's `@x.setter`, a TypeScript `set` accessor.
        Setter = "setter",
        /// A Python `lambda`, a TypeScript arrow function or function expression.
        Lambda = "lambda",
        Interface = "interface",
        TypeAlias = "type_alias",
        Enum = "enum",
        If = "if",
        Match = "match",
        Switch = "switch",
        /// A conditional expression that contains a call.
        Ternary = "ternary",
        For = "for",
        While = "while",
        Do = "do",
        /// A comprehension or generator expression that contains a call.
        Comprehension = "comprehension",
        Try = "try",
        With = "with",
        /// TypeScript's `using` and `await using`.
        Using = "using",
        Then = "then",
        Elif = "elif",
        Else = "else",
        Case = "case",
        /// A `switch`'s `default`, or Python's guardless `case _:`.
        Default = "default",
        Body = "body",
        Except = "except",
        Catch = "catch",
        Finally = "finally",
        Call = "call",
        /// A construction: `new X()`, or a Python call whose class identity is proven.
        New = "new",
        Return = "return",
        Raise = "raise",
        Throw = "throw",
        Break = "break",
        Continue = "continue",
        Yield = "yield",
    }
}

vocabulary! {
    /// A yes-or-no fact about a node ([`Flag::applies_to`] says which nodes may carry it).
    Flag, "node flag" {
        /// An `async` function, or an `async with` or `await using`.
        Async = "async",
        /// A function that yields.
        Generator = "generator",
        /// A call written under `await`.
        Awaited = "awaited",
        /// A call that runs only sometimes inside its expression: the right side of `and`,
        /// `or`, or `??`, or after `?.`.
        Conditional = "conditional",
        /// An expression block whose value is exactly what its exit returns, raises, or yields.
        ExitValue = "exit_value",
        /// A `switch` case that runs into the next one.
        FallsThrough = "falls_through",
        /// A Python `except*` handler.
        Group = "group",
        /// A block the front end could not place: nesting past the depth limit, or a TypeScript
        /// labeled plain block.
        Truncated = "truncated",
    }
}

vocabulary! {
    /// Why a call sits in the body it does: it is written in a part's test, or it runs when a
    /// definition is created (or, for a TypeScript default, when its function is called).
    CallRole, "call role" {
        /// Written in a part's test.
        Test = "test",
        /// A decorator of a definition.
        Decorator = "decorator",
        /// A parameter's default value.
        Default = "default",
        /// A class base written as a call.
        Base = "base",
    }
}

vocabulary! {
    /// Where an inline definition (an unnamed function, or a TypeScript class expression) is
    /// written, which decides its key step.
    Inline, "inline definition" {
        /// In a call's arguments, object properties included: `callback[n]`.
        Callback = "callback",
        /// Anywhere else, including called where it is written: `lambda[n]`.
        Lambda = "lambda",
    }
}

vocabulary! {
    /// A relationship between a block and what it names.
    LinkKind, "link kind" {
        /// A call runs a definition, a library, a builtin, or something unknown.
        Calls = "calls",
        /// A call passes a definition as a value.
        Passes = "passes",
        /// A class or type extends (or, flagged, implements) a class or type.
        Extends = "extends",
        /// A module, class, function, or dynamic-import call imports a module: an import statement
        /// links from the definition it is written in.
        Imports = "imports",
        /// A definition or an `except` part names a class or type in an annotation or clause.
        Types = "types",
    }
}

vocabulary! {
    /// How a link resolved.
    Outcome, "outcome" {
        /// To one proven definition in this project.
        Project = "project",
        /// To a proven outside module, such as `fastapi` or `python:logging`.
        Library = "library",
        /// To the language's own builtins: Python's `builtins`, ECMAScript's standard objects.
        Builtin = "builtin",
        /// To nothing proven, for a reason.
        Unknown = "unknown",
    }
}

vocabulary! {
    /// Why a link's target is unknown.
    Reason, "reason" {
        UnknownName = "unknown_name",
        UnknownReceiver = "unknown_receiver",
        Parameter = "parameter",
        MutableBinding = "mutable_binding",
        NotConcrete = "not_concrete",
        Accessor = "accessor",
        Ambiguous = "ambiguous",
        Overridden = "overridden",
        UnboundThis = "unbound_this",
        NotIndexed = "not_indexed",
        NotFound = "not_found",
        Dynamic = "dynamic",
        Wrapper = "wrapper",
        Unsupported = "unsupported",
    }
}

vocabulary! {
    /// What an app's or entrance's `code` key names after a compile.
    TargetState, "target state" {
        /// A module, a definition, or an inline function.
        Found = "found",
        /// No block has the key.
        Missing = "missing",
        /// Scope or ignore rules exclude the key's file.
        NotAnalyzed = "not_analyzed",
        /// A call, an exit, a control block, or a part.
        NotAStart = "not_a_start",
        /// The key's file stopped parsing; the block is its last good one.
        Stale = "stale",
    }
}

vocabulary! {
    /// What a callable's parameter is to its calls (`docs/graph-model.md`, "Typed records").
    ParameterKind, "parameter kind" {
        /// An ordinary parameter: it takes a positional argument (Python also a keyword).
        Positional = "positional",
        /// TypeScript's explicit `this` parameter, which takes no argument.
        This = "this",
        /// TypeScript `...x`, Python `*args`: every further positional argument.
        Rest = "rest",
        /// Python, before `/`: positional arguments only.
        PositionalOnly = "positional_only",
        /// Python, after `*` or `*args`: keyword arguments only.
        KeywordOnly = "keyword_only",
        /// Python `**kwargs`: every further keyword argument.
        Keywords = "keywords",
    }
}

vocabulary! {
    /// What a member of a type declaration or class is.
    MemberKind, "member kind" {
        /// A property or field.
        Property = "property",
        /// A method signature.
        Method = "method",
        /// An index signature.
        Index = "index",
        /// A call signature.
        Call = "call",
        /// A construct signature.
        Construct = "construct",
        /// An enum member.
        Value = "value",
    }
}

vocabulary! {
    /// How an argument is expanded.
    Expansion, "expansion" {
        /// TypeScript `...x`, Python `*x`.
        Spread = "spread",
        /// Python `**x`.
        Keywords = "keywords",
    }
}

vocabulary! {
    /// Why an argument fills no proven parameter.
    PairingReason, "pairing reason" {
        /// This argument, or an earlier one, is expanded.
        Spread = "spread",
        /// No parameter of the target takes it.
        NoParameter = "no_parameter",
        /// It can't be proven whether the call binds the target's first parameter itself.
        Receiver = "receiver",
        /// The target declares no parameters of its own: a class without a direct constructor
        /// that has a base.
        NoSignature = "no_signature",
    }
}

impl NodeKind {
    /// `module`, `class`, `function`, or `type`: something that can be a card, and the kinds
    /// that own the blocks under them ([`crate::model::owner`]).
    pub const fn is_definition(self) -> bool {
        matches!(self, Self::Module | Self::Class | Self::Function | Self::Type)
    }

    /// `decision`, `loop`, `try`, or `with`: a block whose lanes are parts.
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Decision | Self::Loop | Self::Try | Self::With)
    }
}

impl Form {
    /// The kind every block of this form is.
    pub const fn kind(self) -> NodeKind {
        use NodeKind as K;
        match self {
            Self::Module => K::Module,
            Self::Class => K::Class,
            Self::Function
            | Self::Method
            | Self::Constructor
            | Self::Getter
            | Self::Setter
            | Self::Lambda => K::Function,
            Self::Interface | Self::TypeAlias | Self::Enum => K::Type,
            Self::If | Self::Match | Self::Switch | Self::Ternary => K::Decision,
            Self::For | Self::While | Self::Do | Self::Comprehension => K::Loop,
            Self::Try => K::Try,
            Self::With | Self::Using => K::With,
            Self::Then
            | Self::Elif
            | Self::Else
            | Self::Case
            | Self::Default
            | Self::Body
            | Self::Except
            | Self::Catch
            | Self::Finally => K::Part,
            Self::Call | Self::New => K::Call,
            Self::Return | Self::Raise | Self::Throw | Self::Break | Self::Continue | Self::Yield => K::Exit,
        }
    }

    /// Whether files of `language` can contain this form, following each language's lowering.
    /// Python has no `switch`, `do`, `using`, `catch`, `throw`, or type declarations;
    /// TypeScript has no `match`, comprehensions, `with`, `except`, or `raise`.
    pub const fn allowed_in(self, language: Language) -> bool {
        match language {
            Language::Python => !matches!(
                self,
                Self::Switch
                    | Self::Do
                    | Self::Using
                    | Self::Catch
                    | Self::Throw
                    | Self::Interface
                    | Self::TypeAlias
                    | Self::Enum
            ),
            Language::TypeScript => !matches!(
                self,
                Self::Match | Self::Comprehension | Self::With | Self::Except | Self::Raise
            ),
        }
    }
}

impl Flag {
    /// Whether a node of `form` may carry this flag: `async` on functions and `with` blocks,
    /// `generator` on functions, `awaited` and `conditional` on calls, `exit_value` on calls,
    /// ternaries, and comprehensions, `falls_through` on a `switch`'s `case` and `default`
    /// parts, `group` on `except` parts, and `truncated` on any node.
    pub const fn applies_to(self, form: Form) -> bool {
        match self {
            Self::Async => matches!(form.kind(), NodeKind::Function | NodeKind::With),
            Self::Generator => matches!(form.kind(), NodeKind::Function),
            Self::Awaited | Self::Conditional => matches!(form.kind(), NodeKind::Call),
            Self::ExitValue => {
                matches!(form.kind(), NodeKind::Call) || matches!(form, Form::Ternary | Form::Comprehension)
            }
            Self::FallsThrough => matches!(form, Form::Case | Form::Default),
            Self::Group => matches!(form, Form::Except),
            Self::Truncated => true,
        }
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of [`Flag`]s.
///
/// In self-describing formats it serializes as an array of flag strings in declaration order
/// (`["async", "generator"]`); in compact formats, as one integer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Flags(u16);

impl Flags {
    /// No flags.
    pub const NONE: Self = Self(0);

    /// The set with exactly `flags`.
    pub fn of(flags: &[Flag]) -> Self {
        flags.iter().fold(Self::NONE, |set, &flag| set.with(flag))
    }

    /// This set with `flag` added.
    #[must_use]
    pub const fn with(self, flag: Flag) -> Self {
        Self(self.0 | flag.bit())
    }

    /// Adds `flag`.
    pub fn insert(&mut self, flag: Flag) {
        self.0 |= flag.bit();
    }

    /// Whether `flag` is in the set.
    pub const fn contains(self, flag: Flag) -> bool {
        self.0 & flag.bit() != 0
    }

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The flags in the set, in declaration order.
    pub fn iter(self) -> impl Iterator<Item = Flag> {
        Flag::ALL.iter().copied().filter(move |&flag| self.contains(flag))
    }
}

impl fmt::Debug for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl FromIterator<Flag> for Flags {
    fn from_iter<I: IntoIterator<Item = Flag>>(flags: I) -> Self {
        flags.into_iter().fold(Self::NONE, Self::with)
    }
}

impl Serialize for Flags {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !serializer.is_human_readable() {
            return serializer.serialize_u16(self.0);
        }
        let mut seq = serializer.serialize_seq(None)?;
        for flag in self.iter() {
            seq.serialize_element(&flag)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Flags {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FlagsVisitor;

        impl<'de> Visitor<'de> for FlagsVisitor {
            type Value = Flags;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an array of node flags")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Flags, A::Error> {
                let mut flags = Flags::NONE;
                while let Some(flag) = seq.next_element::<Flag>()? {
                    flags.insert(flag);
                }
                Ok(flags)
            }
        }

        if !deserializer.is_human_readable() {
            let bits = u16::deserialize(deserializer)?;
            let all = Flags::of(Flag::ALL).0;
            return if bits & !all == 0 {
                Ok(Flags(bits))
            } else {
                Err(de::Error::custom(format_args!(
                    "unknown node flag bits {bits:#x}"
                )))
            };
        }
        deserializer.deserialize_seq(FlagsVisitor)
    }
}

impl ParameterKind {
    /// Whether files of `language` can have this kind: `this` is TypeScript only;
    /// `positional_only`, `keyword_only`, and `keywords` are Python only.
    pub const fn allowed_in(self, language: Language) -> bool {
        match self {
            Self::Positional | Self::Rest => true,
            Self::This => matches!(language, Language::TypeScript),
            Self::PositionalOnly | Self::KeywordOnly | Self::Keywords => matches!(language, Language::Python),
        }
    }

    /// Whether a parameter of this kind can be filled by an argument (every kind but `this`).
    pub const fn takes_argument(self) -> bool {
        !matches!(self, Self::This)
    }

    /// Whether a positional argument can fill it one-to-one: `positional` or `positional_only`.
    pub const fn takes_position(self) -> bool {
        matches!(self, Self::Positional | Self::PositionalOnly)
    }

    /// Whether a keyword argument of its name can fill it: `positional` or `keyword_only`.
    pub const fn takes_keyword(self) -> bool {
        matches!(self, Self::Positional | Self::KeywordOnly)
    }

    /// The kind's place in Python's written order (positional-only, positional, `*args`,
    /// keyword-only, `**kwargs`); `None` for `this`.
    pub const fn python_order(self) -> Option<u8> {
        match self {
            Self::PositionalOnly => Some(0),
            Self::Positional => Some(1),
            Self::Rest => Some(2),
            Self::KeywordOnly => Some(3),
            Self::Keywords => Some(4),
            Self::This => None,
        }
    }
}

impl Expansion {
    /// Whether files of `language` can have it: `keywords` is Python only.
    pub const fn allowed_in(self, language: Language) -> bool {
        match self {
            Self::Spread => true,
            Self::Keywords => matches!(language, Language::Python),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(all: &[T])
    where
        T: Copy + fmt::Display + FromStr<Err = UnknownValue> + PartialEq + fmt::Debug + Serialize,
        T: for<'de> Deserialize<'de>,
    {
        let mut seen = std::collections::BTreeSet::new();
        for &value in all {
            let text = value.to_string();
            assert!(seen.insert(text.clone()), "duplicate vocabulary string {text}");
            assert_eq!(text.parse::<T>(), Ok(value));
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value);
            let packed = postcard::to_allocvec(&value).unwrap();
            assert_eq!(postcard::from_bytes::<T>(&packed).unwrap(), value);
        }
    }

    #[test]
    fn vocabularies_round_trip_through_strings_json_and_postcard() {
        round_trip(Language::ALL);
        round_trip(FileState::ALL);
        round_trip(InputRole::ALL);
        round_trip(NodeKind::ALL);
        round_trip(Form::ALL);
        round_trip(Flag::ALL);
        round_trip(CallRole::ALL);
        round_trip(Inline::ALL);
        round_trip(LinkKind::ALL);
        round_trip(Outcome::ALL);
        round_trip(Reason::ALL);
        round_trip(TargetState::ALL);
        round_trip(ParameterKind::ALL);
        round_trip(MemberKind::ALL);
        round_trip(Expansion::ALL);
        round_trip(PairingReason::ALL);
    }

    #[test]
    fn vocabulary_sizes_match_the_graph_model() {
        assert_eq!(NodeKind::ALL.len(), 11);
        assert_eq!(Form::ALL.len(), 39);
        assert_eq!(Flag::ALL.len(), 8);
        assert_eq!(CallRole::ALL.len(), 4);
        assert_eq!(LinkKind::ALL.len(), 5);
        assert_eq!(Outcome::ALL.len(), 4);
        assert_eq!(Reason::ALL.len(), 14);
        assert_eq!(TargetState::ALL.len(), 5);
    }

    /// Compact formats (postcard, the facts cache) encode a value as its variant index, so
    /// reordering, inserting, or removing a variant silently changes what cached facts mean.
    #[test]
    fn compact_encodings_are_pinned() {
        fn names<T: Copy + fmt::Display>(all: &[T]) -> String {
            all.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
        }
        let encoding = [
            names(Language::ALL),
            names(FileState::ALL),
            names(InputRole::ALL),
            names(NodeKind::ALL),
            names(Form::ALL),
            names(Flag::ALL),
            names(CallRole::ALL),
            names(Inline::ALL),
            names(LinkKind::ALL),
            names(Outcome::ALL),
            names(Reason::ALL),
            names(TargetState::ALL),
            names(ParameterKind::ALL),
            names(MemberKind::ALL),
            names(Expansion::ALL),
            names(PairingReason::ALL),
        ]
        .join("\n");
        assert_eq!(
            crate::text::ContentHash::of(encoding.as_bytes()).to_string(),
            "fb9aa03fde7b647f223042852a09811ce52377959465c5fe63e9e11db1f9a86f",
            "a vocabulary's variant order changed: bump unfold-engine's ENGINE_VERSION so cached \
             facts are recomputed, then update this hash"
        );
    }

    #[test]
    fn unknown_values_are_rejected_with_their_vocabulary() {
        let error = "statement".parse::<Form>().unwrap_err();
        assert_eq!(error.to_string(), "unknown node form `statement`");
        assert!("Calls".parse::<LinkKind>().is_err());
        assert!(serde_json::from_str::<Reason>("\"\"").is_err());
    }

    #[test]
    fn every_form_has_the_kind_of_the_design_table() {
        let forms = |kind: NodeKind| -> Vec<&str> {
            Form::ALL
                .iter()
                .filter(|form| form.kind() == kind)
                .map(|form| form.as_str())
                .collect()
        };
        use NodeKind as K;
        assert_eq!(forms(K::Module), ["module"]);
        assert_eq!(forms(K::Class), ["class"]);
        assert_eq!(
            forms(K::Function),
            ["function", "method", "constructor", "getter", "setter", "lambda"]
        );
        assert_eq!(forms(K::Type), ["interface", "type_alias", "enum"]);
        assert_eq!(forms(K::Decision), ["if", "match", "switch", "ternary"]);
        assert_eq!(forms(K::Loop), ["for", "while", "do", "comprehension"]);
        assert_eq!(forms(K::Try), ["try"]);
        assert_eq!(forms(K::With), ["with", "using"]);
        assert_eq!(
            forms(K::Part),
            [
                "then", "elif", "else", "case", "default", "body", "except", "catch", "finally"
            ]
        );
        assert_eq!(forms(K::Call), ["call", "new"]);
        assert_eq!(
            forms(K::Exit),
            ["return", "raise", "throw", "break", "continue", "yield"]
        );
        let definitions: Vec<NodeKind> = NodeKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.is_definition())
            .collect();
        assert_eq!(definitions, [K::Module, K::Class, K::Function, K::Type]);
        let controls: Vec<NodeKind> = NodeKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.is_control())
            .collect();
        assert_eq!(controls, [K::Decision, K::Loop, K::Try, K::With]);
    }

    #[test]
    fn forms_follow_the_language_mapping() {
        use Language::{Python, TypeScript};
        assert!(Form::Match.allowed_in(Python) && !Form::Match.allowed_in(TypeScript));
        assert!(Form::Switch.allowed_in(TypeScript) && !Form::Switch.allowed_in(Python));
        assert!(Form::Except.allowed_in(Python) && !Form::Except.allowed_in(TypeScript));
        assert!(Form::Catch.allowed_in(TypeScript) && !Form::Catch.allowed_in(Python));
        assert!(Form::Interface.allowed_in(TypeScript) && !Form::Interface.allowed_in(Python));
        assert!(Form::New.allowed_in(Python) && Form::New.allowed_in(TypeScript));
        assert!(Form::Default.allowed_in(Python) && Form::Default.allowed_in(TypeScript));
    }

    #[test]
    fn flags_apply_to_the_forms_the_design_names() {
        assert!(Flag::Async.applies_to(Form::Lambda));
        assert!(Flag::Async.applies_to(Form::Using));
        assert!(!Flag::Async.applies_to(Form::Call));
        assert!(Flag::Awaited.applies_to(Form::New));
        assert!(Flag::ExitValue.applies_to(Form::Ternary));
        assert!(!Flag::ExitValue.applies_to(Form::If));
        assert!(Flag::FallsThrough.applies_to(Form::Case));
        assert!(!Flag::FallsThrough.applies_to(Form::Then));
        assert!(Flag::Group.applies_to(Form::Except));
        assert!(Form::ALL.iter().all(|&form| Flag::Truncated.applies_to(form)));
    }

    #[test]
    fn flag_sets_serialize_as_names_or_bits() {
        let flags = Flags::of(&[Flag::Generator, Flag::Async]);
        assert!(flags.contains(Flag::Async) && !flags.contains(Flag::Awaited));
        assert_eq!(flags.iter().collect::<Vec<_>>(), [Flag::Async, Flag::Generator]);
        assert_eq!(serde_json::to_string(&flags).unwrap(), r#"["async","generator"]"#);
        assert_eq!(
            serde_json::from_str::<Flags>(r#"["generator","async"]"#).unwrap(),
            flags
        );
        assert_eq!(serde_json::to_string(&Flags::NONE).unwrap(), "[]");
        assert!(serde_json::from_str::<Flags>(r#"["fast"]"#).is_err());
        let packed = postcard::to_allocvec(&flags).unwrap();
        assert_eq!(postcard::from_bytes::<Flags>(&packed).unwrap(), flags);
        let unknown = postcard::to_allocvec(&0x8000_u16).unwrap();
        assert!(postcard::from_bytes::<Flags>(&unknown).is_err());
        assert_eq!(Flag::ALL.iter().copied().collect::<Flags>(), Flags::of(Flag::ALL));
        assert_eq!(format!("{flags:?}"), "{Async, Generator}");
    }
}
