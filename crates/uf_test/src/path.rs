//! Project-relative paths, and the rules that keep one inside the project.
//!
//! Two untrusted sources hand this crate paths: the recorded timings file on
//! disk, and the import specifiers inside source files. Both are attacker
//! controllable in the usual supply-chain way — a dependency, a generated file,
//! a checked-in cache — and both are used to decide which files to open, stat
//! and watch. Everything from either source goes through [`normalize_relative`]
//! or [`is_safe_relative`] first, which is the guard against the path traversal
//! class (`../../../../etc/passwd`) and against absolute-path escapes.

use compact_str::CompactString;

/// Longest project-relative path accepted.
///
/// Long enough for any real repository, short enough that a hostile timings
/// file cannot use path keys to allocate without bound.
pub const MAX_RELATIVE_PATH_BYTES: usize = 4_096;

/// Whether `path` is a project-relative path this crate will act on.
///
/// Rejects, in order: an empty path, an over-long path, an embedded NUL, a
/// backslash (a Windows separator or drive path smuggled through a POSIX
/// string), an absolute path, a Windows drive prefix, any `.` or `..` segment,
/// and any empty segment.
pub fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() || path.len() > MAX_RELATIVE_PATH_BYTES {
        return false;
    }
    if path.contains('\0') || path.contains('\\') {
        return false;
    }
    if path.starts_with('/') {
        return false;
    }
    if has_drive_prefix(path) {
        return false;
    }
    path.split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Whether `path` starts with a Windows drive letter such as `C:`.
fn has_drive_prefix(path: &str) -> bool {
    let mut bytes = path.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic())
        && matches!(bytes.next(), Some(b':'))
}

/// Resolve `specifier` against the directory holding `from`, returning a
/// project-relative path.
///
/// Returns [`None`] when the specifier is not relative, or when resolving it
/// would leave the project root. Popping past the root is refused rather than
/// clamped: clamping is how a traversal turns into a read of the wrong file.
pub fn normalize_relative(from: &str, specifier: &str) -> Option<CompactString> {
    if specifier.contains('\0') || specifier.contains('\\') {
        return None;
    }
    if !(specifier.starts_with("./") || specifier.starts_with("../") || specifier == ".") {
        return None;
    }

    let mut segments: Vec<&str> = Vec::new();
    if let Some(parent) = from.rsplit_once('/').map(|(parent, _)| parent) {
        segments.extend(parent.split('/').filter(|segment| !segment.is_empty()));
    }

    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    if segments.is_empty() {
        return None;
    }
    let joined = segments.join("/");
    if joined.len() > MAX_RELATIVE_PATH_BYTES || !is_safe_relative(&joined) {
        return None;
    }
    Some(CompactString::from(joined))
}
