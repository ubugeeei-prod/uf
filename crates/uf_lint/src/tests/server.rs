//! The server/client boundary: secrets, server-only imports, and the placement of
//! the `'use client'` and `'use server'` directives that draw the boundary.

use super::*;

#[test]
fn client_modules_may_not_import_server_only_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { db } from '@uniflowed/server';\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn client_modules_may_not_import_dot_server_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { load } from './data.server.js';\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn server_modules_may_import_server_only_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\nimport { db } from '@uniflowed/server';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn client_modules_may_import_shared_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { format } from './format.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn a_boundary_directive_must_lead_the_module() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\nimport { a } from './a.js';\n'use client';\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 1));
}

#[test]
fn a_leading_boundary_directive_is_accepted() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\n'use client';\nimport { a } from './a.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn an_inline_use_server_directive_is_not_a_module_directive() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\nexport async function save() {\n  'use server';\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn server_rule_rejects_secret_reads_in_client_modules() {
    let diagnostics = lint_one(
        "server/no-client-secret",
        "src/app/page.jsx",
        "// @flow\n'use client';\nconst token = process.env.PRIVATE_TOKEN;\n",
    );

    assert!(fired(&diagnostics, "server/no-client-secret"));
}

#[test]
fn server_actions_require_use_server_directive() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "server/actions.js",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn server_actions_accept_use_server_directive() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "server/actions.js",
        "\"use server\";\n// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn server_action_rule_ignores_the_removed_server_flow_spelling() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "actions.server.flow",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(!fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn server_action_rule_matches_dot_server_js_modules() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "actions.server.js",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn client_module_may_import_a_server_flow_named_module() {
    let diagnostics = lint_one(
        "server/no-server-only-import-in-client",
        "app/page.js",
        "// @flow\n'use client';\nimport { save } from './actions.server.flow';\n",
    );

    assert!(!fired(
        &diagnostics,
        "server/no-server-only-import-in-client"
    ));
}

#[test]
fn client_module_may_not_import_a_dot_server_js_module() {
    let diagnostics = lint_one(
        "server/no-server-only-import-in-client",
        "app/page.js",
        "// @flow\n'use client';\nimport { save } from './actions.server.js';\n",
    );

    assert!(fired(
        &diagnostics,
        "server/no-server-only-import-in-client"
    ));
}
