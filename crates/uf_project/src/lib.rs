use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::WalkBuilder;
use thiserror::Error;
use uf_config::UniflowedConfig;

mod template;
pub mod workspace;

use template::{app_react_files, lib_files};
pub use workspace::{Workspace, discover_workspaces, resolve_workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    AppReact,
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOptions {
    pub name: String,
    pub kind: CreateKind,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub root: Utf8PathBuf,
    pub files: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    pub absolute_path: Utf8PathBuf,
    pub relative_path: String,
    pub source: String,
    /// What kind of file this is.
    ///
    /// The linter reads `package.json` as well as JavaScript, so discovery
    /// returns both. The formatter must not: it is a JavaScript formatter, and
    /// running it over JSON inserts statement terminators and destroys the file.
    /// Recording the kind is what lets each caller say which it wants instead of
    /// every caller having to remember.
    pub kind: SourceKind,
}

/// A path discovery found and could not open.
///
/// One stray byte should not stop a project. A `.js` that is not UTF-8 — a
/// build artifact, a vendored blob, a fixture somebody committed by accident
/// — used to abort `uf fmt`, `uf lint`, `uf check` and `uf doc` at the first
/// one, leaving every other file in the project untouched.
///
/// A directory the walk cannot open is recorded the same way, and for the same
/// reason: one unreadable directory is not a reason to do nothing for the rest
/// of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableFile {
    /// Where it is, relative to the project root.
    pub relative_path: String,
    /// Why it could not be read, in one line.
    pub reason: String,
}

/// What discovery found: the files it read, and the ones it could not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceScan {
    /// Every readable source file, sorted by path.
    pub files: Vec<ProjectFile>,
    /// The ones that were skipped, sorted by path.
    ///
    /// Reported rather than returned as an error: a caller that stops on the
    /// first one does nothing for the rest of the project, and a caller that
    /// ignores them silently is worse. Every command prints them and fails.
    pub unreadable: Vec<UnreadableFile>,
}

/// What a discovered project file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    /// Flow-typed JavaScript: `.js`, `.jsx`, `.mjs`, `.cjs`.
    JavaScript,
    /// A `package.json` manifest. Read by the linter, never rewritten.
    PackageManifest,
    /// JSON or JSONC that is not a package manifest.
    Json,
    /// A stylesheet: `.css`, `.scss`, `.less`.
    Style,
    /// TypeScript, which a uf project may still hold at its edges — a config
    /// file, a generated declaration, a dependency's shim.
    TypeScript,
}

impl SourceKind {
    /// Classify a path, or [`None`] when the project does not own it.
    #[must_use]
    pub fn from_path(path: &Utf8Path) -> Option<Self> {
        if path.file_name() == Some("package.json") {
            return Some(Self::PackageManifest);
        }
        match path.extension() {
            Some("js" | "jsx" | "mjs" | "cjs") => Some(Self::JavaScript),
            Some("json" | "jsonc") => Some(Self::Json),
            Some("css" | "scss" | "less") => Some(Self::Style),
            Some("ts" | "tsx" | "mts" | "cts") => Some(Self::TypeScript),
            _ => None,
        }
    }

    /// Whether this file is Flow, and so uf's own to parse, lint and print.
    ///
    /// The other kinds are discovered so that `uf fmt` can hand them to a
    /// formatter that understands them; nothing else in uf reads them.
    #[must_use]
    pub const fn is_flow(self) -> bool {
        match self {
            Self::JavaScript => true,
            Self::PackageManifest | Self::Json | Self::Style | Self::TypeScript => false,
        }
    }

    /// Whether `uf fmt` may rewrite a file of this kind with its own printer.
    ///
    /// A `match` rather than a comparison, so a new kind cannot default into
    /// being formattable by omission.
    #[must_use]
    pub const fn is_formattable(self) -> bool {
        match self {
            Self::JavaScript => true,
            Self::PackageManifest | Self::Json | Self::Style | Self::TypeScript => false,
        }
    }

    /// Whether `uf fmt` hands a file of this kind to the non-Flow formatter.
    ///
    /// `package.json` is deliberately excluded. uf writes it during
    /// `uf install` and `uf create`, a formatter would reorder or re-indent
    /// what uf just wrote, and the two would fight on every run.
    #[must_use]
    pub const fn is_non_flow_formattable(self) -> bool {
        match self {
            Self::Json | Self::Style | Self::TypeScript => true,
            Self::JavaScript | Self::PackageManifest => false,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("refusing to overwrite {0}; pass --force to replace generated files")]
    Exists(Utf8PathBuf),
    #[error("failed to write {path}: {source}")]
    Write {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read {path}: {source}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to walk {path}: {source}")]
    Walk {
        path: Utf8PathBuf,
        /// `std::io::Error` rather than the walker's own: the walk honours
        /// `.gitignore` now, so its error type is the ignore crate's, and
        /// leaking either one would make the crate that walks a detail of this
        /// crate's public error.
        #[source]
        source: std::io::Error,
    },
}

pub fn create_project(
    root: &Utf8Path,
    options: &CreateOptions,
) -> Result<CreateReport, ProjectError> {
    let files = match options.kind {
        CreateKind::AppReact => app_react_files(&options.name),
        CreateKind::Lib => lib_files(&options.name),
    };

    let mut written = Vec::with_capacity(files.len());
    for (path, contents) in files {
        let target = root.join(path);
        write_generated_file(&target, &contents, options.force)?;
        written.push(target);
    }

    Ok(CreateReport {
        root: root.to_path_buf(),
        files: written,
    })
}

/// Every source file under `root`, and every one that could not be read.
///
/// # Errors
///
/// Returns [`ProjectError::Walk`] when `root` itself cannot be read — a
/// directory *below* it that cannot be read is reported in
/// [`SourceScan::unreadable`] instead — and [`ProjectError::Read`] when a path
/// is not valid UTF-8, because a path uf cannot name is one it cannot report
/// either.
pub fn scan_source_files(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<SourceScan, ProjectError> {
    scan_selected_source_files(root, config, &[])
}

/// The same walk, told which paths the caller asked for by name.
///
/// # Why naming a path overrides `.gitignore`
///
/// `.gitignore` says which files are not the project's *source*. It does not
/// say which files a person may ask about, and those are different questions:
/// a generated file is exactly the thing somebody points at when they want to
/// know why it will not compile.
///
/// This is not hypothetical. `tests/library/module-mock.test.js` writes a
/// fixture into a gitignored directory — deliberately, so a run killed half way
/// through does not leave it in the workspace — and then asks
/// `uf check tests/library/<fixture>` about it by name. Applying the ignore to
/// an explicitly named path answered "no diagnostics" for a file that has one,
/// which is the worst shape of wrong: silence that reads as success.
///
/// Every tool in this class draws the line here. `rg`, `prettier` and `biome`
/// all read a path you name and skip one you did not.
///
/// `.uf` and `.git` are *not* suspended by naming them: uf's own working
/// directory and somebody else's repository do not become this project's source
/// by being pointed at.
///
/// # Errors
///
/// The same as [`scan_source_files`].
pub fn scan_selected_source_files(
    root: &Utf8Path,
    config: &UniflowedConfig,
    selected: &[String],
) -> Result<SourceScan, ProjectError> {
    let mut files = Vec::new();
    let mut unreadable = Vec::new();
    // `.gitignore` is the list the project already keeps of what is not its
    // source. A generated file is generated whichever command is asking, so
    // honouring it here makes `uf fmt`, `uf lint`, `uf check` and `uf test`
    // agree about what the project is — by construction rather than by four
    // configuration keys somebody has to keep in step (ubugeeei-prod/uf#483).
    //
    // Three of the walker's defaults are turned off on purpose:
    //
    // * **the global gitignore**, because a developer's personal `~/.config/
    //   git/ignore` must not change what `uf fmt --check` says. Two people on
    //   one repository have to get the same answer.
    // * **parent directories**, because a `.gitignore` above the project root
    //   belongs to whatever the project is sitting inside, and a checkout in
    //   somebody's `~/ignored-scratch/` is still a project.
    // * **hidden files**, because uf walked them before this and stopping
    //   would be a second change hiding inside the first. `.uf` and `.git` are
    //   excluded by name below, which is what actually mattered.
    //
    // A directory holding a `.git` is another repository — a submodule, or a
    // checkout that happens to live inside this one. Its contents are not this
    // project's to read, and formatting them writes into somebody else's
    // history: `uf fmt` reformatted the vendored Flow sources, and the next
    // submodule sync would have thrown the result away.
    let root_path = root.as_std_path().to_path_buf();
    let walk = WalkBuilder::new(root)
        .hidden(false)
        .git_global(false)
        .parents(false)
        // Without a `.git` present too: a tarball of a project has the same
        // `.gitignore` and the same generated files, and should get the same
        // answer as the checkout it came from.
        .require_git(false)
        .filter_entry(move |entry| {
            entry.path() == root_path
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !entry.path().join(".git").exists()
        })
        .build();
    for entry in walk {
        let entry = match entry {
            Ok(entry) => entry,
            // A directory uf cannot open is reported the way a file it cannot
            // read is. One `chmod 000` directory anywhere under the root — a
            // colleague's scratch checkout, a cache another tool wrote — used
            // to abort the whole walk, so `uf fmt`, `uf lint`, `uf check` and
            // `uf test` did nothing for the rest of the project. That is the
            // same failure as the non-UTF-8 file above, one level up.
            //
            // The root itself stays fatal, as does an error with no path to
            // name: a root that cannot be read is not a project with one bad
            // directory in it, and answering "0 files" for a path that does
            // not exist is worse than saying so.
            Err(error) => {
                let reason = error.to_string();
                let named = match &error {
                    ignore::Error::WithPath { path, .. } => Some(path.clone()),
                    _ => None,
                }
                .filter(|path| path != root.as_std_path());
                let Some(path) = named else {
                    return Err(ProjectError::Walk {
                        path: root.to_path_buf(),
                        source: error.into_io_error().unwrap_or_else(|| {
                            std::io::Error::other("the project could not be walked")
                        }),
                    });
                };
                let relative = path.strip_prefix(root.as_std_path()).unwrap_or(&path);
                unreadable.push(UnreadableFile {
                    relative_path: relative.display().to_string(),
                    reason,
                });
                continue;
            }
        };
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()).map_err(|path| {
            ProjectError::Read {
                path: Utf8PathBuf::from(path.display().to_string()),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "path is not UTF-8"),
            }
        })?;

        if !entry.file_type().is_some_and(|kind| kind.is_file()) || is_ignored(root, &path, config)
        {
            continue;
        }
        let Some(kind) = SourceKind::from_path(&path) else {
            continue;
        };

        let relative_path = path
            .strip_prefix(root)
            .map(|path| path.as_str().to_string())
            .unwrap_or_else(|_| path.as_str().to_string());

        // Recorded rather than returned. One file that is not UTF-8 — a build
        // artifact, a vendored blob, a fixture committed by accident — used to
        // stop the walk, so nothing else in the project was formatted, linted
        // or checked either.
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                unreadable.push(UnreadableFile {
                    relative_path,
                    reason: error.to_string(),
                });
                continue;
            }
        };

        files.push(ProjectFile {
            absolute_path: path,
            relative_path,
            source,
            kind,
        });
    }
    // A second pass over the paths the caller named, with the ignore files off.
    // Two walks rather than one, because the walker applies `.gitignore` as it
    // descends and there is no per-path way to suspend it — and because a
    // reader can see what each of the two is for.
    for named in selected {
        let start = root.join(named.trim_start_matches("./"));
        if !start.exists() {
            continue;
        }
        let root_path = root.as_std_path().to_path_buf();
        let walk = WalkBuilder::new(&start)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false)
            .filter_entry(move |entry| {
                entry.path() == root_path
                    || !entry.file_type().is_some_and(|kind| kind.is_dir())
                    || !entry.path().join(".git").exists()
            })
            .build();
        for entry in walk.flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Ok(path) = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()) else {
                continue;
            };
            // `ALWAYS_IGNORED` and `lint.ignore` still apply: naming a path
            // says "this one too", not "everything uf knows to stay out of".
            let Some(kind) = SourceKind::from_path(&path) else {
                continue;
            };
            if is_ignored(root, &path, config) {
                continue;
            }
            let relative_path = path
                .strip_prefix(root)
                .map(|path| path.as_str().to_string())
                .unwrap_or_else(|_| path.as_str().to_string());
            if files
                .iter()
                .any(|file: &ProjectFile| file.relative_path == relative_path)
            {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(source) => files.push(ProjectFile {
                    kind,
                    absolute_path: path,
                    relative_path,
                    source,
                }),
                Err(error) => unreadable.push(UnreadableFile {
                    relative_path,
                    reason: error.to_string(),
                }),
            }
        }
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    files.dedup_by(|a, b| a.relative_path == b.relative_path);
    unreadable.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    unreadable.dedup_by(|a, b| a.relative_path == b.relative_path);
    Ok(SourceScan { files, unreadable })
}

fn write_generated_file(path: &Utf8Path, contents: &str, force: bool) -> Result<(), ProjectError> {
    if path.exists() && !force {
        return Err(ProjectError::Exists(path.to_path_buf()));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ProjectError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(path, contents).map_err(|source| ProjectError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Directories no project owns, whatever the configuration says.
///
/// `.uf` is uf's own working directory — the transform cache, compiled
/// configs, build output — and a project cannot opt back into having its
/// tooling's scratch files linted, formatted or run as tests. The others are
/// removable from `lint.ignore`, which is why they are not here.
const ALWAYS_IGNORED: &[&str] = &[".uf", ".git"];

/// Whether a path is excluded from linting, formatting and test discovery.
///
/// An ignore entry is read one of two ways, chosen by whether it contains a
/// separator. A bare name — `dist`, `node_modules`, `target` — names a kind of
/// directory and matches wherever it appears, because a build directory is
/// still a build directory two levels down: this project's own documentation
/// builds into `docs/dist`, and a root-anchored `dist` did not cover it, so
/// `uf fmt` walked into generated bundles and offered to reformat them. A
/// path — `src/generated`, `packages/legacy/vendor` — names one place and is
/// matched as a prefix, which is what someone writing a path means.
fn is_ignored(root: &Utf8Path, path: &Utf8Path, config: &UniflowedConfig) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).as_str();
    let mut segments = relative.split('/');
    if segments.any(|segment| ALWAYS_IGNORED.contains(&segment)) {
        return true;
    }
    config.lint.ignore.iter().any(|ignored| {
        let ignored = ignored.as_str();
        if ignored.contains('/') {
            relative.starts_with(ignored)
        } else {
            relative.split('/').any(|segment| segment == ignored)
        }
    })
}

#[cfg(test)]
mod tests;
