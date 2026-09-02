//! `react-native/platform-split`, which prefers a `.ios.js` / `.android.js` pair
//! over a `Platform.OS` branch.

use super::*;

#[test]
fn react_native_rule_prefers_platform_files() {
    let diagnostics = lint_one(
        "react-native/platform-split",
        "src/app/Button.jsx",
        "// @flow\nimport { Platform } from '@uniflowed/react-native';\nconst name = Platform.OS;\n",
    );

    assert!(fired(&diagnostics, "react-native/platform-split"));
}
