//! Unused uf releases, retaining executable links, rollback and project pins.
use super::{BINARIES, OWN_VERSION, Store, is_version, recorded_previous};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use std::{collections::BTreeSet, fs};

pub(crate) fn candidates(projects: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>> {
    candidates_in(&Store::from_process(), projects)
}

fn candidates_in(store: &Store, projects: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>> {
    let mut keep = BTreeSet::from([OWN_VERSION.to_owned()]);
    if let Some(previous) = recorded_previous(store) {
        keep.insert(previous);
    }
    for project in projects {
        // An unreadable pin is an unknown live reference: refuse collection.
        if let Some(path) = uf_config::discover_config(project) {
            let config = uf_config::load_config_file(&path)?;
            if let Some(pin) = config.uf {
                keep.insert(pin.to_string());
            }
        }
    }
    let mut live_paths = Vec::new();
    if let Ok(path) = std::env::current_exe() {
        live_paths.push(path.canonicalize()?);
    }
    for binary in BINARIES {
        let path = store.bin_dir.join(super::binary_file(binary));
        if let Ok(path) = path.canonicalize() {
            live_paths.push(path);
        }
    }
    let mut candidates = Vec::new();
    if !store.runtimes.exists() {
        return Ok(candidates);
    }
    for entry in fs::read_dir(&store.runtimes).context("cannot inspect installed uf versions")? {
        let entry = entry?;
        let path = Utf8PathBuf::from_path_buf(entry.path())
            .map_err(|_| anyhow::anyhow!("non UTF-8 path in uf runtime store"))?;
        let name = entry.file_name();
        let Some(version) = name.to_str().and_then(|name| name.strip_prefix("uf@")) else {
            continue;
        };
        if !entry.file_type()?.is_dir() || !is_version(version) || keep.contains(version) {
            continue;
        }
        let canonical = path.canonicalize()?;
        if !live_paths.iter().any(|live| live.starts_with(&canonical)) {
            candidates.push(path);
        }
    }
    candidates.sort();
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[test]
    fn retains_current_previous_and_live_project_pins() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let store = Store {
            runtimes: root.join("runtimes"),
            bin_dir: root.join("bin"),
            state_dir: root.join("state"),
        };
        fs::create_dir_all(store.root()).unwrap();
        fs::write(store.previous_record(), "0.1.0\n").unwrap();
        for version in [OWN_VERSION, "0.1.0", "0.2.0", "0.3.0"] {
            fs::create_dir_all(store.version_dir(version)).unwrap();
        }
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("uf.config.js"),
            "export default { uf: '0.2.0' };",
        )
        .unwrap();
        assert_eq!(
            candidates_in(&store, std::slice::from_ref(&project)).unwrap(),
            vec![store.version_dir("0.3.0")]
        );
        fs::write(project.join("uf.config.js"), "export default {").unwrap();
        assert!(candidates_in(&store, &[project]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn retains_every_active_binary_target_and_skips_symlinked_versions() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let store = Store {
            runtimes: root.join("runtimes"),
            bin_dir: root.join("bin"),
            state_dir: root.join("state"),
        };
        fs::create_dir_all(&store.bin_dir).unwrap();
        let pinned = store.version_dir("0.1.0").join("bin");
        fs::create_dir_all(&pinned).unwrap();
        fs::write(pinned.join("ufr"), "binary").unwrap();
        std::os::unix::fs::symlink(pinned.join("ufr"), store.bin_dir.join("ufr")).unwrap();
        std::os::unix::fs::symlink(root, store.version_dir("0.9.0")).unwrap();
        assert!(candidates_in(&store, &[]).unwrap().is_empty());
    }
}
