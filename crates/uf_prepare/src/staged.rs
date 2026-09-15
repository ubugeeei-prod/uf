//! Which staged files each entry of `staged` in `uf.config.js` is about.

use camino::Utf8Path;
use compact_str::CompactString;
use globset::GlobBuilder;

/// One entry of `staged` that matched a staged file: its tasks, and the files
/// to hand them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedRun {
    /// The glob, as written.
    pub glob: CompactString,
    /// The tasks it names, in order.
    pub tasks: Vec<CompactString>,
    /// The staged files it matched, relative to the project root.
    pub files: Vec<CompactString>,
}

/// A glob in `staged` that does not compile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`staged` in uf.config.js has a glob that cannot be read, {glob:?}: {why}")]
pub struct StagedGlobError {
    /// The glob, as written.
    pub glob: CompactString,
    /// What is wrong with it.
    pub why: String,
}

/// The runs `entries` make over `files`, in the entries' order.
///
/// A glob with no `/` is matched against a file's *name*, wherever the file
/// is — `*.css` is every staged stylesheet — and a glob with one is matched
/// against its path from the project root, so `app/**/*.js` means what it
/// says. That is lint-staged's rule, and the one people already write. An
/// entry that matches no staged file makes no run: a task handed no files
/// would be a task run over nothing, or over everything.
///
/// # Errors
///
/// When a glob does not compile. Before anything runs, so a typo in one
/// entry is not discovered after the others have rewritten files.
pub fn staged_runs<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a [CompactString])>,
    files: &[CompactString],
) -> Result<Vec<StagedRun>, StagedGlobError> {
    let mut runs = Vec::new();
    for (glob, tasks) in entries {
        let matcher = GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map_err(|error| StagedGlobError {
                glob: CompactString::new(glob),
                why: error.kind().to_string(),
            })?
            .compile_matcher();
        let by_name = !glob.contains('/');
        let matched: Vec<CompactString> = files
            .iter()
            .filter(|file| {
                let path = Utf8Path::new(file.as_str());
                if by_name {
                    path.file_name().is_some_and(|name| matcher.is_match(name))
                } else {
                    matcher.is_match(path.as_str())
                }
            })
            .cloned()
            .collect();
        if !matched.is_empty() && !tasks.is_empty() {
            runs.push(StagedRun {
                glob: CompactString::new(glob),
                tasks: tasks.to_vec(),
                files: matched,
            });
        }
    }
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<CompactString> {
        list.iter().map(|name| CompactString::new(name)).collect()
    }

    #[test]
    fn a_glob_without_a_slash_matches_names_anywhere_and_one_with_a_slash_matches_paths() {
        let files = names(&["a.css", "app/b.css", "app/page.js", "lib/page.js"]);
        let fix = names(&["fix"]);
        let lint = names(&["lint", "types"]);

        let runs = staged_runs(
            [("*.css", fix.as_slice()), ("app/*.js", lint.as_slice())],
            &files,
        )
        .unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].files, names(&["a.css", "app/b.css"]));
        assert_eq!(runs[1].files, names(&["app/page.js"]));
        assert_eq!(runs[1].tasks, lint);
    }

    #[test]
    fn an_entry_that_matches_nothing_staged_makes_no_run() {
        let fix = names(&["fix"]);
        assert!(
            staged_runs([("*.rs", fix.as_slice())], &names(&["a.js"]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_glob_that_does_not_compile_is_refused_by_name() {
        let fix = names(&["fix"]);
        let error = staged_runs([("*.{js", fix.as_slice())], &names(&["a.js"])).unwrap_err();
        assert_eq!(error.glob, "*.{js");
    }
}
