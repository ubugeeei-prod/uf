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
//! # Which runtime is embedded, and why the other two are not
//!
//! **Bun's `--compile`, which is what this does.** Bun appends the bundled
//! JavaScript to a copy of its own runtime and writes one file. It is one
//! process invocation with no intermediate artefacts, and — the reason it wins
//! rather than merely the reason it is convenient — it is the only one of the
//! three that can produce a binary for a machine other than the one building
//! it. `bun build --compile --target=bun-linux-x64` downloads that target's
//! runtime and links against it, which is what makes "build the Linux binary
//! you deploy from the macOS laptop you develop on" a flag rather than a CI
//! matrix. The flag is not here yet — fetching and caching a 30 MB runtime
//! mid-build is its own set of decisions, and it could not be verified in the
//! sandbox this was written in — so it is tracked as
//! <https://github.com/ubugeeei-prod/uf/issues/310>.
//!
//! **Node's single-executable applications, rejected.** SEA is the portable
//! answer and would have been the sentimental favourite: Node is the host most
//! projects already have. Two things decided against it. The first is that
//! Node's own documentation states the binary that produces the blob must be
//! the binary that receives it — there is no cross-compilation, not with a
//! flag and not with a download, so `--target` could never be built on it. The
//! second is the shape of the procedure: before Node 25.5 it needs `postject`,
//! an npm dependency `uf` would have to install into a user's project to
//! finish a build, and the versions that avoid it (`node --build-sea`) are
//! newer than the Node most projects run. A build step whose first act is to
//! install a package is not a build step this toolchain should own.
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
//! # What it costs
//!
//! Bun has to be installed to *build* — nothing is needed to *run* — and the
//! application executes on Bun's implementation of the `node:` API surface
//! rather than on Node's. Both are stated when the flag is used and neither is
//! discovered at runtime: a project without Bun is told so before any work
//! begins, by [`runtime`]. Letting a Node project compile with Node is
//! <https://github.com/ubugeeei-prod/uf/issues/312>, and the shim this links
//! against was written for that: `@uniflowed/server/standalone` targets
//! `node:http` rather than `Bun.serve`, so the module a runtime wraps does not
//! itself choose the runtime.

use std::process::Command;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_bundle::{Embedded, ReportOptions, write_embedded_assets};

use crate::commands::vite::{
    Driver, Event, Host, LogLevel, find_program, render_error, render_log,
};
use crate::support::project_label;
use crate::ui::Ui;

/// The directory the linked bundle and its embedded assets are written to.
///
/// Beside `.uf/build/server`, which the ordinary build already uses, and
/// outside the output directory so `emptyOutDir` cannot sweep it away
/// mid-build.
const WORK_DIR: &str = ".uf/build/compile";

/// The runtime `uf build --compile` wraps around the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Runtime {
    /// `bun`, as found on PATH.
    pub(crate) program: Utf8PathBuf,
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
}

/// Find the runtime, or explain what is missing before the build spends time.
///
/// Checked before Vite is asked to link anything: discovering that a binary
/// cannot be produced *after* bundling the application twice is a minute of
/// somebody's afternoon spent on a message that was knowable at the start.
pub(crate) fn runtime() -> Result<Runtime> {
    match find_program("bun") {
        Some(program) => Ok(Runtime { program }),
        None => bail!(
            "`uf build --compile` needs Bun on PATH, and there is none.\n  \
             Bun is the runtime uf embeds in a standalone binary — the compiled file needs \
             nothing to run, but producing it needs `bun`.\n  \
             Install it from https://bun.sh, or build without `--compile` and deploy the \
             output directory with a JavaScript host."
        ),
    }
}

/// The name of the file a project compiles to.
///
/// The project's directory name, which is what the build banner already calls
/// it, so the binary and the thing it was built from are spelled the same way.
pub(crate) fn binary_name(root: &Utf8Path) -> String {
    let name = project_label(root);
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Link the application and wrap the runtime around it.
///
/// Runs after the size report, on purpose: the binary is written *into* the
/// output directory, and a 60 MB executable counted among the shipped assets
/// would make every budget in `uf.config.js` meaningless.
pub(crate) fn compile(
    ui: &mut Ui,
    runtime: &Runtime,
    host: &Host,
    package: &Utf8Path,
    root: &Utf8Path,
    out_dir: &Utf8Path,
) -> Result<Compiled> {
    let work = root.join(WORK_DIR);
    let assets = work.join("assets.js");
    let name = binary_name(root);
    let binary = out_dir.join(&name);

    // The binary lands in the directory whose contents it embeds. `emptyOutDir`
    // means a rebuild has already removed the previous run's copy by the time
    // this walks, so the exclusion is for the project that turned that off —
    // without it, every `--compile` would embed the last binary inside the next
    // one and double in size on each build.
    let mut options = ReportOptions::default();
    options.excluded.push(name.as_str().into());
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
        package,
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
        &[],
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

    let output = Command::new(runtime.program.as_std_path())
        .args(["build", "--compile"])
        .arg(bundle.as_str())
        .arg("--outfile")
        .arg(binary.as_str())
        .current_dir(root.as_std_path())
        .output()
        .with_context(|| {
            format!(
                "failed to start {} for `uf build --compile`",
                runtime.program
            )
        })?;
    if !output.status.success() {
        // Bun writes the useful half of a compile failure to stderr and says
        // nothing on stdout; both are forwarded because a message split across
        // the two is worse than a message repeated.
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!(
            "`bun build --compile` exited with {}\n{}",
            output.status,
            said.trim_end()
        );
    }

    let bytes = std::fs::metadata(binary.as_std_path())
        .with_context(|| format!("`bun build --compile` reported success but wrote no {binary}"))?
        .len();

    Ok(Compiled {
        binary,
        bytes,
        embedded,
    })
}
