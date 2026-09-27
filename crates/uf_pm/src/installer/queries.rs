//! Queries over the native graph and the registries each scope is bound to.
use super::{Graph, Operation, RegistryRouting, agent, download, satisfies, validate_graph};
use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
};
use uf_config::UniflowedConfig;

/// One advisory affecting an actually locked version.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    /// Affected package.
    pub package: CompactString,
    /// Installed versions covered by this advisory.
    pub versions: Vec<CompactString>,
    /// Registry advisory identifier.
    pub id: u64,
    /// Advisory title.
    pub title: String,
    /// Severity reported by the registry.
    pub severity: CompactString,
    /// Range in which the advisory applies.
    pub vulnerable_versions: CompactString,
    /// Primary advisory link.
    pub url: String,
}

/// A complete audit. A registry failure is an error, never a clean result.
#[derive(Debug, Serialize)]
pub struct AuditReport {
    /// Unique package versions inspected.
    pub checked: usize,
    /// Registries that answered, without credentials.
    pub registries: Vec<CompactString>,
    /// Every applicable advisory, ordered by severity and package.
    pub findings: Vec<AuditFinding>,
}

#[derive(Deserialize)]
struct Advisory {
    id: u64,
    title: String,
    severity: CompactString,
    vulnerable_versions: CompactString,
    url: String,
}

fn read_graph(root: &Utf8Path, config: &UniflowedConfig) -> Result<Graph> {
    let document: Value =
        serde_json::from_slice(&fs::read(root.join(config.pm.lockfile.as_str()))?)?;
    let graph = serde_json::from_value(
        document
            .get("native")
            .context("run uf install to create a native lockfile")?
            .clone(),
    )?;
    validate_graph(&graph, root, config)?;
    Ok(graph)
}

/// Audit locked native packages against each configured registry's bulk advisory API.
pub fn audit(root: &Utf8Path, config: &UniflowedConfig, prod: bool) -> Result<AuditReport> {
    let graph = read_graph(root, config)?;
    let selected = super::selected(&graph, prod)?;
    let mut groups: BTreeMap<CompactString, BTreeMap<CompactString, BTreeSet<CompactString>>> =
        BTreeMap::new();
    for id in selected {
        let node = &graph.nodes[&id];
        if node.local.is_none() {
            groups
                .entry(node.registry.clone())
                .or_default()
                .entry(node.name.clone())
                .or_default()
                .insert(node.version.clone());
        }
    }
    let checked = groups
        .values()
        .flat_map(BTreeMap::values)
        .map(BTreeSet::len)
        .sum();
    let mut report = AuditReport {
        checked,
        registries: Vec::new(),
        findings: Vec::new(),
    };
    let agent = agent();
    for (registry, packages) in groups {
        crate::registry::url_for(&registry, "probe")?;
        let url = uf_infra::cstr!(
            "{}/-/npm/v1/security/advisories/bulk",
            registry.trim_end_matches('/')
        );
        let payload = serde_json::to_vec(&packages)?;
        let mut response = agent
            .post(url.as_str())
            .header("Content-Type", "application/json")
            .send(payload.as_slice())
            .with_context(|| {
                uf_infra::cstr!("audit registry {registry} did not answer; audit is incomplete")
            })?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(crate::MAX_PACKUMENT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= crate::MAX_PACKUMENT_BYTES,
            "audit response exceeds byte limit"
        );
        let advisories: BTreeMap<CompactString, Vec<Advisory>> = serde_json::from_slice(&bytes)?;
        report.findings.extend(applicable(&packages, advisories)?);
        report.registries.push(registry);
    }
    report.findings.sort_by(|a, b| {
        rank(&b.severity)
            .cmp(&rank(&a.severity))
            .then(a.package.cmp(&b.package))
            .then(a.id.cmp(&b.id))
    });
    Ok(report)
}

fn rank(value: &str) -> u8 {
    match value {
        "critical" => 4,
        "high" => 3,
        "moderate" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn applicable(
    packages: &BTreeMap<CompactString, BTreeSet<CompactString>>,
    advisories: BTreeMap<CompactString, Vec<Advisory>>,
) -> Result<Vec<AuditFinding>> {
    let mut findings = Vec::new();
    for (package, advisories) in advisories {
        let versions = packages
            .get(&package)
            .context("audit response includes an unrequested package")?;
        for advisory in advisories {
            node_semver::Range::parse(&advisory.vulnerable_versions)
                .context("audit returned an invalid vulnerable range")?;
            let affected: Vec<_> = versions
                .iter()
                .filter(|version| satisfies(&advisory.vulnerable_versions, version))
                .cloned()
                .collect();
            if !affected.is_empty() {
                findings.push(AuditFinding {
                    package: package.clone(),
                    versions: affected,
                    id: advisory.id,
                    title: advisory.title,
                    severity: advisory.severity,
                    vulnerable_versions: advisory.vulnerable_versions,
                    url: advisory.url,
                });
            }
        }
    }
    Ok(findings)
}

pub(super) fn query(
    root: &Utf8Path,
    config: &UniflowedConfig,
    operation: Operation<'_>,
    operands: &[String],
) -> Result<Value> {
    let routing = RegistryRouting::from_config(config);
    match operation {
        Operation::Info => {
            let name = operands.first().context("choose a package")?;
            let (name, range) = super::split_spec(name)?;
            let url = crate::registry::url_for(routing.route(&name).registry, &name)?;
            let document: Value = serde_json::from_slice(&download(
                &agent(),
                &url,
                crate::MAX_PACKUMENT_BYTES as u64,
            )?)?;
            let node =
                super::registry_node(&document, &name, &range, routing.route(&name).registry)?;
            Ok(document
                .get("versions")
                .and_then(|versions| versions.get(node.version.as_str()))
                .context("missing resolved metadata")?
                .clone())
        }
        Operation::Search => {
            let text = operands.join(" ");
            let mut encoded = String::new();
            for byte in text.bytes() {
                if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                    encoded.push(char::from(byte));
                } else {
                    uf_infra::append!(encoded, "%{byte:02X}");
                }
            }
            crate::registry::url_for(routing.default_registry(), "probe")?;
            let url = uf_infra::cstr!(
                "{}/-/v1/search?text={encoded}&size=20",
                routing.default_registry().trim_end_matches('/')
            );
            Ok(serde_json::from_slice(&download(
                &agent(),
                &url,
                crate::MAX_PACKUMENT_BYTES as u64,
            )?)?)
        }
        Operation::Why => {
            let name = operands.first().context("choose a package")?;
            let graph = read_graph(root, config)?;
            let mut parents = Vec::new();
            for (id, node) in &graph.nodes {
                for edges in [
                    &node.dependencies,
                    &node.optional_dependencies,
                    &node.peer_dependencies,
                ] {
                    for target in edges.values() {
                        if graph
                            .nodes
                            .get(target)
                            .is_some_and(|target| target.name == *name)
                        {
                            parents.push(serde_json::json!({"parent":id,"package":name,"version":graph.nodes[target].version}));
                        }
                    }
                }
            }
            for importer in &graph.importers {
                for edges in [
                    &importer.dependencies,
                    &importer.dev_dependencies,
                    &importer.optional_dependencies,
                    &importer.peer_dependencies,
                ] {
                    for target in edges.values() {
                        if graph
                            .nodes
                            .get(target)
                            .is_some_and(|target| target.name == *name)
                        {
                            parents.push(serde_json::json!({"parent":importer.path,"package":name,"version":graph.nodes[target].version}));
                        }
                    }
                }
            }
            Ok(Value::Array(parents))
        }
        _ => {
            let graph = read_graph(root, config)?;
            Ok(serde_json::to_value(graph)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advisory_filters_locked_versions_and_refuses_malformed_ranges() {
        let packages = BTreeMap::from([(
            "lib".into(),
            BTreeSet::from(["1.0.0".into(), "2.0.0".into()]),
        )]);
        let advisory = |range: &str| {
            BTreeMap::from([(
                "lib".into(),
                vec![Advisory {
                    id: 1,
                    title: "issue".into(),
                    severity: "high".into(),
                    vulnerable_versions: range.into(),
                    url: "https://example.org/advisory".into(),
                }],
            )])
        };
        let findings = applicable(&packages, advisory("<2")).unwrap();
        assert_eq!(findings[0].versions, vec![CompactString::from("1.0.0")]);
        assert!(applicable(&packages, advisory("not-a-range")).is_err());
    }
}
