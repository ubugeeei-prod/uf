//! What a route handler says about when it should run without being asked.
//!
//! The build's half of ubugeeei-prod/uf#531. `@uniflowed/server/schedule` is
//! the runtime's half and owns what an expression *means*; this owns finding
//! one before anything runs, because a platform's own scheduler has to be told
//! about it in a file written beside the deployment.
//!
//! # The declaration
//!
//! A route handler exports the expression as a string beside its handler:
//!
//! ```js
//! export const schedule = "*/15 * * * *";
//! export async function GET() { … }
//! ```
//!
//! A *string*, and not a `defineSchedule(...)` call, for one reason: this runs
//! offline. `uf build` writes Cloudflare's `triggers.crons` without evaluating
//! a line of the project, and an expression it would have to run the module to
//! learn is one it cannot write down. A computed value is refused by name
//! rather than skipped, because a schedule the build could not read is a
//! schedule the deployment would not run — which is the failure #531 exists to
//! refuse.
//!
//! # What is checked here, and what is not
//!
//! Five fields, and nothing about what they contain. The meaning of a field
//! lives in `packages/server/internal/cron.js` and belongs in one place: a
//! second matcher here would be two implementations of one rule, drifting the
//! first time either learned something. So `99 * * * *` passes this and is
//! refused by `defineSchedule` when the deployment starts, loudly, rather than
//! being quietly accepted by both.

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde_json::Value;
use uf_config::{DeployAdapter, UniflowedConfig};
use uf_infra::FxHashMap;
use uf_router::{ServerModuleKind, discover_server_modules};

/// The export a route handler declares its schedule with.
const EXPORT: &str = "schedule";

/// The method a scheduled invocation uses, and so the export it needs.
const HANDLER: &str = "GET";

/// One schedule, as the build reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredSchedule {
    /// The route path of the handler that declared it, `/api/sweep`-style.
    pub(crate) path: CompactString,
    /// The file it was read from, for a message that names something.
    pub(crate) file: Utf8PathBuf,
    /// The expression, exactly as it was written.
    pub(crate) cron: String,
}

/// Every schedule declared under the router root, in path order.
///
/// # Errors
///
/// When a module cannot be read or parsed, or when it exports `schedule` as
/// something this cannot read offline.
pub(crate) fn discover_schedules(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<DeclaredSchedule>> {
    let modules = discover_server_modules(root, config)?;
    let mut found = Vec::new();
    for module in modules {
        // Route handlers only. A middleware runs per request by definition, so
        // a schedule on one would be a schedule on something that already has
        // a trigger.
        if module.kind != ServerModuleKind::RouteHandler {
            continue;
        }
        let source = std::fs::read_to_string(module.file.as_std_path())
            .with_context(|| format!("reading {}", module.file))?;
        // The cheap gate first: a module that does not contain the word cannot
        // declare one, and most modules do not. Textual on purpose, and only
        // ever a *skip* — the parse below is what decides.
        if !source.contains(EXPORT) {
            continue;
        }
        let Some(cron) = read_schedule(&source, &module.file)? else {
            continue;
        };
        found.push(DeclaredSchedule {
            path: module.path,
            file: module.file,
            cron,
        });
    }
    found.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(found)
}

/// The expression a module exports, or [`None`] when it exports none.
///
/// Both spellings reach the same answer, because a reader who wrote the second
/// one meant the first: `export const schedule = "…"` and
/// `const schedule = "…"; export { schedule }` are one declaration to
/// everything else that reads this module, and a build that saw only the first
/// would miss a schedule silently — which is the failure this whole file
/// exists to refuse.
fn read_schedule(source: &str, file: &Utf8Path) -> Result<Option<String>> {
    // Through the same parser the build transforms with, rather than a second
    // reading of the file: `uf lint` and `uf build` already disagree with
    // nobody about what this module is, and a lexer here would be the third
    // opinion.
    let (program, _) = uf_transform::lowered_ast(source)
        .with_context(|| format!("parsing {file} to read its `{EXPORT}` export"))?;
    let empty = Vec::new();
    let body = program
        .get("body")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let literals = top_level_literals(body);
    let mut found: Option<String> = None;

    for statement in body {
        if statement.get("type").and_then(Value::as_str) != Some("ExportNamedDeclaration") {
            continue;
        }

        // `null` and not absent: a specifier export carries the key with
        // nothing in it, and treating that as a declaration is how the
        // specifiers below never got looked at.
        let declaration = statement
            .get("declaration")
            .filter(|declaration| !declaration.is_null());
        if let Some(declaration) = declaration {
            if let Some(cron) = declared_here(declaration, file)? {
                found = Some(cron);
            }
            continue;
        }

        for specifier in specifiers(statement) {
            if name_of(specifier.get("exported")) != Some(EXPORT) {
                continue;
            }
            // `export { schedule } from "./elsewhere.js"`: the value is in
            // another module, and following imports to find it is a different
            // and much larger question than reading this file. Refused rather
            // than skipped, for the same reason a computed one is.
            if statement
                .get("source")
                .is_some_and(|source| !source.is_null())
            {
                bail!(
                    "{file} re-exports `{EXPORT}` from another module, and this build reads \
                     only the file that declares it.\n  Write the expression in this file — \
                     `export const {EXPORT} = \"*/15 * * * *\"`."
                );
            }
            let Some(local) = name_of(specifier.get("local")) else {
                continue;
            };
            let Some(cron) = literals.get(local) else {
                bail!("{}", unreadable(file));
            };
            found = Some(five_fields(cron, file)?);
        }
    }

    // The method question, asked of the same tree rather than a second parse,
    // and only of a module that declared something: every other route handler
    // in the project is free to export whatever it answers.
    if found.is_some() && !exports_get(body) {
        bail!(
            "{file} declares a `{EXPORT}` and exports no `{HANDLER}`.\n  \
             A scheduled invocation is a `{HANDLER}` to the route's own path — a cron has \
             no body to send — so this trigger would fire into a 405 nobody reads."
        );
    }
    Ok(found)
}

/// Whether the module exports a `GET`, in any of the three spellings.
///
/// A scheduled invocation is a `GET` to the route's own path — there is no
/// body a cron could send — so a module that declares a schedule and exports
/// no `GET` is a trigger that would fire into a 405 nobody reads. Checked
/// here, where the schedule is read, so it is a build that did not happen
/// rather than a deployment that answers nothing.
fn exports_get(body: &[Value]) -> bool {
    for statement in body {
        if statement.get("type").and_then(Value::as_str) != Some("ExportNamedDeclaration") {
            continue;
        }
        let declaration = statement
            .get("declaration")
            .filter(|declaration| !declaration.is_null());
        if let Some(declaration) = declaration {
            // `export function GET() {}` and `export const GET = …`.
            if name_of(declaration.get("id")) == Some(HANDLER) {
                return true;
            }
            for declarator in declaration
                .get("declarations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if name_of(declarator.get("id")) == Some(HANDLER) {
                    return true;
                }
            }
            continue;
        }
        // `export { GET }`, and `export { handler as GET }`.
        for specifier in specifiers(statement) {
            if name_of(specifier.get("exported")) == Some(HANDLER) {
                return true;
            }
        }
    }
    false
}

/// The expression on an exported `const schedule = "…"`, if that is what this
/// declaration is.
fn declared_here(declaration: &Value, file: &Utf8Path) -> Result<Option<String>> {
    if declaration.get("type").and_then(Value::as_str) != Some("VariableDeclaration") {
        return Ok(None);
    }
    for declarator in declaration
        .get("declarations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if name_of(declarator.get("id")) != Some(EXPORT) {
            continue;
        }
        let Some(cron) = literal_string(declarator.get("init")) else {
            bail!("{}", unreadable(file));
        };
        return Ok(Some(five_fields(cron, file)?));
    }
    Ok(None)
}

/// Every top-level `const`/`let`/`var` bound to a string literal.
///
/// Collected so `export { schedule }` can be answered without a second walk,
/// and only the literal ones: a binding this cannot read is the same refusal
/// whether it was exported directly or by name.
fn top_level_literals(body: &[Value]) -> FxHashMap<&str, &str> {
    let mut literals = FxHashMap::default();
    for statement in body {
        if statement.get("type").and_then(Value::as_str) != Some("VariableDeclaration") {
            continue;
        }
        for declarator in statement
            .get("declarations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(name), Some(value)) = (
                name_of(declarator.get("id")),
                literal_string(declarator.get("init")),
            ) {
                literals.insert(name, value);
            }
        }
    }
    literals
}

fn specifiers(statement: &Value) -> impl Iterator<Item = &Value> {
    statement
        .get("specifiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn name_of(node: Option<&Value>) -> Option<&str> {
    node?.get("name")?.as_str()
}

/// The string a node is, when it is a string literal and nothing else.
fn literal_string(node: Option<&Value>) -> Option<&str> {
    let node = node?;
    if node.get("type")?.as_str()? != "Literal" {
        return None;
    }
    node.get("value")?.as_str()
}

/// The expression, or a refusal naming the field count.
fn five_fields(cron: &str, file: &Utf8Path) -> Result<String> {
    if cron.split_whitespace().count() != 5 {
        bail!(
            "{file} exports `{EXPORT} = {cron:?}`, which is not five fields.\n  \
             A cron expression is `minute hour day-of-month month day-of-week`."
        );
    }
    Ok(cron.to_owned())
}

/// What to say about a `schedule` this build cannot read.
fn unreadable(file: &Utf8Path) -> String {
    format!(
        "{file} exports `{EXPORT}` as something this build cannot read.\n  \
         It has to be a string written in the file — `export const {EXPORT} = \
         \"*/15 * * * *\"` — because uf writes a platform's cron configuration without \
         running the project, and an expression it would have to evaluate is one it cannot \
         write down."
    )
}

/// Whether `adapter` would actually run what a project declared.
///
/// Every target that has somewhere to run one:
///
/// * `edge` — `uf build` writes `triggers.crons` into its `wrangler.json` and
///   a `scheduled()` into its `worker.js`, so Cloudflare's own scheduler has
///   both something to fire and something to call. One without the other was
///   the bug in #712.
/// * `node`, `bun`, `container` — these keep a process, and the `server.js` uf
///   writes now hands the declaration to `serve`, which ticks it through
///   `@uniflowed/server/schedule`.
///
/// `serverless` is the one left, and it is left for a reason rather than for
/// want of attention: uf writes no configuration file for it at all — the
/// artefact is a zip — so there is nowhere to say "call this every fifteen
/// minutes". `static` runs nothing by definition.
///
/// See ubugeeei-prod/uf#531.
pub(crate) const fn runs_schedules(adapter: DeployAdapter) -> bool {
    matches!(
        adapter,
        DeployAdapter::Edge | DeployAdapter::Node | DeployAdapter::Bun | DeployAdapter::Container
    )
}

/// Refuse a build whose schedules this target would not run.
///
/// The clause ubugeeei-prod/uf#531 asks for by name: *"a target that can do
/// neither must refuse the configuration at build time rather than build
/// something that never runs, which is the failure this feature has everywhere
/// it exists."*
///
/// # Errors
///
/// When `schedules` is not empty and `adapter` would not run them.
pub(crate) fn refuse_unrunnable(
    adapter: DeployAdapter,
    schedules: &[DeclaredSchedule],
) -> Result<()> {
    if schedules.is_empty() || runs_schedules(adapter) {
        return Ok(());
    }
    let mut named = String::new();
    for schedule in schedules {
        named.push_str(&format!(
            "\n    {} — `{}`, in {}",
            schedule.path, schedule.cron, schedule.file
        ));
    }
    bail!(
        "the `{}` adapter would not run the {} schedule(s) this project declares, so this \
         build would produce a deployment whose scheduled work never happens:{named}\n  \
         `node`, `bun`, `container` and `edge` all run one: the first three tick it in \
         the process they keep, and `edge` hands it to Cloudflare's own scheduler. \
         `serverless` has no configuration file uf writes, so there is nowhere to say \
         when to call it (ubugeeei-prod/uf#531).",
        adapter.as_str(),
        schedules.len()
    );
}

#[cfg(test)]
mod tests;
