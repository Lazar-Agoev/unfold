//! The Python front end end to end: lowering to nodes, the binder and linker, typed records,
//! the census, failures, robustness, determinism, and incremental compiles.

mod binder;
mod common;
mod engine;
mod failures;
mod lowering;
mod robustness;
mod typed;
