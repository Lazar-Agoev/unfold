//! The alien language's binder payload: declarations, scopes, and what each recorded value is,
//! so linking can answer evidence questions without the syntax tree.

use serde::{Deserialize, Serialize};
use unfold_core::ByteRange;
use unfold_core::vocab::Form;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub decls: Vec<Decl>,
    pub scopes: Vec<Scope>,
    /// Aligned with the neutral value table.
    pub values: Vec<ValueInfo>,
    /// Call sites for the census, from the parser.
    pub call_sites: Vec<CallSite>,
    /// Aligned with the neutral type names: the scope each name is looked up from, and the
    /// name.
    pub types: Vec<(u32, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub range: ByteRange,
    /// `new` for a `new` form, `call` for a `super` form; a `call` form's is only known after
    /// linking (a `:construct` callee makes it `new`).
    pub form: Option<Form>,
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
    /// A function's scope: arrows (`lambda`) inherit `this`; members bind it to their class.
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

/// What a declaration is; `node` fields are local IDs of the declaring nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclKind {
    Function {
        node: u32,
    },
    Class {
        node: u32,
        ctor: Option<u32>,
        bases: Vec<String>,
        members: Vec<u32>,
        /// A `(deco X)` item is written on the class.
        decorated: bool,
    },
    Method {
        node: u32,
        owner: u32,
        is_static: bool,
        is_private: bool,
    },
    Ctor {
        node: u32,
        owner: u32,
    },
    Getter {
        node: u32,
        owner: u32,
    },
    Field {
        owner: u32,
        init: Option<u32>,
    },
    Variable {
        mutable: bool,
        init: Option<u32>,
        ty: Option<String>,
    },
    Signature,
    Param {
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
    /// A function written as a value: its node.
    Inline {
        node: u32,
    },
    Literal(String),
    Other,
}
