//! `uf sqlc`, and `uf` as sqlc's Flow plugin.
//!
//! sqlc owns the SQL: its parser, its catalog, its config file. uf owns the
//! Flow it becomes. The two meet at sqlc's process-plugin protocol, so a
//! project's `sqlc.yaml` names `uf` as the plugin —
//!
//! ```yaml
//! plugins:
//!   - name: flow
//!     process:
//!       cmd: uf
//! ```
//!
//! — and sqlc runs `uf /plugin.CodegenService/Generate` with a
//! `GenerateRequest` on stdin. [`plugin`] answers that; `crates/uf_sqlc` does
//! the work, and this file adds `uf fmt`'s printer on the way out.
//!
//! `uf sqlc generate` and `uf sqlc diff` are `sqlc generate` and `sqlc diff`
//! with one difference: the directory of the running `uf` is put first on
//! `PATH`, so the `uf` sqlc finds is this one rather than whichever an older
//! install left earlier on the path. sqlc itself is resolved as `$SQLC`, then
//! `sqlc` on `PATH`, then `.uf/sqlc/sqlc` when `uf sqlc install` put the
//! pinned release there. `docs/sqlc.md` is the design record.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use uf_term::Status;

/// The sqlc release `tests/sqlc` was captured against.
///
/// `tools/ci/install-sqlc.sh` installs this same release. A different sqlc
/// sends a different plugin request, and `node tests/sqlc/capture.mjs --check`
/// fails, which is the signal to move the pin and the fixtures together.
const PINNED_SQLC_VERSION: &str = "1.31.1";

/// One platform's archive of [`PINNED_SQLC_VERSION`].
///
/// The digests are the `sha256:` GitHub published on the release assets.
struct PinnedArchive {
    os: &'static str,
    arch: &'static str,
    /// `sqlc_<version>_<platform>.tar.gz`.
    platform: &'static str,
    sha256: &'static str,
    /// The file inside the archive.
    binary: &'static str,
}

const PINNED_ARCHIVES: &[PinnedArchive] = &[
    PinnedArchive {
        os: "macos",
        arch: "aarch64",
        platform: "darwin_arm64",
        sha256: "21602158c99eb1f2bae197a66abfb1941d1e9e50b23125bb193349c6b1acc71e",
        binary: "sqlc",
    },
    PinnedArchive {
        os: "macos",
        arch: "x86_64",
        platform: "darwin_amd64",
        sha256: "c5af76772e3785d21663a62697056b383f07629979b1bd25b93872e73dbd519b",
        binary: "sqlc",
    },
    PinnedArchive {
        os: "linux",
        arch: "x86_64",
        platform: "linux_amd64",
        sha256: "497ae4fcdfa64c5b0c311ffe4c2bd991e43991e82e5367792ed78bc2dca27354",
        binary: "sqlc",
    },
    PinnedArchive {
        os: "linux",
        arch: "aarch64",
        platform: "linux_arm64",
        sha256: "b7cae247740d0c51a1e657479e5b2d21e6fef428f596682a01bc55bf4ab8a23d",
        binary: "sqlc",
    },
    PinnedArchive {
        os: "windows",
        arch: "x86_64",
        platform: "windows_amd64",
        sha256: "40d138ec18b1cc80d2be7305917fd4deceda4e0c32d78ba5d8faa4bfa3bc0fc0",
        binary: "sqlc.exe",
    },
    PinnedArchive {
        os: "windows",
        arch: "aarch64",
        platform: "windows_arm64",
        sha256: "aa9b926313f922a2c7c668076dd5a2754724d61f528c1d5ef033bed11b27eccd",
        binary: "sqlc.exe",
    },
];

use crate::cli::SqlcCommand;
use crate::ui::Ui;

/// Answer one sqlc plugin call: a request on stdin, a response on stdout.
///
/// Failures go to stderr, which sqlc prints under the query file's name, and
/// exit non-zero, which is how sqlc knows to stop.
pub(crate) fn plugin() -> ExitCode {
    let mut input = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut input) {
        eprintln!("uf: could not read sqlc's request: {error}");
        return ExitCode::FAILURE;
    }
    match uf_sqlc::run_plugin(&input, &format) {
        Ok(output) => match std::io::stdout().write_all(&output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("uf: could not write the response to sqlc: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("uf: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `uf fmt`'s printer with stable generator layout.
///
/// Not the project's `fmt` settings: the plugin runs wherever sqlc runs, with
/// the environment sqlc clears, and the output must be the same for everyone
/// who runs the same sqlc over the same SQL. The files are signed, and
/// `uf fmt` leaves a signed file alone, so a project that formats differently
/// never sees them change.
fn format(source: &str) -> std::result::Result<String, String> {
    let mut config = uf_config::FmtConfig::default();
    // Existing generated files remain byte-for-byte reproducible across a
    // formatter release; alignment is for author-written Flow source.
    config.align = false;
    uf_fmt::format_source(source, &config)
        .map(|result| result.output)
        .map_err(|error| error.to_string())
}

pub(crate) fn sqlc(cwd: &Utf8Path, ui: &mut Ui, command: SqlcCommand) -> Result<()> {
    let (verb, file) = match command {
        SqlcCommand::Generate { file } => ("generate", file),
        SqlcCommand::Diff { file } => ("diff", file),
        SqlcCommand::Install {
            version,
            sha256,
            force,
        } => {
            return install(
                cwd,
                ui,
                version.as_deref(),
                sha256.as_deref(),
                force,
            );
        }
    };
    let sqlc = resolve_sqlc(cwd)?;
    let mut run = Command::new(&sqlc);
    run.arg(verb)
        .current_dir(cwd)
        .env("PATH", path_with_this_uf()?);
    if let Some(file) = &file {
        run.arg("-f").arg(absolute(cwd, file));
    }
    let failed_to_start = |error: std::io::Error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(uf_infra::cstr!(
                "sqlc is not installed, or not on PATH. Install it \
                 (https://docs.sqlc.dev/en/latest/overview/install.html) or point `SQLC` at it"
            ))
        } else {
            anyhow::Error::new(error).context(uf_infra::into_string(uf_infra::cstr!(
                "could not run {}",
                sqlc.to_string_lossy()
            )))
        }
    };
    if verb == "diff" {
        // sqlc exits 1 both for a stale tree and for a SQL error. Only that
        // exit, with an empty stderr or a unified diff on either stream, is
        // the stale tree. A signal or any other code is sqlc failing.
        let output = run.output().map_err(failed_to_start)?;
        let _ = std::io::stdout().write_all(&output.stdout);
        let _ = std::io::stderr().write_all(&output.stderr);
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let diff = stdout.contains("--- a") || stderr.contains("--- a");
            if output.status.code() == Some(1) && (stderr.is_empty() || diff) {
                bail!(uf_infra::cstr!(
                    "the generated files are out of date; run `uf sqlc generate`"
                ));
            }
            let status = output.status;
            bail!(uf_infra::cstr!("sqlc diff failed ({status})"));
        }
    } else {
        let status = run.status().map_err(failed_to_start)?;
        if !status.success() {
            bail!(uf_infra::cstr!("sqlc {verb} failed"));
        }
    }
    let message = if verb == "generate" {
        "sqlc generated the Flow modules"
    } else {
        "the generated Flow modules are up to date"
    };
    ui.render(|renderer, out| renderer.status(out, Status::Success, message));
    Ok(())
}

fn absolute(cwd: &Utf8Path, file: &Utf8Path) -> Utf8PathBuf {
    if file.is_absolute() {
        file.to_owned()
    } else {
        cwd.join(file)
    }
}

/// `PATH`, with the directory of the running `uf` first.
fn path_with_this_uf() -> Result<OsString> {
    let exe = std::env::current_exe().context("could not find the running uf")?;
    let dir = exe
        .parent()
        .context("the running uf has no directory")?
        .to_path_buf();
    let rest = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(dir).chain(std::env::split_paths(&rest));
    std::env::join_paths(paths).context("could not build PATH for sqlc")
}

/// `$SQLC`, then `sqlc` on `PATH`, then the pinned binary `uf sqlc install` wrote.
///
/// `$SQLC` is returned even when that path does not exist. The caller maps
/// `NotFound` to the install message, which is what a project that pointed
/// `SQLC` at the wrong place already sees.
fn resolve_sqlc(cwd: &Utf8Path) -> Result<OsString> {
    if let Some(sqlc) = std::env::var_os("SQLC") {
        return Ok(sqlc);
    }
    if let Some(found) = sqlc_on_path() {
        return Ok(found.into_os_string());
    }
    let Some(release) = host_archive() else {
        bail!(uf_infra::cstr!(
            "sqlc is not installed, and there is no pinned sqlc archive for {}-{}. \
             Install sqlc and set `SQLC`, or put `sqlc` on PATH \
             (https://docs.sqlc.dev/en/latest/overview/install.html)",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
    };
    let binary = managed_dir(cwd).join(release.binary);
    let version = std::fs::read_to_string(managed_dir(cwd).join("version")).unwrap_or_default();
    let version = version.trim();
    if binary.is_file() && version == PINNED_SQLC_VERSION {
        return Ok(binary.into_os_string());
    }
    if binary.is_file() {
        let found = if version.is_empty() {
            "an unversioned binary"
        } else {
            version
        };
        bail!(uf_infra::cstr!(
            ".uf/sqlc holds sqlc {found}, not the pinned {PINNED_SQLC_VERSION}. \
             Run `uf sqlc install` to replace it, or set `SQLC` to the binary you want"
        ));
    }
    bail!(uf_infra::cstr!(
        "sqlc is not installed. Run `uf sqlc install` to download sqlc {PINNED_SQLC_VERSION} \
         into .uf/sqlc, or set `SQLC`, or put `sqlc` on PATH \
         (https://docs.sqlc.dev/en/latest/overview/install.html)"
    ))
}

fn managed_dir(cwd: &Utf8Path) -> Utf8PathBuf {
    cwd.join(".uf").join("sqlc")
}

/// The first `sqlc` on `PATH`. An empty `PATH` entry is skipped so a `sqlc`
/// in the current directory is not selected by accident.
fn sqlc_on_path() -> Option<Utf8PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["sqlc.exe", "sqlc"]
    } else {
        &["sqlc"]
    };
    for directory in std::env::split_paths(&path) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file()
                && let Ok(path) = Utf8PathBuf::from_path_buf(candidate)
            {
                return Some(path);
            }
        }
    }
    None
}

fn host_archive() -> Option<&'static PinnedArchive> {
    PINNED_ARCHIVES
        .iter()
        .find(|archive| archive.os == std::env::consts::OS && archive.arch == std::env::consts::ARCH)
}

/// Download sqlc into `.uf/sqlc` and print the absolute path on stdout.
///
/// Status goes to stderr. A project that already has this version is left
/// untouched unless `force` is set, so a second `SQLC=$(uf sqlc install)`
/// does not download again.
fn install(
    cwd: &Utf8Path,
    ui: &mut Ui,
    version: Option<&str>,
    sha256: Option<&str>,
    force: bool,
) -> Result<()> {
    let release = host_archive().ok_or_else(|| {
        anyhow::anyhow!(uf_infra::cstr!(
            "uf sqlc install has no archive for {}-{}. Install sqlc yourself and set `SQLC`",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let version = version.unwrap_or(PINNED_SQLC_VERSION);
    if !valid_version(version) {
        bail!(uf_infra::cstr!(
            "`{version}` is not a sqlc version. Pass the release number, such as {PINNED_SQLC_VERSION}"
        ));
    }
    let expected = expected_sha256(release, version, sha256)?;
    let dir = managed_dir(cwd);
    let binary = dir.join(release.binary);
    let version_path = dir.join("version");
    if !force && binary.is_file() {
        let installed = std::fs::read_to_string(&version_path).unwrap_or_default();
        if installed.trim() == version {
            write_path(&binary)?;
            let message = uf_infra::into_string(uf_infra::cstr!(
                "sqlc {version} is already installed"
            ));
            ui.render_err(|renderer, out| renderer.status(out, Status::Success, &message));
            return Ok(());
        }
    }
    std::fs::create_dir_all(&dir).with_context(|| uf_infra::cstr!("could not create {dir}"))?;
    let archive = dir.join(uf_infra::into_string(uf_infra::cstr!(
        ".sqlc-{version}.tar.gz"
    )));
    let url = uf_infra::into_string(uf_infra::cstr!(
        "https://github.com/sqlc-dev/sqlc/releases/download/v{version}/sqlc_{version}_{}.tar.gz",
        release.platform
    ));
    download(&url, &archive)?;
    let bytes = std::fs::read(&archive).with_context(|| uf_infra::cstr!("could not read {archive}"))?;
    verify_sha256(&bytes, &expected).map_err(|error| {
        let _ = std::fs::remove_file(&archive);
        error
    })?;
    extract_binary(&archive, &dir, release.binary)?;
    let _ = std::fs::remove_file(&archive);
    make_executable(&binary)?;
    std::fs::write(&version_path, uf_infra::cstr!("{version}\n"))
        .with_context(|| uf_infra::cstr!("could not write {version_path}"))?;
    write_path(&binary)?;
    let message = uf_infra::into_string(uf_infra::cstr!("installed sqlc {version}"));
    ui.render_err(|renderer, out| renderer.status(out, Status::Success, &message));
    Ok(())
}

fn expected_sha256(release: &PinnedArchive, version: &str, sha256: Option<&str>) -> Result<String> {
    let pinned = version == PINNED_SQLC_VERSION;
    match sha256.map(str::trim).filter(|digest| !digest.is_empty()) {
        Some(digest) => {
            let digest = digest.to_ascii_lowercase();
            if !is_sha256_hex(&digest) {
                bail!("`--sha256` has to be 64 hex digits");
            }
            if pinned && digest != release.sha256 {
                bail!(uf_infra::cstr!(
                    "sqlc {PINNED_SQLC_VERSION} for {} is sha256 {}. \
                     Omit `--sha256` to use that digest, or pass `--version` for a different release",
                    release.platform,
                    release.sha256
                ));
            }
            Ok(digest)
        }
        None if pinned => Ok(release.sha256.to_owned()),
        None => bail!(uf_infra::cstr!(
            "`--version {version}` needs `--sha256`, the digest of \
             sqlc_{version}_{}.tar.gz. The pinned release is {PINNED_SQLC_VERSION}, \
             whose digest is already known",
            release.platform
        )),
    }
}

fn valid_version(version: &str) -> bool {
    let mut chars = version.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
}

fn is_sha256_hex(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|character| character.is_ascii_hexdigit())
}

/// The lower-case hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        uf_infra::append!(out, "{byte:02x}");
    }
    out
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = sha256_hex(bytes);
    if actual == expected {
        return Ok(());
    }
    bail!(uf_infra::cstr!(
        "the sqlc archive does not match its SHA-256, so it was not installed\n  \
         expected {expected}\n  \
         actual   {actual}"
    ))
}

fn download(url: &str, to: &Utf8Path) -> Result<()> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(to.as_str())
        .arg(url)
        .output()
        .map_err(|error| {
            anyhow::anyhow!(uf_infra::cstr!(
                "could not run curl, which downloads sqlc: {error}"
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }
    let _ = std::fs::remove_file(to);
    bail!(uf_infra::cstr!(
        "could not download {url}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn extract_binary(archive: &Utf8Path, dir: &Utf8Path, member: &str) -> Result<()> {
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive.as_str())
        .arg("-C")
        .arg(dir.as_str())
        .arg(member)
        .status()
        .map_err(|error| anyhow::anyhow!(uf_infra::cstr!("could not run tar: {error}")))?;
    if status.success() && dir.join(member).is_file() {
        return Ok(());
    }
    bail!(uf_infra::cstr!(
        "the sqlc archive did not contain `{member}` ({status})"
    ))
}

fn write_path(binary: &Utf8Path) -> Result<()> {
    let path = binary
        .canonicalize()
        .with_context(|| uf_infra::cstr!("could not resolve {binary}"))?;
    let path = Utf8PathBuf::from_path_buf(path)
        .map_err(|path| anyhow::anyhow!(uf_infra::cstr!("sqlc's path is not UTF-8: {}", path.display())))?;
    writeln!(std::io::stdout(), "{}", path.as_str()).context("could not write sqlc's path")
}

#[cfg(unix)]
fn make_executable(binary: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(binary)
        .with_context(|| uf_infra::cstr!("could not read {binary}"))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(binary, permissions)
        .with_context(|| uf_infra::cstr!("could not mark {binary} executable"))
}

#[cfg(not(unix))]
fn make_executable(_binary: &Utf8Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_a_known_digest() {
        assert_eq!(
            sha256_hex(b"a"),
            "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
        );
    }

    #[test]
    fn a_wrong_digest_is_refused_without_installing() {
        let error = verify_sha256(b"a", &"0".repeat(64)).expect_err("mismatch");
        let message = error.to_string();
        assert!(message.contains("does not match"), "{message}");
        assert!(message.contains(&"0".repeat(64)), "{message}");
    }

    #[test]
    fn the_pinned_hashes_match_the_ci_installer() {
        let script = include_str!("../../../../tools/ci/install-sqlc.sh");
        assert!(script.contains(PINNED_SQLC_VERSION), "{script}");
        for archive in PINNED_ARCHIVES {
            if archive.platform == "linux_amd64" || archive.platform == "darwin_arm64" {
                assert!(
                    script.contains(archive.sha256),
                    "{} is missing from install-sqlc.sh",
                    archive.platform
                );
            }
        }
    }

    #[test]
    fn a_version_other_than_the_pin_needs_a_digest() {
        let release = host_archive().expect("this platform has a pin");
        let error = expected_sha256(release, "1.30.0", None).expect_err("needs a digest");
        assert!(error.to_string().contains("--sha256"), "{}", error);
    }

    #[test]
    fn the_pinned_digest_rejects_a_different_one() {
        let release = host_archive().expect("this platform has a pin");
        let error = expected_sha256(release, PINNED_SQLC_VERSION, Some(&"ab".repeat(32)))
            .expect_err("wrong pin digest");
        assert!(error.to_string().contains(release.sha256), "{error}");
    }
}
