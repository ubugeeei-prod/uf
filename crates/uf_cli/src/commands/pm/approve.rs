//! `uf pm approve-builds`: which dependencies may run code when they install.
//!
//! The model, and why an approval is worth having at all, is in
//! [`uf_pm::builds`]. This is the command over it: what is waiting, what the
//! project has already said, and what this particular manager is able to
//! enforce.
//!
//! # Why it names the hooks
//!
//! Because "approve `sharp`?" is not a question anybody can answer. The thing
//! being approved is code that will run on the machine of everyone who installs
//! this project, and a row that says only the package name is a review in name
//! only. So the table carries which of `preinstall`, `install`, `postinstall`
//! and `prepare` each package declares — `postinstall` alone is a different
//! risk from all four.
//!
//! Not the script *bodies*. A minified `postinstall` is one line of four
//! kilobytes, and printing it would let the package decide how much of your
//! terminal it gets; the file is one `cat` away and reading it there is the
//! review this command is asking you to do.
//!
//! # Why it lives under `uf pm`
//!
//! Because it is not something anybody runs daily. `uf install` and `uf add`
//! are the commands; this is a setting the manager reads that uf has an opinion
//! about, and the top level is for the verbs.
//!
//! # Why it names the provenance
//!
//! Approving a package is the moment its `postinstall` becomes code that runs
//! on this machine, so it is the moment "who built this?" is worth a request.
//! npm publishes a provenance attestation for a package built by a CI workflow
//! with an OIDC identity, and the `attested` column says whether this one has
//! one — and, underneath the table, which repository and workflow it names.
//!
//! `no` is not a reason to refuse. Most of npm publishes none, and a command
//! that refused every unattested package would refuse almost every package with
//! a native binary to build, which is the entire population of this table. It
//! is a fact to read before approving, not a gate. What it *does* catch is the
//! change: a package that was attested last month and is not today.
//!
//! What the column proves and does not prove is [`uf_pm::provenance`]'s to
//! state, and it states it: the attestation is bound to the tarball the
//! registry publishes for that version, and uf does not verify the Sigstore
//! signature itself.
//!
//! # Why it refuses a name it cannot see
//!
//! On a security command a typo that silently does nothing is worse than an
//! error: the reader believes they have approved something, the install keeps
//! ignoring it, and the eventual "why doesn't `sharp` work" is debugged
//! somewhere else entirely. So a name that is not in the tree is refused, with
//! the names that are.

use anyhow::{Result, bail};
use camino::Utf8Path;
use compact_str::CompactString;
use serde_json::{Value, json};
use uf_config::load_config;
use uf_pm::builds::{Approvals, Buildable, approvals_for};
use uf_pm::detect_package_manager;
use uf_pm::installable;
use uf_term::{Cell, Column, KeyValue, Status, Table, Tone};

use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many rows the table prints.
const ROWS_SHOWN: usize = 40;

/// `uf pm approve-builds [NAME...] [--dry-run]`.
///
/// # Errors
///
/// When the project cannot be read, when a named package is not in the tree,
/// when the manager cannot approve one package rather than all of them, or when
/// the root manifest cannot be written.
pub(crate) fn approve_builds(
    cwd: &Utf8Path,
    ui: &mut Ui,
    names: &[String],
    dry_run: bool,
) -> Result<()> {
    uf_pm::check_operands(names)?;
    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();
    let (manager, _) = installable(&detect_package_manager(&root));
    let approvals = approvals_for(manager);
    let already = uf_pm::builds::approved(&root, manager)?;
    let waiting = uf_pm::builds::scan(&root, &already)?;

    if names.is_empty() {
        let project = project_label(&root).to_string();
        let manager_label = manager.to_string();
        let configured = resolved.config.pm.allow_lifecycle_scripts;
        let attested = attestations(&resolved.config, &waiting);
        ui.render(|renderer, out| {
            renderer.banner(out, "uf pm approve-builds", Some(&project));
            renderer.blank(out);
            render(
                renderer,
                out,
                &manager_label,
                approvals,
                configured,
                &waiting,
                &attested,
            );
        });
        return Ok(());
    }

    // Every refusal before the first write: a command that approves two of
    // three names and then fails has left the project in a state nobody asked
    // for, on the one surface where that matters most.
    if approvals == Approvals::AllOrNothing {
        bail!(
            "{manager} cannot approve one package rather than all of them: `--ignore-scripts` is \
             every script or none, and there is no third answer to give it.\n\nuf keeps them off. \
             Turning them all on is `pm.allowLifecycleScripts: true` in uf.config.js, which is a \
             deliberate act with a deliberate spelling — and it approves every dependency you \
             have, including the ones you have not read."
        );
    }
    for name in names {
        if !waiting.iter().any(|package| package.name == name.as_str()) {
            let known = waiting
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>();
            if known.is_empty() {
                bail!(
                    "nothing in this project's tree declares an install script, so there is \
                     nothing to approve — `{name}` included. Has it been installed yet?"
                );
            }
            bail!(
                "`{name}` is not a package in this project's tree that would run code at \
                 install time. These are: {}",
                known.join(", ")
            );
        }
    }

    let named: Vec<CompactString> = names.iter().map(|name| name.as_str().into()).collect();
    let new: Vec<&CompactString> = named
        .iter()
        .filter(|name| !already.contains(*name))
        .collect();
    let manifest = root.join("package.json");
    let project = project_label(&root).to_string();
    let field = approvals.field().unwrap_or("package.json").to_owned();
    let manager_label = manager.to_string();
    let listed: Vec<String> = new.iter().map(|name| (*name).to_string()).collect();
    let nothing = new.is_empty();
    let summary = plural(new.len(), "package");

    ui.render(|renderer, out| {
        renderer.banner(out, "uf pm approve-builds", Some(&project));
        renderer.blank(out);
        if nothing {
            renderer.status(out, Status::Success, "already approved; nothing to write");
            return;
        }
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("manager", &manager_label),
                KeyValue::toned("recorded in", &field, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.bullet_list(
            out,
            2,
            &listed.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        renderer.blank(out);
        if dry_run {
            renderer.status(
                out,
                Status::Info,
                &format!("{summary} would be approved; run without --dry-run"),
            );
        }
    });

    if nothing || dry_run {
        return Ok(());
    }

    let mut all: Vec<CompactString> = already.iter().cloned().collect();
    all.extend(named.iter().cloned());
    all.sort();
    all.dedup();
    match approvals {
        Approvals::OnlyBuilt => {
            uf_pm::manifests::set(&manifest, &["pnpm", "onlyBuiltDependencies"], array(&all))?;
        }
        Approvals::Trusted => {
            uf_pm::manifests::set(&manifest, &["trustedDependencies"], array(&all))?;
        }
        Approvals::DependenciesMeta => {
            for name in &named {
                uf_pm::manifests::set(
                    &manifest,
                    &["dependenciesMeta", name.as_str(), "built"],
                    json!(true),
                )?;
            }
        }
        Approvals::AllOrNothing => unreachable!("refused above"),
    }

    ui.render(|renderer, out| {
        renderer.status(out, Status::Success, &format!("approved {summary}"));
        // Not run for them: approving is a decision, installing is an action,
        // and a security command that reaches straight for the second the
        // moment you make the first is a command that runs the script you were
        // still thinking about.
        renderer.status(out, Status::Info, "uf install runs their builds");
    });
    Ok(())
}

/// What the registry says about who built each of these, in table order.
///
/// One request per package, capped at the number of rows the table draws:
/// asking about a package the reader is not going to see is a request nobody
/// asked for. A registry that does not answer leaves `unknown`, which is the
/// honest word — it is not `no`.
///
/// A failure to *verify* an attestation is not swallowed here either, but it is
/// not fatal to a listing: this command reports, and `uf install` is where a
/// mismatch stops something from happening. The row says `mismatch` and the
/// reader has the name of the package it is about.
fn attestations(config: &uf_config::UniflowedConfig, waiting: &[Buildable]) -> Vec<Attested> {
    if !config.pm.provenance.reads_attestations() {
        return Vec::new();
    }
    let routing = uf_pm::RegistryRouting::from_config(config);
    // No integrity: `node_modules` has no tarball, so the subject is bound to
    // the digest the registry publishes for that version rather than to bytes
    // in this tree. `uf_pm::provenance::Subject` says which of the two a caller
    // is asking, and `uf install` is where the local bytes are bound.
    let asked: Vec<uf_pm::Subject> = waiting
        .iter()
        .take(ROWS_SHOWN)
        .map(|package| uf_pm::Subject {
            name: package.name.clone(),
            version: package.version.clone(),
            integrity: None,
        })
        .collect();
    uf_pm::provenance::read_many(&routing, &asked)
        .into_iter()
        .map(|answer| match answer {
            Ok(uf_pm::Outcome::Attested(provenance)) => Attested::Yes(
                provenance
                    .origin()
                    .map(|origin| origin.to_string())
                    .unwrap_or_default(),
            ),
            Ok(uf_pm::Outcome::Unattested) => Attested::No,
            Ok(uf_pm::Outcome::Unavailable(_)) => Attested::Unknown,
            Err(_) => Attested::Mismatch,
        })
        .collect()
}

/// What one row's `attested` column says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Attested {
    /// An attestation exists and is about this version. The string is the
    /// repository and workflow it names, when it names one.
    Yes(String),
    /// The registry publishes no provenance for this version.
    No,
    /// uf could not ask.
    Unknown,
    /// An attestation exists and is not about this version.
    Mismatch,
}

impl Attested {
    /// The word the column prints.
    const fn label(&self) -> &'static str {
        match self {
            Self::Yes(_) => "yes",
            Self::No => "no",
            Self::Unknown => "unknown",
            Self::Mismatch => "mismatch",
        }
    }
}

fn array(names: &[CompactString]) -> Value {
    Value::Array(
        names
            .iter()
            .map(|name| Value::String(name.to_string()))
            .collect(),
    )
}

/// Draw the listing.
///
/// Split out from the command so a test can render it against
/// [`uf_term::Capabilities::plain`] and read what it says, without a tree to
/// scan or a manager to detect.
fn render(
    renderer: &uf_term::Renderer,
    out: &mut String,
    manager: &str,
    approvals: Approvals,
    configured: bool,
    waiting: &[Buildable],
    attested: &[Attested],
) {
    let field = approvals.field().unwrap_or("nothing: it is all or none");
    renderer.key_values(
        out,
        2,
        &[
            KeyValue::new("manager", manager),
            KeyValue::toned("approvals in", field, Tone::Path),
        ],
    );
    renderer.blank(out);

    if configured {
        // The one case where the table is beside the point: every script runs
        // already, approved or not, and saying anything else here would be a
        // report that flatters the project.
        renderer.status(
            out,
            Status::Warn,
            "pm.allowLifecycleScripts is on, so every dependency below runs its scripts — \
             approved or not",
        );
        renderer.blank(out);
    }

    if waiting.is_empty() {
        renderer.status(
            out,
            Status::Success,
            "nothing in this project's tree runs code at install time",
        );
        return;
    }

    let pending = waiting.iter().filter(|package| !package.approved).count();
    let bodies: Vec<String> = waiting
        .iter()
        .map(|package| package.scripts.join(", "))
        .collect();
    // The provenance column is drawn only when the reads happened. A column of
    // `unknown` on a project that turned them off is a column that says uf did
    // not look, in a place that reads as uf having looked.
    let provenance = !attested.is_empty();
    let mut columns = vec![
        Column::left("package"),
        Column::left("version"),
        Column::left("runs"),
        Column::left("approved"),
    ];
    if provenance {
        columns.push(Column::left("attested"));
    }
    let mut table = Table::new(columns);
    for (index, (package, scripts)) in waiting.iter().take(ROWS_SHOWN).zip(&bodies).enumerate() {
        let mut cells = vec![
            Cell::new(package.name.as_str()),
            Cell::toned(package.version.as_str(), Tone::Number),
            Cell::toned(scripts, Tone::Muted),
            if package.approved {
                Cell::toned("yes", Tone::Good)
            } else {
                Cell::toned("no", Tone::Warn)
            },
        ];
        if provenance {
            let state = attested.get(index).unwrap_or(&Attested::Unknown);
            cells.push(Cell::toned(
                state.label(),
                match state {
                    Attested::Yes(_) => Tone::Good,
                    // `no` is the common case and not a finding; `mismatch` is
                    // the one that has to look different from both.
                    Attested::No | Attested::Unknown => Tone::Muted,
                    Attested::Mismatch => Tone::Warn,
                },
            ));
        }
        table.push(cells);
    }
    renderer.table(out, 2, &table);
    render_origins(renderer, out, waiting, attested);
    if waiting.len() > ROWS_SHOWN {
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!(
                "and {} more; --json prints all of them",
                waiting.len() - ROWS_SHOWN
            ),
        );
    }
    renderer.blank(out);

    if configured {
        return;
    }
    if pending == 0 {
        renderer.status(out, Status::Success, "every one of them is approved");
        return;
    }
    renderer.status(
        out,
        Status::Info,
        &format!(
            "{} would run code at install time and {} not approved, so uf does not let {} run",
            plural(pending, "package"),
            if pending == 1 { "is" } else { "are" },
            if pending == 1 { "it" } else { "them" }
        ),
    );
    if approvals == Approvals::AllOrNothing {
        // The honest sentence, rather than an approval that quietly means
        // "and everything else too".
        renderer.status(
            out,
            Status::Warn,
            &format!(
                "{manager} cannot approve one and not another: `--ignore-scripts` is all of them \
                 or none"
            ),
        );
        renderer.status(
            out,
            Status::Info,
            "pm.allowLifecycleScripts in uf.config.js turns on every one of them, deliberately",
        );
        return;
    }
    renderer.status(
        out,
        Status::Info,
        "uf pm approve-builds <name>... records the ones you have read",
    );
}

/// Which repository each attested package says built it.
///
/// Under the table rather than in it: a repository URL is sixty columns and
/// would push every other value off the screen, and it is the answer to a
/// second question — the column says whether to ask it.
fn render_origins(
    renderer: &uf_term::Renderer,
    out: &mut String,
    waiting: &[Buildable],
    attested: &[Attested],
) {
    let lines: Vec<String> = waiting
        .iter()
        .zip(attested)
        .filter_map(|(package, state)| match state {
            Attested::Yes(origin) if !origin.is_empty() => {
                Some(format!("{} built by {origin}", package.name))
            }
            Attested::Mismatch => Some(format!(
                "{} publishes an attestation that is not about this version",
                package.name
            )),
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return;
    }
    renderer.blank(out);
    renderer.bullet_list(
        out,
        2,
        &lines.iter().map(String::as_str).collect::<Vec<_>>(),
    );
}

#[cfg(test)]
mod tests;
