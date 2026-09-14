//! TypeScript declaration files, read as Flow.
//!
//! A uf project is written in Flow, and most of npm ships TypeScript
//! declarations instead. Until this crate, a dependency with no Flow in it was
//! Flow's *unchecked module*: `import { z } from "zod"` typed `z` as `any`,
//! and every call through it was checked against nothing. A TypeScript project
//! gets types for `zod`, `date-fns` or `@tanstack/query-core` by installing
//! them; a uf project got `any` for each. That is ubugeeei-prod/uf#946, and
//! this crate is the translation that closes it: a package's `.d.ts` files in,
//! Flow declaration modules out, and a list of every place the translation
//! could not say what TypeScript said.
//!
//! # Why a translation
//!
//! The vendored Flow port has a `.d.ts` parsing mode of its own, keyed off the
//! file name. uf does not hand it declaration files, for the reason this crate
//! exists: a hole must never be silent. A dependency reaches the checker as a
//! packed signature, and what `flow_type_sig` cannot pack it types `any`
//! without a word — `type_sig_parse.rs` says so beside the gated constructs.
//! A translation makes every such decision itself, so it can report every one
//! of them ([`Hole`]), and what reaches the checker is an ordinary Flow
//! declaration module, typed by the same path every other dependency takes.
//!
//! oxc reads the TypeScript, because it is the TypeScript parser already in
//! the workspace — `uf_transform` generates code with it.
//!
//! # What comes out
//!
//! One Flow module per declaration file, filed under [`flow_path`] of the
//! declaration's own path. Relative specifiers are rewritten to name the
//! translated modules, and bare ones are left alone for the checker to
//! resolve; a declaration file importing another package is another
//! translation, asked for when something needs it. Every statement is printed
//! on the source line it was written on, so a location inside a translation is
//! a location in the file a reader can open — see the `printer` module.
//!
//! The `emit` module says what each construct becomes, and [`Construct`] lists
//! what becomes a hole.
//!
//! # What this crate does not do
//!
//! Read a filesystem, resolve a package's entry points, or cache anything.
//! [`translate`] is a function of the files it reads, handed in by the caller,
//! so the caller can key a cache by exactly those files and nothing here can
//! go stale.

#![deny(missing_docs)]

mod emit;
mod hole;
mod printer;
mod resolve;
mod summary;
mod unit;

pub use crate::hole::{Construct, Hole};
pub use crate::resolve::{declarations_for, flow_path, is_declaration};
pub use crate::unit::{Module, Translation, translate};
