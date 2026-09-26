//! A project's copies, held against the registry: what each file is, and what
//! `uf ui add` would do to it.

use std::collections::BTreeSet;
use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;

use crate::registry::{Component, REGISTRY_VERSION, Registry, UnknownComponent};
use crate::stamp::{self, Copy, Stamp};

/// Where components are written, relative to the project root.
///
/// Inside `app/` because that is where an application's code already is, and a
/// directory the router ignores: a route is a directory holding a `$page.js`,
/// and `components/ui/` holds none.
pub const DEFAULT_DIRECTORY: &str = "app/components/ui";

/// What a component's file in the project is, next to this registry's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyState {
    /// There is no file.
    Missing,
    /// Written by `uf ui add`, untouched since, and the same as this registry's.
    Current { stamp: Stamp },
    /// Written by `uf ui add` and untouched since, from a registry whose version
    /// of the component differs from this one's.
    Outdated { stamp: Stamp },
    /// Written by `uf ui add`, and changed since. `registry_moved` says whether
    /// this registry's version also differs from the one the copy began as,
    /// which is what decides whether a diff is only the project's edits.
    Edited { stamp: Stamp, registry_moved: bool },
    /// A file `uf ui add` did not write, or wrote for a different component.
    Foreign,
}

impl CopyState {
    /// One word for the state, which is also what `--json` reports.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Current { .. } => "current",
            Self::Outdated { .. } => "outdated",
            Self::Edited { .. } => "edited",
            Self::Foreign => "foreign",
        }
    }

    /// The stamp, for the states that have one.
    pub fn stamp(&self) -> Option<&Stamp> {
        match self {
            Self::Current { stamp } | Self::Outdated { stamp } | Self::Edited { stamp, .. } => {
                Some(stamp)
            }
            Self::Missing | Self::Foreign => None,
        }
    }
}

/// One component's file in a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCopy {
    /// The component.
    pub component: &'static str,
    /// Where its file is, or would be.
    pub path: Utf8PathBuf,
    /// What that file is.
    pub state: CopyState,
    /// The file without its stamp, when there is a file.
    pub content: Option<String>,
}

/// Read `component`'s file in `directory`.
///
/// # Errors
///
/// A file that exists and cannot be read. A file that does not exist is
/// [`CopyState::Missing`], not an error.
pub fn inspect(directory: &Utf8Path, component: &Component) -> io::Result<ProjectCopy> {
    let path = directory.join(component.file_name());
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ProjectCopy {
                component: component.name,
                path,
                state: CopyState::Missing,
                content: None,
            });
        }
        Err(error) => return Err(error),
    };
    let copy = Copy::read(&text);
    let untouched = copy.is_untouched();
    let state = match copy.stamp {
        Some(stamp) if stamp.component == component.name => {
            let registry_moved = stamp.digest != stamp::digest(component.source);
            match (untouched, registry_moved) {
                (true, false) => CopyState::Current { stamp },
                (true, true) => CopyState::Outdated { stamp },
                (false, registry_moved) => CopyState::Edited {
                    stamp,
                    registry_moved,
                },
            }
        }
        Some(_) | None => CopyState::Foreign,
    };
    Ok(ProjectCopy {
        component: component.name,
        path,
        state,
        content: Some(copy.content),
    })
}

/// Every component in the registry, as the project has it.
///
/// # Errors
///
/// The first file that exists and cannot be read.
pub fn survey(directory: &Utf8Path, registry: &Registry) -> io::Result<Vec<ProjectCopy>> {
    registry
        .components()
        .iter()
        .map(|component| inspect(directory, component))
        .collect()
}

/// What `uf ui add` does to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddAction {
    /// Writes a file that was not there.
    Create,
    /// Replaces a copy nobody edited with this registry's version.
    Update { from: CompactString },
    /// Nothing: the copy is already this registry's, untouched.
    Unchanged,
    /// Replaces an edited file, or one `uf ui add` did not write, because
    /// `--overwrite` said to.
    Replace,
    /// Leaves an edited or foreign file alone, because it was only needed by a
    /// component that was asked for and it still satisfies that import.
    Keep { edited: bool },
}

/// One file a run of `uf ui add` considered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddStep {
    /// The component.
    pub component: &'static str,
    /// Whether it was asked for, rather than needed by one that was.
    pub requested: bool,
    /// Its file.
    pub path: Utf8PathBuf,
    /// What happens to the file.
    pub action: AddAction,
    /// What is written, for the actions that write.
    pub contents: Option<String>,
}

/// A file `uf ui add` will not replace without `--overwrite`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The component that was asked for.
    pub component: &'static str,
    /// Its file.
    pub path: Utf8PathBuf,
    /// Whether `uf ui add` wrote it and somebody edited it since, as opposed to
    /// a file `uf ui add` never wrote.
    pub edited: bool,
}

/// Everything a run of `uf ui add` would do, decided before any of it is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlan {
    /// Every component the names given need, requirements first.
    pub steps: Vec<AddStep>,
    /// The packages those components import that the project's `package.json`
    /// does not name, sorted.
    pub packages: Vec<CompactString>,
}

impl AddPlan {
    /// Whether the plan writes any file.
    pub fn writes(&self) -> bool {
        self.steps.iter().any(|step| step.contents.is_some())
    }
}

/// Why `uf ui add` will not go ahead.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// A name the registry does not have.
    #[error(transparent)]
    Unknown(#[from] UnknownComponent),
    /// Files that would be replaced and were not written by the run that would
    /// replace them. Every one is named, not only the first.
    #[error("{} would replace a file it did not write", conflicts_sentence(.0))]
    Conflicts(Vec<Conflict>),
    /// A `package.json` that is not a JSON object.
    #[error("{path} could not be read as a manifest: {message}")]
    Manifest { path: Utf8PathBuf, message: String },
    /// A file that exists and cannot be read.
    #[error(transparent)]
    Io(#[from] io::Error),
}

fn conflicts_sentence(conflicts: &[Conflict]) -> String {
    match conflicts {
        [one] => uf_infra::into_string(uf_infra::cstr!("`uf ui add {}`", one.component)),
        _ => "`uf ui add`".to_owned(),
    }
}

/// Decide what `uf ui add <names…>` does in the project at `root`, writing
/// components into `directory`.
///
/// # Errors
///
/// A name the registry does not have, every file that would be replaced
/// without `overwrite`, or a manifest or file that cannot be read. Nothing is
/// written by this function, so an error here has changed nothing.
pub fn plan_add(
    root: &Utf8Path,
    directory: &Utf8Path,
    registry: &Registry,
    names: &[&str],
    overwrite: bool,
) -> Result<AddPlan, AddError> {
    let components = registry.closure(names)?;
    let mut steps = Vec::with_capacity(components.len());
    let mut conflicts = Vec::new();

    for component in &components {
        let requested = names.contains(&component.name);
        let copy = inspect(directory, component)?;
        let fresh = || Some(stamp::stamped(component.name, component.source));
        let (action, contents) = match (&copy.state, requested, overwrite) {
            (CopyState::Missing, _, _) => (AddAction::Create, fresh()),
            (CopyState::Current { .. }, _, _) => (AddAction::Unchanged, None),
            (CopyState::Outdated { stamp }, _, _) => (
                AddAction::Update {
                    from: stamp.version.clone(),
                },
                fresh(),
            ),
            (CopyState::Edited { .. } | CopyState::Foreign, true, true) => {
                (AddAction::Replace, fresh())
            }
            (state @ (CopyState::Edited { .. } | CopyState::Foreign), true, false) => {
                conflicts.push(Conflict {
                    component: component.name,
                    path: copy.path.clone(),
                    edited: matches!(state, CopyState::Edited { .. }),
                });
                continue;
            }
            (state @ (CopyState::Edited { .. } | CopyState::Foreign), false, _) => (
                AddAction::Keep {
                    edited: matches!(state, CopyState::Edited { .. }),
                },
                None,
            ),
        };
        steps.push(AddStep {
            component: component.name,
            requested,
            path: copy.path,
            action,
            contents,
        });
    }

    if !conflicts.is_empty() {
        return Err(AddError::Conflicts(conflicts));
    }

    let installed = installed_packages(root)?;
    let mut packages: BTreeSet<CompactString> = BTreeSet::new();
    for component in &components {
        for dependency in &component.dependencies {
            if !installed.contains(dependency.as_str()) {
                packages.insert(dependency.clone());
            }
        }
    }

    Ok(AddPlan {
        steps,
        packages: packages.into_iter().collect(),
    })
}

/// Write every file `plan` writes.
///
/// # Errors
///
/// The first file that cannot be written.
pub fn apply(plan: &AddPlan) -> io::Result<()> {
    for step in &plan.steps {
        let Some(contents) = &step.contents else {
            continue;
        };
        if let Some(parent) = step.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&step.path, contents)?;
    }
    Ok(())
}

/// How a package is asked for: an `@uniflowed/*` package at this uf's own
/// version, anything else by name for the package manager to resolve.
pub fn package_spec(name: &str) -> String {
    if name.starts_with("@uniflowed/") {
        uf_infra::into_string(uf_infra::cstr!("{name}@{REGISTRY_VERSION}"))
    } else {
        name.to_owned()
    }
}

/// Every package the project's `package.json` names, in any dependency field.
///
/// A project with no `package.json` names none, and is told which packages to
/// add rather than refused here: the package manager is the one that can say
/// what it needs.
pub(crate) fn installed_packages(root: &Utf8Path) -> Result<BTreeSet<String>, AddError> {
    let path = root.join("package.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let manifest: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| AddError::Manifest {
            path: path.clone(),
            message: error.to_string(),
        })?;
    if !manifest.is_object() {
        return Err(AddError::Manifest {
            path,
            message: "the top level is not an object".to_owned(),
        });
    }
    let mut names = BTreeSet::new();
    for field in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(entries) = manifest.get(field).and_then(serde_json::Value::as_object) {
            names.extend(entries.keys().cloned());
        }
    }
    Ok(names)
}
