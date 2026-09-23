//! `uf dev --target native`: the development loop a device connects to, run by
//! the project's own React Native CLI.
//!
//! # Orchestration, and not a second dev server
//!
//! `npx expo start` already is the loop this command exists for. Metro serves
//! the bundle and the Fast Refresh socket; Expo CLI serves the manifest Expo Go
//! and development builds read, prints the QR code, opens tunnels and answers
//! the key commands; React Native CLI does the same for an app without Expo.
//! Every one of those is somebody else's implementation, and red line 8 says a
//! project keeps all of them — a uf that reimplemented `expo start` would be a
//! uf that is always a release behind it. See ubugeeei-prod/uf#990.
//!
//! So this decides what runs and connects the pieces, and nothing else:
//!
//! 1. which server, from what the project installed: Expo CLI when `expo` is
//!    there, React Native CLI when `@react-native-community/cli` is;
//! 2. that the project's Metro config routes modules through uf's transformer,
//!    checked by loading it the way Metro loads it ([`METRO_PROBE`]) — without
//!    it, modules silently skip the React Compiler and uf's Flow lowering, and
//!    nothing on the device says why;
//! 3. what a device needs in order to connect, printed before the server takes
//!    the terminal;
//! 4. the server itself, with `UF_BINARY` naming this `uf`, so the bundle is
//!    compiled by the binary that started it, and everything after `--` passed
//!    through untouched.
//!
//! What it deliberately leaves alone: binding an address, which each server
//! spells its own way (`--lan`, `--tunnel`, `--host`); a QR code, which Expo
//! prints itself; and the server's output, which belongs to the person at the
//! terminal, key commands and all.
//!
//! And it writes the one thing Metro cannot get any other way: the route table.
//! The web router's table is `virtual:uf/routes`, which exists only inside Vite,
//! and Metro has no virtual modules. So before the server starts the table for
//! each platform is written beside `router.js` as `router.ios.js`,
//! `router.android.js` and `router.native.js` (`uf_router::native`), Metro's
//! own platform resolution gives each bundle its platform's file when a module
//! imports `./router`, and [`watch_routes`] keeps them current while the server
//! runs. The web router's `router.js` is not written: in a project with both
//! targets that would replace its web route types. See ubugeeei-prod/uf#981.

use std::net::{IpAddr, UdpSocket};
use std::process::{Command, ExitStatus};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use uf_config::{ResolvedConfig, UniflowedConfig};
use uf_router::RouteTarget;
use uf_router::native::{RouteTable, discover_native_route_tables, write_native_route_tables};
use uf_term::{KeyValue, Status, Tone};

use super::DevArgs;
use crate::commands::task::{adopt_exit_status, installed_binary};
use crate::commands::vite::find_program;
use crate::support::{DEVELOPMENT, env_file_list, plural, project_env, project_label, relative_to};
use crate::ui::Ui;

/// Loads the project's Metro config with the project's own `metro-config`.
const METRO_PROBE: &str = include_str!("metro_probe.cjs");

/// What the probe writes in front of its one line of JSON.
const METRO_PROBE_MARKER: &str = "UF_METRO_PROBE ";

/// The port Metro listens on when neither `--port` nor the config names one.
const METRO_DEFAULT_PORT: u16 = 8081;

/// The Metro config files Metro looks for in a project root, in its order.
const METRO_CONFIG_FILES: &[&str] = &[
    "metro.config.js",
    "metro.config.cjs",
    "metro.config.mjs",
    "metro.config.json",
];

/// The dev server a native target runs. Always the project's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeServer {
    /// `expo start`, in a project that has Expo.
    Expo {
        binary: Utf8PathBuf,
        version: Option<String>,
    },
    /// `react-native start`, from React Native Community CLI.
    ReactNativeCli {
        binary: Utf8PathBuf,
        cli_version: Option<String>,
        react_native_version: Option<String>,
    },
}

impl NativeServer {
    /// The server this project installed, looked for from `root` upward, since
    /// a workspace's `node_modules` can sit above the app that uses it.
    ///
    /// Expo is asked first, and not as a preference. An Expo project has a
    /// `react-native` on its `.bin` path too — the `react-native` package links
    /// its own `cli.js` there — and without `@react-native-community/cli`
    /// installed beside it, that binary has no `start` to run.
    pub(crate) fn detect(root: &Utf8Path) -> Option<Self> {
        for directory in root.ancestors() {
            if directory.join("node_modules/expo/package.json").is_file()
                && let Some(binary) = installed_binary(directory, "expo")
            {
                return Some(Self::Expo {
                    binary,
                    version: package_version(directory, "expo"),
                });
            }
            if directory
                .join("node_modules/@react-native-community/cli/package.json")
                .is_file()
                && let Some(binary) = installed_binary(directory, "react-native")
            {
                return Some(Self::ReactNativeCli {
                    binary,
                    cli_version: package_version(directory, "@react-native-community/cli"),
                    react_native_version: package_version(directory, "react-native"),
                });
            }
        }
        None
    }

    /// The executable that will run.
    pub(crate) fn binary(&self) -> &Utf8Path {
        match self {
            Self::Expo { binary, .. } | Self::ReactNativeCli { binary, .. } => binary,
        }
    }

    /// The command as a person would type it.
    pub(crate) fn command(&self) -> &'static str {
        match self {
            Self::Expo { .. } => "expo start",
            Self::ReactNativeCli { .. } => "react-native start",
        }
    }

    /// The command and the versions that will run, as `uf explain` and the
    /// banner print them.
    pub(crate) fn label(&self) -> String {
        let versions = match self {
            Self::Expo { version, .. } => version
                .as_deref()
                .map(|version| format!("expo {version}"))
                .into_iter()
                .collect::<Vec<_>>(),
            Self::ReactNativeCli {
                cli_version,
                react_native_version,
                ..
            } => [
                cli_version
                    .as_deref()
                    .map(|version| format!("@react-native-community/cli {version}")),
                react_native_version
                    .as_deref()
                    .map(|version| format!("react-native {version}")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
        if versions.is_empty() {
            self.command().to_string()
        } else {
            format!("{} ({})", self.command(), versions.join(", "))
        }
    }

    /// The arguments the server starts with: `start`, the port when `--port`
    /// named one, and everything after `--`, in that order and untouched.
    fn start_args(&self, port: Option<u16>, passthrough: &[String]) -> Vec<String> {
        let mut args = vec!["start".to_string()];
        if let Some(port) = port {
            args.push("--port".to_string());
            args.push(port.to_string());
        }
        args.extend(passthrough.iter().cloned());
        args
    }

    /// The config that composes uf's transformer with this server's default.
    fn metro_config_example(&self) -> &'static str {
        match self {
            Self::Expo { .. } => {
                "    // metro.config.js\n    const { getDefaultConfig } = require(\"expo/metro-config\");\n    const { withUniflowedMetro } = require(\"@uniflowed/react-native/metro\");\n\n    module.exports = withUniflowedMetro(getDefaultConfig(__dirname));"
            }
            Self::ReactNativeCli { .. } => {
                "    // metro.config.js\n    const { getDefaultConfig, mergeConfig } = require(\"@react-native/metro-config\");\n    const { withUniflowedMetro } = require(\"@uniflowed/react-native/metro\");\n\n    module.exports = withUniflowedMetro(mergeConfig(getDefaultConfig(__dirname), {}));"
            }
        }
    }

    /// What a device opens, and one sentence on how.
    ///
    /// Always an address, so the line is never missing: with no network address
    /// the loopback one is printed, and the sentence says that only something
    /// on this machine can reach it.
    fn device(&self, address: Option<IpAddr>, port: u16) -> (String, String) {
        let host = address.map_or_else(|| "127.0.0.1".to_string(), |address| address.to_string());
        match (self, address) {
            (Self::Expo { .. }, Some(_)) => (
                format!("exp://{host}:{port}"),
                "Expo Go, or a development build, on the same network opens this; Expo prints its \
                 QR code below"
                    .to_string(),
            ),
            (Self::Expo { .. }, None) => (
                format!("exp://{host}:{port}"),
                "no network address was found, so only a simulator or emulator on this machine \
                 can open this"
                    .to_string(),
            ),
            (Self::ReactNativeCli { .. }, Some(_)) => (
                format!("{host}:{port}"),
                format!(
                    "a phone on the same network connects here from the Dev Menu (Configure \
                     Bundler); a simulator or emulator uses localhost:{port}"
                ),
            ),
            (Self::ReactNativeCli { .. }, None) => (
                format!("localhost:{port}"),
                "no network address was found, so only a simulator or emulator on this machine \
                 can connect"
                    .to_string(),
            ),
        }
    }
}

/// The Metro config file in `root`, for `uf explain`, which reads the
/// filesystem rather than running the project's JavaScript.
pub(crate) fn metro_config_file(root: &Utf8Path) -> Option<&'static str> {
    METRO_CONFIG_FILES
        .iter()
        .copied()
        .find(|name| root.join(name).is_file())
}

/// Start the project's native dev server for `target`.
pub(crate) fn dev(
    ui: &mut Ui,
    resolved: &ResolvedConfig,
    args: &DevArgs,
    target: RouteTarget,
) -> Result<()> {
    let root = &resolved.root;
    if let Some(host) = &args.host {
        bail!(
            "`uf dev --target {target}` does not bind an address itself: the native dev server \
             does, and each server spells it its own way. Instead of `--host {host}`, pass the \
             server's own flag after `--` — `uf dev --target {target} -- --lan` or `-- --tunnel` \
             for Expo CLI, `uf dev --target {target} -- --host {host}` for React Native CLI.",
            target = target.as_str(),
        );
    }

    let server = NativeServer::detect(root).ok_or_else(|| {
        anyhow!(
            "`uf dev --target {}` runs the project's own React Native dev server, and {root} has \
             none installed: neither `expo` (Expo CLI) nor `@react-native-community/cli` (React \
             Native CLI) is in node_modules. Install the project's dependencies, or add the CLI \
             the app is built with.",
            target.as_str()
        )
    })?;
    let metro = check_metro(&server, root)?;

    let tables = discover_native_route_tables(root, &resolved.config)?;
    let modules = write_native_route_tables(root, &resolved.config, &tables)?;
    let routes = (!modules.files.is_empty()).then(|| {
        format!(
            "{} → {}",
            plural(modules.routes, "route"),
            modules
                .files
                .iter()
                .map(|file| relative_to(root, file))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });

    let env = project_env(resolved, args.mode.as_deref(), DEVELOPMENT)?;
    let mode = env.mode().to_owned();
    let env_files = env_file_list(root, &env);
    let port = args.port.or(metro.port).unwrap_or(METRO_DEFAULT_PORT);
    let (device, hint) = server.device(lan_address(), port);
    let engine = format!("metro, run by {}", server.label());
    let transform = metro.upstream.as_deref().map_or_else(
        || "uf transform, then the project's Babel transformer".to_string(),
        |upstream| {
            format!(
                "uf transform, then {}",
                relative_to(root, Utf8Path::new(upstream))
            )
        },
    );
    let metro_config = metro
        .config_file
        .as_deref()
        .map(|file| relative_to(root, Utf8Path::new(file)));
    let project = project_label(root).to_string();
    let uf = std::env::current_exe().context("failed to find the running uf binary for Metro")?;

    ui.render(|renderer, out| {
        renderer.banner(out, "uf dev", Some(&project));
        let mut rows = vec![
            KeyValue::new("engine", &engine),
            KeyValue::new("target", target.as_str()),
            KeyValue::new("mode", &mode),
            KeyValue::toned("transform", &transform, Tone::Muted),
        ];
        if let Some(file) = &metro_config {
            rows.push(KeyValue::toned("metro", file, Tone::Path));
        }
        if let Some(routes) = &routes {
            rows.push(KeyValue::toned("routes", routes, Tone::Path));
        }
        if let Some(files) = &env_files {
            rows.push(KeyValue::toned("env files", files, Tone::Path));
        }
        rows.push(KeyValue::toned("device", &device, Tone::Accent));
        renderer.key_values(out, 2, &rows);
        renderer.blank(out);
        renderer.status(out, Status::Info, &hint);
        renderer.blank(out);
    });

    if !tables.is_empty() {
        watch_routes(root.clone(), resolved.config.clone(), tables);
    }

    let mut command = Command::new(server.binary().as_std_path());
    env.apply(&mut command);
    command
        .args(server.start_args(args.port, &args.passthrough))
        .current_dir(root.as_std_path())
        .env("UF_BINARY", &uf);
    let status = command.status().with_context(|| {
        format!(
            "failed to start `{}` ({})",
            server.command(),
            server.binary()
        )
    })?;
    if status.success() || interrupted(status) {
        return Ok(());
    }
    adopt_exit_status(ui, status, server.command())
}

/// Whether the server ended because the person at the terminal pressed Ctrl-C.
///
/// The interrupt reaches the server and uf together, and a dev server leaving
/// on it is how a development session ends rather than a failure — so it is
/// not handed to [`adopt_exit_status`], which would print a line about it. A
/// server that traps the signal and exits 130 is saying the same thing.
fn interrupted(status: ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        const SIGINT: i32 = 2;
        if status.signal() == Some(SIGINT) {
            return true;
        }
    }
    status.code() == Some(130)
}

/// How often a running native dev server's route tables are looked at again.
///
/// Polling rather than a platform file watcher, for the reason `uf test
/// --watch` polls (`uf_test::watch`): a watcher is a dependency with failure
/// modes of its own on every operating system, and a router root is small.
/// Half a second is less than the time it takes to save a file and look at a
/// phone.
const ROUTE_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Keep the route tables current while the server runs.
///
/// A thread, because the server owns the terminal and this process only waits
/// for it to exit, and so it reports on stderr rather than through [`Ui`],
/// which the waiting thread holds. A table is written only when the scan found
/// something different from the last one written, so the formatter does not
/// run twice a second over an unchanged tree and Metro sees a file change only
/// when a route did. A failure — a directory spelled the way the router refuses
/// — is reported once rather than every half second, and the last good tables
/// stay in place until the tree is fixed.
fn watch_routes(root: Utf8PathBuf, config: UniflowedConfig, mut current: Vec<RouteTable>) {
    std::thread::spawn(move || {
        let mut reported: Option<String> = None;
        loop {
            std::thread::sleep(ROUTE_POLL_INTERVAL);
            let outcome = discover_native_route_tables(&root, &config).and_then(|tables| {
                if tables == current {
                    return Ok(None);
                }
                let written = write_native_route_tables(&root, &config, &tables)?;
                current = tables;
                Ok(Some(written))
            });
            match outcome {
                Ok(written) => {
                    reported = None;
                    if let Some(written) = written.filter(|written| !written.changed.is_empty()) {
                        let names = written
                            .changed
                            .iter()
                            .map(|file| relative_to(&root, file))
                            .collect::<Vec<_>>()
                            .join(", ");
                        eprintln!("uf: the routes changed; rewrote {names}");
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    if reported.as_deref() != Some(message.as_str()) {
                        eprintln!(
                            "uf: the route table was not rewritten, and the last one stays in \
                             place: {message}"
                        );
                        reported = Some(message);
                    }
                }
            }
        }
    });
}

/// The address a phone on the same network reaches this machine at, when
/// there is one.
///
/// Found by asking the kernel which local address it would use to reach a
/// routable one: `connect` on a UDP socket chooses a route and sends nothing.
/// The destination is in TEST-NET-1 (RFC 5737), which nothing answers on, so
/// this is a routing-table lookup and never a network request. A machine with
/// no route at all gets `None`, and the banner says only a simulator can
/// connect.
pub(crate) fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_loopback() && !address.is_unspecified()).then_some(address)
}

fn package_version(directory: &Utf8Path, package: &str) -> Option<String> {
    let manifest = std::fs::read_to_string(
        directory
            .join("node_modules")
            .join(package)
            .join("package.json"),
    )
    .ok()?;
    let value: Value = serde_json::from_str(&manifest).ok()?;
    value.get("version")?.as_str().map(str::to_owned)
}

/// What loading the project's Metro config found.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MetroProbe {
    Loaded(LoadedMetroConfig),
    NoMetro {
        message: String,
    },
    LoadFailed {
        config_file: Option<String>,
        message: String,
    },
}

/// A Metro config that loaded, and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct LoadedMetroConfig {
    config_file: Option<String>,
    babel_transformer_path: Option<String>,
    uniflowed_transformer: Option<String>,
    composed: bool,
    upstream: Option<String>,
    port: Option<u16>,
}

/// What `uf dev` keeps from a config that composes uf's transformer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ComposedMetro {
    config_file: Option<String>,
    upstream: Option<String>,
    port: Option<u16>,
}

pub(crate) fn check_metro(server: &NativeServer, root: &Utf8Path) -> Result<ComposedMetro> {
    match probe_metro(root)? {
        MetroProbe::NoMetro { message } => bail!(
            "`{}` needs Metro, and Metro's config loader does not resolve from {root} \
             ({message}). Metro comes with React Native: install the project's dependencies \
             first.",
            server.command()
        ),
        MetroProbe::LoadFailed {
            config_file,
            message,
        } => bail!(
            "{} failed to load, so Metro could not start either:\n{message}",
            config_file.as_deref().map_or_else(
                || "The Metro config".to_string(),
                |file| relative_to(root, Utf8Path::new(file))
            )
        ),
        MetroProbe::Loaded(loaded) => composed(server, root, loaded),
    }
}

/// The refusals a loaded config can earn, apart from loading it, so they can
/// be tested without Node.
fn composed(
    server: &NativeServer,
    root: &Utf8Path,
    loaded: LoadedMetroConfig,
) -> Result<ComposedMetro> {
    if loaded.uniflowed_transformer.is_none() {
        bail!(
            "`@uniflowed/react-native` is not installed in {root}, so Metro has no uf transformer \
             to run. Add it with `uf add @uniflowed/react-native`, and compose the Metro config:\n\n{}",
            server.metro_config_example()
        );
    }
    if !loaded.composed {
        let found = loaded.config_file.as_deref().map_or_else(
            || {
                "This project has no Metro config, and Metro's default does not route modules \
                 through uf's transformer"
                    .to_string()
            },
            |file| {
                format!(
                    "{} does not route modules through uf's transformer",
                    relative_to(root, Utf8Path::new(file))
                )
            },
        );
        bail!(
            "{found}: `transformer.babelTransformerPath` is {}. Modules would skip uf's \
             transform — the React Compiler, uf's Flow lowering and its StyleX policy — with \
             nothing on the device to say so. Compose the config with `withUniflowedMetro()`, \
             which keeps the transformer it names running after uf's:\n\n{}",
            loaded
                .babel_transformer_path
                .as_deref()
                .map_or_else(|| "unset".to_string(), |path| format!("`{path}`")),
            server.metro_config_example()
        );
    }
    Ok(ComposedMetro {
        config_file: loaded.config_file,
        upstream: loaded.upstream,
        port: loaded.port,
    })
}

fn probe_metro(root: &Utf8Path) -> Result<MetroProbe> {
    let node = find_program("node").ok_or_else(|| {
        anyhow!(
            "`uf dev --target native` loads the project's Metro config to check it before starting \
             a server, and there is no `node` on PATH to load it with. Expo CLI and React Native \
             CLI both run on Node; install it first."
        )
    })?;
    let output = Command::new(node.as_std_path())
        .arg("-e")
        .arg(METRO_PROBE)
        .current_dir(root.as_std_path())
        .env("UF_METRO_PROBE_ROOT", root.as_str())
        // What the config composes, and not whatever was exported before uf ran.
        .env_remove("UF_METRO_CONFIGURED_UPSTREAM")
        .output()
        .with_context(|| format!("failed to run {node} to load the Metro config"))?;
    read_probe_reply(&String::from_utf8_lossy(&output.stdout)).with_context(|| {
        format!(
            "loading the Metro config in {root} gave no answer uf can read (node exited with {}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })
}

/// The probe's reply, from behind its marker: a Metro config is free to print
/// anything while it loads, so the reply is the last marked line and nothing
/// else on stdout is read.
fn read_probe_reply(stdout: &str) -> Result<MetroProbe> {
    let reply = stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(METRO_PROBE_MARKER))
        .ok_or_else(|| anyhow!("the Metro config check printed no reply"))?;
    let value: Value = serde_json::from_str(reply)
        .with_context(|| format!("the Metro config check replied with invalid JSON: {reply}"))?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    match value.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(MetroProbe::Loaded(LoadedMetroConfig {
            config_file: text("config_file"),
            babel_transformer_path: text("babel_transformer_path"),
            uniflowed_transformer: text("uniflowed_transformer"),
            composed: value
                .get("composed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            upstream: text("upstream"),
            port: value
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok()),
        })),
        Some("no-metro") => Ok(MetroProbe::NoMetro {
            message: text("message").unwrap_or_default(),
        }),
        Some("load-failed") => Ok(MetroProbe::LoadFailed {
            config_file: text("config_file"),
            message: text("message").unwrap_or_default(),
        }),
        other => bail!("the Metro config check replied with a status uf does not know: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding `files`, as `(relative path, contents)` pairs.
    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap().to_path_buf();
        for (name, contents) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        (dir, root)
    }

    fn react_native_cli() -> NativeServer {
        NativeServer::ReactNativeCli {
            binary: Utf8PathBuf::from("/app/node_modules/.bin/react-native"),
            cli_version: Some("20.2.0".to_string()),
            react_native_version: Some("0.87.1".to_string()),
        }
    }

    fn expo() -> NativeServer {
        NativeServer::Expo {
            binary: Utf8PathBuf::from("/app/node_modules/.bin/expo"),
            version: Some("57.0.22".to_string()),
        }
    }

    #[test]
    fn expo_is_chosen_over_the_react_native_binary_an_expo_project_also_has() {
        let (_dir, root) = project(&[
            ("node_modules/.bin/expo", ""),
            ("node_modules/.bin/react-native", ""),
            (
                "node_modules/expo/package.json",
                r#"{ "name": "expo", "version": "57.0.22" }"#,
            ),
        ]);

        let server = NativeServer::detect(&root).expect("an Expo project has a server");

        assert_eq!(server.command(), "expo start");
        assert_eq!(server.label(), "expo start (expo 57.0.22)");
    }

    #[test]
    fn a_react_native_binary_without_the_community_cli_is_not_a_server() {
        let (_dir, root) = project(&[("node_modules/.bin/react-native", "")]);

        assert_eq!(NativeServer::detect(&root), None);
    }

    #[test]
    fn react_native_cli_is_named_with_both_versions() {
        let (_dir, root) = project(&[
            ("node_modules/.bin/react-native", ""),
            (
                "node_modules/@react-native-community/cli/package.json",
                r#"{ "version": "20.2.0" }"#,
            ),
            (
                "node_modules/react-native/package.json",
                r#"{ "version": "0.87.1" }"#,
            ),
        ]);

        let server = NativeServer::detect(&root).expect("a React Native CLI project has a server");

        assert_eq!(
            server.label(),
            "react-native start (@react-native-community/cli 20.2.0, react-native 0.87.1)"
        );
    }

    #[test]
    fn the_server_starts_with_the_port_and_everything_after_the_separator_untouched() {
        let passthrough = vec!["--reset-cache".to_string(), "--host=0.0.0.0".to_string()];

        assert_eq!(
            react_native_cli().start_args(Some(8099), &passthrough),
            ["start", "--port", "8099", "--reset-cache", "--host=0.0.0.0"]
        );
        assert_eq!(expo().start_args(None, &[]), ["start"]);
    }

    #[test]
    fn the_device_line_is_an_address_with_or_without_a_network() {
        let address: IpAddr = "192.168.1.20".parse().unwrap();

        let (device, hint) = expo().device(Some(address), 8081);
        assert_eq!(device, "exp://192.168.1.20:8081");
        assert!(hint.contains("Expo Go"), "{hint}");

        let (device, hint) = react_native_cli().device(None, 8082);
        assert_eq!(device, "localhost:8082");
        assert!(hint.contains("only a simulator"), "{hint}");
    }

    #[test]
    fn the_probe_reply_is_the_last_marked_line_whatever_the_config_printed() {
        let stdout = "loading metro.config.js\n{\"status\":\"not this\"}\n\nUF_METRO_PROBE {\"status\":\"ok\",\"config_file\":\"/app/metro.config.js\",\"babel_transformer_path\":\"/app/t.cjs\",\"uniflowed_transformer\":\"/app/t.cjs\",\"composed\":true,\"upstream\":null,\"port\":8081}\n";

        assert_eq!(
            read_probe_reply(stdout).unwrap(),
            MetroProbe::Loaded(LoadedMetroConfig {
                config_file: Some("/app/metro.config.js".to_string()),
                babel_transformer_path: Some("/app/t.cjs".to_string()),
                uniflowed_transformer: Some("/app/t.cjs".to_string()),
                composed: true,
                upstream: None,
                port: Some(8081),
            })
        );
        assert!(read_probe_reply("nothing marked here\n").is_err());
    }

    #[test]
    fn a_config_that_does_not_compose_is_refused_with_the_config_to_write() {
        let root = Utf8Path::new("/app");
        let error = composed(
            &react_native_cli(),
            root,
            LoadedMetroConfig {
                config_file: Some("/app/metro.config.js".to_string()),
                babel_transformer_path: Some(
                    "/app/node_modules/@react-native/metro-babel-transformer/src/index.js"
                        .to_string(),
                ),
                uniflowed_transformer: Some(
                    "/app/node_modules/@uniflowed/react-native/metro-transformer.cjs".to_string(),
                ),
                composed: false,
                ..LoadedMetroConfig::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.starts_with("metro.config.js does not route modules through uf's transformer"),
            "{error}"
        );
        assert!(
            error.contains("@react-native/metro-babel-transformer"),
            "{error}"
        );
        assert!(
            error.contains("withUniflowedMetro(mergeConfig(getDefaultConfig(__dirname), {}))"),
            "{error}"
        );
    }

    #[test]
    fn a_project_without_uniflowed_react_native_is_told_to_add_it() {
        let error = composed(&expo(), Utf8Path::new("/app"), LoadedMetroConfig::default())
            .unwrap_err()
            .to_string();

        assert!(error.contains("uf add @uniflowed/react-native"), "{error}");
        assert!(
            error.contains("withUniflowedMetro(getDefaultConfig(__dirname))"),
            "{error}"
        );
    }

    #[test]
    fn a_composed_config_keeps_its_file_upstream_and_port() {
        let composed = composed(
            &expo(),
            Utf8Path::new("/app"),
            LoadedMetroConfig {
                config_file: Some("/app/metro.config.js".to_string()),
                babel_transformer_path: Some("/app/t.cjs".to_string()),
                uniflowed_transformer: Some("/app/t.cjs".to_string()),
                composed: true,
                upstream: Some("/app/expo-transformer.js".to_string()),
                port: Some(8082),
            },
        )
        .unwrap();

        assert_eq!(
            composed,
            ComposedMetro {
                config_file: Some("/app/metro.config.js".to_string()),
                upstream: Some("/app/expo-transformer.js".to_string()),
                port: Some(8082),
            }
        );
    }
}
