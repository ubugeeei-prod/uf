//! Cache collection preserves project inputs and runtime links.
use crate::ui::Ui;
use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs;
use uf_term::Status;

pub(crate) fn collect(cwd: &Utf8Path, ui: &mut Ui, global: bool, dry_run: bool) -> Result<()> {
    let root = uf_config::discover_root(cwd);
    let mut projects = vec![root];
    if global {
        for (_, registered) in uf_env::Roots::discover()?.all()? {
            if registered.is_live() {
                projects.push(registered.repository);
            }
        }
    }
    projects.sort();
    projects.dedup();
    let mut candidates = Vec::new();
    for project in &projects {
        add_cache(&mut candidates, project, ".uf/cache")?;
        add_cache(&mut candidates, project, ".uf/exec-cache")?;
    }
    if global {
        let cache = uf_env::index::cache_dir()?;
        if let Some(parent) = cache.parent() {
            if let Some(name) = cache.file_name() {
                add_cache(&mut candidates, parent, name)?;
            }
        }
        candidates.extend(super::toolchain::gc::candidates(&projects)?);
    }
    let environment = if global {
        let store = uf_env::Store::discover()?;
        let roots = uf_env::Roots::discover()?;
        let plan = uf_env::gc::plan(&store, &roots)?;
        Some((store, roots, plan))
    } else {
        None
    };
    // Build the complete plan before touching anything. Broken references abort it.
    ui.render(|renderer, out| {
        renderer.banner(out, "uf gc", None);
        for path in &candidates {
            renderer.status(out, Status::Info, path.as_str());
        }
    });
    if !dry_run {
        for path in &candidates {
            ensure!(
                fs::symlink_metadata(path)?.is_dir(),
                "collection target changed; run uf gc again"
            );
            fs::remove_dir_all(path).with_context(|| uf_infra::cstr!("cannot collect {path}"))?;
        }
    }
    if let Some((store, roots, plan)) = environment {
        // The existing environment collector keeps every live project's tool pins.
        for entry in &plan.unreachable {
            ui.render(|renderer, out| renderer.status(out, Status::Info, entry));
        }
        if !dry_run {
            uf_env::gc::collect(&store, &roots, &plan)?;
        }
    }
    let message = if dry_run {
        "dry run: no files removed"
    } else {
        "garbage collection complete"
    };
    ui.render(|renderer, out| renderer.status(out, Status::Success, message));
    Ok(())
}

fn add_cache(targets: &mut Vec<Utf8PathBuf>, root: &Utf8Path, relative: &str) -> Result<()> {
    let target = root.join(relative);
    if !target.try_exists()? {
        return Ok(());
    }
    // Refuse symlinked ancestors as well as symlinked cache directories.
    let mut cursor = root.to_path_buf();
    for component in Utf8Path::new(relative).components() {
        cursor.push(component.as_str());
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Ok(());
        }
    }
    ensure!(
        target
            .canonicalize_utf8()?
            .starts_with(root.canonicalize_utf8()?),
        "cache escapes its project"
    );
    if fs::symlink_metadata(&target)?.is_dir() {
        targets.push(target);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collects_only_cache_and_keeps_profile_and_locks() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::create_dir_all(root.join(".uf/cache/check")).unwrap();
        fs::write(root.join(".uf/profile"), "staging").unwrap();
        fs::write(root.join("uf.lock"), "{}").unwrap();
        let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);
        collect(root, &mut ui, false, true).unwrap();
        assert!(root.join(".uf/cache/check").exists());
        collect(root, &mut ui, false, false).unwrap();
        assert!(!root.join(".uf/cache").exists());
        assert_eq!(
            fs::read_to_string(root.join(".uf/profile")).unwrap(),
            "staging"
        );
        assert!(root.join("uf.lock").exists());
    }
    #[cfg(unix)]
    #[test]
    fn does_not_follow_cache_or_state_symlinks() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(project.path()).unwrap();
        fs::create_dir_all(outside.path().join("cache")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join(".uf")).unwrap();
        let mut targets = Vec::new();
        add_cache(&mut targets, root, ".uf/cache").unwrap();
        assert!(targets.is_empty());
        assert!(outside.path().join("cache").exists());
    }
}
