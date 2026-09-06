//! Small helpers shared by more than one command.

use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::ResolvedConfig;
use uf_config::env_files::{self, ProjectEnv};

/// The mode a dev server, a watch, or anything else a person leaves running
/// takes when nothing says otherwise.
pub(crate) const DEVELOPMENT: &str = "development";

/// The mode a build and the servers that serve one take.
pub(crate) const PRODUCTION: &str = "production";

/// The mode `uf test` takes, so `.env.test` is a file that means something.
pub(crate) const TEST: &str = "test";

/// The last path segment, which is what a reader calls the project.
pub(crate) fn project_label(root: &Utf8Path) -> &str {
    root.file_name().unwrap_or_else(|| root.as_str())
}

/// The environment this command runs with: the mode, and the values the
/// project's `.env` files defined for it.
///
/// Every command that runs the project's code calls this, with the mode it
/// defaults to when nothing was typed and no profile is set. See
/// [`uf_config::env_files`] for the cascade, the precedence and the client
/// prefix.
pub(crate) fn project_env(
    resolved: &ResolvedConfig,
    requested_mode: Option<&str>,
    default_mode: &str,
) -> Result<ProjectEnv> {
    let mode = env_files::resolve_mode(
        &resolved.root,
        &resolved.config,
        requested_mode,
        default_mode,
    )?;
    env_files::load(&resolved.root, &resolved.config, &mode)
        .with_context(|| format!("failed to load the environment files for mode {mode}"))
}

/// The environment files a run read, or `None` when it read none.
///
/// Names, never values: this goes on a banner, and a banner ends up in a CI
/// log, a screen share and a bug report.
pub(crate) fn env_file_list(root: &Utf8Path, env: &ProjectEnv) -> Option<String> {
    if env.files().is_empty() {
        return None;
    }
    Some(
        env.files()
            .iter()
            .map(|file| relative_to(root, file))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// A path rendered relative to the project root when it lives inside it.
pub(crate) fn relative_to(root: &Utf8Path, path: &Utf8Path) -> String {
    path.strip_prefix(root).unwrap_or(path).as_str().to_string()
}

/// The files discovery could not read, as `path: reason` lines.
///
/// One stray byte should not stop a project, so discovery skips a file it
/// cannot read rather than failing the walk. Every command that walks says
/// so, in the same words, and fails afterwards — silently skipping a file
/// somebody asked to be formatted or linted is the failure this replaced.
pub(crate) fn unreadable_lines(unreadable: &[uf_project::UnreadableFile]) -> Vec<String> {
    unreadable
        .iter()
        .map(|file| format!("{}: {}", file.relative_path, file.reason))
        .collect()
}

/// Whether `path` is one of the files the patterns asked for.
///
/// Substring rather than glob, and the same rule `uf test` uses for its own
/// path arguments: `uf lint packages/ui` is the ordinary way to ask, and a
/// reader who writes it should not have to learn a second matching language
/// to find out why it read nothing.
pub(crate) fn selects(patterns: &[String], path: &str) -> bool {
    patterns.is_empty()
        || patterns
            .iter()
            .any(|pattern| path.contains(pattern.as_str()))
}

/// `a`, `b` and `c`, quoted, for a message about patterns that matched nothing.
pub(crate) fn quoted_list(patterns: &[String]) -> String {
    let quoted: Vec<String> = patterns
        .iter()
        .map(|pattern| format!("`{pattern}`"))
        .collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Render a count with the right plural, e.g. `1 file` / `3 files`.
pub(crate) fn plural(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// `no problems` / `1 error` / `1 error, 2 warnings`.
pub(crate) fn problem_summary(errors: usize, warnings: usize) -> String {
    match (errors, warnings) {
        (0, 0) => "no problems".to_string(),
        (0, warnings) => plural(warnings, "warning"),
        (errors, 0) => plural(errors, "error"),
        (errors, warnings) => format!(
            "{}, {}",
            plural(errors, "error"),
            plural(warnings, "warning")
        ),
    }
}

/// `enabled` / `disabled`, which reads better than `true` / `false`.
pub(crate) fn enabled(value: bool) -> &'static str {
    if value { "enabled" } else { "disabled" }
}

/// `yes` / `no`.
pub(crate) fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Write pretty JSON plus a trailing newline.
pub(crate) fn write_json_file(path: &Utf8Path, value: &serde_json::Value) -> Result<()> {
    let mut contents = serde_json::to_string_pretty(value)?;
    contents.push('\n');
    std::fs::write(path, contents).with_context(|| format!("failed to write {path}"))
}

/// The first line a tool prints for `--version`, or an error when it is absent.
pub(crate) fn command_output(bin: &str, arg: &str) -> Result<String> {
    let output = ProcessCommand::new(bin).arg(arg).output()?;
    if !output.status.success() {
        bail!("{bin} exited with {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().next().unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_agree_with_their_count() {
        assert_eq!(plural(0, "file"), "0 files");
        assert_eq!(plural(1, "file"), "1 file");
        assert_eq!(plural(2, "file"), "2 files");
    }

    #[test]
    fn problem_summaries_read_as_english() {
        assert_eq!(problem_summary(0, 0), "no problems");
        assert_eq!(problem_summary(1, 0), "1 error");
        assert_eq!(problem_summary(0, 2), "2 warnings");
        assert_eq!(problem_summary(2, 1), "2 errors, 1 warning");
    }

    #[test]
    fn booleans_render_as_words() {
        assert_eq!(enabled(true), "enabled");
        assert_eq!(enabled(false), "disabled");
        assert_eq!(yes_no(true), "yes");
        assert_eq!(yes_no(false), "no");
    }

    #[test]
    fn paths_render_relative_to_the_project_root() {
        let root = Utf8Path::new("/tmp/demo");
        assert_eq!(
            relative_to(root, Utf8Path::new("/tmp/demo/dist/a.json")),
            "dist/a.json"
        );
        assert_eq!(
            relative_to(root, Utf8Path::new("/elsewhere/a.json")),
            "/elsewhere/a.json"
        );
    }

    #[test]
    fn the_project_label_is_the_last_path_segment() {
        assert_eq!(project_label(Utf8Path::new("/tmp/demo-app")), "demo-app");
        assert_eq!(project_label(Utf8Path::new("/")), "/");
    }

    #[test]
    fn a_missing_tool_reports_an_error() {
        assert!(command_output("uf-does-not-exist-anywhere", "--version").is_err());
    }
}
