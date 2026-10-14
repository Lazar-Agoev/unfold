//! The Python front end end to end: lowering to nodes, the binder and linker, typed records,
//! the census, failures, robustness, determinism, incremental compiles, and the `py-orders`
//! playground's goldens.

mod binder;
mod common;
mod engine;
mod failures;
mod goldens;
mod lowering;
mod robustness;
mod typed;
