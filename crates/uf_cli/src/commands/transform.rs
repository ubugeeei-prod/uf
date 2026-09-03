//! `uf transform` — the Flow → JavaScript transform, as a service.
//!
//! Vite runs its plugins in JavaScript and uf's transform is native. A plugin
//! that spawned `uf` per module would pay process start-up thousands of times
//! in one build, so this is a long-lived process instead: one per run,
//! newline-delimited JSON in, newline-delimited JSON out, replies in request
//! order. `@uniflowed/vite`, the Node loader hook and the Bun preload all
//! speak this protocol, which is what makes every host produce the same
//! module from the same source.
//!
//! Request, one per line:
//!
//! ```json
//! {"id": "/abs/path.js", "code": "…", "options": {"development": true, "refresh": true}}
//! ```
//!
//! Reply, one per line, in order:
//!
//! ```json
//! {"id": "…", "code": "…", "map": "…", "diagnostics": []}   // transformed
//! {"id": "…"}                                               // not uf's to transform
//! {"id": "…", "error": "…", "line": 3, "column": 8}         // could not be transformed
//! ```
//!
//! A request is a line so the reader never has to guess where one ends; the
//! code is JSON-escaped, so a newline in the source cannot end a request.

use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use uf_config::load_config;
use uf_transform::{
    CompilerDiagnostic, ReactCompilerMode, TransformError, TransformOptions, is_flow_module,
    transform,
};

/// Stack for the service thread.
///
/// Every stage walks a tree recursively — the parser, the lowering passes,
/// the compiler — and a deeply nested module must fail with a clear error,
/// not by overflowing the main thread's stack. 512 MiB is reserved, not
/// committed; an ordinary module touches a few hundred kilobytes of it.
const SERVICE_STACK_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Request {
    id: String,
    code: String,
    #[serde(default)]
    options: RequestOptions,
}

/// The per-request knobs; everything else comes from `uf.config.js`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RequestOptions {
    development: bool,
    refresh: bool,
    source_map: bool,
}

impl Default for RequestOptions {
    /// Production output with a source map — what a build wants when it
    /// says nothing.
    fn default() -> Self {
        Self {
            development: false,
            refresh: false,
            source_map: true,
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct Reply {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<CompilerDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u32>,
}

/// What the project's config says about every transform.
#[derive(Debug, Clone)]
struct ProjectTransform {
    react_compiler: ReactCompilerMode,
    jsx_import_source: String,
}

impl ProjectTransform {
    fn from_config(config: &uf_config::UniflowedConfig) -> Self {
        let compiler = &config.app.builtins.react_compiler;
        Self {
            react_compiler: if compiler.enabled {
                ReactCompilerMode::Syntax
            } else {
                ReactCompilerMode::Off
            },
            jsx_import_source: String::from("react"),
        }
    }

    fn options(&self, id: &str, request: &RequestOptions) -> TransformOptions {
        TransformOptions {
            filename: id.to_owned(),
            development: request.development,
            refresh: request.development && request.refresh,
            react_compiler: self.react_compiler,
            jsx_import_source: self.jsx_import_source.clone(),
            source_map: request.source_map,
        }
    }
}

/// Serve transform requests until stdin closes.
pub(crate) fn transform_service(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let project = ProjectTransform::from_config(&resolved.config);

    std::thread::Builder::new()
        .name(String::from("uf-transform"))
        .stack_size(SERVICE_STACK_BYTES)
        .spawn(move || {
            let stdin = std::io::stdin().lock();
            let mut stdout = std::io::stdout().lock();
            serve(stdin, &mut stdout, &project)
        })
        .context("failed to start the transform service thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("the transform service panicked"))?
}

fn serve(input: impl Read, out: &mut impl Write, project: &ProjectTransform) -> Result<()> {
    for line in BufReader::new(input).lines() {
        let line = line.context("reading a transform request")?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(&request, project),
            Err(error) => Reply {
                error: Some(format!("malformed request: {error}")),
                ..Reply::default()
            },
        };
        serde_json::to_writer(&mut *out, &reply).context("writing a transform reply")?;
        out.write_all(b"\n")?;
        // A build blocks on this reply, so it cannot wait for a full buffer.
        out.flush()?;
    }
    Ok(())
}

fn handle(request: &Request, project: &ProjectTransform) -> Reply {
    if !is_flow_module(&request.id) {
        return Reply {
            id: request.id.clone(),
            ..Reply::default()
        };
    }
    let options = project.options(&request.id, &request.options);
    match transform(&request.code, &options) {
        Ok(transformed) => Reply {
            id: request.id.clone(),
            code: Some(transformed.code),
            map: transformed.map,
            diagnostics: transformed.compiler_diagnostics,
            ..Reply::default()
        },
        Err(error) => {
            let (line, column) = match &error {
                TransformError::Syntax { line, column, .. } => (Some(*line), Some(*column)),
                TransformError::Lowering { line, column, .. } => (*line, *column),
                _ => (None, None),
            };
            Reply {
                id: request.id.clone(),
                error: Some(error.to_string()),
                line,
                column,
                ..Reply::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectTransform {
        ProjectTransform::from_config(&uf_config::UniflowedConfig::default())
    }

    fn replies(input: &str) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out, &project()).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn replies_come_back_in_request_order() {
        let mut input = String::new();
        for index in 0..8 {
            input.push_str(&format!(
                "{{\"id\": \"/app/m{index}.js\", \"code\": \"export const v{index}: number = {index};\"}}\n"
            ));
        }
        let replies = replies(&input);
        assert_eq!(replies.len(), 8);
        for (index, reply) in replies.iter().enumerate() {
            assert_eq!(reply["id"], format!("/app/m{index}.js"));
            assert!(
                reply["code"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("v{index} = {index}"))
            );
        }
    }

    #[test]
    fn a_third_party_module_is_left_alone() {
        let replies = replies("{\"id\": \"/app/node_modules/react/index.js\", \"code\": \"x\"}\n");
        assert!(replies[0]["code"].is_null());
        assert!(replies[0]["error"].is_null());
    }

    #[test]
    fn a_syntax_error_is_reported_with_its_position() {
        let replies = replies("{\"id\": \"/app/bad.js\", \"code\": \"const a = ;\"}\n");
        assert!(replies[0]["error"].as_str().is_some());
        assert_eq!(replies[0]["line"], 1);
    }

    #[test]
    fn a_blank_line_is_skipped_and_garbage_is_named() {
        let replies = replies("\nnot json\n");
        assert_eq!(replies.len(), 1);
        assert!(replies[0]["error"].as_str().unwrap().contains("malformed"));
    }

    #[test]
    fn development_requests_get_refresh_registrations() {
        let replies = replies(
            "{\"id\": \"/app/A.js\", \"code\": \"export component A() { return <p />; }\", \"options\": {\"development\": true, \"refresh\": true}}\n",
        );
        let code = replies[0]["code"].as_str().unwrap();
        assert!(code.contains("$RefreshReg$"), "{code}");
        assert!(code.contains("jsxDEV"), "{code}");
        assert!(replies[0]["map"].as_str().is_some());
    }
}
