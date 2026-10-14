//! The typed records of one file: what each function takes and returns, what each call sends,
//! and the written members of types and classes (`docs/designs/diagram-blocks.md`, "Records").
//!
//! | Record | One per |
//! | --- | --- |
//! | [`Signature`] | Function: its written return type |
//! | [`Parameter`] | Parameter of a function: kind, name, flags, written type |
//! | [`Member`] | Written member of a type or class: its range, kind, name, flags, written type |
//! | [`Argument`] | Argument of a call: expansion, keyword, and pairing |
//!
//! Where a call's value lands is the call node's `binds` and `exit_value`, and whether it is
//! awaited its `awaited` flag. A type name written in an annotation, a class base, or a handler
//! clause is a `types` or `extends` link.
//!
//! Records refer to nodes of their own file by local ID, like links. Written texts are cut to
//! 120 UTF-16 units; ranges are token-exact UTF-8 byte ranges. Only what is written is
//! recorded: nothing here is inferred.

use serde::{Deserialize, Serialize};

use crate::text::ByteRange;
use crate::vocab::{Expansion, MemberKind, PairingReason, ParameterKind};

/// A type as written: its text (cut to 120 UTF-16 units) and its full range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WrittenType {
    /// The written text.
    pub text: Box<str>,
    /// Its token-exact range.
    pub range: ByteRange,
}

/// The written return type of one function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// The function's local ID.
    pub node: u32,
    /// The written return type (TypeScript: after `:`; Python: after `->`), inside the
    /// function's `head`.
    pub returns: Option<WrittenType>,
}

/// One parameter of a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// The function's local ID.
    pub node: u32,
    /// Its position among the function's parameters, contiguous from 0.
    pub index: u32,
    /// The whole parameter as written, inside the function's `head`.
    pub range: ByteRange,
    /// What it is to calls.
    pub kind: ParameterKind,
    /// The identifier; absent for a destructuring pattern.
    pub name: Option<Box<str>>,
    /// TypeScript `x?`.
    pub optional: bool,
    /// A default value is written.
    pub default: bool,
    /// The written annotation, inside `range`.
    pub ty: Option<WrittenType>,
}

/// One written member of a type or class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The type's or class's local ID.
    pub node: u32,
    /// Its position among the node's members, contiguous from 0.
    pub index: u32,
    /// The whole member as written (the field, property, method, signature, or enum member),
    /// inside the node's range.
    pub range: ByteRange,
    /// What it is.
    pub kind: MemberKind,
    /// The name as written; absent for call and construct signatures, an index signature's
    /// parameter text for an index signature.
    pub name: Option<Box<str>>,
    /// TypeScript `?`.
    pub optional: bool,
    /// TypeScript `readonly`.
    pub readonly: bool,
    /// A written initializer (class field) or default (Python).
    pub default: bool,
    /// The written type: an annotation, or a method or signature's text after its name.
    pub ty: Option<WrittenType>,
    /// An enum member's written initializer.
    pub value: Option<Box<str>>,
}

/// How an argument pairs with a parameter of its call's resolved target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pairing {
    /// It fills the target's parameter with this index.
    Parameter(u32),
    /// It fills no proven parameter, for this reason.
    Unpaired(PairingReason),
}

/// One argument of a call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    /// The call's local ID.
    pub call: u32,
    /// Its position among the call's arguments, contiguous from 0.
    pub index: u32,
    /// The whole argument as written, inside the call's range.
    pub range: ByteRange,
    /// How it is expanded, if it is.
    pub expand: Option<Expansion>,
    /// A Python keyword argument's name.
    pub keyword: Option<Box<str>>,
    /// Its pairing, exactly when the call's `calls` link reaches a project definition.
    pub pairing: Option<Pairing>,
    /// For an argument that passes a function (a project `passes` link written in it): how
    /// many of that function's leading parameters the passed value binds itself
    /// (`obj.method` binds the instance), when that is proven. Absent on every other argument,
    /// and on a passed function whose binding isn't proven.
    pub bound: Option<u32>,
}

/// One file's typed records, each in canonical order: by node (preorder), then index.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FileTypes {
    /// One per function.
    pub signatures: Vec<Signature>,
    /// One per parameter of a function.
    pub parameters: Vec<Parameter>,
    /// The written members of types and classes.
    pub members: Vec<Member>,
    /// One per argument of a call.
    pub arguments: Vec<Argument>,
}

impl FileTypes {
    /// Puts every record in canonical order (see [`FileTypes`]).
    pub fn sort(&mut self) {
        self.signatures.sort_by_key(|signature| signature.node);
        self.parameters
            .sort_by_key(|parameter| (parameter.node, parameter.index));
        self.members.sort_by_key(|member| (member.node, member.index));
        self.arguments
            .sort_by_key(|argument| (argument.call, argument.index));
    }
}
