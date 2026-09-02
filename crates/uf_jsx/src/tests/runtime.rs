//! The runtime import, and where the options for it come from.

use super::lower;
use crate::{
    Helpers, JSX_RUNTIME_SPECIFIER, JsxError, JsxOptions, ReactRuntime, import_offset,
    runtime_import, transform,
};
use uf_config::UniflowedConfig;

#[test]
fn a_module_with_jsx_imports_the_helpers_it_uses() {
    let out = lower("const a = <div />;\n");

    assert!(
        out.starts_with("import { jsx as _jsx } from \"@uniflowed/jsx-runtime\";"),
        "{out}"
    );
}

#[test]
fn a_module_using_the_list_form_imports_jsxs_too() {
    let out = lower("const a = <div>{p}{q}<span /></div>;\n");

    assert!(out.contains("jsx as _jsx,"), "{out}");
    assert!(out.contains("jsxs as _jsxs"), "{out}");
}

#[test]
fn a_module_using_a_fragment_imports_the_fragment_helper() {
    let out = lower("const a = <>{p}</>;\n");

    assert!(out.contains("Fragment as _Fragment"), "{out}");
}

#[test]
fn a_module_only_using_jsxs_does_not_import_jsx() {
    let transformed =
        transform("const a = <div>{p}{q}</div>;\n", &JsxOptions::default()).expect("lowers");

    assert!(transformed.helpers.jsxs);
    assert!(
        !transformed.code.contains("jsx as _jsx,"),
        "{}",
        transformed.code
    );
}

#[test]
fn the_import_is_emitted_once_however_many_elements_there_are() {
    let out = lower("const a = <p>1</p>;\nconst b = <p>2</p>;\nconst c = <p>3</p>;\n");

    assert_eq!(out.matches("@uniflowed/jsx-runtime").count(), 1, "{out}");
}

#[test]
fn a_module_with_no_jsx_gains_no_import() {
    let transformed = transform("export const a = 1;\n", &JsxOptions::default()).expect("lowers");

    assert!(transformed.is_unchanged());
    assert_eq!(transformed.code, "export const a = 1;\n");
}

#[test]
fn the_import_does_not_add_a_line() {
    let source = "// @flow\nconst a = <div />;\n";

    let out = lower(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(
        out.lines().next().expect("a line").contains("// @flow"),
        "{out}"
    );
}

#[test]
fn the_import_goes_after_a_shebang() {
    let source = "#!/usr/bin/env uf\nconst a = <div />;\n";

    let out = lower(source);

    assert!(out.starts_with("#!/usr/bin/env uf\nimport {"), "{out}");
}

#[test]
fn the_import_goes_after_a_byte_order_mark() {
    let source = "\u{feff}const a = <div />;\n";

    let out = lower(source);

    assert!(out.starts_with('\u{feff}'), "{out}");
    assert!(out.contains("import {"), "{out}");
}

#[test]
fn the_import_offset_skips_a_mark_and_a_shebang() {
    assert_eq!(import_offset("const a = 1;\n"), 0);
    assert_eq!(import_offset("\u{feff}const a = 1;\n"), 3);
    assert_eq!(import_offset("#!/x\nconst a = 1;\n"), 5);
    assert_eq!(import_offset("\u{feff}#!/x\nconst a = 1;\n"), 8);
}

#[test]
fn the_import_names_the_uniflowed_runtime() {
    let helpers = Helpers {
        jsx: true,
        jsxs: true,
        fragment: true,
    };

    assert_eq!(
        runtime_import(helpers, JSX_RUNTIME_SPECIFIER),
        "import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from \"@uniflowed/jsx-runtime\";"
    );
}

#[test]
fn helpers_report_whether_any_is_needed() {
    assert!(!Helpers::default().any());
    assert!(
        Helpers {
            jsx: true,
            ..Helpers::default()
        }
        .any()
    );
}

#[test]
fn react_nineteen_uses_the_automatic_runtime() {
    assert_eq!(ReactRuntime::from_version("19"), ReactRuntime::Automatic);
    assert_eq!(
        ReactRuntime::from_version("19.2.0"),
        ReactRuntime::Automatic
    );
    assert_eq!(
        ReactRuntime::from_version("^18.3.1"),
        ReactRuntime::Automatic
    );
    assert_eq!(ReactRuntime::from_version("17"), ReactRuntime::Automatic);
}

#[test]
fn react_sixteen_and_earlier_use_the_classic_runtime() {
    assert_eq!(ReactRuntime::from_version("16"), ReactRuntime::Classic);
    assert_eq!(ReactRuntime::from_version("16.14.0"), ReactRuntime::Classic);
    assert_eq!(ReactRuntime::from_version("~15.6"), ReactRuntime::Classic);
}

#[test]
fn an_unreadable_version_is_read_as_the_current_runtime() {
    assert_eq!(
        ReactRuntime::from_version("latest"),
        ReactRuntime::Automatic
    );
    assert_eq!(ReactRuntime::from_version(""), ReactRuntime::Automatic);
}

#[test]
fn the_runtime_names_match_flows_own_spelling() {
    assert_eq!(ReactRuntime::Automatic.as_str(), "automatic");
    assert_eq!(ReactRuntime::Classic.as_str(), "classic");
}

#[test]
fn the_options_come_from_the_react_version_a_project_declares() {
    let config = UniflowedConfig::default();

    let options = JsxOptions::from_config(&config);

    assert_eq!(config.app.react.version.as_str(), "19");
    assert_eq!(options.runtime, ReactRuntime::Automatic);
    assert_eq!(options.import_source.as_str(), JSX_RUNTIME_SPECIFIER);
}

#[test]
fn a_project_needing_the_classic_runtime_is_refused_rather_than_miscompiled() {
    let mut config = UniflowedConfig::default();
    config.app.react.version = "16.14.0".into();
    let options = JsxOptions::from_config(&config);

    let error = transform("const a = <div />;\n", &options).expect_err("refused");

    assert!(matches!(error, JsxError::ClassicRuntimeUnsupported { .. }));
    assert!(error.to_string().contains("React 17 or later"));
}

#[test]
fn a_classic_project_with_no_jsx_still_builds() {
    let options = JsxOptions {
        runtime: ReactRuntime::Classic,
        ..JsxOptions::default()
    };

    let transformed = transform("export const a = 1;\n", &options).expect("lowers");

    assert!(transformed.is_unchanged());
}

#[test]
fn a_custom_import_source_is_honoured() {
    let options = JsxOptions {
        import_source: "react/jsx-runtime".into(),
        ..JsxOptions::default()
    };

    let transformed = transform("const a = <div />;\n", &options).expect("lowers");

    assert!(
        transformed.code.contains("from \"react/jsx-runtime\";"),
        "{}",
        transformed.code
    );
}

#[test]
fn the_element_count_is_reported() {
    let transformed =
        transform("const a = <div><p /><p /></div>;\n", &JsxOptions::default()).expect("lowers");

    assert_eq!(transformed.elements, 3);
    assert!(!transformed.is_unchanged());
}
