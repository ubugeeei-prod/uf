//! Driving Vite from `uf dev` and `uf build`.
//!
//! Vite is the dev server, bundler and plugin system, and it runs in
//! JavaScript. `uf` starts it through `@uniflowed/vite`'s driver on the
//! project's Capability JS Host — Node.js, Bun or Deno, whichever
//! `uf.config.js` names and the machine has — and keeps the terminal for
//! itself: the driver writes one JSON event per line to stdout and this module
//! renders them.
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
use uf_term::{CodeFrame, DiagnosticLevel, Status};

use crate::ui::Ui;

/// The driver module inside `@uniflowed/vite`.
const DRIVER: &str = "driver.js";

/// Bun's counterpart to Node's loader hooks, registered with `--preload`.
const BUN_PRELOAD: &str = "bun-preload.js";

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

/// Locate `@uniflowed/vite` from the project root, walking up through
/// `node_modules` the way module resolution does.
pub(crate) fn package_dir(root: &Utf8Path) -> Result<Utf8PathBuf> {
    installed_package(root, "vite", DRIVER)
}

/// The directory of an installed `@uniflowed/<name>`, found by walking up.
///
/// `marker` is a file the package must contain, which is what distinguishes an
/// installed package from a directory that merely has the right name — a
/// workspace link that has not been built, most often.
pub(crate) fn installed_package(root: &Utf8Path, name: &str, marker: &str) -> Result<Utf8PathBuf> {
    let mut directory = Some(root);
    while let Some(current) = directory {
        let candidate = current.join("node_modules/@uniflowed").join(name);
        if candidate.join(marker).is_file() {
            return Ok(candidate);
        }
        directory = current.parent();
    }
    bail!(
        "`@uniflowed/{name}` is not installed for {root}; add it to the project's dependencies \
         and run the package manager (`uf install`)"
    )
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
    /// Held open on purpose; see the module docs.
    _stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

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
    /// The directory `@uniflowed/vite` is resolved from.
    pub(crate) package: &'a Utf8Path,
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
        package: &Utf8Path,
        root: &Utf8Path,
        command: &str,
        args: &[String],
        env: &ProjectEnv,
        extra: &[(&str, &str)],
    ) -> Result<Self> {
        let driver = package.join(DRIVER);
        let mut process = Command::new(host.program.as_std_path());
        match host.kind {
            CapabilityJsHost::Node => {
                process.arg(driver.as_str());
            }
            CapabilityJsHost::Bun => {
                process
                    .arg("--preload")
                    .arg(package.join(BUN_PRELOAD).as_str())
                    .arg(driver.as_str());
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
            _stdin: stdin,
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
                .context("reading from the Vite driver")?;
            if read == 0 {
                return Ok(None);
            }
            if line.trim().is_empty() {
                continue;
            }
            return Ok(Some(Event::parse(&line)));
        }
    }

    /// Wait for the driver to exit, failing when it did not exit cleanly.
    pub(crate) fn finish(mut self, what: &str) -> Result<()> {
        let status = self.child.wait().context("waiting for the Vite driver")?;
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
        .unwrap_or("the Vite driver failed")
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

    #[test]
    fn a_missing_package_is_named_with_the_fix() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let error = package_dir(root).unwrap_err().to_string();
        assert!(error.contains("@uniflowed/vite"), "{error}");
        assert!(error.contains("uf install"), "{error}");
    }

    #[test]
    fn the_package_is_found_up_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let package = root.join("node_modules/@uniflowed/vite");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join(DRIVER), "").unwrap();
        let nested = root.join("apps/docs");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(package_dir(&nested).unwrap(), package);
    }
}
