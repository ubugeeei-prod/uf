//! One module per command group.
//!
//! Each module owns both the work and the way that work is rendered, so the
//! shape of `uf build` on screen lives next to what `uf build` actually does.

pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod create;
pub(crate) mod dev;
pub(crate) mod env;
pub(crate) mod fmt;
pub(crate) mod info;
pub(crate) mod inspect;
pub(crate) mod lint;
pub(crate) mod pm;
pub(crate) mod release;
pub(crate) mod task;
pub(crate) mod test;
pub(crate) mod transform;
pub(crate) mod vite;
