//! `uf init` and `uf new`: a tree of what was generated, and what to run
//! next.

mod remote;

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_project::{CreateKind, CreateOptions, create_project};
use uf_term::{Status, Tree};

use crate::brand;
use crate::cli::{AppTemplate, CreateCommand};
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

use remote::RemoteTemplate;

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
            None => bail!(uf_infra::cstr!(
                "`{first}` is not a template, and with two arguments the first one is the \
                 template.\n  templates: {templates}\n  for a project in `{first}`, write \
                 `uf create app {first}` with nothing after it"
            )),
        },
    }
}

/// What `uf init` and `uf new` were asked for.
///
/// One value rather than six arguments, because the two commands differ in
/// `path` alone and pass the rest through unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scaffold {
    /// `None` for `init`, the new directory for `new`.
    pub(crate) path: Option<Utf8PathBuf>,
    /// A built-in template's name, or a remote template's source.
    pub(crate) template: Option<String>,
    /// The digest a remote tarball must have.
    pub(crate) integrity: Option<String>,
    /// `--lib`.
    pub(crate) lib: bool,
    /// `--name`.
    pub(crate) name: Option<String>,
    /// `--force`.
    pub(crate) force: bool,
}

/// `uf init` and `uf new`: one function, because they differ in one argument.
///
/// `path` is `None` for `init` and the new directory for `new`. Everything
/// after that — the template, the name, the tree that is printed — is the same
/// scaffold, which is the point of the split: the two commands say *where*,
/// and nothing else about them differs.
pub(crate) fn scaffold(cwd: &Utf8Path, ui: &mut Ui, request: Scaffold) -> Result<()> {
    let Scaffold {
        path,
        template,
        integrity,
        lib,
        name,
        force,
    } = request;
    // The banner names the command the reader typed. A run of `uf new` headed
    // `uf create` sends them to the help for a command they did not use.
    let spelling = if path.is_some() { "uf new" } else { "uf init" };
    let templates = AppTemplate::ALL.join(", ");

    // A remote source first, because it is recognisable as one before it is
    // checked, and an unpinned one has to be refused for being unpinned rather
    // than for not being a template's name.
    let remote = match template.as_deref() {
        Some(written) => RemoteTemplate::parse(written, integrity.as_deref())?,
        None => None,
    };
    if let Some(remote) = remote {
        if lib {
            bail!(uf_infra::cstr!(
                "`--lib` takes no template: a library is one shape, and `uf new --lib` writes it"
            ));
        }
        if name.is_some() {
            bail!(uf_infra::cstr!(
                "`--name` names the package a built-in template writes. A remote template is \
                 copied as its author wrote it, manifest included; rename the package there \
                 after it is written"
            ));
        }
        let target = resolve_target(cwd, path)?;
        return render_remote(cwd, ui, spelling, &remote, target, force);
    }

    // Named rather than inferred. `uf create` guessed — a lone argument was a
    // template when it named one and a directory when it did not — and the
    // guess is what #322 is. Here the directory is a positional of its own, so
    // a word in the template's place that is not a template is a mistake and
    // is reported as one.
    let kind = match (lib, template.as_deref()) {
        (true, None) => CreateKind::Lib,
        (true, Some(named)) => bail!(uf_infra::cstr!(
            "`--lib` takes no template: a library is one shape.\n  for an \
             application from the `{named}` template, drop `--lib`"
        )),
        (false, None) => CreateKind::AppReact,
        (false, Some(named)) => match AppTemplate::parse(named) {
            Some(AppTemplate::React) => CreateKind::AppReact,
            Some(AppTemplate::Monorepo) => CreateKind::Monorepo,
            None => bail!(uf_infra::cstr!(
                "`{named}` is not a template.\n  templates: {templates}, or a remote \
                 template pinned to a commit or a digest\n  to scaffold into a directory \
                 called `{named}`, write `uf new {named}`"
            )),
        },
    };

    let target = resolve_target(cwd, path)?;
    let fallback = match kind {
        CreateKind::AppReact => "uniflowed-app",
        CreateKind::Lib => "uniflowed-lib",
        CreateKind::Monorepo => "uniflowed-monorepo",
    };
    let name = name.unwrap_or_else(|| project_name(&target, fallback));
    render_created(cwd, ui, spelling, kind, target, name, force)
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
            let (kind, fallback) = match template {
                AppTemplate::React => (CreateKind::AppReact, "uniflowed-app"),
                AppTemplate::Monorepo => (CreateKind::Monorepo, "uniflowed-monorepo"),
            };
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, fallback));
            (kind, target, name, force)
        }
        CreateCommand::Lib { path, name, force } => {
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, "uniflowed-lib"));
            (CreateKind::Lib, target, name, force)
        }
    };

    render_created(cwd, ui, "uf create", kind, target, name, force)
}

/// A built-in template's scaffold, and the tree and next steps printed from
/// what it wrote.
fn render_created(
    cwd: &Utf8Path,
    ui: &mut Ui,
    spelling: &str,
    kind: CreateKind,
    target: Utf8PathBuf,
    name: String,
    force: bool,
) -> Result<()> {
    let label = name.clone();
    let mut report = create_project(&target, &CreateOptions { name, kind, force })?;
    super::agents::update(&report.root)?;
    report.files.push(report.root.join("AGENTS.md"));
    render(
        cwd,
        ui,
        &Created {
            spelling,
            label,
            root: report.root,
            files: report.files,
            next: Some(match kind {
                CreateKind::AppReact => "uf dev",
                CreateKind::Lib => "uf test",
                // The root is a repository rather than an application, so the
                // dev server is the application package's. By path, because a
                // workspace package is otherwise named by its manifest, and
                // that name is the project's scope.
                CreateKind::Monorepo => "uf dev#apps/web",
            }),
            notes: Vec::new(),
        },
    )
}

/// A remote template, fetched, checked and copied, and the same tree.
///
/// Nothing is written into the target until the whole template has arrived
/// and passed its check, so a download that is refused leaves no half of a
/// project behind — and the staging directory it arrived in is removed either
/// way.
fn render_remote(
    cwd: &Utf8Path,
    ui: &mut Ui,
    spelling: &str,
    template: &RemoteTemplate,
    target: Utf8PathBuf,
    force: bool,
) -> Result<()> {
    let staging = remote::Staging::new()?;
    let mut progress = ui.progress();
    progress.draw("fetching the template");
    let fetched = remote::fetch(template, staging.path());
    progress.finish();
    drop(progress);
    let mut files = remote::copy_into(&fetched?, &target, force)?;
    super::agents::update(&target)?;
    if !files
        .iter()
        .any(|file| file.file_name() == Some("AGENTS.md"))
    {
        files.push(target.join("AGENTS.md"));
    }

    let source = template.label();
    let mut notes = vec![(
        Status::Info,
        uf_infra::into_string(uf_infra::cstr!("copied from {source}")),
    )];
    let scripts = remote::declared_install_scripts(&target);
    if !scripts.is_empty() {
        notes.push((
            Status::Warn,
            uf_infra::into_string(uf_infra::cstr!(
                "package.json declares {}, and `uf install` refuses to run it until \
                 `pm.allowLifecycleScripts` in uf.config.js allows it — read it before you do",
                scripts.join(", ")
            )),
        ));
    }
    render(
        cwd,
        ui,
        &Created {
            spelling,
            label: project_name(&target, "template"),
            root: target,
            files,
            next: None,
            notes,
        },
    )
}

/// What a scaffold wrote, for [`render`].
struct Created<'a> {
    /// The command the reader typed.
    spelling: &'a str,
    /// What the banner names.
    label: String,
    /// The directory it wrote into.
    root: Utf8PathBuf,
    /// Every file it wrote.
    files: Vec<Utf8PathBuf>,
    /// The step after `uf install`, when uf knows the template well enough to
    /// name one.
    next: Option<&'static str>,
    /// Lines under the tree, before the verdict.
    notes: Vec<(Status, String)>,
}

/// The tree of what was written, the next steps, and the verdict.
fn render(cwd: &Utf8Path, ui: &mut Ui, created: &Created<'_>) -> Result<()> {
    let files = created
        .files
        .iter()
        .map(|file| relative_to(&created.root, file))
        .collect::<Vec<_>>();
    let paths = files.iter().map(String::as_str).collect::<Vec<_>>();
    let root = project_label(&created.root).to_string();
    let summary = uf_infra::into_string(uf_infra::cstr!(
        "created {} in {}",
        plural(files.len(), "file"),
        created.root
    ));

    let change_directory = (created.root != cwd)
        .then(|| uf_infra::into_string(uf_infra::cstr!("cd {}", project_label(&created.root))));
    let mut steps = Vec::new();
    if let Some(step) = &change_directory {
        steps.push(step.as_str());
    }
    steps.push("uf install");
    if let Some(next) = created.next {
        steps.push(next);
    }

    ui.render(|renderer, out| {
        // First contact with the toolchain, which is the one moment a mark
        // earns its five rows.
        brand::render_mark(renderer, out, created.spelling);
        renderer.blank(out);
        renderer.banner(out, created.spelling, Some(&created.label));
        renderer.tree(out, 2, &Tree::from_paths(&root, paths.iter().copied()));
        renderer.blank(out);
        renderer.heading(out, 2, "next steps");
        renderer.ordered_list(out, 4, &steps);
        renderer.blank(out);
        for (status, note) in &created.notes {
            renderer.status(out, *status, note);
        }
        renderer.status(out, Status::Success, &summary);
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
