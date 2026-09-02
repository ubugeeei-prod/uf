//! `uf env`: which environment is active, and which tools are installed.

use std::fs;

use anyhow::{Context, Result};
use camino::Utf8Path;
use uf_term::{Align, Status, push_padded, push_spaces};

use crate::cli::EnvCommand;
use crate::support::command_output;
use crate::ui::Ui;

/// The tools `uf env doctor` looks for, and the flag that reports a version.
const TOOLS: [(&str, &str); 5] = [
    ("rustc", "--version"),
    ("cargo", "--version"),
    ("nix", "--version"),
    ("git", "--version"),
    ("bun", "--version"),
];

pub(crate) fn env(cwd: &Utf8Path, ui: &mut Ui, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::Doctor => doctor(cwd, ui),
        EnvCommand::Use { name } => use_environment(cwd, ui, &name),
    }
}

fn use_environment(cwd: &Utf8Path, ui: &mut Ui, name: &str) -> Result<()> {
    let dir = cwd.join(".uniflowed");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {dir}"))?;
    fs::write(dir.join("env"), format!("{name}\n"))
        .with_context(|| "failed to write .uniflowed/env")?;

    let message = format!("active environment: {name}");
    ui.render(|renderer, out| renderer.status(out, Status::Success, &message));
    Ok(())
}

fn doctor(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let tools = TOOLS
        .into_iter()
        .map(|(name, arg)| (name, command_output(name, arg)))
        .collect::<Vec<_>>();
    let missing = tools.iter().filter(|(_, result)| result.is_err()).count();
    let name_width = TOOLS.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let root = cwd.as_str().to_string();
    let summary = format!(
        "{} of {} tools available",
        tools.len() - missing,
        tools.len()
    );

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env doctor", Some(&root));
        renderer.blank(out);
        let mut line = String::new();
        for (name, result) in &tools {
            let (status, detail) = match result {
                Ok(version) => (Status::Success, version.as_str()),
                Err(_) => (Status::Error, "not found"),
            };
            line.clear();
            push_padded(&mut line, name, name_width + 2, Align::Left);
            line.push_str(detail);
            push_spaces(out, 2);
            renderer.status(out, status, &line);
        }
        renderer.blank(out);
        renderer.status(
            out,
            if missing == 0 {
                Status::Success
            } else {
                Status::Warn
            },
            &summary,
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_the_active_environment_creates_the_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);

        use_environment(root, &mut ui, "staging").unwrap();

        let written = fs::read_to_string(root.join(".uniflowed/env")).unwrap();
        assert_eq!(written, "staging\n");
    }

    #[test]
    fn every_probed_tool_reports_a_version_flag() {
        for (name, arg) in TOOLS {
            assert!(!name.is_empty());
            assert!(arg.starts_with("--"));
        }
    }
}
