use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_config::PackageManagerPreference;

use super::*;

mod field;
mod lockfile;
mod manager;
mod manifest;
mod options;
mod precedence;
mod workspace;

fn temp_root() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, root)
}

fn write(path: &Utf8Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

/// Detect inside a hermetic tree: the boundary keeps the ancestor walk from
/// noticing lockfiles that happen to live above the system temp directory.
fn detect_within(root: &Utf8Path, start: &Utf8Path) -> Detection {
    detect_package_manager_with(start, &DetectionOptions::new().with_boundary(root))
}
