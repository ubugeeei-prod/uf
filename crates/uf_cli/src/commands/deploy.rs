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
//! # What an adapter is
//!
//! Two files and a directory of assets. `handler.js` is the application as a
//! Web-standard `fetch` export — `Request` in, `Response` out, no filesystem —
//! and it is the same [`@uniflowed/server/fetch`] handler `uf preview` and
//! `uf start` answer through. Beside it is one entry that knows which host it
//! is on, and `static/` is a copy of the output directory.
//!
//! That is the seam, and four targets now plug into it. None of them touches
//! the application:
//!
//! | adapter | the entry beside `handler.js` | where `static/` is answered from | what uf writes for the platform |
//! | --- | --- | --- | --- |
//! | `node` | `server.js`, `node:http` | the directory beside it | `package.json` |
//! | `container` | the same `server.js` | the same directory | `Dockerfile`, `.dockerignore` |
//! | `edge` | `worker.js`, `export default { fetch }` | Cloudflare's asset server, through `env.ASSETS` | `wrangler.json` |
//! | `serverless` | `lambda.js`, `export const handler` | the deployment package | — |
//!
//! `bun`, `deno` and `static` are not among them, and naming one is an error
//! that says what it is waiting for — see [`resolve`], and
//! [`uf_config::DeployAdapter::is_implemented`], which is the single place the
//! distinction is recorded.
//!
//! # None of these has ever run on the platform it targets
//!
//! Worth saying here rather than only in the documentation, because this is
//! the file somebody reads before adding the fifth. The sandbox uf is
//! developed in cannot bind a socket and has no credentials for any cloud, so
//! what the tests establish is the emitted file set, the emitted handler's
//! answers in process, and that those answers match `uf start`'s for the same
//! fixture. The shapes are the platforms' own documented ones. A passing test
//! is not a deployment.
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
use serde_json::json;
use uf_config::{DeployAdapter, DeployAnywhereConfig};
use uf_rsc::RSC_MANIFEST_ENV;

use crate::commands::compile::binary_name;
use crate::commands::vite::{Driver, Event, LinkContext, LogLevel, render_error, render_log};
use crate::support::project_label;
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
        // What it is waiting for, and not merely that it is waiting. For all
        // three of these the answer is a decision rather than an omission —
        // `bun` and `deno` want a benchmark first and `static` wants the route
        // table — and a reader told only "not yet" is a reader who opens the
        // issue to find out whether to write it themselves.
        let because = adapter.unimplemented_because().unwrap_or(
            "its entry file and its answer for where the static assets live are unwritten",
        );
        bail!(
            "there is no `{}` deploy adapter, so `uf build --adapter {}` would write \
             nothing.\n  Implemented: {implemented}.\n  The application half every adapter \
             shares is `@uniflowed/server/fetch`; `{}` is not written because {because}.\n  \
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
    link: LinkContext<'_>,
) -> Result<Deployed> {
    let LinkContext {
        host,
        builder,
        root,
        out_dir,
        env,
        rsc_manifest,
    } = link;
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
        builder,
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
        // The same analysis `uf build`'s own Vite run had. `handler.js` is
        // byte-for-byte identical in all four artefacts, so a `virtual:uf/actions`
        // generated from no manifest here is every server action answering 404
        // on every deploy target at once.
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
                let _ = driver.finish("uf build --adapter");
                return Err(failure);
            }
            _ => {}
        }
    }
    driver.finish("linking the deployable application")?;

    for expected in entry_files(adapter) {
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
    // files this wrote are ES modules. Without it `node server.js` fails on the
    // first `import` in a directory that is otherwise complete — exactly the
    // kind of failure a person meets for the first time on the machine they are
    // deploying to. Lambda reads the same field for the same reason, and
    // Wrangler is happier for it.
    let mut files = vec![(
        directory.join("package.json"),
        "{\n  \"private\": true,\n  \"type\": \"module\"\n}\n".to_owned(),
    )];
    files.extend(platform_files(adapter, root, &directory));
    for (file, contents) in &files {
        fs::write(file.as_std_path(), contents)
            .with_context(|| format!("failed to write {file}"))?;
        copied.count(fs::metadata(file.as_std_path())?.len());
    }
    for entry in entry_files(adapter) {
        copied.count(fs::metadata(directory.join(entry).as_std_path())?.len());
    }

    Ok(Deployed {
        adapter,
        directory,
        files: copied.files,
        bytes: copied.bytes,
    })
}

/// The entry files an adapter's link step must have written.
///
/// Checked rather than assumed, because the driver and this file are separate
/// programs: a `uf` that asked for an adapter its `@uniflowed/vite` does not
/// implement would otherwise report a directory it never wrote. The driver
/// refuses such a request by name — this is the other end of the same fact.
///
/// `handler.js` is in every row and that is the point of the seam: the
/// application is one file and one implementation, and what differs is the
/// thing wrapped around it.
const fn entry_files(adapter: DeployAdapter) -> &'static [&'static str] {
    match adapter {
        DeployAdapter::Node | DeployAdapter::Container => &["handler.js", "server.js"],
        DeployAdapter::Edge => &["handler.js", "worker.js"],
        DeployAdapter::Serverless => &["handler.js", "lambda.js"],
        // Refused in `resolve` before anything is built, so this is
        // unreachable rather than permissive: an empty list would make
        // "the adapter wrote nothing" indistinguishable from success.
        DeployAdapter::Bun | DeployAdapter::Deno | DeployAdapter::Static => &[],
    }
}

/// The plain files a platform reads, which the bundler has no business writing.
///
/// The same division `--compile` makes with its embedded assets and the same
/// one the module header describes: the driver links JavaScript, and `uf`
/// writes the text beside it. A `wrangler.json` emitted from inside a Rolldown
/// build would be a bundler deciding what a deployment is called.
fn platform_files(
    adapter: DeployAdapter,
    root: &Utf8Path,
    directory: &Utf8Path,
) -> Vec<(Utf8PathBuf, String)> {
    match adapter {
        DeployAdapter::Edge => vec![(directory.join("wrangler.json"), wrangler_config(root))],
        DeployAdapter::Container => vec![
            (directory.join("Dockerfile"), DOCKERFILE.to_owned()),
            (directory.join(".dockerignore"), DOCKERIGNORE.to_owned()),
        ],
        DeployAdapter::Node
        | DeployAdapter::Serverless
        | DeployAdapter::Bun
        | DeployAdapter::Deno
        | DeployAdapter::Static => Vec::new(),
    }
}

/// The `compatibility_date` the generated `wrangler.json` pins.
///
/// The date `nodejs_compat` began providing the Node built-ins this bundle
/// imports — `node:async_hooks` for the request context, and whatever
/// Rolldown's CommonJS interop reaches for. Pinned rather than set to the day
/// of the build: a compatibility date ahead of the runtime a deployment lands
/// on is an error from Wrangler, and a build whose output changes because a
/// day passed is a build nobody can reproduce. Bumping it is the deployer's
/// decision and the file is theirs to edit — which is the whole reason it is
/// written into the artefact rather than passed on a command line.
const WORKERS_COMPATIBILITY_DATE: &str = "2024-09-23";

/// `wrangler.json`, which is what makes the directory a Worker.
///
/// Three decisions in it, and each is load-bearing:
///
/// * `nodejs_compat`, because `handler.js` imports `node:async_hooks` —
///   `@uniflowed/server`'s request context is an `AsyncLocalStorage`, and
///   without the flag the script does not link at all.
/// * `run_worker_first`, so the *Worker* asks for an asset before the
///   application answers. Cloudflare's default is to serve a matching asset
///   without invoking the script, which is faster and resolves a
///   file/handler collision the same way — and puts the resolution order in a
///   platform setting that no test uf can run is able to check. Asking here
///   means one order, written once, driven by `tests/library/deploy.test.js`.
/// * `not_found_handling: "none"`, so a miss comes back as a 404 the Worker
///   can fall through, and the 404 a visitor sees is the project's own
///   `_uf.not-found` rather than Cloudflare's.
fn wrangler_config(root: &Utf8Path) -> String {
    let config = json!({
        "name": worker_name(root),
        "main": "./worker.js",
        "compatibility_date": WORKERS_COMPATIBILITY_DATE,
        "compatibility_flags": ["nodejs_compat"],
        "assets": {
            "directory": "./static/",
            "binding": "ASSETS",
            "run_worker_first": true,
            "html_handling": "auto-trailing-slash",
            "not_found_handling": "none",
        },
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&config).unwrap_or_default()
    )
}

/// The project's directory name, as a name Cloudflare accepts.
///
/// A Worker's name is a subdomain: lowercase, alphanumeric and hyphens, at
/// most 63 characters. A project directory is none of those things by
/// obligation, so it is transliterated rather than trusted — and a name that
/// survives none of it becomes `uf-app`, which deploys, rather than an empty
/// string, which is an error from Wrangler that names nothing a reader did.
fn worker_name(root: &Utf8Path) -> String {
    let mut name = String::new();
    for character in project_label(root).chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    let name: String = name.chars().take(63).collect();
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        "uf-app".to_owned()
    } else {
        name.to_owned()
    }
}

/// The `Dockerfile` `--adapter container` writes.
///
/// A template, and said so in its first line rather than only in the
/// documentation. It is `--adapter node`'s directory with an image around it:
/// there is no build stage, because the build already happened and the
/// directory is its result, and there is nothing to install, because the whole
/// claim of an adapter's output is that it needs no `node_modules`. What a
/// project changes here is the base image, the port, and whatever its own
/// deployment needs — and it can, because this is a file in the output rather
/// than a flag on a command.
const DOCKERFILE: &str = r#"# Generated by `uf build --adapter container`. A template: read it, then edit it.
#
# The build already happened — this directory is its result — so there is no
# build stage and nothing to install. Change the base image, the port, or
# anything else your deployment needs.

FROM node:24-alpine

ENV NODE_ENV=production
# `server.js` reads both, and the default address is every interface: a
# container that bound loopback is a container nothing outside it can reach.
ENV HOST=0.0.0.0
ENV PORT=3000

WORKDIR /app
COPY --chown=node:node . .

USER node
EXPOSE 3000
CMD ["node", "server.js"]
"#;

/// The `.dockerignore` beside it.
///
/// Two lines, and both are about this directory being the build context: the
/// image is the artefact, and the two files that describe how to make the
/// image are not part of it.
const DOCKERIGNORE: &str = "Dockerfile\n.dockerignore\n";

/// The command a reader runs next, per adapter.
///
/// In the `uf build` summary, because the whole claim of this directory is
/// that nothing else is needed — and a reader who has to guess whether it is
/// `node server.js` or `npm start` does not yet believe that claim. For the
/// two targets that are uploaded rather than started, it is the upload.
pub(crate) fn next_command(adapter: DeployAdapter, root: &Utf8Path, directory: &str) -> String {
    match adapter {
        DeployAdapter::Node => format!("cd {directory} && node server.js"),
        DeployAdapter::Container => {
            let name = worker_name(root);
            format!("docker build -t {name} {directory} && docker run -p 3000:3000 {name}")
        }
        DeployAdapter::Edge => format!("cd {directory} && npx wrangler deploy"),
        DeployAdapter::Serverless => format!("cd {directory} && zip -r ../function.zip ."),
        // Unreachable: `resolve` refuses these before anything is built.
        DeployAdapter::Bun | DeployAdapter::Deno | DeployAdapter::Static => {
            format!("cd {directory}")
        }
    }
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
        let message = resolve(&config(), Some(DeployAdapter::Bun))
            .unwrap_err()
            .to_string();
        assert!(message.contains("no `bun` deploy adapter"), "{message}");
        assert!(
            message.contains("Implemented: node, edge, serverless, container"),
            "{message}"
        );
        // Not merely "not yet": `bun` is unwritten because the design says to
        // measure first, and a reader told only "not yet" is a reader who opens
        // the issue to find out whether to write it themselves.
        assert!(message.contains("benchmark"), "{message}");
        assert!(message.contains("issues/391"), "{message}");
    }

    #[test]
    fn every_implemented_adapter_has_a_shape_and_a_next_command() {
        // The four tables an adapter has a row in, checked together, because
        // adding a fifth means adding a row to each of them and forgetting one
        // is a build that reports a directory it did not write.
        for adapter in DeployAdapter::ALL
            .iter()
            .copied()
            .filter(|adapter| adapter.is_implemented())
        {
            let entries = entry_files(adapter);
            assert!(
                entries.contains(&"handler.js"),
                "`{}` has to write the seam",
                adapter.as_str()
            );
            assert_eq!(
                entries.len(),
                2,
                "`{}` is the application plus one entry",
                adapter.as_str()
            );
            let command = next_command(adapter, Utf8Path::new("/tmp/my-app"), ".uf/deploy/x");
            assert!(
                !command.is_empty(),
                "`{}` has no command to print",
                adapter.as_str()
            );
        }
    }

    #[test]
    fn a_worker_is_named_something_cloudflare_accepts() {
        // A Worker's name is a subdomain, and a project directory is under no
        // obligation to be one.
        assert_eq!(worker_name(Utf8Path::new("/src/Served App")), "served-app");
        assert_eq!(worker_name(Utf8Path::new("/src/my_app.v2")), "my-app-v2");
        assert_eq!(worker_name(Utf8Path::new("/src/___")), "uf-app");
    }

    #[test]
    fn the_wrangler_config_asks_for_what_the_bundle_needs() {
        let written = wrangler_config(Utf8Path::new("/src/served-app"));
        let config: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(config["name"], "served-app");
        assert_eq!(config["main"], "./worker.js");
        // `handler.js` imports `node:async_hooks`, so this is not optional: the
        // script does not link without it.
        assert_eq!(config["compatibility_flags"][0], "nodejs_compat");
        // And the resolution order is uf's rather than a platform default.
        assert_eq!(config["assets"]["run_worker_first"], true);
        assert_eq!(config["assets"]["not_found_handling"], "none");
        assert_eq!(config["assets"]["binding"], "ASSETS");
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
