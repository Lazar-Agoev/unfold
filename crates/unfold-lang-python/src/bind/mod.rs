//! The Python binder's per-file half: extraction records scopes, declarations, the write
//! census, the expressions binding evaluates, and the written type names of annotations, class
//! bases, and handler clauses ([`ir`]), so linking can bind names (and resolve types) across the
//! whole program from cached facts alone ([`crate::link`]).

mod build;
mod definitions;
mod expressions;
mod exprs;
mod finish;
pub mod ir;
mod types;

pub(crate) use build::{Inputs, alias_address, build};
pub(crate) use types::{TypeName, TypeSite};
