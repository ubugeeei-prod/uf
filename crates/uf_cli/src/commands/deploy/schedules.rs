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
            if let Some(found) = declared_here(declaration, file)? {
                return Ok(Some(found));
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
            return Ok(Some(five_fields(cron, file)?));
        }
    }
    Ok(None)
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
/// `edge` is the only one today: `uf build` writes its `wrangler.json`, so
/// there is a file to put `triggers.crons` in and Cloudflare's own scheduler
/// to read it.
///
/// `node`, `bun` and `container` *could* — they keep a process, and
/// `@uniflowed/server/schedule` ticks in one — but the entry `uf build`
/// generates for them does not pass the declaration to `serve` yet. Until it
/// does, a declaration in a route module would do nothing on those targets,
/// and saying so is better than the silence.
pub(crate) const fn runs_schedules(adapter: DeployAdapter) -> bool {
    matches!(adapter, DeployAdapter::Edge)
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
         `--adapter edge` writes them into `wrangler.json` for Cloudflare's own scheduler. \
         On a target that keeps a process, pass them to `serve` yourself with \
         `@uniflowed/server/schedule` — declaring one in a route module is not wired for \
         those yet (ubugeeei-prod/uf#531).",
        adapter.as_str(),
        schedules.len()
    );
}

#[cfg(test)]
mod tests;
