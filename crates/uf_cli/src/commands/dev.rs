//! `uf dev` and `uf lsp`: the two commands that hold a socket or a protocol.
//!
//! `uf dev` runs two loops at once: the accept loop on a worker thread, and the
//! watch loop on the main thread, which owns the [`Ui`] so terminal output stays
//! single-threaded. The update channel and the listener are shared between them,
//! which is the ownership proof `clippy.toml` asks for before an `Arc`.
#![expect(
    clippy::disallowed_types,
    reason = "the listener and the update channel are shared between the accept loop and the watch loop"
)]

use std::io::{IsTerminal, Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde_json::json;
use uf_config::load_config;
use uf_devserver::hmr::{
    HMR_TARGET, HmrSession, PollWatcher, UpdateChannel, render_update, render_watch_error,
    watched_files,
};
use uf_devserver::{DevServer, Exposure, FsPolicy, HEALTH_TARGET};
use uf_router::write_router_manifest;
use uf_term::{KeyValue, Status, Tone};

use crate::support::project_label;
use crate::ui::Ui;

/// Start the access-controlled dev server.
///
/// Serving a file — request-target validation, path resolution, the deny/allow
/// decision — belongs entirely to `uf_devserver`, so this command has no way to
/// reach the filesystem on a request's behalf and no guard here to forget.
pub(crate) fn dev(cwd: &Utf8Path, ui: &mut Ui, host: Option<&str>, once: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let mut config = resolved.config.dev.clone();
    if let Some(host) = host {
        config.host = host.into();
    }

    let channel = Arc::new(UpdateChannel::new());
    let server = DevServer::bind(&resolved.root, &config)
        .with_context(|| {
            if host.is_some() {
                "failed to start `uf dev --host`: exposing the dev server needs an explicit \
                 dev.allowedHosts list in uf.config.js"
            } else {
                "failed to start the dev server"
            }
        })?
        .with_updates(Arc::clone(&channel));
    server.write_state(&resolved.root)?;
    let _ = write_router_manifest(&resolved.root, &resolved.config)?;

    let address = server.address();
    let url = format!("http://{}:{}", address.ip(), address.port());
    let health = format!("{url}{HEALTH_TARGET}");
    let updates = format!("{url}{HMR_TARGET}");
    let network = server.network_policy();
    let (exposure, exposure_tone) = match network.exposure() {
        Exposure::Loopback => ("loopback", Tone::Muted),
        // An exposed dev server is reachable from the network. Say so loudly.
        Exposure::Exposed => ("exposed", Tone::Warn),
    };
    let access = format!(
        "{} host{}, {} origin{}, {} root{}",
        network.allowed_hosts().count(),
        plural(network.allowed_hosts().count()),
        network.allowed_origins().count(),
        plural(network.allowed_origins().count()),
        server.fs_policy().roots().len(),
        plural(server.fs_policy().roots().len()),
    );
    // The graph is seeded before the banner so "modules" is a fact rather than
    // a promise, and so the first edit after start-up is already incremental.
    let mut watcher = PollWatcher::with_default_interval(&resolved.root).with_policy(
        FsPolicy::new(&resolved.root, &config.fs.allow, &config.fs.deny)?,
    );
    let mut session = HmrSession::new(&resolved.root, Arc::clone(&channel));
    let mut seeded = 0usize;
    for file in watched_files(&watcher).unwrap_or_default() {
        if session.seed(&file).unwrap_or(false) {
            seeded += 1;
        }
    }
    let modules = seeded.to_string();
    let interval = uf_term::format_duration(watcher.interval());

    ui.render(|renderer, out| {
        renderer.banner(out, "uf dev", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("local", &url, Tone::Accent),
                KeyValue::toned("health", &health, Tone::Muted),
                KeyValue::toned("updates", &updates, Tone::Muted),
                KeyValue::new("engine", "uf-native"),
                KeyValue::toned("exposure", exposure, exposure_tone),
                KeyValue::toned("allowed", &access, Tone::Muted),
                KeyValue::toned("modules", &modules, Tone::Number),
                KeyValue::toned("watch", &interval, Tone::Muted),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, "dev server ready");
    });

    if once {
        return Ok(());
    }

    // The accept loop moves to a worker so the main thread can own the watch
    // loop and, with it, the `Ui`. Terminal output stays on one thread, which is
    // what keeps an update line from interleaving with a banner.
    let listener = Arc::new(server);
    let accepting = Arc::clone(&listener);
    std::thread::Builder::new()
        .name(String::from("uf-dev-accept"))
        .spawn(move || {
            let _ = accepting.serve_forever();
        })
        .with_context(|| "failed to start the dev server accept loop")?;

    watch_forever(ui, &mut watcher, &mut session);
    Ok(())
}

/// Poll, apply, and print, until the process is interrupted.
///
/// A watch error is reported and the loop keeps going: a directory that
/// disappeared for one poll should not end the session, and a watcher that has
/// silently stopped seeing the project is the failure this reports rather than
/// hides.
fn watch_forever(ui: &mut Ui, watcher: &mut PollWatcher, session: &mut HmrSession) {
    loop {
        std::thread::sleep(watcher.interval());
        match watcher.poll() {
            Ok(changes) => {
                for change in &changes {
                    match session.apply(change) {
                        Ok(update) => ui.render(|renderer, out| {
                            render_update(renderer, out, &update, 2);
                        }),
                        Err(error) => {
                            let message = error.to_string();
                            ui.render_err(|renderer, out| {
                                renderer.status(out, Status::Warn, &message);
                            });
                        }
                    }
                }
            }
            Err(error) => ui.render_err(|renderer, out| {
                render_watch_error(renderer, out, &error, 2);
            }),
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// `uf lsp` speaks JSON-RPC on stdout, so nothing else may be written there.
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
    let start = input.find('{')?;
    let value = serde_json::from_str::<serde_json::Value>(&input[start..]).ok()?;
    value.get("id").cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_json_rpc_id_is_read_from_a_framed_message() {
        let input = "Content-Length: 40\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"x\"}";
        assert_eq!(json_rpc_id(input), Some(json!(7)));
    }

    #[test]
    fn a_missing_or_broken_body_has_no_id() {
        assert_eq!(json_rpc_id(""), None);
        assert_eq!(json_rpc_id("Content-Length: 2\r\n\r\n{oops"), None);
    }

    #[test]
    fn a_message_without_an_id_reports_none() {
        assert_eq!(json_rpc_id("{\"jsonrpc\":\"2.0\"}"), None);
    }

    // Port binding now belongs to `uf_devserver::server`, and is covered by
    // `falls_back_to_an_ephemeral_port_unless_strict` and
    // `a_strict_port_that_is_taken_is_a_bind_error` there.

    #[test]
    fn counts_are_pluralized() {
        assert_eq!(plural(0), "s");
        assert_eq!(plural(1), "");
        assert_eq!(plural(2), "s");
    }
}
