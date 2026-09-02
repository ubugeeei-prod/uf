//! Hot module replacement: the feedback loop, on the server that already
//! exists.
//!
//! # Shape
//!
//! ```text
//! PollWatcher ─▶ FileChange       one bounded stat walk, `.js` only
//!             ─▶ DevGraph::insert one scan of one file, edges relinked
//!             ─▶ invalidate       climb importers, then paint sides
//!             ─▶ HmrUpdate        origin-form targets, never paths
//!             ─▶ UpdateChannel    server-sent events, ids assigned here
//!             ─▶ GET the module   `resolve_with_policy`, the same one
//! ```
//!
//! Each stage is a module: [`watch`], [`graph`], [`mod@invalidate`],
//! [`update`], [`channel`], with [`session`] sequencing them and [`report`]
//! rendering the result through [`uf_term`].
//!
//! # Access control does not change here
//!
//! The update channel is served by the same listener, gated by the same
//! [`crate::network`] allowlists, and the modules an update names are fetched
//! through [`crate::resolve`] — one canonicalization, one decision, one open.
//! [`update::fetch_update`] exists so the update path has a name, not so it has
//! a different implementation: it is a target parse in front of
//! [`crate::resolve::resolve_with_policy`], and `tests/attack_corpus.rs` drives
//! `../../.env` through it and asserts the refusal is identical to the one a
//! plain request gets.
//!
//! No inbound header reaches any of this. `Last-Event-ID`, the resume cursor
//! the server-sent-events specification defines, is dropped with every other
//! header by [`crate::http::RequestHead`], and a subscriber's position is
//! assigned by the server. See [`channel`] for why that is the same bug class
//! as [CVE-2025-29927].
//!
//! # Exactness
//!
//! Invalidation is the part that has to be right. Over-invalidating turns a
//! hot update into a page reload; under-invalidating serves the browser code
//! nobody wrote. [`mod@invalidate`] documents each rule and every one of them
//! has a test.
//!
//! ```
//! use uf_devserver::hmr::{ChangeKind, DevGraph, UpdateKind, invalidate};
//!
//! let mut graph = DevGraph::new();
//! graph.insert("app/types.js", "// @flow\nexport type User = { id: string };\n")?;
//! graph.insert(
//!     "app/Counter.js",
//!     "\"use client\";\nimport type { User } from \"./types.js\";\n\
//!      export function Counter() { return null; }\n",
//! )?;
//!
//! // A type-only module invalidates nothing at runtime.
//! let types = graph.find("app/types.js").expect("scanned");
//! let inert = invalidate(&graph, types, ChangeKind::Modified);
//! assert_eq!(inert.kind(), UpdateKind::Inert);
//!
//! // A client component accepts its own update.
//! let counter = graph.find("app/Counter.js").expect("scanned");
//! let hot = invalidate(&graph, counter, ChangeKind::Modified);
//! assert_eq!(hot.kind(), UpdateKind::Hot);
//! assert_eq!(hot.boundaries(), [counter]);
//! # Ok::<(), uf_devserver::hmr::GraphError>(())
//! ```
//!
//! [CVE-2025-29927]: https://nvd.nist.gov/vuln/detail/CVE-2025-29927

pub mod channel;
pub mod graph;
pub mod invalidate;
pub mod report;
pub mod session;
pub mod update;
pub mod watch;

pub use channel::{
    EVENT_STREAM_HEAD, HEARTBEAT_FRAME, HEARTBEAT_INTERVAL, MAX_BUFFERED_UPDATES, MAX_SUBSCRIBERS,
    RETRY_MILLIS, StreamLimits, SubscribeError, Subscriber, UPDATE_EVENT, UpdateChannel, Waited,
    encode_frame, write_event_stream,
};
pub use graph::{
    DevGraph, DevModule, DevModuleId, GraphError, Insertion, MAX_MODULE_BYTES, MAX_MODULE_DEPTH,
    MAX_MODULE_IMPORTS, MAX_MODULES, ModuleState, ModuleSurface, module_path,
};
pub use invalidate::{
    ChangeKind, Invalidation, MAX_INVALIDATION_DEPTH, ReloadReason, UpdateKind, UpdateSide,
    invalidate,
};
pub use report::{
    change_label, render_update, render_update_modules, render_watch_error, update_label,
    update_status,
};
pub use session::HmrSession;
pub use update::{
    HmrUpdate, MAX_UPDATE_MODULES, MAX_UPDATE_TARGET_BYTES, UpdateModule, UpdateRole, fetch_update,
    update_target,
};
pub use watch::{
    DEFAULT_POLL_INTERVAL, FileChange, MAX_POLL_INTERVAL, MAX_WATCH_DEPTH, MAX_WATCHED_FILES,
    MIN_POLL_INTERVAL, PollWatcher, SKIPPED_DIRECTORIES, WATCHED_EXTENSION, WatchError,
    watched_files,
};

/// The request target the browser opens its `EventSource` on.
///
/// Under the same `/__uf/` prefix as [`crate::http::HEALTH_TARGET`], matched
/// exactly, and only after [`crate::target::RequestTarget`] has accepted the
/// request as origin-form. A target the grammar rejects never reaches this
/// comparison.
pub const HMR_TARGET: &str = "/__uf/hmr";

/// The `@uniflowed/core` subpath of the client runtime that opens the channel.
///
/// Named here so the server and the shipped module cannot drift apart without a
/// test noticing.
pub const CLIENT_RUNTIME_SPECIFIER: &str = "@uniflowed/core/hmr";
