//! `uf routes`: the table the build counts from, and the files a route is.
//!
//! Two subcommands and one list behind both of them. `uf routes list` walks
//! the router root with `discover_routes` — the same function `uf build`,
//! `uf dev` and the generated `RoutePath` are built out of — and
//! `uf routes add` writes a route with `scaffold_route`, whose file names come
//! from the same `crates/uf_router/src/reserved.rs` that discovery reads.
//!
//! That is the point of the command rather than a detail of it. A route is a
//! directory and one to four reserved files, and until this a reader wrote
//! them by hand and found out from the next `uf build` whether the names were
//! right — which is a grammar with no front door. See ubugeeei-prod/uf#500.

use anyhow::{Result, anyhow, bail};
use camino::Utf8Path;
use std::fs;
use uf_config::load_config;
use uf_router::scaffold::{RouteParts, ScaffoldError, ScaffoldFile, scaffold_route};
use uf_router::{Route, RouteParamKind, discover_routes};
use uf_term::{Cell, Column, Status, Table, Tone};

use crate::cli::RoutesCommand;
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

pub(crate) fn routes(cwd: &Utf8Path, ui: &mut Ui, command: RoutesCommand) -> Result<()> {
    match command {
        RoutesCommand::List => list(cwd, ui),
        RoutesCommand::Add {
            path,
            layout,
            loader,
            middleware,
        } => add(
            cwd,
            ui,
            &path,
            RouteParts {
                layout,
                loader,
                middleware,
            },
        ),
    }
}

/// `uf routes list`: every route, as `uf build` counts them.
fn list(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let router_root = resolved.root.join(resolved.config.app.router.root.as_str());
    // The error is the one `uf build` gives — an unsupported directory, a
    // catch-all that nothing can reach — and it is reported rather than
    // swallowed into an empty table. A command that answers "no routes" for a
    // project whose routes are refused would be the second opinion this
    // command exists not to be.
    let found = discover_routes(&resolved.root, &resolved.config)?;
    let label = project_label(&resolved.root).to_owned();

    ui.render(|renderer, out| {
        renderer.heading(out, 1, &format!("routes · {label}"));
        renderer.blank(out);
        if found.is_empty() {
            renderer.status(
                out,
                Status::Info,
                &format!(
                    "no routes under {}; `uf routes add /` writes the first one",
                    relative_to(&resolved.root, &router_root)
                ),
            );
            renderer.blank(out);
            return;
        }

        let rows: Vec<(String, String, String, String)> = found
            .iter()
            .map(|route| {
                (
                    route.path.to_string(),
                    parameters(route),
                    guards(route),
                    relative_to(&resolved.root, &route.page),
                )
            })
            .collect();
        let mut table = Table::new(vec![
            Column::left("path"),
            Column::left("params"),
            Column::left("middleware"),
            Column::left("page"),
        ]);
        for (path, params, middleware, page) in &rows {
            table.push(vec![
                Cell::new(path),
                Cell::toned(params, Tone::Muted),
                Cell::toned(middleware, Tone::Muted),
                Cell::toned(page, Tone::Path),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(out, Status::Success, &plural(found.len(), "route"));
        renderer.blank(out);
    });
    Ok(())
}

/// The parameters a route captures, in the spelling its path already uses.
fn parameters(route: &Route) -> String {
    if route.params.is_empty() {
        return "—".to_owned();
    }
    route
        .params
        .iter()
        .map(|param| match param.kind {
            RouteParamKind::Single => format!("[{}]", param.name),
            RouteParamKind::CatchAll => format!("[...{}]", param.name),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How many middlewares run before a route resolves.
///
/// The count, not the files: the chain is inherited, so a route two segments
/// below the one that declared a guard is guarded too, and a column of paths
/// would be the same path repeated down the table. `uf inspect --json` is
/// where the files are.
fn guards(route: &Route) -> String {
    if route.middleware.is_empty() {
        return "—".to_owned();
    }
    route.middleware.len().to_string()
}

/// `uf routes add <path>`: the files, written.
fn add(cwd: &Utf8Path, ui: &mut Ui, path: &str, parts: RouteParts) -> Result<()> {
    let resolved = load_config(cwd)?;
    if !resolved.config.app.router.enabled {
        bail!(
            "the file-system router is off in this project (`app.router.enabled` is false), so \
             there is no route table for a route to join"
        );
    }
    let router_root = resolved.root.join(resolved.config.app.router.root.as_str());
    // The one error that carries a path is rewritten against the project root
    // before it is printed. `scaffold_route` is given an absolute router root
    // and has no idea where the project starts, and an absolute path in an
    // error is a path a reader has to read the middle of to find the part that
    // is about them.
    let files = scaffold_route(&router_root, path, parts).map_err(|error| match error {
        ScaffoldError::Exists { path } => anyhow!(
            "{} already exists; `uf routes add` never overwrites a file",
            relative_to(&resolved.root, &path)
        ),
        other => anyhow!(other),
    })?;

    // Every file is checked before any is written — `scaffold_route` refuses a
    // set with an existing file in it — so a run that stops has written
    // nothing, rather than half a route somebody now has to finish or undo.
    for file in &files {
        if let Some(parent) = file.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file.path, &file.contents)?;
    }

    let written: Vec<(String, String)> = files
        .iter()
        .map(|file: &ScaffoldFile| {
            (
                relative_to(&resolved.root, &file.path),
                file.role.as_str().to_owned(),
            )
        })
        .collect();
    let count = files.len();
    ui.render(|renderer, out| {
        renderer.blank(out);
        let mut table = Table::new(vec![Column::left("file"), Column::left("role")]);
        for (path, role) in &written {
            table.push(vec![
                Cell::toned(path, Tone::Path),
                Cell::toned(role, Tone::Muted),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            &format!("wrote {}", plural(count, "file")),
        );
        // Not run automatically: `uf prepare` type checks, and a scaffold that
        // silently ran the checker would make writing a file feel like a build.
        renderer.status(
            out,
            Status::Info,
            "`uf prepare` regenerates `router.js` so the new path is in `RoutePath`",
        );
        renderer.blank(out);
    });
    Ok(())
}
