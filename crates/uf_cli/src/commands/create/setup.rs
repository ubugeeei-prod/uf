//! Choices made before a new built-in project is written.
use crate::cli::Editor;
use crate::ui::Ui;
use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use std::fs;
use uf_term::prompt::{Choice, ManyOutcome, Outcome, Request, select, select_many};

/// Explicit choices and whether unanswered questions may be prompted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) package_manager: Option<String>,
    pub(crate) editors: Vec<Editor>,
    pub(crate) no_editors: bool,
    pub(crate) yes: bool,
}

pub(crate) fn parse_manager(value: &str) -> Result<String, String> {
    if value == "uf" {
        return Ok(value.to_owned());
    }
    uf_config::PackageManagerSpec::parse(value)
        .map(|spec| spec.to_string())
        .map_err(|error| error.to_string())
}

const MANAGERS: &[Choice<'static>] = &[
    Choice::new("uf", "uf's native package manager"),
    Choice::new("npm", "npm on PATH"),
    Choice::new("pnpm", "pnpm on PATH"),
    Choice::new("yarn", "Yarn on PATH"),
    Choice::new("bun", "Bun on PATH"),
];
const EDITORS: &[(Editor, &str, &str)] = &[
    (
        Editor::Vscode,
        "vscode",
        "VS Code: install extension and configure Flow",
    ),
    (
        Editor::Cursor,
        "cursor",
        "Cursor: install extension and configure Flow",
    ),
    (
        Editor::Zed,
        "zed",
        "Zed: stage extension and configure Flow",
    ),
    (
        Editor::Jetbrains,
        "jetbrains",
        "JetBrains: stage LSP4IJ configuration",
    ),
    (
        Editor::Neovim,
        "neovim",
        "Neovim: install Lua integration and configure Flow",
    ),
    (Editor::Vim, "vim", "Vim: install vim-lsp integration"),
    (
        Editor::Helix,
        "helix",
        "Helix: configure Flow language server",
    ),
    (
        Editor::Emacs,
        "emacs",
        "Emacs: install integration and configure Flow",
    ),
];

impl Options {
    pub(crate) fn choose(mut self) -> Result<Self> {
        if self.package_manager.is_none() && !self.yes {
            match select(&Request::new("Package manager", MANAGERS)) {
                Outcome::Chose(choice) => self.package_manager = Some(choice.name.to_owned()),
                Outcome::Cancelled => bail!("project creation cancelled; no files written"),
                Outcome::NotInteractive => {}
            }
        }
        if self.editors.is_empty() && !self.no_editors && !self.yes {
            let choices: Vec<_> = EDITORS
                .iter()
                .map(|(_, name, about)| Choice::new(name, about))
                .collect();
            match select_many(&Request::new(
                "IDE setup (optional; Space selects multiple)",
                &choices,
            )) {
                ManyOutcome::Chose(chosen) => {
                    self.editors = chosen
                        .iter()
                        .filter_map(|choice| {
                            EDITORS
                                .iter()
                                .find(|(_, name, _)| *name == choice.name)
                                .map(|(editor, _, _)| *editor)
                        })
                        .collect()
                }
                ManyOutcome::Cancelled => bail!("project creation cancelled; no files written"),
                ManyOutcome::NotInteractive => {}
            }
        }
        // No TTY keeps the previous scaffold unchanged unless a choice was explicit.
        let mut unique = Vec::with_capacity(self.editors.len());
        for editor in self.editors {
            if !unique.contains(&editor) {
                unique.push(editor);
            }
        }
        self.editors = unique;
        Ok(self)
    }

    pub(crate) fn configure(
        &self,
        root: &Utf8Path,
        monorepo: bool,
    ) -> Result<Vec<camino::Utf8PathBuf>> {
        let mut files = Vec::new();
        if let Some(manager) = &self.package_manager {
            let path = root.join("uf.config.js");
            let source = fs::read_to_string(&path)?;
            let setting = if manager == "uf" {
                "  pm: { packageManager: \"uf\" },\n".to_owned()
            } else {
                uf_infra::into_string(uf_infra::cstr!(
                    "  packageManager: {},\n",
                    serde_json::to_string(manager)?
                ))
            };
            let marker = "export default defineConfig({\n";
            let (before, after) = source
                .split_once(marker)
                .context("new project configuration has no defineConfig object")?;
            let mut result = String::with_capacity(source.len() + setting.len());
            result.push_str(before);
            result.push_str(marker);
            result.push_str(&setting);
            result.push_str(after);
            fs::write(&path, result)?;
            if monorepo && manager.split('@').next() == Some("pnpm") {
                let path = root.join("pnpm-workspace.yaml");
                fs::write(
                    &path,
                    "packages:\n  - apps/*\n  - npm/*\nlinkWorkspacePackages: true\n",
                )?;
                files.push(path);
            }
            if manager.split('@').next() == Some("yarn") {
                let path = root.join(".yarnrc.yml");
                fs::write(&path, "nodeLinker: node-modules\n")?;
                files.push(path);
            }
        }
        Ok(files)
    }

    pub(crate) fn install_editors(&self, root: &Utf8Path, ui: &mut Ui) -> Result<()> {
        let options = super::super::editor::InstallOptions {
            version: None,
            vsix: None,
            force: false,
            dry_run: false,
        };
        let mut failures = Vec::new();
        for editor in &self.editors {
            let result = super::super::editor::setup(root, ui, *editor, false)
                .and_then(|()| super::super::editor::install(ui, *editor, &options));
            if let Err(error) = result {
                failures.push(uf_infra::into_string(uf_infra::cstr!(
                    "{}: {error:#}",
                    editor.name()
                )));
            }
        }
        if !failures.is_empty() {
            bail!(uf_infra::cstr!(
                "project created at {root}, but IDE setup needs attention:\n{}\nretry with `uf editor setup <editor>` and `uf editor install <editor>`",
                failures.join("\n")
            ));
        }
        Ok(())
    }
}
