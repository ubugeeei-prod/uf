//! `uf mcp`: the toolchain over the Model Context Protocol, on stdio.
//!
//! An agent that wants `uf check`'s diagnostics today spawns `uf check --json`
//! and parses what comes back. This is the same answers over a protocol built
//! for asking: a tool per command, JSON Schema for the arguments, and a result
//! the caller does not have to know a shell to obtain. See
//! ubugeeei-prod/uf#507.
//!
//! # It runs the commands rather than reimplementing them
//!
//! Every tool below calls the same function `crates/uf_cli/src/lib.rs`
//! dispatches to — `check`, `lint`, `info`, `routes`, `explain`, `test`, `fmt`
//! — with a `Ui` that captures stdout instead of writing it. That is the whole
//! reason [`Ui::capturing`] exists: a tool that formatted its own JSON would be
//! a second surface, and the two would drift the first time a diagnostic gained
//! a field.
//!
//! [`SPECS`] is the one table both `tools/list` and `tools/call` read, so an
//! advertised tool and a dispatched one cannot come apart either.
//!
//! It is also a correctness requirement rather than a preference. **stdout is
//! the protocol here.** A command that wrote its own output to the real stdout
//! would interleave with the JSON-RPC frames and corrupt the stream, which is
//! why capturing had to come first.
//!
//! # Framing
//!
//! Newline-delimited JSON, which is MCP's stdio transport — one message per
//! line, no `Content-Length`. That is *not* what `uf lsp` beside it uses:
//! LSP frames with headers, and the two are different protocols that happen to
//! share JSON-RPC. Mixing them up produces a server that looks right and that
//! no client can talk to.
//!
//! # What is exposed, and what is marked
//!
//! Each command is a tool named for it. The two that write — `uf fmt --write`
//! and `uf lint --fix` — are separate tools whose names say so
//! (`uf_fmt_write`, `uf_lint_fix`), because an agent choosing a tool from a
//! list should not have to read a description to learn that one edits the
//! checkout.
//!
//! `uf_test` is the third case and is neither: it rewrites none of the
//! project's files, but it executes the project's own code. [`Effect`] is the
//! three-way distinction, and every description ends with the sentence for its
//! own. The readers' sentence was "Reads only." until #994, which `uf_check`
//! was not — it keeps its inference cache under `.uf/cache/check/` — so it now
//! says what is true of every reader, and `uf_check` names its cache.
//!
//! # Arguments are held to the schema that is published
//!
//! A call's `arguments` are checked against its tool's own `inputSchema`
//! before anything runs, and a call that does not fit is a tool error that
//! names the argument. Until #994 nothing read the schemas: a misspelled
//! `pathz` was dropped, and `uf_lint` answered for the whole project — a
//! different request from the one that was sent, reported as a success.
//! [`misfit`] is the check.
//!
//! # What a real client found
//!
//! Two bugs here were invisible to unit tests and immediate the first time the
//! official MCP SDK's client was pointed at the server, which is why
//! `tests/library/mcp.test.js` drives the real binary:
//!
//! - `uf_info` returned an empty string, because [`Ui::render`] writes nothing
//!   in JSON mode and `uf info` only renders. [`Speaks`] is the fix: the mode
//!   is a property of the command, not a constant.
//! - A failing `uf_check` returned its JSON report with the failure sentence
//!   appended, so the report no longer parsed. A command's output and the
//!   reason it failed are now two content blocks.

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Write};

use crate::commands;
use crate::fix::files::FixMode;
use crate::ui::{OutputMode, Ui};

/// The protocol version this server speaks.
///
/// `initialize` answers with this one whatever the client asked for, which is
/// what the specification calls for: a server that does not support the
/// requested version responds with one it does, and lets the client decide
/// whether it can proceed. Echoing an unknown version back instead would
/// claim support this server has not got.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC's own codes, the three this server can raise.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;

/// Serve the Model Context Protocol on stdio until the input ends.
///
/// # Errors
///
/// When stdin cannot be read or stdout cannot be written. A malformed message
/// is answered with a JSON-RPC error and is not one of these.
pub(crate) fn mcp(cwd: &Utf8Path) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    // The same courtesy `uf lsp` extends: a person who typed this at a prompt
    // gets a sentence rather than a server that appears to hang.
    if stdin.is_terminal() {
        writeln!(stdout, "uf mcp: Model Context Protocol stdio server ready")
            .with_context(|| "failed to write the mcp banner")?;
        return Ok(());
    }

    serve(&mut std::io::BufReader::new(stdin.lock()), &mut stdout, cwd)
}

/// The message loop, over any pair of streams.
///
/// Separated from [`mcp`] so that the protocol can be tested without a
/// subprocess: everything above this line is stdio, everything below it is MCP.
fn serve(input: &mut impl BufRead, output: &mut impl Write, cwd: &Utf8Path) -> Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        let read = input
            .read_line(&mut line)
            .with_context(|| "failed to read from stdin")?;
        if read == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }

        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            respond_error(output, Value::Null, PARSE_ERROR, "the line was not JSON")?;
            continue;
        };
        // A line that parsed but is not an object: an array (JSON-RPC batching,
        // which MCP's stdio transport does not use), or a bare scalar. There is
        // no `id` to answer with, so it is answered with a null one — silence
        // would leave a client waiting for a reply that is never coming.
        if !message.is_object() {
            respond_error(
                output,
                Value::Null,
                INVALID_REQUEST,
                "a message must be a JSON object; this server does not accept batches",
            )?;
            continue;
        }
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A notification with no method is nothing; a request with none is
            // a request nobody can answer.
            if let Some(id) = id {
                respond_error(
                    output,
                    id,
                    INVALID_REQUEST,
                    "the message carried no `method`",
                )?;
            }
            continue;
        };

        match method {
            "initialize" => {
                respond(
                    output,
                    id,
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "serverInfo": {
                            "name": "uf",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                    }),
                )?;
            }
            // Notifications are not answered, by the specification and by
            // JSON-RPC: a reply to one has no `id` to carry.
            "notifications/initialized" | "notifications/cancelled" => {}
            "ping" => respond(output, id, json!({}))?,
            "tools/list" => respond(output, id, json!({ "tools": tools() }))?,
            "tools/call" => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                let outcome = call(cwd, name, &arguments);
                respond(output, id, outcome)?;
            }
            other => {
                if let Some(id) = id {
                    respond_error(
                        output,
                        id,
                        METHOD_NOT_FOUND,
                        &format!("this server has no method {other:?}"),
                    )?;
                }
            }
        }
    }
}

/// What a tool does to the project, which is the first thing a caller
/// choosing one from a list needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// Reads the project and reports. None of the project's files change and
    /// none of its code runs; uf may write its own caches under `.uf/cache/`,
    /// as `uf check` does.
    Reads,
    /// Rewrites files in the checkout.
    Writes,
    /// Executes the project's own code.
    Runs,
}

impl Effect {
    /// The sentence every description ends with, so that the warning is in
    /// the prose as well as in the name.
    fn note(self) -> &'static str {
        match self {
            Self::Reads => {
                "Changes none of your files, though uf may write its own cache under `.uf/cache/`."
            }
            Self::Writes => "WRITES to files in the checkout.",
            Self::Runs => "RUNS the project's own code.",
        }
    }
}

/// Which output a command produces, and so which [`OutputMode`] it must be
/// given.
///
/// **Not decoration.** [`Ui::render`] writes nothing at all in JSON mode, so a
/// command that only renders — `uf info`, `uf routes list` — returns an empty
/// string when asked for JSON. Getting this wrong is silent: the tool call
/// succeeds and hands back nothing, which is what the first client to try it
/// found.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Speaks {
    /// The command has a `--json` shape, and that is what the tool returns.
    Json,
    /// The command renders a report for a reader; the tool returns that text,
    /// unstyled.
    Prose,
}

impl Speaks {
    fn mode(self) -> OutputMode {
        match self {
            Self::Json => OutputMode::Json,
            Self::Prose => OutputMode::Human,
        }
    }
}

/// One tool: everything both [`tools`] and [`call`] need to agree about.
///
/// A single table rather than a list and a `match` that happen to line up,
/// because the drift between an advertised tool and a dispatched one is
/// exactly what #507 asks this server not to have.
struct Spec {
    name: &'static str,
    effect: Effect,
    speaks: Speaks,
    description: &'static str,
    schema: fn() -> Value,
}

/// A schema with no arguments at all.
fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// A schema whose only argument is the usual optional list of paths.
fn paths_only() -> Value {
    json!({
        "type": "object",
        "properties": {
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Files or directories to act on. Defaults to the whole project.",
            },
        },
        "additionalProperties": false,
    })
}

fn explain_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The command to explain, such as `build` or `test`.",
            },
        },
        "required": ["command"],
        "additionalProperties": false,
    })
}

fn test_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "filter": {
                "type": "string",
                "description": "Keep only tests whose fully qualified name contains this.",
            },
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Only run files whose path contains one of these. Defaults to \
                                the whole suite.",
            },
        },
        "additionalProperties": false,
    })
}

/// Every tool this server exposes.
///
/// Named for the command each one runs, so that a caller who knows uf knows
/// this list, and the two that write say so in the name.
const SPECS: &[Spec] = &[
    Spec {
        name: "uf_check",
        effect: Effect::Reads,
        speaks: Speaks::Json,
        description: "Type-check the project with Flow and report diagnostics as JSON. \
                      The report holds two lists: `diagnostics` is lint findings and \
                      `typeCheck.diagnostics` is type errors, and `errors` is the sum \
                      of both \u{2014} so a project can fail with `diagnostics` empty. \
                      What it inferred is kept in `.uf/cache/check/`, and an unchanged \
                      file is answered from there.",
        schema: paths_only,
    },
    Spec {
        name: "uf_lint",
        effect: Effect::Reads,
        speaks: Speaks::Json,
        description: "Lint the project and report diagnostics as JSON.",
        schema: paths_only,
    },
    Spec {
        name: "uf_info",
        effect: Effect::Reads,
        speaks: Speaks::Prose,
        description: "uf's own version, the machine's architecture and operating system, \
                      the working directory this server was given, where uf's documentation \
                      and installer are, and uf's design tokens. It reads no `uf.config.js`: \
                      `uf_explain` is the tool that says what uf will do in this project.",
        schema: no_arguments,
    },
    Spec {
        name: "uf_routes",
        effect: Effect::Reads,
        speaks: Speaks::Prose,
        description: "The route table, as `uf build` counts it: which file serves which URL path.",
        schema: no_arguments,
    },
    Spec {
        name: "uf_explain",
        effect: Effect::Reads,
        speaks: Speaks::Json,
        description: "Explain how uf will run one command \u{2014} the provider, the stages, and \
                      what decided each.",
        schema: explain_schema,
    },
    Spec {
        name: "uf_test",
        effect: Effect::Runs,
        speaks: Speaks::Json,
        description: "Run the project's tests and report the results as JSON.",
        schema: test_schema,
    },
    Spec {
        name: "uf_fmt_write",
        effect: Effect::Writes,
        speaks: Speaks::Prose,
        description: "Format the project and write the result to disk.",
        schema: paths_only,
    },
    Spec {
        name: "uf_lint_fix",
        effect: Effect::Writes,
        speaks: Speaks::Json,
        description: "Lint the project and write every safe fix to disk.",
        schema: paths_only,
    },
];

/// The `tools/list` payload.
pub(crate) fn tools() -> Vec<Value> {
    SPECS
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": format!("{} {}", spec.description, spec.effect.note()),
                "inputSchema": (spec.schema)(),
            })
        })
        .collect()
}

/// Run one tool and shape the result the way `tools/call` answers.
///
/// A command that fails is a tool result with `isError`, not a JSON-RPC error:
/// the call succeeded and the answer is that the project does not check. A
/// protocol error would tell the caller its *request* was wrong, which is a
/// different thing and the distinction the specification draws.
///
/// A tool that does not exist, and arguments that do not fit the tool's
/// schema, are answered the same way, and deliberately: the agent that sent
/// the call is the one that has to read why and send a better one, and a tool
/// error is what reaches it, where a protocol error is what a client library
/// raises before the model sees anything.
fn call(cwd: &Utf8Path, name: &str, arguments: &Value) -> Value {
    let Some(spec) = SPECS.iter().find(|spec| spec.name == name) else {
        return tool_error(format!("no tool named {name:?}"));
    };
    // Before anything runs. Guessing what a call that does not fit meant is
    // how a misspelled `pathz` became a lint of the whole project.
    if let Some(why) = misfit(&(spec.schema)(), arguments) {
        return tool_error(format!("{name} did not run: {why}"));
    }

    let paths: Vec<String> = arguments
        .get("paths")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let json = spec.speaks == Speaks::Json;

    let mut ui = Ui::capturing(spec.speaks.mode());
    let outcome = match spec.name {
        "uf_check" => commands::check::check(cwd, &mut ui, json, FixMode::Report, &paths),
        "uf_lint" => commands::lint::lint_command(
            cwd,
            &mut ui,
            commands::lint::LintCommand::Lint,
            json,
            FixMode::Report,
            &paths,
        ),
        "uf_info" => commands::info::info(cwd, &mut ui),
        "uf_routes" => commands::routes::routes(cwd, &mut ui, crate::cli::RoutesCommand::List),
        "uf_explain" => {
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            commands::explain::explain(cwd, &mut ui, command, None, json)
        }
        "uf_test" => commands::test::test(
            cwd,
            &mut ui,
            commands::test::TestArgs {
                json,
                filter: arguments
                    .get("filter")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                paths,
                ..Default::default()
            },
        ),
        "uf_fmt_write" => commands::fmt::fmt(cwd, &mut ui, false, &paths),
        "uf_lint_fix" => commands::lint::lint_command(
            cwd,
            &mut ui,
            commands::lint::LintCommand::Lint,
            json,
            FixMode::Safe,
            &paths,
        ),
        // Unreachable while the contract test passes: `SPECS` is what was
        // searched above, so a name that resolved to a spec and has no arm
        // here is a tool added to the table and nowhere else.
        other => return tool_error(format!("no tool named {other:?}")),
    };

    // What the command wrote is one block, whole. A command that failed adds
    // the reason as a *second* block rather than appending it: the output is
    // usually JSON and usually the interesting half, and a sentence glued to
    // the end of it is a document that no longer parses.
    let written = ui.take_captured();
    let mut content = Vec::new();
    if !written.is_empty() {
        content.push(json!({ "type": "text", "text": written }));
    }
    let failed = match outcome {
        Ok(()) => false,
        Err(error) => {
            content.push(json!({ "type": "text", "text": format!("{error:#}") }));
            true
        }
    };
    // A command that succeeded in silence still owes the caller a block, since
    // an empty `content` is a result with nothing in it to read.
    if content.is_empty() {
        content.push(json!({ "type": "text", "text": "" }));
    }
    json!({ "content": content, "isError": failed })
}

/// A tool result that is an error, and says why.
fn tool_error(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// The keywords [`misfit`] enforces, and `description`, which it reads past.
///
/// A test fails on a published schema that uses any other. A keyword the check
/// did not know would be a rule `tools/list` states and nothing enforces,
/// which is the bug the check exists for.
#[cfg(test)]
const SCHEMA_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "description",
];

/// Why `arguments` do not fit `schema`, or [`None`] when they do.
///
/// `schema` is the value [`tools`] publishes, built by the same function, so
/// the schema a client reads and the one its call is held to cannot be two
/// descriptions of one thing. The walk follows the schema rather than the
/// value, so how deep it goes is decided by this file and not by the sender.
fn misfit(schema: &Value, arguments: &Value) -> Option<String> {
    fit(schema, arguments, None).err()
}

/// One level of [`misfit`]: `value` against `schema`, where `at` names the
/// argument, or is [`None`] for the arguments object itself.
fn fit(schema: &Value, value: &Value, at: Option<&str>) -> Result<(), String> {
    let named = |key: &str| match at {
        Some(at) => format!("{at}.{key}"),
        None => key.to_owned(),
    };
    if let Some(expected) = schema.get("type").and_then(Value::as_str)
        && !is_type(value, expected)
    {
        return Err(format!(
            "`{}` must be {}, not {}",
            at.unwrap_or("arguments"),
            a_type(expected),
            a_value(value),
        ));
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        let known = |key: &str| properties.is_some_and(|properties| properties.contains_key(key));
        if schema.get("additionalProperties") == Some(&Value::Bool(false))
            && let Some(unknown) = object.keys().find(|key| !known(key))
        {
            let names: Vec<String> = properties
                .into_iter()
                .flatten()
                .map(|(key, _)| format!("`{}`", named(key)))
                .collect();
            return Err(if names.is_empty() {
                format!("`{}` is not an argument, and it takes none", named(unknown))
            } else {
                format!(
                    "`{}` is not an argument; the arguments are {}",
                    named(unknown),
                    names.join(", ")
                )
            });
        }
        let required = schema.get("required").and_then(Value::as_array);
        for key in required.into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("`{}` is required", named(key)));
            }
        }
        for (key, property) in properties.into_iter().flatten() {
            if let Some(inner) = object.get(key) {
                fit(property, inner, Some(&named(key)))?;
            }
        }
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        let at = at.unwrap_or("arguments");
        for (index, item) in array.iter().enumerate() {
            fit(items, item, Some(&format!("{at}[{index}]")))?;
        }
    }
    Ok(())
}

/// Whether `value` is what a schema's `type` names.
///
/// A type this does not know fits nothing, so a schema naming one refuses every
/// call rather than letting every call through.
fn is_type(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "null" => value.is_null(),
        _ => false,
    }
}

/// A schema type, as it reads in a sentence.
fn a_type(expected: &str) -> &'static str {
    match expected {
        "object" => "an object",
        "array" => "an array",
        "string" => "a string",
        "boolean" => "a boolean",
        "integer" => "an integer",
        "number" => "a number",
        "null" => "null",
        _ => "a type this server does not know",
    }
}

/// What a value is, as it reads in a sentence.
fn a_value(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Write one JSON-RPC result, or nothing when the message was a notification.
fn respond(stdout: &mut impl Write, id: Option<Value>, result: Value) -> Result<()> {
    let Some(id) = id else { return Ok(()) };
    write_message(
        stdout,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
}

/// Write one JSON-RPC error.
fn respond_error(stdout: &mut impl Write, id: Value, code: i64, message: &str) -> Result<()> {
    write_message(
        stdout,
        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    )
}

/// One message, one line, flushed — a client is waiting on it.
fn write_message(stdout: &mut impl Write, message: &Value) -> Result<()> {
    let line = serde_json::to_string(message).with_context(|| "failed to serialise a reply")?;
    writeln!(stdout, "{line}").with_context(|| "failed to write to stdout")?;
    stdout.flush().with_context(|| "failed to flush stdout")?;
    Ok(())
}

#[cfg(test)]
mod tests;
