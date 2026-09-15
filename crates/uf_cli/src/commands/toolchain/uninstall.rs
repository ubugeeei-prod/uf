//! `uf self-uninstall`: remove uf from this machine.
//!
//! # Why not `uf uninstall`
//!
//! `uf uninstall` is `uf remove`, spelled the way npm and pnpm spell removing a
//! dependency, and a person who types it inside a project means exactly that.
//! A command that deletes every runtime and the toolchain store cannot share a
//! name with one that edits `package.json`. The `self-` prefix is
//! `uf self-update`'s, and `rustup self uninstall`'s; Vite+'s `implode` names
//! nothing a newcomer could guess.
//!
//! # What goes, and what stays
//!
//! | goes | where |
//! | --- | --- |
//! | the links | `uf`, `ufr` and `ufx` in `UF_BIN_DIR`, when they point into the runtime store |
//! | the runtimes | `<root>/runtimes`, and the rollback record beside it |
//! | the toolchain store | what `uf env install` installed, the project environments linked to it, and the roots that keep it |
//! | the release indexes | `uf env`'s cache of which releases exist |
//! | the active runtime record | `active-runtime.json` |
//!
//! What stays is what uf did not put there, or cannot tell is its own: a `uf`
//! in the link directory that is not a link into the store — Nix's, cargo's, a
//! copy — the link directory itself, which other tools share, every project's
//! `.uf` and `uf.lock`, and any line in a shell profile, which the installer
//! prints for the reader to add and never writes.
//!
//! # Order
//!
//! Links first. A process killed half way then leaves a `uf` that is not on
//! `PATH`, rather than one that is and points at a runtime already deleted —
//! the rule a switch follows, from the other side.
//!
//! # Asking
//!
//! It prints the list and then asks, because nothing it removes comes back
//! without the network. `--yes` answers for a script and `--dry-run` stops after
//! the list. With no terminal to ask on and neither flag, it removes nothing and
//! names the flag: a pipeline that deleted a toolchain because its stdin
//! happened to be empty would be the worst way to find this out.

use std::fs;
use std::io::{self, BufRead as _, IsTerminal as _, Write as _};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_bundle::size::ByteSize;
use uf_term::{Cell, Column, Status, Table, Tone};

use super::{BINARIES, Store};
use crate::support::plural;
use crate::ui::Ui;

/// One thing `uf self-uninstall` removes.
#[derive(Debug)]
struct Removal {
    /// What it is, in the reader's words.
    what: &'static str,
    path: Utf8PathBuf,
    kind: Kind,
    bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Link,
    File,
    Directory,
}

/// The list, before anything is removed.
#[derive(Debug, Default)]
struct Plan {
    removals: Vec<Removal>,
    /// What looks like uf's and is not, each with the reason it stays.
    left_alone: Vec<String>,
}

/// `uf self-uninstall [--dry-run] [--yes]`.
pub(crate) fn self_uninstall(ui: &mut Ui, dry_run: bool, yes: bool) -> Result<()> {
    let store = Store::from_process();
    let plan = plan(&store)?;

    if plan.removals.is_empty() {
        let left_alone = plan
            .left_alone
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        ui.render(|renderer, out| {
            renderer.banner(out, "uf self-uninstall", None);
            renderer.blank(out);
            if !left_alone.is_empty() {
                renderer.heading(out, 2, "left alone");
                renderer.bullet_list(out, 4, &left_alone);
                renderer.blank(out);
            }
            renderer.status(
                out,
                Status::Success,
                "uf is not installed here; nothing to remove",
            );
        });
        return Ok(());
    }

    let total = plan
        .removals
        .iter()
        .map(|removal| removal.bytes)
        .sum::<u64>();
    let summary = format!(
        "{}, {}",
        plural(plan.removals.len(), "item"),
        ByteSize::from_bytes(total)
    );
    render_plan(ui, &plan);

    if dry_run {
        ui.render(|renderer, out| {
            renderer.status(
                out,
                Status::Info,
                &format!("{summary} would be removed; run without --dry-run to remove them"),
            );
        });
        return Ok(());
    }
    if !yes {
        match confirm("Remove uf from this machine?") {
            Some(true) => {}
            Some(false) => {
                ui.render(|renderer, out| {
                    renderer.status(out, Status::Skip, "nothing removed");
                });
                return Ok(());
            }
            None => bail!(
                "uf self-uninstall removes everything listed above, and there is no terminal to \
                 ask on\n\n  pass --yes to remove it, or --dry-run to only list it"
            ),
        }
    }

    remove(&store, &plan.removals)?;

    let notes = [
        format!(
            "{} stays, and so does any line that puts it on PATH",
            store.bin_dir
        ),
        "every project keeps its .uf directory and uf.lock".to_owned(),
        "curl -fsSL https://setup.uniflowed.dev | sh installs uf again".to_owned(),
    ];
    let notes = notes.iter().map(String::as_str).collect::<Vec<_>>();
    ui.render(|renderer, out| {
        renderer.status(out, Status::Success, &format!("removed {summary}"));
        renderer.blank(out);
        renderer.bullet_list(out, 2, &notes);
    });
    Ok(())
}

/// Everything that would be removed, in the order it would go.
fn plan(store: &Store) -> Result<Plan> {
    let mut plan = Plan::default();

    // The links, first, and only the ones that lead into the store.
    for name in BINARIES {
        let path = store.bin_dir.join(name);
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        match fs::read_link(&path) {
            Ok(target) if target.starts_with(&store.runtimes) => plan.removals.push(Removal {
                what: "link",
                path,
                kind: Kind::Link,
                bytes: 0,
            }),
            Ok(target) => plan.left_alone.push(format!(
                "{path}, a link to {}, which is not in uf's runtime store",
                target.display()
            )),
            Err(_) => plan
                .left_alone
                .push(format!("{path}, which is not a link uf made")),
        }
    }
    // What a killed switch left beside them.
    for leftover in leftovers(&store.bin_dir, |name| BINARIES.contains(&name)) {
        plan.removals.push(Removal {
            what: "leftover link",
            path: leftover,
            kind: Kind::Link,
            bytes: 0,
        });
    }

    let mut push = |what: &'static str, path: Utf8PathBuf| {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return;
        };
        let (kind, bytes) = if metadata.is_dir() {
            (Kind::Directory, size_of(&path))
        } else {
            (Kind::File, metadata.len())
        };
        plan.removals.push(Removal {
            what,
            path,
            kind,
            bytes,
        });
    };
    push(
        "active runtime record",
        store.state_dir.join("active-runtime.json"),
    );
    for leftover in leftovers(&store.state_dir, |name| name == "active-runtime.json") {
        push("leftover record", leftover);
    }
    push("runtimes", store.runtimes.clone());
    push("rollback record", store.previous_record());
    for leftover in leftovers(store.root(), |name| name == "previous-version") {
        push("leftover record", leftover);
    }
    if let Ok(toolchains) = uf_env::Store::discover() {
        push("toolchain store", toolchains.root().to_owned());
    }
    if let Ok(environments) = uf_env::project::Envs::discover() {
        push("project environments", environments.root().to_owned());
    }
    if let Ok(roots) = uf_env::Roots::discover() {
        push("toolchain store roots", roots.path().to_owned());
    }
    if let Ok(indexes) = uf_env::index::cache_dir() {
        push("release indexes", indexes);
    }

    // A variable can put one of these inside another, and removing a directory
    // removes what is in it: listing both would count it twice.
    let directories = plan
        .removals
        .iter()
        .filter(|removal| removal.kind == Kind::Directory)
        .map(|removal| removal.path.clone())
        .collect::<Vec<_>>();
    plan.removals.retain(|removal| {
        !directories
            .iter()
            .any(|directory| removal.path != *directory && removal.path.starts_with(directory))
    });
    let mut seen = Vec::new();
    plan.removals.retain(|removal| {
        let first = !seen.contains(&removal.path);
        seen.push(removal.path.clone());
        first
    });

    refuse_what_holds_home(&plan.removals)?;
    Ok(plan)
}

/// Stop before a variable points this at somewhere far larger than uf.
///
/// `UF_STORE=$HOME` is one typo in a shell profile. Every path here comes from
/// a variable a person can set, and a command that deletes must not be one of
/// them away from deleting a home directory.
fn refuse_what_holds_home(removals: &[Removal]) -> Result<()> {
    let home = std::env::var("HOME").ok().filter(|home| !home.is_empty());
    for removal in removals {
        if removal.path.is_relative() || removal.path.parent().is_none() {
            bail!(
                "uf self-uninstall will not remove {}: it is not a path inside a directory",
                removal.path
            );
        }
        if let Some(home) = &home
            && Utf8Path::new(home).starts_with(&removal.path)
        {
            bail!(
                "uf self-uninstall will not remove {}, which holds your home directory\n\n  \
                 check UF_INSTALL_ROOT, UF_BIN_DIR, UF_STORE, UF_ENVS, UF_ROOTS and \
                 UF_INDEX_CACHE",
                removal.path
            );
        }
    }
    Ok(())
}

/// The `.<name>.incoming.<pid>` entries in `dir` whose name `wanted` accepts:
/// what a killed switch or install left.
fn leftovers(dir: &Utf8Path, wanted: impl Fn(&str) -> bool) -> Vec<Utf8PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|entry| {
            entry
                .strip_prefix('.')
                .and_then(|rest| rest.split_once(".incoming."))
                .is_some_and(|(name, pid)| {
                    wanted(name) && !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
        .map(|entry| dir.join(entry))
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// The bytes under `path`, without following a link out of it.
fn size_of(path: &Utf8Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_type().is_dir())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

fn render_plan(ui: &mut Ui, plan: &Plan) {
    // Owned first, borrowed in the render: `Cell` borrows its text.
    let cells = plan
        .removals
        .iter()
        .map(|removal| {
            let size = match removal.kind {
                Kind::Link => String::new(),
                Kind::File | Kind::Directory => ByteSize::from_bytes(removal.bytes).to_string(),
            };
            (removal.path.to_string(), size)
        })
        .collect::<Vec<_>>();
    let rows = cells
        .iter()
        .zip(&plan.removals)
        .map(|((path, size), removal)| {
            vec![
                Cell::new(removal.what),
                Cell::toned(path, Tone::Path),
                Cell::toned(size, Tone::Number),
            ]
        })
        .collect::<Vec<_>>();
    let left_alone = plan
        .left_alone
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf self-uninstall", None);
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("removes"),
            Column::left("path"),
            Column::right("size"),
        ]);
        for row in &rows {
            table.push(row.clone());
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        if !left_alone.is_empty() {
            renderer.heading(out, 2, "left alone");
            renderer.bullet_list(out, 4, &left_alone);
            renderer.blank(out);
        }
    });
}

/// Remove everything on the list, in its order, then the directories uf made
/// to hold it once they are empty.
fn remove(store: &Store, removals: &[Removal]) -> Result<()> {
    for removal in removals {
        let removed = match removal.kind {
            Kind::Link | Kind::File => fs::remove_file(&removal.path),
            Kind::Directory => fs::remove_dir_all(&removal.path),
        };
        match removed {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to remove {}", removal.path));
            }
        }
    }

    // One level up, only when empty, and only a directory that is uf's by
    // name — `uf` or `uniflowed` — or the install root itself. Never the link
    // directory, which other tools share.
    let mut parents = removals
        .iter()
        .filter(|removal| removal.kind != Kind::Link)
        .filter_map(|removal| removal.path.parent().map(ToOwned::to_owned))
        .filter(|parent| {
            parent.as_path() == store.root()
                || matches!(parent.file_name(), Some("uf" | "uniflowed"))
        })
        .collect::<Vec<_>>();
    parents.sort();
    parents.dedup();
    for parent in parents {
        let _ = fs::remove_dir(&parent);
    }
    Ok(())
}

/// Ask a yes-or-no question on the terminal, when there is one.
///
/// `None` when there is nobody to ask — either stream redirected, `CI` set, or
/// a `dumb` terminal — which are the rules `uf_term::prompt` keeps for its
/// menu.
fn confirm(question: &str) -> Option<bool> {
    let interactive = io::stdin().is_terminal()
        && io::stderr().is_terminal()
        && std::env::var_os("CI").is_none()
        && !matches!(std::env::var("TERM").as_deref(), Ok("dumb"));
    if !interactive {
        return None;
    }
    let mut stderr = io::stderr();
    write!(stderr, "{question} [y/N] ").ok()?;
    stderr.flush().ok()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).ok()?;
    Some(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
