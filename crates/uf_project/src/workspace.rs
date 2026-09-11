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
//! A directory with its own `uf.config.js`, or a package named by the root
//! `package.json`'s standard `workspaces` field. The root's own config is not a
//! member: `uf dev` already means the root.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use serde_json::Value;

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
    /// What `uf <command>#<name>` calls it.
    ///
    /// Config-discovered members use the directory name; package workspaces use
    /// `package.json#name` when present.
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

    for workspace in package_workspaces(root, config) {
        push_workspace(&mut found, workspace);
    }
    visit(root, root, config, 0, &mut found);

    found.sort();
    found
}

fn push_workspace(found: &mut Vec<Workspace>, workspace: Workspace) {
    if found.iter().any(|existing| existing.path == workspace.path) {
        return;
    }
    found.push(workspace);
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
            push_workspace(
                found,
                Workspace {
                    name: name.to_compact_string(),
                    path: relative.to_path_buf(),
                },
            );
            // A project inside a project is that project's business, not this
            // one's: `uf dev#docs` selects `docs`, and what `docs` contains is
            // resolved from there.
            continue;
        }

        visit(root, &path, config, depth + 1, found);
    }
}

fn package_workspaces(root: &Utf8Path, config: &UniflowedConfig) -> Vec<Workspace> {
    let Some(patterns) = package_workspace_patterns(root) else {
        return Vec::new();
    };
    let Some(globs) = WorkspaceGlobs::compile(&patterns) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for relative in globs.walk_roots() {
        let start = root.join(relative);
        visit_package_workspace_root(root, &start, config, &globs, &mut found);
    }
    found
}

fn package_workspace_patterns(root: &Utf8Path) -> Option<Vec<String>> {
    let source = fs::read_to_string(root.join("package.json")).ok()?;
    let manifest = serde_json::from_str::<Value>(&source).ok()?;
    let workspaces = manifest.get("workspaces")?;

    let mut patterns = Vec::new();
    match workspaces {
        Value::Array(entries) => patterns.extend(workspace_pattern_strings(entries)),
        // Yarn 1 accepts `{ "packages": [...], "nohoist": [...] }`. Only the
        // `packages` list names members; `nohoist` is an install layout rule.
        Value::Object(object) => {
            if let Some(entries) = object.get("packages").and_then(Value::as_array) {
                patterns.extend(workspace_pattern_strings(entries));
            }
        }
        _ => {}
    }

    (!patterns.is_empty()).then_some(patterns)
}

fn workspace_pattern_strings(entries: &[Value]) -> impl Iterator<Item = String> + '_ {
    entries.iter().filter_map(Value::as_str).map(str::to_owned)
}

fn visit_package_workspace_root(
    root: &Utf8Path,
    dir: &Utf8Path,
    config: &UniflowedConfig,
    globs: &WorkspaceGlobs,
    found: &mut Vec<Workspace>,
) {
    if dir != root && (is_ignored(root, dir, config) || dir.join(".git").exists()) {
        return;
    }

    if let Ok(relative) = dir.strip_prefix(root)
        && !relative.as_str().is_empty()
        && globs.matches(relative.as_str())
        && dir.join("package.json").is_file()
    {
        push_workspace(found, package_workspace(root, relative));
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            visit_package_workspace_root(root, &path, config, globs, found);
        }
    }
}

fn package_workspace(root: &Utf8Path, relative: &Utf8Path) -> Workspace {
    let manifest_path = root.join(relative).join("package.json");
    let name = package_name(&manifest_path).unwrap_or_else(|| {
        relative
            .file_name()
            .unwrap_or(relative.as_str())
            .to_compact_string()
    });

    Workspace {
        name,
        path: relative.to_path_buf(),
    }
}

fn package_name(path: &Utf8Path) -> Option<CompactString> {
    let source = fs::read_to_string(path).ok()?;
    let manifest = serde_json::from_str::<Value>(&source).ok()?;
    manifest
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToCompactString::to_compact_string)
}

struct WorkspaceGlobs {
    include: GlobSet,
    exclude: GlobSet,
    roots: Vec<Utf8PathBuf>,
    from_root: bool,
}

impl WorkspaceGlobs {
    fn compile(patterns: &[String]) -> Option<Self> {
        let mut include = GlobSetBuilder::new();
        let mut exclude = GlobSetBuilder::new();
        let mut roots = Vec::new();
        let mut from_root = false;
        let mut has_include = false;

        for pattern in patterns {
            let Some(pattern) = workspace_pattern(pattern) else {
                continue;
            };
            let Ok(glob) = workspace_glob(pattern.body) else {
                continue;
            };
            if pattern.include {
                has_include = true;
                include.add(glob);
                match workspace_walk_root(pattern.body) {
                    Some(root) => roots.push(root),
                    None => from_root = true,
                }
            } else {
                exclude.add(glob);
            }
        }
        if !has_include {
            return None;
        }

        roots.sort();
        roots.dedup();
        let mut kept: Vec<Utf8PathBuf> = Vec::with_capacity(roots.len());
        for root in roots {
            if kept.iter().any(|existing| root.starts_with(existing)) {
                continue;
            }
            kept.push(root);
        }

        Some(Self {
            include: include.build().ok()?,
            exclude: exclude.build().ok()?,
            roots: kept,
            from_root,
        })
    }

    fn matches(&self, path: &str) -> bool {
        self.include.is_match(path) && !self.exclude.is_match(path)
    }

    fn walk_roots(&self) -> Vec<&Utf8Path> {
        if self.from_root {
            return vec![Utf8Path::new("")];
        }
        self.roots.iter().map(Utf8PathBuf::as_path).collect()
    }
}

struct WorkspacePattern<'a> {
    include: bool,
    body: &'a str,
}

fn workspace_pattern(entry: &str) -> Option<WorkspacePattern<'_>> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let (include, body) = entry
        .strip_prefix('!')
        .map_or((true, entry), |body| (false, body));
    let body = body
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/');
    if body.is_empty() || body == "." || body.split('/').any(|segment| segment == "..") {
        return None;
    }
    Some(WorkspacePattern { include, body })
}

fn workspace_glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .backslash_escape(true)
        .build()
}

fn workspace_walk_root(pattern: &str) -> Option<Utf8PathBuf> {
    let mut root = Utf8PathBuf::new();
    for segment in pattern.split('/') {
        if segment.is_empty() || has_glob_syntax(segment) {
            break;
        }
        root.push(segment);
    }
    (!root.as_str().is_empty()).then_some(root)
}

fn has_glob_syntax(pattern: &str) -> bool {
    pattern
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | '{'))
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
