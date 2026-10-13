//! Typed lines: what flows through one call a board draws, and the written types it names
//! (`docs/reader.md`, "Typed lines"). Everything is computed per request from the stored typed
//! records of one or two files (the caller's and the callee's), plus the verified source only
//! where written text is shown.
//!
//! | Request | What it is |
//! | --- | --- |
//! | [`call_contract`] | The selected line's contract: what each argument fills, unfilled parameters, where the result lands, the callee's direct throws, and the lines the contract read |
//! | [`type_definition`] | A type declaration or class: its written members, the members it inherits from each base, a type alias's aliased type, and how many places use it |
//! | [`type_uses`] | One page of the places a declaration's type links come from |
//!
//! A written type crosses IPC as a [`TypeText`]: its text and one [`TypeSpan`] per type name
//! inside it, in UTF-16 offsets relative to the text. Nothing here is inferred: a type is what
//! the code wrote, a pairing is what the engine proved.
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
use unfold_core::vocab::{
    Expansion, Heritage, Language, MemberKind, PairingReason, ParameterKind, ResultBinding,
};
use unfold_core::{Key, ModuleLabel, RepoPath};

pub use call::call_contract;
pub use definition::{MAX_INHERITED_DEPTH, type_definition, type_uses};
pub use facts::{FactSource, Facts, UseSite};

/// How one type name inside a written type reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TypeStyle {
    /// It resolves to a declaration of the graph (`target`): it opens its definition.
    Declared,
    /// It comes from a package (`module`), other than the language's built-ins.
    External,
    /// A built-in type: external `global`, `python:builtins`, or `python:typing`.
    Builtin,
    /// It has no reliable declaration; `note` says why.
    Unresolved,
}

/// One type name inside a written type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeSpan {
    /// Where the name starts, in UTF-16 code units from the start of the text.
    pub start: u32,
    /// Where it ends.
    pub end: u32,
    /// How it reads.
    pub style: TypeStyle,
    /// The declaration, for a `declared` name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub target: Option<Key>,
    /// Where an `external` or `builtin` name comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub module: Option<ModuleLabel>,
    /// The module (external) or the reason (unresolved) in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
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
    /// Its written text (cut to 120 UTF-16 units), sliced from the caller's verified source;
    /// absent when the caller's file changed.
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
    /// Its written type.
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

/// One row of a contract's Sends: an argument and the parameter it fills (or why it fills
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

/// Where a call's value lands, and what the callee says it returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractReturn {
    /// Where the value lands.
    pub binding: ResultBinding,
    /// The variable's name (`variable`), or the outer call's written callee (`argument`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
    /// Whether the call is awaited.
    pub awaited: bool,
    /// The callee's written return type, or a constructed class's name.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
}

/// One of the callee's direct throws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContractThrow {
    /// The thrown class's name (`declared` when it resolved), or the construction as written
    /// with its module or reason.
    #[serde(rename = "type")]
    pub ty: TypeText,
    /// The `throw` statement's first line.
    pub line: u32,
}

/// The contract of one selected line: a resolved call or construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallContract {
    /// The occurrence.
    pub source: Key,
    /// The edge's sequence (0: the call itself).
    pub seq: u32,
    /// The callee.
    pub target: Key,
    /// The calling line.
    pub caller_line: u32,
    /// Each argument in written order, then each parameter no argument filled.
    pub sends: Vec<ContractSend>,
    /// Where the result lands and the callee's return type.
    pub returns: ContractReturn,
    /// The callee's direct throws, in source order.
    pub throws: Vec<ContractThrow>,
    /// Whether the call sits in the body of a `try` with a `catch` handler in the caller.
    pub caught: bool,
    /// The callee lines the contract read: its parameters and return type (when its source
    /// verifies) and its listed throws, ascending.
    pub callee_lines: Vec<u32>,
}

/// What a type declaration is.
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

/// One written member of a type declaration or class.
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
    /// Its written type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ty: Option<TypeText>,
    /// An enum member's written initializer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub value: Option<String>,
}

/// The members a type inherits from one of its bases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct InheritedGroup {
    /// The base's name as a heritage clause writes it, with one span: `declared` (its key),
    /// `external` or `builtin` (its module), or `unresolved` (the reason in words).
    pub from: TypeText,
    /// The base's written members, in source order; none for a base that isn't `declared`.
    pub members: Vec<TypeMember>,
}

/// A type declaration or class, as its definition opens under a type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeDefinition {
    /// The declaration.
    pub key: Key,
    /// What it is.
    pub kind: TypeKind,
    /// Its name (its catalog label when unnamed).
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
    /// A type alias's aliased type, when it isn't an object type (an alias with no members),
    /// from its verified source; absent when the file changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub aliased: Option<TypeText>,
    /// How many type links resolve to it ("Used in N places").
    pub uses: u32,
}

/// One place a declaration's type is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeUsePlace {
    /// The definition (or file) the annotation belongs to.
    pub owner: Key,
    /// Its catalog label (a name, `callback to …`, or the file's path).
    pub label: String,
    /// The file.
    pub path: RepoPath,
    /// The line the type name is on.
    pub line: u32,
    /// When the place is a heritage clause naming the type as a base: whether the owner
    /// extends or implements it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub heritage: Option<Heritage>,
}

/// One page of the places a declaration's type is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TypeUses {
    /// How many places there are in all.
    pub total: u32,
    /// This page, by path, then position in the file.
    pub places: Vec<TypeUsePlace>,
}
