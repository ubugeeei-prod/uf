//! Flow lints over what a module declares and imports, including top-level names
//! that collide with a JSX intrinsic element.

use super::*;

#[test]
fn mixed_import_and_require_rejects_a_require_in_an_esm_module() {
    let diagnostics = lint_js(
        "flow/mixed-import-and-require",
        "// @flow\nimport { a } from './a.js';\nconst b = require('./b.js');\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 11));
}

#[test]
fn mixed_import_and_require_accepts_a_pure_esm_module() {
    let diagnostics = lint_js(
        "flow/mixed-import-and-require",
        "// @flow\nimport { a } from './a.js';\nimport { b } from './b.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn non_const_var_export_rejects_mutable_exports() {
    let diagnostics = lint_js(
        "flow/non-const-var-export",
        "// @flow\nexport let count = 0;\nexport var total = 0;\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 8));
}

#[test]
fn non_const_var_export_accepts_const_exports() {
    let diagnostics = lint_js(
        "flow/non-const-var-export",
        "// @flow\nexport const count = 0;\nlet local = 1;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn export_renamed_default_rejects_as_default() {
    let diagnostics = lint_js(
        "flow/export-renamed-default",
        "// @flow\nconst page = 1;\nexport { page as default };\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn export_renamed_default_accepts_importing_the_default() {
    let diagnostics = lint_js(
        "flow/export-renamed-default",
        "// @flow\nimport { default as page } from './page.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn react_intrinsic_overlap_rejects_shadowed_tag_names() {
    let diagnostics = lint_js(
        "flow/react-intrinsic-overlap",
        "// @flow\nconst div = 1;\nexport function span() {}\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 7));
}

#[test]
fn react_intrinsic_overlap_accepts_ordinary_names() {
    let diagnostics = lint_js(
        "flow/react-intrinsic-overlap",
        "// @flow\nconst divider = 1;\ncomponent Section() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}
