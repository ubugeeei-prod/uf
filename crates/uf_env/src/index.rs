//! What each publisher has released, and the newest release a prefix names.
//!
//! # Why this exists
//!
//! `runtime: "node@26"` names the newest 26.x. Turning that into `26.8.2`
//! needs the list of releases somebody published, and the answer is locked in
//! `uf.lock` so it is asked once rather than on every command. This module is
//! the list: where each tool's comes from, how it is read, [`resolve`] over it,
//! and a cache of it on disk that an editor can read without waiting on a
//! network. See ubugeeei-prod/uf#940.
//!
//! # Which list, per tool
//!
//! One list per tool, chosen for being complete where a project will look and
//! answered by a host that does not ration it:
//!
//! | Tool | List | Why |
//! | --- | --- | --- |
//! | Node.js | `nodejs.org/dist/index.json` | What nodejs.org builds its own download pages from: every release, its date and its LTS line, in one request. |
//! | Bun | the npm registry's `bun` packument | Oven publishes each release to npm alongside the GitHub release. GitHub's releases API is the other complete list, and it answers sixty unauthenticated requests an hour per address — which a CI runner behind a shared address can spend before its first build. |
//! | Deno | the npm registry's `deno` packument | The same reason, with one limit: npm has Deno from 1.46 on. A prefix older than that finds nothing here, and the error says so; an exact `deno@1.40.0` never reads an index at all. |
//! | npm, pnpm | their own packuments | They are npm packages, so the registry *is* the publisher. |
//! | Yarn | `yarn` and `@yarnpkg/cli-dist` | Yarn 1 is the `yarn` package, and every later major is published as `@yarnpkg/cli-dist`. Either list alone is half of Yarn. |
//!
//! The packument is the abbreviated one — `Accept:
//! application/vnd.npm.install-v1+json`, what npm itself asks for — because a
//! version list is all this needs, and the full document for `npm` is tens of
//! megabytes of READMEs.
//!
//! # The cache is read without the network, and refreshed on purpose
//!
//! Completion in `uf.config.js` offers versions while somebody types, and a
//! completion that waits on a registry arrives after the keystroke it was for.
//! So reading the cache — [`cached`] — never starts a process and never opens a
//! socket: it reads one file or answers `None`. Fetching — [`refresh`] — is a
//! separate call, made by something that is allowed to take a second.
//!
//! The file is `$XDG_CACHE_HOME/uf/index/<tool>.json`, written whole and renamed
//! into place so a reader never sees half of one, with the time it was fetched
//! in it so the reader can decide for itself whether it is too old to offer.
//!
//! # Why the hosts are overridable
//!
//! `UF_TOOL_INDEX_BASE` replaces both hosts — `https://nodejs.org/dist` and
//! `https://registry.npmjs.org` — so a test serves fixtures over `file://` and
//! never touches the network. It is the same shape as `UF_TOOL_BASE` in
//! [`crate::source`], for the same reason: a list that can only be read from
//! the real internet is a reader that is not tested. Anything but `https://`
//! and `file://` is refused by curl itself, redirects included.

use std::cmp::Ordering;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::EnvError;
use crate::store::{cache_home, env_path};
use crate::tool::Tool;

/// The version of the cache file's own shape.
///
/// A file in any other shape is not an error, it is a cache miss: the next
/// refresh replaces it.
pub const FORMAT: u32 = 1;

/// The environment variable that replaces both publishers' hosts.
pub const BASE_VARIABLE: &str = "UF_TOOL_INDEX_BASE";

/// The environment variable that puts the cache somewhere else.
pub const CACHE_VARIABLE: &str = "UF_INDEX_CACHE";

/// How long one list may take to arrive.
const TIMEOUT_SECONDS: &str = "30";

/// The biggest list uf will read, as curl's `--max-filesize` argument.
///
/// The abbreviated packuments for `pnpm` and `npm` are about two megabytes, and
/// Node's index a third of one; this is the bound past which curl stops rather
/// than uf holding whatever a host chose to send.
const MAX_BYTES: &str = "33554432";

/// One tool's release list, as the cache holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    /// The shape this file was written in; see [`FORMAT`].
    pub format: u32,
    /// Which tool.
    pub tool: Tool,
    /// When it was fetched, in seconds since the Unix epoch.
    pub fetched_at: u64,
    /// The lists it was read from, in the order they were read.
    pub sources: Vec<String>,
    /// Every release the lists name, newest first, prereleases included.
    pub releases: Vec<Release>,
}

impl Index {
    /// How long ago it was fetched, or `None` when the clock is behind the
    /// file — which is a machine to distrust rather than a list to throw away.
    #[must_use]
    pub fn age(&self) -> Option<Duration> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        now.checked_sub(self.fetched_at).map(Duration::from_secs)
    }

    /// The newest release `prefix` names; see [`resolve`].
    #[must_use]
    pub fn resolve(&self, prefix: &str) -> Option<&Release> {
        resolve(prefix, &self.releases)
    }
}

/// One published release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// The version, without the `v` Node puts in front of it.
    pub version: String,
    /// The day it was published, as `YYYY-MM-DD`, when the list says.
    ///
    /// Node's index does; an abbreviated packument does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The LTS line it belongs to — Node's codename for it — when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lts: Option<String>,
}

impl Release {
    /// A release with nothing but a version, which is what a packument knows.
    #[must_use]
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            date: None,
            lts: None,
        }
    }

    /// Whether it is a prerelease: `1.4.2-canary.20260913.1`, `27.0.0-rc.1`.
    ///
    /// Build metadata after a `+` is not a prerelease — `4.9.2+sha.1` is a
    /// release that says which commit it came from.
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        let version = self.version.split('+').next().unwrap_or_default();
        version.contains('-')
    }

    /// `major.minor.patch`, when the version has one.
    ///
    /// `None` for anything that is not one exact release, build metadata
    /// included. The numbers before a `+` can be perfectly good while what
    /// follows it carries a `/`, and a release this answers for becomes a
    /// directory name in the store and part of a download URL.
    fn core(&self) -> Option<[u64; 3]> {
        if !crate::project::is_exact_version(&self.version) {
            return None;
        }
        let core = self.version.split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let core = [
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ];
        parts.next().is_none().then_some(core)
    }
}

/// The newest release that `prefix` names.
///
/// `prefix` is one to three numbers — `26`, `1.4`, `24.14.0` — and matches
/// component by component, so `1.4` is 1.4.x and never 1.40.0, and `2` is 2.x
/// and never 26.0.0. Prereleases are never the answer: `node@26` is the newest
/// 26 that was released, not a candidate for one.
///
/// Pure, over a list the caller already has, so a locked answer can be checked
/// against a cached list without fetching anything.
#[must_use]
pub fn resolve<'a>(prefix: &str, releases: &'a [Release]) -> Option<&'a Release> {
    let wanted: Vec<u64> = prefix
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if wanted.is_empty() || wanted.len() > 3 {
        return None;
    }
    releases
        .iter()
        .filter(|release| !release.is_prerelease())
        .filter_map(|release| release.core().map(|core| (core, release)))
        .filter(|(core, _)| core[..wanted.len()] == wanted[..])
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, release)| release)
}

/// Order two versions the way semver does, newest last.
///
/// A release outranks its own prereleases, and prerelease identifiers compare
/// numerically when both are numbers — `canary.9` before `canary.10` — which a
/// string comparison gets wrong for every list that has ten of anything.
fn compare(left: &Release, right: &Release) -> Ordering {
    match (left.core(), right.core()) {
        (Some(a), Some(b)) if a != b => return a.cmp(&b),
        (Some(_), None) => return Ordering::Greater,
        (None, Some(_)) => return Ordering::Less,
        (None, None) => return left.version.cmp(&right.version),
        _ => {}
    }
    let prerelease = |release: &Release| -> Option<String> {
        let version = release.version.split('+').next()?;
        version.split_once('-').map(|(_, pre)| pre.to_owned())
    };
    match (prerelease(left), prerelease(right)) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => compare_prerelease(&a, &b),
    }
}

fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let mut left = left.split('.');
    let mut right = right.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(a), Ok(b)) => a.cmp(&b),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => a.cmp(b),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Newest first, one entry per version.
fn sorted(mut releases: Vec<Release>) -> Vec<Release> {
    releases.sort_by(|left, right| compare(right, left));
    releases.dedup_by(|later, earlier| later.version == earlier.version);
    releases
}

/// Read nodejs.org's `dist/index.json`.
///
/// `None` when it is not a list of releases, so a captive portal's login page
/// or an HTML error from a mirror is a refusal rather than an empty index that
/// would resolve nothing.
#[must_use]
pub fn parse_node_index(body: &str) -> Option<Vec<Release>> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(body).ok()?;
    let mut releases = Vec::with_capacity(rows.len());
    for row in rows {
        let version = row.get("version")?.as_str()?;
        let version = version.strip_prefix('v').unwrap_or(version);
        // Dropped rather than kept: an entry that is not one release is never
        // an answer, and a cache that held one would offer it to completion.
        if !crate::project::is_exact_version(version) {
            continue;
        }
        releases.push(Release {
            version: version.to_owned(),
            date: row
                .get("date")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            // `false` for a release that is not on an LTS line, the line's
            // codename for one that is.
            lts: row
                .get("lts")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        });
    }
    Some(sorted(releases))
}

/// Read the version list out of an npm packument.
///
/// `None` when it has no `versions` object, for the reason
/// [`parse_node_index`] gives.
#[must_use]
pub fn parse_packument(body: &str) -> Option<Vec<Release>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let versions = value.get("versions")?.as_object()?;
    Some(sorted(
        versions
            .keys()
            .filter(|version| crate::project::is_exact_version(version))
            .map(Release::new)
            .collect(),
    ))
}

/// Where the lists come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    /// Where Node's `index.json` is: `https://nodejs.org/dist`.
    pub nodejs: String,
    /// Where packuments are: `https://registry.npmjs.org`.
    pub registry: String,
}

impl Default for Bases {
    fn default() -> Self {
        Self {
            nodejs: "https://nodejs.org/dist".to_owned(),
            registry: "https://registry.npmjs.org".to_owned(),
        }
    }
}

impl Bases {
    /// The publishers, or `$UF_TOOL_INDEX_BASE` for both.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var(BASE_VARIABLE) {
            Ok(base) if !base.trim().is_empty() => Self::at(&base),
            _ => Self::default(),
        }
    }

    /// Both lists under one base, which is what a fixture directory is.
    #[must_use]
    pub fn at(base: &str) -> Self {
        let base = base.trim().trim_end_matches('/').to_owned();
        Self {
            nodejs: base.clone(),
            registry: base,
        }
    }

    /// The lists `tool`'s releases are read from, and what to ask each for.
    fn lists(&self, tool: Tool) -> Vec<(String, &'static str)> {
        const JSON: &str = "application/json";
        const PACKUMENT: &str = "application/vnd.npm.install-v1+json";
        let packument = |name: &str| {
            (
                uf_infra::into_string(compact_str::format_compact!("{}/{name}", self.registry)),
                PACKUMENT,
            )
        };
        match tool {
            Tool::Node => vec![(
                uf_infra::into_string(compact_str::format_compact!("{}/index.json", self.nodejs)),
                JSON,
            )],
            Tool::Bun => vec![packument("bun")],
            Tool::Deno => vec![packument("deno")],
            Tool::Npm => vec![packument("npm")],
            Tool::Pnpm => vec![packument("pnpm")],
            Tool::Yarn => vec![packument("yarn"), packument("@yarnpkg/cli-dist")],
        }
    }
}

/// Where indexes are cached: `$UF_INDEX_CACHE`, or
/// `$XDG_CACHE_HOME/uf/index`, or `$HOME/.cache/uf/index`.
///
/// # Errors
///
/// When none of the three is set, because then there is nowhere this could
/// mean.
pub fn cache_dir() -> Result<Utf8PathBuf, EnvError> {
    if let Some(explicit) = env_path(CACHE_VARIABLE) {
        return Ok(explicit);
    }
    Ok(cache_home()?.join("uf").join("index"))
}

/// The cached list for `tool`, read without touching the network.
///
/// `None` when there is no cache, when it cannot be read, or when it was
/// written in another [`FORMAT`] — each of which a caller answers the same way,
/// by offering nothing or by calling [`refresh`].
#[must_use]
pub fn cached(tool: Tool) -> Option<Index> {
    cached_in(&cache_dir().ok()?, tool)
}

/// [`cached`], from a cache directory the caller names.
#[must_use]
pub fn cached_in(dir: &Utf8Path, tool: Tool) -> Option<Index> {
    let body = fs::read_to_string(cache_path(dir, tool)).ok()?;
    let index: Index = serde_json::from_str(&body).ok()?;
    (index.format == FORMAT && index.tool == tool).then_some(index)
}

/// Fetch `tool`'s list from its publisher, cache it, and return it.
///
/// # Errors
///
/// When `curl` is missing or a list cannot be fetched, when what arrives is
/// not a release list, or when the cache cannot be written. A failed refresh
/// leaves the previous cache exactly where it was.
pub fn refresh(tool: Tool) -> Result<Index, EnvError> {
    refresh_in(&cache_dir()?, tool, &Bases::from_env())
}

/// [`refresh`], from the lists at `bases` into the cache at `dir`.
///
/// # Errors
///
/// As [`refresh`].
pub fn refresh_in(dir: &Utf8Path, tool: Tool, bases: &Bases) -> Result<Index, EnvError> {
    let mut releases = Vec::new();
    let mut sources = Vec::new();
    for (url, accept) in bases.lists(tool) {
        let body = fetch(&url, accept)?;
        let parsed = match tool {
            Tool::Node => parse_node_index(&body),
            _ => parse_packument(&body),
        };
        releases.extend(parsed.ok_or_else(|| EnvError::MalformedIndex { url: url.clone() })?);
        sources.push(url);
    }
    let index = Index {
        format: FORMAT,
        tool,
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
        sources,
        releases: sorted(releases),
    };
    write_cache(dir, &index)?;
    Ok(index)
}

/// `<dir>/<tool>.json`.
fn cache_path(dir: &Utf8Path, tool: Tool) -> Utf8PathBuf {
    dir.join(uf_infra::into_string(compact_str::format_compact!(
        "{}.json",
        tool.name()
    )))
}

/// Write the cache under a temporary name and rename it into place.
///
/// A completion reading the file while a refresh writes it sees the old list or
/// the new one, never the first half of the new one.
fn write_cache(dir: &Utf8Path, index: &Index) -> Result<(), EnvError> {
    fs::create_dir_all(dir).map_err(|source| EnvError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = cache_path(dir, index.tool);
    let staging = dir.join(uf_infra::into_string(compact_str::format_compact!(
        ".{}.json.{}",
        index.tool.name(),
        std::process::id()
    )));
    let body = serde_json::to_vec(index).map_err(EnvError::Encode)?;
    fs::write(&staging, body).map_err(|source| EnvError::Write {
        path: staging.clone(),
        source,
    })?;
    fs::rename(&staging, &path).map_err(|source| {
        let _ = fs::remove_file(&staging);
        EnvError::Write { path, source }
    })
}

/// One list, as text.
///
/// Through `curl`, like every other fetch uf makes, and held to `https://` —
/// or `file://`, which is what a fixture is — with redirects that may not
/// leave TLS, the same rule `uf_pm::registry` keeps for packuments.
fn fetch(url: &str, accept: &str) -> Result<String, EnvError> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--retry",
            "1",
            "--max-time",
            TIMEOUT_SECONDS,
            "--max-filesize",
            MAX_BYTES,
            "--proto",
            "=https,file",
            "--proto-redir",
            "=https",
            "-H",
        ])
        .arg(uf_infra::into_string(compact_str::format_compact!(
            "Accept: {accept}"
        )))
        .arg("--")
        .arg(url)
        .output()
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => EnvError::MissingProgram { program: "curl" },
            _ => EnvError::Program {
                program: "curl",
                source,
            },
        })?;
    if !output.status.success() {
        return Err(EnvError::Download {
            url: url.to_owned(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| EnvError::MalformedIndex {
        url: url.to_owned(),
    })
}

#[cfg(test)]
mod tests;
