//! The alien language's private facts payload: declarations, scopes, and what each recorded
//! value is, so linking can answer evidence questions without the syntax tree.

use serde::{Deserialize, Serialize};
use unfold_core::ByteRange;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub decls: Vec<Decl>,
    pub scopes: Vec<Scope>,
    /// Aligned with the neutral value table.
    pub values: Vec<ValueInfo>,
    /// Token boundaries, call sites, and module dependencies for the census.
    pub boundaries: Vec<u32>,
    pub call_sites: Vec<CallSite>,
    pub dependencies: Vec<ByteRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub range: ByteRange,
    pub new: bool,
    pub ineligible: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub parent: Option<u32>,
    pub kind: ScopeKind,
    /// Declarations bound directly in this scope, in source order.
    pub decls: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    File,
    /// A callable's scope: arrows (`lambda`) inherit `this`; members bind it to their class.
    Function {
        arrow: bool,
        class: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decl {
    pub name: String,
    /// The scope the declaration is looked up from (for members, the class's scope).
    pub scope: u32,
    pub kind: DeclKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclKind {
    Function {
        block: u32,
    },
    Class {
        block: u32,
        ctor: Option<u32>,
        bases: Vec<String>,
        members: Vec<u32>,
    },
    Method {
        block: u32,
        owner: u32,
        is_static: bool,
        is_private: bool,
    },
    Ctor {
        block: u32,
        owner: u32,
    },
    Getter {
        block: u32,
        owner: u32,
    },
    Field {
        owner: u32,
        init: Option<u32>,
    },
    Variable {
        block: u32,
        mutable: bool,
        init: Option<u32>,
        ty: Option<String>,
    },
    Signature,
    Param {
        owner: u32,
        position: Option<u32>,
        ty: Option<String>,
    },
    Use {
        spec: String,
        export: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueInfo {
    Ref {
        name: String,
        scope: u32,
        options: Vec<(String, Option<String>)>,
    },
    Get {
        receiver: u32,
        name: String,
        external: Option<String>,
    },
    Key {
        receiver: u32,
        name: String,
    },
    Dynamic,
    This {
        scope: u32,
    },
    Base {
        scope: u32,
    },
    Call {
        callee: Option<u32>,
    },
    Inline {
        block: u32,
        class: bool,
    },
    Literal(String),
    Other,
}
