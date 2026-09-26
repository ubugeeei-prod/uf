//! What `uf editor setup <editor>` writes into a project, and how it decides.
//!
//! Each editor that has a per-project settings file gets the settings its page
//! under `/guide/editors` tells a reader to commit: the ones that stop the
//! editor's own JavaScript/TypeScript server from checking Flow files in this
//! project, and the ones that make `uf lsp` the formatter. This module owns
//! *what* those are and *how* an existing file is treated; `editor.rs` owns
//! reading, writing and printing.
//!
//! # The rule for an existing file
//!
//! Add what is missing, keep everything else. A settings file is the project's
//! and is usually shared: a value somebody chose — `"javascript.validate.enable":
//! true` because they want TypeScript's errors, a different formatter — is
//! kept and reported, never overwritten. JSONC files are edited in place by
//! [`super::jsonc`], which inserts members and leaves comments and order
//! alone. A file uf cannot merge into safely (TOML with the same language
//! already configured, an Emacs Lisp alist) is a [`FilePlan::Conflict`]: the
//! reader is told what to add, and uf writes nothing to it.

use anyhow::Result;
use serde_json::{Value, json};

use super::assets;
use super::jsonc::{self, Want};
use crate::cli::Editor;

/// The id the VS Code extension is published under: `publisher.name`.
pub(crate) const VSCODE_EXTENSION: &str = "uniflowed.uf";

/// One file `uf editor setup` may write.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    /// Relative to the project root.
    pub(crate) path: &'static str,
    pub(crate) kind: TargetKind,
}

/// How a target's contents are decided.
#[derive(Debug, Clone)]
pub(crate) enum TargetKind {
    /// A JSONC settings file that must say each of these.
    Jsonc(Vec<Want>),
    /// A file uf writes whole when it is missing, recognises by `marker` when
    /// it is already set up, and appends to when neither — unless the file
    /// contains one of `conflicts`, which means the project configures the
    /// same thing its own way.
    Whole {
        contents: String,
        marker: &'static str,
        conflicts: &'static [&'static str],
        /// Whether text can be appended to an existing file safely. TOML
        /// tables and Lua statements can; an Emacs Lisp alist cannot.
        appendable: bool,
    },
}

/// What will happen to one target.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FilePlan {
    /// The file does not exist and is written with this text.
    Create { text: String },
    /// An existing JSONC file gets these insertions.
    Edit(jsonc::Edit),
    /// An existing file gets this text appended.
    Append { text: String },
    /// The file already says everything.
    Already,
    /// uf cannot merge into the file; `reason` says why and what to add.
    Conflict { reason: String },
}

impl FilePlan {
    /// Whether carrying out the plan writes to the file.
    pub(crate) fn writes(&self) -> bool {
        match self {
            Self::Create { .. } | Self::Append { .. } => true,
            Self::Edit(edit) => edit.changes(),
            Self::Already | Self::Conflict { .. } => false,
        }
    }

    /// The file's text after the plan, given its text before.
    pub(crate) fn result(&self, existing: Option<&str>) -> Option<String> {
        match self {
            Self::Create { text } => Some(text.clone()),
            Self::Edit(edit) => Some(edit.text.clone()),
            Self::Append { text } => {
                let mut out = existing.unwrap_or_default().to_owned();
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(text);
                Some(out)
            }
            Self::Already | Self::Conflict { .. } => None,
        }
    }
}

/// The files `uf editor setup <editor>` manages. Empty for an editor whose
/// per-project settings uf does not write — JetBrains keeps them in `.idea/`
/// in a form that is not documented, and Vim has no per-project file — which
/// `editor.rs` answers with the manual steps instead.
pub(crate) fn targets(editor: Editor) -> Vec<Target> {
    match editor {
        Editor::Vscode | Editor::Cursor => vec![
            Target {
                path: ".vscode/settings.json",
                kind: TargetKind::Jsonc(vscode_settings(editor == Editor::Vscode)),
            },
            Target {
                path: ".vscode/extensions.json",
                kind: TargetKind::Jsonc(vec![Want::contains(
                    &["recommendations"],
                    json!(VSCODE_EXTENSION),
                )]),
            },
        ],
        Editor::Zed => vec![Target {
            path: ".zed/settings.json",
            kind: TargetKind::Jsonc(zed_settings()),
        }],
        Editor::Helix => vec![Target {
            path: ".helix/languages.toml",
            kind: TargetKind::Whole {
                contents: assets::HELIX.to_owned(),
                marker: "[language-server.uf]",
                conflicts: &["name = \"javascript\"", "name = \"jsx\""],
                appendable: true,
            },
        }],
        Editor::Neovim => vec![Target {
            path: ".nvim.lua",
            kind: TargetKind::Whole {
                contents: NVIM_LUA.to_owned(),
                marker: "require(\"uf\")",
                conflicts: &[],
                appendable: true,
            },
        }],
        Editor::Emacs => vec![Target {
            path: ".dir-locals.el",
            kind: TargetKind::Whole {
                contents: DIR_LOCALS.to_owned(),
                marker: "lsp-disabled-clients",
                conflicts: &[],
                appendable: false,
            },
        }],
        Editor::Jetbrains | Editor::Vim => Vec::new(),
    }
}

/// `.vscode/settings.json` for a uf project.
///
/// The validation switch twice: VS Code 1.110 renamed it to
/// `js/ts.validate.enabled` and reads the old `javascript.validate.enable`
/// only while the new one is set nowhere. `"[javascript]"` covers `.jsx` for
/// validation, because VS Code reads it under the `javascript` id for both;
/// the formatter and on-save lines are per language id, so `.jsx` gets its
/// own block. Cursor's extension API predates 1.110, so for Cursor the new
/// name is left out rather than written as an unknown key.
fn vscode_settings(unified_name: bool) -> Vec<Want> {
    let mut wants = vec![Want::value(&["javascript.validate.enable"], json!(false))];
    if unified_name {
        wants.push(Want::value(
            &["[javascript]", "js/ts.validate.enabled"],
            json!(false),
        ));
    }
    for language in ["[javascript]", "[javascriptreact]"] {
        wants.push(Want::value(
            &[language, "editor.defaultFormatter"],
            json!(VSCODE_EXTENSION),
        ));
        wants.push(Want::value(&[language, "editor.formatOnSave"], json!(true)));
        wants.push(Want::value(
            &[language, "editor.codeActionsOnSave", "source.fixAll.uf"],
            json!("explicit"),
        ));
    }
    wants
}

/// `.zed/settings.json` for a uf project: uf in, vtsls and
/// typescript-language-server out, every other JavaScript server (`"..."`)
/// kept, and uf as the formatter on save. A project's list replaces the
/// user's in Zed, so a `language_servers` the project already has is kept as
/// it is rather than merged into.
fn zed_settings() -> Vec<Want> {
    let base = ["languages", "JavaScript"];
    let at = |key: &'static str| [base[0], base[1], key];
    vec![
        Want::value(
            &at("language_servers"),
            json!(["uf", "!vtsls", "!typescript-language-server", "..."]),
        ),
        Want::value(
            &at("formatter"),
            json!({ "language_server": { "name": "uf" } }),
        ),
        Want::value(&at("format_on_save"), json!("on")),
    ]
}

/// `.nvim.lua`, which Neovim reads from the directory it starts in when
/// `exrc` is on and the file is trusted (`:trust`).
const NVIM_LUA: &str = "-- uf: start `uf lsp` for this project's Flow files and keep ts_ls, vtsls\n\
-- and denols off them. Needs lua/uf.lua (`uf editor install neovim`) and\n\
-- `vim.o.exrc = true`; Neovim asks once before running this file (:trust).\n\
require(\"uf\").setup()\n";

/// `.dir-locals.el`: lsp-mode's TypeScript clients off in this project.
/// `lsp-disabled-clients` is marked safe as a directory-local variable, so
/// Emacs applies it without asking. Eglot needs nothing here: `uf.el` serves a
/// uf project with uf.
const DIR_LOCALS: &str = ";; uf: lsp-mode's TypeScript clients read Flow as TypeScript; uf serves this project.\n\
((js-mode . ((lsp-disabled-clients . (ts-ls jsts-ls tsgo deno-ls))))\n \
(js-ts-mode . ((lsp-disabled-clients . (ts-ls jsts-ls tsgo deno-ls)))))\n";

/// Decide what to do with one target, given the file's current text.
///
/// # Errors
///
/// When an existing JSONC file cannot be read as JSONC; the caller writes
/// nothing to it.
pub(crate) fn plan(target: &Target, existing: Option<&str>) -> Result<FilePlan> {
    match &target.kind {
        TargetKind::Jsonc(wants) => {
            let edit = jsonc::edit(existing, wants)?;
            // An existing file is always an `Edit`, even one that changes
            // nothing, so the values it keeps can still be reported.
            Ok(match existing {
                None => FilePlan::Create { text: edit.text },
                Some(_) => FilePlan::Edit(edit),
            })
        }
        TargetKind::Whole {
            contents,
            marker,
            conflicts,
            appendable,
        } => {
            let Some(existing) = existing else {
                return Ok(FilePlan::Create {
                    text: contents.clone(),
                });
            };
            if existing.contains(marker) {
                return Ok(FilePlan::Already);
            }
            if let Some(found) = conflicts.iter().find(|needle| existing.contains(*needle)) {
                return Ok(FilePlan::Conflict {
                    reason: uf_infra::into_string(uf_infra::cstr!(
                        "it already has `{found}`, so uf would be a second configuration of the \
                         same language; add uf to that one by hand"
                    )),
                });
            }
            if !appendable {
                return Ok(FilePlan::Conflict {
                    reason: "uf cannot merge into it safely; add this by hand".to_owned(),
                });
            }
            Ok(FilePlan::Append {
                text: uf_infra::into_string(uf_infra::cstr!("\n{contents}")),
            })
        }
    }
}

/// The `Value` a JSONC want would write, for messages.
pub(crate) fn wanted_value(want: &Want) -> Value {
    match &want.kind {
        jsonc::WantKind::Value(value) | jsonc::WantKind::Contains(value) => value.clone(),
    }
}
