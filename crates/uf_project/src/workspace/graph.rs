//! How the members of a workspace relate: which depends on which, which
//! workspace a project belongs to, and which members a `--filter` selects.
//!
//! All three are read from what a package manager already reads —
//! `package.json`, and the members [`discover_workspaces`] finds — and from
//! nothing uf asks a project to declare a second time. A workspace's build
//! order is already written down, in the dependencies that make it a
//! workspace, and a second copy in `uf.config.js` would be a copy to keep in
//! step.

use std::collections::BTreeSet;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use fixedbitset::FixedBitSet;
use globset::GlobBuilder;
use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;
use serde_json::Value;
use uf_config::{CONFIG_FILES, UniflowedConfig};
use uf_infra::InlineVec;

use super::{MAX_DEPTH, Workspace, discover_workspaces, package_name, package_workspace_patterns};

/// The `package.json` fields that name a package this one needs.
const DEPENDENCY_FIELDS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

/// For each member, the members its `package.json` depends on, as indices
/// into `workspaces`.
///
/// By name, whatever the version range says. `workspace:*`, `^1.2.0` and `*`
/// all mean "this sibling" to a package manager that links a workspace, and a
/// range the sibling's version happened not to satisfy is `uf install`'s to
/// report — not a reason for `uf run` to build two packages in the wrong
/// order. A member with no `package.json` name is matched by the name
/// [`discover_workspaces`] gave it.
#[must_use]
pub fn workspace_dependencies(root: &Utf8Path, workspaces: &[Workspace]) -> Vec<Vec<usize>> {
    let names: Vec<CompactString> = workspaces
        .iter()
        .map(|workspace| {
            package_name(&root.join(&workspace.path).join("package.json"))
                .unwrap_or_else(|| workspace.name.clone())
        })
        .collect();

    // Borrow names once. Each manifest key is resolved by hash lookup instead
    // of cloning it into a tree and scanning all workspace names afterward.
    let mut by_name: HashMap<&str, InlineVec<usize, 2>, FxBuildHasher> =
        HashMap::with_capacity_and_hasher(names.len(), FxBuildHasher);
    for (at, name) in names.iter().enumerate() {
        by_name.entry(name.as_str()).or_default().push(at);
    }
    workspaces
        .iter()
        .enumerate()
        .map(|(at, workspace)| {
            dependency_indices(
                &root.join(&workspace.path).join("package.json"),
                &by_name,
                names.len(),
                at,
            )
        })
        .collect()
}

fn dependency_indices(
    path: &Utf8Path,
    by_name: &HashMap<&str, InlineVec<usize, 2>, FxBuildHasher>,
    count: usize,
    own: usize,
) -> Vec<usize> {
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<Value>(&source) else {
        return Vec::new();
    };
    let mut wanted = FixedBitSet::with_capacity(count);
    for name in DEPENDENCY_FIELDS
        .iter()
        .filter_map(|field| manifest.get(field).and_then(Value::as_object))
        .flat_map(|dependencies| dependencies.keys())
    {
        if let Some(indices) = by_name.get(name.as_str()) {
            for &other in indices {
                if other != own {
                    wanted.insert(other);
                }
            }
        }
    }
    // Bit iteration retains workspace order and deduplicates all four fields.
    wanted.ones().collect()
}

/// The workspace the project at `root` belongs to: its root, and its members.
///
/// The project's own members when it has any. Otherwise the nearest project
/// above it that counts it as a member, because `uf run#app build` runs in
/// `packages/app`, and a `dependsOn: ["ui#build"]` there names a sibling only
/// the workspace above can find.
///
/// Looks no further up than a member can be found down — [`MAX_DEPTH`] — and
/// only at a directory that could be a workspace: one with a uf config, or a
/// `package.json` that lists `workspaces`. Stops at a repository's root, for
/// the reason [`discover_workspaces`] does not descend into another one.
#[must_use]
pub fn enclosing_workspace(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Option<(Utf8PathBuf, Vec<Workspace>)> {
    let own = discover_workspaces(root, config);
    if !own.is_empty() {
        return Some((root.to_path_buf(), own));
    }

    let mut current = root;
    for _ in 0..=MAX_DEPTH {
        if current.join(".git").exists() {
            return None;
        }
        current = current.parent()?;
        let could_be = CONFIG_FILES.iter().any(|name| current.join(name).is_file())
            || package_workspace_patterns(current).is_some();
        if !could_be {
            continue;
        }
        let config = uf_config::load_config(current)
            .map(|resolved| resolved.config)
            .unwrap_or_default();
        let members = discover_workspaces(current, &config);
        if members
            .iter()
            .any(|member| current.join(&member.path) == root)
        {
            return Some((current.to_path_buf(), members));
        }
    }
    None
}

/// Why a `--filter` could not select what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// It matched no member. Carries the selector as written.
    NoMatch(String),
    /// It could not be read: empty once its `...` were taken off, or a glob
    /// that does not compile.
    Malformed { selector: String, why: String },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatch(selector) => {
                write!(f, "--filter {selector:?} selects no workspace member")
            }
            Self::Malformed { selector, why } => {
                write!(f, "--filter {selector:?} cannot be read: {why}")
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// The members `selectors` pick out, as sorted indices into `workspaces`.
///
/// Each selector picks members, and the result is every member any of them
/// picked. The grammar is pnpm's, because it is the one people already type:
///
/// | selector | picks |
/// | --- | --- |
/// | `ui` | the member named `ui`, or at the path `ui` |
/// | `@acme/*`, `packages/*` | the members whose name or path the glob matches |
/// | `./packages/ui` | the members at that path, which may be a glob |
/// | `app...` | `app` and everything it depends on |
/// | `app^...` | only what `app` depends on |
/// | `...ui` | `ui` and everything that depends on it |
/// | `...^ui` | only what depends on `ui` |
///
/// "Depends on" is `dependencies`, directly or not — see
/// [`workspace_dependencies`], which is where `dependencies` comes from.
///
/// # Errors
///
/// A selector that picks nothing is an error rather than an empty run: a
/// misspelt `--filter` that ran zero tasks and exited 0 would be a check that
/// passed without running.
pub fn select_workspaces(
    workspaces: &[Workspace],
    dependencies: &[Vec<usize>],
    selectors: &[String],
) -> Result<Vec<usize>, SelectError> {
    let dependents = reversed(dependencies, workspaces.len());
    let mut selected = BTreeSet::new();

    for selector in selectors {
        let written = selector.trim();
        let (with_dependents, pattern) = match written.strip_prefix("...") {
            Some(rest) => (Some(!rest.starts_with('^')), rest.trim_start_matches('^')),
            None => (None, written),
        };
        let (with_dependencies, pattern) = match pattern.strip_suffix("...") {
            Some(rest) => (Some(!rest.ends_with('^')), rest.trim_end_matches('^')),
            None => (None, pattern),
        };
        // `Some(false)` is the `^` form: the relatives without the members
        // that were matched.
        let keep_matched = with_dependents != Some(false) && with_dependencies != Some(false);

        let matched = matching(workspaces, selector, pattern)?;
        if matched.is_empty() {
            return Err(SelectError::NoMatch(selector.clone()));
        }
        if keep_matched {
            selected.extend(matched.iter().copied());
        }
        if with_dependencies.is_some() {
            selected.extend(reachable(&matched, dependencies));
        }
        if with_dependents.is_some() {
            selected.extend(reachable(&matched, &dependents));
        }
    }
    Ok(selected.into_iter().collect())
}

/// The members `pattern` names — `selector` is what it was cut from, for the
/// error.
fn matching(
    workspaces: &[Workspace],
    selector: &str,
    pattern: &str,
) -> Result<Vec<usize>, SelectError> {
    let malformed = |why: String| SelectError::Malformed {
        selector: selector.to_owned(),
        why,
    };
    if pattern.is_empty() {
        return Err(malformed(String::from("it names no member")));
    }

    let (by_path_only, body) = match pattern.strip_prefix("./") {
        Some(path) => (true, path.trim_end_matches('/')),
        None => (false, pattern),
    };
    let has_glob = body
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | '{'));
    if !has_glob {
        return Ok(workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| {
                workspace.path == body || (!by_path_only && workspace.name == body)
            })
            .map(|(at, _)| at)
            .collect());
    }

    let glob = GlobBuilder::new(body)
        .literal_separator(true)
        .build()
        .map_err(|error| malformed(error.kind().to_string()))?
        .compile_matcher();
    Ok(workspaces
        .iter()
        .enumerate()
        .filter(|(_, workspace)| {
            glob.is_match(workspace.path.as_str())
                || (!by_path_only && glob.is_match(workspace.name.as_str()))
        })
        .map(|(at, _)| at)
        .collect())
}

/// Every member reachable from `start` along `edges`, not counting `start`
/// itself unless it is reached from another member of `start`.
fn reachable(start: &[usize], edges: &[Vec<usize>]) -> BTreeSet<usize> {
    let mut found = BTreeSet::new();
    let mut pending: Vec<usize> = start
        .iter()
        .flat_map(|&at| edges.get(at).into_iter().flatten().copied())
        .collect();
    while let Some(at) = pending.pop() {
        if found.insert(at) {
            pending.extend(edges.get(at).into_iter().flatten().copied());
        }
    }
    found
}

fn reversed(edges: &[Vec<usize>], count: usize) -> Vec<Vec<usize>> {
    let mut reversed = vec![Vec::new(); count];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets {
            if let Some(into) = reversed.get_mut(to) {
                into.push(from);
            }
        }
    }
    reversed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `utils` ← `ui` ← `app`, and `docs` on its own, under `packages/` and
    /// `apps/`.
    fn workspace() -> (Vec<Workspace>, Vec<Vec<usize>>) {
        let members = [
            ("@acme/utils", "packages/utils"),
            ("@acme/ui", "packages/ui"),
            ("app", "apps/app"),
            ("docs", "docs"),
        ]
        .iter()
        .map(|(name, path)| Workspace {
            name: CompactString::new(name),
            path: Utf8PathBuf::from(path),
        })
        .collect();
        (members, vec![vec![], vec![0], vec![1], vec![]])
    }

    fn select(selectors: &[&str]) -> Result<Vec<usize>, SelectError> {
        let (workspaces, dependencies) = workspace();
        let selectors: Vec<String> = selectors.iter().map(ToString::to_string).collect();
        select_workspaces(&workspaces, &dependencies, &selectors)
    }

    #[test]
    fn a_name_or_a_path_selects_one_member() {
        assert_eq!(select(&["app"]), Ok(vec![2]));
        assert_eq!(select(&["packages/ui"]), Ok(vec![1]));
        assert_eq!(select(&["./packages/ui"]), Ok(vec![1]));
    }

    #[test]
    fn a_glob_selects_by_name_or_by_path() {
        assert_eq!(select(&["@acme/*"]), Ok(vec![0, 1]));
        assert_eq!(select(&["./packages/*"]), Ok(vec![0, 1]));
        assert_eq!(select(&["apps/*"]), Ok(vec![2]));
    }

    /// `./` means a path and only a path, so a member whose *name* happens to
    /// look like one is not picked by it.
    #[test]
    fn a_path_selector_does_not_match_names() {
        assert!(matches!(select(&["./app"]), Err(SelectError::NoMatch(_))));
    }

    #[test]
    fn three_dots_after_add_what_a_member_depends_on() {
        assert_eq!(select(&["app..."]), Ok(vec![0, 1, 2]));
        assert_eq!(select(&["app^..."]), Ok(vec![0, 1]));
    }

    #[test]
    fn three_dots_before_add_what_depends_on_a_member() {
        assert_eq!(select(&["...@acme/utils"]), Ok(vec![0, 1, 2]));
        assert_eq!(select(&["...^@acme/utils"]), Ok(vec![1, 2]));
        assert_eq!(select(&["...@acme/ui..."]), Ok(vec![0, 1, 2]));
    }

    #[test]
    fn several_selectors_select_everything_any_of_them_does() {
        assert_eq!(select(&["docs", "@acme/ui"]), Ok(vec![1, 3]));
    }

    /// A misspelt selector that ran nothing and exited 0 would be a check that
    /// passed without running.
    #[test]
    fn a_selector_that_matches_nothing_is_an_error() {
        assert_eq!(
            select(&["ap"]),
            Err(SelectError::NoMatch(String::from("ap")))
        );
        assert!(matches!(
            select(&["..."]),
            Err(SelectError::Malformed { .. })
        ));
    }

    // --- on disk -----------------------------------------------------------

    fn write(root: &Utf8Path, path: &str, contents: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    fn monorepo() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        write(
            &root,
            "package.json",
            r#"{ "private": true, "workspaces": ["packages/*"] }"#,
        );
        write(
            &root,
            "packages/utils/package.json",
            r#"{ "name": "utils" }"#,
        );
        write(
            &root,
            "packages/ui/package.json",
            r#"{ "name": "ui", "dependencies": { "utils": "workspace:*", "react": "^19" } }"#,
        );
        write(
            &root,
            "packages/app/package.json",
            r#"{ "name": "app", "devDependencies": { "ui": "^0.0.0" } }"#,
        );
        (dir, root)
    }

    #[test]
    fn dependencies_are_read_from_each_members_package_json() {
        let (_dir, root) = monorepo();
        let members = discover_workspaces(&root, &UniflowedConfig::default());
        let names: Vec<&str> = members.iter().map(|member| member.name.as_str()).collect();
        assert_eq!(names, vec!["app", "ui", "utils"]);

        // `react` is not a member, and a version range is not looked at.
        assert_eq!(
            workspace_dependencies(&root, &members),
            vec![vec![1], vec![2], vec![]]
        );
    }

    #[test]
    fn a_member_finds_the_workspace_it_belongs_to() {
        let (_dir, root) = monorepo();
        let app = root.join("packages/app");

        let (found, members) = enclosing_workspace(&app, &UniflowedConfig::default())
            .expect("app is a member of the workspace above it");

        assert_eq!(found, root);
        assert_eq!(members.len(), 3);
        assert_eq!(
            enclosing_workspace(&root, &UniflowedConfig::default()).map(|(at, _)| at),
            Some(root.clone()),
            "the workspace root is its own workspace"
        );
    }

    #[test]
    fn a_project_nobody_lists_belongs_to_no_workspace() {
        let (_dir, root) = monorepo();
        write(&root, "scratch/package.json", r#"{ "name": "scratch" }"#);
        assert!(enclosing_workspace(&root.join("scratch"), &UniflowedConfig::default()).is_none());
    }
}
