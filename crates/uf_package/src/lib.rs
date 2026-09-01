use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type TargetPackages = SmallVec<[GeneratedTargetPackage; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePackageSpec {
    pub name: CompactString,
    pub version: CompactString,
    pub scope: CompactString,
    pub targets: SmallVec<[NativePackageTarget; 8]>,
}

impl NativePackageSpec {
    pub fn new(name: impl Into<CompactString>, version: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            scope: CompactString::const_new("@uniflowed"),
            targets: smallvec::smallvec![
                NativePackageTarget::NodeNapi,
                NativePackageTarget::BunNapi,
                NativePackageTarget::DenoNapi,
                NativePackageTarget::EdgeWasm,
                NativePackageTarget::ServerlessNapi,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativePackageTarget {
    NodeNapi,
    BunNapi,
    DenoNapi,
    EdgeWasm,
    ServerlessNapi,
}

impl NativePackageTarget {
    pub fn package_suffix(self) -> &'static str {
        match self {
            Self::NodeNapi => "node-napi",
            Self::BunNapi => "bun-napi",
            Self::DenoNapi => "deno-napi",
            Self::EdgeWasm => "edge-wasm",
            Self::ServerlessNapi => "serverless-napi",
        }
    }

    pub fn artifact_extension(self) -> &'static str {
        match self {
            Self::EdgeWasm => "wasm",
            Self::NodeNapi | Self::BunNapi | Self::DenoNapi | Self::ServerlessNapi => "node",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTargetPackage {
    pub package_name: CompactString,
    pub target: NativePackageTarget,
    pub artifact: CompactString,
    pub declaration: FlowDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDeclaration {
    pub path: CompactString,
    pub source: CompactString,
}

pub fn generate_target_packages(
    spec: &NativePackageSpec,
    typescript_declaration: &str,
) -> TargetPackages {
    let flow_declaration = convert_typescript_declaration_to_flow(typescript_declaration);
    spec.targets
        .iter()
        .map(|target| {
            let target = *target;
            let suffix = target.package_suffix();
            GeneratedTargetPackage {
                package_name: CompactString::from(format!(
                    "{}/{}-{}",
                    spec.scope, spec.name, suffix
                )),
                target,
                artifact: CompactString::from(format!(
                    "{}.{}.{}",
                    spec.name,
                    suffix,
                    target.artifact_extension()
                )),
                declaration: FlowDeclaration {
                    path: CompactString::const_new("index.js.flow"),
                    source: flow_declaration.clone(),
                },
            }
        })
        .collect()
}

pub fn convert_typescript_declaration_to_flow(source: &str) -> CompactString {
    let mut flow = String::with_capacity(source.len() + 16);
    flow.push_str("// @flow\n");

    for line in source.lines() {
        let line = line
            .replace("export declare function ", "declare export function ")
            .replace("export declare const ", "declare export var ")
            .replace("export declare type ", "declare export type ")
            .replace("readonly ", "+")
            .replace(": unknown", ": mixed");
        flow.push_str(line.trim_end());
        flow.push('\n');
    }

    CompactString::from(flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_generated_typescript_declarations_to_flow() {
        let source = "export declare function greet(name: string): Promise<string>;\nexport declare const version: string;";
        let flow = convert_typescript_declaration_to_flow(source);

        assert!(flow.starts_with("// @flow\n"));
        assert!(flow.contains("declare export function greet(name: string): Promise<string>;"));
        assert!(flow.contains("declare export var version: string;"));
    }

    #[test]
    fn generates_target_packages_for_napi_and_wasm_hosts() {
        let spec = NativePackageSpec::new("runtime", "0.1.0");
        let packages = generate_target_packages(
            &spec,
            "export declare function run(entry: string): Promise<void>;",
        );

        assert_eq!(packages.len(), 5);
        assert!(
            packages
                .iter()
                .any(|package| package.package_name == "@uniflowed/runtime-node-napi")
        );
        assert!(
            packages
                .iter()
                .any(|package| package.artifact == "runtime.edge-wasm.wasm")
        );
        assert!(
            packages
                .iter()
                .all(|package| package.declaration.path == "index.js.flow")
        );
    }
}
