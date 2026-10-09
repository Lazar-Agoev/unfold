//! The Python binder's per-file half: extraction records scopes, declarations, the write
//! census, and the expressions binding evaluates ([`ir`]), so linking can bind names across the
//! whole program from cached facts alone ([`crate::link`]).

mod build;
mod definitions;
mod expressions;
mod exprs;
mod finish;
pub mod ir;

pub(crate) use build::{Inputs, alias_address, build};
