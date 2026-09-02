//! The update payload, and the single door it lets the browser fetch through.
//!
//! # The one rule
//!
//! An update tells the browser which modules to re-fetch. That is an
//! access-control surface: if the payload could name a path the request
//! pipeline would refuse, hot module replacement would be a way around the
//! pipeline. It is not, for two reasons that are both structural.
//!
//! First, the payload never carries a filesystem path. It carries an
//! *origin-form request target*, built by [`update_target`] from a module path
//! the graph has already normalized, percent-encoded to the same grammar
//! [`crate::target::RequestTarget`] accepts, and refused outright when the
//! module path cannot be spelled as one.
//!
//! Second, fetching is [`fetch_update`], which is [`resolve_with_policy`] with
//! a target parse in front of it. It is the same pipeline the plain `GET` path
//! runs — the same decode-once, the same normalization, the same deny list, the
//! same canonicalization, the same open. There is no second way to reach a
//! file in this crate, and `an_hmr_fetch_is_refused_exactly_like_a_plain_request`
//! in `tests/attack_corpus.rs` asserts the two agree byte for byte.

use camino::Utf8Path;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::policy::FsPolicy;
use crate::resolve::{AccessDenied, ResolvedFile, resolve_with_policy};
use crate::target::RequestTarget;

use super::invalidate::{ChangeKind, ReloadReason, UpdateKind};

/// Most modules one update payload will name.
///
/// An update that touches more of the project than this is not an update any
/// more; the browser is told to reload instead of being handed a list it would
/// fetch for several seconds.
pub const MAX_UPDATE_MODULES: usize = 256;

/// Longest request target an update will build, in bytes.
///
/// Below [`crate::target::MAX_TARGET_BYTES`] on purpose: a target this crate
/// generates should never be near the limit the parser enforces.
pub const MAX_UPDATE_TARGET_BYTES: usize = 2_048;

/// Why a module is in an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateRole {
    /// The module accepts the update: React re-renders it in place, and its
    /// importers keep the bindings they already hold.
    Boundary,
    /// The module changed underneath a boundary and has to be re-evaluated
    /// before that boundary re-renders.
    Dependency,
}

impl UpdateRole {
    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boundary => "boundary",
            Self::Dependency => "dependency",
        }
    }

    /// Sort key that puts dependencies before the boundaries that import them.
    ///
    /// A client applying the list in order therefore evaluates a changed helper
    /// before re-rendering the component that reads it.
    pub const fn apply_order(self) -> u8 {
        match self {
            Self::Dependency => 0,
            Self::Boundary => 1,
        }
    }
}

/// One module the browser has to re-fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModule {
    /// Project-relative module path, for display and for the client's registry
    /// key. Never used as a filesystem path.
    pub path: CompactString,
    /// The origin-form request target to re-fetch the module from.
    pub url: CompactString,
    /// Why the module is listed.
    pub role: UpdateRole,
}

/// One hot-module-replacement event.
///
/// Serialized as the `data:` field of a server-sent event. Every field is
/// server-derived: nothing a client sent is echoed back, which is what keeps
/// the update channel outside [CVE-2025-29927]'s bug class.
///
/// [CVE-2025-29927]: https://nvd.nist.gov/vuln/detail/CVE-2025-29927
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HmrUpdate {
    /// Monotonic event identifier, assigned by the channel when it publishes.
    pub id: u64,
    /// The file that changed, project-relative.
    pub path: CompactString,
    /// What happened to it.
    pub change: ChangeKind,
    /// What the browser and the server must do.
    pub kind: UpdateKind,
    /// Why a full reload is required, when one is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ReloadReason>,
    /// Client modules to re-fetch, boundaries last so a client that applies
    /// them in order evaluates dependencies first.
    pub modules: Vec<UpdateModule>,
    /// Server modules whose rendered output is stale. Names only; the browser
    /// re-requests the route rather than fetching these.
    pub routes: Vec<CompactString>,
    /// How long computing the update took, in microseconds.
    pub elapsed_micros: u64,
}

impl HmrUpdate {
    /// Whether the browser has nothing to do.
    pub fn is_inert(&self) -> bool {
        self.kind.is_inert()
    }

    /// How many modules the browser has to re-fetch.
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
}

/// Build the origin-form request target that re-fetches `path`.
///
/// Returns `None` when the module path cannot be spelled as an origin-form
/// target — an empty path, a path whose encoding would exceed
/// [`MAX_UPDATE_TARGET_BYTES`], or a path that already carries a percent
/// escape, which [`crate::resolve`] refuses as double-encoding. A caller that
/// gets `None` must fall back to [`ReloadReason::Unservable`] rather than
/// inventing a target.
///
/// `revision` becomes the `t` query key, which [`crate::target`] recognizes and
/// treats as inert: it busts the browser's module cache without selecting a
/// loader.
pub fn update_target(path: &Utf8Path, revision: u32) -> Option<CompactString> {
    let text = path.as_str();
    if text.is_empty() || text.starts_with('/') {
        return None;
    }
    // Belt and braces. The graph normalizes before it stores a module path, so
    // a `.` or `..` segment cannot get this far — but a builder that would
    // happily emit `/../.env?t=1` is a builder one refactor away from being the
    // bug this crate exists to prevent. The target names exactly one module
    // path or it is not built.
    if text
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }

    let mut out = String::with_capacity(text.len() + 16);
    out.push('/');
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char);
            }
            // A literal `%` would decode into something the resolver reads as a
            // second layer of encoding, so a file whose name contains one has no
            // update target at all. `crate::resolve` makes the same trade for
            // the plain request path.
            b'%' => return None,
            other => {
                out.push('%');
                out.push(HEX[usize::from(other >> 4)] as char);
                out.push(HEX[usize::from(other & 0x0f)] as char);
            }
        }
    }
    out.push_str("?t=");
    uf_term::push_u32(&mut out, revision);

    if out.len() > MAX_UPDATE_TARGET_BYTES {
        return None;
    }
    // Built, then checked against the same grammar an inbound target is checked
    // against. A target this crate cannot parse is a target this crate will not
    // hand to a browser.
    RequestTarget::parse(&out).ok()?;
    Some(CompactString::new(out))
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Fetch the bytes of a module named by an update.
///
/// This is the plain request pipeline and nothing else: parse the target under
/// the origin-form grammar, then [`resolve_with_policy`]. It exists as a named
/// function so the update path has somewhere to point, not because it does
/// anything different — an update that names `/../../.env` gets exactly the
/// refusal a browser typing `/../../.env` into the address bar gets.
///
/// # Errors
///
/// Returns [`AccessDenied`] for every rejection, identically to
/// [`crate::resolve::resolve_request`].
pub fn fetch_update(policy: &FsPolicy, target: &str) -> Result<ResolvedFile, AccessDenied> {
    let parsed = RequestTarget::parse(target)?;
    resolve_with_policy(policy, &parsed)
}

#[cfg(test)]
mod tests;
