//! Unused uf releases, retaining executable links, rollback and project pins.
use super::{BINARIES, OWN_VERSION, Store, is_version, recorded_previous};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use std::{collections::BTreeSet, fs};

pub(crate) fn candidates(projects: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>> {
    let store = Store::from_process();
    let mut keep = BTreeSet::from([OWN_VERSION.to_owned()]);
    if let Some(previous) = recorded_previous(&store) {
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
