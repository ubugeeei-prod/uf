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
fn no_default_export_component_reads_code_and_not_the_prose_beside_it() {
    // The rule only looks at a file that declares a `component`, and that fact
    // was a substring search over the whole file. So a module whose comment
    // said "server-component analysis" — `packages/vite/index.js` does — was
    // treated as declaring one, and its default export, a Vite plugin factory,
    // was reported. The comment was reworded to get a clean run, which is the
    // wrong direction: the file was right and the rule was not.
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\n// The server-component analysis, which is not a component declaration.\nexport default function plugin() { return {}; }\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    // A string is not a declaration either. This is the shape a generated
    // entry point has: source written as text, to be emitted rather than run.
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\nconst source = \"component Page() { return null; }\";\nexport default source;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    // And the rule still fires on the thing it is about, so this is not a
    // fact that has quietly stopped being computed.
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\n// The server-component analysis, mentioned in a comment.\ncomponent Page() { return null; }\nexport default Page;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
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

/// ubugeeei-prod/uf#451: source a module *generates* is not source it *is*.
///
/// `packages/vite/driver.js` builds a Cloudflare Worker entry as text, and the
/// generated `export default { fetch: … }` was reported against the line of the
/// file holding the template. Every adapter does this, and so does
/// `internal/routes.js`.
#[test]
fn a_default_export_inside_a_template_literal_belongs_to_the_generated_file() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\ncomponent A() { return null; }\n\nexport const entry: string = `\nexport default { fetch: handle };\n`;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// And this module's own default export is still reported.
#[test]
fn a_default_export_outside_a_template_is_still_this_modules() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\ncomponent A() { return null; }\nexport default A;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

/// ubugeeei-prod/uf#477: a hook called in an object literal is not a
/// conditional call.
///
/// The reproduction from the issue, reported through `uf lint` rather than
/// through the validator, because that is where a reader met it: ten store
/// selections gathered into one object gave ten errors, in a rule a project
/// cannot switch off on its own.
#[test]
fn hooks_rules_accepts_a_hook_called_in_an_object_literal() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nimport { useState } from \"react\";\n\nexport hook useThing(): { a: number, b: number } {\n  const bag = {\n    a: useState(0)[0],\n    b: useState(1)[0],\n  };\n\n  return bag;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// And the same literal inside a condition is still a conditional call.
#[test]
fn hooks_rules_still_rejects_an_object_literal_built_conditionally() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nimport { useState } from \"react\";\n\nexport hook useThing(flag: boolean): { a: number } | null {\n  if (flag) {\n    return { a: useState(0)[0] };\n  }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}
