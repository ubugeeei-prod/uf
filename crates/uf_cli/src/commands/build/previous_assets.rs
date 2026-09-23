//! The previous build's hashed files, kept in the output directory for one
//! more build.
//!
//! A tab opened on build N keeps asking for build N's chunks: a route it has
//! not visited yet, a client component a payload names, a stylesheet a lazy
//! module imports. Every one of those is a hashed file — `assets/page-3kd9.js`
//! — and Vite empties the output directory before it writes build N+1, so the
//! moment N+1 is deployed each of those requests is a 404, and the page in
//! that tab breaks at whatever the reader clicks next.
//!
//! So `uf build` carries them forward. Before the builder runs, the files the
//! previous build's `.vite/manifest.json` names are linked aside; after the
//! size report, every one of them the new build did not write itself is put
//! back. A tab on build N then keeps working for as long as build N+1 is live,
//! and the browser's half of skew protection (`@uniflowed/router`'s
//! `internal/deployment.js`) turns everything else it might ask for — an
//! action, a payload — into a hard navigation onto N+1.
//!
//! # The window is exactly one build
//!
//! Only files the previous build's *own* manifest names are carried, and the
//! files carried into build N+1 are in nobody's manifest — so build N+2 drops
//! them. A tab two deploys behind loses its chunks, and the router loads the
//! document instead of showing an error for a module that is gone. That bounds
//! the output directory at two builds' assets, and it is the promise
//! `docs/app/guide/deploy` makes in writing.
//!
//! Hashed files only: a manifest names chunks, stylesheets and the assets they
//! import, never a document, a `public/` file or the manifest itself. Those
//! are the new build's to write, and a document from build N served by build
//! N+1 would be exactly the skew this exists to prevent.
//!
//! # After the size report, not before it
//!
//! The report measures what *this* build ships, and budgets are enforced on
//! it. Counting the previous build's chunks as well would double every total
//! and put a budget over on the second build of a project that changed
//! nothing. So the files come back after the report and before `--compile` and
//! `--adapter`, which copy the output directory and should carry them too.
//!
//! # What it does not do
//!
//! Anything across machines. A CI job that builds on a fresh checkout has no
//! previous output directory, carries nothing, and says so; a host that
//! replaces its files wholesale on deploy (Cloudflare's asset upload, a Lambda
//! package) keeps only what the upload contained. Both are covered by the
//! fallback rather than by this: a chunk that is gone is a hard navigation.

use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde_json::json;

/// Where the previous build's files wait while the builder runs.
///
/// Under `.uf/build`, beside the server bundle, on the same volume as the
/// output directory in every ordinary layout — which is what lets them be
/// hard links rather than copies.
pub(crate) const STASH_DIR: &str = ".uf/build/previous-assets";

/// The record of what the last build carried forward, in `.uf/build/meta`.
pub(crate) const CARRIED_FILE: &str = "carried-assets.json";

/// The previous build's hashed files, set aside.
#[derive(Debug, Default)]
pub(crate) struct Stashed {
    /// The directory they wait in.
    dir: Utf8PathBuf,
    /// Each file, relative to the output directory.
    files: Vec<Utf8PathBuf>,
}

#[cfg(test)]
impl Stashed {
    /// How many files were set aside.
    fn len(&self) -> usize {
        self.files.len()
    }
}

/// Set aside the files the output directory's current manifest names.
///
/// Linked rather than moved: until the builder empties the directory, it is
/// still the deployed build for anything serving it, and a build that failed
/// before the builder ran must leave it whole.
///
/// Nothing to set aside — no output directory, no manifest, a manifest that is
/// not JSON — is an empty [`Stashed`] rather than an error: it is the first
/// build of a project, or one whose previous output is not a Vite build, and
/// either way there is nothing a tab could be asking for.
pub(crate) fn stash(root: &Utf8Path, out_dir: &Utf8Path) -> Result<Stashed> {
    let dir = root.join(STASH_DIR);
    remove_dir(&dir)?;
    let mut stashed = Stashed {
        dir,
        files: Vec::new(),
    };
    for file in manifest_files(out_dir) {
        let from = out_dir.join(&file);
        if !from.is_file() {
            continue;
        }
        let to = stashed.dir.join(&file);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
        }
        link_or_copy(&from, &to)?;
        stashed.files.push(file);
    }
    Ok(stashed)
}

/// Put back every stashed file the new build did not write itself, record
/// which, and empty the stash.
///
/// Returns how many came back. A file the new build also wrote — the same
/// content hash, so the same bytes — is left as the new build wrote it.
pub(crate) fn restore(stashed: Stashed, out_dir: &Utf8Path, meta_dir: &Utf8Path) -> Result<usize> {
    let mut carried = Vec::new();
    for file in &stashed.files {
        let to = out_dir.join(file);
        if to.exists() {
            continue;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
        }
        let from = stashed.dir.join(file);
        if fs::rename(&from, &to).is_err() {
            fs::copy(&from, &to).with_context(|| format!("failed to copy {from} to {to}"))?;
        }
        carried.push(file.as_str().replace('\\', "/"));
    }
    remove_dir(&stashed.dir)?;
    fs::create_dir_all(meta_dir).with_context(|| format!("failed to create {meta_dir}"))?;
    let record = meta_dir.join(CARRIED_FILE);
    let body = json!({
        "window": "one build",
        "files": carried,
    });
    fs::write(&record, format!("{body:#}\n"))
        .with_context(|| format!("failed to write {record}"))?;
    Ok(carried.len())
}

/// Every file a Vite manifest names, relative to the output directory.
///
/// `file`, `css` and `assets` of every entry, which between them are every
/// hashed file the build emitted. A name that is not a plain relative path —
/// absolute, or climbing out with `..` — is not something Vite writes, and is
/// skipped rather than followed: this reads a file from the output directory,
/// and nothing read from there decides where uf writes.
fn manifest_files(out_dir: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(text) = fs::read_to_string(out_dir.join(".vite/manifest.json")) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(entries)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return Vec::new();
    };
    let mut files: Vec<Utf8PathBuf> = Vec::new();
    for chunk in entries.values() {
        let named = chunk
            .get("file")
            .and_then(serde_json::Value::as_str)
            .into_iter()
            .chain(
                ["css", "assets"]
                    .into_iter()
                    .filter_map(|key| chunk.get(key).and_then(serde_json::Value::as_array))
                    .flatten()
                    .filter_map(serde_json::Value::as_str),
            );
        for name in named {
            let path = Utf8PathBuf::from(name);
            if is_plain_relative(&path) && !files.contains(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Whether `path` names something inside the directory it is joined to.
fn is_plain_relative(path: &Utf8Path) -> bool {
    path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Utf8Component::Normal(_)))
}

/// A second name for `from` at `to`, or a copy where a link cannot be made.
fn link_or_copy(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).with_context(|| format!("failed to copy {from} to {to}"))?;
    Ok(())
}

fn remove_dir(dir: &Utf8Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {dir}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Utf8Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn manifest(out_dir: &Utf8Path, files: &[&str]) {
        let entries: serde_json::Map<String, serde_json::Value> = files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let css: Vec<&str> = if file.ends_with(".js") {
                    Vec::new()
                } else {
                    vec![file]
                };
                (
                    format!("entry-{index}"),
                    json!({ "file": file, "css": css }),
                )
            })
            .collect();
        write(
            &out_dir.join(".vite/manifest.json"),
            &serde_json::Value::Object(entries).to_string(),
        );
    }

    /// One build: empty the output directory the way Vite does, write
    /// `files`, and carry the previous build forward around it.
    fn build(root: &Utf8Path, files: &[&str]) -> usize {
        let out_dir = root.join("dist");
        let stashed = stash(root, &out_dir).unwrap();
        remove_dir(&out_dir).unwrap();
        for file in files {
            write(&out_dir.join(file), file);
        }
        write(&out_dir.join("index.html"), "<!doctype html>");
        manifest(&out_dir, files);
        restore(stashed, &out_dir, &root.join(".uf/build/meta")).unwrap()
    }

    fn root() -> (tempfile::TempDir, Utf8PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        (temp, root)
    }

    #[test]
    fn the_previous_builds_chunks_are_still_there_after_the_next_build() {
        let (_temp, root) = root();
        assert_eq!(build(&root, &["assets/client-n.js", "assets/page-n.js"]), 0);

        let carried = build(&root, &["assets/client-n1.js", "assets/page-n1.js"]);

        assert_eq!(carried, 2);
        for file in [
            "assets/client-n.js",
            "assets/page-n.js",
            "assets/client-n1.js",
            "assets/page-n1.js",
        ] {
            assert!(root.join("dist").join(file).is_file(), "{file} is missing");
        }
        let record = fs::read_to_string(root.join(".uf/build/meta").join(CARRIED_FILE)).unwrap();
        assert!(record.contains("assets/client-n.js"), "{record}");
        assert!(!root.join(STASH_DIR).exists(), "the stash is emptied");
    }

    #[test]
    fn a_chunk_two_builds_old_is_gone() {
        let (_temp, root) = root();
        build(&root, &["assets/page-n.js"]);
        build(&root, &["assets/page-n1.js"]);

        build(&root, &["assets/page-n2.js"]);

        let dist = root.join("dist");
        assert!(
            !dist.join("assets/page-n.js").exists(),
            "the window is one build"
        );
        assert!(dist.join("assets/page-n1.js").is_file());
        assert!(dist.join("assets/page-n2.js").is_file());
    }

    #[test]
    fn a_file_the_new_build_wrote_is_the_new_builds() {
        let (_temp, root) = root();
        build(&root, &["assets/shared.js"]);

        let carried = build(&root, &["assets/shared.js"]);

        assert_eq!(carried, 0);
        assert_eq!(
            fs::read_to_string(root.join("dist/assets/shared.js")).unwrap(),
            "assets/shared.js"
        );
    }

    #[test]
    fn a_name_that_leaves_the_directory_is_not_followed() {
        let (_temp, root) = root();
        let out_dir = root.join("dist");
        write(&root.join("secret.txt"), "not yours");
        write(&out_dir.join("assets/ok.css"), "body{}");
        manifest(&out_dir, &["../secret.txt", "/etc/hosts", "assets/ok.css"]);

        assert_eq!(
            manifest_files(&out_dir),
            vec![Utf8PathBuf::from("assets/ok.css")]
        );
    }

    #[test]
    fn nothing_to_carry_is_not_an_error() {
        let (_temp, root) = root();
        let stashed = stash(&root, &root.join("dist")).unwrap();
        assert_eq!(stashed.len(), 0);

        write(&root.join("dist/.vite/manifest.json"), "not json");
        assert_eq!(stash(&root, &root.join("dist")).unwrap().len(), 0);
    }
}
