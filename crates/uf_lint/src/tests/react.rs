//! The `react/*` rules that read a single construct: Flow's `component` and
//! `hook` spellings, the default-export ban, and side effects during render.

use super::*;

#[test]
fn no_default_export_component_rejects_a_default_export() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\ncomponent Page() { return null; }\nexport default Page;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 1));
}

#[test]
fn no_default_export_component_accepts_named_exports() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\nexport component Page() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn no_default_export_component_covers_reserved_router_modules() {
    let diagnostics = lint_one(
        "react/no-default-export-component",
        "app/_uf.page.js",
        "// @flow\nexport default function Page() { return null; }\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_default_export_component_leaves_plain_modules_alone() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\nexport default { id: 1 };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn framework_rule_prefers_component_syntax() {
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nfunction Button(): React.Node { return null; }\n",
    );

    assert!(fired(&diagnostics, "react/component-syntax"));
}

#[test]
fn hook_rule_prefers_flow_hook_syntax() {
    let diagnostics = lint_one(
        "react/hook-syntax",
        "src/app/page.jsx",
        "// @flow\nfunction useThing(): number { return 1; }\n",
    );

    assert!(fired(&diagnostics, "react/hook-syntax"));
}

#[test]
fn render_side_effects_are_errors_by_default() {
    let diagnostics = lint_one(
        "react/no-render-side-effects",
        "src/app/page.jsx",
        "// @flow\ncomponent Clock() { return <p>{Date.now()}</p>; }\n",
    );

    assert!(fired(&diagnostics, "react/no-render-side-effects"));
}
