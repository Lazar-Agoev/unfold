//! Contracts: what flows through one call, and the written types it names
//! (`docs/designs/mcp-capabilities.md`, "read_call"). Everything is computed per request from
//! the stored records of the caller's and the callee's files, plus the verified source only where
//! written text is shown.
//!
//! | Request | What it is |
//! | --- | --- |
//! | [`call_contract`] | A call's contract, by its block key: what it sends, what returns and where the value lands, what the callee raises, and which enclosing `except` or `catch` of the caller catches each |
//! | [`type_definition`] | A class or type: its written members, the members it inherits from each base, a type alias's aliased type, and how many places name it |
//! | [`type_uses`] | One page of the places a class or type is named, from the store's reverse index |
//!
//! A written type crosses IPC as a [`TypeText`]: its text and one [`TypeSpan`] per type name
//! inside it, in UTF-16 offsets relative to the text. Nothing here is inferred: a type is what
//! the code wrote, a pairing is what the engine proved, and a missing annotation is shown
//! missing.
//!
//! Facts are read through a [`Facts`] cache over a [`FactSource`] (the store's `Reader`, or an
//! in-memory `Graph`), so one request reads each file once.

mod call;
mod definition;
mod facts;
mod text;

#[cfg(test)]
mod tests;

use serde::Serialize;
use unfold_core::vocab::{Expansion, Form, Language, MemberKind, PairingReason, ParameterKind};
use unfold_core::{Key, RepoPath};

pub use call::call_contract;
pub use definition::{MAX_INHERITED_DEPTH, type_definition, type_uses};
pub use facts::{FactSource, Facts};

use crate::notes::LinkEnd;

/// One type name inside a written type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeSpan {
    /// Where the name starts, in UTF-16 code units from the start of the text.
    pub start: u32,
    /// Where it ends.
    pub end: u32,
    /// What the name reaches: a project class or type (which opens its definition), a library
    /// or builtin module, or nothing proven.
    pub to: LinkEnd,
}

/// A written type: its text (cut to 120 UTF-16 units) and the type names in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeText {
    /// The type as written.
    pub text: String,
    /// One span per type name inside the text, in order and never overlapping. Names the cut
    /// text no longer holds whole are left out.
    pub spans: Vec<TypeSpan>,
}

/// An argument as the caller wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SentArgument {
    /// Its position among the call's arguments.
    pub index: u32,
    /// Its written text (cut to 120 UTF-16 units), from the caller's verified source; absent
    /// when the caller's file changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
    /// How it is expanded, if it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub expand: Option<Expansion>,
    /// A Python keyword argument's keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub keyword: Option<String>,
}

/// A parameter of the callee, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractParameter {
    /// Its position among the callee's parameters.
    pub index: u32,
    /// Its name; absent for a destructuring pattern.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// What it is to calls.
    pub kind: ParameterKind,
    /// TypeScript `x?`.
    pub optional: bool,
    /// A default value is written.
    pub default: bool,
    /// Its written type; absent when no annotation is written.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
}

/// Why an argument fills no proven parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct UnpairedArgument {
    /// The pairing reason.
    pub reason: PairingReason,
    /// The reason in words.
    pub note: String,
}

/// One row of what a call sends: an argument and the parameter it fills (or why it fills
/// none), or a parameter no argument filled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractSend {
    /// The argument; absent for a parameter no argument filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub argument: Option<SentArgument>,
    /// The parameter the argument fills, or the parameter no argument filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parameter: Option<ContractParameter>,
    /// Why the argument fills no proven parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub unpaired: Option<UnpairedArgument>,
}

/// Where a call's value lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Landing {
    /// It is assigned to this written target (the call's `binds`).
    Binds {
        /// The target as written: `receipt`, `order.receipt`, `email_row_id, inserted`.
        target: String,
    },
    /// It is exactly what this exit returns, raises, throws, or yields.
    Exit {
        /// The exit.
        exit: Key,
        /// Its form.
        form: Form,
    },
    /// It is passed, inside this argument, to the call it is written in.
    Argument {
        /// The outer call.
        call: Key,
        /// The argument's position.
        index: u32,
    },
    /// It is what the call it is written in is called on.
    Receiver {
        /// The outer call.
        call: Key,
    },
    /// The code does nothing with it.
    Nowhere,
}

/// What comes back from a call, and where it lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractReturn {
    /// The callee's written return type, or the constructed class; absent when no return type
    /// is written (or the callee isn't a project definition).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
    /// Whether the call is written under `await`.
    pub awaited: bool,
    /// Where the value lands.
    pub lands: Landing,
}

/// One `raise` or `throw` of the callee's own code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractRaise {
    /// The exit.
    pub exit: Key,
    /// Its first line.
    pub line: u32,
    /// What it constructs when its value is exactly a construction: the class, or the callee as
    /// written with its module or reason.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
    /// Its value as written (cut to 120 UTF-16 units), from the callee's verified source;
    /// absent for a bare `raise` or when the callee's file changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub value: Option<String>,
    /// The caller's enclosing `except` or `catch` part that catches it, if one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub caught_by: Option<Key>,
}

/// A call's contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallContract {
    /// The call.
    pub call: Key,
    /// Its first line.
    pub line: u32,
    /// The callee as written.
    pub text: Box<str>,
    /// What the call runs.
    pub callee: LinkEnd,
    /// Each argument in written order, then each parameter no argument filled (when the
    /// pairings prove which were filled).
    pub sends: Vec<ContractSend>,
    /// What comes back and where it lands.
    pub returns: ContractReturn,
    /// What a project callee raises or throws itself and doesn't catch, in source order.
    pub raises: Vec<ContractRaise>,
}

/// What a class or type is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// A TypeScript interface.
    Interface,
    /// A TypeScript type alias.
    TypeAlias,
    /// A TypeScript enum.
    Enum,
    /// A class.
    Class,
}

/// One written member of a class or type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeMember {
    /// Its name as written; absent for call and construct signatures.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    /// What it is.
    pub kind: MemberKind,
    /// TypeScript `?`.
    pub optional: bool,
    /// TypeScript `readonly`.
    pub readonly: bool,
    /// A written initializer or default.
    pub default: bool,
    /// Its written type; absent when none is written.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
    /// An enum member's written initializer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub value: Option<String>,
}

/// The members a class or type inherits from one of its bases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct InheritedGroup {
    /// The base's name as written, with one span for what it reaches.
    pub from: TypeText,
    /// The base's written members, in source order; none for a base outside the project.
    pub members: Vec<TypeMember>,
}

/// A class or type, as its definition opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeDefinition {
    /// The class or type.
    pub key: Key,
    /// What it is.
    pub kind: TypeKind,
    /// Its name.
    pub name: String,
    /// Its file.
    pub path: RepoPath,
    /// Its first line.
    pub line: u32,
    /// Its file's language.
    pub language: Language,
    /// Its written members, in source order.
    pub members: Vec<TypeMember>,
    /// The members it inherits, one group per base it extends: depth first in written order,
    /// each type once (never the type itself), at most [`MAX_INHERITED_DEPTH`] levels down.
    pub inherited: Vec<InheritedGroup>,
    /// A type alias's aliased type, when the alias has no members, from its verified source;
    /// absent when the file changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub aliased: Option<TypeText>,
    /// How many `types` and `extends` links name it ("Used in N places").
    pub uses: u64,
}

/// How a place names a class or type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum UseKind {
    /// In an annotation of a definition (a parameter, a return type, a field).
    Annotation,
    /// In an `except` clause.
    Handler,
    /// As a base the owner extends.
    Extends,
    /// As an interface the owner implements.
    Implements,
}

/// One place a class or type is named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeUsePlace {
    /// The definition that names it: the link's own definition, or the one its `except` part
    /// belongs to.
    pub owner: Key,
    /// The owner's catalog label.
    pub label: String,
    /// The file.
    pub path: RepoPath,
    /// The line the name is on.
    pub line: u32,
    /// How it names the type.
    pub kind: UseKind,
}

/// One page of the places a class or type is named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeUses {
    /// How many places there are in all.
    pub total: u64,
    /// This page, by path, then position in the file.
    pub places: Vec<TypeUsePlace>,
    /// Where the next page starts, or `None` after the last page.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub next: Option<unfold_store::Cursor>,
}
