//! A private name answered by a registry it was never bound to.
//!
//! # The attack
//!
//! `@company/internal-thing` exists on the company registry and nowhere else,
//! until somebody publishes a package of that name to the public one. A
//! resolver that asks the public registry — first, or as a fallback, or as the
//! only registry it knows — installs the attacker's copy, on a developer laptop
//! or a CI runner, with whatever that copy's install scripts do. It has hit
//! Apple, Microsoft, PayPal and Netflix. It requires compromising nothing: the
//! attacker publishes under a name they are allowed to publish under.
//!
//! [`crate::registry::RegistryRouting`] is the half of the answer that governs
//! the reads uf makes itself: a bound scope is asked of one registry and never
//! of another. This module is the other half, and it is the one that matters
//! more, because `uf install` **delegates**: npm, pnpm, yarn or bun does the
//! resolving, from whatever its own `.npmrc` says, and uf finds out afterwards.
//!
//! So uf reads the answer. A lockfile records the URL every package was
//! resolved from, and a bound scope resolved from anywhere but its bound
//! registry is this attack having already happened — or about to, on the next
//! `npm ci` of that lockfile. Either way it is a refusal, not a warning: a
//! warning on this one is a line in a CI log above a successful install.
//!
//! # Why the check is on the origin and its path prefix
//!
//! A registry's tarball URLs sit under the registry URL: npmjs serves
//! `https://registry.npmjs.org/react/-/react-18.2.0.tgz`, an Artifactory
//! repository serves `https://artifactory.example/api/npm/npm-local/…`. So a
//! resolved URL belongs to a registry when it has the same scheme, host and
//! port, *and* begins with whatever path the registry URL has.
//!
//! Comparing whole URLs would be wrong — the tarball path is the registry's to
//! choose. Comparing hosts alone would be too weak for the one case where two
//! registries share a host and differ by path, which is how GitLab and
//! Artifactory give a company more than one repository.
//!
//! # What it does not prove
//!
//! That the bound registry is honest, or that the tarball at that URL is the
//! one the lockfile's integrity hash covers — the hash is what answers the
//! second, and [`crate::provenance`] is what says who built it. This proves
//! one thing: every package in a bound scope came from the registry the
//! project said that scope lives on.

use compact_str::{CompactString, ToCompactString};
use thiserror::Error;

use crate::delta::LockfileSnapshot;
use crate::registry::{RegistryRouting, scope_of};

/// One package the lockfile resolves from a registry its scope is not bound to.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Confusion {
    /// Answered by a registry that is not the one this scope is bound to.
    #[error(
        "`{name}` is in the scope `{scope}`, which uf.config.js binds to {bound}, \
         but {lockfile} resolves it from {answered}"
    )]
    WrongRegistry {
        /// The package.
        name: CompactString,
        /// Its scope, which is the bound one.
        scope: CompactString,
        /// The registry `pm.scopes` names.
        bound: CompactString,
        /// The registry the lockfile says actually answered.
        answered: CompactString,
        /// The lockfile this was read from, for the message.
        lockfile: CompactString,
    },
    /// In a bound scope, and the lockfile does not say where it came from.
    ///
    /// Refused rather than assumed: a lockfile entry with no `resolved` is one
    /// uf cannot check, and "cannot check" is not "checked".
    #[error(
        "`{name}` is in the scope `{scope}`, which uf.config.js binds to {bound}, \
         and {lockfile} does not record where it was resolved from"
    )]
    Unrecorded {
        /// The package.
        name: CompactString,
        /// Its scope.
        scope: CompactString,
        /// The registry `pm.scopes` names.
        bound: CompactString,
        /// The lockfile this was read from.
        lockfile: CompactString,
    },
}

impl Confusion {
    /// The package this is about.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::WrongRegistry { name, .. } | Self::Unrecorded { name, .. } => name,
        }
    }
}

/// The sentence that follows a refusal, telling the reader what to do.
///
/// uf refuses; it does not silently rewrite somebody's lockfile or their
/// `.npmrc`. The manager that resolved this is the manager that has to be told,
/// and it is told in its own configuration — the same principle as
/// [`crate::builds`], where an approval is recorded in the field the project's
/// own manager already reads.
pub const REMEDY: &str = "uf will not install this. Bind the scope in the manager's own \
                          configuration too — `@scope:registry=…` in .npmrc — then delete the \
                          lockfile entry and resolve it again. If the package really does live \
                          on the registry that answered, that is what pm.scopes should say.";

/// Every package in a bound scope that a lockfile resolves from somewhere else.
///
/// An empty answer is the good one. A snapshot uf could not read as a tree —
/// pnpm's YAML, yarn's own syntax, bun's binary format — yields nothing, which
/// is honest rather than reassuring: see [`LockfileSnapshot::detailed`].
#[must_use]
pub fn check(routing: &RegistryRouting, snapshot: &LockfileSnapshot) -> Vec<Confusion> {
    if !routing.has_bindings() || !snapshot.detailed {
        return Vec::new();
    }
    let lockfile = snapshot
        .path
        .file_name()
        .unwrap_or("the lockfile")
        .to_compact_string();
    let mut found = Vec::new();
    for entry in snapshot.entries.values() {
        // A workspace package came from this repository, not from a registry.
        if entry.link {
            continue;
        }
        let Some(scope) = scope_of(&entry.name) else {
            continue;
        };
        let route = routing.route(&entry.name);
        if !route.is_bound() {
            continue;
        }
        let bound = route.registry;
        match entry.resolved.as_deref() {
            None => found.push(Confusion::Unrecorded {
                name: entry.name.clone(),
                scope: scope.to_compact_string(),
                bound: bound.to_compact_string(),
                lockfile: lockfile.clone(),
            }),
            Some(resolved) if !serves(bound, resolved) => found.push(Confusion::WrongRegistry {
                name: entry.name.clone(),
                scope: scope.to_compact_string(),
                bound: bound.to_compact_string(),
                answered: origin_of(resolved).unwrap_or(resolved).to_compact_string(),
                lockfile: lockfile.clone(),
            }),
            Some(_) => {}
        }
    }
    found.sort_by(|left, right| left.name().cmp(right.name()));
    found.dedup();
    found
}

/// Whether `registry` is the registry `resolved` came from.
///
/// Same scheme, host and port, and the registry's own path is a prefix of the
/// resolved one *at a path boundary* — so `https://host/npm-local` does not
/// claim `https://host/npm-local-public/…`, which is a different repository
/// with a name that happens to start the same way.
fn serves(registry: &str, resolved: &str) -> bool {
    let (Some(registry_origin), Some(resolved_origin)) = (origin_of(registry), origin_of(resolved))
    else {
        return false;
    };
    // Case-insensitive, because a host is: `Registry.NPMJS.org` and
    // `registry.npmjs.org` are one host, and a check that said otherwise would
    // refuse a project for its capitalisation.
    if !registry_origin.eq_ignore_ascii_case(resolved_origin) {
        return false;
    }
    let registry_path = registry[registry_origin.len()..].trim_end_matches('/');
    if registry_path.is_empty() {
        return true;
    }
    let resolved_path = &resolved[resolved_origin.len()..];
    if climbs_out(resolved_path) {
        return false;
    }
    resolved_path
        .strip_prefix(registry_path)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether a URL path walks back out of the prefix it begins with.
///
/// `https://host/npm-local/../public/evil.tgz` starts with `/npm-local` and is
/// served by the repository next door, so a plain prefix test would call it
/// bound. Where two registries share a host and differ only by path — which is
/// how GitLab and Artifactory give one company more than one repository — that
/// path boundary is the entire separation, and a `..` segment is a lockfile
/// entry written to step through it.
///
/// `%2e` is decoded first, because a server that normalises the path does too
/// and uf must not disagree with it about what the URL means. No manager writes
/// either spelling, so refusing both costs an honest project nothing.
fn climbs_out(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment.to_ascii_lowercase().replace("%2e", ".") == "..")
}

/// `scheme://host[:port]`, or `None` for something that is not one.
///
/// A hand-written split rather than a URL parser: uf needs the authority and
/// nothing else, and the two inputs are a config value and a lockfile string
/// that are both untrusted. Anything that does not have `://` and a non-empty
/// authority is not a URL this can compare, and saying so is the safe answer —
/// [`serves`] treats it as "not from this registry".
fn origin_of(url: &str) -> Option<&str> {
    let separator = url.find("://")?;
    let scheme = &url[..separator];
    if scheme.is_empty() || !scheme.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    let after = separator + "://".len();
    let authority_end = url[after..]
        .find('/')
        .map_or(url.len(), |offset| after + offset);
    if authority_end == after {
        return None;
    }
    Some(&url[..authority_end])
}

#[cfg(test)]
mod tests;
