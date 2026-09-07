//! `uf use` and `uf self-update`: the uf binary itself.
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
//! refusing an archive whose members escape their own directory, unpacking it
//! and linking the three binaries. `infra/cloudflare/setup-assets/install.sh`
//! does all six, it is what `curl -fsSL https://setup.uniflowed.dev | sh`
//! runs, and `tools/release/test-install.sh` proves it against a real packaged
//! release on every release build. So uf does not write a second one: it runs
//! that one, from [`INSTALLER`], by piping it into `sh` exactly the way the
//! documented install line does.
//!
//! The alternative was porting it to Rust, and it is worse in every direction
//! that matters here. uf links no HTTP client — `uf_pm::registry` and
//! `uf_env::archive` both shell out to `curl`, and that is the house pattern —
//! so a Rust port means a TLS stack in the binary to repeat what the operating
//! system already ships. It also means the tar guard, the checksum comparison
//! and the `latest` resolution existing twice, drifting independently, with
//! the shell copy still the one every new user's first command runs. One
//! implementation, exercised by the installer's own tests, is the whole point.
//!
//! The script is embedded at build time rather than fetched at run time. A
//! self-update that downloaded a shell script and ran it would be trusting the
//! network with code execution *before* any checksum is involved; embedding it
//! means `uf self-update` trusts exactly what `uf` was built from, and the
//! only thing that crosses the network is an archive whose digest is checked.
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

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_rm::{RuntimeReference, RuntimeUsePlan, RuntimeUseStep, XdgEnv, XdgLayout};
use uf_term::{KeyValue, Status, Tone};

use crate::support::{enabled, write_json_file};
use crate::ui::Ui;

/// The installer, as it stood when this binary was built.
///
/// The same bytes served at `https://setup.uniflowed.dev`, and the same bytes
/// `tools/release/test-install.sh` runs against a packaged release. See the
/// module documentation for why it is embedded rather than fetched, and why
/// there is no second acquisition path.
const INSTALLER: &str = include_str!("../../../../infra/cloudflare/setup-assets/install.sh");

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

    /// Where `uf@<version>` is unpacked, whoever unpacked it.
    fn version_dir(&self, version: &str) -> Utf8PathBuf {
        self.runtimes.join(format!("uf@{version}"))
    }

    /// The `uf` binary of an installed version, which is the file whose
    /// existence decides whether anything has to be downloaded.
    fn binary(&self, version: &str) -> Utf8PathBuf {
        self.version_dir(version)
            .join("bin")
            .join(if cfg!(windows) { "uf.exe" } else { "uf" })
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

/// `uf use uf@<version>`: make that version the one `uf` runs.
pub(crate) fn use_runtime(cwd: &Utf8Path, ui: &mut Ui, runtime: &str) -> Result<()> {
    let resolved = load_config(cwd)?;
    let requested = RuntimeReference::parse(runtime)
        .ok_or_else(|| anyhow!("runtime must look like uf@0.1.0"))?;
    if requested.name != RUNTIME_NAME {
        bail!(
            "uf use switches the uf toolchain, and {:?} is not uf\n\n  \
             the JavaScript host a project runs on is `host.runtime` in uf.config.js, \
             installed with `uf env install`",
            requested.name.as_str()
        );
    }
    let version = requested.version.to_string();
    let store = Store::from_process();

    let origin = if store.binary(&version).exists() {
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

    let auto_switch = resolved.config.rm.auto_switch;
    let mut plan = RuntimeUsePlan::new(requested, xdg_layout_from_process(), auto_switch);
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

    let runtime_label = format!("uf@{version}");
    let shim = report.shim.to_string();
    let state = report.active_runtime.to_string();
    let manifest = report.runtime_manifest.to_string();
    let binary = report.runtime_binary.to_string();
    let source = report.origin.as_str();
    let steps = plan
        .steps
        .iter()
        .map(|step| format!("{step:?}"))
        .collect::<Vec<_>>();
    let step_labels = steps.iter().map(String::as_str).collect::<Vec<_>>();
    let summary = format!("now using {runtime_label}");

    ui.render(|renderer, out| {
        renderer.banner(out, "uf use", Some(&runtime_label));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("auto switch", enabled(plan.auto_switch)),
                KeyValue::new("source", source),
                KeyValue::toned("shim", &shim, Tone::Path),
                KeyValue::toned("state", &state, Tone::Path),
                KeyValue::toned("manifest", &manifest, Tone::Path),
                KeyValue::toned("binary", &binary, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.heading(out, 2, "steps");
        renderer.bullet_list(out, 4, &step_labels);
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// `uf self-update`: resolve the newest release and switch to it.
///
/// The command `uf upgrade` was named after and never was
/// (ubugeeei-prod/uf#424, ubugeeei-prod/uf#499). It reads no project — a
/// broken `uf.config.js` is a reason to want a different uf, not a reason to
/// be unable to install one — so it takes no working directory and honours
/// only the installer's own variables: `UF_VERSION` to pin, `UF_RELEASE_BASE`
/// for a mirror, `UF_REPO`, `UF_INSTALL_ROOT` and `UF_BIN_DIR`.
pub(crate) fn self_update(ui: &mut Ui) -> Result<()> {
    let store = Store::from_process();
    let requested = variable("UF_VERSION").unwrap_or_else(|| "latest".to_owned());

    acquire(&store, &requested)?;
    // The installer resolved `latest`; asking it what it resolved to means
    // reading the link it just wrote, rather than parsing its prose.
    let version = installed_version(&store)?;
    let report = activate(&store, &version, Origin::acquired())?;

    let runtime_label = format!("uf@{version}");
    let was = if version == OWN_VERSION {
        format!("{OWN_VERSION} was already the newest")
    } else {
        format!("{OWN_VERSION} → {version}")
    };
    let shim = report.shim.to_string();
    let binary = report.runtime_binary.to_string();
    let manifest = report.runtime_manifest.to_string();
    let source = report.origin.as_str();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf self-update", Some(&runtime_label));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("version", &was),
                KeyValue::new("source", source),
                KeyValue::toned("shim", &shim, Tone::Path),
                KeyValue::toned("manifest", &manifest, Tone::Path),
                KeyValue::toned("binary", &binary, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &format!("now using {runtime_label}"));
    });
    Ok(())
}

/// Run the embedded installer for one version, or `latest`.
///
/// Piped into `sh` on stdin, which is what `curl … | sh` does, so the script
/// runs the way it is tested rather than the way a second caller invented. Its
/// output is the reader's: it draws the download, the checksum and the unpack
/// as they happen, and a failure it diagnoses — a version that does not exist,
/// a checksum that does not match, an archive whose members escape — is the
/// diagnosis a reader needs, not one paraphrased through here.
///
/// # Errors
///
/// When `sh` cannot be started, or the installer exits non-zero. On a platform
/// with no `sh`, before either.
#[cfg(unix)]
fn acquire(store: &Store, version: &str) -> Result<()> {
    let root = store
        .runtimes
        .parent()
        .ok_or_else(|| anyhow!("the runtime store {} has no parent", store.runtimes))?;

    let mut child = Command::new("sh")
        .env("UF_VERSION", version)
        .env("UF_INSTALL_ROOT", root.as_str())
        .env("UF_BIN_DIR", store.bin_dir.as_str())
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| "failed to run sh, which the uf installer is written in")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("sh accepted no script on stdin"))?
        .write_all(INSTALLER.as_bytes())
        .with_context(|| "failed to hand the installer to sh")?;
    let status = child
        .wait()
        .with_context(|| "failed to wait for the uf installer")?;

    if !status.success() {
        // The installer has already said what went wrong, in its own words and
        // with the fix; repeating a guess here would bury it.
        bail!("the uf installer could not install uf@{version}");
    }
    Ok(())
}

#[cfg(not(unix))]
fn acquire(_store: &Store, version: &str) -> Result<()> {
    bail!(
        "uf publishes no Windows build yet, so uf@{version} cannot be acquired here\n\n  \
         run uf under WSL2, or build from source:\n    \
         cargo install --git https://github.com/ubugeeei-prod/uf uf_cli"
    )
}

/// The version the installer just made active, read from the link it wrote.
fn installed_version(store: &Store) -> Result<String> {
    let link = store.bin_dir.join("uf");
    let target = fs::read_link(link.as_std_path())
        .with_context(|| format!("the installer linked no {link}"))?;
    let target = Utf8PathBuf::from_path_buf(target)
        .map_err(|path| anyhow!("the installer linked a non-UTF-8 path: {}", path.display()))?;

    // `<runtimes>/uf@<version>/bin/uf`, so the version is two directories up.
    target
        .parent()
        .and_then(Utf8Path::parent)
        .and_then(Utf8Path::file_name)
        .and_then(|name| name.strip_prefix("uf@"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{link} does not point into the uf runtime store: {target}"))
}

/// Install the running binary as its own version.
///
/// Only ever called when the requested version *is* [`OWN_VERSION`], which is
/// what makes it a copy rather than a rename. `ufr` and `ufx` are the same
/// binary with a different `argv[0]`, so all three are written.
fn install_running_binary(store: &Store, version: &str) -> Result<()> {
    let bin_dir = store.version_dir(version).join("bin");
    fs::create_dir_all(&bin_dir).with_context(|| format!("failed to create {bin_dir}"))?;

    let current_exe = std::env::current_exe().with_context(|| "failed to locate current uf")?;
    for name in BINARIES {
        let destination = bin_dir.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            (*name).to_owned()
        });
        fs::copy(&current_exe, destination.as_std_path()).with_context(|| {
            format!(
                "failed to install {destination} from {}",
                current_exe.display()
            )
        })?;
        mark_executable(&destination)?;
    }
    Ok(())
}

/// What activating a version wrote.
#[derive(Debug)]
struct RuntimeUseApplyReport {
    active_runtime: Utf8PathBuf,
    runtime_manifest: Utf8PathBuf,
    runtime_binary: Utf8PathBuf,
    shim: Utf8PathBuf,
    origin: Origin,
}

/// Point the machine's `uf` at `version`, and record what it is.
///
/// The version must already be in the store; everything that puts it there is
/// above. Linking rather than writing a shell wrapper is the installer's own
/// `ln -sfn`, and all three names move together — a `uf` that is version A
/// beside a `ufx` that is still version B is a machine nobody can reason
/// about.
fn activate(store: &Store, version: &str, origin: Origin) -> Result<RuntimeUseApplyReport> {
    let runtime_binary = store.binary(version);
    if !runtime_binary.exists() {
        bail!("uf@{version} is not installed: {runtime_binary} does not exist");
    }

    let runtime_manifest = store.version_dir(version).join("runtime.json");
    // A version the installer unpacked has no manifest, and inventing an
    // origin for it would be a guess written down as a fact. An existing one
    // is left alone: where a binary came from is settled when it arrives, and
    // activating it a second time does not change the answer.
    let origin = match (origin, recorded_origin(&runtime_manifest)) {
        (Origin::AlreadyInstalled, Some(recorded)) => recorded,
        (origin, _) => origin,
    };
    write_json_file(
        &runtime_manifest,
        &json!({
            "name": RUNTIME_NAME,
            "version": version,
            "binary": runtime_binary.as_str(),
            "source": origin.as_str(),
        }),
    )?;

    fs::create_dir_all(&store.bin_dir)
        .with_context(|| format!("failed to create {}", store.bin_dir))?;
    for name in BINARIES {
        link(
            &store.version_dir(version).join("bin").join(name),
            &store.bin_dir.join(name),
        )?;
    }

    fs::create_dir_all(&store.state_dir)
        .with_context(|| format!("failed to create {}", store.state_dir))?;
    let active_runtime = store.state_dir.join("active-runtime.json");
    write_json_file(
        &active_runtime,
        &json!({
            "name": RUNTIME_NAME,
            "version": version,
            "manifest": runtime_manifest.as_str(),
            "binary": runtime_binary.as_str(),
        }),
    )?;

    Ok(RuntimeUseApplyReport {
        active_runtime,
        runtime_manifest,
        runtime_binary,
        shim: store.bin_dir.join("uf"),
        origin,
    })
}

/// The `source` a manifest already records, when it is one uf writes.
///
/// Unreadable, unparsable and unrecognised all answer `None`, because the
/// point of reading it is to avoid overwriting a fact with a guess — and a
/// file uf cannot read is not a fact.
fn recorded_origin(manifest: &Utf8Path) -> Option<Origin> {
    let contents = fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    match value.get("source")?.as_str()? {
        "release" => Some(Origin::Release),
        "mirror" => Some(Origin::Mirror),
        "running-binary" => Some(Origin::RunningBinary),
        _ => None,
    }
}

/// Link `target` at `path`, replacing whatever was there.
///
/// `ln -sfn`, spelled out: the installer's own line, so a machine cannot end
/// up with `uf` a symlink and `ufx` a shell script depending on which of the
/// two last ran.
#[cfg(unix)]
fn link(target: &Utf8Path, path: &Utf8Path) -> Result<()> {
    if !target.exists() {
        bail!("the runtime is missing {target}");
    }
    // `symlink` refuses an existing path, and a `uf` already linked to the
    // previous version is the normal case rather than an error.
    match fs::remove_file(path.as_std_path()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to replace {path}"));
        }
    }
    std::os::unix::fs::symlink(target.as_std_path(), path.as_std_path())
        .with_context(|| format!("failed to link {path} to {target}"))
}

/// A batch file that runs the versioned binary, for a platform with no
/// symlink an unprivileged user may create.
#[cfg(not(unix))]
fn link(target: &Utf8Path, path: &Utf8Path) -> Result<()> {
    let target = target.with_extension("exe");
    if !target.exists() {
        bail!("the runtime is missing {target}");
    }
    let path = path.with_extension("cmd");
    fs::write(&path, format!("@echo off\r\n\"{target}\" %*\r\n"))
        .with_context(|| format!("failed to write {path}"))
}

#[cfg(unix)]
fn mark_executable(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {path}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to update permissions for {path}"))
}

#[cfg(not(unix))]
fn mark_executable(_path: &Utf8Path) -> Result<()> {
    Ok(())
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
