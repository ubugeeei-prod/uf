//! `uf build --compile`: the application as one executable file.
//!
//! `uf build` already produces everything an application needs and nothing
//! that can run it: a client bundle, a server bundle, and prerendered HTML,
//! all of which want a JavaScript runtime and a `node_modules` beside them.
//! This step removes both requirements. It links the server bundle and an
//! embedded copy of the output directory into one module ([`super::vite`]'s
//! driver does the linking, because Vite is uf's bundler), then wraps a
//! JavaScript runtime around that module.
//!
//! # Which runtime is embedded, and who decides
//!
//! The project does, through `app.runtime.capabilityJsHost` — the same setting
//! that decides which host `uf dev`, `uf test` and `uf build` themselves run
//! on. Until ubugeeei-prod/uf#312 this one flag ignored it and required Bun,
//! so a project that pins Node was told to install a second runtime to compile
//! at all. [`runtime`] now walks the project's accepted hosts in the order
//! [`super::vite::resolve_host`] walks them and returns the first that has a
//! backend here and can be used:
//!
//! | host | backend | cross-compiles | what it needs |
//! | --- | --- | --- | --- |
//! | `bun` | `bun build --compile` | yes, eight targets | `bun` on PATH |
//! | `node` | `node --build-sea` | no | Node [`NODE_SEA_FLOOR`] or newer |
//! | `deno` | — | — | refused by name |
//!
//! **Bun** appends the bundled JavaScript to a copy of its own runtime and
//! writes one file. It is one process invocation with no intermediate
//! artefacts, and it is the only one of the two that can produce a binary for
//! a machine other than the one building it — see [`Target`].
//!
//! **Node** builds a single-executable application: a copy of `node` with the
//! bundle in a blob appended to it. It takes three steps rather than one — a
//! config file, `--build-sea`, and a signature on macOS — and it cannot
//! cross-compile, because Node's own documentation states that the binary
//! producing the blob is the binary receiving it. What made it worth the extra
//! steps is that Node is the host most projects already have, and that
//! `--build-sea` removed the part that made it unacceptable: before Node 25.5
//! the procedure needed `postject`, an npm dependency uf would have had to
//! install into a user's project to finish a build.
//!
//! **Deno** has `deno compile`, and no backend here. Nothing about the shim
//! stands in the way; what is missing is the work of writing and testing it,
//! and a backend nobody has run is not one to select silently. A project whose
//! host is Deno is told so and told which two work.
//!
//! **A Rust host embedding a JavaScript engine, rejected.** This is the most
//! uf-shaped answer in the abstract and the least honest one in practice. An
//! engine is not a runtime: React 19's server renderer needs WHATWG streams,
//! `fetch`, `AsyncLocalStorage`, timers and a working `node:` surface, none of
//! which QuickJS or Boa bring and all of which would have to be written here.
//! That is building a JavaScript runtime, which is a larger project than uf,
//! and it is the same mistake as rebuilding Vite — the guide names that one
//! specifically.
//!
//! **A backend that fails is not the end of the build**, where the project
//! accepts more than one host. Neither backend compiles or evaluates the
//! bundle — each appends it to a copy of its own runtime — so a failure there
//! is about the toolchain rather than about the application, and uf tries the
//! next accepted host and says so. That is not a nicety: `--build-sea` support
//! is compiled into a `node` binary as well as being a version of it, and a
//! mainstream distribution can ship a recent Node without it. See [`wrap`].
//!
//! The shim both backends link against is `@uniflowed/server/standalone`,
//! written against `node:http` rather than `Bun.serve` precisely so the module
//! a runtime is wrapped around does not itself pick the runtime. That
//! foresight is the whole reason a second backend is this file and not a
//! rewrite of the server.
//!
//! # What it costs
//!
//! Nothing is needed to *run* a compiled binary, and one of the two runtimes
//! is needed to *produce* one. The application executes on that runtime's
//! implementation of the `node:` API surface, which for Bun is not Node's.
//! Both facts are stated when the flag is used and neither is discovered at
//! runtime: a project with no usable backend is told so before any work
//! begins, and the build's summary names the runtime and its version.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_bundle::{Embedded, ReportOptions, write_embedded_assets};
use uf_config::{CapabilityJsHost, Permissions, UniflowedConfig};
use uf_rsc::RSC_MANIFEST_ENV;
use uf_runtime::{RuntimeHost, ToolchainAccess};
use uf_term::Status;

use crate::commands::vite::{
    Driver, Event, LinkContext, LogLevel, find_program, render_error, render_log,
};
use crate::support::project_label;
use crate::ui::Ui;

/// The directory the linked bundle and its embedded assets are written to.
///
/// Beside `.uf/build/server`, which the ordinary build already uses, and
/// outside the output directory so `emptyOutDir` cannot sweep it away
/// mid-build.
const WORK_DIR: &str = ".uf/build/compile";

/// Where a cross-compiled runtime is cached, when one has to be downloaded.
///
/// Inside the project, and that is the decision rather than a detail. Bun's own
/// default is `~/.bun/install/cache`, which is a build writing outside the tree
/// it was pointed at — it behaves one way on a developer's laptop, another on a
/// CI runner with a cold cache, and on a sandboxed runner it does not behave at
/// all:
///
/// ```text
/// EPERM: Failed to move cross-compiled bun binary into cache directory
///        /Users/…/.bun/install/cache/bun-linux-x64-v1.1.27
/// ```
///
/// That failure is what ubugeeei-prod/uf#310 records as the reason `--target`
/// was not shipped with `--compile`. A project-local cache costs a second copy
/// per checkout and buys a build that does the same thing everywhere, which is
/// the trade `docs/red-lines.md` line 5 asks for. A machine that would rather
/// pay once sets `BUN_INSTALL_CACHE_DIR` itself and uf leaves it alone.
const RUNTIME_CACHE_DIR: &str = ".uf/cache/bun";

/// Bun's name for the environment variable that moves its cache.
const BUN_CACHE_ENV: &str = "BUN_INSTALL_CACHE_DIR";

/// The first Node whose `--build-sea` needs no `postject`.
///
/// A build step whose first act is to install a package into somebody's
/// project is not a build step this toolchain should own, so the backend
/// starts here rather than reaching for the older procedure. A project on an
/// older Node is told the version it has and the version it needs — which is
/// the whole of ubugeeei-prod/uf#312's "an unsupported combination is refused
/// by name".
const NODE_SEA_FLOOR: Version = Version {
    major: 25,
    minor: 5,
    patch: 0,
};

/// Which runtime a backend wraps around the bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    /// `bun build --compile`.
    Bun,
    /// `node --build-sea`, a single-executable application.
    NodeSea,
}

impl Backend {
    /// The runtime's name, as a person writes it and as the summary prints it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Bun => "bun",
            Self::NodeSea => "node",
        }
    }
}

/// A platform `--target` can name, in the spelling uf uses for itself.
///
/// **Rust triples, not Bun's own names.** uf already publishes itself as
/// `aarch64-apple-darwin` and friends (`.github/workflows/release.yml`), and a
/// toolchain that spells the same machine two ways depending on which of its
/// commands you are running is a toolchain with two vocabularies. Bun's names
/// are accepted nowhere and recognised in the error, so `--target
/// bun-linux-x64` is answered with the triple to type instead rather than with
/// a list to search.
///
/// # What uf names and what a backend can produce are two questions
///
/// This table is Bun's documented target set. Whether the *installed* Bun has
/// a runtime for each of them is a property of that Bun and not of uf: 1.1.27
/// refuses `bun-linux-x64-musl`, `bun-linux-arm64-musl` and
/// `bun-windows-arm64` with `Unsupported compile target`, and a later one does
/// not. uf cannot ask Bun for the list — there is no command that answers —
/// so what it does instead is refuse a triple it has never heard of before any
/// work happens, and turn a target Bun declines into a sentence naming the
/// triple and the Bun that declined it. See [`compile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    /// The triple, which is what `--target` takes.
    pub(crate) triple: &'static str,
    /// Bun's name for the same machine, as `bun build --target` takes it.
    bun: &'static str,
    /// Bun's name for the same machine in its *cache*, which is not always
    /// the one above.
    ///
    /// Bun accepts `arm64` on the command line and writes `aarch64` in the
    /// filename it caches the runtime under. That is Bun's own rule and it is
    /// recorded here as data rather than inferred with a `replace`, because a
    /// table is checkable and a rewrite is a guess about somebody else's
    /// naming. Its own messages are the evidence: asking 1.1.27 for
    /// `bun-linux-arm64-musl` is refused as `bun-linux-aarch64-musl-v1.1.27`,
    /// and `bun-windows-arm64` as `bun-windows-aarch64-v1.1.27`.
    ///
    /// Getting this wrong is not cosmetic. [`cached_runtime`] is what decides
    /// whether uf announces a 90 MB download before the build and whether it
    /// reports one afterwards, so a name that never matches means every build
    /// claims to be fetching and none admits to having fetched.
    runtime: &'static str,
    /// `std::env::consts::OS` for this platform.
    os: &'static str,
    /// `std::env::consts::ARCH` for this platform.
    arch: &'static str,
    /// Whether the executable for this platform is named `.exe`.
    windows: bool,
}

impl Target {
    /// Whether this target is the machine running the build.
    ///
    /// The two builds differ in one way that a reader can see: a target that
    /// is not the host needs that platform's runtime, which is a download the
    /// build has to announce. Compared through `std::env::consts` rather than
    /// against a triple constant because Rust has no target triple to hand at
    /// runtime without a build script, and the two fields are exactly what
    /// distinguishes the eight.
    fn is_host(self) -> bool {
        self.os == std::env::consts::OS && self.arch == std::env::consts::ARCH
    }
}

/// Every platform `--target` names.
///
/// Ordered by operating system and then by architecture, which is the order
/// the refusal lists them in and therefore the order a reader scans.
pub(crate) const TARGETS: &[Target] = &[
    Target {
        triple: "aarch64-apple-darwin",
        bun: "bun-darwin-arm64",
        runtime: "bun-darwin-aarch64",
        os: "macos",
        arch: "aarch64",
        windows: false,
    },
    Target {
        triple: "x86_64-apple-darwin",
        bun: "bun-darwin-x64",
        runtime: "bun-darwin-x64",
        os: "macos",
        arch: "x86_64",
        windows: false,
    },
    Target {
        triple: "aarch64-unknown-linux-gnu",
        bun: "bun-linux-arm64",
        runtime: "bun-linux-aarch64",
        os: "linux",
        arch: "aarch64",
        windows: false,
    },
    Target {
        triple: "aarch64-unknown-linux-musl",
        bun: "bun-linux-arm64-musl",
        runtime: "bun-linux-aarch64-musl",
        os: "linux",
        arch: "aarch64",
        windows: false,
    },
    Target {
        triple: "x86_64-unknown-linux-gnu",
        bun: "bun-linux-x64",
        runtime: "bun-linux-x64",
        os: "linux",
        arch: "x86_64",
        windows: false,
    },
    Target {
        triple: "x86_64-unknown-linux-musl",
        bun: "bun-linux-x64-musl",
        runtime: "bun-linux-x64-musl",
        os: "linux",
        arch: "x86_64",
        windows: false,
    },
    Target {
        triple: "aarch64-pc-windows-msvc",
        bun: "bun-windows-arm64",
        runtime: "bun-windows-aarch64",
        os: "windows",
        arch: "aarch64",
        windows: true,
    },
    Target {
        triple: "x86_64-pc-windows-msvc",
        bun: "bun-windows-x64",
        runtime: "bun-windows-x64",
        os: "windows",
        arch: "x86_64",
        windows: true,
    },
];

/// The triple `requested` names, or why uf will not accept it.
///
/// Refused *before* anything is built, which is the same rule `--compile` and
/// `--adapter` already follow: a build that spends a minute on the bundle and
/// then says the target does not exist has spent a minute on the wrong answer.
pub(crate) fn parse_target(requested: &str) -> Result<Target> {
    if let Some(target) = TARGETS.iter().find(|target| target.triple == requested) {
        return Ok(*target);
    }
    let supported = TARGETS
        .iter()
        .map(|target| target.triple)
        .collect::<Vec<_>>()
        .join("\n  ");
    // A reader who typed Bun's name for the machine typed a real name for a
    // real platform, and answering that with a list to search would make them
    // find the line they already knew. Name the triple instead.
    if let Some(target) = TARGETS.iter().find(|target| target.bun == requested) {
        bail!(
            "`--target {requested}` is Bun's name for that machine; uf names it \
             `{}`, the way it names its own release binaries.\n  \
             uf builds for:\n  {supported}",
            target.triple
        );
    }
    bail!(
        "`--target {requested}` is not a platform uf builds for.\n  \
         uf builds for:\n  {supported}"
    )
}

/// The runtime `uf build --compile` wraps around the application, resolved.
///
/// Everything a compile needs that can be known before the bundle exists,
/// answered once: which backend, where its program is, which version of it,
/// which platform the binary is for, whether producing it needs the network,
/// and what the artefact will be allowed to reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Runtime {
    /// Which of the two backends this is.
    pub(crate) backend: Backend,
    /// The program, as found on PATH.
    pub(crate) program: Utf8PathBuf,
    /// Its version, as it reported it. Named in the summary, because "uf
    /// compiled with Bun" and "uf compiled with *that* Bun" are two facts and
    /// the second is the one that explains a target it refused.
    pub(crate) version: String,
    /// The platform the binary is for, when one was asked for.
    pub(crate) target: Option<Target>,
    /// Where a downloaded runtime is cached, when the target needs one.
    cache: Option<Utf8PathBuf>,
    /// Whether that runtime is already in the cache.
    ///
    /// `true` when nothing has to be fetched — either the target is the host,
    /// or a previous build already downloaded it.
    pub(crate) cached: bool,
    /// The flags the embedded runtime starts the application with.
    ///
    /// Empty unless the project declared `permissions`; see
    /// [`artefact_permissions`].
    exec_argv: Vec<String>,
}

impl Runtime {
    /// The runtime and its version, as the build summary prints them.
    pub(crate) fn label(&self) -> String {
        format!("{} {}", self.backend.name(), self.version)
    }

    /// Whether producing this binary has to reach the network.
    ///
    /// Said before the build starts rather than discovered in the middle of
    /// it: a build that needs 90 MB off the internet to succeed is a build a
    /// person on a train wants to know about before they wait for the bundle.
    pub(crate) fn fetches_a_runtime(&self) -> bool {
        !self.cached
    }
}

/// A finished binary.
#[derive(Debug, Clone)]
pub(crate) struct Compiled {
    /// Where it was written.
    pub(crate) binary: Utf8PathBuf,
    /// How big it is, which is the number the summary reports.
    pub(crate) bytes: u64,
    /// What went inside it.
    pub(crate) embedded: Embedded,
    /// The runtime that produced it, which is the one inside it.
    ///
    /// The runtime that *worked*, and not necessarily the first uf tried —
    /// see [`compile`]. Carried on the result rather than read back off the
    /// resolution, because after a fallback those are two different answers
    /// and the summary must print the true one.
    pub(crate) runtime: Runtime,
    /// The runtime that was downloaded for it, and how big it was.
    ///
    /// `Some` only when this build fetched one; a cached runtime is not
    /// reported as a fetch, which is how ubugeeei-prod/uf#310's "a cached
    /// runtime is not fetched twice" is visible rather than merely true.
    pub(crate) fetched: Option<u64>,
}

/// A version triple, compared rather than parsed twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    /// The leading `<major>.<minor>.<patch>` of `text`, ignoring the rest.
    ///
    /// `node --version` says `v25.8.1` and `bun --version` says `1.1.27`; a
    /// nightly says `26.0.0-nightly20260901`. Only the three numbers decide
    /// anything here, and a suffix that cannot be parsed is a suffix that does
    /// not matter.
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('v');
        let mut parts = text.split(['.', '-', '+']);
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Ask `program` for its version.
fn program_version(program: &Utf8Path, argument: &str) -> Option<String> {
    let output = Command::new(program.as_std_path())
        .arg(argument)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8_lossy(&output.stdout);
    let line = said.lines().next()?.trim();
    // `deno --version` puts its name first; the two that matter here do not,
    // but taking the last whitespace-separated word costs nothing and makes
    // this the same function for all three.
    Some(
        line.split_whitespace()
            .last()
            .unwrap_or(line)
            .trim_start_matches('v')
            .to_owned(),
    )
}

/// Find a backend for this project, or explain what is missing.
///
/// Checked before Vite is asked to link anything: discovering that a binary
/// cannot be produced *after* bundling the application twice is a minute of
/// somebody's afternoon spent on a message that was knowable at the start.
///
/// # The order the hosts are tried in
///
/// The project's `capabilityJsHost.default` first, then — only when
/// `autoDetect` is on — the rest of the accepted set, which is exactly what
/// [`super::vite::resolve_host`] does for every other command. A project that
/// pins one host and cannot compile on it is told so rather than handed a
/// binary running a runtime it did not choose, and a project that accepts
/// several gets the first that works, with the summary naming it.
///
/// `--target` narrows the same walk rather than steering around it. A backend
/// that cannot produce the requested machine is not a usable backend for
/// *this* build, so it is skipped with a reason exactly as a missing program
/// is — which means a project on the default configuration (`node`, with
/// `autoDetect` on) can cross-compile through Bun, and the summary names the
/// runtime that went in. A project that pinned its host with `autoDetect:
/// false` has one candidate and gets the reason as the refusal, which is the
/// right answer for a project that said it wanted no inference.
pub(crate) fn runtimes(
    root: &Utf8Path,
    config: &UniflowedConfig,
    requested_target: Option<&str>,
) -> Result<Vec<Runtime>> {
    let target = requested_target.map(parse_target).transpose()?;
    let hosts = &config.app.runtime.capability_js_host;
    let mut candidates = vec![hosts.default];
    if hosts.auto_detect {
        candidates.extend(
            hosts
                .hosts
                .iter()
                .copied()
                .filter(|host| *host != hosts.default),
        );
    }

    let mut usable = Vec::new();
    let mut refusals = Vec::new();
    for kind in candidates {
        match backend_for(kind) {
            Ok((backend, program, version)) => match refuse_target(backend, target) {
                Some(refusal) => refusals.push(refusal),
                None => match finish(root, config, backend, program, version, target) {
                    Ok(runtime) => usable.push(runtime),
                    // A permission set this backend cannot enforce makes it
                    // unusable *for this project*, which is the same kind of
                    // fact as a missing program — so it joins the list of
                    // reasons rather than ending the walk. A project that
                    // pinned one host still gets that sentence as its refusal,
                    // because there is nothing else in the list.
                    Err(refusal) => refusals.push(format!("{}: {refusal}", backend.name())),
                },
            },
            Err(refusal) => refusals.push(refusal),
        }
    }
    if !usable.is_empty() {
        return Ok(usable);
    }
    // Every host that was tried and what it said, then the one sentence that
    // is the same for all of them. A reader whose `autoDetect` is off sees one
    // line and the fix; a reader who accepted three sees why each was passed
    // over, which is the difference between "uf will not do this" and "uf
    // cannot do this here".
    let what = match target {
        Some(target) => format!("a binary for {}", target.triple),
        None => String::from("a binary"),
    };
    let advice = match target {
        Some(_) => {
            "Cross-compiling is Bun's backend only: install `bun` and let \
                    `app.runtime.capabilityJsHost` accept it, or build this target on a machine \
                    that is one."
        }
        None => {
            "The hosts uf compiles on are Bun and Node — name an installed one in \
                 `app.runtime.capabilityJsHost.default`, or build without `--compile` and deploy \
                 the output directory with a JavaScript host."
        }
    };
    bail!(
        "`uf build --compile` found no runtime it can build {what} with.\n{}\n  {advice}",
        refusals
            .iter()
            .map(|refusal| format!("  {refusal}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// The backend for one accepted host, or the sentence saying why not.
fn backend_for(kind: CapabilityJsHost) -> Result<(Backend, Utf8PathBuf, String), String> {
    match kind {
        CapabilityJsHost::Bun => match find_program("bun") {
            Some(program) => {
                let version = program_version(&program, "--version")
                    .unwrap_or_else(|| String::from("unknown"));
                Ok((Backend::Bun, program, version))
            }
            None => Err("bun: not on PATH".to_owned()),
        },
        CapabilityJsHost::Node => {
            let Some(program) = find_program("node") else {
                return Err("node: not on PATH".to_owned());
            };
            let Some(version) = program_version(&program, "--version") else {
                return Err(format!("node: {program} did not answer `--version`"));
            };
            match Version::parse(&version) {
                Some(parsed) if parsed >= NODE_SEA_FLOOR => {
                    Ok((Backend::NodeSea, program, version))
                }
                Some(parsed) => Err(format!(
                    "node: {parsed} is older than {NODE_SEA_FLOOR}, the first with `--build-sea`; \
                     before it, building a single-executable application needs the `postject` \
                     package installed into your project, which uf will not do"
                )),
                None => Err(format!("node: could not read a version out of `{version}`")),
            }
        }
        // No `deno compile` backend here, and the honest reason is that nobody
        // has written and run one — not that Deno cannot. Saying "not yet" and
        // naming the two that work is what a reader can act on; silently
        // compiling their Deno project with Bun is not.
        CapabilityJsHost::Deno => {
            Err("deno: `deno compile` is not a backend uf has written".to_owned())
        }
    }
}

/// Why `backend` cannot produce `target`, when it cannot.
///
/// The one thing that separates the two backends for a reader, so it is one
/// function rather than a condition inside the walk: Bun cross-compiles by
/// downloading the target's runtime, and Node cannot cross-compile at all —
/// its own documentation states that the binary producing the blob is the
/// binary receiving it, which is not a limitation a flag or a download can
/// lift.
fn refuse_target(backend: Backend, target: Option<Target>) -> Option<String> {
    let target = target?;
    match backend {
        Backend::Bun => None,
        Backend::NodeSea => Some(format!(
            "node: a single-executable application is a copy of the running `node` with the \
             bundle appended, so it cannot be built for {} — Node has no cross-compilation, with \
             a flag or without one. Bun's backend does",
            target.triple
        )),
    }
}

/// Check the permission set against the chosen backend and settle the cache.
///
/// The target has already been checked by [`refuse_target`] in the walk above,
/// which is where a backend that cannot produce it is skipped rather than
/// selected and then refused.
fn finish(
    root: &Utf8Path,
    config: &UniflowedConfig,
    backend: Backend,
    program: Utf8PathBuf,
    version: String,
    target: Option<Target>,
) -> Result<Runtime> {
    let exec_argv = artefact_permissions(backend, config.permissions.as_ref())?;
    let (cache, cached) = match target {
        // Nothing is downloaded for the machine the build is running on: Bun
        // appends to the copy of itself that uf just started.
        Some(target) if !target.is_host() => {
            let cache = std::env::var(BUN_CACHE_ENV)
                .map_or_else(|_| root.join(RUNTIME_CACHE_DIR), Utf8PathBuf::from);
            let cached = cached_runtime(&cache, target).is_some();
            (Some(cache), cached)
        }
        _ => (None, true),
    };

    Ok(Runtime {
        backend,
        program,
        version,
        target,
        cache,
        cached,
        exec_argv,
    })
}

/// What the compiled binary is allowed to reach, as the runtime's own flags.
///
/// ubugeeei-prod/uf#617 made a permission set a property of the *toolchain*:
/// one declaration in `uf.config.js`, translated per host, and refused rather
/// than partly applied where a host cannot enforce it. A compiled binary is
/// the case that setting has to answer next, because it is the artefact that
/// leaves the machine — and the rule that decides it is #617's own: uf will
/// not run a set it cannot enforce and call it enforced.
///
/// * **Node** can. A single-executable application's `execArgv` is baked into
///   the binary, so `--permission --allow-fs-read=…` is in force from the
///   first line the application runs, on a machine uf will never see. The
///   translation is `uf_runtime`'s, unchanged, which is the point: the same
///   declaration that restricts `uf test` restricts the shipped binary, and a
///   category Node cannot express is refused here exactly as it is there.
/// * **Bun** cannot. It has no permission model to translate into and no
///   equivalent of `execArgv`, so a project that declared a set and compiled
///   on Bun would ship a binary that looks sandboxed and is not.
///
/// # Why the toolchain grants are empty here and not everywhere
///
/// [`ToolchainAccess`] exists because a *run* of uf has to read the project to
/// run it — the module graph, `node_modules`, the loader thread, the `uf
/// transform` child. A compiled binary has none of that: the bundle is one
/// file inside the executable and the output directory is bytes inside it too,
/// so uf needs no grant of its own and adds none. That makes this the only
/// place in the toolchain where the declared set is the *whole* set, which is
/// worth a reader knowing before they compare it against what `uf test` gets.
fn artefact_permissions(
    backend: Backend,
    permissions: Option<&Permissions>,
) -> Result<Vec<String>> {
    let Some(permissions) = permissions else {
        return Ok(Vec::new());
    };
    match backend {
        Backend::NodeSea => uf_runtime::permissions::host_arguments(
            RuntimeHost::Node,
            permissions,
            &ToolchainAccess::default(),
        )
        .map_err(|error| anyhow::anyhow!("{error}")),
        Backend::Bun => bail!(
            "`uf.config.js` declares permissions ({}) and `uf build --compile` on Bun cannot put \
             them in force: Bun has no permission model, and a compiled binary has no command \
             line to hand one to.\n  \
             Compile on Node {NODE_SEA_FLOOR} or newer, which bakes `--permission` into the \
             executable and enforces `read` and `write`, or remove the block — uf will not ship \
             a binary that looks sandboxed and is not.",
            permissions
                .granted()
                .map(|permission| format!("`{permission}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The cached runtime for `target`, if it is already there.
///
/// Bun names the file `<its cache name>-v<version>` — see [`Target::runtime`],
/// which is not always what `--target` took — so the prefix is enough and the
/// version is deliberately not part of the question: what a reader is told is
/// whether this build has to reach the network, and a cache holding some other
/// Bun's copy means it does.
fn cached_runtime(cache: &Utf8Path, target: Target) -> Option<(Utf8PathBuf, u64)> {
    let prefix = format!("{}-v", target.runtime);
    for entry in fs::read_dir(cache.as_std_path()).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) {
            continue;
        }
        let size = entry.metadata().ok()?.len();
        let path = Utf8PathBuf::from_path_buf(entry.path()).ok()?;
        return Some((path, size));
    }
    None
}

/// The name of the file a project compiles to.
///
/// The project's directory name, which is what the build banner already calls
/// it, so the binary and the thing it was built from are spelled the same way.
/// The extension follows the *target* rather than the build machine, because a
/// Windows binary cross-compiled from a Mac is still a Windows binary — and
/// because Bun appends `.exe` to a Windows target's output itself, so a name
/// without one would leave `uf` reporting a path that does not exist.
pub(crate) fn binary_name(root: &Utf8Path, target: Option<Target>) -> String {
    let name = project_label(root);
    let windows = target.map_or(cfg!(windows), |target| target.windows);
    if windows {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Every name a `--compile` of this project could have written into `dist/`.
///
/// For a reader of one build there is exactly one, and it is
/// [`binary_name`]'s. This is for the step that has to recognise the file
/// without knowing which target produced it — [`super::deploy`], which copies
/// the output directory and must not copy a 60 MB executable into it as a
/// static asset. Two entries rather than a guess, because the extension is
/// decided by `--target` and a Windows binary cross-compiled from Linux is the
/// case a single spelling gets wrong.
pub(crate) fn binary_names(root: &Utf8Path) -> Vec<String> {
    let name = project_label(root);
    vec![name.to_owned(), format!("{name}.exe")]
}

/// Link the application and wrap the runtime around it.
///
/// Runs after the size report, on purpose: the binary is written *into* the
/// output directory, and a 60 MB executable counted among the shipped assets
/// would make every budget in `uf.config.js` meaningless.
pub(crate) fn compile(
    ui: &mut Ui,
    runtimes: &[Runtime],
    link: LinkContext<'_>,
) -> Result<Compiled> {
    let runtime = runtimes
        .first()
        .expect("`compile::runtimes` returns at least one backend or an error");
    let LinkContext {
        host,
        builder,
        root,
        out_dir,
        env,
        rsc_manifest,
    } = link;
    let work = root.join(WORK_DIR);
    let assets = work.join("assets.js");
    let name = binary_name(root, runtime.target);
    let binary = out_dir.join(&name);

    // The binary lands in the directory whose contents it embeds. `emptyOutDir`
    // means a rebuild has already removed the previous run's copy by the time
    // this walks, so the exclusion is for the project that turned that off —
    // without it, every `--compile` would embed the last binary inside the next
    // one and double in size on each build.
    //
    // Both spellings, and the second is not hypothetical: `--target` decides
    // the extension, so building for Windows and then for Linux in the same
    // `dist/` would otherwise put the `.exe` inside the ELF. It is the same
    // list [`super::deploy`] skips, for the same reason.
    let mut options = ReportOptions::default();
    for spelling in binary_names(root) {
        options.excluded.push(spelling.as_str().into());
    }
    // The embedded total comes out larger than the `shipped` figure beside it
    // in the summary, and the gap is source maps: the size report counts what
    // a visitor downloads and a map is not that, while the binary has to carry
    // everything the output directory holds or a `sourceMappingURL` in it
    // becomes a 404. A project that would rather not pay for them turns them
    // off where they are produced, in `build.sourcemap`; dropping them here
    // would be uf deciding not to ship a file the project asked it to build.
    let embedded = write_embedded_assets(out_dir, &options, &assets)
        .context("packing the output directory into the binary")?;

    let mut driver = Driver::spawn(
        host,
        builder,
        root,
        "compile",
        &[
            String::from("--out-dir"),
            out_dir
                .strip_prefix(root)
                .unwrap_or(out_dir)
                .as_str()
                .to_owned(),
            String::from("--assets"),
            assets.to_string(),
            String::from("--bundle"),
            work.to_string(),
        ],
        env,
        // The second Vite run of one build, and it needs the same analysis the
        // first had: without it `virtual:uf/actions` is generated from no
        // manifest, which is an empty table, which is every server action
        // answering 404 in the binary a person actually ships.
        &[(RSC_MANIFEST_ENV, rsc_manifest.as_str())],
    )?;
    while let Some(event) = driver.next_event()? {
        match event {
            Event::Log { level, message } => match level {
                LogLevel::Warn | LogLevel::Error => render_log(ui, level, &message),
                LogLevel::Info => {}
            },
            Event::Error(error) => {
                let failure = render_error(ui, root, &error);
                let _ = driver.finish("uf build --compile");
                return Err(failure);
            }
            _ => {}
        }
    }
    driver.finish("linking the standalone bundle")?;

    let bundle = work.join("server.js");
    if !bundle.is_file() {
        bail!("the standalone link wrote no bundle at {bundle}");
    }

    let runtime = wrap(ui, runtimes, root, &work, &bundle, &binary)?;

    let bytes = std::fs::metadata(binary.as_std_path())
        .with_context(|| {
            format!(
                "`{}` reported success but wrote no {binary}",
                runtime.backend.name()
            )
        })?
        .len();
    let fetched = match (&runtime.cache, runtime.target) {
        (Some(cache), Some(target)) if !runtime.cached => {
            cached_runtime(cache, target).map(|(_, size)| size)
        }
        _ => None,
    };

    Ok(Compiled {
        binary,
        bytes,
        embedded,
        runtime: runtime.clone(),
        fetched,
    })
}

/// Wrap the first backend that can, saying so when it is not the first tried.
///
/// The bundle above is the same file for both backends, so a backend that
/// fails costs the build a subprocess rather than a minute — which is what
/// makes trying the next one reasonable rather than wasteful.
///
/// # Why a failure here is never the application's
///
/// Neither backend compiles or evaluates what it is handed: Bun appends the
/// bundle to a copy of its own runtime, and `--build-sea` appends a blob to a
/// copy of `node`. What can go wrong is the *toolchain* — a `node` built
/// without single-executable support (see [`wrap_with_node`]), a Bun with no
/// runtime for the target, a path that cannot be written. So moving on cannot
/// hide a broken application, which is the objection this would otherwise
/// have to answer.
///
/// It is announced rather than silent, and it happens only where
/// `capabilityJsHost.auto_detect` already says uf may infer a host: a project
/// that pinned one has a list of one, and gets that backend's failure as the
/// build's failure.
fn wrap<'a>(
    ui: &mut Ui,
    runtimes: &'a [Runtime],
    root: &Utf8Path,
    work: &Utf8Path,
    bundle: &Utf8Path,
    binary: &Utf8Path,
) -> Result<&'a Runtime> {
    let mut failures = Vec::new();
    for (at, runtime) in runtimes.iter().enumerate() {
        let attempt = match runtime.backend {
            Backend::Bun => wrap_with_bun(runtime, root, bundle, binary),
            Backend::NodeSea => wrap_with_node(runtime, root, work, bundle, binary),
        };
        match attempt {
            Ok(()) => return Ok(runtime),
            Err(failure) => {
                let Some(next) = runtimes.get(at + 1) else {
                    // The last one, so this is the build's failure — and it is
                    // the headline, with the earlier attempts recapped under
                    // it. They were each warned about as they happened, but a
                    // reader who has only the error stream in front of them
                    // has to be able to see that Bun's message is the *second*
                    // thing that went wrong.
                    return Err(match failures.is_empty() {
                        true => failure,
                        false => anyhow::anyhow!(
                            "{failure:?}\n\nuf had already tried:\n{}",
                            failures.join("\n\n")
                        ),
                    });
                };
                let said = format!(
                    "{} could not write the binary, so uf is trying {} instead:\n{failure:?}",
                    runtime.label(),
                    next.label()
                );
                ui.render(|renderer, out| renderer.status(out, Status::Warn, &said));
                failures.push(said);
            }
        }
    }
    unreachable!("`compile::runtimes` returns at least one backend or an error")
}

/// `bun build --compile`, for the host or for another platform.
fn wrap_with_bun(
    runtime: &Runtime,
    root: &Utf8Path,
    bundle: &Utf8Path,
    binary: &Utf8Path,
) -> Result<()> {
    let mut command = Command::new(runtime.program.as_std_path());
    command
        .args(["build", "--compile"])
        .arg(bundle.as_str())
        .arg("--outfile")
        .arg(binary.as_str())
        .current_dir(root.as_std_path());
    if let Some(target) = runtime.target {
        command.arg(format!("--target={}", target.bun));
    }
    if let Some(cache) = &runtime.cache {
        fs::create_dir_all(cache.as_std_path())
            .with_context(|| format!("failed to create the runtime cache at {cache}"))?;
        command.env(BUN_CACHE_ENV, cache.as_str());
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to start {} for `uf build --compile`",
            runtime.program
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    // Bun writes the useful half of a compile failure to stderr and says
    // nothing on stdout; both are forwarded because a message split across
    // the two is worse than a message repeated.
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // The one failure uf can say more about than Bun does. Which targets a Bun
    // has runtimes for changes between releases and there is no command that
    // asks, so uf names the triple it accepted, the Bun that declined it, and
    // the fix — rather than leaving a reader with Bun's own name for a machine
    // they did not type.
    if let Some(target) = runtime.target
        && said.contains("Unsupported compile target")
    {
        bail!(
            "bun {} has no runtime for {} (it calls it `{}`), so it cannot build that binary.\n  \
             uf accepts the triple because a newer Bun does have it: upgrade Bun, or build this \
             target on a machine that is one.\n{}",
            runtime.version,
            target.triple,
            target.bun,
            said.trim_end()
        );
    }
    bail!(
        "`bun build --compile` exited with {}\n{}",
        output.status,
        said.trim_end()
    )
}

/// `node --build-sea`, then a signature where the platform needs one.
///
/// Three steps rather than Bun's one, and each is load-bearing:
///
/// 1. **a config file.** Node's SEA takes its instructions from JSON rather
///    than from flags. `mainFormat: "module"` is what lets the entry be the
///    ESM bundle the driver just wrote — without it Node parses a module as a
///    script and the first `import` is a syntax error — and `execArgv` is
///    where the project's permission set is baked in.
/// 2. **`--build-sea`.** Node copies its own executable, appends the blob, and
///    writes the result. There is nothing to install; before Node
///    [`NODE_SEA_FLOOR`] there was, which is why this backend starts there.
/// 3. **`codesign`.** Modifying a Mach-O invalidates its signature, and macOS
///    on Apple silicon refuses to execute an ad-hoc binary with a broken one —
///    the failure is `SIGKILL` with no message, which is the worst kind. An
///    ad-hoc signature (`--sign -`) restores runnability without claiming an
///    identity; a project that ships this to other people signs it properly
///    afterwards with its own certificate. Bun needs none of this because it
///    writes its own headers.
///
/// # What uf cannot check before it runs this
///
/// Single-executable support is a *build-time* option of the `node` binary as
/// well as a version, and a distribution can ship a recent Node without it —
/// Homebrew's `node@25.8.1` has no SEA fuse in it at all and answers
/// `--build-sea` with `sentinel NODE_SEA_FUSE_… not found`. There is no cheap
/// way to ask a `node` whether it has one, so [`runtime`] checks the version
/// (which catches the common case precisely) and this forwards the rest with
/// the sentence that turns Node's message into an action.
fn wrap_with_node(
    runtime: &Runtime,
    root: &Utf8Path,
    work: &Utf8Path,
    bundle: &Utf8Path,
    binary: &Utf8Path,
) -> Result<()> {
    let config = work.join("sea-config.json");
    let mut payload = serde_json::json!({
        "main": bundle.as_str(),
        "output": binary.as_str(),
        "disableExperimentalSEAWarning": true,
        "mainFormat": "module",
    });
    if !runtime.exec_argv.is_empty() {
        payload["execArgv"] = serde_json::json!(runtime.exec_argv);
    }
    crate::support::write_json_file(&config, &payload)?;

    let output = Command::new(runtime.program.as_std_path())
        .arg("--build-sea")
        .arg(config.as_str())
        .current_dir(root.as_std_path())
        .output()
        .with_context(|| {
            format!(
                "failed to start {} for `uf build --compile`",
                runtime.program
            )
        })?;
    if !output.status.success() {
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!(
            "`node --build-sea` exited with {}\n{}\n\n\
             Single-executable support is compiled into a `node` binary as well as being a \
             version of it, and some distributions ship it turned off — `sentinel \
             NODE_SEA_FUSE_… not found` is what that looks like. Try the official build from \
             nodejs.org, or set `app.runtime.capabilityJsHost.default` to \"bun\" and compile \
             with Bun instead.",
            output.status,
            said.trim_end()
        );
    }

    sign_for_macos(binary)
}

/// Re-sign a freshly written Mach-O, on the platform that requires it.
///
/// A no-op everywhere else, and deliberately not an error when `codesign` is
/// absent: a macOS without the developer tools still has `/usr/bin/codesign`,
/// so a failure here is a real one and is reported as one.
#[cfg(target_os = "macos")]
fn sign_for_macos(binary: &Utf8Path) -> Result<()> {
    let output = Command::new("codesign")
        .args(["--sign", "-", "--force"])
        .arg(binary.as_str())
        .output()
        .context("failed to start `codesign`, which macOS needs to make a new binary runnable")?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "`codesign --sign -` on {binary} exited with {}\n{}\n\n\
         macOS refuses to execute a Mach-O whose signature no longer matches its contents, and \
         appending the application to a copy of `node` is exactly that change. The binary was \
         written and will not run until it is signed.",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim_end()
    )
}

#[cfg(not(target_os = "macos"))]
fn sign_for_macos(_binary: &Utf8Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
