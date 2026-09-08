//! The one file `uf check` reads that `uf` did not design: `.flowconfig`.
//!
//! uf has no configuration of its own for the dialect — [`crate`]'s checker
//! options are constants, and that is the point of a unified toolchain. But a
//! project's **library definitions** are not a dialect setting. They are
//! source: the `declare module` blocks that say what an untyped dependency
//! exports, and the `declare type` blocks that say what a global is. Flow
//! loads them through `.flowconfig`'s `[libs]`; every project with a
//! dependency that ships no Flow types has some; and a checker that cannot see
//! them types every name they declare as `any` — which is not a missing
//! warning but a wrong answer, one `value-as-type` error per use.
//! ubugeeei-prod/uf#480 measured that at 506 errors where `flow check`
//! reported 20 on the same tree.
//!
//! So this module reads the section, and reads it with Flow's own parser
//! rather than a second one: `flow_config` is the crate `flow check` itself
//! parses `.flowconfig` with, so a section uf accepts is one Flow accepts, a
//! `[rollouts]` uf resolves is the one Flow resolves, and a line uf rejects is
//! one Flow rejects too.
//!
//! # Why the version constraint is ignored
//!
//! `[version]` says which `flow` binary may check this project. uf is not that
//! binary — it embeds Flow's checker rather than being it — so enforcing the
//! constraint would refuse the `[libs]` of every project that pins a Flow
//! release, which is most of them. The section this module wants does not
//! depend on the version, and reading it under a pin uf cannot satisfy is
//! better than reading nothing.
//!
//! # What is not read
//!
//! Everything else. `[ignore]`, `[include]`, `[options]`, `[lints]` and
//! `[declarations]` are configuration of exactly the kind uf declines to take:
//! `uf_project`'s scan decides what the project owns, `uf_lint` owns lint
//! severities, and the dialect is fixed. Reading half of `[options]` would be
//! worse than reading none of it, because a reader could not tell which half.

use std::path::Path;

use crate::CheckError;

/// The file name Flow's configuration lives in.
const FLOWCONFIG: &str = ".flowconfig";

/// The library directory Flow includes whether or not a project lists it.
///
/// `flow-typed` is where `flow-typed install` writes, and Flow's own
/// `flow_command_utils` inserts it at the front of the lib paths when the
/// config did not name it. Ahead of the configured entries because a later
/// definition shadows an earlier one: a project that also lists a directory of
/// its own can override what `flow-typed` declared, and not the other way
/// round.
const IMPLICIT_LIB: &str = "flow-typed";

/// The library definitions a project adds to Flow's own.
///
/// Paths are project-relative and in **declaration order**, which is
/// significant: library definitions are merged in that order and a later one
/// shadows an earlier one, so a caller that reorders them changes what the
/// project's globals mean.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibPaths {
    paths: Vec<String>,
}

impl LibPaths {
    /// The lib paths, in declaration order.
    ///
    /// Each is relative to the project root and may name either a file or a
    /// directory — Flow accepts both, and expands a directory to the files
    /// under it. Expanding it is the caller's job, because it is a walk of the
    /// file system and this crate does not do one: see `uf_cli`'s
    /// `commands::check::libdefs`.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.paths.iter().map(String::as_str)
    }

    /// Whether the project declares no library definitions at all.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// The `[libs]` of the project rooted at `project_root`.
///
/// A project with no `.flowconfig` is neither an error nor an empty answer: it
/// still gets `flow-typed`, because that directory is a convention rather than
/// a configuration — it is what `flow-typed install` writes, it is what Flow
/// includes without being asked, and a project that has one meant it. A
/// project that has neither gets a list whose one entry resolves to no file,
/// which costs a directory read that misses.
///
/// A `.flowconfig` that does not parse **is** an error. It is a file the author
/// wrote and can fix, and checking on without the libdefs it names would report
/// hundreds of type errors whose whole cause is one bad line.
pub fn lib_paths(project_root: &Path) -> Result<LibPaths, CheckError> {
    #[cfg(feature = "upstream-typecheck")]
    {
        read(project_root)
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        let _ = project_root;
        Err(CheckError::Unavailable)
    }
}

/// Parse the project's `.flowconfig`, if it has one.
#[cfg(feature = "upstream-typecheck")]
fn read(project_root: &Path) -> Result<LibPaths, CheckError> {
    use compact_str::ToCompactString;

    let path = project_root.join(FLOWCONFIG);
    if !path.is_file() {
        return Ok(assemble(std::iter::empty()));
    }
    // Upstream takes the file name as a `&str` and opens it itself. A path
    // that is not UTF-8 is one it could not have been given either, so it is
    // reported as unreadable rather than silently skipped — a project whose
    // libdefs were ignored looks like a project with hundreds of type errors.
    let Some(spelling) = path.to_str() else {
        return Err(CheckError::FlowConfig {
            path: path.display().to_string().to_compact_string(),
            line: 0,
            detail: "the path is not valid UTF-8".to_compact_string(),
        });
    };
    // Warnings are Flow's own advice about options uf does not read — an
    // option that is deprecated, or one this build does not support. Repeating
    // them would be `uf check` reporting on configuration it then ignores.
    match flow_config::get_with_ignored_version(spelling, true) {
        Ok((config, _warnings, _hash)) => Ok(assemble(config.libs)),
        Err(flow_config::Error(line, detail)) => Err(CheckError::FlowConfig {
            path: path.display().to_string().to_compact_string(),
            line,
            detail: detail.to_compact_string(),
        }),
    }
}

/// The lib paths a `[libs]` section means, with the two rules that are uf's
/// rather than Flow's parser's applied: `flow-typed` first unless the config
/// placed it, and `<PROJECT_ROOT>` spelled as the relative path uf's batch
/// paths are in.
fn assemble(configured: impl IntoIterator<Item = String>) -> LibPaths {
    let mut paths = vec![String::from(IMPLICIT_LIB)];
    for configured in configured {
        let path = relative(&configured);
        if path.is_empty() {
            continue;
        }
        if let Some(position) = paths.iter().position(|existing| *existing == path) {
            // Listed explicitly: the entry keeps the position the author gave
            // it, because that is what decides what shadows what. This is the
            // branch that moves `flow-typed` when a config names it.
            paths.remove(position);
        }
        paths.push(path);
    }
    LibPaths { paths }
}

/// The path spelling of a configured lib entry, relative to the project root.
///
/// uf's batch paths are project-relative, so the token that means "the project
/// root" is the empty prefix rather than an absolute path. Upstream's own
/// `expand_project_root_token_as_relative` is that spelling, and it also
/// normalises `\` to `/` for a config written on Windows.
#[cfg(feature = "upstream-typecheck")]
fn relative(configured: &str) -> String {
    flow_common::files::expand_project_root_token_as_relative(configured)
}

/// The path spelling of a configured lib entry, with no checker compiled in.
///
/// Unreachable through [`lib_paths`], which reports itself unavailable first;
/// it exists so that [`assemble`]'s own rules stay testable in a build that
/// has no upstream to ask.
#[cfg(not(feature = "upstream-typecheck"))]
fn relative(configured: &str) -> String {
    configured.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assembled(configured: &[&str]) -> Vec<String> {
        assemble(configured.iter().map(|entry| (*entry).to_owned()))
            .paths()
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn a_project_that_configures_nothing_still_gets_flow_typed() {
        assert_eq!(assembled(&[]), ["flow-typed"]);
    }

    #[test]
    fn a_configured_directory_is_merged_after_flow_typed() {
        // Order decides which definition wins, and the implicit directory has
        // to lose to one the author wrote down.
        assert_eq!(assembled(&["libdefs"]), ["flow-typed", "libdefs"]);
    }

    #[test]
    fn naming_flow_typed_explicitly_moves_it_rather_than_repeating_it() {
        assert_eq!(
            assembled(&["libdefs", "flow-typed"]),
            ["libdefs", "flow-typed"]
        );
    }

    #[test]
    fn an_empty_entry_names_no_directory() {
        assert_eq!(assembled(&[""]), ["flow-typed"]);
    }

    #[cfg(feature = "upstream-typecheck")]
    #[test]
    fn the_project_root_token_becomes_a_relative_path() {
        assert_eq!(
            assembled(&["<PROJECT_ROOT>/types"]),
            ["flow-typed", "types"]
        );
    }

    #[test]
    fn a_lib_paths_with_entries_is_not_empty() {
        assert!(!assemble(["libdefs".to_owned()]).is_empty());
        assert!(LibPaths::default().is_empty());
    }
}
