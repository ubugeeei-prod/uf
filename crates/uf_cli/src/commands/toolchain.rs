//! `uf use`, `uf self-update` and `uf self-uninstall`: the uf binary itself —
//! and following a project's `uf: "<version>"` to the release it pins, in
//! [`pin`].
//!
//! Everything here is about *uf*, not about a project's dependencies — those
//! are [`super::pm`] — and not about the JavaScript hosts a project runs on,
//! which are `uf env`. The one question this module answers is which build of
//! uf `~/.local/bin/uf` points at, and how a build that is not on the machine
//! yet gets there.
//!
//! # Acquisition is the installer's, and only the installer's
//!
//! `uf use uf@0.9.9` used to copy [`std::env::current_exe`] into a directory
//! called `0.9.9` and write three files saying that is what it was. Nothing
//! was fetched, nothing was resolved and nothing was verified, so the only
//! honest thing in the manifest was a `"source": "current-exe"` field nobody
//! reads (ubugeeei-prod/uf#534).
//!
//! Acquiring a release means resolving a version, downloading
//! `uf-<target>.tar.gz`, checking it against the sha256 published beside it,
//! refusing an archive whose members escape their own directory, and unpacking
//! it. `infra/cloudflare/setup-assets/install.sh` and `install.ps1` do all
//! five, and `tools/release/test-install.sh` plus `test-install.ps1` prove them
//! against a real packaged release on every release build. So uf does not write
//! a second one: it runs the installer for the host it is on, from
//! `INSTALLER_SH` or `INSTALLER_POWERSHELL`, the way the documented install
//! line does.
//!
//! The alternative was porting it to Rust, and it is worse in every direction
//! that matters here. uf links no HTTP client — `uf_pm::registry` and
//! `uf_env::archive` both shell out to `curl`, and that is the house pattern —
//! so a Rust port means a TLS stack in the binary to repeat what the operating
//! system already ships. It also means the tar guard, the checksum comparison
//! and the `latest` resolution existing twice, drifting independently, with
//! the shell copy still the one every new user's first command runs. One
//! implementation, exercised by the installer's own tests, is the whole point.
//! That is also why `uf self-update --check` asks the installer which release
//! is newest rather than asking GitHub itself: `UF_STOP_AFTER=resolve` makes
//! it answer and stop.
//!
//! The script is embedded at build time rather than fetched at run time. A
//! self-update that downloaded a shell script and ran it would be trusting the
//! network with code execution *before* any checksum is involved; embedding it
//! means `uf self-update` trusts exactly what `uf` was built from, and the
//! only thing that crosses the network is an archive whose digest is checked.
//!
//! # Switching is uf's
//!
//! The sixth step the installer takes for a person, linking `uf`, `ufr` and
//! `ufx`, uf takes itself: it runs the installer with `UF_STOP_AFTER=unpack`
//! and switches the links in [`switch`], where a process killed at any point
//! leaves every name pointing at a complete runtime. Both make the switch the
//! same way — one rename per name, `uf` last — and both record the version
//! they switched away from in `<root>/previous-version`, which is what
//! `uf self-update --rollback` reads. Nothing uf switches away from is
//! deleted, so a rollback needs no network.
//!
//! # The store is the installer's too
//!
//! `${UF_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/uf}/runtimes`,
//! holding `uf@<version>/bin/{uf,ufr,ufx}`, with `${UF_BIN_DIR:-$HOME/.local/bin}`
//! linking to the active one. That is where `curl | sh` puts a release, so it
//! is where `uf use` looks for one and where it activates from. A second store
//! under a second name — which is what `uf use` wrote into before — is a
//! directory `uf use` fills and the installer never reads, and a
//! `~/.local/bin/uf` the two of them take turns overwriting with different
//! kinds of file.

mod pin;
mod switch;
mod uninstall;

pub(crate) use pin::follow as follow_pin;

pub(crate) use uninstall::self_uninstall;

use std::cmp::Ordering;
use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_rm::{RuntimeReference, RuntimeUsePlan, RuntimeUseStep, XdgEnv, XdgLayout};
use uf_term::{KeyValue, Status, Tone};

use crate::ui::Ui;

use switch::{
    Step, Switched, active_version, binary_file, install_running_binary, is_executable_file,
    links_agree, recorded_previous, switch_to,
};

/// The installer, as it stood when this binary was built.
///
/// The same bytes served at `https://setup.uniflowed.dev`, and the same bytes
/// the installer tests run against a packaged release. See the module
/// documentation for why they are embedded rather than fetched, and why there
/// is no second acquisition path.
#[cfg(any(test, not(windows)))]
const INSTALLER_SH: &str = include_str!("../../../../infra/cloudflare/setup-assets/install.sh");
#[cfg(windows)]
const INSTALLER_POWERSHELL: &str =
    include_str!("../../../../infra/cloudflare/setup-assets/install.ps1");

/// The version this binary is, which is the only version it can honestly
/// install a copy of itself as.
const OWN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The three binaries a uf release ships, and the three the installer links.
const BINARIES: &[&str] = &["uf", "ufr", "ufx"];

/// The only runtime `uf use` switches.
///
/// The host a project's JavaScript runs on is `host.runtime` in
/// `uf.config.js`, installed by `uf env install`. `uf use node@22` used to be
/// accepted and produced a directory called `node/22` holding a copy of the
/// uf binary, which is the same lie as #534 with a second name on it.
const RUNTIME_NAME: &str = "uf";

/// Where a runtime came from, as `runtime.json` records it.
///
/// The field exists because the three are not interchangeable and a reader
/// checking a machine needs to tell them apart: a release is what the
/// publisher signed for, a mirror is whatever `UF_RELEASE_BASE` pointed at,
/// and a copy of the running binary is a build with no provenance beyond
/// whoever ran the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Downloaded from GitHub Releases and checksummed by the installer.
    Release,
    /// Downloaded from `UF_RELEASE_BASE` and checksummed by the installer.
    Mirror,
    /// Copied from the running binary, whose version is the one requested.
    RunningBinary,
    /// Already in the store when uf was asked for it, with nothing recorded
    /// about how it got there — what a `curl … | sh` install leaves behind.
    AlreadyInstalled,
}

impl Origin {
    /// Whether this origin means an archive crossed the network.
    ///
    /// Which is what decides whether `uf use` claims to have downloaded and
    /// checksummed anything — the claim #534 was about.
    const fn downloaded(self) -> bool {
        matches!(self, Self::Release | Self::Mirror)
    }

    /// What a fresh download's origin is, given the environment it ran in.
    fn acquired() -> Self {
        match variable("UF_RELEASE_BASE") {
            Some(_) => Self::Mirror,
            None => Self::Release,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Mirror => "mirror",
            Self::RunningBinary => "running-binary",
            Self::AlreadyInstalled => "already-installed",
        }
    }
}

/// The installer's directories, read from the installer's own variables.
///
/// Not [`XdgLayout`] alone: `UF_INSTALL_ROOT` and `UF_BIN_DIR` are the
/// installer's contract rather than XDG's, and they are how
/// `tools/release/test-install.sh` — and the tests here — install somewhere
/// that is not the developer's own `~/.local`.
#[derive(Debug, Clone)]
struct Store {
    /// `<root>/runtimes`, one directory per installed version.
    runtimes: Utf8PathBuf,
    /// Where `uf`, `ufr` and `ufx` are linked from.
    bin_dir: Utf8PathBuf,
    /// Where the active version is recorded.
    state_dir: Utf8PathBuf,
}

impl Store {
    /// The store this process would install into.
    fn from_process() -> Self {
        let layout = xdg_layout_from_process();
        let runtimes = match variable("UF_INSTALL_ROOT") {
            Some(root) => Utf8PathBuf::from(root).join("runtimes"),
            None => Utf8PathBuf::from(layout.versions_dir.as_str()),
        };
        let bin_dir = variable("UF_BIN_DIR").map_or_else(
            || Utf8PathBuf::from(layout.bin_dir.as_str()),
            Utf8PathBuf::from,
        );
        Self {
            runtimes,
            bin_dir,
            state_dir: Utf8PathBuf::from(layout.state_dir.as_str()),
        }
    }

    /// What `UF_INSTALL_ROOT` names: the runtimes, and the record beside them.
    fn root(&self) -> &Utf8Path {
        self.runtimes.parent().unwrap_or(&self.runtimes)
    }

    /// Where the version the last switch replaced is recorded.
    ///
    /// One version on one line, beside the runtimes rather than in the state
    /// directory with `active-runtime.json`, because the installer switches
    /// versions too and has to write it: a person who updated with
    /// `curl … | sh` rolls back to what that replaced, not to whatever uf
    /// itself last switched away from.
    fn previous_record(&self) -> Utf8PathBuf {
        self.root().join("previous-version")
    }

    /// Where `uf@<version>` is unpacked, whoever unpacked it.
    fn version_dir(&self, version: &str) -> Utf8PathBuf {
        self.runtimes
            .join(uf_infra::into_string(uf_infra::cstr!("uf@{version}")))
    }

    /// The `uf` binary of an installed version.
    fn binary(&self, version: &str) -> Utf8PathBuf {
        self.version_dir(version)
            .join("bin")
            .join(binary_file("uf"))
    }

    /// Whether all three binaries of `version` are in the store, and run.
    ///
    /// All three, and not `uf` alone: a runtime missing `ufx` is one a switch
    /// would leave `ufx` dangling into, and the installer refuses to unpack
    /// such a build for the same reason.
    fn has_complete(&self, version: &str) -> bool {
        let bin = self.version_dir(version).join("bin");
        BINARIES
            .iter()
            .all(|name| is_executable_file(&bin.join(binary_file(name))))
    }
}

/// An environment variable that was set to something.
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(set_to_something)
}

/// A variable set to nothing is a variable nobody set.
///
/// `UF_INSTALL_ROOT=` is what a CI environment looks like when the value was
/// meant to come from a secret that is not there. Reading it as a path
/// resolves the store to `/runtimes`, and the installer — which spells the
/// same default with `${UF_INSTALL_ROOT:-…}`, colon included — would not have
/// agreed.
fn set_to_something(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// Whether `text` can be a uf version: what a release tag carries after `uf@`.
///
/// Checked wherever a version arrives from outside — an argument, the
/// installer's answer, a record on disk — because it becomes a directory name
/// and a URL segment, and a version of `../bin` would be both somewhere else.
fn is_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && !text.starts_with(['.', '-'])
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
}

/// `uf use uf@<version>`: make that version the one `uf` runs.
///
/// Nothing in the project decides any of it, so the project's config is not
/// read: `rm.autoSwitch` was, to print a row that changed nothing, and it went
/// with the rest of `rm` (ubugeeei-prod/uf#1387).
pub(crate) fn use_runtime(ui: &mut Ui, runtime: &str) -> Result<()> {
    let requested = RuntimeReference::parse(runtime)
        .ok_or_else(|| anyhow!(uf_infra::cstr!("runtime must look like uf@0.1.0")))?;
    if requested.name != RUNTIME_NAME {
        bail!(uf_infra::cstr!(
            "uf use switches the uf toolchain, and {:?} is not uf\n\n  \
             the JavaScript host a project runs on is `host.runtime` in uf.config.js, \
             installed with `uf env install`",
            requested.name.as_str()
        ));
    }
    let version = requested.version.to_string();
    if !is_version(&version) {
        bail!(uf_infra::cstr!(
            "{version:?} is not a uf version: a release is named like uf@0.1.0"
        ));
    }
    let store = Store::from_process();

    let origin = if store.has_complete(&version) {
        Origin::AlreadyInstalled
    } else if version == OWN_VERSION {
        // The one case where copying the running binary is not a lie: the
        // version being asked for is the version that is running. A uf built
        // from source, or installed by something other than the installer,
        // reaches its own store this way with no network at all.
        install_running_binary(&store, &version)?;
        Origin::RunningBinary
    } else {
        acquire(&store, &version)?;
        Origin::acquired()
    };

    let mut plan = RuntimeUsePlan::new(requested, xdg_layout_from_process());
    // The plan `uf_rm` declares is the whole ladder; what gets printed is the
    // rungs this run climbed. A version already on the machine downloads and
    // verifies nothing, and printing that it did is the shape of the bug this
    // command had — six confident step names over a `fs::copy`.
    plan.steps.retain(|step| match step {
        RuntimeUseStep::DownloadRuntime | RuntimeUseStep::VerifyChecksum => origin.downloaded(),
        RuntimeUseStep::InstallVersion => origin != Origin::AlreadyInstalled,
        RuntimeUseStep::ResolveVersion
        | RuntimeUseStep::WriteShim
        | RuntimeUseStep::ActivateVersion => true,
    });
    let report = activate(&store, &version, origin)?;

    let runtime_label = uf_infra::into_string(uf_infra::cstr!("uf@{version}"));
    let shim = report.shim.to_string();
    let state = report.active_runtime.to_string();
    let manifest = report.runtime_manifest.to_string();
    let binary = report.runtime_binary.to_string();
    let source = report.origin.as_str();
    let kept = kept(&report);
    let steps = plan
        .steps
        .iter()
        .map(|step| uf_infra::into_string(uf_infra::cstr!("{step:?}")))
        .collect::<Vec<_>>();
    let step_labels = steps.iter().map(String::as_str).collect::<Vec<_>>();
    let summary = uf_infra::into_string(uf_infra::cstr!("now using {runtime_label}"));

    let mut rows = vec![
        KeyValue::new("source", source),
        KeyValue::toned("shim", &shim, Tone::Path),
        KeyValue::toned("state", &state, Tone::Path),
        KeyValue::toned("manifest", &manifest, Tone::Path),
        KeyValue::toned("binary", &binary, Tone::Path),
    ];
    if let Some(kept) = &kept {
        rows.push(KeyValue::new("kept", kept));
    }
    ui.render(|renderer, out| {
        renderer.banner(out, "uf use", Some(&runtime_label));
        renderer.key_values(out, 2, &rows);
        renderer.blank(out);
        renderer.heading(out, 2, "steps");
        renderer.bullet_list(out, 4, &step_labels);
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// What `uf self-update` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelfUpdate<'a> {
    /// Install the newest release, or the one named, and switch to it.
    Install(Option<&'a str>),
    /// Say whether a newer release exists, and change nothing.
    Check,
    /// Switch back to the version the last switch replaced.
    Rollback,
}

/// `uf self-update`: install a release and switch to it, say whether there is
/// one to install, or switch back.
///
/// The command `uf upgrade` was named after and never was
/// (ubugeeei-prod/uf#424, ubugeeei-prod/uf#499). It reads no project — a
/// broken `uf.config.js` is a reason to want a different uf, not a reason to
/// be unable to install one — so it takes no working directory and honours
/// only the installer's own variables: `UF_VERSION` to pin, `UF_RELEASE_BASE`
/// for a mirror, `UF_REPO`, `UF_INSTALL_ROOT` and `UF_BIN_DIR`.
pub(crate) fn self_update(ui: &mut Ui, action: SelfUpdate<'_>) -> Result<()> {
    let store = Store::from_process();
    match action {
        SelfUpdate::Install(named) => update(ui, &store, named),
        SelfUpdate::Check => check(ui, &store),
        SelfUpdate::Rollback => roll_back(ui, &store),
    }
}

/// `uf self-update [VERSION]`.
fn update(ui: &mut Ui, store: &Store, named: Option<&str>) -> Result<()> {
    let requested = named.map_or_else(
        || variable("UF_VERSION").unwrap_or_else(|| "latest".to_owned()),
        ToOwned::to_owned,
    );
    let requested = requested.strip_prefix("uf@").unwrap_or(&requested);
    let version = if requested == "latest" {
        resolve(store, requested)?
    } else {
        requested.to_owned()
    };
    if !is_version(&version) {
        bail!(uf_infra::cstr!(
            "{version:?} is not a uf version: a release is named like 0.0.0-alpha.35, \
             or uf@0.0.0-alpha.35"
        ));
    }
    let runtime_label = uf_infra::into_string(uf_infra::cstr!("uf@{version}"));
    let active = active_version(store);

    if active.as_deref() == Some(version.as_str())
        && store.has_complete(&version)
        && links_agree(store, &version)
    {
        let shim = store.bin_dir.join("uf").to_string();
        let binary = store.binary(&version).to_string();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf self-update", Some(&runtime_label));
            renderer.key_values(
                out,
                2,
                &[
                    KeyValue::toned("shim", &shim, Tone::Path),
                    KeyValue::toned("binary", &binary, Tone::Path),
                ],
            );
            renderer.blank(out);
            renderer.status(
                out,
                Status::Success,
                &uf_infra::cstr!("{runtime_label} is already active; nothing changed")
                    .into_string(),
            );
        });
        return Ok(());
    }

    let origin = if store.has_complete(&version) {
        // Switched away from earlier and kept, so there is nothing to fetch.
        Origin::AlreadyInstalled
    } else {
        acquire(store, &version)?;
        Origin::acquired()
    };
    let report = activate(store, &version, origin)?;

    let from = match &active {
        Some(active) => uf_infra::into_string(uf_infra::cstr!("uf@{active}")),
        None => {
            uf_infra::cstr!("uf@{OWN_VERSION}, this binary; no runtime in the store was active")
                .into_string()
        }
    };
    let shim = report.shim.to_string();
    let binary = report.runtime_binary.to_string();
    let manifest = report.runtime_manifest.to_string();
    let source = report.origin.as_str();
    let kept = kept(&report);

    let mut rows = vec![
        KeyValue::new("from", &from),
        KeyValue::new("source", source),
        KeyValue::toned("shim", &shim, Tone::Path),
        KeyValue::toned("manifest", &manifest, Tone::Path),
        KeyValue::toned("binary", &binary, Tone::Path),
    ];
    if let Some(kept) = &kept {
        rows.push(KeyValue::new("kept", kept));
    }
    ui.render(|renderer, out| {
        renderer.banner(out, "uf self-update", Some(&runtime_label));
        renderer.key_values(out, 2, &rows);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            uf_infra::cstr!("now using {runtime_label}").as_str(),
        );
    });
    Ok(())
}

/// `uf self-update --check`: the newest release beside the active one.
///
/// Always the newest, whatever `UF_VERSION` pins: the question is whether
/// something newer exists, and a pin is an answer to a different one.
fn check(ui: &mut Ui, store: &Store) -> Result<()> {
    let newest = resolve(store, "latest")?;
    let (current, active) = match active_version(store) {
        Some(version) => {
            let detail = uf_infra::cstr!("uf@{version}, linked at {}", store.bin_dir.join("uf"))
                .into_string();
            (version, detail)
        }
        None => (
            OWN_VERSION.to_owned(),
            uf_infra::cstr!("uf@{OWN_VERSION}, this binary; no runtime in the store is active")
                .into_string(),
        ),
    };
    let newest_label = uf_infra::into_string(uf_infra::cstr!("uf@{newest}"));

    let (status, summary) = match order(&newest, &current) {
        Some(Ordering::Greater) => (
            Status::Info,
            uf_infra::into_string(uf_infra::cstr!(
                "{newest_label} is newer; `uf self-update` installs it"
            )),
        ),
        Some(Ordering::Less) => (
            Status::Info,
            uf_infra::cstr!("uf@{current} is newer than the newest release, {newest_label}")
                .into_string(),
        ),
        Some(Ordering::Equal) => (
            Status::Success,
            uf_infra::into_string(uf_infra::cstr!("uf@{current} is the newest release")),
        ),
        None if newest == current => (
            Status::Success,
            uf_infra::into_string(uf_infra::cstr!("uf@{current} is the newest release")),
        ),
        None => (
            Status::Info,
            uf_infra::cstr!(
                "the newest release is {newest_label}, and uf cannot order it against uf@{current}"
            )
            .into_string(),
        ),
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf self-update --check", None);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("active", &active),
                KeyValue::new("newest", &newest_label),
            ],
        );
        renderer.blank(out);
        renderer.status(out, status, &summary);
    });
    Ok(())
}

/// Semver precedence between two versions, when both are versions.
///
/// `alpha.10` is newer than `alpha.9`, which a string comparison gets the
/// other way round — the mistake that offers a downgrade as an update.
fn order(left: &str, right: &str) -> Option<Ordering> {
    Some(uf_pm::Version::parse(left)?.cmp(&uf_pm::Version::parse(right)?))
}

/// `uf self-update --rollback`: switch back, from the store, offline.
fn roll_back(ui: &mut Ui, store: &Store) -> Result<()> {
    let record = store.previous_record();
    let Some(previous) = recorded_previous(store) else {
        bail!(uf_infra::cstr!(
            "there is no version to roll back to: {record} names none\n\n  \
             it is written when `uf self-update`, `uf use` or the installer switches uf \
             from one runtime in the store to another"
        ));
    };
    let active = active_version(store);
    if active.as_deref() == Some(previous.as_str()) {
        bail!(uf_infra::cstr!(
            "uf@{previous} is the version recorded to roll back to, and it is already active\n\n  \
             `uf self-update <version>` switches to any other release"
        ));
    }
    if !store.has_complete(&previous) {
        bail!(uf_infra::cstr!(
            "uf@{previous} is the version to roll back to, and {} no longer holds all of it\n\n  \
             `uf self-update {previous}` downloads it again",
            store.version_dir(&previous)
        ));
    }

    let report = activate(store, &previous, Origin::AlreadyInstalled)?;
    let runtime_label = uf_infra::into_string(uf_infra::cstr!("uf@{previous}"));
    let from = active.map_or_else(
        || "no runtime in the store".to_owned(),
        |active| uf_infra::into_string(uf_infra::cstr!("uf@{active}")),
    );
    let shim = report.shim.to_string();
    let binary = report.runtime_binary.to_string();
    let kept = kept(&report);

    let mut rows = vec![
        KeyValue::new("from", &from),
        KeyValue::toned("shim", &shim, Tone::Path),
        KeyValue::toned("binary", &binary, Tone::Path),
    ];
    if let Some(kept) = &kept {
        rows.push(KeyValue::new("kept", kept));
    }
    ui.render(|renderer, out| {
        renderer.banner(out, "uf self-update --rollback", Some(&runtime_label));
        renderer.key_values(out, 2, &rows);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            uf_infra::cstr!("rolled back to {runtime_label}").as_str(),
        );
    });
    Ok(())
}

/// The line saying which version a switch kept for `--rollback`, if any.
fn kept(report: &Switched) -> Option<String> {
    report.replaced.as_ref().map(|replaced| {
        uf_infra::into_string(uf_infra::cstr!(
            "uf@{replaced}, which `uf self-update --rollback` returns to"
        ))
    })
}

/// Point the machine's `uf` at `version`, and record what it is and what it
/// replaced. See [`switch`] for what a kill at any point leaves.
fn activate(store: &Store, version: &str, origin: Origin) -> Result<Switched> {
    switch_to(store, version, origin, &mut |_: Step| Ok(()))
}

/// How far the installer goes before it hands back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopAfter {
    /// Print the version `UF_VERSION` resolves to, and download nothing.
    Resolve,
    /// Download, verify and unpack into the store, and link nothing.
    Unpack,
}

impl StopAfter {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Unpack => "unpack",
        }
    }
}

/// The version the installer resolves `requested` to.
fn resolve(store: &Store, requested: &str) -> Result<String> {
    let printed = run_installer(store, requested, StopAfter::Resolve)?;
    let version = printed.trim();
    if !is_version(version) {
        bail!(uf_infra::cstr!(
            "the uf installer resolved uf@{requested} to {version:?}, which is not a version"
        ));
    }
    Ok(version.to_owned())
}

/// Download, verify and unpack `version` into the store, linking nothing.
fn acquire(store: &Store, version: &str) -> Result<()> {
    run_installer(store, version, StopAfter::Unpack)?;
    if !store.has_complete(version) {
        bail!(uf_infra::cstr!(
            "the uf installer reported success, and {} does not hold uf, ufr and ufx",
            store.version_dir(version)
        ));
    }
    Ok(())
}

/// Run the embedded installer for one version, or `latest`, as far as
/// `stop_after`, and return what it printed on stdout.
///
/// Piped into `sh` on stdin, which is what `curl … | sh` does, so the script
/// runs the way it is tested rather than the way a second caller invented. Its
/// stderr is the reader's: it draws the download, the checksum and the unpack
/// as they happen, and a failure it diagnoses — a version that does not exist,
/// a checksum that does not match, an archive whose members escape — is the
/// diagnosis a reader needs, not one paraphrased through here. Its stdout
/// carries nothing but a resolution's answer.
///
/// # Errors
///
/// When `sh` cannot be started, or the installer exits non-zero. On a platform
/// with no `sh`, before either.
#[cfg(unix)]
fn run_installer(store: &Store, version: &str, stop_after: StopAfter) -> Result<String> {
    let mut child = Command::new("sh")
        .env("UF_VERSION", version)
        .env("UF_INSTALL_ROOT", store.root().as_str())
        .env("UF_BIN_DIR", store.bin_dir.as_str())
        .env("UF_STOP_AFTER", stop_after.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| "failed to run sh, which the uf installer is written in")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!(uf_infra::cstr!("sh accepted no script on stdin")))?;
    // A script that stops early — at a resolution, or at a failure it has
    // already explained — may close its end before reading the rest. What
    // happened is in its exit status, not in the pipe.
    match stdin.write_all(INSTALLER_SH.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => return Err(error).with_context(|| "failed to hand the installer to sh"),
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .with_context(|| "failed to wait for the uf installer")?;

    if !output.status.success() {
        // The installer has already said what went wrong, in its own words and
        // with the fix; repeating a guess here would bury it.
        let verb = match stop_after {
            StopAfter::Resolve => "resolve",
            StopAfter::Unpack => "install",
        };
        bail!(uf_infra::cstr!(
            "the uf installer could not {verb} uf@{version}"
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        anyhow!(uf_infra::cstr!(
            "the uf installer printed a version that is not UTF-8"
        ))
    })
}

#[cfg(windows)]
fn run_installer(store: &Store, version: &str, stop_after: StopAfter) -> Result<String> {
    let spawn = |shell: &str| {
        Command::new(shell)
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "-"])
            .env("UF_VERSION", version)
            .env("UF_INSTALL_ROOT", store.root().as_str())
            .env("UF_BIN_DIR", store.bin_dir.as_str())
            .env("UF_STOP_AFTER", stop_after.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
    };
    let mut child = spawn("pwsh")
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                spawn("powershell.exe")
            } else {
                Err(error)
            }
        })
        .with_context(|| "failed to run PowerShell for the Windows uf installer")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!(uf_infra::cstr!("pwsh accepted no script on stdin")))?;
    match stdin.write_all(INSTALLER_POWERSHELL.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => return Err(error).with_context(|| "failed to hand the installer to pwsh"),
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .with_context(|| "failed to wait for the uf installer")?;

    if !output.status.success() {
        let verb = match stop_after {
            StopAfter::Resolve => "resolve",
            StopAfter::Unpack => "install",
        };
        bail!(uf_infra::cstr!(
            "the uf installer could not {verb} uf@{version}"
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        anyhow!(uf_infra::cstr!(
            "the uf installer printed a version that is not UTF-8"
        ))
    })
}

#[cfg(not(any(unix, windows)))]
fn run_installer(_store: &Store, version: &str, _stop_after: StopAfter) -> Result<String> {
    bail!(uf_infra::cstr!(
        "uf publishes no build for this platform yet, so uf@{version} cannot be acquired here\n\n  \
         build from source instead:\n    \
         cargo install --git https://github.com/ubugeeei-prod/uf uf_cli"
    ))
}

/// The `source` a manifest already records, when it is one uf writes.
///
/// Unreadable, unparsable and unrecognised all answer `None`, because the
/// point of reading it is to avoid overwriting a fact with a guess — and a
/// file uf cannot read is not a fact.
fn recorded_origin(manifest: &Utf8Path) -> Option<Origin> {
    let contents = std::fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    match value.get("source")?.as_str()? {
        "release" => Some(Origin::Release),
        "mirror" => Some(Origin::Mirror),
        "running-binary" => Some(Origin::RunningBinary),
        _ => None,
    }
}

fn xdg_layout_from_process() -> XdgLayout {
    let home = std::env::var("HOME").unwrap_or_else(|_| "$HOME".to_string());
    let config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let data_home = std::env::var("XDG_DATA_HOME").ok();
    let cache_home = std::env::var("XDG_CACHE_HOME").ok();
    let state_home = std::env::var("XDG_STATE_HOME").ok();
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

    XdgLayout::from_env(XdgEnv {
        home: &home,
        config_home: config_home.as_deref(),
        data_home: data_home.as_deref(),
        cache_home: cache_home.as_deref(),
        state_home: state_home.as_deref(),
        runtime_dir: runtime_dir.as_deref(),
    })
}

#[cfg(test)]
mod tests;
