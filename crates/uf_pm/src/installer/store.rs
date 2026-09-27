//! Immutable verified package trees and file deduplication across projects.
use super::{Graph, Node, download, hex};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use flate2::read::GzDecoder;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Component,
};

pub(super) struct Store {
    pub(super) root: Utf8PathBuf,
}

impl Store {
    pub(super) fn discover() -> Result<Self> {
        let root = match std::env::var_os("UF_PM_STORE") {
            Some(path) => Utf8PathBuf::from_path_buf(path.into())
                .map_err(|_| anyhow::anyhow!("UF_PM_STORE is not UTF-8"))?,
            None => uf_env::Store::discover()?
                .root()
                .parent()
                .context("shared data directory has no parent")?
                .join("pm"),
        };
        ensure!(root.is_absolute(), "UF_PM_STORE must be absolute");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    pub(super) fn package(&self, node: &Node) -> Utf8PathBuf {
        self.root.join("packages").join(package_key(node))
    }
    pub(super) fn ensure(&self, node: &Node, agent: &ureq::Agent) -> Result<bool> {
        let destination = self.package(node);
        if complete(&destination, node)? {
            return Ok(false);
        }
        let guard = self.root.join("locks").join(package_key(node));
        let _guard = uf_env::lock::guard(&guard)?;
        if complete(&destination, node)? {
            return Ok(false);
        }
        let bytes = download(agent, &node.tarball, 256 * 1024 * 1024)?;
        verify(&bytes, &node.integrity)?;
        let packages = self.root.join("packages");
        fs::create_dir_all(&packages)?;
        let temporary = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&packages)?;
        let path = Utf8Path::from_path(temporary.path()).context("non UTF-8 staging path")?;
        let content = path.join("package");
        fs::create_dir(&content)?;
        unpack(&bytes, &content)?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(content.join("package.json"))?)?;
        ensure!(
            manifest.get("name").and_then(serde_json::Value::as_str) == Some(node.name.as_str())
                && manifest.get("version").and_then(serde_json::Value::as_str)
                    == Some(node.version.as_str()),
            "archive identity differs from locked package"
        );
        #[cfg(unix)]
        for bin in node.bin.values() {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                safe_relative(Utf8Path::new(bin)),
                "unsafe archive binary path"
            );
            let target = content.join(bin.as_str());
            if target.is_file() {
                fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
            }
        }
        let _lease = uf_env::lock::shared_guard(&self.root.join("gc"))?;
        intern_tree(&self.root.join("files"), &content)?;
        write_json(
            &path.join("complete.json"),
            &serde_json::json!({"name":node.name,"version":node.version,"integrity":node.integrity}),
        )?;
        fs::rename(path, &destination)?;
        Ok(true)
    }
    pub(super) fn register(&self, project: &Utf8Path, entries: Vec<String>) -> Result<()> {
        let _guard = uf_env::lock::guard(&self.root.join("gc"))?;
        let root = uf_env::Roots::new(self.root.join("roots"));
        root.register(&project.canonicalize_utf8()?, &entries)?;
        Ok(())
    }
}

pub(super) fn package_key(node: &Node) -> String {
    hex(&Sha256::digest(node.integrity.as_bytes()))
}

fn complete(destination: &Utf8Path, node: &Node) -> Result<bool> {
    let bytes = match fs::read(destination.join("complete.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        value.get("name").and_then(serde_json::Value::as_str) == Some(node.name.as_str())
            && value.get("version").and_then(serde_json::Value::as_str)
                == Some(node.version.as_str())
            && value.get("integrity").and_then(serde_json::Value::as_str)
                == Some(node.integrity.as_str()),
        "shared cache identity differs from the lock"
    );
    Ok(destination.join("package/package.json").is_file())
}

pub(super) fn safe_relative(path: &Utf8Path) -> bool {
    !path.as_str().is_empty()
        && !path.as_str().contains(['\\', ':', '\0'])
        && path
            .as_std_path()
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

fn verify(bytes: &[u8], integrity: &str) -> Result<()> {
    // SRI chooses the strongest supported algorithm, rather than accepting a
    // weaker matching digest when a stronger supplied one does not match.
    for algorithm in ["sha512", "sha384", "sha256"] {
        let candidates: Vec<_> = integrity
            .split_whitespace()
            .filter_map(|part| part.split_once('-'))
            .filter(|(name, _)| *name == algorithm)
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let digest = match algorithm {
            "sha512" => Sha512::digest(bytes).to_vec(),
            "sha384" => sha2::Sha384::digest(bytes).to_vec(),
            _ => Sha256::digest(bytes).to_vec(),
        };
        ensure!(
            candidates.into_iter().any(|(_, encoded)| STANDARD
                .decode(encoded.split('?').next().unwrap_or(encoded))
                .is_ok_and(|expected| expected == digest)),
            "package archive failed SRI integrity verification"
        );
        return Ok(());
    }
    bail!("package has no supported SRI integrity")
}

fn unpack(bytes: &[u8], destination: &Utf8Path) -> Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut expanded = 0u64;
    let mut count = 0usize;
    let mut prefix: Option<CompactString> = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        count += 1;
        expanded = expanded
            .checked_add(entry.size())
            .context("archive size overflow")?;
        ensure!(
            expanded <= 1024 * 1024 * 1024 && count <= 200_000,
            "package archive exceeds extraction limit"
        );
        let path = entry.path()?.into_owned();
        let path =
            Utf8Path::from_path(&path).context("package archive contains a non UTF-8 path")?;
        ensure!(
            safe_relative(path),
            "package archive path escapes its destination"
        );
        let mut parts = path.components();
        let directory = parts.next().context("package archive has an empty path")?;
        let directory = directory.as_str();
        ensure!(
            directory != "." && safe_relative(Utf8Path::new(directory)),
            "package archive lacks a directory prefix"
        );
        // Older npm tarballs, including DefinitelyTyped, use the package's
        // basename instead of package/. Strip one consistent, safe directory.
        let expected = prefix.get_or_insert_with(|| CompactString::new(directory));
        ensure!(
            expected.as_str() == directory,
            "package archive mixes directory prefixes"
        );
        let relative = parts.as_path();
        let kind = entry.header().entry_type();
        // Refuse devices, archive symlinks and hardlink entries before they can
        // redirect a later archive member. Verified data is still untrusted code.
        ensure!(
            kind.is_dir() || kind.is_file(),
            "package archive contains an unsupported link or special file"
        );
        if relative.as_str().is_empty() {
            ensure!(
                kind.is_dir(),
                "package archive file lacks a directory prefix"
            );
            continue;
        }
        ensure!(
            safe_relative(relative),
            "package archive path escapes its destination"
        );
        let target = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        let parent = target.parent().context("archive file has no parent")?;
        fs::create_dir_all(parent)?;
        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target);
        match created {
            Ok(mut output) => {
                std::io::copy(&mut entry, &mut output)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // A regular file can occur more than once, including via ./.
                // Match tar's last-member semantics in this private staging
                // directory without following links or truncating directories.
                ensure!(
                    fs::symlink_metadata(&target)?.is_file(),
                    "duplicate archive path is not a regular file"
                );
                let mut replacement = tempfile::NamedTempFile::new_in(parent)?;
                std::io::copy(&mut entry, replacement.as_file_mut())?;
                replacement.persist(&target)?;
            }
            Err(error) => return Err(error.into()),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = entry.header().mode()? & 0o755;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn intern_tree(files: &Utf8Path, directory: &Utf8Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = Utf8PathBuf::from_path_buf(entry.path())
            .map_err(|_| anyhow::anyhow!("non UTF-8 package path"))?;
        if entry.file_type()?.is_dir() {
            intern_tree(files, &path)?;
            continue;
        }
        ensure!(
            entry.file_type()?.is_file(),
            "unexpected link in extracted package"
        );
        let mut file = fs::File::open(&path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 32 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        // Executable and non-executable instances cannot share one inode's mode.
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(&path)?.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        let key = hex(&digest.finalize());
        let directory = files.join(&key[..2]);
        fs::create_dir_all(&directory)?;
        let destination = directory.join(uf_infra::into_string(uf_infra::cstr!(
            "{key}{}",
            if executable { "-x" } else { "" }
        )));
        match fs::hard_link(&path, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        fs::remove_file(&path)?;
        fs::hard_link(&destination, &path)?;
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&destination)?.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&destination, permissions)?;
        }
    }
    Ok(())
}

fn node_path(modules: &Utf8Path, id: &str, node: &Node) -> Utf8PathBuf {
    modules
        .join(".uf")
        .join(hex(&Sha256::digest(id.as_bytes())))
        .join("node_modules")
        .join(node.name.as_str())
}

pub(super) fn installed_package(root: &Utf8Path, id: &str, node: &Node) -> Utf8PathBuf {
    node_path(&root.join("node_modules"), id, node)
}

pub(super) fn materialize(
    root: &Utf8Path,
    store: &Store,
    graph: &Graph,
    selected: &BTreeSet<CompactString>,
    prod: bool,
    allow_scripts: bool,
) -> Result<(usize, usize)> {
    let modules = root.join("node_modules");
    let stamp = serde_json::json!({"graph":hex(&Sha256::digest(serde_json::to_vec(graph)?)),"prod":prod,"store":store.root,"scripts":allow_scripts});
    if fs::read(modules.join(".uf/state.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .as_ref()
        == Some(&stamp)
        && selected.iter().all(|id| {
            let node = &graph.nodes[id];
            if let Some(local) = &node.local {
                let output = root.join(local.as_str()).join("node_modules");
                [
                    &node.dependencies,
                    &node.optional_dependencies,
                    &node.peer_dependencies,
                ]
                .into_iter()
                .flat_map(|edges| edges.iter())
                .all(|(name, target)| {
                    !selected.contains(target)
                        || output
                            .join(name.as_str())
                            .canonicalize_utf8()
                            .ok()
                            .is_some_and(|path| {
                                target_path(root, &modules, target, graph)
                                    .canonicalize_utf8()
                                    .ok()
                                    .as_ref()
                                    == Some(&path)
                            })
                })
            } else {
                node_path(&modules, id, node).join("package.json").is_file()
            }
        })
        && graph.importers.iter().all(|importer| {
            let output = root.join(importer.path.as_str()).join("node_modules");
            importer_edges(importer, prod).all(|(name, id)| {
                !selected.contains(id) || {
                    let target = target_path(root, &modules, id, graph);
                    output
                        .join(name.as_str())
                        .canonicalize_utf8()
                        .ok()
                        .is_some_and(|path| target.canonicalize_utf8().ok().as_ref() == Some(&path))
                }
            })
        })
    {
        return Ok((0, 0));
    }
    let staging = tempfile::Builder::new()
        .prefix(".uf-modules-")
        .tempdir_in(root)?;
    let stage = Utf8Path::from_path(staging.path()).context("non UTF-8 staging path")?;
    let mut hardlinked = 0;
    let mut copied = 0;
    for id in selected {
        let node = &graph.nodes[id];
        if node.local.is_some() {
            continue;
        }
        let output = node_path(stage, id, node);
        fs::create_dir_all(&output)?;
        copy_tree(
            &store.package(node).join("package"),
            &output,
            &mut hardlinked,
            &mut copied,
            allow_scripts
                && crate::builds::LIFECYCLE_SCRIPTS
                    .iter()
                    .any(|hook| node.scripts.contains_key(*hook)),
        )?;
    }
    for id in selected {
        let node = &graph.nodes[id];
        if node.local.is_some() {
            continue;
        }
        let output = node_path(stage, id, node);
        let siblings = if node.name.starts_with('@') {
            output.parent().and_then(Utf8Path::parent)
        } else {
            output.parent()
        }
        .context("package has no module directory")?;
        for edges in [
            &node.dependencies,
            &node.optional_dependencies,
            &node.peer_dependencies,
        ] {
            for (name, target) in edges {
                if selected.contains(target) {
                    link_dependency(root, &modules, siblings, name, target, graph)?;
                }
            }
        }
        // Dependencies expose executables to one another, as node_modules/.bin.
        for edges in [&node.dependencies, &node.optional_dependencies] {
            for target in edges.values().filter(|target| selected.contains(*target)) {
                link_bins(root, &modules, siblings, target, graph)?;
            }
        }
    }
    for importer in graph
        .importers
        .iter()
        .filter(|importer| importer.path == ".")
    {
        link_importer(root, &modules, stage, importer, graph, selected, prod)?;
    }
    write_json(&stage.join(".uf/state.json"), &stamp)?;
    let backup = tempfile::Builder::new()
        .prefix(".uf-previous-modules-")
        .tempdir_in(root)?;
    let mut prepared = vec![(modules.clone(), staging, backup)];
    for importer in graph
        .importers
        .iter()
        .filter(|importer| importer.path != ".")
    {
        let directory = root.join(importer.path.as_str());
        let staging = tempfile::Builder::new()
            .prefix(".uf-modules-")
            .tempdir_in(&directory)?;
        let stage = Utf8Path::from_path(staging.path()).context("non UTF-8 workspace staging")?;
        link_importer(root, &modules, stage, importer, graph, selected, prod)?;
        let backup = tempfile::Builder::new()
            .prefix(".uf-previous-modules-")
            .tempdir_in(&directory)?;
        prepared.push((directory.join("node_modules"), staging, backup));
    }
    let mut local_directories = BTreeSet::new();
    for id in selected {
        let node = &graph.nodes[id];
        let Some(local) = &node.local else {
            continue;
        };
        let directory = root.join(local.as_str()).canonicalize_utf8()?;
        if graph.importers.iter().any(|importer| {
            root.join(importer.path.as_str())
                .canonicalize_utf8()
                .ok()
                .as_ref()
                == Some(&directory)
        }) || !local_directories.insert(directory.clone())
        {
            continue;
        }
        let staging = tempfile::Builder::new()
            .prefix(".uf-modules-")
            .tempdir_in(&directory)?;
        let stage = Utf8Path::from_path(staging.path()).context("non UTF-8 local staging")?;
        for edges in [
            &node.dependencies,
            &node.optional_dependencies,
            &node.peer_dependencies,
        ] {
            for (name, target) in edges {
                if selected.contains(target) {
                    link_dependency(root, &modules, stage, name, target, graph)?;
                    link_bins(root, &modules, stage, target, graph)?;
                }
            }
        }
        let backup = tempfile::Builder::new()
            .prefix(".uf-previous-modules-")
            .tempdir_in(&directory)?;
        prepared.push((directory.join("node_modules"), staging, backup));
    }
    // Check every destination before activating any graph, then keep enough
    // state to roll all workspaces back on a later rename failure.
    for (destination, _, _) in &prepared {
        ensure!(
            !fs::symlink_metadata(destination).is_ok_and(|m| m.file_type().is_symlink()),
            "node_modules is a symlink; refusing to replace it"
        );
    }
    let mut moved = vec![(false, false); prepared.len()];
    let activation: Result<()> = (|| {
        for (index, (destination, staging, backup)) in prepared.iter().enumerate() {
            if destination.exists() {
                fs::rename(destination, backup.path().join("node_modules"))?;
                moved[index].0 = true;
            }
            fs::rename(staging.path(), destination)?;
            moved[index].1 = true;
        }
        Ok(())
    })();
    if let Err(error) = activation {
        for ((destination, _, backup), (previous, installed)) in prepared.iter().zip(moved).rev() {
            if installed {
                fs::remove_dir_all(destination)?;
            }
            if previous {
                fs::rename(backup.path().join("node_modules"), destination)
                    .context("could not restore previous workspace dependencies")?;
            }
        }
        return Err(error);
    }
    Ok((hardlinked, copied))
}

fn importer_edges(
    importer: &super::Importer,
    prod: bool,
) -> impl Iterator<Item = (&CompactString, &CompactString)> {
    [
        &importer.dependencies,
        &importer.optional_dependencies,
        &importer.peer_dependencies,
    ]
    .into_iter()
    .flat_map(|edges| edges.iter())
    .chain(importer.dev_dependencies.iter().filter(move |_| !prod))
}

fn link_importer(
    root: &Utf8Path,
    modules: &Utf8Path,
    output: &Utf8Path,
    importer: &super::Importer,
    graph: &Graph,
    selected: &BTreeSet<CompactString>,
    prod: bool,
) -> Result<()> {
    for (name, target) in importer_edges(importer, prod) {
        if selected.contains(target) {
            link_dependency(root, modules, output, name, target, graph)?;
            link_bins(root, modules, output, target, graph)?;
        }
    }
    Ok(())
}

fn copy_tree(
    source: &Utf8Path,
    target: &Utf8Path,
    hardlinked: &mut usize,
    copied: &mut usize,
    writable: bool,
) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let source = source.join(name.to_str().context("non UTF-8 shared file")?);
        let target = target.join(name.to_str().context("non UTF-8 shared file")?);
        if entry.file_type()?.is_dir() {
            fs::create_dir(&target)?;
            copy_tree(&source, &target, hardlinked, copied, writable)?;
        } else {
            ensure!(
                entry.file_type()?.is_file(),
                "shared store contains an unexpected link"
            );
            if writable {
                fs::copy(&source, &target)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = fs::metadata(&source)?.permissions().mode() | 0o200;
                    fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
                }
                *copied += 1;
                continue;
            }
            match fs::hard_link(&source, &target) {
                Ok(()) => *hardlinked += 1,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::CrossesDevices
                            | std::io::ErrorKind::Unsupported
                            | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    fs::copy(&source, &target)?;
                    *copied += 1;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn target_path(root: &Utf8Path, modules: &Utf8Path, id: &str, graph: &Graph) -> Utf8PathBuf {
    let node = &graph.nodes[id];
    node.local.as_ref().map_or_else(
        || node_path(modules, id, node),
        |local| root.join(local.as_str()),
    )
}

fn link_dependency(
    root: &Utf8Path,
    modules: &Utf8Path,
    output: &Utf8Path,
    name: &str,
    id: &str,
    graph: &Graph,
) -> Result<()> {
    let target = target_path(root, modules, id, graph);
    let link = output.join(name);
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::symlink_metadata(&link).is_ok() {
        ensure!(
            fs::symlink_metadata(&link)?.file_type().is_symlink(),
            "dependency entry already contains user files"
        );
        #[cfg(unix)]
        fs::remove_file(&link)?;
        #[cfg(windows)]
        fs::remove_dir(&link)?;
    }
    link_directory(&target, &link)
}

fn link_directory(target: &Utf8Path, link: &Utf8Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(windows)]
    junction::create(target, link)?;
    Ok(())
}

fn link_bins(
    root: &Utf8Path,
    modules: &Utf8Path,
    output: &Utf8Path,
    id: &str,
    graph: &Graph,
) -> Result<()> {
    let node = &graph.nodes[id];
    for (name, path) in &node.bin {
        let target = target_path(root, modules, id, graph).join(path.as_str());
        let directory = output.join(".bin");
        fs::create_dir_all(&directory)?;
        let link = directory.join(name.as_str());
        if fs::symlink_metadata(&link).is_ok() {
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        #[cfg(windows)]
        {
            ensure!(
                !target.as_str().contains(['"', '%', '\r', '\n']),
                "unsafe executable path for Windows shim"
            );
            fs::write(
                link.with_extension("cmd"),
                uf_infra::cstr!("@echo off\r\nnode \"{target}\" %*\r\n").as_bytes(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn write_json(path: &Utf8Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("JSON file has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, value)?;
    temporary.write_all(b"\n")?;
    temporary.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};

    fn file_archive(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (path, bytes, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(*mode);
            header.set_size(bytes.len() as u64);
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn extracts_legacy_npm_directory_prefixes() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8Path::from_path(output.path()).unwrap();
        let bytes = file_archive(&[
            ("estree/package.json", br#"{"name":"@types/estree"}"#, 0o644),
            ("estree/index.d.ts", b"export interface Node {}", 0o644),
        ]);
        unpack(&bytes, output).unwrap();
        assert_eq!(
            fs::read(output.join("index.d.ts")).unwrap(),
            b"export interface Node {}"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(output.join("package.json")).unwrap()
            )
            .unwrap()["name"],
            "@types/estree"
        );
        assert!(!output.join("estree").exists());
    }

    #[test]
    fn normalized_duplicate_files_use_the_last_member_and_mode() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8Path::from_path(output.path()).unwrap();
        let bytes = file_archive(&[
            ("package/./dist/index.js", b"old", 0o444),
            ("package/dist/index.js", b"replacement", 0o755),
        ]);
        unpack(&bytes, output).unwrap();
        let path = output.join("dist/index.js");
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
    }

    #[test]
    fn rejects_mixed_prefixes_and_replacing_directories_with_files() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8Path::from_path(output.path()).unwrap();
        let mixed = file_archive(&[
            ("package/first.js", b"first", 0o644),
            ("other/second.js", b"second", 0o644),
        ]);
        assert!(unpack(&mixed, output).is_err());
        assert!(!output.join("second.js").exists());
        let collision = file_archive(&[
            ("package/dist/index.js", b"first", 0o644),
            ("package/dist", b"replacement", 0o644),
        ]);
        assert!(unpack(&collision, output).is_err());
        assert_eq!(fs::read(output.join("dist/index.js")).unwrap(), b"first");
    }

    #[test]
    fn verifies_strongest_integrity_and_refuses_corruption() {
        let bytes = b"package";
        let strong = uf_infra::cstr!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)));
        verify(bytes, &strong).unwrap();
        assert!(verify(b"changed", &strong).is_err());
        let weak = uf_infra::cstr!("sha256-{}", STANDARD.encode(Sha256::digest(bytes)));
        assert!(verify(bytes, &uf_infra::cstr!("{weak} sha512-AA==")).is_err());
        assert!(verify(bytes, "sha1-x").is_err());
    }

    #[test]
    fn refuses_archive_links_before_extraction() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8Path::from_path(output.path()).unwrap();
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_size(0);
        builder
            .append_link(&mut header, "package/escape", "../../outside")
            .unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        assert!(unpack(&bytes, output).is_err());
        assert!(!output.join("escape").exists());
        for path in [
            "../outside",
            "/outside",
            "a/../../outside",
            "a\\b",
            "c:drive",
        ] {
            assert!(!safe_relative(Utf8Path::new(path)), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn identical_files_in_different_packages_share_one_inode() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        for package in ["a", "b"] {
            fs::create_dir(root.join(package)).unwrap();
            let file = root.join(package).join("data.js");
            fs::write(&file, b"same bytes").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
            intern_tree(&root.join("files"), &root.join(package)).unwrap();
        }
        let a = fs::metadata(root.join("a/data.js")).unwrap();
        let b = fs::metadata(root.join("b/data.js")).unwrap();
        assert_eq!(a.ino(), b.ino());
        assert_eq!(a.dev(), b.dev());
        assert_eq!(a.nlink(), 3);
        assert!(a.permissions().readonly());
    }
}
