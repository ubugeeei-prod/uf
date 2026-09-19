//! Named scripts remain usable by Expo; only disabled install hooks are refused.

use super::*;

#[test]
fn named_package_scripts_are_allowed() {
    let diagnostics = lint_one(
        "package/no-npm-scripts",
        "package.json",
        "{\n  \"scripts\": { \"start\": \"expo start\", \"ios\": \"expo start --ios\", \"android\": \"expo start --android\", \"web\": \"expo start --web\" }\n}\n",
    );

    assert!(!fired(&diagnostics, "package/no-npm-scripts"));
}

#[test]
fn every_install_hook_uses_the_package_managers_refusal() {
    for name in uf_config::INSTALL_LIFECYCLE_SCRIPTS {
        let source = format!("{{\"scripts\":{{\"{name}\":\"run-code\"}}}}");
        let diagnostics = lint_one("package/no-npm-scripts", "package.json", &source);
        assert!(fired(&diagnostics, "package/no-npm-scripts"), "{name}");
    }
}
