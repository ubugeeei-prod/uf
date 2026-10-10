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

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uf_config::{
    ConfigError, UniflowedConfig, discover_config, discover_root, load_config,
    parse_config_projection,
};
use uf_infra::FxHashMap;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
    native_styles: bool,
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
            native_styles: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
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
    style: uf_config::StyleEngine,
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
            style: config.app.builtins.style,
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

/// Where `npm/host/internal/node-hooks.js` files what this service
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

/// How many answers one transform process keeps warm.
///
/// This cache is deliberately small and in-process. It is not the durable
/// `.uf/cache/transform` store; it catches repeated requests inside one dev
/// server when the same module, source and output options reach the service
/// again. The entry is the whole protocol reply, so a hit skips the Flow
/// parse, ESTree render, lowering, Babel conversion, React Compiler and StyleX
/// passes together.
const SERVICE_CACHE_ENTRIES: usize = 64;

/// One compiled reply held by this `uf transform` process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey([u8; 32]);

/// Bounded, least-recently-used answers for one transform service.
#[derive(Debug, Default)]
struct TransformCache {
    entries: FxHashMap<CacheKey, Reply>,
    recent: VecDeque<CacheKey>,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl TransformCache {
    fn key(request: &Request) -> CacheKey {
        let mut hasher = Sha256::new();
        frame(&mut hasher, b"uf-transform-service-cache-v1");
        frame(&mut hasher, request.id.as_bytes());
        frame(&mut hasher, request.code.as_bytes());
        hasher.update([
            u8::from(request.options.development),
            u8::from(request.options.refresh),
            u8::from(request.options.source_map),
            u8::from(request.options.in_source_tests),
            u8::from(request.options.native_styles),
        ]);
        CacheKey(hasher.finalize().into())
    }

    fn get(&mut self, key: CacheKey) -> Option<Reply> {
        let answer = self.entries.get(&key).cloned();
        if answer.is_some() {
            #[cfg(test)]
            {
                self.hits += 1;
            }
            self.touch(key);
        } else {
            #[cfg(test)]
            {
                self.misses += 1;
            }
        }
        answer
    }

    fn insert(&mut self, key: CacheKey, reply: Reply) {
        if self.entries.insert(key, reply).is_none() && self.entries.len() > SERVICE_CACHE_ENTRIES {
            while let Some(old) = self.recent.pop_front() {
                if self.entries.remove(&old).is_some() {
                    break;
                }
            }
        }
        self.touch(key);
    }

    fn touch(&mut self, key: CacheKey) {
        if let Some(index) = self.recent.iter().position(|seen| *seen == key) {
            self.recent.remove(index);
        }
        self.recent.push_back(key);
    }
}

fn frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// The evaluated config's JSON projection, when the host that started this
/// service has one. See [`service_config`].
const TRANSFORM_CONFIG_ENV: &str = "UF_TRANSFORM_CONFIG";

/// Serve transform requests until stdin closes.
pub(crate) fn transform_service(cwd: &Utf8Path) -> Result<()> {
    let config = service_config(
        cwd,
        std::env::var_os("UF_TRANSFORM_BOOTSTRAP_CONFIG").is_some(),
        std::env::var(TRANSFORM_CONFIG_ENV).ok().as_deref(),
    )?;
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
        .map_err(|_| anyhow::anyhow!(uf_infra::cstr!("the transform service panicked")))?
}

/// The config this service compiles under, in order of preference.
///
/// - **Bootstrap.** The config loader itself reaches `uf transform` before the
///   config can be evaluated. That transform must not ask for the file it is
///   compiling, so it gets the defaults.
/// - **The host's projection.** A host that has evaluated `uf.config.js` —
///   `@uniflowed/vite`'s `TransformService`, started by `uf build` and
///   `uf dev` — passes its JSON projection in `UF_TRANSFORM_CONFIG`, which is
///   read exactly as `uf config` reads the same projection.
/// - **The static reader**, for a host with nothing evaluated in hand. A file
///   it refuses — a Vite plugin in `plugins`, a value from `process.env` —
///   gets the defaults rather than a service that exits: every command that
///   reads such a file has already evaluated it, and reported it if it was
///   broken, before any module reaches this service. Exiting here made one
///   Vite plugin enough to leave a project unbuildable
///   (ubugeeei-prod/uf#1674).
fn service_config(
    cwd: &Utf8Path,
    bootstrap: bool,
    projection: Option<&str>,
) -> Result<UniflowedConfig> {
    if bootstrap {
        return Ok(UniflowedConfig::default());
    }
    if let Some(projection) = projection {
        let root = discover_root(cwd);
        let path = discover_config(&root).unwrap_or_else(|| root.join("uf.config.js"));
        let value = serde_json::from_str(projection).with_context(|| {
            uf_infra::cstr!("{TRANSFORM_CONFIG_ENV} is not the JSON projection of {path}")
        })?;
        return Ok(parse_config_projection(&path, value)?);
    }
    match load_config(cwd) {
        Ok(resolved) => Ok(resolved.config),
        Err(ConfigError::Parse { .. } | ConfigError::UnsupportedExpression { .. }) => {
            Ok(UniflowedConfig::default())
        }
        Err(error) => Err(error.into()),
    }
}

fn serve(input: impl Read, out: &mut impl Write, project: &ProjectTransform) -> Result<()> {
    let mut cache = TransformCache::default();
    for line in BufReader::new(input).lines() {
        let line = line.context("reading a transform request")?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(&request, project, &mut cache),
            Err(error) => Reply {
                error: Some(uf_infra::into_string(uf_infra::cstr!(
                    "malformed request: {error}"
                ))),
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

fn handle(request: &Request, project: &ProjectTransform, cache: &mut TransformCache) -> Reply {
    if !is_flow_module(&request.id) {
        return Reply {
            id: request.id.clone(),
            ..Reply::default()
        };
    }
    let key = TransformCache::key(request);
    if let Some(reply) = cache.get(key) {
        return reply;
    }
    let options = project.options(&request.id, &request.options);
    let reply = match transform(&request.code, &options) {
        Ok(transformed) => {
            // StyleX last, over the JavaScript the Flow chain produced. It is a
            // source-to-source rewrite of `stylex.create` calls into the class
            // names its stylesheet declares, so it wants the code in the shape
            // the browser will see it — after the types are gone and after the
            // React Compiler has had its pass.
            let styled = match (project.style, request.options.native_styles) {
                (uf_config::StyleEngine::Off, _) => Styled {
                    code: transformed.code,
                    css: None,
                },
                (uf_config::StyleEngine::StyleX, true) => {
                    match uf_stylex::native::compile_native_module(&transformed.code) {
                        Ok(code) => Styled { code, css: None },
                        Err(error) => {
                            return Reply {
                                id: request.id.clone(),
                                error: Some(error.to_string()),
                                ..Reply::default()
                            };
                        }
                    }
                }
                (uf_config::StyleEngine::StyleX, false) => compile_styles(&transformed.code),
            };
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
    };
    cache.insert(key, reply.clone());
    reply
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

    fn request(id: &str, code: &str) -> Request {
        Request {
            id: id.to_owned(),
            code: code.to_owned(),
            options: RequestOptions::default(),
        }
    }

    const STYLEX_MODULE: &str = "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\
         const styles = stylex.create({ root: { color: \"red\" } });\n\
         export const used: mixed = stylex.props(styles.root);\n";

    fn assert_style_left_uncompiled(project: &ProjectTransform) {
        let reply = handle(
            &request("/app/box.js", STYLEX_MODULE),
            project,
            &mut TransformCache::default(),
        );
        assert!(reply.error.is_none(), "{reply:?}");
        assert!(reply.css.is_none(), "{reply:?}");
        let code = reply.code.unwrap_or_default();
        assert!(code.contains("stylex.create"), "{code}");
    }

    #[test]
    fn identical_requests_are_answered_from_the_process_cache() {
        let request = request(
            "/app/main.js",
            "// @flow\nexport const value: number = 1;\n",
        );
        let mut cache = TransformCache::default();

        let first = handle(&request, &project(), &mut cache);
        let second = handle(&request, &project(), &mut cache);

        assert_eq!(first.code, second.code);
        assert_eq!(first.map, second.map);
        assert_eq!(cache.misses, 1);
        assert_eq!(cache.hits, 1);
    }

    #[test]
    fn transform_options_are_part_of_the_process_cache_key() {
        let source = "// @flow\nexport component App() { return <p />; }\n";
        let production = request("/app/App.js", source);
        let development = Request {
            options: RequestOptions {
                development: true,
                refresh: true,
                ..RequestOptions::default()
            },
            ..request("/app/App.js", source)
        };
        let mut cache = TransformCache::default();

        let plain = handle(&production, &project(), &mut cache).code.unwrap();
        let dev = handle(&development, &project(), &mut cache).code.unwrap();

        assert!(!plain.contains("jsxDEV"), "{plain}");
        assert!(dev.contains("jsxDEV"), "{dev}");
        assert_eq!(cache.misses, 2);
        assert_eq!(cache.hits, 0);
    }

    #[test]
    fn web_and_native_stylex_requests_do_not_share_cached_output() {
        let source = "import { stylex } from '@uniflowed/stylex'; const styles = stylex.create({root: {padding: 12}}); export const props = stylex.props(styles.root);";
        let web = request("/app/Shared.js", source);
        let native = Request {
            options: RequestOptions {
                native_styles: true,
                ..RequestOptions::default()
            },
            ..request("/app/Shared.js", source)
        };
        let mut cache = TransformCache::default();
        let css = handle(&web, &project(), &mut cache);
        let object = handle(&native, &project(), &mut cache);
        assert!(css.css.as_deref().unwrap_or_default().contains("padding:"));
        assert!(object.css.is_none());
        assert!(
            object
                .code
                .as_deref()
                .unwrap_or_default()
                .contains("$$native")
        );
        assert_eq!(handle(&web, &project(), &mut cache).code, css.code);
        assert_eq!(cache.misses, 2);
        assert_eq!(cache.hits, 1);
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
        let replies = replies(uf_infra::cstr!("{request}\n").as_str());

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

    /// `"none"` is a styling choice, not a second compiler. The call stays in
    /// the module, including the native request, and no stylesheet comes back.
    #[test]
    fn style_off_leaves_a_stylex_call_uncompiled() {
        let mut project = project();
        project.style = uf_config::StyleEngine::Off;
        let mut cache = TransformCache::default();
        let web = handle(&request("/app/box.js", STYLEX_MODULE), &project, &mut cache);
        assert!(web.error.is_none(), "{web:?}");
        assert!(web.css.is_none(), "{web:?}");
        let code = web.code.unwrap_or_default();
        assert!(code.contains("stylex.create"), "{code}");

        let native = handle(
            &Request {
                options: RequestOptions {
                    native_styles: true,
                    ..RequestOptions::default()
                },
                ..request("/app/box.js", STYLEX_MODULE)
            },
            &project,
            &mut cache,
        );
        assert!(native.css.is_none(), "{native:?}");
        let native_code = native.code.unwrap_or_default();
        assert!(native_code.contains("stylex.create"), "{native_code}");
        assert!(!native_code.contains("$$native"), "{native_code}");
    }

    /// A module with no styles must not carry an empty CSS payload: the caller
    /// keys "this module has a stylesheet" off the field being present.
    #[test]
    fn a_module_without_styles_has_no_css() {
        let request = serde_json::json!({
            "id": "/app/plain.js",
            "code": "// @flow\nexport const v: number = 1;\n",
        });
        let replies = replies(uf_infra::cstr!("{request}\n").as_str());

        assert!(replies[0]["css"].is_null(), "{}", replies[0]);
    }

    #[test]
    fn replies_come_back_in_request_order() {
        let mut input = String::new();
        for index in 0..8 {
            uf_infra::append!(
                input,
                "{{\"id\": \"/app/m{index}.js\", \"code\": \"export const v{index}: number = {index};\"}}\n"
            );
        }
        let replies = replies(&input);
        assert_eq!(replies.len(), 8);
        for (index, reply) in replies.iter().enumerate() {
            assert_eq!(
                reply["id"],
                uf_infra::into_string(uf_infra::cstr!("/app/m{index}.js"))
            );
            assert!(
                reply["code"]
                    .as_str()
                    .unwrap()
                    .contains(uf_infra::cstr!("v{index} = {index}").as_str())
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

    /// A project directory holding `uf.config.js` with `source` in it.
    fn configured(source: &str) -> (tempfile::TempDir, camino::Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("uf.config.js"), source).unwrap();
        (dir, root)
    }

    /// The shape ubugeeei-prod/uf#1674 was found with: a Vite plugin and a
    /// value from `process.env`, which the static reader refuses. The service
    /// used to exit on it, and every build that started one failed.
    const EVALUATED_ONLY: &str = "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\nimport { stamp } from \"./plugins/stamp.js\";\n\nexport default defineConfig({\n  app: { router: { basePath: process.env.BASE_PATH } },\n  plugins: [stamp(\"hello\")],\n});\n";

    #[test]
    fn a_config_the_static_reader_refuses_compiles_under_the_defaults() {
        let (_dir, root) = configured(EVALUATED_ONLY);
        let config = service_config(&root, false, None).unwrap();
        assert!(config.app.builtins.react_compiler.enabled);
    }

    #[test]
    fn the_hosts_projection_is_the_config_the_service_compiles_under() {
        let (_dir, root) = configured(EVALUATED_ONLY);
        let config = service_config(
            &root,
            false,
            Some(r#"{"app":{"builtins":{"reactCompiler":{"enabled":false}}},"plugins":["stamp"]}"#),
        )
        .unwrap();
        assert!(!config.app.builtins.react_compiler.enabled);
    }

    /// The projection wins over a file the static reader *can* read, because
    /// it is the config the host actually evaluated.
    #[test]
    fn the_projection_is_preferred_to_the_static_reader() {
        let (_dir, root) = configured(
            "export default { app: { builtins: { reactCompiler: { enabled: true } } } };\n",
        );
        let config = service_config(
            &root,
            false,
            Some(r#"{"app":{"builtins":{"reactCompiler":{"enabled":false}}}}"#),
        )
        .unwrap();
        assert!(!config.app.builtins.react_compiler.enabled);
    }

    #[test]
    fn a_static_config_is_still_read_without_a_projection() {
        let (_dir, root) = configured(
            "export default { app: { builtins: { reactCompiler: { enabled: false } } } };\n",
        );
        let config = service_config(&root, false, None).unwrap();
        assert!(!config.app.builtins.react_compiler.enabled);
    }

    /// `uf.config.js` is what a project writes. The service has to read
    /// `"none"` from that file, not only from a field a test assigned.
    #[test]
    fn style_none_in_the_config_file_reaches_the_transform() {
        let (_dir, root) =
            configured("export default { app: { builtins: { style: \"none\" } } };\n");
        let project = ProjectTransform::from_config(&service_config(&root, false, None).unwrap());
        assert_eq!(project.style, uf_config::StyleEngine::Off);
        assert_style_left_uncompiled(&project);
    }

    /// Vite passes the evaluated config as JSON. `"none"` has to survive that
    /// projection, which is the path `uf dev` and `uf build` actually take.
    #[test]
    fn the_hosts_style_projection_reaches_the_transform() {
        let (_dir, root) = configured("export default {};\n");
        let project = ProjectTransform::from_config(
            &service_config(
                &root,
                false,
                Some(r#"{"app":{"builtins":{"style":"none"}}}"#),
            )
            .unwrap(),
        );
        assert_eq!(project.style, uf_config::StyleEngine::Off);
        assert_style_left_uncompiled(&project);
    }

    #[test]
    fn an_unknown_style_engine_is_not_silently_the_default() {
        let (_dir, root) = configured("export default {};\n");
        let error = service_config(
            &root,
            false,
            Some(r#"{"app":{"builtins":{"style":"tailwind"}}}"#),
        )
        .unwrap_err();
        assert!(error.to_string().contains("tailwind"), "{error}");
    }

    #[test]
    fn the_bootstrap_transform_reads_nothing() {
        let (_dir, root) = configured(
            "export default { app: { builtins: { reactCompiler: { enabled: false } } } };\n",
        );
        let config = service_config(&root, true, Some("not json")).unwrap();
        assert!(config.app.builtins.react_compiler.enabled);
    }

    #[test]
    fn a_malformed_projection_is_an_error_rather_than_the_defaults() {
        let (_dir, root) = configured("export default {};\n");
        let error = service_config(&root, false, Some("{ not json")).unwrap_err();
        assert!(error.to_string().contains("UF_TRANSFORM_CONFIG"), "{error}");
    }
}
