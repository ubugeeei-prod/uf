//! Which dependencies may run code at install time.
//!
//! # The problem, and why uf's default is what it is
//!
//! A dependency with a `postinstall` script is arbitrary code, executed on the
//! machine of everyone who installs it, before anybody has read a line of it.
//! It is the shortest path from a compromised package to a compromised laptop,
//! and it is the one every supply-chain incident of the last decade has used.
//!
//! uf's answer is `--ignore-scripts` on every install, by default, under every
//! manager. That is safe and it is also not free: `esbuild`, `sharp` and every
//! other package with a native binary to place will not work until their build
//! runs. So there has to be a way to say "this one, and no others".
//!
//! # Where the approved set lives
//!
//! In the root `package.json`, in the field the project's own package manager
//! already reads:
//!
//! | manager | field |
//! | --- | --- |
//! | pnpm | `pnpm.onlyBuiltDependencies` |
//! | bun | `trustedDependencies` |
//! | yarn 2+ | `dependenciesMeta.<name>.built` |
//! | npm, yarn 1 | — |
//!
//! Not a file of uf's own, and not a mirror of anybody's config schema. This is
//! uf configuring the tool it drives, which is what it does everywhere else: uf
//! records the decision, and the manager is the thing that enforces it. A
//! project that later stops using uf keeps a working allow-list, and one that
//! already had one is read rather than overridden.
//!
//! # npm, which cannot
//!
//! npm has no per-package control. `--ignore-scripts` is all of them or none of
//! them, and there is no third answer to give it. So on npm and Yarn 1 uf keeps
//! scripts off and says so, rather than presenting an approval that quietly
//! means "and everything else too". Turning them all on is a deliberate act
//! with a deliberate spelling — `pm.allowLifecycleScripts` in `uf.config.js` —
//! and `uf pm approve-builds` will not do it for you.
//!
//! # What the install then does
//!
//! `--ignore-scripts` is dropped only when the manager can enforce the
//! allow-list itself *and* the project has recorded at least one entry. An
//! empty list under pnpm still means no scripts, because an empty
//! `onlyBuiltDependencies` is pnpm's way of saying exactly that; dropping the
//! flag for it would turn "nothing is approved" into "the manager's default",
//! which on a manager that changes its default is a silent reopening of the
//! hole this module exists to close.

use std::collections::BTreeSet;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde_json::Value;

use crate::detect::PackageManager;
use crate::{PackageManagerError, is_polluting_json_key};

/// The manifest scripts npm and its relatives run around an install.
///
/// `prepare` is here because it runs on `npm install` in the package's own
/// directory, which for a git dependency is somebody else's repository.
pub const LIFECYCLE_SCRIPTS: [&str; 4] = ["preinstall", "install", "postinstall", "prepare"];

/// How many installed packages uf will look at.
///
/// A tree larger than this is not a tree a person is going to review one row at
/// a time, and walking it further only makes the command slower at the moment
/// it has already found more than anybody will read.
pub const MAX_SCANNED_PACKAGES: usize = 20_000;

/// Where a manager records which packages may build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approvals {
    /// `pnpm.onlyBuiltDependencies`: an array of names.
    OnlyBuilt,
    /// `trustedDependencies`: an array of names, bun's own field.
    Trusted,
    /// `dependenciesMeta.<name>.built`: a boolean per package, Yarn 2+.
    DependenciesMeta,
    /// Nothing: npm and Yarn 1 are all of them or none of them.
    AllOrNothing,
}

impl Approvals {
    /// Whether this manager can be told about one package rather than all.
    #[must_use]
    pub const fn is_per_package(self) -> bool {
        !matches!(self, Self::AllOrNothing)
    }

    /// The manifest field, as a reader would grep for it.
    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::OnlyBuilt => Some("pnpm.onlyBuiltDependencies"),
            Self::Trusted => Some("trustedDependencies"),
            Self::DependenciesMeta => Some("dependenciesMeta"),
            Self::AllOrNothing => None,
        }
    }
}

/// Which field this manager reads.
#[must_use]
pub const fn approvals_for(manager: PackageManager) -> Approvals {
    match manager {
        PackageManager::Pnpm => Approvals::OnlyBuilt,
        PackageManager::Bun => Approvals::Trusted,
        PackageManager::Yarn(crate::detect::YarnEdition::Berry) => Approvals::DependenciesMeta,
        // And uf's own resolver, which fetches nothing yet and so runs
        // nothing: it has no field to record an approval in because it has no
        // install script to approve.
        PackageManager::Npm
        | PackageManager::Uf
        | PackageManager::Yarn(crate::detect::YarnEdition::Classic) => Approvals::AllOrNothing,
    }
}

/// One installed package that would run code at install time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buildable {
    /// The package's own name, from its manifest rather than from its path.
    pub name: CompactString,
    /// Its version, or `0.0.0` when the manifest omits one.
    pub version: CompactString,
    /// Which of [`LIFECYCLE_SCRIPTS`] it declares, in that order.
    pub scripts: Vec<&'static str>,
    /// Whether the project has already approved it.
    pub approved: bool,
}

/// Every installed package that declares a lifecycle script.
///
/// From `node_modules`, because that is where the answer is: a manifest's
/// dependency list says what was asked for, and the tree says what arrived,
/// including the transitive package nobody chose. Nested `node_modules` is not
/// walked — the hoisted copy is the one that installs, which is the same
/// simplification `uf check` makes and documents in ubugeeei-prod/uf#486.
///
/// Sorted by name, so two runs of the command read the same.
///
/// # Errors
///
/// When `node_modules` exists but cannot be read. A project that has not
/// installed yet is not an error: it has nothing in its tree, which is exactly
/// what an empty answer says.
pub fn scan(
    root: &Utf8Path,
    approved: &BTreeSet<CompactString>,
) -> Result<Vec<Buildable>, PackageManagerError> {
    let modules = root.join("node_modules");
    if !modules.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    let mut seen = 0;
    for entry in read_dir(&modules)? {
        let name = entry.file_name().unwrap_or_default();
        // `.bin`, `.package-lock.json`, `.pnpm` and friends are the manager's
        // own bookkeeping, not packages.
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            for scoped in read_dir(&entry)? {
                visit(&scoped, approved, &mut found, &mut seen)?;
            }
        } else {
            visit(&entry, approved, &mut found, &mut seen)?;
        }
        if seen >= MAX_SCANNED_PACKAGES {
            break;
        }
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

fn visit(
    directory: &Utf8Path,
    approved: &BTreeSet<CompactString>,
    found: &mut Vec<Buildable>,
    seen: &mut usize,
) -> Result<(), PackageManagerError> {
    if *seen >= MAX_SCANNED_PACKAGES {
        return Ok(());
    }
    *seen += 1;
    let manifest = directory.join("package.json");
    let Ok(source) = fs::read_to_string(&manifest) else {
        return Ok(());
    };
    // A manifest that does not parse is a package that will not install, and
    // the manager's own error about it is better than one uf could write.
    let Ok(value) = serde_json::from_str::<Value>(&source) else {
        return Ok(());
    };
    let scripts: Vec<&'static str> = LIFECYCLE_SCRIPTS
        .into_iter()
        .filter(|script| {
            value
                .get("scripts")
                .and_then(|scripts| scripts.get(*script))
                .and_then(Value::as_str)
                .is_some_and(|body| !body.trim().is_empty())
        })
        .collect();
    if scripts.is_empty() {
        return Ok(());
    }
    // From the manifest rather than from the path: a directory can be renamed,
    // and the name the manager approves is the one the package claims.
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| directory.file_name().unwrap_or_default())
        .to_compact_string();
    if is_polluting_json_key(&name) {
        return Ok(());
    }
    found.push(Buildable {
        approved: approved.contains(&name),
        name,
        version: value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("0.0.0")
            .to_compact_string(),
        scripts,
    });
    Ok(())
}

fn read_dir(directory: &Utf8Path) -> Result<Vec<Utf8PathBuf>, PackageManagerError> {
    let entries = fs::read_dir(directory).map_err(|source| PackageManagerError::Read {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| PackageManagerError::Read {
            path: directory.to_path_buf(),
            source,
        })?;
        if let Ok(path) = Utf8PathBuf::from_path_buf(entry.path())
            && path.is_dir()
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// What the root manifest already approves, in whichever field this manager
/// reads.
///
/// An unreadable or absent manifest approves nothing, which is the safe answer
/// and the true one.
///
/// # Errors
///
/// When the root manifest exists but is not JSON — the same failure an install
/// would hit a moment later.
pub fn approved(
    root: &Utf8Path,
    manager: PackageManager,
) -> Result<BTreeSet<CompactString>, PackageManagerError> {
    let manifest = root.join("package.json");
    let Ok(source) = fs::read_to_string(&manifest) else {
        return Ok(BTreeSet::new());
    };
    let value =
        serde_json::from_str::<Value>(&source).map_err(|source| PackageManagerError::Parse {
            path: manifest,
            source,
        })?;
    Ok(match approvals_for(manager) {
        Approvals::OnlyBuilt => names_in(
            value
                .get("pnpm")
                .and_then(|it| it.get("onlyBuiltDependencies")),
        ),
        Approvals::Trusted => names_in(value.get("trustedDependencies")),
        Approvals::DependenciesMeta => value
            .get("dependenciesMeta")
            .and_then(Value::as_object)
            .map(|meta| {
                meta.iter()
                    .filter(|(name, _)| !is_polluting_json_key(name))
                    .filter(|(_, entry)| entry.get("built").and_then(Value::as_bool) == Some(true))
                    .map(|(name, _)| name.as_str().to_compact_string())
                    .collect()
            })
            .unwrap_or_default(),
        Approvals::AllOrNothing => BTreeSet::new(),
    })
}

fn names_in(value: Option<&Value>) -> BTreeSet<CompactString> {
    value
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .filter(|name| !is_polluting_json_key(name))
                .map(ToCompactString::to_compact_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether an install may let dependency scripts run.
///
/// `true` when the project said so outright, and otherwise only when the
/// manager can enforce an allow-list *and* the project has recorded at least
/// one entry in it. See the module docs for why an empty list is not enough.
///
/// # Errors
///
/// When the root manifest is not JSON.
pub fn scripts_allowed(
    root: &Utf8Path,
    manager: PackageManager,
    configured: bool,
) -> Result<bool, PackageManagerError> {
    if configured {
        return Ok(true);
    }
    if !approvals_for(manager).is_per_package() {
        return Ok(false);
    }
    Ok(!approved(root, manager)?.is_empty())
}

#[cfg(test)]
mod tests;
