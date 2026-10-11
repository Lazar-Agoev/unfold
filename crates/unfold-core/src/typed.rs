//! The typed records of one file: what each call sends, returns, and where its value lands, and
//! the written types of declarations (`docs/graph-model.md`, "Typed records").
//!
//! | Record | One per |
//! | --- | --- |
//! | [`Signature`] | Function-like block: its written return type |
//! | [`Parameter`] | `parameter` slot: kind, name, flags, written type |
//! | [`Member`] | Written member of an `interface`, `type_alias`, `enum`, or `class` block |
//! | [`Argument`] | `argument` slot of a `call` or `new` block: expansion, keyword, and pairing |
//! | [`CallResult`] | `call` or `new` block: where its value lands |
//! | [`TypeLink`] | Type name written in a recorded annotation or a heritage clause, with how it resolved |
//!
//! Records refer to blocks of their own file by local ID (and to other files' blocks by key),
//! like slots and edges. Written texts are cut to 120 UTF-16 units; ranges are token-exact UTF-8
//! byte ranges. Only what is written is recorded: nothing here is inferred.

use serde::{Deserialize, Serialize};

use crate::key::Key;
use crate::model::ModuleLabel;
use crate::text::ByteRange;
use crate::vocab::{
    Expansion, Heritage, MemberKind, PairingReason, ParameterKind, Reason, Resolution, ResultBinding,
};

/// A type as written: its text (cut to 120 UTF-16 units) and its full range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WrittenType {
    /// The written text.
    pub text: Box<str>,
    /// Its token-exact range.
    pub range: ByteRange,
}

/// The written return type of one function-like block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// The function-like block's local ID.
    pub block: u32,
    /// The written return type (TypeScript: after `:`; Python: after `->`), inside the block's
    /// `return_type` slot.
    pub returns: Option<WrittenType>,
}

/// One parameter of a function-like block, aligned with its `parameter` slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// The function-like block's local ID.
    pub block: u32,
    /// The `parameter` slot's index.
    pub index: u32,
    /// What it is to calls.
    pub kind: ParameterKind,
    /// The identifier; absent for a destructuring pattern.
    pub name: Option<Box<str>>,
    /// TypeScript `x?`.
    pub optional: bool,
    /// A default value is written.
    pub default: bool,
    /// The written annotation, inside the parameter slot.
    pub ty: Option<WrittenType>,
}

/// One written member of a type declaration or class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The `interface`, `type_alias`, `enum`, or `class` block's local ID.
    pub block: u32,
    /// Its position among the block's members, contiguous from 0.
    pub index: u32,
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

/// One argument of a call or construction, aligned with its `argument` slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    /// The `call` or `new` block's local ID.
    pub call: u32,
    /// The `argument` slot's index.
    pub index: u32,
    /// How it is expanded, if it is.
    pub expand: Option<Expansion>,
    /// A Python keyword argument's name.
    pub keyword: Option<Box<str>>,
    /// Its pairing, exactly when the call's occurrence edge is resolved.
    pub pairing: Option<Pairing>,
    /// For an argument that passes a callable (a resolved `references` edge at its position to
    /// a function-like block): how many of that callable's leading parameters the passed value
    /// binds itself (`obj.method` binds the instance), when that is proven. Absent on every
    /// other argument, and on a passed callable whose binding isn't proven.
    pub bound: Option<u32>,
}

/// Where the value of one call or construction lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallResult {
    /// The `call` or `new` block's local ID.
    pub call: u32,
    /// Where it lands.
    pub binding: ResultBinding,
    /// Whether an `await` was one of the transparent layers around it.
    pub awaited: bool,
    /// The `variable` block it initializes, or the outer `call`/`new` block it is an argument
    /// of.
    pub target: Option<u32>,
    /// For an argument, the outer argument's slot index.
    pub position: Option<u32>,
    /// When the occurrence resolved to a function-like block: how many of its leading
    /// parameters the call binds itself (a Python method called through an instance binds
    /// `self`), when that is proven. Absent otherwise.
    pub bound: Option<u32>,
}

/// A type name written in a recorded annotation or a heritage clause, and the declaration it
/// resolves to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeLink {
    /// The innermost block whose range contains the name.
    pub block: u32,
    /// The written name, a qualified name whole, without type arguments.
    pub range: ByteRange,
    /// The name as written (at most 120 UTF-16 units).
    pub text: Box<str>,
    /// How it resolved.
    pub resolution: Resolution,
    /// The declaration's key, exactly when resolved.
    pub target: Option<Key>,
    /// Why it is unresolved, exactly when unresolved.
    pub reason: Option<Reason>,
    /// The module label, exactly when external.
    pub module: Option<ModuleLabel>,
    /// When the name is a base of its block (a `class` or `interface`) in a heritage clause:
    /// whether the block extends or implements it. Absent for every other name, a type argument
    /// written in a heritage clause included.
    pub heritage: Option<Heritage>,
}

/// One file's typed records, each in canonical order: signatures, parameters, members,
/// arguments, and results by block (preorder) then index; type links by block, then range.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FileTypes {
    /// One per function-like block.
    pub signatures: Vec<Signature>,
    /// One per `parameter` slot.
    pub parameters: Vec<Parameter>,
    /// The written members of type declarations and classes.
    pub members: Vec<Member>,
    /// One per `argument` slot of a `call` or `new` block.
    pub arguments: Vec<Argument>,
    /// One per `call` or `new` block.
    pub results: Vec<CallResult>,
    /// Every type name in a recorded annotation or a heritage clause.
    pub type_links: Vec<TypeLink>,
}

impl FileTypes {
    /// Whether the file has no typed records at all.
    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
            && self.parameters.is_empty()
            && self.members.is_empty()
            && self.arguments.is_empty()
            && self.results.is_empty()
            && self.type_links.is_empty()
    }

    /// Puts every record in canonical order (see [`FileTypes`]).
    pub fn sort(&mut self) {
        self.signatures.sort_by_key(|signature| signature.block);
        self.parameters
            .sort_by_key(|parameter| (parameter.block, parameter.index));
        self.members.sort_by_key(|member| (member.block, member.index));
        self.arguments
            .sort_by_key(|argument| (argument.call, argument.index));
        self.results.sort_by_key(|result| result.call);
        self.type_links.sort_by_key(|link| (link.block, link.range));
    }

    /// Rewrites every local ID through `map` (old local ID → new), dropping records whose block
    /// (or, for a result, whose target) was dropped. Used when a file's blocks are renumbered.
    pub fn renumber(&mut self, map: impl Fn(u32) -> Option<u32>) {
        self.signatures
            .retain_mut(|record| map(record.block).map(|block| record.block = block).is_some());
        self.parameters
            .retain_mut(|record| map(record.block).map(|block| record.block = block).is_some());
        self.members
            .retain_mut(|record| map(record.block).map(|block| record.block = block).is_some());
        self.arguments
            .retain_mut(|record| map(record.call).map(|call| record.call = call).is_some());
        self.results.retain_mut(|record| {
            let Some(call) = map(record.call) else {
                return false;
            };
            record.call = call;
            match record.target {
                None => true,
                Some(target) => map(target).map(|target| record.target = Some(target)).is_some(),
            }
        });
        self.type_links
            .retain_mut(|record| map(record.block).map(|block| record.block = block).is_some());
    }
}
