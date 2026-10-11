//! The graph's closed vocabularies and the per-kind rules other crates share.
//!
//! Every vocabulary is a fieldless enum whose serialized form is the exact string the graph
//! model uses (`expression_statement`, `express.route`, `POST`, ...). In self-describing formats
//! (JSON, the canonical export) values serialize as those strings; in compact formats (postcard,
//! the facts cache) they serialize as their variant index.
//!
//! `docs/graph-model.md` is the authoritative description of every value.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A string that is not part of a vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {vocabulary} `{value}`")]
pub struct UnknownValue {
    /// The vocabulary's name, such as `block kind`.
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
        /// No version of the file has parsed; it has no blocks.
        Failed = "failed",
    }
}

vocabulary! {
    /// What a fingerprinted configuration input is.
    InputRole, "input role" {
        UnfoldConfig = "unfold-config",
        Tsconfig = "tsconfig",
        Package = "package",
        Pyproject = "pyproject",
    }
}

vocabulary! {
    /// A selected syntax entity.
    BlockKind, "block kind" {
        File = "file",
        Function = "function",
        Class = "class",
        Method = "method",
        Constructor = "constructor",
        Getter = "getter",
        Setter = "setter",
        Variable = "variable",
        Call = "call",
        New = "new",
        Import = "import",
        If = "if",
        For = "for",
        ForIn = "for_in",
        ForOf = "for_of",
        While = "while",
        DoWhile = "do_while",
        Switch = "switch",
        Case = "case",
        Default = "default",
        Try = "try",
        Catch = "catch",
        Return = "return",
        Throw = "throw",
        Break = "break",
        Continue = "continue",
        ExpressionStatement = "expression_statement",
        With = "with",
        Match = "match",
        /// A TypeScript interface.
        Interface = "interface",
        /// A TypeScript type alias.
        TypeAlias = "type_alias",
        /// A TypeScript enum.
        Enum = "enum",
    }
}

vocabulary! {
    /// A block's relationship to its saved parent. A root, or an unmapped structural
    /// relationship, has no role (`Option<Role>` is `None`).
    Role, "role" {
        Body = "body",
        Parameter = "parameter",
        Condition = "condition",
        Then = "then",
        Else = "else",
        Initializer = "initializer",
        Update = "update",
        Iterable = "iterable",
        Catch = "catch",
        Finally = "finally",
        Expression = "expression",
        Binding = "binding",
        Callee = "callee",
        Argument = "argument",
        Heritage = "heritage",
        Decorator = "decorator",
        Header = "header",
        Name = "name",
    }
}

vocabulary! {
    /// A written part of a block.
    SlotName, "slot" {
        Name = "name",
        Parameter = "parameter",
        ReturnType = "return_type",
        Body = "body",
        Heritage = "heritage",
        Initializer = "initializer",
        Condition = "condition",
        Update = "update",
        Iterable = "iterable",
        Then = "then",
        Else = "else",
        Expression = "expression",
        Callee = "callee",
        Argument = "argument",
        Catch = "catch",
        Finally = "finally",
        Binding = "binding",
        Specifier = "specifier",
        /// A TypeScript type alias's aliased type.
        Type = "type",
    }
}

vocabulary! {
    /// A relationship sourced at one occurrence.
    EdgeKind, "edge kind" {
        Calls = "calls",
        Constructs = "constructs",
        Imports = "imports",
        References = "references",
    }
}

vocabulary! {
    /// How an edge or entrance target was resolved.
    Resolution, "resolution" {
        /// One proven target in the graph.
        Resolved = "resolved",
        /// A proven origin outside the graph, with a module label.
        External = "external",
        /// No reliable target, with a reason.
        Unresolved = "unresolved",
    }
}

vocabulary! {
    /// Why an occurrence or entrance target is unresolved.
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
    /// A declared way that a process or unit of work starts.
    EntranceKind, "entrance kind" {
        Process = "process",
        Command = "command",
        Http = "http",
        Worker = "worker",
        Schedule = "schedule",
        Event = "event",
    }
}

vocabulary! {
    /// How much of an entrance's trigger is known.
    TriggerStatus, "trigger status" {
        Exact = "exact",
        Partial = "partial",
        /// Exactly when the trigger is absent.
        Unknown = "unknown",
    }
}

vocabulary! {
    /// What an entrance target is to its entrance.
    TargetRole, "target role" {
        /// The module initialization (or console callable) a process starts with.
        Startup = "startup",
        /// Declared work for every other entrance kind.
        Handler = "handler",
    }
}

vocabulary! {
    /// The method of an HTTP entrance.
    HttpMethod, "HTTP method" {
        Get = "GET",
        Head = "HEAD",
        Post = "POST",
        Put = "PUT",
        Patch = "PATCH",
        Delete = "DELETE",
        Options = "OPTIONS",
        All = "ALL",
    }
}

vocabulary! {
    /// An entrance detector.
    DetectorId, "detector" {
        Manual = "manual",
        PackageScript = "package.script",
        PackageBin = "package.bin",
        ExpressRoute = "express.route",
        BullmqWorker = "bullmq.worker",
        NodeCronSchedule = "node-cron.schedule",
        PyprojectScript = "pyproject.script",
        FastapiRoute = "fastapi.route",
        CeleryTask = "celery.task",
        ApschedulerJob = "apscheduler.job",
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

vocabulary! {
    /// Where a call's value lands.
    ResultBinding, "result binding" {
        /// It is exactly the initializer of a `variable` block.
        Variable = "variable",
        /// It is exactly a returned expression (Python: also a lambda body; TypeScript: also an
        /// arrow function's expression body).
        Return = "return",
        /// It is exactly an argument of another call or construction.
        Argument = "argument",
        /// It is exactly the expression of an `expression_statement`.
        Discard = "discard",
        /// Anything else.
        Other = "other",
    }
}

vocabulary! {
    /// How a type name written in a heritage clause relates its declaration to the type it
    /// names.
    Heritage, "heritage" {
        /// A base the declaration extends: TypeScript `extends` (an interface's or a class's), a
        /// Python class base.
        Extends = "extends",
        /// An interface a TypeScript class implements.
        Implements = "implements",
    }
}

impl BlockKind {
    /// `function`, `method`, `constructor`, `getter`, or `setter`: a block with a callable body.
    pub const fn is_function_like(self) -> bool {
        matches!(
            self,
            Self::Function | Self::Method | Self::Constructor | Self::Getter | Self::Setter
        )
    }

    /// `call`, `new`, or `import`: a block that is the source of exactly one occurrence edge.
    pub const fn is_occurrence(self) -> bool {
        matches!(self, Self::Call | Self::New | Self::Import)
    }

    /// `class`, `interface`, `type_alias`, or `enum`: a declaration a type name can resolve to,
    /// and the kinds that carry `member` records.
    pub const fn is_type_declaration(self) -> bool {
        matches!(self, Self::Class | Self::Interface | Self::TypeAlias | Self::Enum)
    }

    /// Whether files of `language` can contain this kind. Python never emits `new`, `for`,
    /// `for_in`, `do_while`, `switch`, `default`, `interface`, `type_alias`, or `enum`; `with`
    /// and `match` are Python only.
    pub const fn allowed_in(self, language: Language) -> bool {
        match language {
            Language::TypeScript => !matches!(self, Self::With | Self::Match),
            Language::Python => !matches!(
                self,
                Self::New
                    | Self::For
                    | Self::ForIn
                    | Self::DoWhile
                    | Self::Switch
                    | Self::Default
                    | Self::Interface
                    | Self::TypeAlias
                    | Self::Enum
            ),
        }
    }

    /// The slots a block of this kind may have in `language`, in canonical order.
    pub const fn slots(self, language: Language) -> &'static [SlotName] {
        use SlotName as S;
        let python = matches!(language, Language::Python);
        match self {
            Self::Function | Self::Method | Self::Constructor | Self::Getter | Self::Setter => {
                &[S::Name, S::Parameter, S::ReturnType, S::Body]
            }
            Self::Class => &[S::Name, S::Heritage],
            Self::Interface => &[S::Name, S::Heritage, S::Body],
            Self::TypeAlias => &[S::Name, S::Type],
            Self::Enum => &[S::Name, S::Body],
            Self::Variable => &[S::Name, S::Initializer],
            Self::Call | Self::New => &[S::Callee, S::Argument],
            Self::If => &[S::Condition, S::Then, S::Else],
            Self::For => &[S::Initializer, S::Condition, S::Update, S::Body],
            Self::ForOf if python => &[S::Initializer, S::Iterable, S::Body, S::Else],
            Self::ForIn | Self::ForOf => &[S::Initializer, S::Iterable, S::Body],
            Self::While if python => &[S::Condition, S::Body, S::Else],
            Self::While | Self::DoWhile => &[S::Condition, S::Body],
            Self::Switch | Self::With | Self::Match => &[S::Expression, S::Body],
            Self::Case => &[S::Condition, S::Body],
            Self::Default => &[S::Body],
            Self::Try if python => &[S::Body, S::Catch, S::Else, S::Finally],
            Self::Try => &[S::Body, S::Catch, S::Finally],
            Self::Catch => &[S::Binding, S::Body],
            Self::Return | Self::Throw | Self::ExpressionStatement => &[S::Expression],
            Self::Import => &[S::Specifier],
            Self::File | Self::Break | Self::Continue => &[],
        }
    }

    /// Whether `slot` may appear on this kind in `language` (see [`BlockKind::slots`]).
    pub fn has_slot(self, slot: SlotName, language: Language) -> bool {
        self.allowed_in(language) && self.slots(language).contains(&slot)
    }

    /// Whether `slot` repeats on this kind and so carries a contiguous zero-based index:
    /// every `parameter` and `argument`, and Python `try` handlers (`catch`).
    pub const fn slot_is_indexed(self, slot: SlotName, language: Language) -> bool {
        match slot {
            SlotName::Parameter | SlotName::Argument => true,
            SlotName::Catch => matches!(self, Self::Try) && matches!(language, Language::Python),
            _ => false,
        }
    }

    /// The occurrence edge kinds a block of this kind may source in `language`: `calls` for a
    /// call (or `constructs` for a Python call with proven class identity), `constructs` for
    /// `new`, `imports` for `import`. Empty for every other kind.
    pub const fn occurrence_edge_kinds(self, language: Language) -> &'static [EdgeKind] {
        match (self, language) {
            (Self::Call, Language::Python) => &[EdgeKind::Calls, EdgeKind::Constructs],
            (Self::Call, Language::TypeScript) => &[EdgeKind::Calls],
            (Self::New, _) => &[EdgeKind::Constructs],
            (Self::Import, _) => &[EdgeKind::Imports],
            _ => &[],
        }
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

impl Role {
    /// Whether a block with this role under a parent of `parent` kind is allowed in `language`.
    /// `header` is Python only, and `else` under a loop or `try` is Python only.
    pub const fn allowed_in(self, parent: BlockKind, language: Language) -> bool {
        match (self, language) {
            (_, Language::Python) => true,
            (Self::Header, Language::TypeScript) => false,
            (Self::Else, Language::TypeScript) => !matches!(
                parent,
                BlockKind::For
                    | BlockKind::ForIn
                    | BlockKind::ForOf
                    | BlockKind::While
                    | BlockKind::DoWhile
                    | BlockKind::Try
            ),
            _ => true,
        }
    }
}

impl EdgeKind {
    /// The block kinds a resolved edge of this kind may target. For `constructs`, a `class`
    /// target is permitted only when it declares no direct constructor; the validator checks
    /// that separately.
    pub const fn target_kinds(self) -> &'static [BlockKind] {
        use BlockKind as K;
        match self {
            Self::Calls => FUNCTION_LIKE_AND_CLASS,
            Self::Constructs => &[K::Function, K::Method, K::Constructor, K::Class],
            Self::Imports => &[K::File],
            Self::References => &[K::Function, K::Method, K::Class],
        }
    }

    /// Whether this is an occurrence edge (every kind except `references`).
    pub const fn is_occurrence(self) -> bool {
        !matches!(self, Self::References)
    }
}

const FUNCTION_LIKE_AND_CLASS: &[BlockKind] = &[
    BlockKind::Function,
    BlockKind::Method,
    BlockKind::Constructor,
    BlockKind::Getter,
    BlockKind::Setter,
    BlockKind::Class,
];

impl EntranceKind {
    /// The role every target of this entrance kind has.
    pub const fn target_role(self) -> TargetRole {
        match self {
            Self::Process => TargetRole::Startup,
            _ => TargetRole::Handler,
        }
    }
}

impl TargetRole {
    /// The block kinds a resolved target with this role may name, given its entrance's detector.
    /// A startup is a file root, except a `pyproject.script` console callable; a manual handler
    /// may also be a branch (`if`, `case`, `default`) for explicit CLI dispatch.
    pub const fn target_kinds(self, detector: DetectorId) -> &'static [BlockKind] {
        use BlockKind as K;
        match (self, detector) {
            (Self::Startup, DetectorId::PyprojectScript) => &[K::Function, K::Method],
            (Self::Startup, _) => &[K::File],
            (Self::Handler, DetectorId::Manual) => &[K::Function, K::Method, K::If, K::Case, K::Default],
            (Self::Handler, _) => &[K::Function, K::Method],
        }
    }
}

impl DetectorId {
    /// The entrance kinds this detector may produce. Manual declarations may produce any kind.
    pub const fn entrance_kinds(self) -> &'static [EntranceKind] {
        match self {
            Self::Manual => EntranceKind::ALL,
            Self::PackageScript | Self::PackageBin | Self::PyprojectScript => &[EntranceKind::Process],
            Self::ExpressRoute | Self::FastapiRoute => &[EntranceKind::Http],
            Self::BullmqWorker | Self::CeleryTask => &[EntranceKind::Worker],
            Self::NodeCronSchedule | Self::ApschedulerJob => &[EntranceKind::Schedule],
        }
    }

    /// The language whose code this detector reads, or `None` for manual declarations, which
    /// may target either language.
    pub const fn language(self) -> Option<Language> {
        match self {
            Self::Manual => None,
            Self::PackageScript
            | Self::PackageBin
            | Self::ExpressRoute
            | Self::BullmqWorker
            | Self::NodeCronSchedule => Some(Language::TypeScript),
            Self::PyprojectScript | Self::FastapiRoute | Self::CeleryTask | Self::ApschedulerJob => {
                Some(Language::Python)
            }
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
        round_trip(BlockKind::ALL);
        round_trip(Role::ALL);
        round_trip(SlotName::ALL);
        round_trip(EdgeKind::ALL);
        round_trip(Resolution::ALL);
        round_trip(Reason::ALL);
        round_trip(EntranceKind::ALL);
        round_trip(TriggerStatus::ALL);
        round_trip(TargetRole::ALL);
        round_trip(HttpMethod::ALL);
        round_trip(DetectorId::ALL);
        round_trip(ParameterKind::ALL);
        round_trip(MemberKind::ALL);
        round_trip(Expansion::ALL);
        round_trip(PairingReason::ALL);
        round_trip(ResultBinding::ALL);
        round_trip(Heritage::ALL);
    }

    #[test]
    fn vocabulary_sizes_match_the_graph_model() {
        assert_eq!(BlockKind::ALL.len(), 32);
        assert_eq!(Role::ALL.len(), 18);
        assert_eq!(SlotName::ALL.len(), 19);
        assert_eq!(Reason::ALL.len(), 14);
        assert_eq!(EntranceKind::ALL.len(), 6);
        assert_eq!(HttpMethod::ALL.len(), 8);
        assert_eq!(DetectorId::ALL.len(), 10);
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
            names(BlockKind::ALL),
            names(Role::ALL),
            names(SlotName::ALL),
            names(EdgeKind::ALL),
            names(Resolution::ALL),
            names(Reason::ALL),
            names(EntranceKind::ALL),
            names(TriggerStatus::ALL),
            names(TargetRole::ALL),
            names(HttpMethod::ALL),
            names(DetectorId::ALL),
            names(ParameterKind::ALL),
            names(MemberKind::ALL),
            names(Expansion::ALL),
            names(PairingReason::ALL),
            names(ResultBinding::ALL),
            names(Heritage::ALL),
        ]
        .join("\n");
        assert_eq!(
            crate::text::ContentHash::of(encoding.as_bytes()).to_string(),
            "f1056a54d8780e17d67b288a66dcc66a93e732326b36435f641edd143c7e49a1",
            "a vocabulary's variant order changed: bump unfold-engine's ENGINE_VERSION so cached \
             facts are recomputed, then update this hash"
        );
    }

    #[test]
    fn unknown_values_are_rejected_with_their_vocabulary() {
        let error = "lambda".parse::<BlockKind>().unwrap_err();
        assert_eq!(error.to_string(), "unknown block kind `lambda`");
        assert!("get".parse::<HttpMethod>().is_err());
        assert!(serde_json::from_str::<Reason>("\"\"").is_err());
    }

    #[test]
    fn language_rules_follow_the_slot_table() {
        use BlockKind as K;
        use Language::{Python, TypeScript};
        assert!(K::ForOf.has_slot(SlotName::Else, Python));
        assert!(!K::ForOf.has_slot(SlotName::Else, TypeScript));
        assert!(
            !K::For.has_slot(SlotName::Body, Python),
            "Python never emits `for`"
        );
        assert!(K::With.has_slot(SlotName::Expression, Python));
        assert!(!K::With.has_slot(SlotName::Expression, TypeScript));
        assert!(K::Try.slot_is_indexed(SlotName::Catch, Python));
        assert!(!K::Try.slot_is_indexed(SlotName::Catch, TypeScript));
        assert!(K::Call.slot_is_indexed(SlotName::Argument, TypeScript));
        assert!(K::File.slots(TypeScript).is_empty());
        assert_eq!(
            K::Call.occurrence_edge_kinds(Python),
            &[EdgeKind::Calls, EdgeKind::Constructs]
        );
        assert_eq!(K::Call.occurrence_edge_kinds(TypeScript), &[EdgeKind::Calls]);
        assert!(K::Variable.occurrence_edge_kinds(TypeScript).is_empty());
        assert!(!Role::Header.allowed_in(K::Function, TypeScript));
        assert!(Role::Else.allowed_in(K::If, TypeScript));
        assert!(!Role::Else.allowed_in(K::While, TypeScript));
        assert!(Role::Else.allowed_in(K::While, Python));
    }

    #[test]
    fn target_rules_follow_the_edge_and_entrance_tables() {
        use BlockKind as K;
        assert!(EdgeKind::Calls.target_kinds().contains(&K::Getter));
        assert!(!EdgeKind::References.target_kinds().contains(&K::Getter));
        assert_eq!(EdgeKind::Imports.target_kinds(), &[K::File]);
        assert_eq!(
            TargetRole::Startup.target_kinds(DetectorId::PackageScript),
            &[K::File]
        );
        assert!(
            TargetRole::Startup
                .target_kinds(DetectorId::PyprojectScript)
                .contains(&K::Function)
        );
        assert!(
            TargetRole::Handler
                .target_kinds(DetectorId::Manual)
                .contains(&K::Case)
        );
        assert!(
            !TargetRole::Handler
                .target_kinds(DetectorId::ExpressRoute)
                .contains(&K::Case)
        );
        assert_eq!(DetectorId::CeleryTask.entrance_kinds(), &[EntranceKind::Worker]);
        assert_eq!(DetectorId::Manual.language(), None);
        assert_eq!(DetectorId::FastapiRoute.language(), Some(Language::Python));
        assert_eq!(EntranceKind::Process.target_role(), TargetRole::Startup);
        let function_like: Vec<BlockKind> = BlockKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.is_function_like())
            .collect();
        assert_eq!(&function_like, &EdgeKind::Calls.target_kinds()[..5]);
    }
}
