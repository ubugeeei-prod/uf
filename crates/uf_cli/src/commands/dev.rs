//! `uf dev` and `uf lsp`: the two commands that hold a socket or a protocol.
//!
//! `uf dev` is Vite's dev server, started through `@uniflowed/vite`'s driver
//! on the project's JavaScript host (see [`super::vite`]). Vite owns the
//! module graph, hot module replacement and the transform pipeline; uf owns
//! the terminal, the generated route types, and the transform itself, which
//! the driver reaches back into through `uf transform`.
//!
//! `uf lsp` is the other half, and this file is its protocol: framing, the
//! dispatch loop, the open-document store, and the conversion between uf's
//! byte positions and the protocol's UTF-16 ones. Two questions are large
//! enough to answer somewhere else, because each is a judgement about what uf
//! is willing to claim rather than a detail of the wire format:
//!
//! - [`crate::fix`] — which lint diagnostics have an edit uf is willing to
//!   make, in which tier, and which deliberately have none. It lives beside
//!   the commands rather than under this one because `uf lint --fix` reaches
//!   it too, and a second copy of that judgement is a second answer.
//! - [`hover`] — what uf can honestly say about the thing under the cursor,
//!   and what it cannot say yet.

mod hover;
mod rsc;

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use serde_json::{Value, json};
use uf_config::{FmtConfig, UniflowedConfig, load_config};
use uf_infra::FxHashMap;
use uf_lib::NativeModule;
use uf_router::write_router_manifest;
use uf_rsc::RSC_MANIFEST_ENV;
use uf_term::{KeyValue, Status, Tone};

use crate::commands::lint::identifier_span;
use crate::commands::vite::{Driver, Event, package_dir, render_error, render_log, resolve_host};
use crate::support::{DEVELOPMENT, env_file_list, plural, project_env, project_label};
use crate::ui::Ui;

use crate::fix::{self, FORMATTED_AWAY, Fix, Safety};
use rsc::RscReport;

/// What `uf dev` was asked to do.
#[derive(Debug, Clone, Default)]
pub(crate) struct DevArgs {
    /// Bind a routable address instead of loopback.
    pub(crate) host: Option<String>,
    /// Listen on this port instead of `dev.port`.
    pub(crate) port: Option<u16>,
    /// Run in this mode instead of `development`.
    pub(crate) mode: Option<String>,
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

    let env = project_env(&resolved, args.mode.as_deref(), DEVELOPMENT)?;
    let driver_args = driver_args(args.host.as_deref(), args.port);
    // Before the driver, not after: `@uniflowed/vite` reads the analysis to
    // decide which routes keep a page in the client route table, and it reads
    // it as it generates that table — which happens on the first request. A
    // manifest written afterwards would leave the first page load splitting
    // nothing while every later one split, and a dev server that disagrees with
    // itself is worse than one that never splits.
    let mut server_components = RscReport::new(&root);
    server_components.prime();
    let mut driver = Driver::spawn(
        &host,
        &package,
        &root,
        "dev",
        &driver_args,
        &env,
        &[(RSC_MANIFEST_ENV, server_components.manifest_path().as_str())],
    )?;

    let host_name = host.name();
    let project = project_label(&root).to_string();
    // The mode is on the banner because it is the thing that decides which
    // `.env` files were read, and a person looking at a value they did not
    // expect should not have to reason about which of four files won.
    let mode = env.mode().to_owned();
    let env_files = env_file_list(&root, &env);
    ui.render(|renderer, out| {
        renderer.banner(out, "uf dev", Some(&project));
        renderer.blank(out);
        let mut rows = vec![
            KeyValue::new("engine", "vite"),
            KeyValue::toned("host", host_name, Tone::Muted),
            KeyValue::new("mode", &mode),
            KeyValue::toned(
                "transform",
                "uf transform (official Flow parser, React Compiler, oxc)",
                Tone::Muted,
            ),
        ];
        if let Some(files) = &env_files {
            rows.push(KeyValue::toned("env files", files, Tone::Path));
        }
        renderer.key_values(out, 2, &rows);
    });

    while let Some(event) = driver.next_event()? {
        match event {
            Event::Listening {
                local,
                network,
                routes,
                // `uf dev` dispatches handlers through the same table it
                // renders pages from, so there is nothing to report that the
                // route count does not already cover.
                handlers: _,
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
                server_components.report(ui);
            }
            // Vite saw a module change. The RSC graph is a whole-project
            // property, so there is nothing to patch and nothing to defer:
            // rescan, and say something only if the answer moved.
            Event::SourceChanged => server_components.report(ui),
            Event::Log { level, message } => render_log(ui, level, &message),
            Event::Error(error) => {
                let failure = render_error(ui, &root, &error);
                let _ = driver.finish("uf dev");
                return Err(failure);
            }
            Event::ConfigLoaded { .. }
            | Event::Phase { .. }
            | Event::Page { .. }
            | Event::PageFailed { .. }
            | Event::RscSplit { .. }
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
///
/// Code actions, from [`fix`]: the quick fixes uf can apply to its own
/// diagnostics without guessing at intent, plus `source.fixAll.uf` for all of
/// them at once. `source.organizeImports` is **not** advertised, because uf
/// has no opinion about import order to organise them by — no sorter exists in
/// any crate, and a server that advertised one would be promising an editor
/// something uf cannot do.
///
/// Hover, from [`hover`]: the rule behind a diagnostic, what an import
/// specifier names, and what a rule id in a suppression comment means. Not the
/// type at a position; that module's header says exactly what is missing.
///
/// # Being hard to wedge
///
/// The loop must survive whatever arrives on the pipe, because the thing on
/// the other end is a program. A body that is not JSON is answered with
/// `-32700` and the loop continues, since the frame header already said how
/// many bytes to consume and the stream is therefore still in sync. A
/// `Content-Length` larger than [`MAX_MESSAGE_BYTES`] is refused rather than
/// allocated. A request with no `method` gets `-32600`, a method uf does not
/// serve gets `-32601`, and a request whose params are unusable gets `-32602`.
/// Notifications get none of those, because a notification has no id to answer.
pub(crate) fn lsp(cwd: &Utf8Path) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    if stdin.is_terminal() {
        writeln!(stdout, "uf lsp: JSON-RPC stdio server ready")
            .with_context(|| "failed to write LSP banner")?;
        return Ok(());
    }

    let mut reader = std::io::BufReader::new(stdin.lock());
    let mut documents: FxHashMap<String, Document> = FxHashMap::default();
    let mut shutting_down = false;
    // Once, not once per keystroke. A server lints on every change and formats
    // as often as the editor asks, and reading `uf.config.js` from disk each
    // time would put a file read on the path a keystroke can trigger.
    // `--cwd` rather than the process's directory: an editor starts one server
    // per workspace folder and has every reason to say which one, and a server
    // that read `.` instead answered with uf's defaults while looking like it
    // had read the project's `uf.config.js`.
    let config =
        load_config(cwd).map_or_else(|_| UniflowedConfig::default(), |resolved| resolved.config);
    let fmt = config.fmt.clone();
    // Same reasoning: `uf_lib::builtin_modules` rebuilds the whole registry on
    // every call, and a hover happens on mouse-move.
    let modules = uf_lib::builtin_modules();

    while let Some(frame) = read_message(&mut reader)? {
        let message = match frame {
            Frame::Message(message) => message,
            Frame::Malformed => {
                respond_error(
                    &mut stdout,
                    Value::Null,
                    PARSE_ERROR,
                    "the message body was not valid JSON",
                )?;
                continue;
            }
        };

        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A request without a method is not a request. A notification
            // without one is nothing at all, and gets nothing back.
            if let Some(id) = id {
                respond_error(
                    &mut stdout,
                    id,
                    INVALID_REQUEST,
                    "the message carried no `method`",
                )?;
            }
            continue;
        };

        // After `shutdown` only `exit` is valid; the specification says to
        // refuse the rest rather than serve them.
        if shutting_down && method != "exit" {
            if let Some(id) = id {
                respond_error(
                    &mut stdout,
                    id,
                    INVALID_REQUEST,
                    "the server has been asked to shut down",
                )?;
            }
            continue;
        }

        match method {
            "initialize" => respond(
                &mut stdout,
                id,
                json!({
                    "serverInfo": { "name": "uf-lsp", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": {
                        "textDocumentSync": 1,
                        "documentFormattingProvider": true,
                        "hoverProvider": true,
                        "codeActionProvider": {
                            "codeActionKinds": [QUICK_FIX, FIX_ALL],
                        },
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
                    let document = Document::lint(&uri, text, &config);
                    publish_diagnostics(&mut stdout, &uri, &document)?;
                    documents.insert(uri, document);
                }
            }
            "textDocument/didChange" => {
                if let Some((uri, text)) = changed_document(&message) {
                    let document = Document::lint(&uri, text, &config);
                    publish_diagnostics(&mut stdout, &uri, &document)?;
                    documents.insert(uri, document);
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
                let document = document_uri(&message).and_then(|uri| documents.get(&uri));
                let edits = match document {
                    Some(document) => format_edits(&document.text, &fmt),
                    None => Value::Null,
                };
                respond(&mut stdout, id, edits)?;
            }
            "textDocument/codeAction" => {
                let answer = code_actions(&message, &documents, &config, &fmt);
                answer_request(&mut stdout, id, answer)?;
            }
            "textDocument/hover" => {
                let answer = hover_answer(&message, &documents, &modules);
                answer_request(&mut stdout, id, answer)?;
            }
            // A request uf does not serve is answered as one, not ignored: an
            // editor waiting on an id it never gets back is a hang. A
            // *notification* uf does not serve is dropped, because answering
            // one is itself a protocol violation.
            _ => {
                if let Some(id) = id {
                    respond_error(
                        &mut stdout,
                        id,
                        METHOD_NOT_FOUND,
                        &format!("uf lsp does not serve `{method}`"),
                    )?;
                }
            }
        }
    }

    Ok(())
}

/// JSON-RPC parse error: the body was not JSON.
const PARSE_ERROR: i64 = -32700;
/// JSON-RPC invalid request: the body was JSON but not a request.
const INVALID_REQUEST: i64 = -32600;
/// JSON-RPC method not found: a request for something uf does not serve.
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC invalid params: the request named a method but not a usable subject.
const INVALID_PARAMS: i64 = -32602;

/// Code action kind for a fix to one diagnostic.
const QUICK_FIX: &str = "quickfix";
/// Code action kind for "fix everything in this file that uf can fix".
///
/// Namespaced under `source.fixAll` so an editor configured with
/// `codeActionsOnSave: { "source.fixAll": true }` matches it: the
/// specification's kinds are hierarchical, and a request for a parent kind
/// selects its children.
const FIX_ALL: &str = "source.fixAll.uf";

/// Send a request's answer, or the error that stopped it being one.
///
/// The error half only applies to requests. A notification named
/// `textDocument/hover` is malformed, but answering it would be worse than
/// ignoring it.
fn answer_request(
    out: &mut impl Write,
    id: Option<Value>,
    answer: Result<Value, String>,
) -> Result<()> {
    match answer {
        Ok(result) => respond(out, id, result),
        Err(detail) => match id {
            Some(id) => respond_error(out, id, INVALID_PARAMS, &detail),
            None => Ok(()),
        },
    }
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
fn publish_diagnostics(out: &mut impl Write, uri: &str, document: &Document) -> Result<()> {
    let Some(report) = document.diagnostics.as_deref() else {
        return Ok(());
    };

    let lines: Vec<&str> = document.text.lines().collect();
    let diagnostics: Vec<Value> = report
        .iter()
        .map(|diagnostic| encode_diagnostic(&lines, diagnostic))
        .collect();

    notify(
        out,
        "textDocument/publishDiagnostics",
        json!({ "uri": uri, "diagnostics": diagnostics }),
    )
}

/// One open document: the text the editor holds, and what linting it said.
///
/// The diagnostics are kept rather than recomputed because they cannot change
/// without the text changing, and the text only changes through `didOpen` and
/// `didChange` — both of which already lint, to publish. Hover and code
/// actions then read the answer instead of asking for it again, which matters
/// because a hover is asked on mouse-move and a parse is not free.
struct Document {
    /// The document as the editor last sent it, in full (`textDocumentSync: 1`).
    text: String,
    /// What linting [`Document::text`] said, or [`None`] when the linter could
    /// not read it at all.
    ///
    /// The two are different answers and the editor treats them differently:
    /// `Some(vec![])` clears the file's markers, `None` leaves whatever the
    /// editor was last told. Half a keystroke into a rename a document is
    /// often not anything yet, and a server that fails there is a server that
    /// stops.
    diagnostics: Option<Vec<uf_lint::Diagnostic>>,
}

impl Document {
    /// Take the editor's text and lint it.
    ///
    /// The single place the LSP calls the linter for an open document.
    /// Diagnostics, quick fixes and hover all need the same answer for the
    /// same text, and asking three different ways is how an editor ends up
    /// offering a fix for a diagnostic it is not showing.
    fn lint(uri: &str, text: String, config: &UniflowedConfig) -> Self {
        let diagnostics = lint_text(uri, &text, config);
        Self { text, diagnostics }
    }
}

/// Lint some text as the file `uri` names, or [`None`] if the linter refused.
fn lint_text(
    uri: &str,
    source: &str,
    config: &UniflowedConfig,
) -> Option<Vec<uf_lint::Diagnostic>> {
    let file = uf_lint::SourceFile {
        path: document_path(uri),
        source: source.to_owned(),
    };
    uf_lint::lint_source(&file, config)
        .ok()
        .map(|report| report.diagnostics)
}

/// One lint diagnostic as the protocol spells it.
fn encode_diagnostic(lines: &[&str], diagnostic: &uf_lint::Diagnostic) -> Value {
    json!({
        "range": diagnostic_range(lines, diagnostic),
        "severity": match diagnostic.severity {
            uf_lint::Severity::Error => 1,
            uf_lint::Severity::Warn => 2,
        },
        "source": "uf",
        "code": diagnostic.rule,
        "message": diagnostic.message,
    })
}

/// The range a diagnostic covers, in the protocol's zero-based UTF-16 units.
fn diagnostic_range(lines: &[&str], diagnostic: &uf_lint::Diagnostic) -> Value {
    let line = diagnostic.line.saturating_sub(1);
    let text = lines.get(line).copied();
    let span = text.map_or(1, |text| identifier_span(text, diagnostic.column));
    json!({
        "start": { "line": line, "character": character(text, diagnostic.column) },
        "end": { "line": line, "character": character(text, diagnostic.column + span) },
    })
}

/// Every action the editor may take over `range`, as `CodeAction`s.
///
/// The diagnostics come from re-linting the document the server currently
/// holds rather than from `context.diagnostics`, which is whatever the editor
/// was last told and can be a keystroke behind. An action built from a stale
/// diagnostic is an edit landing in the wrong place, and the whole point of
/// this request is that the editor applies the edit without asking again.
///
/// `Err` carries the reason the request could not be served at all, which the
/// caller turns into `-32602`. A document the server has never been told about
/// is not that: it is simply a document with no actions.
fn code_actions(
    message: &Value,
    documents: &FxHashMap<String, Document>,
    config: &UniflowedConfig,
    fmt: &FmtConfig,
) -> Result<Value, String> {
    let params = message
        .get("params")
        .ok_or_else(|| String::from("`textDocument/codeAction` needs `params`"))?;
    let uri = document_uri(message)
        .ok_or_else(|| String::from("`params.textDocument.uri` is required"))?;
    let range = params
        .get("range")
        .and_then(position_range)
        .ok_or_else(|| String::from("`params.range` is required"))?;
    let only = params
        .get("context")
        .and_then(|context| context.get("only"))
        .map(|only| {
            only.as_array()
                .map(|kinds| {
                    kinds
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });

    let Some(document) = documents.get(&uri) else {
        return Ok(json!([]));
    };
    let Some(diagnostics) = document.diagnostics.as_deref() else {
        return Ok(json!([]));
    };
    let source = document.text.as_str();
    let lines: Vec<&str> = source.lines().collect();

    let mut actions = Vec::new();

    // Quick fixes: one per in-range diagnostic that has a mechanical answer.
    let in_range: Vec<&uf_lint::Diagnostic> = diagnostics
        .iter()
        .filter(|diagnostic| overlaps(&range, &lines, diagnostic))
        .collect();

    if wanted(only.as_deref(), QUICK_FIX) {
        for diagnostic in &in_range {
            let text = lines.get(diagnostic.line.saturating_sub(1)).copied();
            if let Some(fix) = text.and_then(|text| fix::fix_for(diagnostic, text)) {
                actions.push(json!({
                    "title": fix.title,
                    "kind": QUICK_FIX,
                    "diagnostics": [encode_diagnostic(&lines, diagnostic)],
                    // A lightbulb a person clicks is that person asking, so
                    // both tiers are offered here. `isPreferred` is not: it is
                    // what "apply the obvious fix" binds to, and an edit whose
                    // correctness rests on something the rule could not check
                    // is not the obvious one.
                    "isPreferred": fix.safety == Safety::Safe,
                    "edit": { "changes": { &uri: [fix_edit(&lines, &fix)] } },
                }));
            }
        }

        // The whitespace rules, whose answer is `uf fmt` rather than an edit of
        // uf's own devising — but only where the formatter really does remove
        // them.
        let whitespace: Vec<&uf_lint::Diagnostic> = in_range
            .iter()
            .copied()
            .filter(|diagnostic| FORMATTED_AWAY.contains(&diagnostic.rule))
            .collect();
        if !whitespace.is_empty()
            && let Some((output, cleared)) =
                formatter_answer(&uri, source, diagnostics, fmt, config)
        {
            let answered: Vec<Value> = whitespace
                .iter()
                .filter(|diagnostic| cleared.contains(&diagnostic.rule))
                .map(|diagnostic| encode_diagnostic(&lines, diagnostic))
                .collect();
            if !answered.is_empty() {
                actions.push(json!({
                    "title": "Format this document with uf fmt",
                    "kind": QUICK_FIX,
                    "diagnostics": answered,
                    "edit": {
                        "changes": { &uri: [whole_document_edit(source, &output)] },
                    },
                }));
            }
        }
    }

    // `source.fixAll`: every mechanical fix in the document, not just the ones
    // under the cursor. Safe ones only, and through the same planner the CLI
    // uses, so the edits are computed against the document as it is now and no
    // two of them overlap — which is what lets them be applied as one batch.
    //
    // Safe only because this is the unattended path: an editor configured with
    // `codeActionsOnSave` runs it on every save, and an edit that can change
    // what the program does must be asked for rather than arrive with a
    // keystroke somebody has stopped thinking about.
    if wanted(only.as_deref(), FIX_ALL) {
        let edits: Vec<Value> = fix::plan(source, diagnostics, false)
            .iter()
            .map(|fix| fix_edit(&lines, fix))
            .collect();
        if !edits.is_empty() {
            actions.push(json!({
                "title": "Fix all uf lint problems in this file",
                "kind": FIX_ALL,
                "edit": { "changes": { &uri: edits } },
            }));
        }
    }

    Ok(Value::Array(actions))
}

/// One [`Fix`] as a `TextEdit`.
fn fix_edit(lines: &[&str], fix: &Fix) -> Value {
    let text = lines.get(fix.line).copied();
    json!({
        "range": {
            "start": { "line": fix.line, "character": character(text, fix.start + 1) },
            "end": { "line": fix.line, "character": character(text, fix.end + 1) },
        },
        "newText": fix.replacement,
    })
}

/// What `uf fmt` makes of this document, when formatting is the answer to a
/// whitespace diagnostic: the formatted text, and which of [`FORMATTED_AWAY`]'s
/// rules it actually clears.
///
/// Asked once per request rather than once per diagnostic: one format and one
/// re-lint answer it for every whitespace diagnostic in the document, and the
/// formatted text comes back so the action's edit does not format it again.
///
/// It has to be asked at all because the two tools genuinely disagree in one
/// case. `uf fmt` reprints from the syntax tree and therefore preserves the
/// inside of a template literal, while `uniflowed/no-trailing-whitespace`
/// measures the raw line and so reports trailing spaces inside one. Offering
/// "format this document" there would be offering a fix that does not fix.
/// A rule is offered only when *every* diagnostic of it disappears, so a file
/// with one fixable and one unfixable instance offers nothing rather than
/// something that half works.
fn formatter_answer(
    uri: &str,
    source: &str,
    before: &[uf_lint::Diagnostic],
    fmt: &FmtConfig,
    config: &UniflowedConfig,
) -> Option<(String, Vec<&'static str>)> {
    let formatted = uf_fmt::format_source(source, fmt).ok()?;
    if !formatted.changed {
        return None;
    }
    let after = lint_text(uri, &formatted.output, config)?;

    let cleared: Vec<&'static str> = FORMATTED_AWAY
        .into_iter()
        .filter(|rule| before.iter().any(|diagnostic| diagnostic.rule == *rule))
        .filter(|rule| !after.iter().any(|diagnostic| diagnostic.rule == *rule))
        .collect();
    (!cleared.is_empty()).then_some((formatted.output, cleared))
}

/// Whether a code action of `kind` was asked for.
///
/// Kinds are hierarchical, so `source.fixAll` selects `source.fixAll.uf`. A
/// missing `only` means "everything"; an empty one means "nothing", which is
/// what an editor sends when it has filtered every kind out.
fn wanted(only: Option<&[String]>, kind: &str) -> bool {
    let Some(only) = only else {
        return true;
    };
    only.iter().any(|wanted| {
        kind == wanted
            || kind
                .strip_prefix(wanted.as_str())
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// A `Range` as `(start line, start character, end line, end character)`.
fn position_range(range: &Value) -> Option<(usize, usize, usize, usize)> {
    let point = |name: &str| -> Option<(usize, usize)> {
        let point = range.get(name)?;
        Some((
            usize::try_from(point.get("line")?.as_u64()?).ok()?,
            usize::try_from(point.get("character")?.as_u64()?).ok()?,
        ))
    };
    let (start_line, start_character) = point("start")?;
    let (end_line, end_character) = point("end")?;
    Some((start_line, start_character, end_line, end_character))
}

/// Whether a diagnostic's range touches the range the editor asked about.
///
/// Touching, not containing: an editor asks about the cursor, which is an
/// empty range, and the answer a reader wants is the diagnostic the cursor is
/// sitting in.
fn overlaps(
    range: &(usize, usize, usize, usize),
    lines: &[&str],
    diagnostic: &uf_lint::Diagnostic,
) -> bool {
    let &(start_line, start_character, end_line, end_character) = range;
    let line = diagnostic.line.saturating_sub(1);
    let text = lines.get(line).copied();
    let span = text.map_or(1, |text| identifier_span(text, diagnostic.column));
    let start = (line, character(text, diagnostic.column));
    let end = (line, character(text, diagnostic.column + span));

    start <= (end_line, end_character) && end >= (start_line, start_character)
}

/// What uf can say about the position the cursor is on.
fn hover_answer(
    message: &Value,
    documents: &FxHashMap<String, Document>,
    modules: &[NativeModule],
) -> Result<Value, String> {
    let params = message
        .get("params")
        .ok_or_else(|| String::from("`textDocument/hover` needs `params`"))?;
    let uri = document_uri(message)
        .ok_or_else(|| String::from("`params.textDocument.uri` is required"))?;
    let position = params
        .get("position")
        .and_then(|position| {
            Some((
                usize::try_from(position.get("line")?.as_u64()?).ok()?,
                usize::try_from(position.get("character")?.as_u64()?).ok()?,
            ))
        })
        .ok_or_else(|| String::from("`params.position` needs a `line` and a `character`"))?;

    let Some(document) = documents.get(&uri) else {
        return Ok(Value::Null);
    };
    let (line, requested) = position;
    let lines: Vec<&str> = document.text.lines().collect();
    let text = lines.get(line).copied();
    let path = document_path(&uri);

    let Some(answer) = hover::hover(&hover::Request {
        path: &path,
        source: &document.text,
        line,
        column: byte_column(text, requested),
        diagnostics: document.diagnostics.as_deref().unwrap_or_default(),
        modules,
    }) else {
        return Ok(Value::Null);
    };

    Ok(json!({
        "contents": { "kind": "markdown", "value": answer.markdown },
        "range": {
            "start": { "line": line, "character": character(text, answer.start + 1) },
            "end": { "line": line, "character": character(text, answer.end + 1) },
        },
    }))
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

/// An LSP zero-based UTF-16 character offset, as a zero-based *byte* offset.
///
/// The inverse of [`character`], and the direction a request travels: the
/// protocol counts UTF-16 code units and everything uf holds — the linter's
/// columns, the fixer's spans, `str` itself — counts bytes. The two are tested
/// against each other rather than merely written next to each other; see
/// `a_utf16_position_and_a_byte_column_are_inverses`.
///
/// A character past the end of the line clamps to the end, which is what an
/// editor sends when the cursor sits in the virtual space past a short line,
/// and one landing inside a surrogate pair names the character it is inside.
fn byte_column(line: Option<&str>, character: usize) -> usize {
    let Some(line) = line else {
        return 0;
    };
    let mut units = 0usize;
    for (offset, letter) in line.char_indices() {
        if units >= character {
            return offset;
        }
        units += letter.len_utf16();
        if units > character {
            return offset;
        }
    }
    line.len()
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

/// Largest framed message the server will read, in bytes.
///
/// A `Content-Length` is a promise about an allocation, made by the peer
/// before the server has parsed anything at all, so it is the one number a
/// hostile or broken client fully controls. 32 MiB is four times the largest
/// source `uf_rsc` will scan, so no document uf would work on can reach it,
/// and it turns `Content-Length: 99999999999` into a refusal instead of an
/// out-of-memory abort.
const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

/// One frame off the stream.
#[derive(Debug)]
enum Frame {
    /// A body that parsed as JSON.
    Message(Value),
    /// A body that did not. The header already said how many bytes it was and
    /// they have been consumed, so the stream is still in sync and this is a
    /// message to answer rather than a reason to stop.
    Malformed,
}

/// One `Content-Length`-framed message, or [`None`] at end of input.
///
/// Headers are read line by line and everything but `Content-Length` is
/// skipped, which is what the specification asks for — `Content-Type` is the
/// other one clients send, and it carries nothing uf needs.
///
/// The distinction this function draws is between a broken *frame* and a
/// broken *message*. A missing or oversized `Content-Length` means the next
/// byte of the stream is unknown, which nothing downstream can recover from,
/// so it is an error that ends the server. A body that is not JSON is only a
/// bad message: its length was known and consumed, so it comes back as
/// [`Frame::Malformed`] and the loop answers it.
fn read_message(reader: &mut impl BufRead) -> Result<Option<Frame>> {
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
    if length > MAX_MESSAGE_BYTES {
        bail!("an LSP message claimed {length} bytes, over the {MAX_MESSAGE_BYTES} byte limit");
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .with_context(|| "failed to read an LSP message body")?;
    Ok(Some(
        serde_json::from_slice(&body).map_or(Frame::Malformed, Frame::Message),
    ))
}

/// Write one framed response, unless the message was a notification.
fn respond(out: &mut impl Write, id: Option<Value>, result: Value) -> Result<()> {
    let Some(id) = id else {
        return Ok(());
    };
    write_message(
        out,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
}

/// Write one framed error response.
///
/// The id is [`Value::Null`] when the request was too broken to have one,
/// which is what the specification says to send rather than omitting the
/// field: a response with no id is not a response.
fn respond_error(out: &mut impl Write, id: Value, code: i64, message: &str) -> Result<()> {
    write_message(
        out,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    )
}

fn write_message(out: &mut impl Write, message: &Value) -> Result<()> {
    let body = serde_json::to_string(message)?;
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
fn format_edits(source: &str, config: &FmtConfig) -> Value {
    let Ok(result) = uf_fmt::format_source(source, config) else {
        // A file that does not parse is left alone, the way `uf fmt` leaves
        // it alone. An error here would be an editor popup on every keystroke
        // in a file being typed.
        return Value::Null;
    };
    if !result.changed {
        return json!([]);
    }
    json!([whole_document_edit(source, &result.output)])
}

/// One `TextEdit` replacing the whole of `source` with `output`.
fn whole_document_edit(source: &str, output: &str) -> Value {
    json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": source.lines().count() + 1, "character": 0 },
        },
        "newText": output,
    })
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

/// What `uf dev` tells the driver, from what was typed.
///
/// `--port` carries `--strict-port` with it. Vite's default is to move to the
/// next free port, which is right for `dev.port` — a preference the project
/// wrote down once — and wrong for an argument somebody just typed: the
/// bookmark, the proxy rule and the container mapping all name the number that
/// was asked for, and a server quietly listening one along is a server nobody
/// can reach. Failing says which port is taken; moving says nothing until
/// something else breaks, somewhere else.
///
/// `dev.strictPort` still decides the case where the port came from the
/// config, and the driver reads it there.
fn driver_args(host: Option<&str>, port: Option<u16>) -> Vec<String> {
    let mut driver_args = Vec::new();
    if let Some(bind) = host {
        driver_args.push(String::from("--host"));
        driver_args.push(bind.to_owned());
    }
    if let Some(port) = port {
        driver_args.push(String::from("--port"));
        driver_args.push(port.to_string());
        driver_args.push(String::from("--strict-port"));
    }
    driver_args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_that_was_typed_is_the_port_the_server_binds() {
        // Without `--strict-port`, Vite takes the next free one and says so in
        // a banner nobody is reading, and the request that was going to prove
        // the server works goes to whatever else is on that port. The test
        // that found this had a dev server up on a port it was not asking
        // about; see ubugeeei-prod/uf#234.
        assert_eq!(
            driver_args(None, Some(5173)),
            ["--port", "5173", "--strict-port"]
        );
    }

    #[test]
    fn a_port_that_was_not_typed_leaves_the_choice_to_the_config() {
        // `dev.port` and `dev.strictPort` are the project's preference, and the
        // driver reads both. Sending `--strict-port` here would override a
        // `false` nobody asked to change.
        assert!(driver_args(None, None).is_empty());
        assert_eq!(driver_args(Some("0.0.0.0"), None), ["--host", "0.0.0.0"]);
    }

    /// The parsed body of a frame, for the tests that only care about that.
    fn body_of(frame: Option<Frame>) -> Value {
        match frame.expect("a frame") {
            Frame::Message(message) => message,
            Frame::Malformed => panic!("expected a parsed message"),
        }
    }

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
        assert_eq!(body_of(read_message(&mut reader).unwrap())["id"], json!(7));
        assert_eq!(body_of(read_message(&mut reader).unwrap())["id"], json!(7));
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

        assert_eq!(body_of(read_message(&mut reader).unwrap())["id"], json!(1));
    }

    /// A body that is not JSON is a bad *message*, not a bad frame: its length
    /// was known, so the stream is still in sync and the next frame is read.
    #[test]
    fn a_body_that_is_not_json_leaves_the_stream_in_sync() {
        let broken = "{not json";
        let good = r#"{"id":2}"#;
        let stream = format!(
            "Content-Length: {}\r\n\r\n{broken}Content-Length: {}\r\n\r\n{good}",
            broken.len(),
            good.len()
        );
        let mut reader = std::io::BufReader::new(stream.as_bytes());

        assert!(matches!(
            read_message(&mut reader).unwrap(),
            Some(Frame::Malformed)
        ));
        assert_eq!(body_of(read_message(&mut reader).unwrap())["id"], json!(2));
    }

    /// A length is an allocation the peer chose. One that could not be a real
    /// document is refused before it is allocated.
    #[test]
    fn an_absurd_content_length_is_refused_rather_than_allocated() {
        let stream = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        let mut reader = std::io::BufReader::new(stream.as_bytes());

        let error = read_message(&mut reader).expect_err("a refusal");
        assert!(error.to_string().contains("limit"), "{error}");
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

    /// The two converters have to be inverses, not merely neighbours: a
    /// position arrives in UTF-16 and every span uf owns is in bytes, so a
    /// disagreement is an edit landing in the wrong place.
    #[test]
    fn a_utf16_position_and_a_byte_column_are_inverses() {
        for line in ["const x = 1;", "const π = 1;", "const 🦀 = 1;", ""] {
            for (byte, _) in line.char_indices() {
                let unit = character(Some(line), byte + 1);
                assert_eq!(
                    byte_column(Some(line), unit),
                    byte,
                    "byte {byte} of {line:?} round-tripped through UTF-16 unit {unit}"
                );
            }
        }

        // Past the end of the line, which is where an editor puts a cursor
        // sitting in the virtual space after a short line.
        assert_eq!(byte_column(Some("ab"), 99), 2);
        assert_eq!(byte_column(None, 3), 0);
        // Inside a surrogate pair: the character it is inside, not the next one.
        assert_eq!(byte_column(Some("🦀x"), 1), 0);
        assert_eq!(byte_column(Some("🦀x"), 2), 4);
    }

    /// Code action kinds are hierarchical, which is what makes an editor's
    /// `codeActionsOnSave: { "source.fixAll": true }` find uf's own fix-all.
    #[test]
    fn a_requested_kind_selects_its_children() {
        assert!(wanted(None, FIX_ALL));
        assert!(wanted(Some(&[String::from("source.fixAll")]), FIX_ALL));
        assert!(wanted(Some(&[String::from("source.fixAll.uf")]), FIX_ALL));
        assert!(wanted(Some(&[String::from("source")]), FIX_ALL));
        assert!(wanted(Some(&[String::from("quickfix")]), QUICK_FIX));

        assert!(!wanted(Some(&[String::from("quickfix")]), FIX_ALL));
        assert!(!wanted(Some(&[String::from("refactor")]), QUICK_FIX));
        // A prefix that is not a whole kind segment is not a parent kind.
        assert!(!wanted(Some(&[String::from("source.fixAl")]), FIX_ALL));
        // Filtered down to nothing is a real answer, and it is "nothing".
        assert!(!wanted(Some(&[]), QUICK_FIX));
    }
}
