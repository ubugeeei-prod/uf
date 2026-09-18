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
fn no_cycle_rejects_a_relative_import_cycle() {
    let diagnostics = lint_many(
        "import/no-cycle",
        &[
            ("src/a.js", "// @flow\nimport \"./b\";\n"),
            ("src/b.js", "// @flow\nimport \"./folder\";\n"),
            ("src/folder/index.js", "// @flow\nimport \"../a.js\";\n"),
        ],
    );

    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[0].path.as_deref(), Some("src/a.js"));
    assert!(diagnostics[0].message.contains("src/b.js"));
}

#[test]
fn no_cycle_accepts_acyclic_relative_imports_and_packages() {
    let diagnostics = lint_many(
        "import/no-cycle",
        &[
            (
                "src/a.js",
                "// @flow\nimport \"./b.js\";\nimport React from \"react\";\n",
            ),
            ("src/b.js", "// @flow\nimport \"./c.js\";\n"),
            ("src/c.js", "// @flow\nexport const value: number = 1;\n"),
        ],
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_extraneous_dependencies_rejects_packages_missing_from_the_nearest_manifest() {
    let diagnostics = lint_many(
        "import/no-extraneous-dependencies",
        &[
            (
                "package.json",
                r#"{ "name": "app", "dependencies": { "react": "^19.0.0" } }"#,
            ),
            (
                "app/page.js",
                "// @flow\nimport * as React from \"react\";\nimport leftPad from \"left-pad\";\n",
            ),
        ],
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 21));
    assert!(diagnostics[0].message.contains("`left-pad`"));
}

#[test]
fn no_extraneous_dependencies_uses_context_manifests_for_narrowed_runs() {
    let selected = [at(
        "app/page.js",
        "// @flow\nimport leftPad from \"left-pad\";\n",
    )];
    let context = [at("package.json", r#"{ "name": "app" }"#)];
    let diagnostics = lint_sources_with_context(
        &selected,
        &context,
        &only("import/no-extraneous-dependencies"),
    )
    .expect("lint")
    .diagnostics;

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path.as_deref(), Some("app/page.js"));
}

#[test]
fn no_extraneous_dependencies_accepts_declared_dependencies_and_self_imports() {
    let diagnostics = lint_many(
        "import/no-extraneous-dependencies",
        &[
            (
                "package.json",
                r#"{
                  "name": "@acme/app",
                  "dependencies": { "react": "^19.0.0" },
                  "devDependencies": { "@testing-library/react": "^16.0.0" },
                  "peerDependencies": { "react-dom": "^19.0.0" },
                  "optionalDependencies": { "fsevents": "^2.0.0" }
                }"#,
            ),
            (
                "app/page.js",
                "// @flow\nimport * as React from \"react/jsx-runtime\";\nimport { render } from \"@testing-library/react\";\nimport { createRoot } from \"react-dom/client\";\nimport fsevents from \"fsevents\";\nimport own from \"@acme/app/internal\";\n",
            ),
        ],
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_extraneous_dependencies_accepts_generated_router_manifests() {
    let diagnostics = lint_many(
        "import/no-extraneous-dependencies",
        &[
            ("package.json", r#"{ "name": "app" }"#),
            (
                "router.js",
                "// @flow\nimport { buildRoute } from \"@uniflowed/router/routing\";\n",
            ),
            (
                "app/page.js",
                "// @flow\nimport { buildRoute } from \"@uniflowed/router/routing\";\n",
            ),
        ],
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path.as_deref(), Some("app/page.js"));

    let mut config = only("import/no-extraneous-dependencies");
    config.app.router.manifest = CompactString::from("src/routes.js");
    let files = [
        at("package.json", r#"{ "name": "app" }"#),
        at(
            "src/routes.ios.js",
            "// @flow\nimport type { RouteTable } from \"@uniflowed/router/routing\";\n",
        ),
    ];
    let diagnostics = lint_sources(&files, &config).expect("lint").diagnostics;

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_extraneous_dependencies_uses_the_nearest_package_manifest() {
    let diagnostics = lint_many(
        "import/no-extraneous-dependencies",
        &[
            (
                "package.json",
                r#"{ "name": "root", "dependencies": { "left-pad": "^1.0.0" } }"#,
            ),
            (
                "packages/ui/package.json",
                r#"{ "name": "@acme/ui", "dependencies": { "react": "^19.0.0" } }"#,
            ),
            (
                "packages/ui/index.js",
                "// @flow\nimport * as React from \"react\";\nimport leftPad from \"left-pad\";\n",
            ),
        ],
    );

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("`left-pad`"));
}

#[test]
fn no_extraneous_dependencies_ignores_non_package_specifiers() {
    let diagnostics = lint_many(
        "import/no-extraneous-dependencies",
        &[
            ("package.json", r#"{ "name": "app" }"#),
            (
                "app/page.js",
                "// @flow\nimport fs from \"fs\";\nimport path from \"node:path\";\nimport local from \"./local.js\";\nimport privateName from \"#app/env\";\nimport alias from \"@/components/Button\";\n",
            ),
        ],
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
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
fn no_relative_packages_rejects_a_relative_path_into_another_workspace_package() {
    let diagnostics = lint_one(
        "import/no-relative-packages",
        "packages/web/src/page.js",
        "// @flow\nimport state from \"../../state/index.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 19));
    assert!(diagnostics[0].message.contains("`state` package"));
}

#[test]
fn no_relative_packages_accepts_package_imports_and_same_package_relatives() {
    let diagnostics = lint_one(
        "import/no-relative-packages",
        "packages/web/src/page.js",
        "// @flow\nimport state from \"@uniflowed/state\";\nimport local from \"../state/index.js\";\nimport sibling from \"./button.js\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_relative_packages_ignores_non_workspace_paths() {
    let diagnostics = lint_one(
        "import/no-relative-packages",
        "app/src/page.js",
        "// @flow\nimport shared from \"../shared/index.js\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_useless_path_segments_rejects_dotdot_segments_that_cancel_out() {
    let diagnostics = lint_one(
        "import/no-useless-path-segments",
        "app/page.js",
        "// @flow\nimport model from \"./features/../model.js\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 19));
    assert!(diagnostics[0].message.contains("`./model.js`"));
}

#[test]
fn no_useless_path_segments_keeps_js_index_modules() {
    let diagnostics = lint_one(
        "import/no-useless-path-segments",
        "app/page.js",
        "// @flow\nimport route from \"./routes/index.js\";\nimport local from \"./index\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn no_useless_path_segments_keeps_asset_index_files_with_query_suffixes() {
    let diagnostics = lint_one(
        "import/no-useless-path-segments",
        "app/page.js",
        "// @flow\nimport text from \"./content/index.js?raw\";\nimport data from \"./data/../data.json?raw\";\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("`./data.json?raw`"));
}

#[test]
fn no_useless_path_segments_accepts_already_short_relative_imports() {
    let diagnostics = lint_one(
        "import/no-useless-path-segments",
        "app/page.js",
        "// @flow\nimport local from \"./model.js\";\nimport parent from \"../shared.js\";\nimport pkg from \"react\";\n",
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
