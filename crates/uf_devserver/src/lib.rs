#![deny(missing_docs)]
//! The `uf dev` HTTP server and its access-control layer.
//!
//! # Why this is its own crate
//!
//! The Vite dev server was bypassed four separate ways in a single year —
//! [CVE-2025-30208], [CVE-2025-31125], [CVE-2025-32395] and [CVE-2025-62522] —
//! and all four are one bug: **the access decision ran against a string that
//! was not the path that was eventually opened.** A query suffix survived the
//! check and changed the open; a second decode round happened after the check;
//! an unparseable target was repaired into a different path than the one that
//! was validated; a separator was normalized on one platform and not another.
//!
//! Defending against that with a helper function inside the CLI would leave the
//! guard optional: any future handler could reach `std::fs` on its own and skip
//! it. So the dev server lives here instead, and the only way to obtain a file
//! is [`resolve_request`], which returns a [`ResolvedFile`] — an already-open
//! handle whose approved path is not exposed as a path type. There is no
//! "resolve without checking" entry point to forget to call.
//!
//! # Pipeline
//!
//! ```text
//! bytes ─▶ RequestHead::parse   only the method, target, Host and Origin survive
//!       ─▶ NetworkPolicy        Host and Origin may refuse; they never route
//!       ─▶ RequestTarget::parse origin-form or 400 — never a repaired target
//!       ─▶ resolve_with_policy  decode once ▸ normalize ▸ canonicalize
//!                               ▸ FsPolicy::decide ▸ open the checked path
//!       ─▶ Response             server-controlled bytes only
//! ```
//!
//! Each stage is a module: [`http`], [`network`], [`target`], [`resolve`],
//! [`policy`], and [`media`], with [`server`] holding the socket. Each module's
//! documentation names the failure it exists to prevent.
//!
//! # Defaults
//!
//! Loopback bind, no exposed hosts, no allowed origins, and a deny list
//! covering `.env*`, `**/.git/**`, `*.pem`, `*.key`, `*.crt` and `**/.uf/**`.
//! There is no `*` anywhere, and [`NetworkPolicy::new`] refuses one.
//!
//! ```no_run
//! use camino::Utf8Path;
//! use uf_devserver::{DevServer, DevConfig};
//!
//! let root = Utf8Path::new(".");
//! let server = DevServer::bind(root, &DevConfig::default())?;
//! server.write_state(root)?;
//! server.serve_forever()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [CVE-2025-30208]: https://github.com/advisories/GHSA-x574-m823-4x7w
//! [CVE-2025-31125]: https://nvd.nist.gov/vuln/detail/CVE-2025-31125
//! [CVE-2025-32395]: https://nvd.nist.gov/vuln/detail/CVE-2025-32395
//! [CVE-2025-62522]: https://nvd.nist.gov/vuln/detail/CVE-2025-62522

pub mod http;
pub mod media;
pub mod network;
pub mod policy;
pub mod resolve;
pub mod server;
pub mod target;

pub use uf_config::DevConfig;

pub use http::{
    HEALTH_TARGET, HttpError, MAX_HEADER_LINES, MAX_REQUEST_HEAD_BYTES, Method, RequestHead,
    Response, Status, respond, status_for, status_for_network,
};
pub use media::MediaType;
pub use network::{
    Exposure, LOOPBACK_HOSTS, MAX_ALLOWLIST_ENTRIES, MAX_AUTHORITY_BYTES, NetworkDenial,
    NetworkPolicy, NetworkPolicyError,
};
pub use policy::{
    DEFAULT_DENY, FsPolicy, MAX_ALLOW_ROOTS, MAX_DENY_PATTERNS, MAX_PATTERN_BYTES, PolicyDenial,
    PolicyError,
};
pub use resolve::{
    AccessDenied, CheckedPath, DIRECTORY_INDEX, FILESYSTEM_PREFIX, MAX_FILE_BYTES,
    MAX_PATH_SEGMENTS, ResolvedFile, resolve_request, resolve_with_policy,
};
pub use server::{
    CONNECTION_TIMEOUT, DevServer, DevServerError, DevServerState, ENGINE, PLUGIN_CONTRACT,
    STATE_DIR, STATE_FILE,
};
pub use target::{Loader, MAX_TARGET_BYTES, RequestTarget, TargetError};
