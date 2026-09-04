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

/// The rule is not a syntactic one, and uf's syntactic version was wrong.
///
/// It flagged any `const`/`let`/`var` whose name matched an HTML element, on
/// the grounds that shadowing one "silently changes what JSX means". It does
/// not: a lowercase tag is always an intrinsic, resolved to the string and
/// never from scope — `const body = 42; <body />` still compiles to
/// `_jsx("body")`. The rule reported 90 errors against uf's own packages for
/// naming variables `source`, `table`, `text` and `slot`.
#[test]
fn react_intrinsic_overlap_does_not_flag_an_ordinary_local_binding() {
    let diagnostics = lint_js(
        "flow/react-intrinsic-overlap",
        "// @flow\nconst div = 1;\nconst body = 42;\nconst table = [];\n",
    );

    assert!(
        diagnostics.is_empty(),
        "naming a variable after an element is not a bug: {diagnostics:?}"
    );
}
