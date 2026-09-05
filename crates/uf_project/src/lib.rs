use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

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

/// A file discovery found and could not read.
///
/// One stray byte should not stop a project. A `.js` that is not UTF-8 — a
/// build artifact, a vendored blob, a fixture somebody committed by accident
/// — used to abort `uf fmt`, `uf lint`, `uf check` and `uf doc` at the first
/// one, leaving every other file in the project untouched.
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
        #[source]
        source: walkdir::Error,
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
/// Returns [`ProjectError::Walk`] when the directory tree cannot be read, and
/// [`ProjectError::Read`] when a path is not valid UTF-8 — a path uf cannot
/// name is one it cannot report either.
pub fn scan_source_files(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<SourceScan, ProjectError> {
    let mut files = Vec::new();
    let mut unreadable = Vec::new();
    // A directory holding a `.git` is another repository — a submodule, or a
    // checkout that happens to live inside this one. Its contents are not this
    // project's to read, and formatting them writes into somebody else's
    // history: `uf fmt` reformatted the vendored Flow sources, and the next
    // submodule sync would have thrown the result away.
    let walk = WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry.path() == root.as_std_path()
            || !entry.file_type().is_dir()
            || !entry.path().join(".git").exists()
    });
    for entry in walk {
        let entry = entry.map_err(|source| ProjectError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()).map_err(|path| {
            ProjectError::Read {
                path: Utf8PathBuf::from(path.display().to_string()),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "path is not UTF-8"),
            }
        })?;

        if !entry.file_type().is_file() || is_ignored(root, &path, config) {
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
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    unreadable.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
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
