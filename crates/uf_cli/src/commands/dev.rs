//! `uf dev` and `uf lsp`: the two commands that hold a socket or a protocol.
//!
//! `uf dev` is Vite's dev server, started through `@uniflowed/vite`'s driver
//! on the project's JavaScript host (see [`super::vite`]). Vite owns the
//! module graph, hot module replacement and the transform pipeline; uf owns
//! the terminal, the generated route types, and the transform itself, which
//! the driver reaches back into through `uf transform`.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use serde_json::{Value, json};
use uf_config::{FmtConfig, UniflowedConfig, load_config};
use uf_infra::FxHashMap;
use uf_router::write_router_manifest;
use uf_term::{KeyValue, Status, Tone};

use crate::commands::lint::identifier_span;
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

/// Serve the Language Server Protocol on stdio until the client says `exit`.
///
/// # Why a loop and not one answer
///
/// This read stdin to EOF, answered one message and returned. An editor holds
/// the pipe open for the life of the session, so it blocked on the read
/// forever, never saw the `initialize` it had been sent, and wrote nothing —
/// against every editor in `editors/`. The test that covered it passed
/// because it closed the pipe, which is the one thing an editor never does.
/// See ubugeeei-prod/uf#162.
///
/// # What it serves
///
/// Formatting, from the same `uf_fmt::format_source` that `uf fmt` calls, so
/// the two cannot disagree. Documents are kept in full — `textDocumentSync: 1`
/// is what the capabilities already advertised — and a format is one
/// `TextEdit` over the whole file, which is what a printer that reprints from
/// the syntax tree produces.
///
/// Diagnostics, from the same `uf_lint::lint_source` that `uf lint` calls,
/// pushed on open and on every change. `publishDiagnostics` is a
/// notification the server sends rather than a request the editor makes, so
/// there is no capability to advertise — the pull-model `diagnosticProvider`
/// that used to be advertised, and served nothing, stays gone.
pub(crate) fn lsp() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    if stdin.is_terminal() {
        writeln!(stdout, "uf lsp: JSON-RPC stdio server ready")
            .with_context(|| "failed to write LSP banner")?;
        return Ok(());
    }

    let mut reader = std::io::BufReader::new(stdin.lock());
    let mut documents: FxHashMap<String, String> = FxHashMap::default();
    let mut shutting_down = false;
    // Once, not once per keystroke. A server lints on every change and formats
    // as often as the editor asks, and reading `uf.config.js` from disk each
    // time would put a file read on the path a keystroke can trigger.
    let config = load_config(Utf8Path::new("."))
        .map_or_else(|_| UniflowedConfig::default(), |resolved| resolved.config);
    let fmt = config.fmt.clone();

    while let Some(message) = read_message(&mut reader)? {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();

        match method {
            "initialize" => respond(
                &mut stdout,
                id,
                json!({
                    "serverInfo": { "name": "uf-lsp", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": {
                        "textDocumentSync": 1,
                        "documentFormattingProvider": true,
                    },
                }),
            )?,
            "shutdown" => {
                shutting_down = true;
                respond(&mut stdout, id, Value::Null)?;
            }
            "exit" => return Ok(()),
            "textDocument/didOpen" => {
                if let Some((uri, text)) = opened_document(&message) {
                    publish_diagnostics(&mut stdout, &uri, &text, &config)?;
                    documents.insert(uri, text);
                }
            }
            "textDocument/didChange" => {
                if let Some((uri, text)) = changed_document(&message) {
                    publish_diagnostics(&mut stdout, &uri, &text, &config)?;
                    documents.insert(uri, text);
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = document_uri(&message) {
                    // An empty list, not silence: the editor keeps whatever it
                    // was last told until it is told otherwise, so a closed
                    // file would keep its markers.
                    notify(
                        &mut stdout,
                        "textDocument/publishDiagnostics",
                        json!({ "uri": uri, "diagnostics": [] }),
                    )?;
                    documents.remove(&uri);
                }
            }
            "textDocument/formatting" => {
                let source = document_uri(&message).and_then(|uri| documents.get(&uri).cloned());
                let edits = match source {
                    Some(source) => format_edits(&source, &fmt)?,
                    None => Value::Null,
                };
                respond(&mut stdout, id, edits)?;
            }
            // A request uf does not serve is answered as one, not ignored:
            // an editor waiting on an id it never gets back is a hang.
            _ if id.is_some() => respond(&mut stdout, id, Value::Null)?,
            _ => {}
        }

        // Some clients close the pipe after `shutdown` rather than sending
        // `exit`. Reading on would block until they give up.
        if shutting_down && method == "shutdown" {
            continue;
        }
    }

    Ok(())
}

/// Lint `source` and send the result to the editor.
///
/// The same `uf_lint::lint_source` `uf lint` calls, so a marker in the editor
/// and a line in the terminal are the same diagnostic — including
/// `flow/syntax`, which is the parser's own errors and the one an editor most
/// wants while the file is still being typed.
///
/// A file the linter cannot read at all is reported as nothing rather than as
/// an error: half a keystroke into a rename, the document is often not
/// anything yet, and a server that fails there is a server that stops.
fn publish_diagnostics(
    out: &mut impl Write,
    uri: &str,
    source: &str,
    config: &UniflowedConfig,
) -> Result<()> {
    let file = uf_lint::SourceFile {
        path: document_path(uri),
        source: source.to_owned(),
    };
    let Ok(report) = uf_lint::lint_source(&file, config) else {
        return Ok(());
    };

    let lines: Vec<&str> = source.lines().collect();
    let diagnostics: Vec<Value> = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let text = lines.get(diagnostic.line.saturating_sub(1)).copied();
            let start = character(text, diagnostic.column);
            let span = text.map_or(1, |text| identifier_span(text, diagnostic.column));
            let end = character(text, diagnostic.column + span);
            json!({
                "range": {
                    "start": { "line": diagnostic.line.saturating_sub(1), "character": start },
                    "end": { "line": diagnostic.line.saturating_sub(1), "character": end },
                },
                "severity": match diagnostic.severity {
                    uf_lint::Severity::Error => 1,
                    uf_lint::Severity::Warn => 2,
                },
                "source": "uf",
                "code": diagnostic.rule,
                "message": diagnostic.message,
            })
        })
        .collect();

    notify(
        out,
        "textDocument/publishDiagnostics",
        json!({ "uri": uri, "diagnostics": diagnostics }),
    )
}

/// A one-based *byte* column, as an LSP zero-based UTF-16 character offset.
///
/// The linter counts bytes and the protocol counts UTF-16 code units, and the
/// two agree only while the line is ASCII. A file with `const π = 1;` in it
/// would put every marker on that line two columns to the right.
fn character(line: Option<&str>, column: usize) -> usize {
    let Some(line) = line else {
        return column.saturating_sub(1);
    };
    let byte = column.saturating_sub(1).min(line.len());
    let head = match line.is_char_boundary(byte) {
        true => &line[..byte],
        // A column that lands inside a character counts that character.
        false => {
            let mut at = byte;
            while at > 0 && !line.is_char_boundary(at) {
                at -= 1;
            }
            &line[..at]
        }
    };
    head.chars().map(char::len_utf16).sum()
}

/// The path an editor's `file://` URI names.
///
/// The linter is path-sensitive — `flow/syntax` claims `.js`, `.jsx`, `.mjs`
/// and `.cjs` and nothing else — so a URI that arrives without its extension
/// intact is a file that is silently not parsed. Percent escapes are decoded
/// for the same reason: a project under `My Project` arrives as `My%20Project`.
fn document_path(uri: &str) -> String {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1), bytes.get(at + 2)) {
            (b'%', Some(high), Some(low)) => {
                match (
                    char::from(*high).to_digit(16),
                    char::from(*low).to_digit(16),
                ) {
                    #[allow(clippy::cast_possible_truncation)]
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        at += 3;
                    }
                    _ => {
                        out.push(bytes[at]);
                        at += 1;
                    }
                }
            }
            _ => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
}

/// A notification: a method and params, and no id to answer.
fn notify(out: &mut impl Write, method: &str, params: Value) -> Result<()> {
    let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    let body =
        serde_json::to_string(&message).with_context(|| "failed to encode a notification")?;
    write!(out, "Content-Length: {}\r\n\r\n{body}", body.len())
        .with_context(|| "failed to write a notification")?;
    out.flush()
        .with_context(|| "failed to flush a notification")
}

/// One `Content-Length`-framed message, or [`None`] at end of input.
///
/// Headers are read line by line and everything but `Content-Length` is
/// skipped, which is what the specification asks for — `Content-Type` is the
/// other one clients send, and it carries nothing uf needs.
fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .with_context(|| "failed to read an LSP header")?
            == 0
        {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok();
        }
    }

    let Some(length) = length else {
        bail!("an LSP message arrived without a Content-Length header");
    };
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .with_context(|| "failed to read an LSP message body")?;
    let message =
        serde_json::from_slice(&body).with_context(|| "failed to parse an LSP message as JSON")?;
    Ok(Some(message))
}

/// Write one framed response, unless the message was a notification.
fn respond(out: &mut impl Write, id: Option<Value>, result: Value) -> Result<()> {
    let Some(id) = id else {
        return Ok(());
    };
    let body = serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))?;
    write!(out, "Content-Length: {}\r\n\r\n{body}", body.len())
        .with_context(|| "failed to write an LSP response")?;
    out.flush()
        .with_context(|| "failed to flush an LSP response")
}

/// The whole document, replaced, or [`Value::Null`] when it is already
/// formatted.
///
/// One edit over everything: `uf_fmt` reprints from the syntax tree, so the
/// smallest honest description of what it did is "this is the file now".
/// Ranges are in UTF-16 units and the end is past any line the document has,
/// which is how the specification says to name the whole of it.
fn format_edits(source: &str, config: &FmtConfig) -> Result<Value> {
    let Ok(result) = uf_fmt::format_source(source, config) else {
        // A file that does not parse is left alone, the way `uf fmt` leaves
        // it alone. An error here would be an editor popup on every keystroke
        // in a file being typed.
        return Ok(Value::Null);
    };
    if !result.changed {
        return Ok(json!([]));
    }
    Ok(json!([{
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": source.lines().count() + 1, "character": 0 },
        },
        "newText": result.output,
    }]))
}

fn document_uri(message: &Value) -> Option<String> {
    Some(
        message
            .get("params")?
            .get("textDocument")?
            .get("uri")?
            .as_str()?
            .to_owned(),
    )
}

fn opened_document(message: &Value) -> Option<(String, String)> {
    let document = message.get("params")?.get("textDocument")?;
    Some((
        document.get("uri")?.as_str()?.to_owned(),
        document.get("text")?.as_str()?.to_owned(),
    ))
}

/// The text of a full-sync change: the last change's `text`, with no `range`.
fn changed_document(message: &Value) -> Option<(String, String)> {
    let uri = document_uri(message)?;
    let text = message
        .get("params")?
        .get("contentChanges")?
        .as_array()?
        .last()?
        .get("text")?
        .as_str()?
        .to_owned();
    Some((uri, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Framing, which is the half of the protocol a stream gets wrong.
    #[test]
    fn messages_are_read_one_frame_at_a_time() {
        let body = r#"{"jsonrpc":"2.0","id":7,"method":"x"}"#;
        let stream = format!(
            "Content-Length: {n}\r\n\r\n{body}Content-Length: {n}\r\n\r\n{body}",
            n = body.len()
        );
        let mut reader = std::io::BufReader::new(stream.as_bytes());

        // Two messages on one stream, and then end of input rather than a
        // third: reading past the last frame is how a loop hangs.
        assert_eq!(read_message(&mut reader).unwrap().unwrap()["id"], json!(7));
        assert_eq!(read_message(&mut reader).unwrap().unwrap()["id"], json!(7));
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    /// A header uf does not use is skipped rather than refused. Clients send
    /// `Content-Type`, and the specification says to.
    #[test]
    fn other_headers_are_skipped() {
        let body = r#"{"id":1}"#;
        let stream = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut reader = std::io::BufReader::new(stream.as_bytes());

        assert_eq!(read_message(&mut reader).unwrap().unwrap()["id"], json!(1));
    }

    /// A frame with no length is an error, not a guess.
    /// The linter counts bytes and the protocol counts UTF-16 code units.
    #[test]
    fn a_byte_column_becomes_a_utf16_character() {
        // ASCII: the two agree.
        assert_eq!(character(Some("const x = 1;"), 1), 0);
        assert_eq!(character(Some("const x = 1;"), 7), 6);

        // `π` is two bytes and one UTF-16 unit, so everything after it moves.
        let line = "const π = 1;";
        assert_eq!(character(Some(line), 7), 6);
        assert_eq!(character(Some(line), 9), 7);

        // An emoji is four bytes and *two* UTF-16 units.
        let line = "const 🦀 = 1;";
        assert_eq!(character(Some(line), 7), 6);
        assert_eq!(character(Some(line), 11), 8);

        // A column past the end, and one that lands inside a character, both
        // land somewhere rather than panicking.
        assert_eq!(character(Some("ab"), 99), 2);
        assert_eq!(character(Some("π"), 2), 0);
        assert_eq!(character(None, 5), 4);
    }

    /// A `file://` URI is a path, and the linter is path-sensitive.
    #[test]
    fn a_file_uri_becomes_the_path_it_names() {
        assert_eq!(document_path("file:///a/b.js"), "/a/b.js");
        assert_eq!(
            document_path("file:///My%20Project/app.js"),
            "/My Project/app.js"
        );
        assert_eq!(document_path("/already/a/path.js"), "/already/a/path.js");
        // A stray `%` is not an escape and is left alone.
        assert_eq!(document_path("file:///100%.js"), "/100%.js");
        assert_eq!(document_path("file:///a%zz.js"), "/a%zz.js");
    }

    #[test]
    fn a_message_without_a_length_is_refused() {
        let mut reader = std::io::BufReader::new(&b"Content-Type: x\r\n\r\n{}"[..]);

        assert!(read_message(&mut reader).is_err());
    }
}
