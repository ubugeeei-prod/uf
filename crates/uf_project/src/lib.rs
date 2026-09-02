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

        if !entry.file_type().is_file() || !is_source_file(&path) || is_ignored(root, &path, config)
        {
            continue;
        }

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

fn is_source_file(path: &Utf8Path) -> bool {
    matches!(path.file_name(), Some("package.json"))
        || matches!(
            path.extension(),
            Some("flow" | "js" | "jsx" | "mjs" | "cjs")
        )
}

fn is_ignored(root: &Utf8Path, path: &Utf8Path, config: &UniflowedConfig) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).as_str();
    config
        .lint
        .ignore
        .iter()
        .any(|ignored| relative.starts_with(ignored.as_str()))
}

#[cfg(test)]
mod tests;
