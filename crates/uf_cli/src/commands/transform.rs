//! `uf transform` — uf's module transform, as a service Vite can pipe through.
//!
//! Vite runs its plugins in JavaScript, and uf's transform is native Rust. A
//! plugin that spawned `uf` per module would pay process startup thousands of
//! times in one build, so this is a service instead: one process for the whole
//! run, newline-delimited JSON in, newline-delimited JSON out.
//!
//! Request: `{"id": "/abs/path.js", "code": "…"}`
//! Reply:   `{"id": "…", "code": "…"}` — the transformed module
//!          `{"id": "…"}`              — nothing to do, use the source as-is
//!          `{"id": "…", "error": "…"}` — the module could not be transformed
//!
//! One object per line, in the order they arrived. A request is a line so the
//! reader never has to guess where one ends; the code is JSON-escaped, so a
//! newline in the source cannot be mistaken for the end of a request.

use std::io::{BufRead, BufReader, StdinLock, Write};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use uf_bundler::{is_project_module, transform_module};
use uf_config::load_config;
use uf_jsx::JsxOptions;

#[derive(Debug, Deserialize)]
struct Request {
    id: String,
    code: String,
}

#[derive(Debug, Serialize)]
struct Reply {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Serve transform requests until stdin closes.
pub(crate) fn transform(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let jsx = JsxOptions::from_config(&resolved.config);

    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    serve(stdin, &mut stdout, &jsx)
}

fn serve(stdin: StdinLock<'_>, out: &mut impl Write, jsx: &JsxOptions) -> Result<()> {
    for line in BufReader::new(stdin).lines() {
        let line = line.context("reading a transform request")?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(request, jsx),
            Err(error) => Reply {
                id: String::new(),
                code: None,
                error: Some(format!("malformed request: {error}")),
            },
        };
        serde_json::to_writer(&mut *out, &reply).context("writing a transform reply")?;
        out.write_all(b"\n")?;
        // A build blocks on this reply, so it cannot wait for a full buffer.
        out.flush()?;
    }
    Ok(())
}

fn handle(request: Request, jsx: &JsxOptions) -> Reply {
    if !is_project_module(&request.id) {
        return Reply {
            id: request.id,
            code: None,
            error: None,
        };
    }
    match transform_module(&request.id, &request.code, jsx) {
        Ok(code) => Reply {
            id: request.id,
            code,
            error: None,
        },
        Err(error) => Reply {
            id: request.id,
            code: None,
            error: Some(error.to_string()),
        },
    }
}
