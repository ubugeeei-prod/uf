//! `uf dev` and `uf lsp`: the two commands that hold a socket or a protocol.
//!
//! `uf dev` is Vite's dev server, started through `@uniflowed/vite`'s driver
//! on the project's JavaScript host (see [`super::vite`]). Vite owns the
//! module graph, hot module replacement and the transform pipeline; uf owns
//! the terminal, the generated route types, and the transform itself, which
//! the driver reaches back into through `uf transform`.

use std::io::{IsTerminal, Read, Write};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use serde_json::json;
use uf_config::load_config;
use uf_router::write_router_manifest;
use uf_term::{KeyValue, Status, Tone};

use crate::commands::vite::{Driver, Event, package_dir, render_error, render_log, resolve_host};
use crate::support::{plural, project_label};
use crate::ui::Ui;

/// What `uf dev` was asked to do.
#[derive(Debug, Clone, Default)]
pub(crate) struct DevArgs {
    /// Bind a routable address instead of loopback.
    pub(crate) host: Option<String>,
    /// Listen on this port instead of `dev.port`.
    pub(crate) port: Option<u16>,
}

/// Start the dev server and render its events until it exits.
pub(crate) fn dev(cwd: &Utf8Path, ui: &mut Ui, args: DevArgs) -> Result<()> {
    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();

    // Exposing the server needs an allowlist; see docs/security.md. Vite
    // enforces `server.allowedHosts` itself, but a `--host` with nothing to
    // allow would start a server that refuses every request, which is worse
    // than refusing to start.
    if args
        .host
        .as_deref()
        .is_some_and(|host| host != "127.0.0.1" && host != "localhost")
        && resolved.config.dev.allowed_hosts.is_empty()
    {
        bail!(
            "`uf dev --host` exposes the dev server to the network, which needs a non-empty \
             `dev.allowedHosts` in uf.config.js"
        );
    }

    let host = resolve_host(&resolved.config)?;
    let package = package_dir(&root)?;
    let _ = write_router_manifest(&root, &resolved.config)?;

    let mut driver_args = Vec::new();
    if let Some(bind) = &args.host {
        driver_args.push(String::from("--host"));
        driver_args.push(bind.clone());
    }
    if let Some(port) = args.port {
        driver_args.push(String::from("--port"));
        driver_args.push(port.to_string());
    }
    let mut driver = Driver::spawn(&host, &package, &root, "dev", &driver_args)?;

    let host_name = host.name();
    let project = project_label(&root).to_string();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf dev", Some(&project));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("engine", "vite"),
                KeyValue::toned("host", host_name, Tone::Muted),
                KeyValue::toned(
                    "transform",
                    "uf transform (official Flow parser, React Compiler, oxc)",
                    Tone::Muted,
                ),
            ],
        );
    });

    while let Some(event) = driver.next_event()? {
        match event {
            Event::Listening {
                local,
                network,
                routes,
            } => {
                let route_count = plural(routes.len(), "route");
                ui.render(|renderer, out| {
                    renderer.blank(out);
                    for url in &local {
                        renderer.key_values(out, 2, &[KeyValue::toned("local", url, Tone::Accent)]);
                    }
                    for url in &network {
                        renderer.key_values(out, 2, &[KeyValue::toned("network", url, Tone::Warn)]);
                    }
                    renderer.key_values(
                        out,
                        2,
                        &[KeyValue::toned("routes", &route_count, Tone::Number)],
                    );
                    renderer.blank(out);
                    renderer.status(out, Status::Success, "dev server ready");
                });
            }
            Event::Log { level, message } => render_log(ui, level, &message),
            Event::Error(error) => {
                let failure = render_error(ui, &root, &error);
                let _ = driver.finish("uf dev");
                return Err(failure);
            }
            Event::ConfigLoaded { .. }
            | Event::Phase { .. }
            | Event::Page { .. }
            | Event::Done { .. }
            | Event::Config { .. } => {}
        }
    }
    driver.finish("the dev server")
}

/// Answer a JSON-RPC `initialize` on stdio.
pub(crate) fn lsp() -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    if std::io::stdin().is_terminal() {
        writeln!(stdout, "uf lsp: JSON-RPC stdio server ready")
            .with_context(|| "failed to write LSP banner")?;
        return Ok(());
    }

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .with_context(|| "failed to read LSP stdin")?;
    if input.trim().is_empty() {
        return Ok(());
    }

    let id = json_rpc_id(&input).unwrap_or(serde_json::Value::Null);
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "serverInfo": {
                "name": "uf-lsp",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "textDocumentSync": 1,
                "documentFormattingProvider": true,
                "diagnosticProvider": {
                    "interFileDependencies": true,
                    "workspaceDiagnostics": true,
                },
            },
        },
    });
    let body = serde_json::to_string(&response)?;
    write!(stdout, "Content-Length: {}\r\n\r\n{body}", body.len())
        .with_context(|| "failed to write LSP response")
}

fn json_rpc_id(input: &str) -> Option<serde_json::Value> {
    let body = input
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(input);
    serde_json::from_str::<serde_json::Value>(body.trim())
        .ok()?
        .get("id")
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_id_is_echoed_from_a_framed_message() {
        let framed = "Content-Length: 40\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"x\"}";
        assert_eq!(json_rpc_id(framed), Some(json!(7)));
        assert_eq!(json_rpc_id("{\"id\":\"a\"}"), Some(json!("a")));
        assert_eq!(json_rpc_id("nope"), None);
    }
}
