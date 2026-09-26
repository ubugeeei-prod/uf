//! `uf env`: which environment is active, and which tools are installed.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::env_files::{self, PROFILE_FILE};
use uf_config::{discover_root, load_config};
use uf_env::toolchain::{Declared, Lookup, Publishers, Resolution, Toolchain};
use uf_term::{KeyValue, Status, Tone};

use crate::cli::EnvCommand;
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

mod doctor;

pub(crate) fn env(cwd: &Utf8Path, ui: &mut Ui, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::Doctor { json, verbose } => doctor::doctor(cwd, ui, json, verbose),
        EnvCommand::Use { name } => use_environment(cwd, ui, &name),
        EnvCommand::Install => install(cwd, ui),
        EnvCommand::List => list(cwd, ui),
        EnvCommand::Update => update(cwd, ui),
        EnvCommand::Exec { command } => exec(cwd, &command),
        EnvCommand::Gc { dry_run } => gc(ui, dry_run),
    }
}

/// This project's toolchain, resolved as far as `lookup` allows, and the
/// platform its pins are for.
fn toolchain(
    cwd: &Utf8Path,
    lookup: Lookup,
) -> Result<(uf_config::ResolvedConfig, uf_env::Platform, Toolchain)> {
    let resolved = load_config(cwd)?;
    let platform = uf_env::Platform::current().ok_or_else(|| {
        anyhow::anyhow!(uf_infra::cstr!(
            "uf does not install tools for {} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let toolchain =
        uf_env::toolchain::resolve(&resolved.root, &resolved.config, lookup, &Publishers)?;
    Ok((resolved, platform, toolchain))
}

/// One declared tool on a line: what it is, what it is for, and `state`.
///
/// `node@26 (26.10.0)  runtime, build runtime  installed` — the spec as written
/// first, because that is the text a reader can find in their config, and the
/// release it locked to after it, because that is the text they can find in
/// the store.
fn tool_row(declared: &Declared, state: &str) -> String {
    let release = match &declared.resolution {
        Resolution::Locked { version, .. } | Resolution::Resolved { version, .. } => {
            uf_infra::cstr!(" ({version})").into_string()
        }
        Resolution::OnPath | Resolution::Exact(_) | Resolution::Unlocked { .. } => String::new(),
    };
    uf_infra::cstr!(
        "{}{release}  {}  {state}",
        declared.spec(),
        declared.roles()
    )
    .into_string()
}

/// Install every tool this project declares a version of, lock what had to be
/// resolved, and link the tools into the project.
fn install(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let (resolved, platform, toolchain) = toolchain(cwd, Lookup::Missing)?;
    crate::support::render_deprecations(ui, resolved.config.toolchain_deprecation());
    let pins = toolchain.pins(platform);
    if pins.is_empty() {
        let sentence = if toolchain.tools.is_empty() {
            "neither uf.config.js nor package.json engines declares a tool at a version, so \
             nothing is pinned to this project"
        } else {
            "every tool this project declares is the one on PATH, so there is nothing to \
             install — write a version, such as `node@26`, to pin one"
        };
        ui.render(|renderer, out| {
            renderer.banner(out, "uf env install", Some(project_label(&resolved.root)));
            renderer.status(out, Status::Warn, sentence);
        });
        return Ok(());
    }

    let store = uf_env::Store::discover()?;
    let mut fetched = Vec::new();
    for pin in &pins {
        if uf_env::archive::ensure(&store, pin)
            .with_context(|| uf_infra::cstr!("failed to install {pin}").into_string())?
        {
            fetched.push(pin.clone());
        }
    }

    // Before the links are made, not after: a `.uniflowed` from an older uf
    // holds links into the store that nothing rebuilds, and as a *file* it is
    // what stops `uf env install` outright — ubugeeei-prod/uf#427.
    let migrated = uf_env::project::migrate_legacy_dir(&resolved.root)?;
    let envs = uf_env::project::Envs::discover()?;
    let linked = uf_env::project::link(&resolved.root, &envs, &store, &toolchain.linked(platform))?;
    // Every pin, linked or not: a `build.runtime` that shares its tool with
    // `runtime` is not on `PATH`, and is still this project's to keep.
    let entries: Vec<String> = pins.iter().map(uf_env::Pin::slug).collect();
    uf_env::Roots::discover()?.register(&resolved.root, &entries)?;

    let rows: Vec<String> = toolchain
        .tools
        .iter()
        .map(|declared| {
            let state = match declared.pin(platform) {
                None => "on PATH",
                Some(pin) if fetched.contains(&pin) => "installed",
                Some(_) => "in the store",
            };
            tool_row(declared, state)
        })
        .collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let locked: Vec<String> = toolchain
        .tools
        .iter()
        .filter_map(|declared| match &declared.resolution {
            Resolution::Resolved { version, .. } => {
                Some(uf_infra::cstr!("{} at {version}", declared.spec()).into_string())
            }
            _ => None,
        })
        .collect();
    let lock_note = (!locked.is_empty()).then(|| {
        uf_infra::cstr!(
            "locked {} in {}",
            locked.join(", "),
            relative_to(&resolved.root, &toolchain.lock_path)
        )
        .into_string()
    });
    let bin = envs.bin_dir(&resolved.root).to_string();
    let summary =
        uf_infra::cstr!("{} linked into {bin}", plural(linked.len(), "executable")).into_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env install", Some(project_label(&resolved.root)));
        renderer.bullet_list(out, 2, &rows);
        renderer.blank(out);
        // Said out loud, because uf removed a directory from somebody's
        // project. It was uf's own, and a command that deletes should still
        // name what it deleted.
        if migrated {
            renderer.status(
                out,
                Status::Info,
                "removed `.uniflowed/`, which an older uf wrote; the profile in it, if there \
                 was one, is `.uf/profile` now",
            );
        }
        if let Some(note) = &lock_note {
            renderer.status(out, Status::Info, note);
        }
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// Each tool this project declares and what it is for, and what the store
/// holds for everybody.
///
/// Reads `uf.lock` and nothing else — no release list is fetched and nothing
/// is locked — so a prefix nothing has resolved yet says so, and names the
/// command that resolves it.
fn list(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let (resolved, platform, toolchain) = toolchain(cwd, Lookup::LockOnly)?;
    crate::support::render_deprecations(ui, resolved.config.toolchain_deprecation());
    let store = uf_env::Store::discover()?;
    let mine: Vec<String> = toolchain
        .tools
        .iter()
        .map(|declared| {
            let state = match (&declared.resolution, declared.pin(platform)) {
                (_, Some(pin)) if store.has(&pin) => "installed",
                (_, Some(_)) => "missing",
                (Resolution::Unlocked { .. }, None) => {
                    "not locked yet — `uf env install` resolves it"
                }
                (_, None) => "on PATH",
            };
            tool_row(declared, state)
        })
        .collect();
    let mine: Vec<&str> = mine.iter().map(String::as_str).collect();
    let entries = store.entries()?;
    let all: Vec<&str> = entries.iter().map(String::as_str).collect();
    let root = store.root().to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env", Some(project_label(&resolved.root)));
        renderer.heading(out, 2, "this project");
        if mine.is_empty() {
            renderer.bullet_list(
                out,
                4,
                &["nothing declared in uf.config.js or package.json engines"],
            );
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

/// Move every version prefix to the newest release its publisher lists now.
///
/// Installs nothing. What moved is a line in `uf.lock` a reader can review
/// before anything is downloaded, and `uf env install` is the step that
/// downloads it.
fn update(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    // Not through `toolchain`, which refuses a platform uf installs nothing
    // for. `uf.lock` is the same file on every machine, and a checkout where uf
    // cannot install a tool can still move the release a prefix locks.
    let resolved = load_config(cwd)?;
    let toolchain = uf_env::toolchain::resolve(
        &resolved.root,
        &resolved.config,
        Lookup::Latest,
        &Publishers,
    )?;
    let rows: Vec<String> = toolchain
        .tools
        .iter()
        .filter_map(|declared| match &declared.resolution {
            Resolution::Resolved {
                version,
                was: Some(was),
                ..
            } => Some(uf_infra::cstr!("{}  {was} → {version}", declared.spec()).into_string()),
            Resolution::Resolved {
                version, was: None, ..
            } => Some(uf_infra::cstr!("{}  locked at {version}", declared.spec()).into_string()),
            Resolution::Locked { version, .. } => Some(
                uf_infra::cstr!("{}  {version}, already the newest", declared.spec()).into_string(),
            ),
            Resolution::OnPath | Resolution::Exact(_) | Resolution::Unlocked { .. } => None,
        })
        .collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let lock = relative_to(&resolved.root, &toolchain.lock_path);
    let summary = if rows.is_empty() {
        "no tool this project declares is written as a version prefix, so there is nothing to \
         move"
            .to_owned()
    } else if toolchain.lock_changed {
        uf_infra::cstr!("{lock} updated; `uf env install` installs what moved").into_string()
    } else {
        uf_infra::cstr!("{lock} already locks the newest release of every prefix").into_string()
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env update", Some(project_label(&resolved.root)));
        if !rows.is_empty() {
            renderer.bullet_list(out, 2, &rows);
            renderer.blank(out);
        }
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

/// Run a command with this project's `bin` in front of `PATH`.
///
/// In front of, not instead of: a project that pins Node still needs `git`,
/// `sh` and everything else the command it is running expects to find.
fn exec(cwd: &Utf8Path, command: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let bin = uf_env::project::Envs::discover()?.bin_dir(&resolved.root);
    if !bin.is_dir() {
        bail!(uf_infra::cstr!(
            "this project has no environment yet; run `uf env install`"
        ));
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
        .with_context(|| uf_infra::cstr!("failed to run {program}").into_string())?;
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
        uf_infra::cstr!("{count} entries").into_string()
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
        .map(|(_, repository)| uf_infra::cstr!("{repository} (gone)").into_string())
        .collect();
    let dead: Vec<&str> = dead.iter().map(String::as_str).collect();
    let kept = entries_word(plan.kept);

    let summary = if plan.is_empty() {
        uf_infra::cstr!("nothing to collect; {kept} in use").into_string()
    } else if dry_run {
        uf_infra::cstr!(
            "{} would be removed; {kept} in use",
            entries_word(plan.unreachable.len())
        )
        .into_string()
    } else {
        let (entries, roots_removed) = uf_env::gc::collect(&store, &roots, &plan)?;
        uf_infra::cstr!(
            "removed {}, forgot {}; {kept} in use",
            entries_word(entries),
            plural(roots_removed, "root")
        )
        .into_string()
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env gc", None);
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

/// Record the profile every later command reads as its mode.
///
/// `.uniflowed/profile`, not `.uniflowed/env`. That path is the directory `uf
/// env install` links this project's toolchain into, so the two halves of `uf
/// env` used to claim one name: whichever ran second failed with `Is a
/// directory (os error 21)` or left a file where `bin/` had to go. See
/// ubugeeei-prod/uf#259.
///
/// The name is checked before anything is written, because it becomes the end
/// of a file name — `.env.<profile>` — and a profile that could climb out of
/// the project would be a profile that reads somebody else's file.
fn use_environment(cwd: &Utf8Path, ui: &mut Ui, name: &str) -> Result<()> {
    // The project root rather than the working directory: `uf dev` reads this
    // from the root, so `uf env use` run one directory down has to write it
    // there or the two would disagree about a file with one name.
    let root = discover_root(cwd);
    env_files::check_mode(name)?;

    let path = root.join(PROFILE_FILE);
    let dir = path.parent().unwrap_or(&root).to_path_buf();
    fs::create_dir_all(&dir)
        .with_context(|| uf_infra::cstr!("failed to create {dir}").into_string())?;
    fs::write(&path, uf_infra::cstr!("{name}\n").into_string())
        .with_context(|| uf_infra::cstr!("failed to write {path}").into_string())?;

    // What it selects, not only what was recorded: "active environment:
    // staging" was true of a file nothing read, and the reader's next question
    // is which files this now means.
    let message = uf_infra::cstr!(
        "mode {name}: `.env`, `.env.local`, `.env.{name}` and `.env.{name}.local`, in {}",
        env_files::PROFILE_FILE
    )
    .into_string();
    ui.render(|renderer, out| renderer.status(out, Status::Success, &message));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project root inside `dir`.
    ///
    /// The marker file matters: root discovery walks up until it finds one, and
    /// a temporary directory with nothing in it would resolve to whatever
    /// happens to be above the system temp directory.
    fn project(dir: &tempfile::TempDir) -> camino::Utf8PathBuf {
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::write(root.join("package.json"), "{}\n").unwrap();
        root
    }

    #[test]
    fn writing_the_active_environment_creates_the_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(&dir);
        let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);

        use_environment(&root, &mut ui, "staging").unwrap();

        let written = fs::read_to_string(root.join(PROFILE_FILE)).unwrap();
        assert_eq!(written, "staging\n");
    }

    /// The two halves of `uf env` cannot collide, because only one of them
    /// writes into the project now.
    ///
    /// They used to claim the same path: `uf env install` linked the
    /// toolchain into `.uniflowed/env/`, and `uf env use` wrote a *file* at
    /// `.uniflowed/env`. Whichever ran second lost. The links are beside the
    /// store now, so the profile is the only thing `uf env` puts in a project
    /// — a guard replaced by there being nothing to guard against.
    #[test]
    fn the_profile_is_the_only_thing_uf_env_writes_into_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(&dir);
        let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);

        use_environment(&root, &mut ui, "staging").unwrap();

        assert_eq!(
            fs::read_to_string(root.join(PROFILE_FILE)).unwrap(),
            "staging\n"
        );
        // And the links are somewhere the project is not: wherever `Envs` is
        // rooted, a project's directory is under *that* rather than under the
        // project.
        let envs = uf_env::project::Envs::new(dir.path().join("envs").to_str().unwrap());
        assert!(envs.bin_dir(&root).starts_with(envs.root()));
        assert!(!root.join(".uniflowed").exists());
    }

    /// A profile an older uf wrote is kept, and the directory it was in goes.
    #[test]
    fn a_profile_from_the_old_location_is_read_until_it_is_moved() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(&dir);
        fs::create_dir_all(root.join(".uniflowed")).unwrap();
        fs::write(root.join(".uniflowed/profile"), "review\n").unwrap();

        // Read before anything moves it, so a project that has not run
        // `uf env install` since upgrading is not silently on the default.
        assert_eq!(
            uf_config::env_files::active_profile(&root)
                .unwrap()
                .as_deref(),
            Some("review")
        );

        assert!(uf_env::project::migrate_legacy_dir(&root).unwrap());
        assert_eq!(
            fs::read_to_string(root.join(PROFILE_FILE)).unwrap(),
            "review\n"
        );
        assert!(!root.join(".uniflowed").exists());
    }

    /// A profile becomes a file name, so a name that could climb out of the
    /// project is refused before anything is written.
    #[test]
    fn a_profile_that_could_not_be_a_file_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(&dir);
        let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);

        let error = use_environment(&root, &mut ui, "../../etc").unwrap_err();

        assert!(error.to_string().contains("is not a mode"), "{error}");
        assert!(!root.join(PROFILE_FILE).exists());
    }
}
