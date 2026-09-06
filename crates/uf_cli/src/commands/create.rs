//! `uf create`: a tree of what was generated, and what to run next.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_project::{CreateKind, CreateOptions, create_project};
use uf_term::{Status, Tree};

use crate::brand;
use crate::cli::{AppTemplate, CreateCommand};
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

/// Which template, and where, out of the one or two positionals given.
///
/// `uf create app` takes `[TEMPLATE] [PATH]`, and the first command a reader
/// types is one word. So a lone argument is the template when it names one and
/// the path when it does not — which is what the home page, the CLI reference
/// and `ufx @uniflowed/create app` already assumed, and what the grammar
/// rejected. See ubugeeei-prod/uf#322.
///
/// Two arguments keep the old meaning exactly: the first must be a template,
/// and saying so is better than quietly reading a typo as a directory name and
/// scaffolding into it.
fn app_arguments(
    template_or_path: Option<String>,
    path: Option<Utf8PathBuf>,
) -> Result<(AppTemplate, Option<Utf8PathBuf>)> {
    let templates = AppTemplate::ALL.join(", ");
    match (template_or_path, path) {
        (None, path) => Ok((AppTemplate::React, path)),
        (Some(first), None) => match AppTemplate::parse(&first) {
            Some(template) => Ok((template, None)),
            None => Ok((AppTemplate::React, Some(Utf8PathBuf::from(first)))),
        },
        (Some(first), Some(path)) => match AppTemplate::parse(&first) {
            Some(template) => Ok((template, Some(path))),
            None => bail!(
                "`{first}` is not a template, and with two arguments the first one is the \
                 template.\n  templates: {templates}\n  for a project in `{first}`, write \
                 `uf create app {first}` with nothing after it"
            ),
        },
    }
}

pub(crate) fn create(cwd: &Utf8Path, ui: &mut Ui, command: CreateCommand) -> Result<()> {
    let (kind, target, name, force) = match command {
        CreateCommand::App {
            template_or_path,
            path,
            name,
            force,
        } => {
            let (template, path) = app_arguments(template_or_path, path)?;
            let AppTemplate::React = template;
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, "uniflowed-app"));
            (CreateKind::AppReact, target, name, force)
        }
        CreateCommand::Lib { path, name, force } => {
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, "uniflowed-lib"));
            (CreateKind::Lib, target, name, force)
        }
    };

    let label = name.clone();
    let report = create_project(&target, &CreateOptions { name, kind, force })?;
    let files = report
        .files
        .iter()
        .map(|file| relative_to(&report.root, file))
        .collect::<Vec<_>>();
    let paths = files.iter().map(String::as_str).collect::<Vec<_>>();
    let root = project_label(&report.root).to_string();
    let created = format!("created {} in {}", plural(files.len(), "file"), report.root);

    let change_directory =
        (report.root != cwd).then(|| format!("cd {}", project_label(&report.root)));
    let mut steps = Vec::new();
    if let Some(step) = &change_directory {
        steps.push(step.as_str());
    }
    steps.push("uf install");
    steps.push(match kind {
        CreateKind::AppReact => "uf dev",
        CreateKind::Lib => "uf test",
    });

    ui.render(|renderer, out| {
        // First contact with the toolchain, which is the one moment a mark
        // earns its five rows.
        brand::render_mark(renderer, out, "uf create");
        renderer.blank(out);
        renderer.banner(out, "uf create", Some(&label));
        renderer.blank(out);
        renderer.tree(out, 2, &Tree::from_paths(&root, paths.iter().copied()));
        renderer.blank(out);
        renderer.heading(out, 2, "next steps");
        renderer.ordered_list(out, 4, &steps);
        renderer.blank(out);
        renderer.status(out, Status::Success, &created);
    });
    Ok(())
}

fn resolve_target(cwd: &Utf8Path, path: Option<Utf8PathBuf>) -> Result<Utf8PathBuf> {
    Ok(match path {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    })
}

fn project_name(path: &Utf8Path, fallback: &str) -> String {
    path.file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_target_resolves_against_the_working_directory() {
        let cwd = Utf8Path::new("/tmp/work");
        assert_eq!(
            resolve_target(cwd, Some(Utf8PathBuf::from("demo"))).unwrap(),
            Utf8PathBuf::from("/tmp/work/demo")
        );
    }

    #[test]
    fn an_absolute_target_is_used_as_is() {
        let cwd = Utf8Path::new("/tmp/work");
        assert_eq!(
            resolve_target(cwd, Some(Utf8PathBuf::from("/elsewhere"))).unwrap(),
            Utf8PathBuf::from("/elsewhere")
        );
    }

    #[test]
    fn no_target_creates_in_the_working_directory() {
        let cwd = Utf8Path::new("/tmp/work");
        assert_eq!(resolve_target(cwd, None).unwrap(), cwd.to_path_buf());
    }

    #[test]
    fn the_project_name_falls_back_when_the_path_has_no_segment() {
        assert_eq!(project_name(Utf8Path::new("/tmp/demo"), "fallback"), "demo");
        assert_eq!(project_name(Utf8Path::new("/"), "fallback"), "fallback");
    }
}
