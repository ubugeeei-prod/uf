//! `uf ui update`: the registry's changes, merged into copies the project has
//! edited.
//!
//! `uf ui add` already brings an *untouched* copy up to this uf's version,
//! because nothing of anybody's is lost by it. What it cannot do is the case
//! that matters most once a project has lived with its components for a while:
//! a copy somebody edited, of a component the registry has changed since. The
//! choice there used to be keeping the edit and never receiving the registry's
//! fix, or `--overwrite` and losing the edit. ubugeeei-prod/uf#1356.
//!
//! # A three-way merge, and where its base comes from
//!
//! Two versions are on hand — the project's copy and this registry's text — and
//! a merge needs a third: the registry text the copy *began as*. The stamp
//! already names it exactly. Its digest is the SHA-256 of what `uf ui add`
//! wrote, so any candidate text can be checked against it, and a candidate
//! that does not hash to it is not the base, whoever supplied it. That is what
//! lets the base come from anywhere without being trusted:
//!
//! * **the project's history**, where the copy was usually committed the day it
//!   was added and before anybody edited it, or
//! * **the uf source at the release the stamp names**, which is the registry
//!   that wrote the copy, byte for byte.
//!
//! This module decides nothing about where to look. [`plan_update`] is handed
//! a function that produces candidates, and [`base_of`] is the check each one
//! has to pass; the command is the one that knows about `git` and the network.
//!
//! The merge itself is `similar`'s, the crate the diff already uses: line
//! based, non-overlapping edits combined, overlapping ones kept as a conflict.
//!
//! # What a merged copy is stamped as
//!
//! The digest of *this registry's* text, and this uf's version — not the digest
//! of what was written. That is what keeps [`crate::project::inspect`] right
//! afterwards: the content is not the registry's, so the copy reads as edited;
//! the stamp's digest is this registry's, so the registry has not moved since.
//! "Edited since uf <this>", with every remaining difference the project's own,
//! is exactly what a merge produces. Stamping the merge's own digest instead
//! would make the copy read as untouched, and the next `uf ui add` would
//! replace the project's edits without asking.
//!
//! A merge with conflicts is stamped the same way and written with markers in
//! it. The markers are a Flow syntax error, so `uf check` and every build stop
//! on the file until somebody resolves it, which is the property a conflict
//! needs: it cannot be shipped by accident.

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use similar::{ConflictStyle, TextMerge};

use crate::project::{AddError, CopyState, ProjectCopy, inspect, installed_packages};
use crate::registry::{Component, REGISTRY_VERSION, Registry, UnknownComponent};
use crate::stamp::{self, Copy, Stamp};

/// The label a conflict gives the project's side.
pub const OURS_LABEL: &str = "this project";

/// The text `stamp` says a copy began as, if `candidate` is it.
///
/// `candidate` may be a copy as it was committed — stamp and all — or the
/// registry's source as a release shipped it. Either way the stamp line, if
/// there is one, is set aside and `\r\n` read as `\n`, exactly as the digest
/// was taken, and the rest is the base only when it hashes to the stamp's
/// digest.
pub fn base_of(stamp: &Stamp, candidate: &str) -> Option<String> {
    let copy = Copy::read(candidate);
    (stamp::digest(&copy.content) == stamp.digest).then_some(copy.content)
}

/// A merge of the project's copy with this registry's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merge {
    /// The merged text, with conflict markers where there are conflicts.
    pub text: String,
    /// How many regions the two sides changed differently.
    pub conflicts: usize,
}

/// Merge `ours` (the project's copy) and `theirs` (this registry's text) from
/// `base` (the registry text the copy began as, at uf `from`).
///
/// Conflicts are written in diff3 style, with the base between `|||||||` and
/// `=======`, because the question a person resolving one has to answer is
/// "what did each side change", and the base is what says.
pub fn merge(base: &str, ours: &str, theirs: &str, from: &str) -> Merge {
    let mut merged = TextMerge::from_lines(base, ours, theirs);
    merged.conflict_style(ConflictStyle::Diff3).labels(
        compact_str::format_compact!("uf {from}").as_str(),
        OURS_LABEL,
        compact_str::format_compact!("uf {REGISTRY_VERSION}").as_str(),
    );
    Merge {
        text: merged.to_string(),
        conflicts: merged.conflict_count(),
    }
}

/// What `uf ui update` does to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateAction {
    /// Nothing: the copy is this registry's, untouched.
    Unchanged,
    /// Nothing: the copy was edited and the registry has not moved, so every
    /// difference is the project's own.
    Kept,
    /// Replaces a copy nobody edited with this registry's version.
    Update { from: CompactString },
    /// Merges this registry's changes into an edited copy.
    Merged {
        from: CompactString,
        conflicts: usize,
    },
    /// Nothing: the copy was edited and the registry moved, and no text that
    /// hashes to the stamp's digest was found to merge from.
    NoBase { from: CompactString },
    /// Nothing: a file `uf ui add` did not write.
    Foreign,
    /// Writes a component the updated ones now import and the project lacks.
    Create,
}

/// One file a run of `uf ui update` considered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStep {
    /// The component.
    pub component: &'static str,
    /// Its file.
    pub path: Utf8PathBuf,
    /// What happens to it.
    pub action: UpdateAction,
    /// What is written, for the actions that write.
    pub contents: Option<String>,
}

/// Everything a run of `uf ui update` would do, decided before any of it is
/// done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    /// Every copy considered, then every requirement it now needs.
    pub steps: Vec<UpdateStep>,
    /// The packages the written components import that `package.json` does not
    /// name, sorted.
    pub packages: Vec<CompactString>,
}

impl UpdatePlan {
    /// Whether the plan writes any file.
    pub fn writes(&self) -> bool {
        self.steps.iter().any(|step| step.contents.is_some())
    }

    /// The steps whose merge left conflicts.
    pub fn conflicted(&self) -> impl Iterator<Item = &UpdateStep> {
        self.steps.iter().filter(
            |step| matches!(step.action, UpdateAction::Merged { conflicts, .. } if conflicts > 0),
        )
    }
}

/// Why `uf ui update` will not go ahead.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// A name the registry does not have.
    #[error(transparent)]
    Unknown(#[from] UnknownComponent),
    /// Names the project has no file for. Every one is named.
    #[error("the project has no copy of {}", .0.join(", "))]
    NotAdded(Vec<&'static str>),
    /// A `package.json` that is not a JSON object.
    #[error("{path} could not be read as a manifest: {message}")]
    Manifest { path: Utf8PathBuf, message: String },
    /// A file that exists and cannot be read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Decide what `uf ui update <names…>` does in the project at `root`, whose
/// components are in `directory`.
///
/// With no names, every component the project has a file for. `find_base` is
/// asked for the base of each edited copy the registry has moved under; it
/// returns candidate texts, and the first that passes [`base_of`] is used. It
/// is not asked about any other copy, so a run with nothing to merge touches
/// neither `git` nor the network.
///
/// # Errors
///
/// A name the registry does not have, names the project has no copy of, or a
/// manifest or file that cannot be read. Nothing is written by this function.
pub fn plan_update<F>(
    root: &Utf8Path,
    directory: &Utf8Path,
    registry: &Registry,
    names: &[&str],
    mut find_base: F,
) -> Result<UpdatePlan, UpdateError>
where
    F: FnMut(&ProjectCopy, &Stamp) -> Vec<String>,
{
    let selected: Vec<&Component> = if names.is_empty() {
        registry
            .components()
            .iter()
            .filter(|component| directory.join(component.file_name()).exists())
            .collect()
    } else {
        names
            .iter()
            .map(|name| {
                registry.get(name).ok_or_else(|| UnknownComponent {
                    name: (*name).to_owned(),
                })
            })
            .collect::<Result<_, _>>()?
    };

    let mut steps = Vec::with_capacity(selected.len());
    let mut missing = Vec::new();
    for component in &selected {
        let copy = inspect(directory, component)?;
        let fresh = || Some(stamp::stamped(component.name, component.source));
        let (action, contents) = match &copy.state {
            CopyState::Missing => {
                missing.push(component.name);
                continue;
            }
            CopyState::Current { .. } => (UpdateAction::Unchanged, None),
            CopyState::Outdated { stamp } => (
                UpdateAction::Update {
                    from: stamp.version.clone(),
                },
                fresh(),
            ),
            CopyState::Edited {
                registry_moved: false,
                ..
            } => (UpdateAction::Kept, None),
            CopyState::Edited {
                stamp,
                registry_moved: true,
            } => {
                let base = find_base(&copy, stamp)
                    .iter()
                    .find_map(|candidate| base_of(stamp, candidate));
                match base {
                    Some(base) => {
                        let ours = copy.content.as_deref().unwrap_or_default();
                        let merged = merge(&base, ours, component.source, &stamp.version);
                        let onto = Stamp::new(component.name, component.source);
                        (
                            UpdateAction::Merged {
                                from: stamp.version.clone(),
                                conflicts: merged.conflicts,
                            },
                            Some(stamp::with_stamp(&merged.text, &onto)),
                        )
                    }
                    None => (
                        UpdateAction::NoBase {
                            from: stamp.version.clone(),
                        },
                        None,
                    ),
                }
            }
            CopyState::Foreign => (UpdateAction::Foreign, None),
        };
        steps.push(UpdateStep {
            component: component.name,
            path: copy.path,
            action,
            contents,
        });
    }
    if !missing.is_empty() {
        return Err(UpdateError::NotAdded(missing));
    }

    // A component's new version may import a sibling its old one did not. The
    // project needs that file, or the component it just received no longer
    // resolves.
    let written: Vec<&str> = steps
        .iter()
        .filter(|step| step.contents.is_some())
        .map(|step| step.component)
        .collect();
    let mut created: Vec<&Component> = Vec::new();
    for required in registry.closure(&written)? {
        if written.contains(&required.name) || created.iter().any(|seen| seen.name == required.name)
        {
            continue;
        }
        let copy = inspect(directory, required)?;
        if copy.state == CopyState::Missing {
            steps.push(UpdateStep {
                component: required.name,
                path: copy.path,
                action: UpdateAction::Create,
                contents: Some(stamp::stamped(required.name, required.source)),
            });
            created.push(required);
        }
    }

    let installed = installed_packages(root).map_err(|error| match error {
        AddError::Manifest { path, message } => UpdateError::Manifest { path, message },
        AddError::Io(error) => UpdateError::Io(error),
        // Reading a manifest fails in the two ways above and no other; this
        // arm is here so that a variant added to `AddError` later is still
        // reported rather than dropped.
        other => UpdateError::Io(std::io::Error::other(other.to_string())),
    })?;
    let mut packages: BTreeSet<CompactString> = BTreeSet::new();
    for step in steps.iter().filter(|step| step.contents.is_some()) {
        if let Some(component) = registry.get(step.component) {
            for dependency in &component.dependencies {
                if !installed.contains(dependency.as_str()) {
                    packages.insert(dependency.clone());
                }
            }
        }
    }

    Ok(UpdatePlan {
        steps,
        packages: packages.into_iter().collect(),
    })
}

/// Write every file `plan` writes.
///
/// # Errors
///
/// The first file that cannot be written.
pub fn apply(plan: &UpdatePlan) -> std::io::Result<()> {
    for step in &plan.steps {
        let Some(contents) = &step.contents else {
            continue;
        };
        if let Some(parent) = step.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&step.path, contents)?;
    }
    Ok(())
}
