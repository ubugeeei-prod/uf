//! `uf ui`: styled components the project owns, added, listed and compared.
//!
//! The decisions are `uf_ui`'s, and its header argues each of them: where the
//! registry lives, what a component is, what a copy remembers about where it
//! came from, and when a file that is already there may be replaced. This
//! module is the command around those decisions — which project, which
//! directory, the package manager for what a component imports — and how each
//! answer reads on a terminal and as `--json`.
//!
//! # The order `uf ui add` does things in
//!
//! Decide everything, install, then write. [`uf_ui::project::plan_add`] reads
//! every file the run would touch and refuses before anything happens, so a
//! refusal has changed nothing. The packages come next, through the same
//! `uf add` a person would run, because a component written before its imports
//! resolve is a project that no longer builds — and an install that fails
//! leaves the files unwritten, which is the state the project was in.

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::{UiConfig, load_config};
use uf_pm::DependencyKind;
use uf_term::{Cell, Column, Renderer, Status, Table, Tone};
use uf_ui::project::{self, AddAction, AddError, AddPlan, Conflict, CopyState, ProjectCopy};
use uf_ui::{Component, DEFAULT_DIRECTORY, REGISTRY_VERSION, Registry};

use crate::cli::UiCommand;
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

pub(crate) fn ui(cwd: &Utf8Path, ui: &mut Ui, command: UiCommand) -> Result<()> {
    match command {
        UiCommand::Add { overwrite, names } => add(cwd, ui, &names, overwrite),
        UiCommand::List { json } => list(cwd, ui, json),
        UiCommand::Diff { json, names } => diff(cwd, ui, &names, json),
    }
}

/// The project, and the directory its components are written to.
struct Place {
    root: Utf8PathBuf,
    directory: Utf8PathBuf,
}

impl Place {
    fn find(cwd: &Utf8Path) -> Result<Self> {
        let resolved = load_config(cwd)?;
        let named = configured_directory(&resolved.config.ui)?;
        let directory = resolved.root.join(named);
        Ok(Self {
            root: resolved.root,
            directory,
        })
    }

    fn relative(&self, path: &Utf8Path) -> String {
        relative_to(&self.root, path)
    }
}

/// The directory components live in, relative to the project root:
/// `ui.directory`, or [`DEFAULT_DIRECTORY`] when the project names none.
///
/// A path that leaves the project is refused. `uf.config.js` is a file a cloned
/// repository brings with it, and it does not get to point `uf ui add` at files
/// outside the checkout.
fn configured_directory(config: &UiConfig) -> Result<&Utf8Path> {
    let named = if config.directory.is_empty() {
        DEFAULT_DIRECTORY
    } else {
        config.directory.as_str()
    };
    let path = Utf8Path::new(named);
    let leaves = path.components().any(|part| {
        matches!(
            part,
            Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
        )
    });
    if leaves {
        bail!(
            "`ui.directory` in uf.config.js is `{named}`, which leaves the project; name a \
             directory inside it, such as `{DEFAULT_DIRECTORY}`"
        );
    }
    Ok(path)
}

/// The registry this uf was built with.
fn registry() -> Result<Registry> {
    Registry::embedded().context(
        "this uf was built with a component registry it cannot read, which is a defect in the \
         uf binary rather than in this project",
    )
}

// --- uf ui add -----------------------------------------------------------------

fn add(cwd: &Utf8Path, ui: &mut Ui, names: &[String], overwrite: bool) -> Result<()> {
    let place = Place::find(cwd)?;
    let registry = registry()?;
    let asked: Vec<&str> = names.iter().map(String::as_str).collect();

    let plan = match project::plan_add(&place.root, &place.directory, &registry, &asked, overwrite)
    {
        Ok(plan) => plan,
        Err(AddError::Unknown(unknown)) => return Err(unknown_component(&registry, &unknown.name)),
        Err(AddError::Conflicts(conflicts)) => return Err(refusal(&place, &conflicts)),
        Err(other) => return Err(other.into()),
    };

    if !plan.packages.is_empty() {
        let specs: Vec<String> = plan
            .packages
            .iter()
            .map(|package| project::package_spec(package))
            .collect();
        crate::commands::pm::add(
            &place.root,
            ui,
            &specs,
            DependencyKind::Prod,
            &crate::commands::pm::Scope::Project,
        )
        .with_context(|| {
            format!(
                "`uf ui add` wrote no component, because the packages they import could not \
                     be added: {}",
                specs.join(" ")
            )
        })?;
    }

    project::apply(&plan)
        .with_context(|| format!("could not write into {}", place.relative(&place.directory)))?;
    render_added(ui, &place, &plan);
    Ok(())
}

/// Every file a run refused to replace, and the two commands that follow.
fn refusal(place: &Place, conflicts: &[Conflict]) -> anyhow::Error {
    let named: Vec<&str> = conflicts
        .iter()
        .map(|conflict| conflict.component)
        .collect();
    let names = named.join(" ");
    let mut message = match conflicts {
        [one] => format!(
            "{} {}, and `uf ui add` will not replace it",
            place.relative(&one.path),
            what_it_is(one)
        ),
        _ => {
            let mut message = format!(
                "{} files would be replaced, and `uf ui add` will not replace a file it did not \
                 write:",
                conflicts.len()
            );
            for conflict in conflicts {
                message.push_str(&format!(
                    "\n  {} {}",
                    place.relative(&conflict.path),
                    what_it_is(conflict)
                ));
            }
            message
        }
    };
    message.push_str(&format!(
        "\n\n  nothing was written\n\n  uf ui diff {names}                shows how it differs from \
         this uf's version\n  uf ui add {names} --overwrite     replaces it with this uf's version"
    ));
    anyhow!(message)
}

fn what_it_is(conflict: &Conflict) -> &'static str {
    if conflict.edited {
        "has changed since `uf ui add` wrote it"
    } else {
        "was not written by `uf ui add`"
    }
}

/// A name the registry does not have, with the closest ones it does.
fn unknown_component(registry: &Registry, name: &str) -> anyhow::Error {
    let names = registry.components().iter().map(|component| component.name);
    let suggestions = crate::suggest::closest(name, names);
    let mut message = format!("no component named `{name}`");
    if !suggestions.is_empty() {
        message.push_str("\n\n  did you mean: ");
        message.push_str(&suggestions.join(", "));
    }
    message.push_str("\n\n  `uf ui list` shows every component this uf carries");
    anyhow!(message)
}

fn render_added(ui: &mut Ui, place: &Place, plan: &AddPlan) {
    let label = project_label(&place.root).to_owned();
    let rows: Vec<(String, &str, String, Tone)> = plan
        .steps
        .iter()
        .map(|step| {
            let (what, tone) = match &step.action {
                AddAction::Create => ("added".to_owned(), Tone::Good),
                AddAction::Update { from } => (format!("updated from uf {from}"), Tone::Good),
                AddAction::Unchanged => ("already this uf's version".to_owned(), Tone::Muted),
                AddAction::Replace => ("replaced".to_owned(), Tone::Warn),
                AddAction::Keep { edited: true } => {
                    ("kept: edited since it was added".to_owned(), Tone::Warn)
                }
                AddAction::Keep { edited: false } => {
                    ("kept: not written by uf ui add".to_owned(), Tone::Warn)
                }
            };
            (place.relative(&step.path), step.component, what, tone)
        })
        .collect();
    let written = plan
        .steps
        .iter()
        .filter(|step| step.contents.is_some())
        .count();
    let kept = plan
        .steps
        .iter()
        .any(|step| matches!(step.action, AddAction::Keep { .. }));

    ui.render(|renderer, out| {
        renderer.banner(out, "uf ui add", Some(&label));
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("file"),
            Column::left("component"),
            Column::left("result"),
        ]);
        for (path, component, what, tone) in &rows {
            table.push(vec![
                Cell::toned(path, Tone::Path),
                Cell::new(component),
                Cell::toned(what, *tone),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        if written == 0 {
            renderer.status(
                out,
                Status::Success,
                "nothing to write: every file is already this uf's version",
            );
        } else {
            renderer.status(
                out,
                Status::Success,
                &format!("wrote {}", plural(written, "file")),
            );
        }
        if kept {
            renderer.status(
                out,
                Status::Info,
                "a kept file stays as it is; `uf ui diff` shows how it differs from this uf's version",
            );
        }
        renderer.blank(out);
    });
}

// --- uf ui list ----------------------------------------------------------------

fn list(cwd: &Utf8Path, ui: &mut Ui, as_json: bool) -> Result<()> {
    let place = Place::find(cwd)?;
    let registry = registry()?;
    let copies = project::survey(&place.directory, &registry)
        .with_context(|| format!("could not read {}", place.relative(&place.directory)))?;

    if as_json {
        ui.json(&json!({
            "version": REGISTRY_VERSION,
            "directory": place.relative(&place.directory),
            "components": registry
                .components()
                .iter()
                .zip(&copies)
                .map(|(component, copy)| json!({
                    "name": component.name,
                    "kind": component.kind.as_str(),
                    "description": component.description.as_str(),
                    "dependencies": component.dependencies.iter().map(|each| each.as_str()).collect::<Vec<_>>(),
                    "requires": component.requires.iter().map(|each| each.as_str()).collect::<Vec<_>>(),
                    "file": place.relative(&copy.path),
                    "state": copy.state.as_str(),
                    "from": copy.state.stamp().map(|stamp| stamp.version.as_str()),
                }))
                .collect::<Vec<_>>(),
        }))?;
        return Ok(());
    }

    let label = project_label(&place.root).to_owned();
    let directory = place.relative(&place.directory);
    let rows: Vec<(&str, String, Tone, &str)> = registry
        .components()
        .iter()
        .zip(&copies)
        .map(|(component, copy)| {
            let (state, tone) = describe_state(&copy.state);
            (component.name, state, tone, component.description.as_str())
        })
        .collect();
    let added = copies
        .iter()
        .filter(|copy| copy.state != CopyState::Missing)
        .count();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf ui list", Some(&label));
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("component"),
            Column::left("in this project"),
            Column::left("what it is"),
        ]);
        for (name, state, tone, description) in &rows {
            table.push(vec![
                Cell::new(name),
                Cell::toned(state, *tone),
                Cell::toned(description, Tone::Muted),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!(
                "{} in uf {REGISTRY_VERSION}; {added} added to {directory}",
                plural(rows.len(), "component")
            ),
        );
        renderer.blank(out);
    });
    Ok(())
}

/// A copy's state, as a person reads it, and how loudly.
fn describe_state(state: &CopyState) -> (String, Tone) {
    match state {
        CopyState::Missing => ("—".to_owned(), Tone::Muted),
        CopyState::Current { stamp } => (format!("added, uf {}", stamp.version), Tone::Good),
        CopyState::Outdated { stamp } => (
            format!("added, uf {}; this uf's version differs", stamp.version),
            Tone::Warn,
        ),
        CopyState::Edited {
            stamp,
            registry_moved: false,
        } => (format!("edited since uf {}", stamp.version), Tone::Accent),
        CopyState::Edited {
            stamp,
            registry_moved: true,
        } => (
            format!(
                "edited since uf {}; this uf's version differs too",
                stamp.version
            ),
            Tone::Warn,
        ),
        CopyState::Foreign => ("a file uf ui add did not write".to_owned(), Tone::Warn),
    }
}

// --- uf ui diff ----------------------------------------------------------------

/// One component compared.
struct Compared<'a> {
    component: &'a Component,
    copy: ProjectCopy,
    unified: Option<String>,
}

fn diff(cwd: &Utf8Path, ui: &mut Ui, names: &[String], as_json: bool) -> Result<()> {
    let place = Place::find(cwd)?;
    let registry = registry()?;

    let components: Vec<&Component> = if names.is_empty() {
        registry
            .components()
            .iter()
            .filter(|component| place.directory.join(component.file_name()).exists())
            .collect()
    } else {
        names
            .iter()
            .map(|name| {
                registry
                    .get(name)
                    .ok_or_else(|| unknown_component(&registry, name))
            })
            .collect::<Result<_>>()?
    };

    let mut compared = Vec::with_capacity(components.len());
    for component in components {
        let copy = project::inspect(&place.directory, component)
            .with_context(|| format!("could not read {}", place.relative(&place.directory)))?;
        if copy.state == CopyState::Missing {
            bail!(
                "{} does not exist, so there is nothing to compare\n\n  uf ui add {}   writes it",
                place.relative(&copy.path),
                component.name
            );
        }
        let unified = uf_ui::diff::unified(
            &format!("{}.js in uf {REGISTRY_VERSION}", component.name),
            &place.relative(&copy.path),
            component.source,
            copy.content.as_deref().unwrap_or_default(),
        );
        compared.push(Compared {
            component,
            copy,
            unified,
        });
    }

    if as_json {
        ui.json(&json!({
            "version": REGISTRY_VERSION,
            "components": compared
                .iter()
                .map(|each| json!({
                    "name": each.component.name,
                    "file": place.relative(&each.copy.path),
                    "state": each.copy.state.as_str(),
                    "from": each.copy.state.stamp().map(|stamp| stamp.version.as_str()),
                    "registryMoved": match &each.copy.state {
                        CopyState::Edited { registry_moved, .. } => Some(*registry_moved),
                        CopyState::Outdated { .. } => Some(true),
                        CopyState::Current { .. } => Some(false),
                        CopyState::Missing | CopyState::Foreign => None,
                    },
                    "diff": each.unified,
                }))
                .collect::<Vec<_>>(),
        }))?;
        return Ok(());
    }

    let label = project_label(&place.root).to_owned();
    let directory = place.relative(&place.directory);
    ui.render(|renderer, out| {
        renderer.banner(out, "uf ui diff", Some(&label));
        renderer.blank(out);
        if compared.is_empty() {
            renderer.status(
                out,
                Status::Info,
                &format!(
                    "no component has been added to {directory}; `uf ui list` shows what this uf \
                     carries"
                ),
            );
            renderer.blank(out);
            return;
        }
        for each in &compared {
            let heading = format!(
                "{}  {}",
                each.component.name,
                place.relative(&each.copy.path)
            );
            renderer.heading(out, 2, &heading);
            let (status, sentence) = explain_state(&each.copy.state, each.component.name);
            renderer.status(out, status, &sentence);
            if let Some(unified) = &each.unified {
                renderer.blank(out);
                render_unified(renderer, out, unified);
            }
            renderer.blank(out);
        }
    });
    Ok(())
}

/// What a copy's state means for the diff printed under it.
///
/// The sentence is the reason `uf ui diff` exists rather than `diff`: it says
/// whether the lines below are the project's own edits, the registry's changes,
/// or both at once.
fn explain_state(state: &CopyState, name: &str) -> (Status, String) {
    match state {
        CopyState::Current { stamp } => (
            Status::Success,
            format!("the same as uf {}'s version, untouched", stamp.version),
        ),
        CopyState::Outdated { stamp } => (
            Status::Warn,
            format!(
                "untouched since uf {} wrote it, and this uf's version differs: every line below is \
                 the registry's, and `uf ui add {name}` takes them",
                stamp.version
            ),
        ),
        CopyState::Edited {
            stamp,
            registry_moved: false,
        } => (
            Status::Info,
            format!(
                "edited since uf {} wrote it, and the registry has not moved: every line below is \
                 this project's",
                stamp.version
            ),
        ),
        CopyState::Edited {
            stamp,
            registry_moved: true,
        } => (
            Status::Warn,
            format!(
                "edited since uf {} wrote it, and this uf's version differs too: the lines below \
                 are the project's edits and the registry's changes together",
                stamp.version
            ),
        ),
        CopyState::Foreign => (
            Status::Warn,
            "not written by `uf ui add`, so there is no version to say it began as".to_owned(),
        ),
        CopyState::Missing => (Status::Skip, "not in this project".to_owned()),
    }
}

/// A unified diff, a line at a time, coloured by what each line is.
fn render_unified(renderer: &Renderer, out: &mut String, unified: &str) {
    let theme = renderer.theme();
    for line in unified.lines() {
        let style = if line.starts_with("+++") || line.starts_with("---") {
            theme.muted
        } else if line.starts_with('+') {
            theme.success
        } else if line.starts_with('-') {
            theme.error
        } else if line.starts_with("@@") {
            theme.accent
        } else {
            theme.value
        };
        out.push_str("    ");
        style.paint(renderer.color(), line, out);
        out.push('\n');
    }
}
