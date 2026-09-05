//! Collection: what the store holds that no repository is using.
//!
//! # Why reachability rather than a reference count
//!
//! A count has to be decremented, which means every way an entry stops being
//! used has to remember to say so — a repository deleted with `rm -rf`, a
//! machine that lost power mid-install, a `uf.config.js` edited by hand. Each
//! of those is a leak that grows silently.
//!
//! Reachability has no such obligation. The roots are the truth, the store is
//! a cache of them, and anything the roots do not name is garbage whatever
//! happened to it. It is why this can be run at any time, including after a
//! crash, and give the right answer.
//!
//! # Why a plan is a value
//!
//! [`plan`] decides and returns; [`collect`] deletes what a plan says. So
//! `uf env gc --dry-run` and `uf env gc` run the same decision, and the thing
//! a reader was shown is exactly the thing that is removed.

use camino::Utf8PathBuf;

use crate::EnvError;
use crate::roots::Roots;
use crate::store::Store;

/// What collection would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Store entries no live repository references, by directory name.
    pub unreachable: Vec<String>,
    /// Roots whose repository is gone, and the entries they were holding.
    pub dead_roots: Vec<(Utf8PathBuf, Utf8PathBuf)>,
    /// Entries a live repository is using, and so are kept.
    pub kept: usize,
}

impl Plan {
    /// Whether there is anything to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unreachable.is_empty() && self.dead_roots.is_empty()
    }
}

/// Decide what to collect.
///
/// A dead root is pruned *before* reachability is computed, which is what
/// makes deleting a repository release its tools: the root goes, nothing
/// names its entries, and they become unreachable in the same pass.
///
/// # Errors
///
/// When the store or the roots cannot be read. A root that does not parse
/// stops the whole plan rather than being skipped — an unreadable root is an
/// unknown set of entries in use, and guessing is how a working tool is
/// deleted out from under a project.
pub fn plan(store: &Store, roots: &Roots) -> Result<Plan, EnvError> {
    let mut plan = Plan::default();
    let mut reachable: Vec<String> = Vec::new();

    for (file, root) in roots.all()? {
        if root.is_live() {
            reachable.extend(root.entries.iter().cloned());
        } else {
            plan.dead_roots.push((file, root.repository));
        }
    }
    reachable.sort();
    reachable.dedup();
    plan.kept = reachable.len();

    for entry in store.entries()? {
        // A half-finished unpack from an interrupted install. Nothing links
        // it and nothing ever will, because the next install stages a fresh
        // one; leaving it would be a leak that only ever grows.
        if entry.starts_with(".staging-") || !reachable.binary_search(&entry).is_ok() {
            plan.unreachable.push(entry);
        }
    }
    Ok(plan)
}

/// Carry out `plan`.
///
/// Returns how many store entries and how many roots were removed.
///
/// # Errors
///
/// When something cannot be deleted. It stops rather than continuing, so a
/// store that is partly collected is a store whose remaining contents are
/// still described by the plan that was printed.
pub fn collect(store: &Store, roots: &Roots, plan: &Plan) -> Result<(usize, usize), EnvError> {
    for (file, _) in &plan.dead_roots {
        roots.forget(file)?;
    }
    for entry in &plan.unreachable {
        store.remove(entry)?;
    }
    Ok((plan.unreachable.len(), plan.dead_roots.len()))
}
