//! Compare installed native dependencies with the app's Expo SDK and RN peers.
//! These are warnings: Expo and the selected package manager still own installs.

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;

use crate::{Range, Version};

fn read(path: &Utf8Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn installed(root: &Utf8Path, name: &str, file: &str) -> Option<Utf8PathBuf> {
    root.ancestors()
        .map(|dir| dir.join("node_modules").join(name).join(file))
        .find(|path| path.is_file())
}

/// Diagnose installed versions without trusting a declared range as a version.
#[must_use]
pub fn compatibility_warnings(root: &Utf8Path) -> Vec<String> {
    let Some(manifest) = read(&root.join("package.json")) else {
        return Vec::new();
    };
    let expo = installed(root, "expo", "package.json").and_then(|path| read(&path));
    let expected =
        installed(root, "expo", "bundledNativeModules.json").and_then(|path| read(&path));
    let rn = installed(root, "react-native", "package.json").and_then(|path| read(&path));
    if expo.is_none() && rn.is_none() {
        return Vec::new();
    }
    let mut names = BTreeSet::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(deps) = manifest.get(field).and_then(Value::as_object) {
            names.extend(deps.keys());
        }
    }
    let mut warnings = Vec::new();
    for name in names {
        let Some(dependency) = installed(root, name, "package.json").and_then(|path| read(&path))
        else {
            continue;
        };
        if let (Some(range), Some(version)) = (
            expected
                .as_ref()
                .and_then(|value| value.get(name))
                .and_then(Value::as_str),
            dependency.get("version").and_then(Value::as_str),
        ) {
            let owner = uf_infra::into_string(uf_infra::cstr!(
                "Expo {}",
                expo.as_ref()
                    .and_then(|value| value.get("version"))
                    .and_then(Value::as_str)
                    .unwrap_or("SDK")
            ));
            compare(&mut warnings, name, version, range, &owner);
        }
        if let (Some(range), Some(version)) = (
            dependency
                .pointer("/peerDependencies/react-native")
                .and_then(Value::as_str),
            rn.as_ref()
                .and_then(|value| value.get("version"))
                .and_then(Value::as_str),
        ) {
            compare(&mut warnings, "react-native", version, range, name);
        }
    }
    warnings
}

fn compare(warnings: &mut Vec<String>, package: &str, version: &str, range: &str, owner: &str) {
    let parsed = Version::parse(version);
    let alternatives: Option<Vec<_>> = range
        .split("||")
        .map(|part| Range::parse(part.trim()))
        .collect();
    match (parsed, alternatives) {
        (Some(version), Some(alternatives)) if alternatives.iter().any(|range| range.allows(&version)) => {}
        (Some(_), Some(_)) => warnings.push(uf_infra::into_string(uf_infra::cstr!("{owner} requires {package}@{range}, but {version} is installed; use `uf add {package}@'{range}'`"))),
        _ if range == "*" => {}
        _ => warnings.push(uf_infra::into_string(uf_infra::cstr!("native compatibility: {owner} requires {package}@{range}; uf cannot compare this range with {version}. Check the peer range or run `expo install --check` before bundling"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_and_peer_mismatches_name_the_compatible_release() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        for (file, value) in [
            (
                "package.json",
                r#"{"dependencies":{"native-addon":"1.0.0"}}"#,
            ),
            ("node_modules/expo/package.json", r#"{"version":"57.0.22"}"#),
            (
                "node_modules/expo/bundledNativeModules.json",
                r#"{"native-addon":"~1.2.0"}"#,
            ),
            (
                "node_modules/react-native/package.json",
                r#"{"version":"0.86.3"}"#,
            ),
            (
                "node_modules/native-addon/package.json",
                r#"{"version":"1.0.0","peerDependencies":{"react-native":"^0.87.0"}}"#,
            ),
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, value).unwrap();
        }
        let warnings = compatibility_warnings(root);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("native-addon@~1.2.0"));
        assert!(warnings[1].contains("react-native@^0.87.0"));
    }
}
