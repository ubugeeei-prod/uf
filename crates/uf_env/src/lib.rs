#![deny(missing_docs)]
//! Per-repository JavaScript toolchains.
//!
//! A project says which runtimes and package managers it uses and at which
//! versions; uf installs them into a store shared by every repository on the
//! machine, links them into the project, and collects what nothing uses any
//! more. Nothing is installed globally and nothing on `PATH` is changed.
//!
//! # The three pieces
//!
//! - [`store`] holds one immutable directory per tool, version and platform.
//!   Two repositories on the same Node share it.
//! - [`roots`] records which repository uses which entries, outside the
//!   repository, so a checkout that is deleted stops holding its tools.
//! - [`gc`] deletes what the roots do not reach.
//!
//! [`tool`] is the vocabulary the three share.
//!
//! # What is not here
//!
//! Fetching. Acquiring an archive means a network, a checksum published by
//! somebody else, and a `tar` — all of which belong to the command, not to
//! the model. This crate is given a directory that is already unpacked and
//! decides where it goes and when it goes away, which is what makes every
//! part of it testable without a network.

pub mod gc;
pub mod roots;
pub mod store;
pub mod tool;

#[cfg(test)]
mod tests;

use camino::Utf8PathBuf;

pub use gc::Plan;
pub use roots::{Root, Roots};
pub use store::Store;
pub use tool::{Arch, Os, Pin, Platform, Tool};

/// Anything that can go wrong managing an environment.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnvError {
    /// A path could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// The path.
        path: Utf8PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A path could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// The path.
        path: Utf8PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A root could not be parsed, so what it holds is unknown.
    #[error("failed to read the root {path}: {source}")]
    Decode {
        /// The root file.
        path: Utf8PathBuf,
        /// The underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// A root could not be written.
    #[error("failed to encode a root: {0}")]
    Encode(#[source] serde_json::Error),
    /// A path in the store or the roots is not UTF-8.
    #[error("{path} is not valid UTF-8")]
    NotUtf8 {
        /// The path, as the operating system gave it.
        path: std::path::PathBuf,
    },
    /// Neither the XDG variable nor `$HOME` is set, so there is nowhere the
    /// store could mean.
    #[error("neither XDG_DATA_HOME nor HOME is set, so there is no store directory")]
    NoHome,
    /// uf runs here but does not know how to fetch tools for this platform.
    #[error("uf does not install tools for {os} on {arch}")]
    UnsupportedPlatform {
        /// The operating system, as Rust names it.
        os: &'static str,
        /// The architecture, as Rust names it.
        arch: &'static str,
    },
}
