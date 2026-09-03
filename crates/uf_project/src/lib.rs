use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;
use uf_config::UniflowedConfig;
use walkdir::WalkDir;

mod template;

use template::{app_react_files, lib_files};

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

/// What a discovered project file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    /// Flow-typed JavaScript: `.js`, `.jsx`, `.mjs`, `.cjs`.
    JavaScript,
    /// A `package.json` manifest. Read by the linter, never rewritten.
    PackageManifest,
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
            _ => None,
        }
    }

    /// Whether `uf fmt` may rewrite a file of this kind.
    ///
    /// A `match` rather than a comparison, so a new kind cannot default into
    /// being formattable by omission.
    #[must_use]
    pub const fn is_formattable(self) -> bool {
        match self {
            Self::JavaScript => true,
            Self::PackageManifest => false,
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

pub fn collect_source_files(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<ProjectFile>, ProjectError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root) {
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

        let source = fs::read_to_string(&path).map_err(|source| ProjectError::Read {
            path: path.clone(),
            source,
        })?;
        let relative_path = path
            .strip_prefix(root)
            .map(|path| path.as_str().to_string())
            .unwrap_or_else(|_| path.as_str().to_string());

        files.push(ProjectFile {
            absolute_path: path,
            relative_path,
            source,
            kind,
        });
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
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
