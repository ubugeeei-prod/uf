//! Driving a builder from `uf dev`, `uf build`, `uf preview` and `uf start`.
//!
//! The dev server, the bundler and the plugin system run in JavaScript. `uf`
//! starts them through a **builder's driver** on the project's Capability JS
//! Host — Node.js, Bun or Deno, whichever `uf.config.js` names and the machine
//! has — and keeps the terminal for itself: the driver writes one JSON event
//! per line to stdout and this module renders them.
//!
//! Which builder that is comes from [`super::builder`], and Vite is only the
//! default. The contract names no implementation: the subcommands, the
//! arguments, the [`Event`] vocabulary below and every message this module
//! puts in front of a person are written in terms of *a* builder's driver,
//! written down in full in `docs/architecture.md`, and `@uniflowed/vite` is
//! one implementation of it. That distinction is ubugeeei-prod/uf#549, and it
//! is red line 3 — a default is a default, not a dependency. (Comments below
//! do name Vite, where the thing being explained is why the default behaves as
//! it does. Explaining an implementation is not depending on one.)
//!
//! This module was called `vite` while saying all of that, and three of its
//! error messages named Vite to whoever was running the paper builder. Both
//! are fixed; the name is `driver` because that is the word the rest of this
//! file and `uf.builder.driver` already use.
//!
//! Two things about the process are deliberate. The driver is told which `uf`
//! binary started it (`UF_BINARY`), so every module it transforms goes
//! through exactly this build of `uf transform` and never a different `uf` on
//! PATH. And it is given a pipe as stdin that `uf` holds open: when `uf` goes
//! away, for any reason, the pipe closes and the driver exits, so a dev server
//! cannot outlive the command that started it.

use std::env;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use uf_config::env_files::ProjectEnv;
use uf_config::{CapabilityJsHost, UniflowedConfig};
use uf_term::{CodeFrame, DiagnosticLevel, KeyValue, Status, Tone};

use crate::commands::builder::Builder;
use crate::ui::Ui;

/// A JavaScript host that can run the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Host {
    /// Which host it is.
    pub(crate) kind: CapabilityJsHost,
    /// The executable, as found on PATH.
    pub(crate) program: Utf8PathBuf,
}

impl Host {
    /// The host's name as a person writes it.
    pub(crate) fn name(&self) -> &'static str {
        match self.kind {
            CapabilityJsHost::Node => "node",
            CapabilityJsHost::Deno => "deno",
            CapabilityJsHost::Bun => "bun",
        }
    }
}

/// Find the host the config prefers, falling back through the accepted set.
///
/// The default host is tried first; when it is not installed and
/// `autoDetect` is on, each remaining accepted host is tried in order. A
/// project that pins one host and lacks it is told so rather than silently
/// run on another.
pub(crate) fn resolve_host(config: &UniflowedConfig) -> Result<Host> {
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
    for kind in &candidates {
        if let Some(program) = find_program(host_program(*kind)) {
            return Ok(Host {
                kind: *kind,
                program,
            });
        }
    }
    let names = candidates
        .iter()
        .map(|kind| host_program(*kind))
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "no JavaScript host found on PATH (looked for {names}); install Node.js, Bun or Deno, \
         or name an installed one in `app.runtime.capabilityJsHost.default`"
    )
}

fn host_program(kind: CapabilityJsHost) -> &'static str {
    match kind {
        CapabilityJsHost::Node => "node",
        CapabilityJsHost::Deno => "deno",
        CapabilityJsHost::Bun => "bun",
    }
}

/// Look `program` up on PATH the way a shell would.
pub(crate) fn find_program(program: &str) -> Option<Utf8PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(program);
        if is_executable(&candidate) {
            return Utf8PathBuf::from_path_buf(candidate).ok();
        }
        if cfg!(windows) {
            for extension in ["exe", "cmd", "bat"] {
                let candidate = directory.join(format!("{program}.{extension}"));
                if candidate.is_file() {
                    return Utf8PathBuf::from_path_buf(candidate).ok();
                }
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// What the driver reported, one line at a time.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event {
    /// `uf.config.js` was loaded from `file`, or the defaults were used.
    ConfigLoaded { file: Option<String> },
    /// A build phase started.
    Phase { name: String },
    /// A line from Vite's logger, or anything else the driver printed.
    Log { level: LogLevel, message: String },
    /// A server is up: `dev`, `preview` or `start`.
    ///
    /// `handlers` is empty for `uf dev`, which reports page routes only. The
    /// two servers that serve a *build* report both, because a route handler
    /// silently absent from a build is the failure they were written to make
    /// visible, and a count of zero is the thing to look at when it is.
    Listening {
        local: Vec<String>,
        network: Vec<String>,
        routes: Vec<String>,
        handlers: Vec<String>,
    },
    /// One page was prerendered.
    Page {
        url: String,
        file: String,
        status: u16,
        bytes: u64,
    },
    /// One route did not prerender.
    ///
    /// Not [`Self::Error`], which ends the command: the build carries on and
    /// writes the routes that did render, then fails once with all of them
    /// named. A page that throws is one page's problem until the build is
    /// over, and the reader needs the whole list rather than the first item.
    PageFailed { url: String, error: DriverError },
    /// A source module under the project root was added, changed or removed.
    ///
    /// Vite's watcher is the one that sees it, and it is deliberately the only
    /// one: `uf dev` recomputes whole-project answers on this rather than
    /// keeping a watcher of its own over the same tree. It carries no path
    /// because nothing that listens is incremental — a second answer to "which
    /// file" would be a promise the recompute does not keep.
    SourceChanged,
    /// What the build decided to render when.
    ///
    /// Emitted once, after the route table exists and before the first
    /// document is written. `prerender` is the policy `uf` passed in —
    /// `everything`, `possible` or `nothing`, from [`uf_config::Prerender`] —
    /// and `per_request` is every route this build is *not* writing a file
    /// for, including the route handlers and the middleware, which never had
    /// one.
    ///
    /// It exists because that list was invisible. A build printed the pages it
    /// wrote and said nothing at all about the routes it skipped, so a project
    /// whose `/posts/:slug` was silently absent from `dist/` found out from a
    /// 404 after the deploy. See ubugeeei-prod/uf#250 and #336.
    Rendering {
        prerender: String,
        prerendered: u64,
        per_request: Vec<String>,
    },
    /// A `.env` file `uf dev` read, or would read, was added, changed or
    /// removed.
    ///
    /// uf reads the `.env` cascade in Rust before the driver starts, and
    /// `envDir: false` turns Vite's own file loading off so that `uf dev`,
    /// `uf build`, `uf test` and `uf run` cannot get two answers. The cost was
    /// that nothing watched them; this event is the driver saying "the values
    /// you gave me are stale", and the answer is a restart with the files read
    /// again. See ubugeeei-prod/uf#428.
    EnvChanged { file: String },
    /// Something the browser reported through `uf dev`'s diagnostic channel.
    ///
    /// A hydration mismatch, a web-vitals measurement — anything a page can
    /// see and the Node process cannot. Every other uf diagnostic reaches the
    /// terminal the developer already has open, and a browser-only one had to
    /// be noticed in a window that may not be in front. See
    /// ubugeeei-prod/uf#583 and `@uniflowed/vite`'s `internal/diagnostics.js`.
    Diagnostic(BrowserDiagnostic),
    /// How the client route table came out of the server-component split.
    ///
    /// Emitted by `@uniflowed/vite` while it generates the *client* copy of the
    /// route table, so the numbers are the table's rather than a prediction
    /// made before the bundle existed. `pages` is how many routes kept their
    /// page module; `routes` is how many there were.
    RscSplit { pages: u64, routes: u64 },
    /// A build finished.
    Done { out_dir: String, pages: u64 },
    /// The JSON projection of the config, from `driver config`.
    Config { config: Value },
    /// The driver failed.
    Error(DriverError),
}

/// How loud a log line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogLevel {
    Info,
    Warn,
    Error,
}

/// A failure the driver reported, with a position when it had one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DriverError {
    pub(crate) message: String,
    pub(crate) file: Option<String>,
    pub(crate) line: Option<usize>,
    pub(crate) column: Option<usize>,
    pub(crate) frame: Option<String>,
}

/// A diagnostic a browser produced, on its way to the terminal.
///
/// The shape is deliberately close to [`DriverError`]'s, because the rendering
/// is the same rendering: a code frame when there is a position to draw one
/// around, and a status line with its detail indented under it when there is
/// not. A browser usually has the second kind — a hydration mismatch is a fact
/// about a DOM node rather than about a line of a file — and the point of the
/// channel is that it still reads like every other uf diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserDiagnostic {
    /// How loudly it reads. `Info` is a report rather than a problem.
    pub(crate) severity: LogLevel,
    /// The headline: one line, the thing that happened.
    pub(crate) message: String,
    /// The page the browser was on, which is what makes it reproducible.
    pub(crate) origin: Option<String>,
    /// Everything under the headline, one line per line.
    pub(crate) detail: Vec<String>,
    /// A source position, for the rare report that has one.
    pub(crate) file: Option<String>,
    pub(crate) line: Option<usize>,
    pub(crate) column: Option<usize>,
}

impl Event {
    /// Parse one stdout line. Anything that is not a JSON event is a log line.
    fn parse(line: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Self::Log {
                level: LogLevel::Info,
                message: line.trim().to_owned(),
            };
        };
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let list = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let number = |key: &str| value.get(key).and_then(Value::as_u64);
        // Two events carry a failure, and they are the same shape because
        // `errorEvent` in the driver builds both.
        let failure = || DriverError {
            message: text("message").unwrap_or_else(|| String::from("the driver failed")),
            file: text("file"),
            line: number("line").and_then(|n| usize::try_from(n).ok()),
            column: number("column").and_then(|n| usize::try_from(n).ok()),
            frame: text("frame"),
        };
        match value.get("event").and_then(Value::as_str) {
            Some("config-loaded") => Self::ConfigLoaded { file: text("file") },
            Some("phase") => Self::Phase {
                name: text("name").unwrap_or_default(),
            },
            Some("log") => Self::Log {
                level: match text("level").as_deref() {
                    Some("error") => LogLevel::Error,
                    Some("warn") => LogLevel::Warn,
                    _ => LogLevel::Info,
                },
                message: text("message").unwrap_or_default(),
            },
            Some("listening") => Self::Listening {
                local: list("local"),
                network: list("network"),
                routes: list("routes"),
                handlers: list("handlers"),
            },
            Some("page") => Self::Page {
                url: text("url").unwrap_or_default(),
                file: text("file").unwrap_or_default(),
                status: u16::try_from(number("status").unwrap_or(200)).unwrap_or(200),
                bytes: number("bytes").unwrap_or(0),
            },
            Some("page-failed") => Self::PageFailed {
                url: text("url").unwrap_or_default(),
                error: failure(),
            },
            Some("source-changed") => Self::SourceChanged,
            Some("rendering") => Self::Rendering {
                prerender: text("prerender").unwrap_or_default(),
                prerendered: number("prerendered").unwrap_or(0),
                per_request: list("perRequest"),
            },
            Some("env-changed") => Self::EnvChanged {
                file: text("file").unwrap_or_default(),
            },
            Some("diagnostic") => Self::Diagnostic(BrowserDiagnostic {
                severity: match text("severity").as_deref() {
                    Some("error") => LogLevel::Error,
                    Some("warn") => LogLevel::Warn,
                    _ => LogLevel::Info,
                },
                message: text("message").unwrap_or_else(|| String::from("the browser reported")),
                origin: text("origin"),
                detail: list("detail"),
                file: text("file"),
                line: number("line").and_then(|n| usize::try_from(n).ok()),
                column: number("column").and_then(|n| usize::try_from(n).ok()),
            }),
            Some("rsc-split") => Self::RscSplit {
                pages: number("pages").unwrap_or(0),
                routes: number("routes").unwrap_or(0),
            },
            Some("done") => Self::Done {
                out_dir: text("outDir").unwrap_or_default(),
                pages: number("pages").unwrap_or(0),
            },
            Some("config") => Self::Config {
                config: value.get("config").cloned().unwrap_or(Value::Null),
            },
            Some("error") => Self::Error(failure()),
            _ => Self::Log {
                level: LogLevel::Info,
                message: line.trim().to_owned(),
            },
        }
    }
}

/// A running driver.
pub(crate) struct Driver {
    child: Child,
    /// Held open on purpose; see the module docs. `None` once [`Driver::stop`]
    /// has closed it, which is how the driver is asked to exit.
    _stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

/// How long a driver gets to exit after its stdin closes, before it is killed.
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often to ask whether it has gone.
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// Everything a second Vite run of one build needs to find.
///
/// `--compile` and `--adapter` each link the application again, after the
/// bundle `uf build` already produced, and each needs the same six answers:
/// which host, which package directory, which project, which output directory,
/// which environment, and where the React Server Components analysis was
/// written. Passed as one value because six positional arguments beside a `Ui`
/// and a target is the point at which the next one is added in the wrong
/// place — and because `rsc_manifest` was added late and to only one of the
/// two, which is exactly that failure.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkContext<'a> {
    /// The JavaScript host the driver runs on.
    pub(crate) host: &'a Host,
    /// The builder that produced the first bundle, and will produce this one.
    pub(crate) builder: &'a Builder,
    /// The project root.
    pub(crate) root: &'a Utf8Path,
    /// Where `uf build` wrote the client bundle and the prerendered documents.
    pub(crate) out_dir: &'a Utf8Path,
    /// The project's environment for this build.
    pub(crate) env: &'a ProjectEnv,
    /// The RSC manifest, so this run splits the way the first one did.
    ///
    /// Without it `virtual:uf/actions` is generated from no manifest, which is
    /// an empty table, which is every server action answering `404` in the
    /// artefact a person actually ships.
    pub(crate) rsc_manifest: &'a Utf8Path,
}

impl Driver {
    /// Start `driver.js <command> --root <root> <args>` on `host`.
    ///
    /// `env` is the project's environment for this run, and both halves of it
    /// travel here rather than at each call site: the mode, as `--mode`, which
    /// is what Vite reports as `import.meta.env.MODE` and what selected the
    /// files; and the values, in the driver's own environment, which is where
    /// server code reads them through `process.env` and where Vite picks the
    /// client-prefixed subset up. One place, because "the dev server saw a
    /// variable and the build did not" is the defect this exists to prevent —
    /// see ubugeeei-prod/uf#259 — and five call sites are five chances to
    /// forget.
    ///
    /// `extra` is what *this command* knows and neither the driver nor a `.env`
    /// file can work out for itself. It is deliberately not a general escape
    /// hatch: today it carries [`uf_rsc::RSC_MANIFEST_ENV`], the path to the
    /// server-component analysis the bundler splits the route table by, because
    /// that analysis is Rust's and runs before Vite starts. It is set after the
    /// project's files for the same reason `UF_BINARY` is: it is uf's own, and
    /// a file that names it must not be able to answer it.
    pub(crate) fn spawn(
        host: &Host,
        builder: &Builder,
        root: &Utf8Path,
        command: &str,
        args: &[String],
        env: &ProjectEnv,
        extra: &[(&str, &str)],
    ) -> Result<Self> {
        let driver = builder.driver.as_path();
        let mut process = Command::new(host.program.as_std_path());
        match host.kind {
            CapabilityJsHost::Node => {
                process.arg(driver.as_str());
            }
            CapabilityJsHost::Bun => {
                // Only when the builder declared one. Bun has no
                // `module.register`, so a builder that transforms Flow needs
                // its hooks installed this way — and a builder that does not
                // transform anything needs no preload, which is why the flag
                // is the builder's to ask for rather than uf's to assume.
                if let Some(preload) = &builder.bun_preload {
                    process.arg("--preload").arg(preload);
                }
                process.arg(driver.as_str());
            }
            CapabilityJsHost::Deno => {
                process.args(["run", "-A"]).arg(driver.as_str());
            }
        }
        process
            .arg(command)
            .arg("--root")
            .arg(root.as_str())
            .arg("--mode")
            .arg(env.mode())
            .args(args);
        // The project's `.env` files first, and uf's own two variables after
        // them: a cloned repository's `.env` is untrusted input, and
        // `UF_BINARY` names the binary every module in this build is
        // transformed through. Written in this order, a file that sets it is
        // overwritten rather than obeyed.
        env.apply(&mut process);
        process
            .env(
                "UF_BINARY",
                std::env::current_exe().context("locating the uf binary")?,
            )
            .env("UF_PROJECT_ROOT", root.as_str())
            .envs(extra.iter().copied())
            .current_dir(root.as_std_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = process
            .spawn()
            .with_context(|| format!("failed to start {} for `uf {command}`", host.name()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("driver stdin was not piped"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("driver stdout was not piped"))?;
        Ok(Self {
            child,
            _stdin: Some(stdin),
            stdout: BufReader::new(stdout),
        })
    }

    /// The next event, or `None` once the driver has closed its stdout.
    pub(crate) fn next_event(&mut self) -> Result<Option<Event>> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .stdout
                .read_line(&mut line)
                .context("reading from the builder's driver")?;
            if read == 0 {
                return Ok(None);
            }
            if line.trim().is_empty() {
                continue;
            }
            return Ok(Some(Event::parse(&line)));
        }
    }

    /// Ask the driver to go, and wait until it has.
    ///
    /// Closing stdin is the ask: the driver exits when its stdin closes, which
    /// is the same mechanism that stops a dev server outliving the `uf` that
    /// started it (see the module docs). Waiting is the point — a restart that
    /// spawned the next driver before this one released its socket would find
    /// the port taken, and Vite's answer to a taken port is to move to the next
    /// free one, so the server would come back somewhere nobody was looking.
    ///
    /// A driver that does not go within [`STOP_GRACE`] is killed. It is not
    /// expected and it is not worth hanging a terminal over: the process being
    /// waited for is one that has already been told to leave.
    pub(crate) fn stop(&mut self) {
        // Dropped, which closes the pipe. `Option` only so that this can take
        // it; nothing else reads the field.
        self._stdin = None;
        let deadline = std::time::Instant::now() + STOP_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => {}
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(STOP_POLL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Wait for the driver to exit, failing when it did not exit cleanly.
    pub(crate) fn finish(mut self, what: &str) -> Result<()> {
        let status = self
            .child
            .wait()
            .context("waiting for the builder's driver")?;
        if status.success() {
            Ok(())
        } else {
            bail!("{what} exited with {status}")
        }
    }
}

/// Render a driver error as a code frame when it has a position, and as a
/// plain block otherwise. Returns the error to fail the command with.
pub(crate) fn render_error(ui: &mut Ui, root: &Utf8Path, error: &DriverError) -> anyhow::Error {
    let headline = error
        .message
        .lines()
        .next()
        .unwrap_or("the builder's driver failed")
        .to_owned();
    let path = error.file.as_deref().map(|file| {
        Utf8Path::new(file)
            .strip_prefix(root)
            .map_or(file, Utf8Path::as_str)
            .to_owned()
    });
    let source_line = match (&error.file, error.line) {
        (Some(file), Some(line)) => std::fs::read_to_string(file).ok().and_then(|source| {
            source
                .lines()
                .nth(line.saturating_sub(1))
                .map(str::to_owned)
        }),
        _ => None,
    };

    ui.render_err(|renderer, out| {
        if let (Some(path), Some(line)) = (path.as_deref(), error.line) {
            renderer.code_frame(
                out,
                &CodeFrame {
                    level: DiagnosticLevel::Error,
                    rule: None,
                    message: &headline,
                    path,
                    line,
                    column: error.column.map_or(1, |column| column + 1),
                    span: 1,
                    source_line: source_line.as_deref(),
                    label: None,
                },
            );
        } else {
            renderer.status(out, Status::Error, &headline);
        }
        if let Some(frame) = &error.frame {
            out.push_str(frame);
            out.push('\n');
        }
        for line in error.message.lines().skip(1) {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    });
    anyhow!("{headline}")
}

/// Render a diagnostic the browser sent, the way uf renders its own.
///
/// The severity decides the mark and the colour, so a web-vitals report whose
/// numbers are all good reads as information and a `poor` one reads as a
/// failure — which is the whole point of putting them in the terminal rather
/// than in a log somebody greps afterwards.
///
/// A code frame when the browser gave a file and a line, and a status line with
/// the detail indented under it when it did not. The second is the ordinary
/// case: a hydration mismatch names a DOM node and two values, and inventing a
/// position for it would send the reader to a line that is not the answer. The
/// page the report came from is printed either way, because a diagnostic
/// nobody can reproduce is a diagnostic nobody can fix.
///
/// Nothing here reaches the network. What is rendered arrived from the page on
/// the other end of this machine's own dev server and goes to this terminal.
pub(crate) fn render_diagnostic(ui: &mut Ui, root: &Utf8Path, diagnostic: &BrowserDiagnostic) {
    let status = match diagnostic.severity {
        LogLevel::Info => Status::Info,
        LogLevel::Warn => Status::Warn,
        LogLevel::Error => Status::Error,
    };
    let level = match diagnostic.severity {
        LogLevel::Info => DiagnosticLevel::Note,
        LogLevel::Warn => DiagnosticLevel::Warning,
        LogLevel::Error => DiagnosticLevel::Error,
    };
    let path = diagnostic.file.as_deref().map(|file| {
        Utf8Path::new(file)
            .strip_prefix(root)
            .map_or(file, Utf8Path::as_str)
            .to_owned()
    });

    ui.render_err(|renderer, out| {
        match (path.as_deref(), diagnostic.line) {
            (Some(path), Some(line)) => renderer.code_frame(
                out,
                &CodeFrame {
                    level,
                    rule: None,
                    message: &diagnostic.message,
                    path,
                    line,
                    column: diagnostic.column.map_or(1, |column| column + 1),
                    span: 1,
                    source_line: None,
                    label: None,
                },
            ),
            _ => renderer.status(out, status, &diagnostic.message),
        }
        if let Some(origin) = &diagnostic.origin {
            // `page` rather than `at`, which is what every other uf diagnostic
            // uses for a location: what follows is the URL the browser was on
            // and not the place the problem is, and the hydration report's own
            // detail already has an `at` naming the DOM node. Two `at` lines in
            // one report pointing at two different things is worse than one
            // word that is not the house word.
            renderer.key_values(out, 2, &[KeyValue::toned("page", origin, Tone::Muted)]);
        }
        for line in &diagnostic.detail {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    });
}

/// Render a log event as a status line. Info lines from a build are noise
/// next to uf's own phases and are dropped.
pub(crate) fn render_log(ui: &mut Ui, level: LogLevel, message: &str) {
    let status = match level {
        LogLevel::Info => return,
        LogLevel::Warn => Status::Warn,
        LogLevel::Error => Status::Error,
    };
    if message.is_empty() {
        return;
    }
    ui.render_err(|renderer, out| renderer.status(out, status, message));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_parse_by_their_tag() {
        let event = Event::parse(
            r#"{"event":"listening","local":["http://127.0.0.1:5173/"],"network":[],"routes":["/","/docs"]}"#,
        );
        assert_eq!(
            event,
            Event::Listening {
                local: vec![String::from("http://127.0.0.1:5173/")],
                network: vec![],
                routes: vec![String::from("/"), String::from("/docs")],
                handlers: vec![],
            }
        );
        // `uf preview` and `uf start` report the handler table as well, and
        // `uf dev` does not, so its absence has to mean "none" rather than
        // failing to parse the event that has it.
        let event = Event::parse(
            r#"{"event":"listening","local":[],"network":[],"routes":["/"],"handlers":["/api/health"]}"#,
        );
        assert_eq!(
            event,
            Event::Listening {
                local: vec![],
                network: vec![],
                routes: vec![String::from("/")],
                handlers: vec![String::from("/api/health")],
            }
        );
        let event = Event::parse(
            r#"{"event":"error","message":"boom","file":"/a/b.js","line":3,"column":4}"#,
        );
        assert!(matches!(
            event,
            Event::Error(DriverError {
                line: Some(3),
                column: Some(4),
                ..
            })
        ));
        let event = Event::parse(r#"{"event":"done","outDir":"dist","pages":2}"#);
        assert_eq!(
            event,
            Event::Done {
                out_dir: String::from("dist"),
                pages: 2
            }
        );
    }

    /// The two events `uf dev` alone reads, and the shape each has to survive
    /// the channel in.
    ///
    /// `diagnostic` is the one that arrives from a *browser*, through the dev
    /// server, so every field of it is optional and none of it can be trusted
    /// to be there — a report with no severity is information rather than a
    /// parse failure, because dropping it would be losing the only trace of
    /// something a page saw.
    #[test]
    fn the_dev_server_reads_a_restart_and_a_browser_report() {
        assert_eq!(
            Event::parse(r#"{"event":"env-changed","file":"/p/.env.local","change":"change"}"#),
            Event::EnvChanged {
                file: String::from("/p/.env.local"),
            }
        );

        let event = Event::parse(
            r#"{"event":"diagnostic","severity":"warn","message":"web vitals: LCP is poor",
                "origin":"http://127.0.0.1:5173/","detail":["LCP 3200 ms — poor"]}"#,
        );
        assert_eq!(
            event,
            Event::Diagnostic(BrowserDiagnostic {
                severity: LogLevel::Warn,
                message: String::from("web vitals: LCP is poor"),
                origin: Some(String::from("http://127.0.0.1:5173/")),
                detail: vec![String::from("LCP 3200 ms — poor")],
                file: None,
                line: None,
                column: None,
            })
        );

        // A report with a position gets a code frame on the other side, so all
        // three fields have to come through.
        let positioned = Event::parse(
            r#"{"event":"diagnostic","severity":"error","message":"boom",
                "file":"/p/app/x.js","line":3,"column":4}"#,
        );
        assert!(matches!(
            positioned,
            Event::Diagnostic(BrowserDiagnostic {
                severity: LogLevel::Error,
                line: Some(3),
                column: Some(4),
                ..
            })
        ));

        // And one with nothing on it is still a diagnostic: losing it would be
        // losing the only trace of whatever the page saw.
        assert!(matches!(
            Event::parse(r#"{"event":"diagnostic"}"#),
            Event::Diagnostic(BrowserDiagnostic {
                severity: LogLevel::Info,
                ..
            })
        ));
    }

    #[test]
    fn anything_else_is_a_log_line() {
        assert_eq!(
            Event::parse("transforming...\n"),
            Event::Log {
                level: LogLevel::Info,
                message: String::from("transforming...")
            }
        );
        assert_eq!(
            Event::parse(r#"{"event":"mystery"}"#),
            Event::Log {
                level: LogLevel::Info,
                message: String::from(r#"{"event":"mystery"}"#)
            }
        );
    }

    #[test]
    fn the_configured_host_is_tried_first() {
        let mut config = UniflowedConfig::default();
        config.app.runtime.capability_js_host.default = CapabilityJsHost::Bun;
        // Whichever is installed, the answer must be a host from the accepted
        // set and must exist on disk.
        if let Ok(host) = resolve_host(&config) {
            assert!(
                config
                    .app
                    .runtime
                    .capability_js_host
                    .hosts
                    .contains(&host.kind)
            );
            assert!(host.program.is_file());
        }
    }
}
