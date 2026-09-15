//! What a registry has published, which is the one fact `uf update` cannot get
//! from the project.
//!
//! # Why uf reads this at all
//!
//! Everywhere else, uf asks the project's own package manager and stays out of
//! the way — `docs/red-lines.md` is emphatic about it, and `uf add` delegates
//! for a reason that matters: installing is when a `postinstall` script
//! arrives, so the tool that resolves must be the tool the project chose.
//!
//! Reading a packument is not installing. Nothing is fetched, nothing is
//! unpacked, nothing is executed; uf asks the same registry the project's
//! manager would, over the same URL, and gets back a list of version numbers.
//! The install that follows a `uf update --latest` is still the manager's.
//!
//! And no manager will answer this question. `npm outdated` reports what is
//! outdated *within the declared range* on npm, in one JSON shape; `pnpm
//! outdated` in another; `yarn npm outdated` in a third; `bun outdated` in none
//! at all, having no JSON output. A command that meant different things under
//! four managers would be worse than no command.
//!
//! # The request
//!
//! `Accept: application/vnd.npm.install-v1+json` — the abbreviated packument,
//! which is what npm itself asks for. It carries `dist-tags` and the version
//! keys and drops the READMEs, which on a package like `react` is the
//! difference between tens of kilobytes and several megabytes.
//!
//! Through `curl`, like every other fetch uf makes (`uf_env::archive`), and for
//! the same reason: it is already a requirement of the installer, it is on
//! every machine that can install uf, and it means uf links no TLS stack.
//!
//! # HTTPS, and nothing else
//!
//! A registry URL can carry userinfo, and curl sends it. Over `http://` that is
//! a credential on the wire; through a redirect out of TLS it is the same thing
//! one hop later. So `url_for` refuses anything but `https://`, refuses a URL
//! with an authority at all, and curl is given `--proto =https --proto-redir
//! =https` so neither decision can be undone by a `301`. A project on a
//! plain-http mirror gets a named refusal beside the packages uf could not read
//! — which is the honest outcome, and the one a report that quietly leaked a
//! token would not be.
//!
//! # Which registry, for which name
//!
//! [`RegistryRouting`] is the answer, and the reason it exists is dependency
//! confusion: publish `@company/internal-thing` to the public registry and a
//! resolver that asks the public registry — first, or only — installs the
//! attacker's copy. It has hit Apple, Microsoft, PayPal and Netflix, and it
//! needs no compromise of anything.
//!
//! So a scope bound in `pm.scopes` is resolved from that registry **and
//! nowhere else**. There is deliberately no fallback to the default registry
//! when the bound one does not have the name, because the fallback *is* the
//! vulnerability: it is the step that turns "the company registry has never
//! heard of this" into "so let us try the one anybody can publish to". A name
//! the bound registry does not have is [`RegistryError::NotOnBoundRegistry`],
//! which names the scope and the registry it asked.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use compact_str::{CompactString, ToCompactString};
use serde_json::Value;
use thiserror::Error;
use uf_config::UniflowedConfig;

use crate::detect::Version;

/// How long one packument request may take.
///
/// A registry that has not answered in this long is a registry that is down,
/// and `uf update` has already run the manager's own update by the time it
/// asks: waiting longer would be holding a finished command open.
const TIMEOUT_SECONDS: &str = "20";

/// How many requests are in flight at once.
///
/// Enough to make a fifty-dependency project fast, few enough to be a polite
/// client of a registry that is answering everyone else too.
pub(crate) const CONCURRENCY: usize = 8;

/// The biggest packument uf will parse.
///
/// An abbreviated packument for the largest packages on npm is a few megabytes;
/// this is the bound past which uf stops reading rather than growing a buffer
/// on whatever a registry chose to send.
pub const MAX_PACKUMENT_BYTES: usize = 32 * 1024 * 1024;

/// The same bound, as curl's `--max-filesize` argument.
///
/// Written out rather than formatted, because it is an argv entry and a
/// `const fn` that produced one would be more machinery than a number.
const MAX_PACKUMENT_BYTES_TEXT: &str = "33554432";

/// What one package has published.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Packument {
    /// Every published version that parses as one, unordered.
    pub versions: Vec<Version>,
    /// What the registry's `latest` tag points at, when it has one.
    ///
    /// Not the same as the newest version: a package that ships a `2.0.0-rc.1`
    /// still has `latest` on `1.9.0`, and that is the publisher saying which
    /// one they mean.
    pub latest: Option<Version>,
}

/// Why a packument could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    /// `curl` could not be started.
    #[error("could not run curl: {detail}")]
    Program {
        /// What the operating system said.
        detail: String,
    },
    /// The request failed.
    #[error("could not read {name} from the registry: {detail}")]
    Fetch {
        /// Package that was asked about.
        name: CompactString,
        /// What curl said, trimmed.
        detail: String,
    },
    /// The answer was not a packument.
    #[error("the registry's answer for {name} is not a packument")]
    Malformed {
        /// Package that was asked about.
        name: CompactString,
    },
    /// The package name could not go in a URL.
    #[error("`{name}` is not a package name uf will put in a URL")]
    UnsafeName {
        /// The rejected name, length-bounded.
        name: CompactString,
    },
    /// The configured registry is not an HTTP URL.
    #[error("the configured registry `{registry}` is not an http or https URL")]
    UnsafeRegistry {
        /// The rejected registry, length-bounded.
        registry: CompactString,
    },
    /// A name in a bound scope that the bound registry does not publish.
    ///
    /// Not a reason to ask anybody else. The whole point of binding a scope is
    /// that this name has one source, so "not there" is the answer rather than
    /// the first half of a search.
    #[error(
        "`{name}` is not published on {registry}, which is the registry `{scope}` is bound to.\n\n  \
         uf does not look anywhere else for a bound scope: a fallback to the public registry is \
         the dependency-confusion attack, not a recovery from it.\n  \
         Either the name is wrong, or pm.scopes in uf.config.js binds `{scope}` to the wrong \
         registry."
    )]
    NotOnBoundRegistry {
        /// Package that was asked about.
        name: CompactString,
        /// The scope it is in.
        scope: CompactString,
        /// The registry that was asked, and the only one that will be.
        registry: CompactString,
    },
}

/// Which registry answers for which package name.
///
/// A default for everything, plus any number of `@scope -> registry` bindings
/// from `pm.scopes`. See the module docs for why a bound scope never falls back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryRouting {
    default: CompactString,
    scopes: BTreeMap<CompactString, CompactString>,
}

/// Where one name resolves from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route<'a> {
    /// The registry to ask, and — when [`Route::scope`] is set — the only one.
    pub registry: &'a str,
    /// The bound scope this came from, or `None` for the default registry.
    pub scope: Option<&'a str>,
}

impl Route<'_> {
    /// Whether this name may only be resolved from [`Route::registry`].
    #[must_use]
    pub const fn is_bound(&self) -> bool {
        self.scope.is_some()
    }
}

impl RegistryRouting {
    /// Everything from one registry, which is where a project starts.
    #[must_use]
    pub fn new(default: &str) -> Self {
        Self {
            default: default.to_compact_string(),
            scopes: BTreeMap::new(),
        }
    }

    /// Bind one scope, with or without its leading `@`.
    ///
    /// Both spellings are accepted because both are what people write, and a
    /// binding that silently did not apply because of a missing `@` would be a
    /// security setting that looked configured and was not.
    #[must_use]
    pub fn bind(mut self, scope: &str, registry: &str) -> Self {
        self.scopes
            .insert(normalize_scope(scope), registry.to_compact_string());
        self
    }

    /// The routing a project's `uf.config.js` describes.
    #[must_use]
    pub fn from_config(config: &UniflowedConfig) -> Self {
        let mut routing = Self::new(config.read_registry().url);
        for (scope, registry) in config.scope_registries() {
            routing = routing.bind(scope, registry);
        }
        routing
    }

    /// Whether any scope is bound at all.
    #[must_use]
    pub fn has_bindings(&self) -> bool {
        !self.scopes.is_empty()
    }

    /// The registry uf asks for everything that is not in a bound scope.
    #[must_use]
    pub fn default_registry(&self) -> &str {
        &self.default
    }

    /// Where `name` resolves from.
    #[must_use]
    pub fn route(&self, name: &str) -> Route<'_> {
        match scope_of(name).and_then(|scope| self.scopes.get_key_value(scope)) {
            Some((scope, registry)) => Route {
                registry,
                scope: Some(scope),
            },
            None => Route {
                registry: &self.default,
                scope: None,
            },
        }
    }
}

/// The scope a package name is in, with its `@`, or `None` for an unscoped one.
#[must_use]
pub fn scope_of(name: &str) -> Option<&str> {
    if !name.starts_with('@') {
        return None;
    }
    let end = name.find('/')?;
    Some(&name[..end])
}

/// `company` and `@company` are the same binding.
fn normalize_scope(scope: &str) -> CompactString {
    if scope.starts_with('@') {
        return scope.to_compact_string();
    }
    let mut normalized = CompactString::const_new("@");
    normalized.push_str(scope);
    normalized
}

/// Read one package's published versions from one registry.
///
/// # Errors
///
/// [`RegistryError`], which every caller reports beside the package rather than
/// failing the command: one unreachable package is not a reason to stop
/// reporting the other forty.
pub fn packument(registry: &str, name: &str) -> Result<Packument, RegistryError> {
    packument_from(
        Route {
            registry,
            scope: None,
        },
        name,
    )
}

/// Read one package's published versions from wherever `routing` sends it.
///
/// A name in a bound scope is asked of that scope's registry and of nothing
/// else, whatever the answer is. See the module docs.
///
/// # Errors
///
/// [`RegistryError`]. A bound scope whose registry does not have the name gets
/// [`RegistryError::NotOnBoundRegistry`] rather than a second request.
pub fn packument_via(routing: &RegistryRouting, name: &str) -> Result<Packument, RegistryError> {
    packument_from(routing.route(name), name)
}

fn packument_from(route: Route<'_>, name: &str) -> Result<Packument, RegistryError> {
    let url = url_for(route.registry, name)?;
    let body = get(&url, "application/vnd.npm.install-v1+json")
        .map_err(|failure| unread(&failure, route, name))?;
    parse(&body).ok_or_else(|| RegistryError::Malformed {
        name: bounded(name),
    })
}

/// The error a request that produced no body becomes.
///
/// A pure function of the three facts, so the one branch that matters can be
/// tested without a registry: a **bound** scope whose registry *answered* — and
/// answered that it does not publish this name — is the end of the search, not
/// the beginning of one. That is the whole of uf's dependency-confusion
/// defence on the read path, and if it ever quietly became a `Fetch` error some
/// caller retried elsewhere, nothing else here would notice.
///
/// An unreachable registry is deliberately *not* that case. "The company
/// registry is down" is not evidence that a name does not live there, and
/// saying it was would teach people to distrust the message.
fn unread(failure: &HttpFailure, route: Route<'_>, name: &str) -> RegistryError {
    match (failure, route.scope) {
        (HttpFailure::Program(detail), _) => RegistryError::Program {
            detail: detail.clone(),
        },
        (HttpFailure::Answered(_), Some(scope)) => RegistryError::NotOnBoundRegistry {
            name: bounded(name),
            scope: bounded(scope),
            registry: bounded(route.registry),
        },
        _ => RegistryError::Fetch {
            name: bounded(name),
            detail: failure.detail().to_owned(),
        },
    }
}

/// Read several, a few at a time, each from its own registry.
///
/// Every name comes back, with its own answer or its own error. The order of
/// the map is the order a report prints in, which is the name order rather than
/// whichever request finished first.
#[must_use]
pub fn packuments(
    routing: &RegistryRouting,
    names: &[CompactString],
) -> BTreeMap<CompactString, Result<Packument, RegistryError>> {
    let answers = Mutex::new(BTreeMap::new());
    let next = AtomicUsize::new(0);
    let workers = CONCURRENCY.min(names.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(name) = names.get(index) else {
                        return;
                    };
                    let answer = packument_via(routing, name);
                    // `unwrap` on a mutex a scope owns: the only way it is
                    // poisoned is a panic in this closure, and there is nothing
                    // above to unwind into.
                    answers
                        .lock()
                        .expect("no other thread panicked holding this")
                        .insert(name.clone(), answer);
                }
            });
        }
    });

    answers.into_inner().expect("the scope joined every worker")
}

/// A request that did not produce a body.
///
/// The three are kept apart because they are three different sentences to a
/// reader. "That registry does not have this name" is a different problem from
/// "uf could not reach that registry" — which is in turn different from "this
/// machine has no curl". [`classify`] is where the first two are told apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HttpFailure {
    /// `curl` could not be started at all.
    Program(String),
    /// The host answered, with an HTTP status of 400 or above.
    Answered(String),
    /// The request never got a whole answer.
    Unreachable(String),
}

impl HttpFailure {
    /// What curl, or the operating system, said.
    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::Program(detail) | Self::Answered(detail) | Self::Unreachable(detail) => detail,
        }
    }

    /// Whether the host answered rather than never being reached.
    pub(crate) const fn answered(&self) -> bool {
        matches!(self, Self::Answered(_))
    }
}

/// What `--write-out` prints after the body: a newline, then the HTTP status.
///
/// On a line of its own, so that nothing in a body can be read as part of the
/// status: the status is whatever follows the last newline curl wrote. curl
/// prints it whether or not `-f` failed the transfer, and prints `000` when no
/// response arrived at all.
const WRITE_OUT_STATUS: &str = "\n%{http_code}";

/// GET one URL over TLS, bounded, and hand back the body.
///
/// Shared by the packument reads and by [`crate::provenance`], because the
/// argument for every flag on it is the same in both places and a second copy
/// would be a second place for one of them to be dropped.
pub(crate) fn get(url: &str, accept: &str) -> Result<Vec<u8>, HttpFailure> {
    let output = Command::new("curl")
        .args([
            // `-f` still, so that the body of a response curl retries past — a
            // `503` before the answer — is never written ahead of the answer.
            // What it no longer decides is whether the host answered: the
            // status `--write-out` reports does. See [`classify`].
            "-fsSL",
            "--write-out",
            WRITE_OUT_STATUS,
            "--retry",
            "1",
            "--max-time",
            TIMEOUT_SECONDS,
            // curl gives up on its own rather than uf discovering the size of a
            // buffer it has already grown: `Command::output` collects the whole
            // child stream into memory, so a registry that streams for as long
            // as the timeout allows would be uf's memory, not curl's.
            "--max-filesize",
            MAX_PACKUMENT_BYTES_TEXT,
            // A redirect must not be able to leave TLS. `url_for` refuses
            // `http://`, and without these a `301` to one would undo that.
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-H",
        ])
        .arg(format!("Accept: {accept}"))
        .arg("--")
        .arg(url)
        .output();
    let output = match output {
        Ok(output) => output,
        Err(source) => return Err(HttpFailure::Program(source.to_string())),
    };
    classify(output.status.code(), &output.stdout, &output.stderr)
}

/// What one run of curl came to: its exit status, what it wrote, and what it
/// said.
///
/// Decided by the HTTP status, because curl's exit status does not carry it.
/// This used to count a failure as an answer when curl exited 22, and curl
/// 8.7.1 on macOS exits 56 for the same 404 — so every package the registry
/// holds no attestation for was reported as *unknown*, uf's word for "could
/// not reach the registry", when the registry had answered and curl had said so
/// in words (#1005). The status is the registry's own answer, and reads the
/// same from every curl that received one.
///
/// A pure function, so that a 404 under any exit status can be tested without
/// a registry or a particular build of curl.
fn classify(exit: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Vec<u8>, HttpFailure> {
    let (body, status) = match stdout.iter().rposition(|&byte| byte == b'\n') {
        Some(at) => (&stdout[..at], &stdout[at + 1..]),
        None => (&stdout[..0], stdout),
    };
    let status = std::str::from_utf8(status)
        .ok()
        .and_then(|status| status.trim().parse::<u16>().ok());
    match status {
        Some(status @ 400..=599) => Err(HttpFailure::Answered(format!(
            "the registry answered {status}"
        ))),
        // A success status alone is not a whole answer: curl reports the status
        // of a response it then failed to finish reading — a timeout part way
        // through — and half a packument is not a packument.
        Some(200..=299) if exit == Some(0) => {
            if body.len() > MAX_PACKUMENT_BYTES {
                return Err(HttpFailure::Answered(format!(
                    "the answer is larger than {MAX_PACKUMENT_BYTES} bytes"
                )));
            }
            Ok(body.to_vec())
        }
        // `000`, which is curl for no response at all, or anything else.
        _ => {
            let said = String::from_utf8_lossy(stderr).trim().to_owned();
            Err(HttpFailure::Unreachable(if said.is_empty() {
                String::from("curl ended without an answer")
            } else {
                said
            }))
        }
    }
}

/// The abbreviated packument's two fields.
fn parse(bytes: &[u8]) -> Option<Packument> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let versions = value
        .get("versions")?
        .as_object()?
        .keys()
        // A registry may carry a version string uf's parser declines — a
        // `1.0.0-beta+exp.sha.5114f85` is fine, `0.1` from 2011 is not. Skipped
        // rather than refused: one unparseable key does not make a packument
        // unreadable.
        .filter_map(|key| Version::parse(key))
        .collect();
    let latest = value
        .get("dist-tags")
        .and_then(|tags| tags.get("latest"))
        .and_then(Value::as_str)
        .and_then(Version::parse);
    Some(Packument { versions, latest })
}

/// The packument URL, with both halves checked before either reaches curl.
///
/// The registry comes from `uf.config.js` and the name from a `package.json`,
/// so both are project text. curl is spawned with an argv rather than a shell,
/// so there is no quoting to get wrong — but a name carrying `?`, `#` or `..`
/// would still change which URL is requested, and a `file://` registry would
/// make `uf update` read the disk. Both are refused here.
fn url_for(registry: &str, name: &str) -> Result<String, RegistryError> {
    let Some(authority) = registry.strip_prefix("https://") else {
        // HTTPS only, `http://` included. A registry URL can carry userinfo,
        // and `curl` sends it — over plaintext, to whoever is listening, on a
        // command uf runs on its own initiative. There is no version of that
        // worth the convenience of a plain-http mirror, and a project with one
        // sees a named refusal beside the packages uf could not read rather
        // than a report that quietly leaked its credentials.
        return Err(RegistryError::UnsafeRegistry {
            registry: bounded(registry),
        });
    };
    // `https://user:token@host/` is a URL uf will not build a request from even
    // over TLS: the token would be in the process table for the life of the
    // request, which is a different exposure from the wire and not a smaller
    // one. Checked before the first `/`, so a `@` in a path is not an authority.
    if authority
        .split('/')
        .next()
        .is_some_and(|host| host.contains('@'))
    {
        return Err(RegistryError::UnsafeRegistry {
            registry: bounded(registry),
        });
    }
    if !is_safe_package_name(name) {
        return Err(RegistryError::UnsafeName {
            name: bounded(name),
        });
    }
    // npm's own encoding for a scoped name: `@scope%2fname` is one path
    // segment, so a scope cannot become a directory in the URL.
    Ok(format!(
        "{}/{}",
        registry.trim_end_matches('/'),
        name.replace('/', "%2f")
    ))
}

/// npm's package-name alphabet, and nothing else.
///
/// Lower-case letters, digits, `-`, `_`, `.`, and a single leading `@scope/`.
/// Deliberately narrower than npm's historical names allow: a name uf refuses
/// is reported beside the package, and reporting one legacy package is a better
/// failure than requesting a URL nobody wrote.
pub(crate) fn is_safe_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 214 {
        return false;
    }
    let body = match name.strip_prefix('@') {
        None => name,
        Some(scoped) => {
            let Some((scope, rest)) = scoped.split_once('/') else {
                return false;
            };
            if scope.is_empty() || !is_safe_segment(scope) {
                return false;
            }
            rest
        }
    };
    !body.is_empty() && is_safe_segment(body)
}

fn is_safe_segment(segment: &str) -> bool {
    // A leading `.` or `_` is npm's own rule; the rest keeps `..`, `?`, `#`,
    // `%` and every other URL-significant byte out.
    if segment.starts_with('.') || segment.starts_with('_') {
        return false;
    }
    segment.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
    })
}

/// Keep untrusted text out of error messages beyond a fixed budget.
pub(crate) fn bounded_text(value: &str) -> CompactString {
    const BUDGET: usize = 64;

    let end = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= BUDGET)
        .last()
        .unwrap_or(0);
    value[..end].to_compact_string()
}

/// The same, under the name this module's own errors use.
fn bounded(value: &str) -> CompactString {
    bounded_text(value)
}

#[cfg(test)]
mod tests;
