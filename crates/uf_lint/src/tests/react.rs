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
fn component_rule_reads_past_an_export() {
    // The rule used to strip `const ` from the start of the line, so every
    // exported component in a codebase — which is most of them — was invisible
    // to it.
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nexport const Button = (): React.Node => null;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn component_rule_leaves_a_screaming_snake_constant_alone() {
    // `UNITS` begins with a capital and is not a component. Reporting it asks
    // the reader to rewrite an array of numbers as a React component.
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nconst UNITS: Array<number> = [1, 2];\nconst ROOT_ID = 'root';\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn component_rule_leaves_a_pascal_case_value_alone() {
    // A context, a schema and a client are all PascalCase by convention and
    // none of them is a function, let alone a component.
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nconst ThemeContext = React.createContext(null);\nconst Schema = { id: 1 };\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn component_rule_finds_an_arrow_whose_parameters_wrap() {
    // The `=>` is on a later line, so the opening `(` is all the rule has.
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nconst Card = ({\n  title,\n}: Props): React.Node => null;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn component_rule_finds_a_memoised_component() {
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nconst Row = React.memo(function Row() { return null; });\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn component_rule_accepts_the_syntax_it_asks_for() {
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nexport component Button() { return null; }\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
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
