//! Which `uf` to start, and with what — without Zed.
//!
//! Everything here is a plain function over strings and two injected probes,
//! so `cargo test` runs it on the host. `lib.rs` is the thin part that asks
//! Zed for the setting, the worktree and the shell environment, and hands
//! them to these functions.
//!
//! # The order, and why it is that order
//!
//! It is the VS Code extension's order (`editors/vscode/src/binary.js`), for
//! the same reasons:
//!
//! 1. `lsp.uf.binary.path` in Zed's settings, when set. A setting is used as
//!    written and never falls through to another `uf`: someone who points at
//!    a build of uf they are debugging would otherwise silently get the
//!    released one.
//! 2. `<worktree>/node_modules/.bin/uf`, the copy the project pinned. The
//!    version a project installed is the version its files were formatted and
//!    linted with, so a different global one would disagree with CI.
//! 3. `PATH`, as the worktree's shell sees it, for a global install.
//!
//! # What an extension can and cannot see
//!
//! A Zed extension runs in WebAssembly and has no file system access outside
//! its own directory. It cannot ask whether a file exists; it can only ask
//! Zed to read a file of the worktree *as text*. So step 2 is "Zed could read
//! `node_modules/.bin/uf` as text", which is true of the launcher scripts
//! npm, pnpm and yarn write there and false of a native executable copied or
//! linked there. The setting is the way to name such a binary. For the same
//! reason a configured path is not checked at all: if it is wrong, Zed's own
//! error names it when the spawn fails.

/// uf's single configuration surface, and therefore the project marker.
pub const CONFIG_FILE: &str = "uf.config.js";

/// Whether the worktree is a uf project: one with `uf.config.js` at its root.
///
/// This is the check that keeps uf out of every other JavaScript project.
/// Zed's default `language_servers` for JavaScript ends in `"..."`, which
/// means "and every other server registered for the language" — so once this
/// extension is installed, Zed asks it for a command in *every* JavaScript
/// worktree, a TypeScript one included. Answering with `uf lsp` there would
/// report uf's lint rules against code that never opted into them.
/// `readable` answers whether Zed can read a worktree-relative path as text,
/// which is the only question an extension can ask about a file.
pub fn is_uf_project(readable: impl Fn(&str) -> bool) -> bool {
    readable(CONFIG_FILE)
}

/// What Zed is told when a JavaScript worktree is not a uf project.
pub const NOT_A_UF_PROJECT: &str = "uf: this worktree has no uf.config.js at its root, so uf lsp \
     is not started here. Add \"!uf\" to languages.JavaScript.language_servers in your user \
     settings to stop Zed asking.";

/// The machine the language server will run on, as far as a path cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    pub windows: bool,
}

impl Host {
    fn separator(self) -> char {
        if self.windows { '\\' } else { '/' }
    }

    /// The names the binary may have in `node_modules/.bin`, most specific
    /// first. npm writes a `.cmd` shim beside the shell script on Windows.
    fn project_names(self) -> &'static [&'static str] {
        if self.windows {
            &["uf.cmd", "uf.exe", "uf"]
        } else {
            &["uf"]
        }
    }

    fn is_absolute(self, path: &str) -> bool {
        if path.starts_with('/') {
            return true;
        }
        if !self.windows {
            return false;
        }
        let bytes = path.as_bytes();
        path.starts_with('\\')
            || (bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/'))
    }

    fn join(self, base: &str, relative: &str) -> String {
        let separator = self.separator();
        let base = base.trim_end_matches(['/', '\\']);
        let relative = if self.windows {
            relative.replace('/', "\\")
        } else {
            relative.to_owned()
        };
        format!("{base}{separator}{relative}")
    }
}

/// Find `uf` for one worktree.
///
/// `setting` is `lsp.uf.binary.path` as configured, `root` the worktree's
/// root path and `home` the shell's `HOME`. `readable` answers whether Zed can
/// read a worktree-relative path as text; `which` searches the worktree
/// shell's `PATH`. The error is the message Zed shows, and names every place
/// that was looked at.
pub fn find_binary(
    setting: Option<&str>,
    root: &str,
    home: Option<&str>,
    host: Host,
    readable: impl Fn(&str) -> bool,
    which: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    if let Some(setting) = setting.map(str::trim).filter(|setting| !setting.is_empty()) {
        let expanded = match (setting.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => host.join(home, rest),
            _ if setting == "~" => home.unwrap_or(setting).to_owned(),
            _ => setting.to_owned(),
        };
        let command = if host.is_absolute(&expanded) {
            expanded
        } else {
            host.join(root, &expanded)
        };
        return Ok(command);
    }

    let mut tried = Vec::new();
    for name in host.project_names() {
        let relative = format!("node_modules/.bin/{name}");
        let command = host.join(root, &relative);
        if readable(&relative) {
            return Ok(command);
        }
        tried.push(command);
    }

    if let Some(command) = which("uf") {
        return Ok(command);
    }
    tried.push("`uf` on PATH".to_owned());

    Err(format!(
        "uf: no `uf` binary found, so diagnostics, formatting, quick fixes and hover are off. \
         Looked in: {}. Install uf (https://uniflowed.dev/guide/install), or set \
         `lsp.uf.binary.path` in Zed's settings.",
        tried.join(", ")
    ))
}

/// The arguments `uf` is started with.
///
/// `uf lsp` reads `uf.config.js` once, at start-up, from the directory
/// `--cwd` names. Zed starts a language server in the worktree root already;
/// naming it as well means the project is right however Zed starts it, and
/// it is what `ps` shows when someone asks which project a server serves.
/// `lsp.uf.binary.arguments`, when set, replaces all of this: someone who
/// writes arguments is taking the command line over.
pub fn arguments(root: &str, configured: Option<Vec<String>>) -> Vec<String> {
    configured.unwrap_or_else(|| vec!["lsp".to_owned(), "--cwd".to_owned(), root.to_owned()])
}

/// The environment: the worktree shell's, with `lsp.uf.binary.env` on top.
pub fn environment(
    shell: Vec<(String, String)>,
    configured: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    let mut env = shell;
    for (name, value) in configured {
        match env.iter_mut().find(|(existing, _)| *existing == name) {
            Some(entry) => entry.1 = value,
            None => env.push((name, value)),
        }
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIX: Host = Host { windows: false };
    const WINDOWS: Host = Host { windows: true };

    fn nothing(_: &str) -> bool {
        false
    }

    fn nowhere(_: &str) -> Option<String> {
        None
    }

    fn global(_: &str) -> Option<String> {
        Some("/usr/local/bin/uf".to_owned())
    }

    #[test]
    fn the_setting_wins_over_the_project_and_path() {
        let found = find_binary(
            Some("/opt/uf/bin/uf"),
            "/home/dev/app",
            None,
            UNIX,
            |_| true,
            global,
        );
        assert_eq!(found, Ok("/opt/uf/bin/uf".to_owned()));
    }

    #[test]
    fn a_relative_setting_is_relative_to_the_worktree() {
        let found = find_binary(
            Some("target/debug/uf"),
            "/home/dev/app/",
            None,
            UNIX,
            nothing,
            nowhere,
        );
        assert_eq!(found.unwrap(), "/home/dev/app/target/debug/uf");
    }

    #[test]
    fn a_leading_tilde_is_the_shell_home() {
        let found = find_binary(
            Some("~/.local/bin/uf"),
            "/home/dev/app",
            Some("/home/dev"),
            UNIX,
            nothing,
            nowhere,
        );
        assert_eq!(found.unwrap(), "/home/dev/.local/bin/uf");
    }

    #[test]
    fn an_empty_setting_means_search() {
        let found = find_binary(Some("  "), "/home/dev/app", None, UNIX, nothing, global);
        assert_eq!(found.unwrap(), "/usr/local/bin/uf");
    }

    #[test]
    fn the_project_copy_wins_over_path() {
        let found = find_binary(
            None,
            "/home/dev/app",
            None,
            UNIX,
            |relative| relative == "node_modules/.bin/uf",
            global,
        );
        assert_eq!(found, Ok("/home/dev/app/node_modules/.bin/uf".to_owned()));
    }

    #[test]
    fn windows_finds_the_npm_shim() {
        let found = find_binary(
            None,
            "C:\\dev\\app",
            None,
            WINDOWS,
            |relative| relative == "node_modules/.bin/uf.cmd",
            nowhere,
        );
        assert_eq!(found.unwrap(), "C:\\dev\\app\\node_modules\\.bin\\uf.cmd");
    }

    #[test]
    fn a_windows_drive_path_is_absolute() {
        let found = find_binary(
            Some("D:\\tools\\uf.exe"),
            "C:\\dev\\app",
            None,
            WINDOWS,
            nothing,
            nowhere,
        );
        assert_eq!(found.unwrap(), "D:\\tools\\uf.exe");
    }

    #[test]
    fn path_is_the_last_resort() {
        let found = find_binary(None, "/home/dev/app", None, UNIX, nothing, global);
        assert_eq!(found, Ok("/usr/local/bin/uf".to_owned()));
    }

    #[test]
    fn nothing_found_names_every_place_and_the_setting() {
        let error = find_binary(None, "/home/dev/app", None, UNIX, nothing, nowhere).unwrap_err();
        assert!(
            error.contains("/home/dev/app/node_modules/.bin/uf"),
            "{error}"
        );
        assert!(error.contains("`uf` on PATH"), "{error}");
        assert!(error.contains("lsp.uf.binary.path"), "{error}");
    }

    #[test]
    fn the_server_is_told_its_project() {
        assert_eq!(
            arguments("/home/dev/app", None),
            ["lsp", "--cwd", "/home/dev/app"]
        );
    }

    #[test]
    fn configured_arguments_replace_the_default() {
        let configured = vec!["lsp".to_owned()];
        assert_eq!(arguments("/home/dev/app", Some(configured)), ["lsp"]);
    }

    #[test]
    fn configured_environment_overrides_the_shell() {
        let shell = vec![
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            ("HOME".to_owned(), "/home/dev".to_owned()),
        ];
        let configured = [
            ("PATH".to_owned(), "/opt/bin".to_owned()),
            ("RUST_LOG".to_owned(), "debug".to_owned()),
        ];
        assert_eq!(
            environment(shell, configured),
            [
                ("PATH".to_owned(), "/opt/bin".to_owned()),
                ("HOME".to_owned(), "/home/dev".to_owned()),
                ("RUST_LOG".to_owned(), "debug".to_owned()),
            ]
        );
    }

    #[test]
    fn only_a_worktree_with_the_config_is_a_uf_project() {
        assert!(is_uf_project(|path| path == "uf.config.js"));
        assert!(!is_uf_project(|path| path == "package.json"));
        assert!(!is_uf_project(|path| path == "app/uf.config.js"));
        assert!(NOT_A_UF_PROJECT.contains("\"!uf\""), "{NOT_A_UF_PROJECT}");
    }

    /// The id Zed files the settings under is the id `extension.toml`
    /// declares; if the two drift, `lsp.uf` settings silently stop applying.
    #[test]
    fn the_settings_key_is_the_declared_server() {
        let manifest = include_str!("../extension.toml");
        assert!(
            manifest.contains(&format!("[language_servers.{}]", crate::SERVER_ID)),
            "extension.toml does not declare `{}`",
            crate::SERVER_ID
        );
    }

    /// Zed has one language for `.js`, `.jsx`, `.mjs` and `.cjs`: `JavaScript`.
    /// A name Zed does not know attaches the server to nothing.
    #[test]
    fn the_server_is_attached_to_javascript() {
        let manifest = include_str!("../extension.toml");
        let languages = manifest
            .lines()
            .find(|line| line.starts_with("languages"))
            .expect("extension.toml names the server's languages");
        assert_eq!(languages, r#"languages = ["JavaScript"]"#);
    }
}
