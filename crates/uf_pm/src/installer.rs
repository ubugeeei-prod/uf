//! Native resolution, integrity-checked downloads and shared content-addressed files.
//! Frozen and warm installs read their graph from uf.lock without metadata requests.
mod gc;
pub use gc::{GcPlan, gc_collect, gc_plan};
mod queries;
mod store;
pub use queries::{AuditFinding, AuditReport, audit};
#[cfg(test)]
mod tests;

use crate::{Operation, RegistryRouting, is_polluting_json_key};
use anyhow::{Context, Result, bail, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    time::{Duration, Instant},
};
use uf_config::UniflowedConfig;
use uf_infra::FxHashMap;

type Edges = BTreeMap<CompactString, CompactString>;
const MAX_PACKAGES: usize = 50_000;

/// Options for an install of uf's own graph.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Refuse absent or stale uf.lock without changing it.
    pub frozen: bool,
    /// Materialize production dependencies only; the lock still describes all dependencies.
    pub prod: bool,
    /// Refresh resolutions within manifest ranges.
    pub update: bool,
    /// Direct dependencies to refresh; empty refreshes every manifest range.
    pub update_packages: Vec<CompactString>,
    /// Project runtime directories prepended for explicitly enabled lifecycle hooks.
    pub path: Vec<Utf8PathBuf>,
}

/// Measured native work, with no external package manager involved.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Number of unique packages materialized for this platform.
    pub packages: usize,
    /// Archives downloaded rather than reused from the shared store.
    pub downloaded: usize,
    /// Files hardlinked into node_modules.
    pub hardlinked: usize,
    /// Files copied when hardlinks cross a filesystem boundary.
    pub copied: usize,
    /// Shared store used by every project.
    pub store: Utf8PathBuf,
    /// Resolution, including lock reuse, in milliseconds.
    pub resolve_ms: u128,
    /// Fetching and integrity checks in milliseconds.
    pub fetch_ms: u128,
    /// Creating the isolated package graph in milliseconds.
    pub link_ms: u128,
}

/// Run an operation against uf's native store, without invoking another manager.
pub fn execute(cwd: &Utf8Path, operation: Operation<'_>, operands: &[String]) -> Result<Value> {
    execute_with_path(cwd, operation, operands, &[])
}

/// Run a native operation with the project's prepared runtime directories.
pub fn execute_with_path(
    cwd: &Utf8Path,
    operation: Operation<'_>,
    operands: &[String],
    path: &[Utf8PathBuf],
) -> Result<Value> {
    let resolved = uf_config::load_config(cwd)?;
    let options = Options {
        frozen: matches!(
            operation,
            Operation::InstallFrozen | Operation::InstallFrozenProd
        ),
        prod: matches!(
            operation,
            Operation::InstallProd | Operation::InstallFrozenProd
        ),
        update: matches!(
            operation,
            Operation::Update | Operation::Dedupe { check: false }
        ),
        path: path.to_vec(),
        update_packages: if operation == Operation::Update {
            operands.iter().map(|name| name.as_str().into()).collect()
        } else {
            Vec::new()
        },
    };
    match operation {
        Operation::Install
        | Operation::InstallFrozen
        | Operation::InstallProd
        | Operation::InstallFrozenProd
        | Operation::Update
        | Operation::Dedupe { check: false } => {
            ensure!(
                operation == Operation::Update || operands.is_empty(),
                "native install does not take package names"
            );
            Ok(serde_json::to_value(install(
                &resolved.root,
                &resolved.config,
                options,
            )?)?)
        }
        Operation::Add { .. } | Operation::Remove => {
            ensure!(!operands.is_empty(), "choose at least one package");
            let path = cwd.join("package.json");
            let bytes = fs::read(&path)?;
            let mut manifest: Value = serde_json::from_slice(&bytes)?;
            for operand in operands {
                if let Operation::Add { kind } = operation {
                    let (name, range) = split_spec(operand)?;
                    let range = if range == "latest" {
                        let routing = RegistryRouting::from_config(&resolved.config);
                        let metadata = packument(&agent(), &routing, &name)?;
                        let node = registry_node(
                            &metadata,
                            &name,
                            "latest",
                            routing.route(&name).registry,
                        )?;
                        uf_infra::cstr!("^{}", node.version)
                    } else {
                        range
                    };
                    let field = kind.manifest_field();
                    let object = manifest
                        .as_object_mut()
                        .context("package.json must contain an object")?;
                    let dependencies = object
                        .entry(field)
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                        .context("dependency field must contain an object")?;
                    dependencies.insert(name.into(), Value::from(range.as_str()));
                } else {
                    ensure!(
                        crate::links::is_package_name(operand),
                        "invalid package name"
                    );
                    for field in crate::DEPENDENCY_FIELDS {
                        if let Some(dependencies) =
                            manifest.get_mut(field).and_then(Value::as_object_mut)
                        {
                            dependencies.remove(operand);
                        }
                    }
                }
            }
            store::write_json(&path, &manifest)?;
            match install(&resolved.root, &resolved.config, options) {
                Ok(report) => Ok(serde_json::to_value(report)?),
                Err(error) => {
                    fs::write(&path, bytes)
                        .context("could not restore manifest after failed native install")?;
                    Err(error)
                }
            }
        }
        Operation::Audit => Ok(serde_json::to_value(audit(
            &resolved.root,
            &resolved.config,
            false,
        )?)?),
        Operation::List | Operation::Why | Operation::Info | Operation::Search => {
            queries::query(&resolved.root, &resolved.config, operation, operands)
        }
        _ => bail!(uf_infra::cstr!(
            "native uf does not yet implement {}; choose an external manager explicitly for this operation",
            operation.name()
        )),
    }
}

fn split_spec(spec: &str) -> Result<(CompactString, CompactString)> {
    let (name, range) = spec
        .rsplit_once('@')
        .filter(|(name, _)| !name.is_empty())
        .unwrap_or((spec, "latest"));
    ensure!(
        crate::links::is_package_name(name) && !range.is_empty(),
        "invalid native package spec"
    );
    Ok((name.into(), range.into()))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    name: CompactString,
    version: CompactString,
    #[serde(default)]
    registry: CompactString,
    #[serde(default)]
    tarball: CompactString,
    #[serde(default)]
    integrity: CompactString,
    #[serde(default)]
    local: Option<CompactString>,
    #[serde(default)]
    dependencies: Edges,
    #[serde(default)]
    optional_dependencies: Edges,
    #[serde(default)]
    peer_dependencies: Edges,
    #[serde(default)]
    bin: BTreeMap<CompactString, CompactString>,
    #[serde(default)]
    scripts: BTreeMap<CompactString, CompactString>,
    #[serde(default)]
    os: Vec<CompactString>,
    #[serde(default)]
    cpu: Vec<CompactString>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Importer {
    path: CompactString,
    name: CompactString,
    version: CompactString,
    dependencies: Edges,
    dev_dependencies: Edges,
    optional_dependencies: Edges,
    peer_dependencies: Edges,
    scripts: BTreeMap<CompactString, CompactString>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Graph {
    fingerprint: CompactString,
    importers: Vec<Importer>,
    nodes: BTreeMap<CompactString, Node>,
}

/// Resolve and apply a native dependency graph. This function never spawns npm.
pub fn install(root: &Utf8Path, config: &UniflowedConfig, options: Options) -> Result<Report> {
    let store = store::Store::discover()?;
    install_with_store(root, config, options, store)
}

fn install_with_store(
    root: &Utf8Path,
    config: &UniflowedConfig,
    options: Options,
    store: store::Store,
) -> Result<Report> {
    let start = Instant::now();
    let lock_path = root.join(config.pm.lockfile.as_str());
    let _guard = uf_env::lock::guard(&lock_path)?;
    let mut importers = read_importers(root, config)?;
    let fingerprint = fingerprint(&importers, config)?;
    let before: Value = match fs::read(&lock_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("uf.lock is invalid JSON")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(error) => return Err(error.into()),
    };
    let locked = before
        .get("native")
        .cloned()
        .map(serde_json::from_value::<Graph>)
        .transpose()?;
    let reusable = locked.as_ref().is_some_and(|graph| {
        graph.fingerprint == fingerprint && local_manifests_match(root, graph)
    });
    ensure!(
        !options.frozen || reusable,
        "uf.lock is absent or stale; run uf install to resolve the native graph"
    );
    let agent = agent();
    if options.update && !options.update_packages.is_empty() {
        ensure!(
            options
                .update_packages
                .iter()
                .all(|name| crate::links::is_package_name(name)),
            "updates take package names, without version specifiers"
        );
        if let Some(graph) = &locked {
            pin_unselected(&mut importers, graph, &options.update_packages);
        }
    }
    let graph = if reusable && !options.update {
        locked.expect("checked lock graph")
    } else {
        resolve(root, importers, fingerprint, config, &agent)?
    };
    validate_graph(&graph, root, config)?;
    let selected = selected(&graph, options.prod)?;
    let resolve_ms = start.elapsed().as_millis();
    let fetch = Instant::now();
    // Publish the acquisition lease before fetching so concurrent GC retains
    // packages needed by an install that has not written its lockfile yet.
    store.register(
        root,
        selected
            .iter()
            .filter_map(|id| {
                let node = &graph.nodes[id];
                node.local.is_none().then(|| store::package_key(node))
            })
            .collect(),
    )?;
    let requests: Vec<_> = selected.iter().map(|id| (&graph.nodes[id], id)).collect();
    let acquired = parallel(&requests, |(node, _)| {
        if node.local.is_some() {
            Ok(false)
        } else {
            store.ensure(node, &agent)
        }
    })?;
    let fetch_ms = fetch.elapsed().as_millis();
    let link = Instant::now();
    let (hardlinked, copied) = store::materialize(
        root,
        &store,
        &graph,
        &selected,
        options.prod,
        config.pm.allow_lifecycle_scripts,
    )?;
    if config.pm.allow_lifecycle_scripts {
        lifecycle(
            root,
            &graph,
            &selected,
            &options.path,
            hardlinked + copied > 0,
        )?;
    }
    let link_ms = link.elapsed().as_millis();
    if !options.frozen && (!reusable || options.update) {
        let mut document = before.as_object().cloned().unwrap_or_default();
        document.insert("lockfileVersion".into(), Value::from(2));
        document.insert("resolver".into(), Value::from("uf-native"));
        document.insert("native".into(), serde_json::to_value(&graph)?);
        store::write_json(&lock_path, &document)?;
    }
    store.register(
        root,
        selected
            .iter()
            .filter_map(|id| {
                let node = &graph.nodes[id];
                node.local.is_none().then(|| store::package_key(node))
            })
            .collect(),
    )?;
    Ok(Report {
        packages: selected.len(),
        downloaded: acquired.into_iter().filter(|value| *value).count(),
        hardlinked,
        copied,
        store: store.root,
        resolve_ms,
        fetch_ms,
        link_ms,
    })
}

fn pin_unselected(importers: &mut [Importer], locked: &Graph, requested: &[CompactString]) {
    for importer in importers {
        let Some(previous) = locked
            .importers
            .iter()
            .find(|old| old.path == importer.path)
        else {
            continue;
        };
        for (current, previous) in [
            (&mut importer.dependencies, &previous.dependencies),
            (&mut importer.dev_dependencies, &previous.dev_dependencies),
            (
                &mut importer.optional_dependencies,
                &previous.optional_dependencies,
            ),
            (&mut importer.peer_dependencies, &previous.peer_dependencies),
        ] {
            for (name, range) in current {
                if requested.contains(name) {
                    continue;
                }
                let Some(node) = previous.get(name).and_then(|id| locked.nodes.get(id)) else {
                    continue;
                };
                let (_, wanted) = alias(name, range);
                if node.local.is_none() && (satisfies(&wanted, &node.version) || wanted == "latest")
                {
                    *range = if range.starts_with("npm:") {
                        uf_infra::cstr!("npm:{}@{}", node.name, node.version)
                    } else {
                        node.version.clone()
                    };
                }
            }
        }
    }
}

fn lifecycle(
    root: &Utf8Path,
    graph: &Graph,
    selected: &BTreeSet<CompactString>,
    path: &[Utf8PathBuf],
    changed: bool,
) -> Result<()> {
    let run = |directory: &Utf8Path,
               name: &str,
               scripts: &BTreeMap<CompactString, CompactString>|
     -> Result<()> {
        for hook in crate::builds::LIFECYCLE_SCRIPTS {
            let Some(script) = scripts.get(hook) else {
                continue;
            };
            let mut command = if cfg!(windows) {
                let mut command = std::process::Command::new("cmd");
                command.args(["/d", "/s", "/c", script]);
                command
            } else {
                let mut command = std::process::Command::new("sh");
                command.args(["-c", script]);
                command
            };
            let mut entries = vec![
                directory.join("node_modules/.bin").into_std_path_buf(),
                root.join("node_modules/.bin").into_std_path_buf(),
            ];
            entries.extend(path.iter().map(|p| p.clone().into_std_path_buf()));
            if let Some(existing) = std::env::var_os("PATH") {
                entries.extend(std::env::split_paths(&existing));
            }
            let status = command
                .current_dir(directory)
                .env("PATH", std::env::join_paths(entries)?)
                .env("npm_lifecycle_event", hook)
                .env("npm_package_name", name)
                .status()?;
            ensure!(
                status.success(),
                "native lifecycle hook {name}:{hook} failed"
            );
        }
        Ok(())
    };
    if changed {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<_> = selected.iter().map(|id| (id, false)).collect();
        while let Some((id, ready)) = pending.pop() {
            let node = &graph.nodes[id];
            if ready {
                if node.local.is_none() {
                    run(
                        &store::installed_package(root, id, node),
                        &node.name,
                        &node.scripts,
                    )?;
                }
            } else if seen.insert(id) {
                pending.push((id, true));
                pending.extend(
                    node.dependencies
                        .values()
                        .filter(|id| selected.contains(*id))
                        .map(|id| (id, false)),
                );
            }
        }
    }
    for importer in &graph.importers {
        run(
            &root.join(importer.path.as_str()),
            &importer.name,
            &importer.scripts,
        )?;
    }
    Ok(())
}

fn local_manifests_match(root: &Utf8Path, graph: &Graph) -> bool {
    graph.nodes.values().all(|node| {
        let Some(path) = &node.local else { return true };
        fs::read(root.join(path.as_str()).join("package.json"))
            .ok()
            .map(|bytes| hex(&Sha256::digest(bytes)))
            .is_some_and(|digest| node.integrity == digest)
    })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .new_agent()
}

fn read_importers(root: &Utf8Path, config: &UniflowedConfig) -> Result<Vec<Importer>> {
    let mut directories = vec![root.to_path_buf()];
    directories.extend(
        uf_project::discover_workspaces(root, config)
            .into_iter()
            .map(|member| root.join(member.path)),
    );
    directories.sort();
    directories.dedup();
    directories
        .into_iter()
        .filter(|dir| dir.join("package.json").is_file())
        .map(|dir| {
            let manifest: Value = serde_json::from_slice(&fs::read(dir.join("package.json"))?)?;
            let path = dir.strip_prefix(root)?.as_str();
            Ok(Importer {
                path: if path.is_empty() {
                    ".".into()
                } else {
                    path.into()
                },
                name: manifest
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                version: manifest
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("0.0.0")
                    .into(),
                dependencies: map(&manifest, "dependencies")?,
                dev_dependencies: map(&manifest, "devDependencies")?,
                optional_dependencies: map(&manifest, "optionalDependencies")?,
                peer_dependencies: map(&manifest, "peerDependencies")?,
                scripts: map(&manifest, "scripts")?,
            })
        })
        .collect()
}

fn fingerprint(importers: &[Importer], config: &UniflowedConfig) -> Result<CompactString> {
    let value = serde_json::json!([
        importers,
        config.read_registry().url,
        config.scope_registries().collect::<Vec<_>>()
    ]);
    Ok(hex(&Sha256::digest(serde_json::to_vec(&value)?)).into())
}

fn map(value: &Value, field: &str) -> Result<Edges> {
    let mut result = BTreeMap::new();
    if let Some(fields) = value.get(field).and_then(Value::as_object) {
        for (name, range) in fields {
            ensure!(!is_polluting_json_key(name), "invalid manifest key");
            let range = range
                .as_str()
                .context("dependency and script values must be strings")?;
            result.insert(name.as_str().into(), range.into());
        }
    }
    Ok(result)
}

#[derive(Clone)]
struct Request {
    parent: CompactString,
    name: CompactString,
    range: CompactString,
    field: &'static str,
}

fn resolve(
    root: &Utf8Path,
    mut importers: Vec<Importer>,
    fingerprint: CompactString,
    config: &UniflowedConfig,
    agent: &ureq::Agent,
) -> Result<Graph> {
    let routing = RegistryRouting::from_config(config);
    let workspaces: FxHashMap<_, _> = importers
        .iter()
        .filter(|i| !i.name.is_empty())
        .map(|i| (i.name.clone(), (i.path.clone(), i.version.clone())))
        .collect();
    let root_declarations: Edges = importers
        .iter()
        .find(|i| i.path == ".")
        .map(|i| {
            i.dev_dependencies
                .iter()
                .chain(i.dependencies.iter())
                .map(|(name, range)| (name.clone(), range.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut frontier = Vec::new();
    for (index, importer) in importers.iter().enumerate() {
        let parent = uf_infra::cstr!("importer:{index}");
        for (field, edges) in [
            ("dependencies", &importer.dependencies),
            ("devDependencies", &importer.dev_dependencies),
            ("optionalDependencies", &importer.optional_dependencies),
            ("peerDependencies", &importer.peer_dependencies),
        ] {
            for (name, range) in edges {
                frontier.push(Request {
                    parent: parent.clone(),
                    name: name.clone(),
                    range: range.clone(),
                    field,
                });
            }
        }
    }
    let mut nodes = BTreeMap::new();
    let mut metadata = FxHashMap::default();
    while !frontier.is_empty() {
        ensure!(
            nodes.len() < MAX_PACKAGES,
            "native dependency graph exceeds package limit"
        );
        let wanted: BTreeSet<_> = frontier
            .iter()
            .filter(|request| {
                !workspaces.contains_key(&request.name)
                    && !request.range.starts_with("file:")
                    && !request.range.starts_with("link:")
            })
            .map(|request| {
                let (name, range) = alias(&request.name, &request.range);
                let version = exact_version(&range).unwrap_or_default();
                (name, version)
            })
            .filter(|key| !metadata.contains_key(key))
            .collect();
        let wanted: Vec<_> = wanted.into_iter().collect();
        let fetched = parallel(&wanted, |(name, version)| {
            if version.is_empty() {
                packument(agent, &routing, name)
            } else {
                version_document(agent, &routing, name, version)
            }
        })?;
        for (key, document) in wanted.into_iter().zip(fetched) {
            metadata.insert(key, document);
        }
        let requests = std::mem::take(&mut frontier);
        for request in requests {
            ensure!(
                crate::links::is_package_name(&request.name),
                "invalid dependency name"
            );
            let (name, range) = alias(&request.name, &request.range);
            let node = if let Some((path, version)) = workspaces.get(&name) {
                let wanted = range.strip_prefix("workspace:").unwrap_or(&range);
                ensure!(
                    matches!(wanted, "*" | "^" | "~") || satisfies(wanted, version),
                    "workspace package does not satisfy its declared version"
                );
                local_node(root, path)?
            } else if let Some(path) = range
                .strip_prefix("file:")
                .or_else(|| range.strip_prefix("link:"))
            {
                let base = if let Some(index) = request.parent.strip_prefix("importer:") {
                    root.join(importers[index.parse::<usize>()?].path.as_str())
                } else if let Some(parent) = nodes
                    .get(&request.parent)
                    .and_then(|node: &Node| node.local.as_ref())
                {
                    root.join(parent.as_str())
                } else {
                    root.to_path_buf()
                };
                let local = base.join(path).canonicalize_utf8()?;
                // Directory dependencies can live beside a project; they are explicit manifest inputs.
                local_node(root, local.as_str())?
            } else {
                let key = (name.clone(), exact_version(&range).unwrap_or_default());
                let document = &metadata[&key];
                registry_node(document, &name, &range, routing.route(&name).registry)?
            };
            let id = if let Some(path) = &node.local {
                uf_infra::cstr!("local:{path}")
            } else {
                uf_infra::cstr!(
                    "{}@{}:{}",
                    node.name,
                    node.version,
                    hex(&Sha256::digest(node.registry.as_bytes()))
                )
            };
            if !nodes.contains_key(&id) {
                for (field, edges) in [
                    ("dependencies", &node.dependencies),
                    ("optionalDependencies", &node.optional_dependencies),
                    ("peerDependencies", &node.peer_dependencies),
                ] {
                    for (name, range) in edges {
                        // A peer declared at the root uses that exact root range, rather than a second React.
                        let root_range = if field == "peerDependencies" {
                            root_declarations.get(name)
                        } else {
                            None
                        };
                        frontier.push(Request {
                            parent: id.clone(),
                            name: name.clone(),
                            range: root_range.unwrap_or(range).clone(),
                            field,
                        });
                    }
                }
                nodes.insert(id.clone(), node);
            }
            let target = if let Some(index) = request.parent.strip_prefix("importer:") {
                let importer = &mut importers[index.parse::<usize>()?];
                match request.field {
                    "devDependencies" => &mut importer.dev_dependencies,
                    "optionalDependencies" => &mut importer.optional_dependencies,
                    "peerDependencies" => &mut importer.peer_dependencies,
                    _ => &mut importer.dependencies,
                }
            } else {
                let peer_version = nodes
                    .get(&id)
                    .context("missing peer target")?
                    .version
                    .clone();
                let node = nodes
                    .get_mut(&request.parent)
                    .context("missing resolution parent")?;
                if request.field == "peerDependencies"
                    && let Some(range) = node.peer_dependencies.get(&request.name)
                {
                    ensure!(
                        satisfies(range, &peer_version),
                        "resolved peer does not satisfy its consumer"
                    );
                }
                match request.field {
                    "optionalDependencies" => &mut node.optional_dependencies,
                    "peerDependencies" => &mut node.peer_dependencies,
                    _ => &mut node.dependencies,
                }
            };
            target.insert(request.name, id);
        }
    }
    Ok(Graph {
        fingerprint,
        importers,
        nodes,
    })
}

fn alias(name: &str, range: &str) -> (CompactString, CompactString) {
    if let Some(spec) = range.strip_prefix("npm:") {
        if let Some((target, range)) = spec.rsplit_once('@')
            && !target.is_empty()
        {
            return (target.into(), range.into());
        }
        return (spec.into(), "latest".into());
    }
    (name.into(), range.into())
}

fn satisfies(range: &str, version: &str) -> bool {
    node_semver::Range::parse(range)
        .ok()
        .zip(node_semver::Version::parse(version).ok())
        .is_some_and(|(range, version)| range.satisfies(&version))
}

fn registry_node(document: &Value, name: &str, range: &str, registry: &str) -> Result<Node> {
    let versions = document
        .get("versions")
        .and_then(Value::as_object)
        .context("invalid registry metadata")?;
    let wanted = document
        .get("dist-tags")
        .and_then(|tags| tags.get(range))
        .and_then(Value::as_str);
    let value = if let Some(wanted) = wanted {
        versions.get(wanted)
    } else {
        versions
            .iter()
            .filter_map(|(version, value)| {
                let parsed = node_semver::Version::parse(version).ok()?;
                satisfies(range, version).then_some((parsed, value))
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, value)| value)
    }
    .context("no published version satisfies the dependency range")?;
    let mut node = node_from_manifest(value)?;
    ensure!(
        node.name == name && node_semver::Version::parse(&node.version).is_ok(),
        "registry returned a different package"
    );
    node.registry = registry.into();
    node.tarball = value
        .pointer("/dist/tarball")
        .and_then(Value::as_str)
        .context("package has no tarball")?
        .into();
    node.integrity = value
        .pointer("/dist/integrity")
        .and_then(Value::as_str)
        .context("package has no SRI integrity; refusing an unchecked archive")?
        .into();
    Ok(node)
}

fn node_from_manifest(manifest: &Value) -> Result<Node> {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .context("package has no name")?;
    ensure!(crate::links::is_package_name(name), "invalid package name");
    let bin = match manifest.get("bin") {
        Some(Value::String(path)) => BTreeMap::from([(
            name.rsplit('/').next().unwrap_or(name).into(),
            path.as_str().into(),
        )]),
        Some(Value::Object(_)) => map(manifest, "bin")?,
        _ => BTreeMap::new(),
    };
    let list = |field| {
        manifest
            .get(field)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(Into::into)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut node = Node {
        name: name.into(),
        version: manifest
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("0.0.0")
            .into(),
        dependencies: map(manifest, "dependencies")?,
        optional_dependencies: map(manifest, "optionalDependencies")?,
        peer_dependencies: map(manifest, "peerDependencies")?,
        bin,
        scripts: map(manifest, "scripts")?,
        os: list("os"),
        cpu: list("cpu"),
        ..Node::default()
    };
    node.dependencies
        .retain(|name, _| !node.optional_dependencies.contains_key(name));
    node.peer_dependencies.retain(|name, _| {
        manifest
            .get("peerDependenciesMeta")
            .and_then(|meta| meta.get(name.as_str()))
            .and_then(|meta| meta.get("optional"))
            .and_then(Value::as_bool)
            != Some(true)
    });
    Ok(node)
}

fn local_node(root: &Utf8Path, path: &str) -> Result<Node> {
    let dir = root.join(path);
    let bytes = fs::read(dir.join("package.json"))?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    let mut node = node_from_manifest(&manifest)?;
    node.local = Some(path.into());
    node.integrity = hex(&Sha256::digest(bytes)).into();
    Ok(node)
}

fn exact_version(range: &str) -> Option<CompactString> {
    let version = node_semver::Version::parse(range).ok()?;
    (version.to_string() == range).then(|| range.into())
}

fn packument(agent: &ureq::Agent, routing: &RegistryRouting, name: &str) -> Result<Value> {
    let url = crate::registry::url_for(routing.route(name).registry, name)?;
    let bytes = download(agent, &url, crate::MAX_PACKUMENT_BYTES as u64)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn version_document(
    agent: &ureq::Agent,
    routing: &RegistryRouting,
    name: &str,
    version: &str,
) -> Result<Value> {
    let base = crate::registry::url_for(routing.route(name).registry, name)?;
    let url = uf_infra::cstr!("{base}/{version}");
    let bytes = download_with_accept(
        agent,
        &url,
        crate::MAX_PACKUMENT_BYTES as u64,
        "application/json",
    )?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.get("version").and_then(Value::as_str) == Some(version),
        "registry returned a different package version"
    );
    Ok(serde_json::json!({ "versions": { version: manifest } }))
}

fn download(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    download_with_accept(agent, url, limit, "application/vnd.npm.install-v1+json")
}

fn download_with_accept(
    agent: &ureq::Agent,
    url: &str,
    limit: u64,
    accept: &str,
) -> Result<Vec<u8>> {
    ensure!(
        url.starts_with("https://") && !url[8..].split('/').next().unwrap_or("").contains('@'),
        "package URL must be HTTPS without userinfo"
    );
    let mut response = agent.get(url).header("Accept", accept).call()?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "registry response exceeds byte limit"
    );
    Ok(bytes)
}

fn validate_graph(graph: &Graph, root: &Utf8Path, config: &UniflowedConfig) -> Result<()> {
    ensure!(
        graph.nodes.len() <= MAX_PACKAGES,
        "lock graph exceeds package limit"
    );
    let routing = RegistryRouting::from_config(config);
    for (id, node) in &graph.nodes {
        ensure!(
            crate::links::is_package_name(&node.name),
            "unsafe locked package name"
        );
        if let Some(local) = &node.local {
            ensure!(
                root.join(local.as_str()).join("package.json").is_file(),
                "local dependency disappeared"
            );
        } else {
            ensure!(
                node.registry == routing.route(&node.name).registry,
                "locked package registry differs from configured scope routing"
            );
            ensure!(
                !node.tarball.is_empty() && !node.integrity.is_empty(),
                "locked archive lacks integrity"
            );
        }
        for edges in [
            &node.dependencies,
            &node.optional_dependencies,
            &node.peer_dependencies,
        ] {
            for (name, target) in edges {
                ensure!(
                    crate::links::is_package_name(name) && graph.nodes.contains_key(target),
                    "unsafe or dangling locked dependency in {id}"
                );
            }
        }
        for (name, path) in &node.bin {
            ensure!(
                crate::links::is_package_name(name) && !name.contains('/'),
                "unsafe binary name"
            );
            ensure!(
                store::safe_relative(Utf8Path::new(path)),
                "binary path escapes package"
            );
        }
    }
    for importer in &graph.importers {
        ensure!(
            importer.path == "." || store::safe_relative(Utf8Path::new(&importer.path)),
            "unsafe importer path"
        );
        if !config.pm.allow_lifecycle_scripts {
            ensure!(
                !crate::INSTALL_LIFECYCLE_SCRIPTS
                    .iter()
                    .any(|script| importer.scripts.contains_key(*script)),
                "project declares install lifecycle scripts; move them to uf tasks or explicitly allow them"
            );
        }
    }
    Ok(())
}

fn selected(graph: &Graph, prod: bool) -> Result<BTreeSet<CompactString>> {
    let mut selected = BTreeSet::new();
    let mut pending = Vec::new();
    for importer in &graph.importers {
        pending.extend(
            importer
                .dependencies
                .values()
                .chain(importer.peer_dependencies.values())
                .map(|id| (id.clone(), false)),
        );
        pending.extend(
            importer
                .optional_dependencies
                .values()
                .map(|id| (id.clone(), true)),
        );
        if !prod {
            pending.extend(
                importer
                    .dev_dependencies
                    .values()
                    .map(|id| (id.clone(), false)),
            );
        }
    }
    while let Some((id, optional)) = pending.pop() {
        let node = graph
            .nodes
            .get(&id)
            .context("dangling importer dependency")?;
        if !compatible(node) {
            ensure!(
                optional,
                "required dependency is incompatible with this platform"
            );
            continue;
        }
        if !selected.insert(id) {
            continue;
        }
        pending.extend(
            node.dependencies
                .values()
                .chain(node.peer_dependencies.values())
                .map(|id| (id.clone(), false)),
        );
        pending.extend(
            node.optional_dependencies
                .values()
                .map(|id| (id.clone(), true)),
        );
    }
    Ok(selected)
}

fn compatible(node: &Node) -> bool {
    let platform = match std::env::consts::OS {
        "windows" => "win32",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    let matches = |values: &[CompactString], current: &str| {
        !values.iter().any(|v| v.strip_prefix('!') == Some(current))
            && (values.iter().all(|v| v.starts_with('!'))
                || values.iter().any(|v| v == current || v == "any"))
    };
    matches(&node.os, platform) && matches(&node.cpu, arch)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 15) as usize] as char);
    }
    output
}

// Network latency benefits from concurrency even on a single-core machine.
fn parallel<T: Sync, U: Send>(
    items: &[T],
    body: impl Fn(&T) -> Result<U> + Sync,
) -> Result<Vec<U>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let cursor = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..items.len().min(8))
            .map(|_| {
                scope.spawn(|| {
                    let mut output = Vec::new();
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        if index >= items.len() {
                            break;
                        }
                        output.push((index, body(&items[index])));
                    }
                    output
                })
            })
            .collect();
        let mut output: Vec<_> = handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("native worker panicked"))
            .collect();
        output.sort_unstable_by_key(|(index, _)| *index);
        output.into_iter().map(|(_, value)| value).collect()
    })
}
