//! `package/no-npm-scripts`: a `scripts` block in `package.json` is a task
//! nothing in a uf project will ever run.

use super::*;

#[test]
fn package_json_scripts_are_rejected() {
    let diagnostics = lint_one(
        "package/no-npm-scripts",
        "package.json",
        "{\n  \"scripts\": { \"dev\": \"vite\" }\n}\n",
    );

    assert!(fired(&diagnostics, "package/no-npm-scripts"));
}
