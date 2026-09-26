//! `uf editor install` and `uf editor setup`: an editor ready for a uf project.
//!
//! Every editor integration under `editors/` is a client of `uf lsp`, and each
//! used to be installed by hand — a `.vsix` built from a checkout, a Lua file
//! copied into `~/.config`, a dev extension pointed at a directory. And every
//! editor that ships JavaScript support of its own had to be told, by hand and
//! per project, to stop reading Flow files as TypeScript. These two commands
//! do both, from the same binary that serves the language:
//!
//! * **`uf editor install <editor>`** puts the integration on this machine
//!   ([`install`]): the VS Code extension through `code`/`cursor
//!   --install-extension`, from the release's checksummed `.vsix`; the rest from
//!   files compiled into uf, written where the editor looks.
//! * **`uf editor setup <editor>`** writes the per-project settings the editor's
//!   guide page describes ([`setup`]): adds what is missing, keeps what the
//!   project already says, shows what it added, and writes nothing on
//!   `--check`.
//!
//! What they share is the list of editors (`crate::cli::Editor`) and the
//! refusal to overwrite what someone else wrote. Neither changes anything
//! outside the file it names, and neither needs a `uf.config.js` except
//! `setup`, which is about a project and says so when there is none.

mod assets;
mod install;
mod jsonc;
mod setup;

use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_term::{KeyValue, Status};

use self::install::{FileInstall, Placed};
use self::setup::{FilePlan, Target, TargetKind};
use crate::cli::Editor;
use crate::support::project_label;
use crate::ui::Ui;

impl Editor {
    /// The editor's name as a reader knows it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Vscode => "VS Code",
            Self::Cursor => "Cursor",
            Self::Zed => "Zed",
            Self::Jetbrains => "JetBrains IDEs",
            Self::Neovim => "Neovim",
            Self::Vim => "Vim",
            Self::Helix => "Helix",
            Self::Emacs => "Emacs",
        }
    }

    /// The editor's page in the manual.
    pub(crate) fn guide(self) -> &'static str {
        match self {
            Self::Vscode | Self::Cursor => "https://docs.uniflowed.dev/guide/editors/vscode",
            Self::Zed => "https://docs.uniflowed.dev/guide/editors/zed",
            Self::Jetbrains => "https://docs.uniflowed.dev/guide/editors/jetbrains",
            Self::Neovim | Self::Vim => "https://docs.uniflowed.dev/guide/editors/neovim",
            Self::Helix => "https://docs.uniflowed.dev/guide/editors/helix",
            Self::Emacs => "https://docs.uniflowed.dev/guide/editors/emacs",
        }
    }
}

/// What `uf editor install` was asked for.
#[derive(Debug, Clone)]
pub(crate) struct InstallOptions {
    /// The release to install from; the running uf's version when absent.
    pub(crate) version: Option<String>,
    /// A `.vsix` on disk to install instead of downloading one.
    pub(crate) vsix: Option<Utf8PathBuf>,
    /// Replace a file in the editor's configuration uf did not write.
    pub(crate) force: bool,
    /// Say what would happen, and do nothing.
    pub(crate) dry_run: bool,
}

/// The process environment, as the lookups in `install.rs` take it.
fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `uf editor install <editor>`.
///
/// # Errors
///
/// When the editor's command-line launcher is not on `PATH`, the release has
/// no extension for this version, a download fails its checksum, the editor's
/// install fails, or a destination file belongs to someone else.
pub(crate) fn install(ui: &mut Ui, editor: Editor, options: &InstallOptions) -> Result<()> {
    let version = install::normalize_version(
        options
            .version
            .as_deref()
            .unwrap_or(env!("CARGO_PKG_VERSION")),
    );

    if editor == Editor::Helix {
        let guide = editor.guide();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf editor install", Some(editor.name()));
            renderer.status(
                out,
                Status::Info,
                "Helix needs nothing installed: it speaks LSP itself, and uf is configured per project",
            );
            renderer.hint(out, 2, "run `uf editor setup helix` in the project");
            renderer.hint(out, 2, guide);
        });
        return Ok(());
    }

    if let Some(cli_name) = install::extension_cli(editor) {
        return install_extension(ui, editor, cli_name, &version, options);
    }

    if options.dry_run {
        let target = match editor {
            Editor::Neovim => install::config_dir(&process_env)?.join("nvim/lua/uf.lua"),
            Editor::Vim => install::home(&process_env)?.join(".vim/plugin/uf.vim"),
            Editor::Emacs => install::data_dir(&process_env)?.join("emacs"),
            Editor::Zed => install::data_dir(&process_env)?.join("zed"),
            _ => install::data_dir(&process_env)?.join("jetbrains/lsp4ij-template"),
        };
        let shown = target.to_string();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf editor install", Some(editor.name()));
            renderer.status(
                out,
                Status::Info,
                uf_infra::cstr!("would write {shown}; run without --dry-run").as_str(),
            );
        });
        return Ok(());
    }

    let (FileInstall { path, next }, placed) =
        install::install_files(editor, &process_env, options.force)?;
    let shown = path.to_string();
    let verb = match placed {
        Placed::Written => "wrote",
        Placed::Updated => "updated",
        Placed::Unchanged => "already up to date:",
    };
    let guide = editor.guide();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf editor install", Some(editor.name()));
        renderer.status(
            out,
            Status::Success,
            uf_infra::cstr!("{verb} {shown}").as_str(),
        );
        for line in &next {
            renderer.hint(out, 2, line);
        }
        renderer.hint(out, 2, guide);
    });
    Ok(())
}

/// VS Code or Cursor: find the launcher, get a checked `.vsix`, install it.
fn install_extension(
    ui: &mut Ui,
    editor: Editor,
    cli_name: &str,
    version: &str,
    options: &InstallOptions,
) -> Result<()> {
    let path_var = std::env::var_os("PATH");
    let cli = install::find_on_path(cli_name, path_var.as_deref()).ok_or_else(|| {
        anyhow!(uf_infra::cstr!(
            "`{cli_name}` is not on PATH, and it is what installs the extension. In {}, run \
             \"Shell Command: Install '{cli_name}' command in PATH\" from the command palette, \
             then run this again",
            editor.name()
        ))
    })?;
    let base = std::env::var("UF_EDITOR_RELEASE_BASE").ok();
    let asset = install::vsix_asset(version);

    if options.dry_run {
        let source = match &options.vsix {
            Some(file) => file.to_string(),
            None => install::asset_url(base.as_deref(), version, &asset),
        };
        let cli = cli.to_string();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf editor install", Some(editor.name()));
            renderer.key_values(
                out,
                2,
                &[
                    KeyValue::new("extension", &source),
                    KeyValue::new("installs with", &cli),
                ],
            );
            renderer.status(
                out,
                Status::Info,
                "nothing installed; run without --dry-run",
            );
        });
        return Ok(());
    }

    let staging = tempfile::tempdir().context("could not create a temporary directory")?;
    let staging_path = Utf8Path::from_path(staging.path())
        .ok_or_else(|| anyhow!(uf_infra::cstr!("the temporary directory is not UTF-8")))?;
    let (vsix, checked) = match &options.vsix {
        Some(file) => {
            if !file.is_file() {
                bail!(uf_infra::cstr!("{file} is not a file"));
            }
            (file.clone(), None)
        }
        None => {
            let (file, digest) =
                install::fetch_verified(base.as_deref(), version, &asset, staging_path)?;
            (file, Some(digest))
        }
    };

    install::install_vsix(&cli, &vsix)?;

    let source = match &options.vsix {
        Some(file) => {
            uf_infra::cstr!("{file} (a local file; not checked against a release)").into_string()
        }
        None => uf_infra::cstr!("{asset} from uf@{version}").into_string(),
    };
    let digest = checked.map(|digest| uf_infra::cstr!("sha256 {}", &digest[..12]).into_string());
    let cli = cli.to_string();
    let guide = editor.guide();
    let setup_hint = uf_infra::cstr!(
        "`uf editor setup {}` in a project turns the editor's own JavaScript checking off for its Flow files",
        if editor == Editor::Cursor {
            "cursor"
        } else {
            "vscode"
        }
    ).into_string();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf editor install", Some(editor.name()));
        let mut rows = vec![
            KeyValue::new("extension", &source),
            KeyValue::new("installed with", &cli),
        ];
        if let Some(digest) = &digest {
            rows.push(KeyValue::new("verified", digest));
        }
        renderer.key_values(out, 2, &rows);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            "installed uniflowed.uf; reload the window to start it",
        );
        renderer.hint(out, 2, &setup_hint);
        renderer.hint(out, 2, guide);
    });
    Ok(())
}

/// The uf project `cwd` is in: the nearest directory at or above it with a
/// `uf.config.js`.
fn project_root(cwd: &Utf8Path) -> Option<Utf8PathBuf> {
    cwd.ancestors()
        .find(|dir| {
            uf_config::CONFIG_FILES
                .iter()
                .any(|name| dir.join(name).is_file())
        })
        .map(Utf8Path::to_path_buf)
}

/// `uf editor setup <editor> [--check]`.
///
/// # Errors
///
/// Outside a uf project; when a settings file cannot be read as the format it
/// should be (nothing is written to it); when a write fails; and, with
/// `--check`, when anything would be added.
pub(crate) fn setup(cwd: &Utf8Path, ui: &mut Ui, editor: Editor, check: bool) -> Result<()> {
    let root = project_root(cwd).ok_or_else(|| {
        anyhow!(uf_infra::cstr!(
            "no uf.config.js in {cwd} or above it; `uf editor setup` writes a uf project's editor \
             settings, so run it in one"
        ))
    })?;
    let project = project_label(&root).to_owned();
    let subtitle = uf_infra::cstr!("{} · {project}", editor.name()).into_string();
    let targets = setup::targets(editor);

    if targets.is_empty() {
        let steps = manual_steps(editor);
        let guide = editor.guide();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf editor setup", Some(&subtitle));
            renderer.status(
                out,
                Status::Info,
                &uf_infra::cstr!(
                    "{} has no project settings file uf writes; by hand:",
                    editor.name()
                )
                .into_string(),
            );
            let items = steps.iter().map(String::as_str).collect::<Vec<_>>();
            renderer.ordered_list(out, 2, &items);
            renderer.hint(out, 2, guide);
        });
        return Ok(());
    }

    let mut planned = Vec::new();
    for target in &targets {
        let path = root.join(target.path);
        let existing = read_optional(&path)?;
        let plan = setup::plan(target, existing.as_deref()).with_context(|| {
            uf_infra::cstr!(
                "{} is not a file uf can read as settings; nothing was written to it",
                target.path
            )
            .into_string()
        })?;
        planned.push((target, path, existing, plan));
    }

    let pending = planned.iter().any(|(_, _, _, plan)| plan.writes());
    ui.render(|renderer, out| {
        renderer.banner(out, "uf editor setup", Some(&subtitle));
        for (target, _, _, plan) in &planned {
            render_plan(renderer, out, target, plan, check);
        }
    });

    if check {
        if pending {
            bail!(uf_infra::cstr!(
                "the {} settings are not all written; run `uf editor setup {}`",
                editor.name(),
                editor_arg(editor)
            ));
        }
        ui.render(|renderer, out| {
            renderer.status(out, Status::Success, "every setting is in place");
        });
        return Ok(());
    }

    for (target, path, existing, plan) in &planned {
        let Some(text) = plan.result(existing.as_deref()) else {
            continue;
        };
        if !plan.writes() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| uf_infra::cstr!("could not create {parent}").into_string())?;
        }
        fs::write(path, text)
            .with_context(|| uf_infra::cstr!("could not write {}", target.path).into_string())?;
    }

    let guide = editor.guide();
    let trust = trust_note(editor);
    ui.render(|renderer, out| {
        renderer.blank(out);
        if pending {
            renderer.status(
                out,
                Status::Success,
                "written; commit them with the project",
            );
        } else {
            renderer.status(out, Status::Success, "nothing to add");
        }
        if let Some(trust) = trust {
            renderer.hint(out, 2, trust);
        }
        renderer.hint(out, 2, guide);
    });
    Ok(())
}

/// The argument that names `editor` on the command line.
fn editor_arg(editor: Editor) -> &'static str {
    match editor {
        Editor::Vscode => "vscode",
        Editor::Cursor => "cursor",
        Editor::Zed => "zed",
        Editor::Jetbrains => "jetbrains",
        Editor::Neovim => "neovim",
        Editor::Vim => "vim",
        Editor::Helix => "helix",
        Editor::Emacs => "emacs",
    }
}

/// What the editor asks before it uses what `setup` wrote, if anything.
fn trust_note(editor: Editor) -> Option<&'static str> {
    match editor {
        Editor::Zed => {
            Some("Zed starts the servers a project's settings ask for once you trust the project")
        }
        Editor::Helix => Some("Helix after 25.07 reads .helix/ once you run :workspace-trust"),
        Editor::Neovim => {
            Some("Neovim reads .nvim.lua with `vim.o.exrc = true`, once you :trust it")
        }
        _ => None,
    }
}

/// The steps for an editor whose project settings uf does not write.
fn manual_steps(editor: Editor) -> Vec<String> {
    match editor {
        Editor::Jetbrains => vec![
            "Settings → Languages & Frameworks → JavaScript: language version Flow".to_owned(),
            "Settings → Languages & Frameworks → TypeScript: uncheck TypeScript language service"
                .to_owned(),
            "uf's server itself comes from LSP4IJ: `uf editor install jetbrains`".to_owned(),
        ],
        _ => vec![
            "Vim has no JavaScript server of its own; if vim-lsp also registers \
             typescript-language-server, disable it in this project's local vimrc"
                .to_owned(),
        ],
    }
}

/// Render one file's plan: what it is, what is added, what is kept.
fn render_plan(
    renderer: &uf_term::Renderer,
    out: &mut String,
    target: &Target,
    plan: &FilePlan,
    check: bool,
) {
    let (status, verb) = match plan {
        FilePlan::Create { .. } if check => (Status::Warn, "would create"),
        FilePlan::Create { .. } => (Status::Success, "create"),
        FilePlan::Edit(edit) if edit.changes() && check => (Status::Warn, "would add to"),
        FilePlan::Edit(edit) if edit.changes() => (Status::Success, "add to"),
        FilePlan::Append { .. } if check => (Status::Warn, "would append to"),
        FilePlan::Append { .. } => (Status::Success, "append to"),
        FilePlan::Edit(_) | FilePlan::Already => (Status::Skip, "already set up:"),
        FilePlan::Conflict { .. } => (Status::Warn, "left alone:"),
    };
    renderer.status(
        out,
        status,
        uf_infra::cstr!("{verb} {}", target.path).as_str(),
    );

    let added = match plan {
        FilePlan::Create { text } | FilePlan::Append { text } => {
            vec![text.trim_matches('\n').to_owned()]
        }
        FilePlan::Edit(edit) => edit.inserted.clone(),
        _ => Vec::new(),
    };
    for block in added {
        for line in block.lines() {
            renderer.line(
                out,
                renderer.theme().success,
                uf_infra::cstr!("    + {line}").as_str(),
            );
        }
    }

    if let (FilePlan::Edit(edit), TargetKind::Jsonc(wants)) = (plan, &target.kind) {
        for (want, outcome) in wants.iter().zip(&edit.outcomes) {
            if let jsonc::Outcome::Kept { current } = outcome {
                renderer.hint(
                    out,
                    4,
                    &uf_infra::cstr!(
                        "kept {} = {current}, which this project set; uf would write {}",
                        want.path.join(" › "),
                        setup::wanted_value(want)
                    )
                    .into_string(),
                );
            }
        }
    }
    if let FilePlan::Conflict { reason } = plan {
        renderer.hint(out, 4, reason);
        if let TargetKind::Whole { contents, .. } = &target.kind {
            for line in contents.trim_matches('\n').lines() {
                renderer.line(
                    out,
                    renderer.theme().muted,
                    uf_infra::cstr!("      {line}").as_str(),
                );
            }
        }
    }
}

/// A file's text, or `None` when it does not exist.
fn read_optional(path: &Utf8Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow!(uf_infra::cstr!("could not read {path}: {error}"))),
    }
}

#[cfg(test)]
mod tests;
