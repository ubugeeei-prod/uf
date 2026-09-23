//! uf for Zed: the WebAssembly half of the extension, which tells Zed how to
//! start `uf lsp` for a worktree.
//!
//! The decisions — which binary, which arguments, which environment — are in
//! `command.rs`, as plain functions `cargo test` runs on the host. This file
//! only fetches what they need from Zed.

mod command;

use zed_extension_api::{self as zed, settings::LspSettings};

/// The language server id `extension.toml` declares, which is also the key
/// Zed files its settings under: `"lsp": { "uf": { … } }`.
const SERVER_ID: &str = "uf";

struct UfExtension;

impl zed::Extension for UfExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        if !command::is_uf_project(|relative| worktree.read_text_file(relative).is_ok()) {
            return Err(command::NOT_A_UF_PROJECT.to_owned());
        }

        let binary = LspSettings::for_worktree(SERVER_ID, worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let (path, configured_arguments, configured_env) = match binary {
            Some(binary) => (binary.path, binary.arguments, binary.env),
            None => (None, None, None),
        };

        let root = worktree.root_path();
        let shell = worktree.shell_env();
        let home = shell
            .iter()
            .find(|(name, _)| name == "HOME" || name == "USERPROFILE")
            .map(|(_, value)| value.clone());
        let (os, _) = zed::current_platform();
        let host = command::Host {
            windows: matches!(os, zed::Os::Windows),
        };

        let program = command::find_binary(
            path.as_deref(),
            &root,
            home.as_deref(),
            host,
            |relative| worktree.read_text_file(relative).is_ok(),
            |name| worktree.which(name),
        )?;

        Ok(zed::Command {
            command: program,
            args: command::arguments(&root, configured_arguments),
            env: command::environment(shell, configured_env.into_iter().flatten()),
        })
    }
}

zed::register_extension!(UfExtension);
