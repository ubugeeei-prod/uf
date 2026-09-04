//! The uf projects inside a uf project.
//!
//! A repository is commonly more than one thing: this one is the toolchain, a
//! documentation site, and a library test suite, and each has its own
//! `uf.config.js` because each is genuinely a different project. `uf dev` at
//! the root cannot serve all three, and asking someone to `cd docs` first is
//! asking them to know the layout before they can use the tool.
//!
//! So a command may name which one it means: `uf dev#docs`. The selector is on
//! the command rather than a flag because it changes *where the command runs*
//! rather than how, and reads in the order it happens.
//!
//! # What counts as a member
//!
//! A directory with its own `uf.config.js`. Nothing is declared, because a
//! declaration would be a second list to keep in step with the first — and the
//! first already exists, in the form of the config files themselves. The root's
//! own config is not a member: `uf dev` already means the root.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};

use uf_config::{CONFIG_FILES, UniflowedConfig};

use crate::is_ignored;

#[cfg(test)]
mod tests;

/// How deep below the root a member is looked for.
///
/// `docs` is one level down and `tests/library` is two, which is where projects
/// in a repository actually live. Going deeper means walking into build output
/// and vendored trees for no gain, and a member buried four levels down is not
/// discoverable by a person either.
const MAX_DEPTH: usize = 3;

/// One uf project inside another.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Workspace {
    /// What `uf <command>#<name>` calls it: the directory's own name.
    pub name: CompactString,
    /// Where it is, relative to the root.
    pub path: Utf8PathBuf,
}

impl Workspace {
    /// Whether `selector` names this member.
    ///
    /// Both the short name and the path are accepted, because both are things
    /// someone reasonably types: `docs` is what it is called, and
    /// `tests/library` is what it is. A path selector settles the ambiguity
    /// when two directories share a name.
    #[must_use]
    pub fn matches(&self, selector: &str) -> bool {
        self.name == selector || self.path == selector
    }
}

/// Every uf project under `root`, excluding `root` itself, sorted by path.
///
/// Ignored directories are skipped using the project's own ignore list, so a
/// `uf.config.js` inside `node_modules` or a build output directory is not a
/// member of anything.
pub fn discover_workspaces(root: &Utf8Path, config: &UniflowedConfig) -> Vec<Workspace> {
    let mut found = Vec::new();
    visit(root, root, config, 0, &mut found);
    found.sort();
    found
}

fn visit(
    root: &Utf8Path,
    dir: &Utf8Path,
    config: &UniflowedConfig,
    depth: usize,
    found: &mut Vec<Workspace>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) || is_ignored(root, &path, config) {
            continue;
        }
        // Another repository's contents are not this project's members, for the
        // same reason they are not this project's source files.
        if path.join(".git").exists() {
            continue;
        }

        if CONFIG_FILES.iter().any(|name| path.join(name).exists())
            && let Some(name) = path.file_name()
            && let Ok(relative) = path.strip_prefix(root)
        {
            found.push(Workspace {
                name: name.to_compact_string(),
                path: relative.to_path_buf(),
            });
            // A project inside a project is that project's business, not this
            // one's: `uf dev#docs` selects `docs`, and what `docs` contains is
            // resolved from there.
            continue;
        }

        visit(root, &path, config, depth + 1, found);
    }
}

/// The member `selector` names, or an error naming what was available.
///
/// # Errors
///
/// Returns the list of member names when nothing matched, so a caller can say
/// what could have been meant rather than only that this was not it.
pub fn resolve_workspace<'a>(
    workspaces: &'a [Workspace],
    selector: &str,
) -> Result<&'a Workspace, Vec<CompactString>> {
    workspaces
        .iter()
        .find(|workspace| workspace.matches(selector))
        .ok_or_else(|| {
            workspaces
                .iter()
                .map(|workspace| workspace.name.clone())
                .collect()
        })
}
