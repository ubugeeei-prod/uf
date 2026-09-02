//! Network exposure: which hosts and origins may talk to the dev server.
//!
//! # Threat model
//!
//! A dev server holds the developer's source tree, so reaching it from another
//! machine or another origin is the whole attack. Two mechanisms matter:
//!
//! * **DNS rebinding.** An attacker's page resolves `evil.test` to `127.0.0.1`
//!   and then talks to the dev server as same-origin. The defence is the `Host`
//!   header: a request arriving as `Host: evil.test` is not a request for this
//!   server, whatever the socket says, and is refused.
//! * **Cross-origin writes.** A page on another origin can always *send* a
//!   simple `GET`; what it must not do is anything with side effects. Non-simple
//!   methods therefore require an `Origin` on an explicit list.
//!
//! # No wildcards, ever
//!
//! There is no `*` default and no accepted `*` entry: [`NetworkPolicy::new`]
//! rejects the literal wildcard in either list. Binding a non-loopback address
//! is only possible with a non-empty [`allowed_hosts`](NetworkPolicy::allowed_hosts)
//! list — that is a construction-time error, not a warning, so an exposed
//! server with no allowlist cannot exist.
//!
//! # Headers are used to refuse, never to route
//!
//! `Host` and `Origin` are the only inbound headers this crate retains, and the
//! only thing either can do is turn an otherwise-served request into a refusal.
//! Neither selects a handler, a root, a loader, or a path. That is the
//! [CVE-2025-29927] bug class — a header (`x-middleware-subrequest`) that
//! *chose* a dispatch path and so could be spoofed into skipping authorization.
//!
//! [CVE-2025-29927]: https://nvd.nist.gov/vuln/detail/CVE-2025-29927

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

use crate::http::Method;

#[cfg(test)]
mod tests;

/// Host names accepted without configuration when bound to loopback.
pub const LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// Most entries either allowlist may hold.
pub const MAX_ALLOWLIST_ENTRIES: usize = 64;

/// Longest accepted `Host` or `Origin` header value, in bytes.
pub const MAX_AUTHORITY_BYTES: usize = 255;

/// Where the listening socket is reachable from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Exposure {
    /// Bound to a loopback address. [`LOOPBACK_HOSTS`] are implicitly allowed.
    #[default]
    Loopback,
    /// Bound to a routable address. Every acceptable host must be listed.
    Exposed,
}

/// Why a network policy could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NetworkPolicyError {
    /// A non-loopback bind was requested without an allowed-hosts list.
    #[error("exposing the dev server requires a non-empty dev.allowedHosts list")]
    ExposedWithoutAllowedHosts,
    /// An allowlist contained `*`.
    #[error("`*` is not an accepted {list} entry; list the hosts or origins explicitly")]
    Wildcard {
        /// Which list held it.
        list: &'static str,
    },
    /// An allowlist entry is empty or over [`MAX_AUTHORITY_BYTES`].
    #[error("{list} entry is empty or longer than {MAX_AUTHORITY_BYTES} bytes")]
    InvalidEntry {
        /// Which list held it.
        list: &'static str,
    },
    /// An allowlist has more than [`MAX_ALLOWLIST_ENTRIES`] entries.
    #[error("{list} may hold at most {MAX_ALLOWLIST_ENTRIES} entries, got {count}")]
    TooManyEntries {
        /// Which list held it.
        list: &'static str,
        /// How many were configured.
        count: usize,
    },
}

/// Why a request was refused before it reached the filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NetworkDenial {
    /// HTTP/1.1 requires a `Host`, and this server will not guess one.
    #[error("request has no Host header")]
    MissingHost,
    /// The `Host` value is not a plausible authority.
    #[error("Host header is not a valid authority")]
    MalformedHost,
    /// The `Host` name is not on the allowlist.
    #[error("host {host} is not in dev.allowedHosts")]
    HostNotAllowed {
        /// The rejected name, port removed.
        host: CompactString,
    },
    /// A non-simple request arrived without an `Origin`.
    #[error("a non-simple request must carry an Origin header")]
    MissingOrigin,
    /// The `Origin` is not on the allowlist.
    #[error("origin {origin} is not in dev.allowedOrigins")]
    OriginNotAllowed {
        /// The rejected origin.
        origin: CompactString,
    },
}

/// The host and origin allowlists for one dev server.
#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    exposure: Exposure,
    allowed_hosts: SmallVec<[CompactString; 4]>,
    allowed_origins: SmallVec<[CompactString; 4]>,
}

impl NetworkPolicy {
    /// Build a policy.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkPolicyError`] for an exposed bind with no allowed
    /// hosts, for a `*` entry, and for anything over the configured bounds.
    pub fn new<H, O>(
        exposure: Exposure,
        allowed_hosts: H,
        allowed_origins: O,
    ) -> Result<Self, NetworkPolicyError>
    where
        H: IntoIterator,
        H::Item: AsRef<str>,
        O: IntoIterator,
        O::Item: AsRef<str>,
    {
        let allowed_hosts = collect_list("dev.allowedHosts", allowed_hosts, normalize_host)?;
        let allowed_origins =
            collect_list("dev.allowedOrigins", allowed_origins, normalize_origin)?;
        if exposure == Exposure::Exposed && allowed_hosts.is_empty() {
            return Err(NetworkPolicyError::ExposedWithoutAllowedHosts);
        }
        Ok(Self {
            exposure,
            allowed_hosts,
            allowed_origins,
        })
    }

    /// A loopback policy with no extra hosts and no allowed origins.
    ///
    /// This is the default posture: reachable only as `localhost`, `127.0.0.1`
    /// or `[::1]`, and no cross-origin request with side effects is accepted.
    pub fn loopback() -> Self {
        Self {
            exposure: Exposure::Loopback,
            allowed_hosts: SmallVec::new(),
            allowed_origins: SmallVec::new(),
        }
    }

    /// Where the socket is reachable from.
    pub fn exposure(&self) -> Exposure {
        self.exposure
    }

    /// The configured host allowlist, without the implicit loopback names.
    pub fn allowed_hosts(&self) -> impl Iterator<Item = &str> {
        self.allowed_hosts.iter().map(CompactString::as_str)
    }

    /// The configured origin allowlist.
    pub fn allowed_origins(&self) -> impl Iterator<Item = &str> {
        self.allowed_origins.iter().map(CompactString::as_str)
    }

    /// Check a request's `Host` header.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkDenial`] when the header is absent, malformed, or names
    /// a host that is not allowed.
    pub fn check_host(&self, host: Option<&str>) -> Result<(), NetworkDenial> {
        let Some(host) = host else {
            return Err(NetworkDenial::MissingHost);
        };
        let name = host_name(host).ok_or(NetworkDenial::MalformedHost)?;
        let allowed = self
            .allowed_hosts
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(name))
            || (self.exposure == Exposure::Loopback
                && LOOPBACK_HOSTS
                    .iter()
                    .any(|entry| entry.eq_ignore_ascii_case(name)));
        if allowed {
            Ok(())
        } else {
            Err(NetworkDenial::HostNotAllowed {
                host: CompactString::new(name),
            })
        }
    }

    /// Check a request's `Origin` header.
    ///
    /// A simple `GET` or `HEAD` is always permitted: a browser can send one
    /// cross-origin no matter what the server says, and this server emits no
    /// `Access-Control-Allow-Origin`, so the response stays unreadable to the
    /// other origin. Everything else needs an allowlisted `Origin`.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkDenial`] when a non-simple request has no `Origin` or
    /// an `Origin` that is not allowed.
    pub fn check_origin(&self, method: Method, origin: Option<&str>) -> Result<(), NetworkDenial> {
        if method.is_simple() {
            return Ok(());
        }
        let Some(origin) = origin else {
            return Err(NetworkDenial::MissingOrigin);
        };
        let candidate = origin.trim_end_matches('/');
        if candidate.len() <= MAX_AUTHORITY_BYTES
            && self
                .allowed_origins
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(candidate))
        {
            Ok(())
        } else {
            Err(NetworkDenial::OriginNotAllowed {
                origin: CompactString::new(origin),
            })
        }
    }
}

fn collect_list<I>(
    list: &'static str,
    entries: I,
    normalize: fn(&str) -> Option<CompactString>,
) -> Result<SmallVec<[CompactString; 4]>, NetworkPolicyError>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut out: SmallVec<[CompactString; 4]> = SmallVec::new();
    for entry in entries {
        let entry = entry.as_ref().trim();
        if entry == "*" {
            return Err(NetworkPolicyError::Wildcard { list });
        }
        if entry.is_empty() || entry.len() > MAX_AUTHORITY_BYTES {
            return Err(NetworkPolicyError::InvalidEntry { list });
        }
        let value = normalize(entry).ok_or(NetworkPolicyError::InvalidEntry { list })?;
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() > MAX_ALLOWLIST_ENTRIES {
            return Err(NetworkPolicyError::TooManyEntries {
                list,
                count: out.len(),
            });
        }
    }
    Ok(out)
}

fn normalize_host(entry: &str) -> Option<CompactString> {
    host_name(entry).map(CompactString::new)
}

fn normalize_origin(entry: &str) -> Option<CompactString> {
    let trimmed = entry.trim_end_matches('/');
    // An origin is a scheme plus an authority. `null` and bare host names are
    // refused so a configuration typo cannot widen the allowlist.
    let (scheme, rest) = trimmed.split_once("://")?;
    if scheme.is_empty() || rest.is_empty() || rest.contains('/') {
        return None;
    }
    host_name(rest)?;
    Some(CompactString::new(trimmed))
}

/// Strip the port from an authority and validate what is left.
///
/// Returns `None` for anything that is not a plausible host, so a `Host` header
/// carrying a path, userinfo, or whitespace is refused rather than truncated
/// into something that happens to match.
fn host_name(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.len() > MAX_AUTHORITY_BYTES {
        return None;
    }
    let (name, port) = if let Some(rest) = authority.strip_prefix('[') {
        let close = rest.find(']')?;
        let inner = &rest[..close];
        if inner.is_empty()
            || !inner
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b':' || byte == b'.')
        {
            return None;
        }
        (&authority[..close + 2], &rest[close + 1..])
    } else {
        match authority.split_once(':') {
            Some((name, port)) => (name, port),
            None => (authority, ""),
        }
    };
    if !port.is_empty() {
        let digits = port.strip_prefix(':').unwrap_or(port);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    if name.is_empty() {
        return None;
    }
    // A bracketed literal was already validated character by character above.
    let valid = name.starts_with('[')
        || name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
    valid.then_some(name)
}
