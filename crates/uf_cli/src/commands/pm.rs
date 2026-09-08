//! `uf install`, `uf add`, `uf remove`, `uf update` and `uf why`: the
//! project's dependencies.
//!
//! The uf binary itself is [`super::toolchain`]. The two used to share this
//! module, and `uf upgrade` — which ran an install and wrote a plan under a
//! name that promised a new uf — is why they no longer do.

mod approve;
mod catalog;
mod deps;
mod install;
mod update;

pub(crate) use approve::approve_builds;
pub(crate) use catalog::{list as catalog, set as catalog_set};
pub(crate) use deps::{add, patch, query, remove, why};
pub(crate) use install::install;
pub(crate) use update::update;

use anyhow::Result;
use camino::Utf8Path;
use uf_pm::PackageManagerPlan;

/// Whether this install may let a dependency's own scripts run.
///
/// `pm.allowLifecycleScripts` is the blunt answer and stays the blunt answer:
/// on means every dependency, including the ones nobody has read. The other way
/// in is [`uf_pm::builds`]: a manager that can be handed an allow-list, with at
/// least one name in it, gets to run exactly those, so `--ignore-scripts` comes
/// off and the manager holds the line uf was holding.
///
/// Both call sites go through here rather than through
/// `PackageManagerPlan::forbids_npm_scripts`, because a rule about running
/// somebody else's code should have one spelling.
///
/// # Errors
///
/// When the root manifest is not JSON — the same failure the install is about
/// to hit anyway.
fn scripts_allowed(
    root: &Utf8Path,
    manager: uf_pm::PackageManager,
    plan: &PackageManagerPlan,
) -> Result<bool> {
    Ok(uf_pm::builds::scripts_allowed(
        root,
        manager,
        !plan.forbids_npm_scripts(),
    )?)
}
