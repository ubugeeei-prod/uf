//! Paths, virtual file systems and glob matching.
//!
//! Covers the `vfs`, `fs`, `path` and `glob` std modules: what a host file
//! system is allowed to expose, how a virtual path is spelled, and the slash
//! normalisation every other module joins paths through.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// File-system capability exposed by `@uniflowed/std/fs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FsCapability {
    /// Async read operations.
    Read,
    /// Async write operations.
    Write,
    /// Directory traversal.
    Walk,
    /// File watching.
    Watch,
    /// Atomic rename and replace operations.
    AtomicRename,
}

/// Virtual file-system entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VfsEntryKind {
    /// Virtual file.
    File,
    /// Virtual directory.
    Directory,
}

/// Normalized virtual path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPath {
    /// Slash-normalized path text.
    pub path: CompactString,
}

impl VirtualPath {
    /// Create a slash-normalized virtual path.
    pub fn new(path: &str) -> Self {
        Self {
            path: CompactString::from(path.replace('\\', "/")),
        }
    }
}

/// Join path segments with slash normalization.
pub fn join_path(parts: &[&str]) -> CompactString {
    let mut output = CompactString::new("");
    for part in parts {
        let trimmed = part.trim_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('/');
        }
        output.push_str(trimmed);
    }
    output
}

/// Normalize slash separators and remove `.` path segments.
pub fn normalize_path(path: &str) -> CompactString {
    let replaced = path.replace('\\', "/");
    let mut segments = SmallVec::<[&str; 16]>::new();
    for segment in replaced.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            value => segments.push(value),
        }
    }
    join_path(&segments)
}

/// Glob pattern descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobPattern {
    /// Original pattern.
    pub pattern: CompactString,
    /// Whether dotfiles are matched.
    pub dotfiles: bool,
}

impl GlobPattern {
    /// Create a glob pattern descriptor.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_compact_string(),
            dotfiles: false,
        }
    }

    /// Match the subset of glob syntax used for fast include filters.
    pub fn matches(&self, path: &str) -> bool {
        match self.pattern.split_once('*') {
            Some((prefix, suffix)) => path.starts_with(prefix) && path.ends_with(suffix),
            None => self.pattern == path,
        }
    }
}
