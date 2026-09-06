//! `uf build --adapter <target>`: the build as a directory you can copy.
//!
//! `uf build` already produces everything an application needs and nothing
//! that can be moved: a client bundle and prerendered HTML in `dist/`, and a
//! server bundle in `.uf/build/server/server.js` whose dependencies are still
//! bare imports — so serving it needs the project's `node_modules` and the
//! checkout they sit in. `uf start` is fine with that, because `uf start` runs
//! where the build happened. A deployment is not.
//!
//! This is the third thing `uf build` can write, and the one most hosts
//! actually want:
//!
//! | | the host needs | you get |
//! | --- | --- | --- |
//! | `uf build` | the checkout, `node_modules`, `uf` | `uf start` and `uf preview` |
//! | `uf build --adapter node` | a JavaScript runtime | a directory to copy |
//! | `uf build --compile` | nothing | one executable file, and Bun to make it |
//!
//! # What an adapter is, once one exists
//!
//! Two files and a directory of assets. `handler.js` is the application as a
//! Web-standard `fetch` export — `Request` in, `Response` out, no filesystem —
//! and it is the same [`@uniflowed/server/fetch`] handler `uf preview` and
//! `uf start` answer through. `server.js` is a socket around it, and it is the
//! only part that knows it is Node. `static/` is a copy of the output
//! directory.
//!
//! That is the seam the other six adapters plug into: each of them replaces
//! `server.js` and decides where `static/` lives, and none of them touches the
//! application. Until one is written, naming it here is an error that says so
//! and names the issue — see [`resolve`], and
//! [`uf_config::DeployAdapter::is_implemented`], which is the single place the
//! distinction is recorded.
//!
//! # Why the copy happens in Rust and the link happens in JavaScript
//!
//! The same division `--compile` makes. Bundling is Vite's, so the driver
//! does it; walking an output directory and copying every file is bulk work
//! over the whole build, so `uf` does it. The alternative — a `cp -r` inside
//! the host process — would put the slowest part of this command on the
//! runtime uf spawned rather than on the toolchain that owns the terminal.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::env_files::ProjectEnv;
use uf_config::{DeployAdapter, DeployAnywhereConfig};

use crate::commands::compile::binary_name;
use crate::commands::vite::{Driver, Event, Host, LogLevel, render_error, render_log};
use crate::ui::Ui;

/// Where the generated entry files are written before they are linked.
///
/// Beside `.uf/build/server` and `.uf/build/compile`, which the ordinary build
/// and `--compile` already use, and outside the output directory so
/// `emptyOutDir` cannot sweep them away mid-build.
const WORK_DIR: &str = ".uf/build/deploy";

/// Where a finished artefact is written, one directory per adapter.
///
/// Not inside `dist/`. `dist/` is what a static host serves and what the size
/// budgets are measured against, and a second copy of it living under itself
/// would double every number in the report and ship the server bundle to the
/// browser.
const OUTPUT_DIR: &str = ".uf/deploy";

/// A finished artefact.
#[derive(Debug, Clone)]
pub(crate) struct Deployed {
    /// The adapter that produced it.
    pub(crate) adapter: DeployAdapter,
    /// The directory to copy.
    pub(crate) directory: Utf8PathBuf,
    /// How many files are in it, `static/` included.
    pub(crate) files: u64,
    /// How big it is, which is what a reader wants before they copy it.
    pub(crate) bytes: u64,
}

/// Which adapter this build is for, if any.
///
/// The flag wins over `app.runtime.deploy.adapter`, because one is what
/// somebody just typed and the other is what the project decided once.
///
/// An adapter with no implementation is refused *here*, before the client
/// bundle is built, for the reason `--compile` checks for Bun before it links
/// anything: a build that spends a minute on the bundle and then says the
/// target does not exist has spent a minute on the wrong answer. The message
/// names the target and the issue rather than listing what is valid — "uf will
/// not do this" and "uf does not do this yet" are different sentences, and
/// clap's list of accepted values says neither.
pub(crate) fn resolve(
    config: &DeployAnywhereConfig,
    requested: Option<DeployAdapter>,
) -> Result<Option<DeployAdapter>> {
    let Some(adapter) = requested.or(config.adapter) else {
        return Ok(None);
    };
    if !config.enabled {
        bail!(
            "`{}` was asked for, and `app.runtime.deploy.enabled` is false in this project",
            adapter.as_str()
        );
    }
    if let Some(issue) = adapter.tracking_issue() {
        let implemented = DeployAdapter::ALL
            .iter()
            .filter(|candidate| candidate.is_implemented())
            .map(|candidate| candidate.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "there is no `{}` deploy adapter yet, so `uf build --adapter {}` would write \
             nothing.\n  Implemented: {implemented}.\n  The application half every adapter \
             shares is `@uniflowed/server/fetch`; what `{}` still needs is its own entry file \
             and its own answer for where the static assets live.\n  \
             https://github.com/ubugeeei-prod/uf/issues/{issue}",
            adapter.as_str(),
            adapter.as_str(),
            adapter.as_str()
        );
    }
    // And the project's own list, which is what makes `adapters` a setting
    // rather than a decoration. It defaulted to all seven and nothing read it,
    // which is the shape ubugeeei-prod/uf#250 calls "a `serde` struct that
    // describes a feature"; the default is now the implemented set, and a
    // project that narrows it is narrowing something.
    if !config.adapters.contains(&adapter) {
        let listed = config
            .adapters
            .iter()
            .map(|candidate| candidate.as_str())
            .collect::<Vec<_>>();
        bail!(
            "`{}` is not in this project's `app.runtime.deploy.adapters`, which lists {}",
            adapter.as_str(),
            if listed.is_empty() {
                "nothing".to_owned()
            } else {
                listed.join(", ")
            }
        );
    }
    Ok(Some(adapter))
}

/// Link the application, then copy the build beside it.
///
/// Runs after the size report, like `--compile` and for the same reason: this
/// directory is a second copy of `dist/`, and counting it among the shipped
/// assets would put every budget in `uf.config.js` permanently over.
pub(crate) fn deploy(
    ui: &mut Ui,
    adapter: DeployAdapter,
    host: &Host,
    package: &Utf8Path,
    root: &Utf8Path,
    out_dir: &Utf8Path,
    env: &ProjectEnv,
) -> Result<Deployed> {
    let work = root.join(WORK_DIR);
    let directory = root.join(OUTPUT_DIR).join(adapter.as_str());

    // Removed rather than written over: a rebuild that renamed a hashed asset
    // would otherwise leave the previous build's copy in `static/`, and the
    // directory a person copies would carry files no version of the site ever
    // referenced.
    fs::remove_dir_all(directory.as_std_path()).or_else(ignore_missing)?;
    fs::create_dir_all(directory.as_std_path())
        .with_context(|| format!("failed to create {directory}"))?;

    let mut driver = Driver::spawn(
        host,
        package,
        root,
        "deploy",
        &[
            String::from("--out-dir"),
            out_dir
                .strip_prefix(root)
                .unwrap_or(out_dir)
                .as_str()
                .to_owned(),
            String::from("--adapter"),
            adapter.as_str().to_owned(),
            String::from("--work"),
            work.to_string(),
            String::from("--output"),
            directory.to_string(),
        ],
        env,
    )?;
    while let Some(event) = driver.next_event()? {
        match event {
            Event::Log { level, message } => match level {
                LogLevel::Warn | LogLevel::Error => render_log(ui, level, &message),
                LogLevel::Info => {}
            },
            Event::Error(error) => {
                let failure = render_error(ui, root, &error);
                let _ = driver.finish("uf build --adapter");
                return Err(failure);
            }
            _ => {}
        }
    }
    driver.finish("linking the deployable application")?;

    for expected in ["handler.js", "server.js"] {
        let file = directory.join(expected);
        if !file.is_file() {
            bail!("the `{}` adapter wrote no {file}", adapter.as_str());
        }
    }

    // The build's own output, beside the server that serves it. Excluding the
    // compiled binary is for the project that uses both flags at once: `dist/`
    // is where `--compile` writes, so without this a `--compile --adapter node`
    // would copy a 60 MB executable into the directory as a static asset.
    let mut copied = Copied::default();
    copy_tree(
        out_dir,
        &directory.join("static"),
        &binary_name(root),
        &mut copied,
    )
    .with_context(|| format!("copying {out_dir} into {directory}"))?;

    // A `package.json` with nothing in it but `type`, and it is not optional:
    // Node reads `.js` as CommonJS unless something says otherwise, and the
    // two files this wrote are ES modules. Without it `node server.js` fails
    // on the first `import` in a directory that is otherwise complete —
    // exactly the kind of failure a person meets for the first time on the
    // machine they are deploying to.
    let manifest = directory.join("package.json");
    fs::write(
        manifest.as_std_path(),
        "{\n  \"private\": true,\n  \"type\": \"module\"\n}\n",
    )
    .with_context(|| format!("failed to write {manifest}"))?;
    copied.count(fs::metadata(manifest.as_std_path())?.len());
    for entry in ["handler.js", "server.js"] {
        copied.count(fs::metadata(directory.join(entry).as_std_path())?.len());
    }

    Ok(Deployed {
        adapter,
        directory,
        files: copied.files,
        bytes: copied.bytes,
    })
}

/// What a copy added up to.
#[derive(Debug, Default)]
struct Copied {
    files: u64,
    bytes: u64,
}

impl Copied {
    fn count(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }
}

/// Copy `from` into `to`, recursively, skipping one name at the top level.
///
/// Written out rather than shelling to `cp`: a build step that depends on a
/// system utility behaves differently on the three platforms uf supports, and
/// the difference shows up as a deployment that is missing a directory.
///
/// Symlinks are followed rather than recreated, and that is deliberate: this
/// directory is meant to be copied to another machine, where a link pointing
/// outside it resolves to nothing. `fs::copy` follows, which is what makes a
/// linked asset in `public/` arrive as its bytes.
fn copy_tree(from: &Utf8Path, to: &Utf8Path, skip: &str, copied: &mut Copied) -> Result<()> {
    fs::create_dir_all(to.as_std_path()).with_context(|| format!("failed to create {to}"))?;
    for entry in fs::read_dir(from.as_std_path())
        .with_context(|| format!("failed to read {from}"))?
        .collect::<Result<Vec<_>, _>>()?
    {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            // A name that is not UTF-8 cannot be a URL either, so it is not a
            // file this build can ever have served.
            continue;
        };
        if name == skip {
            continue;
        }
        let source = from.join(name);
        let target = to.join(name);
        if entry.file_type()?.is_dir() {
            copy_tree(&source, &target, "", copied)?;
            continue;
        }
        let bytes = fs::copy(source.as_std_path(), target.as_std_path())
            .with_context(|| format!("failed to copy {source} to {target}"))?;
        copied.count(bytes);
    }
    Ok(())
}

/// A directory that was not there is a directory that is already gone.
fn ignore_missing(error: std::io::Error) -> Result<(), std::io::Error> {
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DeployAnywhereConfig {
        DeployAnywhereConfig::default()
    }

    #[test]
    fn no_adapter_is_asked_for_by_default() {
        assert_eq!(resolve(&config(), None).unwrap(), None);
    }

    #[test]
    fn the_flag_wins_over_the_project_setting() {
        let mut configured = config();
        configured.adapter = Some(DeployAdapter::Edge);
        assert_eq!(
            resolve(&configured, Some(DeployAdapter::Node)).unwrap(),
            Some(DeployAdapter::Node)
        );
    }

    #[test]
    fn an_unwritten_adapter_is_refused_by_name_and_by_issue() {
        let message = resolve(&config(), Some(DeployAdapter::Edge))
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("no `edge` deploy adapter yet"),
            "{message}"
        );
        assert!(message.contains("Implemented: node"), "{message}");
        assert!(message.contains("issues/391"), "{message}");
    }

    #[test]
    fn a_project_that_narrowed_the_list_is_told_which_setting_did_it() {
        let mut configured = config();
        configured.adapters.clear();
        let message = resolve(&configured, Some(DeployAdapter::Node))
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("app.runtime.deploy.adapters") && message.contains("nothing"),
            "{message}"
        );
    }

    #[test]
    fn a_project_that_turned_the_feature_off_is_told_which_setting_did_it() {
        let mut configured = config();
        configured.enabled = false;
        let message = resolve(&configured, Some(DeployAdapter::Node))
            .unwrap_err()
            .to_string();
        assert!(message.contains("app.runtime.deploy.enabled"), "{message}");
    }
}
