//! Unit tests, one file per topic.
//!
//! Everything here is about the decisions `uf` makes on its own: what a file
//! declares, what order files run in, what a filter keeps, what an edit
//! invalidates. Actually executing a test needs a JavaScript host and the `uf`
//! binary the worker transforms through, so those tests live in `uf_cli`,
//! where both exist — see `crates/uf_cli/tests/testing.rs`.

mod discovery;
mod filtering;
mod graph;
mod schedule;
mod security;
mod selection;
mod timings;
mod watch;
