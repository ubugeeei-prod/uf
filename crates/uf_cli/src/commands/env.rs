//! `uf env`: which environment is active, and which tools are installed.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_term::{Align, KeyValue, Status, Tone, push_padded, push_spaces};

use crate::cli::EnvCommand;
use crate::support::{command_output, plural, project_label};
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
        EnvCommand::Install => install(cwd, ui),
        EnvCommand::List => list(cwd, ui),
        EnvCommand::Exec { command } => exec(cwd, &command),
        EnvCommand::Gc { dry_run } => gc(ui, dry_run),
    }
}

/// The pins this project declares, and where they would live.
fn declared(cwd: &Utf8Path) -> Result<(uf_config::ResolvedConfig, Vec<uf_env::Pin>)> {
    let resolved = load_config(cwd)?;
    let platform = uf_env::Platform::current().ok_or_else(|| {
        anyhow::anyhow!(
            "uf does not install tools for {} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let pins = uf_env::project::declared(&resolved.config, platform)?;
    Ok((resolved, pins))
}

/// Install everything `uf.config.js` declares, and link it into the project.
fn install(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let (resolved, pins) = declared(cwd)?;
    if pins.is_empty() {
        ui.render(|renderer, out| {
            renderer.banner(out, "uf env install", Some(project_label(&resolved.root)));
            renderer.blank(out);
            renderer.status(
                out,
                Status::Warn,
                "uf.config.js declares no toolchain, so nothing is pinned to this project",
            );
        });
        return Ok(());
    }

    let store = uf_env::Store::discover()?;
    let mut installed = Vec::new();
    for pin in &pins {
        if store.has(pin) {
            installed.push((pin.clone(), false));
            continue;
        }
        let source = uf_env::source::Source::for_pin(pin)
            .with_context(|| format!("uf has no published build of {pin} for this platform"))?;
        let staging = store.staging(pin)?;
        uf_env::archive::install(&source, &staging)?;
        store.adopt(pin, &staging)?;
        installed.push((pin.clone(), true));
    }

    let linked = uf_env::project::link(&resolved.root, &store, &pins)?;
    let entries: Vec<String> = pins.iter().map(uf_env::Pin::slug).collect();
    uf_env::Roots::discover()?.register(&resolved.root, &entries)?;

    let bin = uf_env::project::bin_dir(&resolved.root).to_string();
    let rows: Vec<String> = installed
        .iter()
        .map(|(pin, fetched)| {
            format!(
                "{pin}  {}",
                if *fetched {
                    "installed"
                } else {
                    "in the store"
                }
            )
        })
        .collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let summary = format!("{} linked into {bin}", plural(linked.len(), "executable"));

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env install", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.bullet_list(out, 2, &rows);
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// What this project declares, and what the store holds for everybody.
fn list(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let (resolved, pins) = declared(cwd)?;
    let store = uf_env::Store::discover()?;
    let mine: Vec<String> = pins
        .iter()
        .map(|pin| {
            let mark = if store.has(pin) {
                "installed"
            } else {
                "missing"
            };
            format!("{pin}  {mark}")
        })
        .collect();
    let mine: Vec<&str> = mine.iter().map(String::as_str).collect();
    let entries = store.entries()?;
    let all: Vec<&str> = entries.iter().map(String::as_str).collect();
    let root = store.root().to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.heading(out, 2, "this project");
        if mine.is_empty() {
            renderer.bullet_list(out, 4, &["nothing pinned in uf.config.js"]);
        } else {
            renderer.bullet_list(out, 4, &mine);
        }
        renderer.blank(out);
        renderer.heading(out, 2, "the store");
        renderer.key_values(out, 4, &[KeyValue::toned("at", &root, Tone::Path)]);
        if all.is_empty() {
            renderer.bullet_list(out, 4, &["empty"]);
        } else {
            renderer.bullet_list(out, 4, &all);
        }
    });
    Ok(())
}

/// Run a command with this project's `bin` in front of `PATH`.
///
/// In front of, not instead of: a project that pins Node still needs `git`,
/// `sh` and everything else the command it is running expects to find.
fn exec(cwd: &Utf8Path, command: &[String]) -> Result<()> {
    let (resolved, _) = declared(cwd)?;
    let bin = uf_env::project::bin_dir(&resolved.root);
    if !bin.is_dir() {
        bail!("this project has no environment yet; run `uf env install`");
    }
    let (program, arguments) = command
        .split_first()
        .context("uf env exec needs a command to run")?;
    let path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut parts = vec![bin.clone().into_std_path_buf()];
            parts.extend(std::env::split_paths(&existing));
            std::env::join_paths(parts).context("failed to build PATH")?
        }
        None => bin.clone().into_std_path_buf().into_os_string(),
    };

    let status = std::process::Command::new(program)
        .args(arguments)
        .env("PATH", path)
        .current_dir(&resolved.root)
        .status()
        .with_context(|| format!("failed to run {program}"))?;
    if status.success() {
        return Ok(());
    }
    // The command's own exit status is the answer; wrapping it in uf's would
    // hide a test failure behind "uf exited 1".
    std::process::exit(status.code().unwrap_or(1));
}

/// `1 entry` / `2 entries`, which `plural` cannot produce.
fn entries_word(count: usize) -> String {
    if count == 1 {
        "1 entry".to_owned()
    } else {
        format!("{count} entries")
    }
}

/// Delete store entries no repository is using.
fn gc(ui: &mut Ui, dry_run: bool) -> Result<()> {
    let store = uf_env::Store::discover()?;
    let roots = uf_env::Roots::discover()?;
    let plan = uf_env::gc::plan(&store, &roots)?;

    let going: Vec<&str> = plan.unreachable.iter().map(String::as_str).collect();
    let dead: Vec<String> = plan
        .dead_roots
        .iter()
        .map(|(_, repository)| format!("{repository} (gone)"))
        .collect();
    let dead: Vec<&str> = dead.iter().map(String::as_str).collect();
    let kept = entries_word(plan.kept);

    let summary = if plan.is_empty() {
        format!("nothing to collect; {kept} in use")
    } else if dry_run {
        format!(
            "{} would be removed; {kept} in use",
            entries_word(plan.unreachable.len())
        )
    } else {
        let (entries, roots_removed) = uf_env::gc::collect(&store, &roots, &plan)?;
        format!(
            "removed {}, forgot {}; {kept} in use",
            entries_word(entries),
            plural(roots_removed, "root")
        )
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env gc", None);
        renderer.blank(out);
        if !dead.is_empty() {
            renderer.heading(out, 2, "repositories that are gone");
            renderer.bullet_list(out, 4, &dead);
            renderer.blank(out);
        }
        if !going.is_empty() {
            renderer.heading(out, 2, if dry_run { "would remove" } else { "removed" });
            renderer.bullet_list(out, 4, &going);
            renderer.blank(out);
        }
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
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
