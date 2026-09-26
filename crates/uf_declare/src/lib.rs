#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![deny(missing_docs)]
//! The TypeScript declarations a Flow library publishes.
//!
//! `uf build` for a library writes `es` and `cjs` beside the Flow source and,
//! until ubugeeei-prod/uf#969, nothing else. Most people who install a Flow
//! library write TypeScript, and to them a package with no `.d.ts` is `any` —
//! every call into it unchecked, every rename of its API silent. This crate
//! is the missing half: it reads the Flow modules a library exports and
//! writes the declaration file a TypeScript consumer is typed against.
//!
//! [`uf_dts`] is the same boundary from the other side, translating a
//! dependency's `.d.ts` into Flow so `uf check` can read npm. The two never
//! meet — one reads TypeScript and writes Flow, the other reads Flow and
//! writes TypeScript — and they are separate crates because they share no
//! code, only a principle.
//!
//! # Honest translation, not total translation
//!
//! That principle is the whole design. Flow and TypeScript are not the same
//! language, and a translator that insists on producing *something* for every
//! construct produces a declaration file that lies. The lie is worse here
//! than almost anywhere else in a toolchain, because **TypeScript does not
//! check a `.d.ts` against the code it describes**: whatever this crate
//! writes is believed. A widened type is not a weaker guarantee, it is a
//! wrong answer that the consumer discovers at runtime.
//!
//! So the rule is: translate what maps cleanly, refuse to guess at what does
//! not, and record every refusal as a [`Gap`] naming the declaration it
//! happened in. `uf build` prints the list. A library author told "these
//! three exports could not be translated" is far better served than one
//! handed a `.d.ts` that quietly says `any`.
//!
//! A gap is not always a refusal. [`Construct::keeps_the_type`] separates the
//! two: an exact object type still publishes every one of its members and
//! still rejects a missing or mistyped one — what it loses is the rejection
//! of an *extra* one — while `$Diff<A, B>` leaves `unknown` where a type was.
//! Both are reported, because both are things the author did not write and
//! would not otherwise find out about.
//!
//! # What is deliberately not attempted
//!
//! A name this crate does not recognise is copied through, because in a
//! library's own declarations most named types are the library's own. The
//! exception is the set it *does* recognise as having no TypeScript meaning —
//! Flow's `$`-prefixed utilities and its React types — which become `unknown`
//! and a gap rather than a dangling reference that would stop the emitted
//! file compiling at all.
//!
//! [`uf_dts`]: https://docs.rs/uf_dts

mod emit;
mod gap;
mod printer;
mod resolve;
mod unit;

pub use crate::gap::{Construct, Gap};
pub use crate::resolve::declaration_path;
pub use crate::unit::{Module, Translation, translate};
