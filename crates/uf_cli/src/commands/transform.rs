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
//! {"id": "…", "code": "…", "map": "…", "css": "…", "diagnostics": []}  // transformed
//! {"id": "…"}                                                          // not uf's to transform
//! {"id": "…", "error": "…", "line": 3, "column": 8}                    // could not be transformed
//! ```
//!
//! Every optional field is absent when it has no value, and a host has to read
//! all of them. `css` is the field that was here and undocumented while the
//! Vite plugin's shim copied the three above it and dropped this one, which is
//! how a StyleX application came to ship class names and no stylesheet
//! (ubugeeei-prod/uf#306).
//!
//! A request is a line so the reader never has to guess where one ends; the
//! code is JSON-escaped, so a newline in the source cannot end a request.

use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use uf_config::{UniflowedConfig, load_config};
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
    /// Whether `import.meta.uf.test` reaches uf's test API in this module.
    ///
    /// Asked for by the hosts `uf test` starts and by nothing else, so a
    /// build's modules cannot pick it up from a service a test run left
    /// behind: the flag travels per request rather than per process.
    in_source_tests: bool,
}

impl Default for RequestOptions {
    /// Production output with a source map — what a build wants when it
    /// says nothing.
    fn default() -> Self {
        Self {
            development: false,
            refresh: false,
            source_map: true,
            in_source_tests: false,
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
    /// The CSS this module's StyleX rules produced, when it has any.
    ///
    /// Absent rather than empty for a module with no styles: the caller keys
    /// "this module has a stylesheet" off the field being there at all, and an
    /// empty string would make it import a stylesheet with nothing in it.
    #[serde(skip_serializing_if = "Option::is_none")]
    css: Option<String>,
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
pub(crate) struct ProjectTransform {
    react_compiler: ReactCompilerMode,
    jsx_import_source: String,
}

impl ProjectTransform {
    pub(crate) fn from_config(config: &uf_config::UniflowedConfig) -> Self {
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
            in_source_tests: request.in_source_tests,
        }
    }
}

/// Where `packages/host/internal/node-hooks.js` files what this service
/// compiles, relative to the project root it was started with.
///
/// Named here because this process is the only *native* thing that knows the
/// directory exists: the entries are written by the loader, in JavaScript, and
/// the loader is the wrong place to sweep them from. A `read_dir` and a `stat`
/// per entry over thousands of files, in JavaScript, on every host start is
/// exactly the repository-wide work `ubugeeei-redundancy.md` says belongs in
/// Rust — and it would be paid by the run that needed it least, the fully warm
/// one that compiles nothing and starts no `uf` at all.
const TRANSFORM_CACHE: [&str; 3] = [".uf", "cache", "transform"];

/// Serve transform requests until stdin closes.
pub(crate) fn transform_service(cwd: &Utf8Path) -> Result<()> {
    // The config loader itself reaches `uf transform` before the config can be
    // evaluated. That bootstrap transform must not recursively ask the static
    // config loader to read the file it is currently compiling.
    let config = if std::env::var_os("UF_TRANSFORM_BOOTSTRAP_CONFIG").is_some() {
        UniflowedConfig::default()
    } else {
        load_config(cwd)?.config
    };
    let project = ProjectTransform::from_config(&config);

    // A host starts this process only when it has something to compile, and
    // compiling is the only thing that adds to the cache — so a sweep here
    // runs exactly when the directory may have grown and never on a run that
    // was answered entirely from disk. That is the whole reason the sweep
    // lives at the service's door rather than on a timer or in `uf clean`.
    //
    // Under `cwd` and deliberately not under `resolved.root`, which is where
    // `uf.config.js` was found and may be an ancestor. The loader computes its
    // cache path from the root it was initialised with and spawns
    // `uf --cwd <that root> transform`, so `cwd` is the one value the two
    // sides are guaranteed to agree on — and sweeping a directory the loader
    // is not writing to would be a bound that quietly bounds nothing.
    let mut directory = cwd.to_path_buf().into_std_path_buf();
    directory.extend(TRANSFORM_CACHE);
    uf_infra::cache::sweep(&directory, uf_infra::cache::CacheBound::default());

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

/// One module's code after StyleX, and the CSS it contributed.
struct Styled {
    code: String,
    css: Option<String>,
}

/// Compile a module's StyleX calls, or leave it exactly as it was.
///
/// A module that does not use StyleX comes back untouched and contributes no
/// CSS, which is the overwhelmingly common case and costs one parse.
///
/// A module the StyleX compiler cannot read is *not* an error. The compiler
/// parses JavaScript that has already been through the whole Flow chain, so a
/// failure here is a disagreement between two parsers about valid code rather
/// than a problem with the user's module — and failing the transform would
/// turn a style that could not be extracted into a build that will not run.
/// The `uf:style` plugin reports its own diagnostics for the cases that are
/// genuinely the module's fault.
fn compile_styles(code: &str) -> Styled {
    match uf_stylex::compile_module(code) {
        Ok(compiled) if compiled.changed => {
            let css = compiled.sheet.to_css();
            Styled {
                code: compiled.code,
                css: (!css.is_empty()).then_some(css),
            }
        }
        Ok(_) | Err(_) => Styled {
            code: code.to_owned(),
            css: None,
        },
    }
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
        Ok(transformed) => {
            // StyleX last, over the JavaScript the Flow chain produced. It is a
            // source-to-source rewrite of `stylex.create` calls into the class
            // names its stylesheet declares, so it wants the code in the shape
            // the browser will see it — after the types are gone and after the
            // React Compiler has had its pass.
            let styled = compile_styles(&transformed.code);
            Reply {
                id: request.id.clone(),
                code: Some(styled.code),
                map: transformed.map,
                css: styled.css,
                diagnostics: transformed.compiler_diagnostics,
                ..Reply::default()
            }
        }
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

/// One module compiled the way a *loader* frames it, for a host that has to be
/// handed the answer as a file rather than asked for it as the module loads.
///
/// Deno has no module hook, so its Flow loader is an ahead-of-time pass that
/// writes the transformed module to disk (see
/// [`crate::commands::deno_loader`]). That pass must produce the same bytes the
/// Node hook produces, or the same source would mean two things depending on
/// which host ran it — which is the property this service exists to provide and
/// would be the first thing a second implementation gave away. So it comes
/// through here, with `handle` above, rather than through a transform call of
/// its own.
///
/// The options are `packages/host/internal/node-hooks.js`'s: development
/// output, a source map, and `in_source_tests` from the run. So is the framing
/// — the map appended as a base64 `sourceMappingURL` comment — which is the one
/// part still written twice, in that file and in this function, because the
/// Node hook does it in JavaScript after the reply arrives.
///
/// # Errors
///
/// The transform's own error, unchanged, so a caller can report the position it
/// carries.
pub(crate) fn compile_for_a_loader(
    project: &ProjectTransform,
    id: &str,
    code: &str,
    in_source_tests: bool,
) -> Result<String, TransformError> {
    let options = project.options(
        id,
        &RequestOptions {
            development: true,
            refresh: false,
            source_map: true,
            in_source_tests,
        },
    );
    let transformed = transform(code, &options)?;
    let styled = compile_styles(&transformed.code);
    let Some(map) = transformed.map else {
        return Ok(styled.code);
    };
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(map.as_bytes());
    Ok(format!(
        "{}\n//# sourceMappingURL=data:application/json;base64,{encoded}\n",
        styled.code
    ))
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

    /// StyleX is uf's style engine, and a module that uses it has to come back
    /// with its rules extracted — not with the `stylex.create` call still in it.
    ///
    /// The compiler crate existed and nothing called it: `uf:style` was in the
    /// resolved pipeline, `uf inspect` listed it, and `stylex.create({...})`
    /// went through `uf transform` untouched and reached the runtime stub,
    /// which throws.
    #[test]
    fn a_stylex_module_comes_back_compiled_and_with_its_css() {
        let source = "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\
                      const styles = stylex.create({ root: { color: \"red\" } });\n\
                      export const used: mixed = stylex.props(styles.root);\n";
        let request = serde_json::json!({ "id": "/app/box.js", "code": source });
        let replies = replies(&format!("{request}\n"));

        let reply = &replies[0];
        assert!(reply["error"].is_null(), "{reply}");

        let css = reply["css"].as_str().unwrap_or_default();
        assert!(
            css.contains("color:red") || css.contains("color: red"),
            "the module's rules must come back as CSS, got {css:?}"
        );

        let code = reply["code"].as_str().unwrap_or_default();
        assert!(
            !code.contains("stylex.create"),
            "the call must be compiled away, got {code}"
        );
    }

    /// A module with no styles must not carry an empty CSS payload: the caller
    /// keys "this module has a stylesheet" off the field being present.
    #[test]
    fn a_module_without_styles_has_no_css() {
        let request = serde_json::json!({
            "id": "/app/plain.js",
            "code": "// @flow\nexport const v: number = 1;\n",
        });
        let replies = replies(&format!("{request}\n"));

        assert!(replies[0]["css"].is_null(), "{}", replies[0]);
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
