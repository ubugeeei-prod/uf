use zed_extension_api as zed;

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
        let command = worktree.which("uf").ok_or_else(|| {
            "uf is not installed. Install it from https://uniflowed.dev/guide/install".to_owned()
        })?;
        Ok(zed::Command {
            command,
            args: vec!["lsp".to_owned()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(UfExtension);
