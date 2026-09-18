//! Static import rules that can answer without resolving the module graph.

use super::*;

#[test]
fn no_absolute_path_rejects_unix_roots() {
    let diagnostics = lint_js(
        "import/no-absolute-path",
        "// @flow\nimport local from \"/Users/me/app/value.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 19));
}

#[test]
fn no_absolute_path_rejects_windows_roots() {
    let diagnostics = lint_js(
        "import/no-absolute-path",
        "// @flow\nimport local from \"C:\\\\work\\\\app\\\\value.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_absolute_path_accepts_packages_and_relative_paths() {
    let diagnostics = lint_js(
        "import/no-absolute-path",
        "// @flow\nimport React from \"react\";\nimport local from \"./local.js\";\nimport fs from \"node:fs\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_duplicates_rejects_the_second_import_from_the_same_module() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport first from \"./thing.js\";\nimport { second } from \"./thing.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 24));
}

#[test]
fn no_duplicates_reads_multiline_imports() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport {\n  first,\n} from \"./thing.js\";\nimport second from \"./thing.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn no_duplicates_allows_type_and_value_imports_from_the_same_module() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport { value } from \"./thing.js\";\nimport type { Thing } from \"./thing.js\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_duplicates_rejects_two_type_imports_from_the_same_module() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport type { A } from \"./thing.js\";\nimport type { B } from \"./thing.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_duplicates_allows_a_namespace_import_beside_named_imports() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport * as React from \"react\";\nimport { useState } from \"react\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_duplicates_rejects_two_namespace_imports_from_the_same_module() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nimport * as one from \"./thing.js\";\nimport * as two from \"./thing.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_self_import_rejects_a_file_importing_itself_with_an_extension() {
    let diagnostics = lint_one(
        "import/no-self-import",
        "app/page.jsx",
        "// @flow\nimport Page from \"./page.jsx\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 18));
}

#[test]
fn no_self_import_rejects_a_file_importing_itself_without_an_extension() {
    let diagnostics = lint_one(
        "import/no-self-import",
        "app/page.jsx",
        "// @flow\nimport Page from \"./page\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_self_import_rejects_an_index_file_importing_its_directory() {
    let diagnostics = lint_one(
        "import/no-self-import",
        "app/index.js",
        "// @flow\nimport app from \".\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_self_import_accepts_neighbouring_relative_imports() {
    let diagnostics = lint_one(
        "import/no-self-import",
        "app/page.jsx",
        "// @flow\nimport index from \"./index.js\";\nimport parent from \"../page.jsx\";\nimport React from \"react\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn import_rules_ignore_strings_that_talk_about_imports() {
    let diagnostics = lint_js(
        "import/no-duplicates",
        "// @flow\nconst text = 'import x from \"./thing.js\"; import y from \"./thing.js\";';\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}
