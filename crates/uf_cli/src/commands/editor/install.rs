//! How `uf editor install <editor>` gets an integration onto this machine.
//!
//! Two kinds of install, decided by what the editor accepts:
//!
//! * **A packaged extension, through the editor's own command line.** VS Code
//!   and Cursor install a `.vsix` with `code --install-extension <file>` and
//!   `cursor --install-extension <file>`. The `.vsix` is built by CI
//!   (`.github/workflows/editors.yml`) and attached to the GitHub release of
//!   the same uf version, beside a `.sha256`. uf downloads both, checks the
//!   digest, and only then hands the file to the editor, so a corrupted or
//!   substituted download never reaches the editor's extension directory.
//! * **Files uf carries.** Everything else under `editors/` is configuration
//!   or source, compiled into uf by [`super::assets`], and written where the
//!   editor, or the person configuring it, will look.
//!
//! # What it will not do
//!
//! Overwrite a file somebody else wrote. A destination in the editor's own
//! configuration (`~/.config/nvim/lua/uf.lua`, `~/.vim/plugin/uf.vim`) is
//! replaced only when it is missing, already identical, or an earlier copy uf
//! wrote (recognised by the first line every copy starts with). Anything else
//! is refused and named, and `--force` is how to say "replace it anyway".
//! Destinations under uf's own data directory (`…/uf/editors/`) are uf's, and
//! are replaced.
//!
//! # Where the release is
//!
//! `https://github.com/ubugeeei-prod/uf/releases/download/uf@<version>/<asset>`,
//! or `$UF_EDITOR_RELEASE_BASE/<version>/<asset>` when that is set — the flat
//! layout `UF_RELEASE_BASE` uses for the installer, under its own name because
//! a mirror of the installer's archives does not carry the editor assets. It
//! is also how the tests serve a release from a directory, over `file://`.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use super::assets::{self, Asset};
use crate::cli::Editor;

/// The repository whose releases carry the editor assets.
const REPOSITORY: &str = "ubugeeei-prod/uf";

/// The name of the packaged VS Code extension on a release.
pub(crate) fn vsix_asset(version: &str) -> String {
    uf_infra::into_string(uf_infra::cstr!("uf-vscode-{version}.vsix"))
}

/// Where a release asset is downloaded from.
pub(crate) fn asset_url(base: Option<&str>, version: &str, asset: &str) -> String {
    match base.map(|base| base.trim_end_matches('/')) {
        Some(base) if !base.is_empty() => {
            uf_infra::into_string(uf_infra::cstr!("{base}/{version}/{asset}"))
        }
        _ => uf_infra::into_string(uf_infra::cstr!(
            "https://github.com/{REPOSITORY}/releases/download/uf@{version}/{asset}"
        )),
    }
}

/// `0.2.0` from `0.2.0`, `uf@0.2.0` or `v0.2.0`, the spellings a reader types.
pub(crate) fn normalize_version(version: &str) -> String {
    let version = version.trim();
    let version = version.strip_prefix("uf@").unwrap_or(version);
    version.strip_prefix('v').unwrap_or(version).to_owned()
}

/// The editor command that installs a `.vsix`, by name.
pub(crate) fn extension_cli(editor: Editor) -> Option<&'static str> {
    match editor {
        Editor::Vscode => Some("code"),
        Editor::Cursor => Some("cursor"),
        _ => None,
    }
}

/// The first `name` on `path`, as `PATH` lists directories. On Windows the
/// editors' launchers are `.cmd` files, which are tried as well.
pub(crate) fn find_on_path(name: &str, path: Option<&std::ffi::OsStr>) -> Option<Utf8PathBuf> {
    let path = path?;
    let names: Vec<String> = if cfg!(windows) {
        vec![
            uf_infra::into_string(uf_infra::cstr!("{name}.cmd")),
            uf_infra::into_string(uf_infra::cstr!("{name}.exe")),
            name.to_owned(),
        ]
    } else {
        vec![name.to_owned()]
    };
    for directory in std::env::split_paths(path) {
        for candidate in &names {
            let full = directory.join(candidate);
            if full.is_file() {
                return Utf8PathBuf::from_path_buf(full).ok();
            }
        }
    }
    None
}

/// The lower-case hex SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        uf_infra::append!(out, "{byte:02x}");
    }
    out
}

/// The digest a `.sha256` file states: its first word, as `sha256sum` writes
/// `<hex>  <name>`. `None` when that word is not 64 hex digits, because a
/// checksum file that does not hold a checksum must not pass for one.
pub(crate) fn stated_digest(listing: &str) -> Option<String> {
    let word = listing.split_whitespace().next()?.to_ascii_lowercase();
    (word.len() == 64 && word.chars().all(|character| character.is_ascii_hexdigit()))
        .then_some(word)
}

/// Fetch `url` to `to` with curl, which reads `https://` and `file://` alike.
///
/// curl rather than an HTTP client in the binary, for the reason `uf env`
/// gives: it is on every machine uf supports, it honours the proxy settings a
/// machine already has, and one downloader is one to get right.
pub(crate) fn download(url: &str, to: &Utf8Path) -> Result<()> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(to.as_str())
        .arg(url)
        .output()
        .map_err(|error| {
            anyhow!(uf_infra::cstr!(
                "could not run curl, which downloads the extension: {error}"
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }
    bail!(uf_infra::cstr!(
        "could not download {url}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Download a release asset and its `.sha256`, check one against the other,
/// and return the asset's path inside `into`.
///
/// # Errors
///
/// When either download fails, the checksum file does not state a digest, or
/// the digest does not match. Nothing is returned for a file that failed its
/// check, so nothing can install it.
pub(crate) fn fetch_verified(
    base: Option<&str>,
    version: &str,
    asset: &str,
    into: &Utf8Path,
) -> Result<(Utf8PathBuf, String)> {
    let url = asset_url(base, version, asset);
    let file = into.join(asset);
    let listing_file = into.join(uf_infra::into_string(uf_infra::cstr!("{asset}.sha256")));
    download(uf_infra::cstr!("{url}.sha256").as_str(), &listing_file).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "the uf@{version} release has no {asset}.sha256. Releases before the one that \
             added `uf editor install` did not attach the extension; install it from the \
             Marketplace or Open VSX, or pass --vsix with a file you built"
        ))
    })?;
    download(&url, &file)?;
    let listing = std::fs::read_to_string(&listing_file)
        .with_context(|| uf_infra::cstr!("could not read {listing_file}"))?;
    let expected = stated_digest(&listing).ok_or_else(|| {
        anyhow!(uf_infra::cstr!(
            "{url}.sha256 does not state a SHA-256 digest"
        ))
    })?;
    let bytes = std::fs::read(&file).with_context(|| uf_infra::cstr!("could not read {file}"))?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        bail!(uf_infra::cstr!(
            "{asset} does not match the checksum published beside it, so it was not \
             installed\n  expected {expected}\n  actual   {actual}"
        ));
    }
    Ok((file, actual))
}

/// Run `<cli> --install-extension <vsix> --force`.
///
/// `--force` because the point of running this is to get *this* version: the
/// editor otherwise keeps an installed copy it considers newer.
pub(crate) fn install_vsix(cli: &Utf8Path, vsix: &Utf8Path) -> Result<()> {
    let output = Command::new(cli.as_str())
        .args(["--install-extension", vsix.as_str(), "--force"])
        .output()
        .map_err(|error| anyhow!(uf_infra::cstr!("could not run {cli}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    bail!(uf_infra::cstr!(
        "{cli} --install-extension failed ({}): {}",
        output.status,
        if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        }
    ))
}

/// What happened to one file uf writes into an editor's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placed {
    Written,
    Updated,
    Unchanged,
}

/// Write `contents` to `path`, which uf may not own.
///
/// Replaced only when missing, identical, or an earlier uf copy — one whose
/// first line is `contents`' first line, which every copy of these files
/// starts with — or when `force` says so.
///
/// # Errors
///
/// When the file is someone else's and `force` is off; the message names it.
pub(crate) fn place(path: &Utf8Path, contents: &str, force: bool) -> Result<Placed> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(anyhow!(uf_infra::cstr!("could not read {path}: {error}"))),
    };
    let placed = match existing {
        None => Placed::Written,
        Some(text) if text == contents => return Ok(Placed::Unchanged),
        Some(text) if force || first_line(&text) == first_line(contents) => Placed::Updated,
        Some(_) => bail!(uf_infra::cstr!(
            "{path} exists and was not written by uf, so it was left alone; move it, or \
             pass --force to replace it"
        )),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| uf_infra::cstr!("could not create {parent}"))?;
    }
    std::fs::write(path, contents).with_context(|| uf_infra::cstr!("could not write {path}"))?;
    Ok(placed)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// Replace the directory `root` — one under uf's own data directory — with
/// `files`.
pub(crate) fn write_owned_dir(root: &Utf8Path, files: &[Asset]) -> Result<()> {
    match std::fs::remove_dir_all(root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(anyhow!(uf_infra::cstr!(
                "could not replace {root}: {error}"
            )));
        }
    }
    for file in files {
        let path = root.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| uf_infra::cstr!("could not create {parent}"))?;
        }
        std::fs::write(&path, file.contents)
            .with_context(|| uf_infra::cstr!("could not write {path}"))?;
    }
    Ok(())
}

/// `$XDG_DATA_HOME/uf/editors`, or `~/.local/share/uf/editors`: where uf keeps
/// the integrations a person points their editor at.
pub(crate) fn data_dir(env: &dyn Fn(&str) -> Option<String>) -> Result<Utf8PathBuf> {
    if let Some(xdg) = env("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(Utf8PathBuf::from(xdg).join("uf").join("editors"));
    }
    let home = home(env)?;
    Ok(home.join(".local/share/uf/editors"))
}

/// `$XDG_CONFIG_HOME`, or `~/.config`: where Neovim reads `lua/` from.
pub(crate) fn config_dir(env: &dyn Fn(&str) -> Option<String>) -> Result<Utf8PathBuf> {
    if let Some(xdg) = env("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(Utf8PathBuf::from(xdg));
    }
    Ok(home(env)?.join(".config"))
}

/// The user's home directory, from `HOME` or, on Windows, `USERPROFILE`.
pub(crate) fn home(env: &dyn Fn(&str) -> Option<String>) -> Result<Utf8PathBuf> {
    env("HOME")
        .or_else(|| env("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(Utf8PathBuf::from)
        .ok_or_else(|| {
            anyhow!(uf_infra::cstr!(
                "neither HOME nor USERPROFILE is set, so there is no home directory"
            ))
        })
}

/// Where a config-file integration is written, and what the reader does next.
#[derive(Debug, Clone)]
pub(crate) struct FileInstall {
    /// The file, or the directory for a multi-file integration.
    pub(crate) path: Utf8PathBuf,
    /// The lines to show after it, each one step.
    pub(crate) next: Vec<String>,
}

/// Write the integration for an editor that is installed from files.
///
/// # Errors
///
/// For an editor installed another way (VS Code, Cursor) or not at all
/// (Helix), which the caller handles before this, and when a file cannot be
/// placed; see [`place`].
pub(crate) fn install_files(
    editor: Editor,
    env: &dyn Fn(&str) -> Option<String>,
    force: bool,
) -> Result<(FileInstall, Placed)> {
    match editor {
        Editor::Neovim => {
            let path = config_dir(env)?.join("nvim/lua/uf.lua");
            let placed = place(&path, assets::NEOVIM, force)?;
            Ok((
                FileInstall {
                    path,
                    next: vec![
                        "add `require(\"uf\").setup()` to your init.lua".to_owned(),
                        "`uf editor setup neovim` in a project writes a .nvim.lua that does it \
                         for that project"
                            .to_owned(),
                    ],
                },
                placed,
            ))
        }
        Editor::Vim => {
            let path = home(env)?.join(".vim/plugin/uf.vim");
            let placed = place(&path, assets::VIM, force)?;
            Ok((
                FileInstall {
                    path,
                    next: vec![
                        "install vim-lsp (prabirshrestha/vim-lsp); uf.vim registers `uf lsp` with it"
                            .to_owned(),
                    ],
                },
                placed,
            ))
        }
        Editor::Emacs => {
            let dir = data_dir(env)?.join("emacs");
            write_owned_dir(
                &dir,
                &[Asset {
                    path: "uf.el",
                    contents: assets::EMACS,
                }],
            )?;
            Ok((
                FileInstall {
                    next: vec![uf_infra::into_string(uf_infra::cstr!(
                        "add to your init file: (add-to-list 'load-path \"{dir}\") (require 'uf) \
                         (add-hook 'js-mode-hook #'uf-eglot-ensure)"
                    ))],
                    path: dir,
                },
                Placed::Written,
            ))
        }
        Editor::Zed => {
            let dir = data_dir(env)?.join("zed");
            write_owned_dir(&dir, assets::ZED)?;
            Ok((
                FileInstall {
                    next: vec![
                        "in Zed, run `zed: install dev extension` and choose this directory; \
                         Zed compiles it, which needs Rust installed through rustup"
                            .to_owned(),
                        "then `uf editor setup zed` in a project keeps vtsls off its Flow files"
                            .to_owned(),
                    ],
                    path: dir,
                },
                Placed::Written,
            ))
        }
        Editor::Jetbrains => {
            let dir = data_dir(env)?.join("jetbrains/lsp4ij-template");
            write_owned_dir(&dir, assets::JETBRAINS)?;
            Ok((
                FileInstall {
                    next: vec![
                        "install the LSP4IJ plugin (Settings → Plugins)".to_owned(),
                        "Settings → Languages & Frameworks → Language Servers → + → Template → \
                         Import from custom template…, and choose this directory"
                            .to_owned(),
                    ],
                    path: dir,
                },
                Placed::Written,
            ))
        }
        Editor::Vscode | Editor::Cursor | Editor::Helix => {
            bail!(uf_infra::cstr!(
                "{} is not installed from files",
                editor.name()
            ))
        }
    }
}
