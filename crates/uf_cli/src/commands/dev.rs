//! `uf dev` and `uf lsp`: the two commands that hold a socket or a protocol.

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::net::{TcpListener, TcpStream};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde_json::json;
use uf_config::load_config;
use uf_router::write_router_manifest;
use uf_term::{KeyValue, Status, Tone};

use crate::support::{project_label, write_json_file};
use crate::ui::Ui;

pub(crate) fn dev(cwd: &Utf8Path, ui: &mut Ui, once: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let listener = bind_dev_listener(
        resolved.config.dev.host.as_str(),
        resolved.config.dev.port,
        resolved.config.dev.strict_port,
    )?;
    let address = listener.local_addr()?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    write_json_file(
        &state_dir.join("dev-server.json"),
        &json!({
            "host": address.ip().to_string(),
            "port": address.port(),
            "engine": "uf-native",
            "viteCompatibility": true,
            "rolldownCompatibility": true,
            "health": "/__uf/health",
        }),
    )?;
    let _ = write_router_manifest(&resolved.root, &resolved.config)?;

    let url = format!("http://{}:{}", address.ip(), address.port());
    let health = format!("{url}/__uf/health");
    ui.render(|renderer, out| {
        renderer.banner(out, "uf dev", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("local", &url, Tone::Accent),
                KeyValue::toned("health", &health, Tone::Muted),
                KeyValue::new("engine", "uf-native"),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, "dev server ready");
    });

    if once {
        return Ok(());
    }

    for stream in listener.incoming() {
        let stream = stream.with_context(|| "failed to accept dev server connection")?;
        serve_dev_request(stream)?;
    }
    Ok(())
}

fn bind_dev_listener(host: &str, port: u16, strict_port: bool) -> Result<TcpListener> {
    match TcpListener::bind((host, port)) {
        Ok(listener) => Ok(listener),
        Err(_) if !strict_port => TcpListener::bind((host, 0)).with_context(|| {
            format!("failed to bind requested port {host}:{port} and fallback port")
        }),
        Err(error) => Err(error).with_context(|| format!("failed to bind {host}:{port}")),
    }
}

fn serve_dev_request(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0u8; 2048];
    let bytes = stream
        .read(&mut buffer)
        .with_context(|| "failed to read dev server request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (content_type, body) = if path == "/__uf/health" {
        (
            "application/json",
            r#"{"status":"ok","engine":"uf-native"}"#,
        )
    } else {
        ("text/plain; charset=utf-8", "uf dev server\n")
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .with_context(|| "failed to write dev server response")
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

    #[test]
    fn a_non_strict_port_falls_back_to_an_ephemeral_one() {
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();

        let listener = bind_dev_listener("127.0.0.1", port, false).unwrap();
        assert!(listener.local_addr().unwrap().port() > 0);
    }

    #[test]
    fn a_strict_port_reports_the_conflict() {
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();

        let error = bind_dev_listener("127.0.0.1", port, true).unwrap_err();
        assert!(error.to_string().contains("failed to bind"));
    }
}
