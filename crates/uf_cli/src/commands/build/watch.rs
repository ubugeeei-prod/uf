//! Library watch builds run the ordinary build again, including declarations,
//! budgets and the manifest. A failed rebuild stays visible and can be repaired.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{DeployAdapter, LibraryPlan};
use walkdir::WalkDir;

use super::BuildReports;
use crate::commands::vite::load_project_config;
use crate::support::PRODUCTION;
use crate::ui::Ui;

pub(crate) fn watch(
    cwd: &Utf8Path,
    ui: &mut Ui,
    reports: BuildReports,
    mode: Option<&str>,
    standalone: bool,
    target: Option<&str>,
    adapter: Option<DeployAdapter>,
) -> Result<()> {
    let resolved = load_project_config(cwd, mode, PRODUCTION)?;
    let Some(plan) = LibraryPlan::resolve(&resolved.config) else {
        bail!("`uf build --watch` builds libraries; use `uf dev` for an application");
    };
    super::refuse_an_application_artefact(
        &plan,
        standalone,
        adapter.or(resolved.config.app.runtime.deploy.adapter),
        target,
    )?;
    if reports.analyze {
        bail!("`uf build --analyze` needs application routes; use `--size-report` for a library");
    }
    let root = resolved.root;
    let output = root.join(&resolved.config.build.out_dir);
    let mut previous = snapshot(&root, &output)?;
    loop {
        if let Err(error) = super::build(cwd, ui, reports, mode, false, target, adapter) {
            eprintln!("uf: rebuild failed: {error:#}");
        }
        eprintln!("uf: watching library sources; press Ctrl+C to stop");
        loop {
            std::thread::sleep(Duration::from_millis(250));
            match snapshot(&root, &output) {
                Ok(current) if current != previous => {
                    previous = current;
                    break;
                }
                Ok(_) => {}
                Err(error) => eprintln!("uf: could not inspect library sources: {error:#}"),
            }
        }
    }
}

// Content rather than timestamps also catches rapid same-sized edits. The
// output and tool caches cannot trigger a rebuild of the build that wrote them.
fn snapshot(root: &Utf8Path, output: &Utf8Path) -> Result<BTreeMap<Utf8PathBuf, u64>> {
    let mut files = BTreeMap::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry.path() != output.as_std_path()
            && !matches!(
                entry.file_name().to_str(),
                Some("node_modules" | ".git" | ".uf" | "target")
            )
    }) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| anyhow::anyhow!("non-UTF-8 library path: {}", path.display()))?;
        let bytes = std::fs::read(&path).with_context(|| format!("failed to read {path}"))?;
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        files.insert(path, hasher.finish());
    }
    Ok(files)
}
