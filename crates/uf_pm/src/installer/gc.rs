//! Collect unreferenced shared package trees and files while respecting live roots.
use super::{Graph, store};
use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
#[cfg(unix)]
use std::collections::BTreeMap;
use std::{collections::BTreeSet, fs};

/// Shared store candidates. Active projects and files with other hardlinks stay.
pub struct GcPlan {
    /// Paths named by the dry-run and considered for collection.
    pub paths: Vec<Utf8PathBuf>,
    root: Utf8PathBuf,
}

/// Plan collection without removing project dependencies or fetching metadata.
pub fn gc_plan(projects: &[Utf8PathBuf]) -> Result<GcPlan> {
    let store = store::Store::discover()?;
    let _guard = uf_env::lock::guard(&store.root.join("gc"))?;
    plan_in(&store, projects)
}

fn plan_in(store: &store::Store, projects: &[Utf8PathBuf]) -> Result<GcPlan> {
    for directory in ["packages", "files"] {
        let path = store.root.join(directory);
        ensure!(
            !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()),
            "shared store directory is a symlink; collection refused"
        );
    }
    let mut keep = BTreeSet::new();
    let mut live = projects.to_vec();
    let mut dead = Vec::new();
    for (record, root) in uf_env::Roots::new(store.root.join("roots")).all()? {
        if root.is_live() {
            keep.extend(root.entries);
            live.push(root.repository);
        } else {
            dead.push(record);
        }
    }
    live.sort();
    live.dedup();
    for project in live {
        if !project.is_dir() {
            continue;
        }
        let config = match uf_config::discover_config(&project) {
            Some(path) => uf_config::load_config_file(&path)?,
            None => uf_config::UniflowedConfig::default(),
        };
        let lock = project.join(config.pm.lockfile.as_str());
        match fs::read(lock) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .context("cannot inspect a live project's lockfile for GC")?;
                if let Some(native) = value.get("native") {
                    let graph: Graph = serde_json::from_value(native.clone())?;
                    keep.extend(
                        graph
                            .nodes
                            .values()
                            .filter(|node| node.local.is_none())
                            .map(store::package_key),
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        keep.iter()
            .all(|key| key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "invalid shared store reference; collection refused"
    );
    let mut paths = dead;
    let packages = store.root.join("packages");
    if packages.is_dir() {
        for entry in fs::read_dir(packages)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("non UTF-8 store entry")?;
            if entry.file_type()?.is_dir() && name.len() == 64 && !keep.contains(name) {
                paths.push(
                    Utf8PathBuf::from_path_buf(entry.path())
                        .map_err(|_| anyhow::anyhow!("non UTF-8 store entry"))?,
                );
            }
        }
    }
    #[cfg(unix)]
    paths.extend(file_candidates(&store.root.join("files"), &paths)?);
    paths.sort();
    Ok(GcPlan {
        paths,
        root: store.root.clone(),
    })
}

/// Recheck live roots before collecting the plan; new references win over deletion.
pub fn gc_collect(plan: &GcPlan, projects: &[Utf8PathBuf]) -> Result<()> {
    let store = store::Store {
        root: plan.root.clone(),
    };
    let _guard = uf_env::lock::guard(&store.root.join("gc"))?;
    let current = plan_in(&store, projects)?;
    let files = store.root.join("files");
    for path in plan.paths.iter().filter(|path| !path.starts_with(&files)) {
        if !current.paths.contains(path) {
            continue;
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else if metadata.is_file() {
            fs::remove_file(path)?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for path in plan.paths.iter().filter(|path| path.starts_with(&files)) {
            if current.paths.contains(path)
                && fs::symlink_metadata(path)
                    .is_ok_and(|metadata| metadata.is_file() && metadata.nlink() == 1)
            {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn file_candidates(directory: &Utf8Path, packages: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>> {
    use std::os::unix::fs::MetadataExt;
    let mut removed = BTreeMap::<(u64, u64), u64>::new();
    let mut pending: Vec<_> = packages
        .iter()
        .filter(|path| path.is_dir())
        .cloned()
        .collect();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(
                    Utf8PathBuf::from_path_buf(entry.path())
                        .map_err(|_| anyhow::anyhow!("non UTF-8 package directory"))?,
                );
            } else if entry.file_type()?.is_file() {
                let metadata = entry.metadata()?;
                *removed.entry((metadata.dev(), metadata.ino())).or_default() += 1;
            }
        }
    }
    let mut candidates = Vec::new();
    if !directory.is_dir() {
        return Ok(candidates);
    }
    for shard in fs::read_dir(directory)? {
        let shard = shard?;
        if !shard.file_type()?.is_dir() {
            continue;
        }
        for entry in fs::read_dir(shard.path())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let metadata = entry.metadata()?;
            if metadata.nlink()
                == 1 + removed
                    .get(&(metadata.dev(), metadata.ino()))
                    .copied()
                    .unwrap_or(0)
            {
                candidates.push(
                    Utf8PathBuf::from_path_buf(entry.path())
                        .map_err(|_| anyhow::anyhow!("non UTF-8 shared file"))?,
                );
            }
        }
    }
    Ok(candidates)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn collects_orphans_but_rechecks_new_roots_and_preserves_project_hardlinks() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let store = store::Store {
            root: root.join("store"),
        };
        let key = "a".repeat(64);
        let package = store.root.join("packages").join(&key).join("package");
        let files = store.root.join("files/aa");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&files).unwrap();
        let data = files.join("content");
        fs::write(&data, "shared bytes").unwrap();
        fs::hard_link(&data, package.join("index.js")).unwrap();
        let plan = plan_in(&store, &[]).unwrap();
        assert!(plan.paths.contains(&data));
        assert!(
            plan.paths
                .contains(&package.parent().unwrap().to_path_buf())
        );
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        uf_env::Roots::new(store.root.join("roots"))
            .register(&project, std::slice::from_ref(&key))
            .unwrap();
        gc_collect(&plan, &[]).unwrap();
        assert!(package.join("index.js").exists());
        assert!(data.exists());
        fs::hard_link(&data, project.join("installed.js")).unwrap();
        uf_env::Roots::new(store.root.join("roots"))
            .register(&project, &[])
            .unwrap();
        let plan = plan_in(&store, &[]).unwrap();
        assert!(!plan.paths.contains(&data));
        gc_collect(&plan, &[]).unwrap();
        assert!(!package.exists());
        assert_eq!(fs::metadata(&data).unwrap().nlink(), 2);
        assert_eq!(
            fs::read_to_string(project.join("installed.js")).unwrap(),
            "shared bytes"
        );
        fs::remove_file(project.join("installed.js")).unwrap();
        let plan = plan_in(&store, &[]).unwrap();
        assert!(plan.paths.contains(&data));
        gc_collect(&plan, &[]).unwrap();
        assert!(!data.exists());
    }

    #[test]
    fn refuses_symlinked_store_subdirectories_and_unknown_live_locks() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let store = store::Store {
            root: root.join("store"),
        };
        fs::create_dir(&store.root).unwrap();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, store.root.join("packages")).unwrap();
        assert!(plan_in(&store, &[]).is_err());
        fs::remove_file(store.root.join("packages")).unwrap();
        fs::write(outside.join("uf.lock"), "{").unwrap();
        assert!(plan_in(&store, &[outside]).is_err());
    }
}
