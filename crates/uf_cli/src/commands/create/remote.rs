//! Remote templates for `uf new` and `uf init`: a git repository pinned to one
//! commit, or a tarball pinned by the digest it must have.
//!
//! # Pinned, or refused
//!
//! A template is code a person is about to build on, run and commit, fetched
//! from somewhere they do not control. So the source has to name the exact
//! bytes, and uf has to check that it got them:
//!
//! * **git** — `github:owner/repo#<commit>`, `git+https://…#<commit>`. The
//!   commit must be written out in full. A branch or a tag is a name somebody
//!   else can move, and a short hash is a prefix somebody else can collide
//!   with, so both are refused by name. A full commit id is itself a digest of
//!   the tree, and git checks every object it fetches against it; after the
//!   checkout, `HEAD` is compared with what was asked for as well.
//! * **tarball** — `https://….tar.gz` with `--integrity sha512-<base64>` or
//!   `--integrity sha256:<hex>`. The archive is hashed in Rust, over the bytes
//!   on disk, before `tar` sees it — `uf_env::archive`'s path, the one every
//!   tool install takes — so a tarball whose digest does not match is never
//!   unpacked. A tarball without `--integrity` is refused rather than trusted.
//!
//! # What a template may do
//!
//! Be copied, and nothing else. `uf new` runs no script from a template —
//! no install, no `prepare`, no generator — and fetches with every git hook,
//! filter and user configuration switched off, so a repository cannot run code
//! on the machine by being cloned. A symbolic link in a template is refused
//! rather than copied: where it points is decided on the machine that unpacks
//! it. A manifest that declares install-time lifecycle scripts is written as
//! it is, and named, because `uf install` refuses to run them until the
//! project allows them — which is the right moment for that decision, and not
//! this one.

#[cfg(test)]
mod tests;

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_env::source::{Checksum, Digest, Format, Source};

/// The lifecycle scripts a package manager runs during an install.
const INSTALL_SCRIPTS: [&str; 6] = [
    "preinstall",
    "install",
    "postinstall",
    "prepare",
    "preprepare",
    "postprepare",
];

/// Where a remote template comes from, and what pins it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteTemplate {
    /// A git repository at one commit.
    Git {
        /// Where to fetch from, as git reads it.
        url: String,
        /// The full commit id.
        commit: String,
    },
    /// A gzipped tarball, and the digest its bytes must have.
    Tarball {
        /// Where to download it from.
        url: String,
        /// What the download must hash to.
        integrity: Digest,
    },
}

impl RemoteTemplate {
    /// What was written in the template's place, read as a remote source.
    ///
    /// `Ok(None)` for a word that names no remote source at all — `react`,
    /// `monorepo`, a typo — which the built-in templates answer. Anything that
    /// is recognisably a source and is not pinned is an error, never a
    /// fallback: `github:owner/repo` with no commit would otherwise be read as
    /// a template name and refused with the wrong sentence.
    pub(crate) fn parse(template: &str, integrity: Option<&str>) -> Result<Option<Self>> {
        let git = if let Some(shorthand) = template.strip_prefix("github:") {
            let (repository, reference) = split_reference(shorthand);
            let valid = repository.split('/').count() == 2
                && repository
                    .split('/')
                    .all(|part| !part.is_empty() && part.chars().all(repository_character));
            if !valid {
                bail!(uf_infra::cstr!(
                    "`{template}` is not a GitHub repository; write `github:owner/repo#<commit>`"
                ));
            }
            Some((
                uf_infra::into_string(uf_infra::cstr!("https://github.com/{repository}.git")),
                reference,
                template,
            ))
        } else {
            template.strip_prefix("git+").map(|rest| {
                let (url, reference) = split_reference(rest);
                (url.to_owned(), reference, template)
            })
        };
        if let Some((url, reference, written)) = git {
            if integrity.is_some() {
                bail!(uf_infra::cstr!(
                    "`--integrity` pins a tarball, and `{written}` is a git repository: its commit \
                     id is the pin"
                ));
            }
            check_scheme(&url, written)?;
            let commit = pinned_commit(reference, written)?;
            return Ok(Some(Self::Git { url, commit }));
        }

        if template.contains("://") {
            check_scheme(template, template)?;
            let path = template.split(['?', '#']).next().unwrap_or(template);
            if !(path.ends_with(".tar.gz") || path.ends_with(".tgz")) {
                bail!(uf_infra::cstr!(
                    "`{template}` is neither a git repository nor a `.tar.gz` tarball; write \
                     `git+{template}#<commit>` for a repository"
                ));
            }
            let Some(integrity) = integrity else {
                bail!(uf_infra::cstr!(
                    "`{template}` is not pinned. A tarball is fetched only with the digest it must \
                     have: add `--integrity sha512-<base64>` or `--integrity sha256:<hex>`"
                ));
            };
            return Ok(Some(Self::Tarball {
                url: template.to_owned(),
                integrity: parse_integrity(integrity)?,
            }));
        }

        if integrity.is_some() {
            bail!(uf_infra::cstr!(
                "`--integrity` pins a tarball, and `{template}` names no tarball"
            ));
        }
        Ok(None)
    }

    /// The source, as the summary names it.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Git { url, commit } => {
                uf_infra::into_string(uf_infra::cstr!("{url} at {commit}"))
            }
            Self::Tarball { url, integrity } => {
                uf_infra::into_string(uf_infra::cstr!("{url} ({integrity})"))
            }
        }
    }
}

/// `repository#reference`, with an empty reference when there is no `#`.
fn split_reference(text: &str) -> (&str, &str) {
    text.split_once('#').unwrap_or((text, ""))
}

fn repository_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

/// Only schemes that fetch without running anything: `https`, and `file` for
/// a template on this machine. `ssh` would consult the user's SSH
/// configuration, and `git://` and `http://` carry no transport security.
fn check_scheme(url: &str, written: &str) -> Result<()> {
    if url.starts_with("https://") || url.starts_with("file://") {
        return Ok(());
    }
    bail!(uf_infra::cstr!(
        "`{written}` is fetched over a scheme uf does not use for templates; use `https://`"
    ))
}

/// A full commit id: 40 hex digits for SHA-1, 64 for SHA-256 repositories.
fn pinned_commit(reference: &str, written: &str) -> Result<String> {
    if reference.is_empty() {
        bail!(uf_infra::cstr!(
            "`{written}` is not pinned. A template is fetched at one commit, written in full: \
             `{written}#<commit>`"
        ));
    }
    let hex = reference
        .chars()
        .all(|character| character.is_ascii_hexdigit());
    if hex && matches!(reference.len(), 40 | 64) {
        return Ok(reference.to_ascii_lowercase());
    }
    if hex {
        bail!(uf_infra::cstr!(
            "`{reference}` is a short commit id, which another commit can come to share; write the \
             commit out in full"
        ));
    }
    bail!(uf_infra::cstr!(
        "`{reference}` is a branch or a tag, which can be moved to other code after you read it; \
         pin the template to a commit id instead"
    ))
}

/// `sha512-<base64>` or `sha256:<hex>`.
fn parse_integrity(integrity: &str) -> Result<Digest> {
    if let Some(value) = integrity.strip_prefix("sha512-") {
        let base64 = value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
        });
        if base64 && value.len() == 88 {
            return Ok(Digest::Sha512Base64(value.to_owned()));
        }
    }
    if let Some(value) = integrity.strip_prefix("sha256:")
        && value.len() == 64
        && value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Ok(Digest::Sha256Hex(value.to_ascii_lowercase()));
    }
    bail!(uf_infra::cstr!(
        "`{integrity}` is not an integrity uf checks; write `sha512-<base64>`, as npm's \
         `integrity` does, or `sha256:<64 hex digits>`"
    ))
}

/// A directory that is removed when it goes out of scope.
pub(crate) struct Staging {
    path: Utf8PathBuf,
}

impl Staging {
    /// A new, empty directory under the system's temporary directory.
    pub(crate) fn new() -> Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .map_err(|path| {
                anyhow!(uf_infra::cstr!(
                    "the temporary directory is not UTF-8: {}",
                    path.display()
                ))
            })?
            .join(uf_infra::into_string(uf_infra::cstr!(
                "uf-template-{}-{nanos}",
                std::process::id()
            )));
        fs::create_dir_all(&path).with_context(|| uf_infra::cstr!("failed to create {path}"))?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Fetch `template` into `into`, an empty directory, and return the directory
/// whose contents are the template.
pub(crate) fn fetch(template: &RemoteTemplate, into: &Utf8Path) -> Result<Utf8PathBuf> {
    match template {
        RemoteTemplate::Tarball { url, integrity } => {
            let installed = uf_env::archive::install(
                &Source {
                    archive: url.clone(),
                    checksum: Checksum::Known(integrity.clone()),
                    format: Format::TarGz,
                    strip: 0,
                },
                into,
            );
            // `uf_env`'s sentence is about a publisher's digest. Here the digest
            // is the one the reader pinned, and what they need to hear is that
            // nothing was unpacked.
            if let Err(uf_env::EnvError::ChecksumMismatch {
                url,
                expected,
                actual,
            }) = installed
            {
                bail!(uf_infra::cstr!(
                    "{url} does not match the integrity it was pinned to, so nothing was \
                     unpacked\n  expected {expected}\n  actual   {actual}"
                ));
            }
            installed?;
            Ok(single_directory(into)?.unwrap_or_else(|| into.to_path_buf()))
        }
        RemoteTemplate::Git { url, commit } => {
            git(into, &["init", "--quiet"])?;
            git(
                into,
                &["fetch", "--quiet", "--depth", "1", "--no-tags", url, commit],
            )?;
            git(into, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
            let head = git(into, &["rev-parse", "HEAD"])?;
            if head.trim() != commit {
                bail!(uf_infra::cstr!(
                    "{url} gave commit {} when {commit} was asked for",
                    head.trim()
                ));
            }
            Ok(into.to_path_buf())
        }
    }
}

/// Run `git` in `dir` with nothing configured but what is written here, and
/// return what it printed.
///
/// The user's and the system's configuration are both ignored, which is what
/// keeps a clone from running a hook, a filter or a credential helper it
/// found there; hooks are pointed at nothing besides; and only `https` and
/// `file` may be spoken, so a repository cannot redirect the fetch to a
/// transport that would.
fn git(dir: &Utf8Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "advice.detachedHead=false",
        ])
        .args(args)
        .output()
        .map_err(|error| {
            anyhow!(uf_infra::cstr!(
                "failed to run git, which a git template needs: {error}"
            ))
        })?;
    if !output.status.success() {
        bail!(uf_infra::cstr!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The one directory `dir` holds when it holds nothing else: the wrapper a
/// tarball of a repository puts everything in.
fn single_directory(dir: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let mut entries = fs::read_dir(dir).with_context(|| uf_infra::cstr!("failed to read {dir}"))?;
    let (Some(first), None) = (entries.next(), entries.next()) else {
        return Ok(None);
    };
    let first = first.with_context(|| uf_infra::cstr!("failed to read {dir}"))?;
    if !first.file_type().is_ok_and(|kind| kind.is_dir()) {
        return Ok(None);
    }
    Ok(Utf8PathBuf::from_path_buf(first.path()).ok())
}

/// Copy the template in `from` into `to`, returning every file written.
///
/// Every file is checked before any is written: a template that would
/// overwrite a file without `--force`, or that holds a link, writes nothing at
/// all rather than half a project. A `.git` directory is the fetch's and not
/// the template's, and is not copied.
pub(crate) fn copy_into(from: &Utf8Path, to: &Utf8Path, force: bool) -> Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| uf_infra::cstr!("failed to read {dir}"))? {
            let entry = entry.with_context(|| uf_infra::cstr!("failed to read {dir}"))?;
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                anyhow!(uf_infra::cstr!(
                    "the template holds a non-UTF-8 path: {}",
                    path.display()
                ))
            })?;
            let kind = entry
                .file_type()
                .with_context(|| uf_infra::cstr!("failed to read {path}"))?;
            let relative = path.strip_prefix(from).unwrap_or(&path).to_path_buf();
            if kind.is_symlink() {
                bail!(uf_infra::cstr!(
                    "the template holds a symbolic link, {relative}, and uf copies no link out of a \
                     template: where it points is decided on the machine that unpacks it"
                ));
            }
            if kind.is_dir() {
                if dir == from && relative == ".git" {
                    continue;
                }
                stack.push(path);
            } else if kind.is_file() {
                files.push(relative);
            }
        }
    }
    files.sort();
    if files.is_empty() {
        bail!(uf_infra::cstr!("the template holds no files"));
    }
    if !force && let Some(existing) = files.iter().find(|relative| to.join(relative).exists()) {
        bail!(uf_infra::cstr!(
            "refusing to overwrite {}; pass --force to replace generated files",
            to.join(existing)
        ));
    }
    let mut written = Vec::with_capacity(files.len());
    for relative in files {
        let target = to.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| uf_infra::cstr!("failed to create {parent}"))?;
        }
        fs::copy(from.join(&relative), &target)
            .with_context(|| uf_infra::cstr!("failed to write {target}"))?;
        written.push(target);
    }
    Ok(written)
}

/// The install-time lifecycle scripts the template's root manifest declares.
pub(crate) fn declared_install_scripts(root: &Utf8Path) -> Vec<&'static str> {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = manifest
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    INSTALL_SCRIPTS
        .into_iter()
        .filter(|name| scripts.contains_key(*name))
        .collect()
}
