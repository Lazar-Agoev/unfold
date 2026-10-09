//! Integration tests for `unfold-store`'s public API. Fault injection and concurrency tests,
//! which need the writer's test hooks, live in `src/tests`.

mod bench;
mod commit;
mod common;
mod facts;
mod open;
mod reads;
mod search;
