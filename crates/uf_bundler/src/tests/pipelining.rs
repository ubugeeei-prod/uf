//! The plugin container, and the built-in stages now wired into it.

use camino::Utf8Path;
use uf_config::{PipelineMode, UniflowedConfig};
use uf_plugin::PluginHook;
use uf_router::{Route, RouteParam, RouteParamKind};

use super::fixture::{Fixture, assert_chunks_parse};
use crate::pipeline::{
    ROUTE_TABLE_MODULE, ROUTE_TABLE_SPECIFIER, asset_extension, asset_file_name, asset_url,
    blank_directive_prologue, build_entries, build_pipeline, is_mdx_module, mdx_module,
};

fn route(path: &str, page: &str) -> Route {
    Route {
        path: path.into(),
        directory: Utf8Path::new(page)
            .parent()
            .unwrap_or(Utf8Path::new(""))
            .to_path_buf(),
        page: page.into(),
        params: vec![RouteParam {
            name: "id".into(),
            kind: RouteParamKind::Single,
        }],
        has_layout: true,
        has_middleware: false,
    }
}

#[test]
fn the_pipeline_places_flow_before_the_react_compiler() {
    let fixture = Fixture::new();

    let container = fixture.container();

    let order: Vec<&str> = container.names().collect();
    assert!(
        order.iter().position(|name| *name == "uf:flow")
            < order.iter().position(|name| *name == "uf:react-compiler"),
        "{order:?}"
    );
    fixture.keep();
}

#[test]
fn every_default_builtin_is_in_the_pipeline() {
    let fixture = Fixture::new();

    let container = fixture.container();
    let names: Vec<&str> = container.names().collect();

    for expected in ["uf:mdx", "uf:flow", "uf:router", "uf:rsc", "uf:asset"] {
        assert!(names.contains(&expected), "{names:?}");
    }
    fixture.keep();
}

#[test]
fn the_mdx_stage_runs_before_flow() {
    let fixture = Fixture::new();

    let container = fixture.container();
    let transformers = container
        .plugins_for(PluginHook::Transform)
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>();

    assert!(
        transformers.iter().position(|name| *name == "uf:mdx")
            < transformers.iter().position(|name| *name == "uf:flow"),
        "{transformers:?}"
    );
    fixture.keep();
}

#[test]
fn the_mdx_stage_turns_documents_into_javascript_modules() {
    let fixture = Fixture::new();
    let container = fixture.container();

    let outcome = container
        .transform("docs/intro.mdx", "# Hello\n\nFlow docs")
        .expect("transform runs");

    let code = outcome.handled().expect("mdx handled").code;
    assert!(code.contains("export const mdxSource"), "{code}");
    assert!(code.contains("MdxDocument"), "{code}");
    assert!(!code.contains("renders React.Node"), "{code}");
    assert!(
        uf_flow::validate_source(&code).expect("parser ran").is_ok(),
        "{code}"
    );
    fixture.keep();
}

#[test]
fn an_mdx_import_is_bundled_without_plugin_config() {
    let mut fixture = Fixture::new();
    fixture.write("docs/intro.mdx", "# Hello\n\nFlow docs");
    fixture.entry(
        "app.js",
        "import Intro from \"./docs/intro.mdx\";\nexport default Intro;\n",
    );

    let output = fixture.bundle();

    assert!(
        output.chunks[0].code.contains("mdxSource"),
        "{}",
        output.chunks[0].code
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn the_flow_stage_is_connected_to_a_transform() {
    let fixture = Fixture::new();
    let container = fixture.container();

    let outcome = container
        .transform("app.js", "// @flow\nconst a: number = 1;\n")
        .expect("transform runs");

    assert!(outcome.is_handled());
    assert!(!outcome.handled().expect("code").code.contains(": number"));
    fixture.keep();
}

#[test]
fn the_flow_stage_passes_plain_javascript_through() {
    let fixture = Fixture::new();

    let outcome = fixture
        .container()
        .transform("app.js", "const a = 1;\n")
        .expect("transform runs");

    assert!(outcome.is_passthrough());
    fixture.keep();
}

#[test]
fn the_router_stage_resolves_and_generates_the_route_table() {
    let mut fixture = Fixture::new();
    fixture.routes = vec![route("/thing/:id", "app/thing/_uf.page.js")];
    let container = fixture.container();

    let resolved = container
        .resolve_id(ROUTE_TABLE_SPECIFIER, Some("app.js"))
        .expect("resolve runs")
        .handled()
        .expect("claimed");
    let loaded = container
        .load(&resolved.id)
        .expect("load runs")
        .handled()
        .expect("claimed");

    assert_eq!(resolved.id, ROUTE_TABLE_MODULE);
    assert!(loaded.code.contains("\"/thing/:id\""), "{}", loaded.code);
    assert!(
        loaded.code.contains("export const routes"),
        "{}",
        loaded.code
    );
    fixture.keep();
}

#[test]
fn the_generated_route_table_parses_as_javascript() {
    let mut fixture = Fixture::new();
    fixture.routes = vec![route("/", "app/_uf.page.js")];
    let container = fixture.container();

    let loaded = container
        .load(ROUTE_TABLE_MODULE)
        .expect("load runs")
        .handled()
        .expect("claimed");

    let outcome = uf_flow::validate_source(&loaded.code).expect("parser ran");
    assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    fixture.keep();
}

#[test]
fn the_route_table_can_be_imported_by_a_module() {
    let mut fixture = Fixture::new();
    fixture.routes = vec![route("/", "app/_uf.page.js")];
    fixture.entry(
        "app.js",
        "import { routes } from \"@uniflowed/router/routes\";\nexport const all = routes;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("const routes = ["), "{code}");
    assert!(code.contains("app/_uf.page.js"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn the_rsc_stage_blanks_a_client_directive() {
    let fixture = Fixture::new();

    let outcome = fixture
        .container()
        .transform("app.js", "\"use client\";\nconst a = 1;\n")
        .expect("transform runs");

    let code = outcome.handled().expect("code").code;
    assert!(!code.contains("use client"), "{code}");
    assert_eq!(code.lines().count(), 2);
    fixture.keep();
}

#[test]
fn a_use_strict_directive_is_left_alone() {
    assert_eq!(
        blank_directive_prologue("\"use strict\";\nconst a = 1;\n"),
        None
    );
}

#[test]
fn a_directive_inside_a_function_is_left_alone() {
    assert_eq!(
        blank_directive_prologue("function f() {\n  \"use client\";\n}\n"),
        None
    );
}

#[test]
fn blanking_a_directive_keeps_the_byte_length() {
    let source = "\"use server\";\nconst a = 1;\n";

    let blanked = blank_directive_prologue(source).expect("blanked");

    assert_eq!(blanked.len(), source.len());
    assert!(blanked.starts_with("             \n"));
}

#[test]
fn the_asset_stage_resolves_a_stylesheet_import() {
    let mut fixture = Fixture::new();
    fixture.write("app/styles.css", "body { color: red; }\n");
    fixture.entry(
        "app/page.js",
        "import url from \"./styles.css\";\nexport const style = url;\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.assets.len(), 1);
    assert_eq!(output.assets[0].as_str(), "app/styles.css");
    assert!(output.chunks[0].code.contains("/assets/app-styles.css"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_asset_is_copied_into_the_output_directory() {
    let mut fixture = Fixture::new();
    fixture.write("app/styles.css", "body { color: red; }\n");
    fixture.entry(
        "app/page.js",
        "import url from \"./styles.css\";\nexport const style = url;\n",
    );

    let output = fixture.bundle();
    let written = crate::write_bundle(&fixture.options(), &output, &fixture.container())
        .expect("write succeeds");

    let copied = fixture.out_dir().join("assets/app-styles.css");
    assert!(written.contains(&copied), "{written:?}");
    assert_eq!(
        std::fs::read_to_string(&copied).expect("read"),
        "body { color: red; }\n"
    );
    fixture.keep();
}

#[test]
fn a_javascript_import_is_not_claimed_by_the_asset_stage() {
    assert_eq!(asset_extension(Utf8Path::new("app/page.js")), None);
    assert_eq!(asset_extension(Utf8Path::new("docs/page.mdx")), None);
    assert_eq!(asset_extension(Utf8Path::new("app/style.css")), Some("css"));
    assert_eq!(asset_extension(Utf8Path::new("app/logo.svg")), Some("svg"));
    assert_eq!(asset_extension(Utf8Path::new("app/no-extension")), None);
}

#[test]
fn mdx_detection_is_extension_based() {
    assert!(is_mdx_module(Utf8Path::new("docs/page.mdx")));
    assert!(!is_mdx_module(Utf8Path::new("docs/page.md")));
    assert!(!is_mdx_module(Utf8Path::new("app/page.js")));

    let code = mdx_module("# Title");
    assert!(
        code.contains("export component MdxDocument() renders React.Node"),
        "{code}"
    );
    assert!(code.contains("\"# Title\""), "{code}");
}

#[test]
fn an_asset_url_is_derived_from_its_module_path() {
    assert_eq!(
        asset_url(Utf8Path::new("app/deep/logo.svg")),
        "/assets/app-deep-logo.svg"
    );
    assert_eq!(
        asset_file_name(Utf8Path::new("app/deep/logo.svg")),
        "app-deep-logo.svg"
    );
}

#[test]
fn the_style_and_react_compiler_stages_are_placed_but_transform_nothing() {
    let mut config = UniflowedConfig::default();
    config.app.builtins.react_compiler.enabled = true;
    let fixture = Fixture::new();
    let container = build_pipeline(&config, &fixture.root, PipelineMode::Build, &[])
        .expect("pipeline resolves");

    let names: Vec<&str> = container.names().collect();

    assert!(names.contains(&"uf:style"), "{names:?}");
    assert!(container.implements(PluginHook::Transform));
    fixture.keep();
}

#[test]
fn a_plugin_entry_outside_the_project_is_refused() {
    let mut config = UniflowedConfig::default();
    config
        .plugins
        .push(uf_config::PluginEntry::Name("../evil.js".into()));
    let fixture = Fixture::new();

    let error =
        build_pipeline(&config, &fixture.root, PipelineMode::Build, &[]).expect_err("refused");

    assert!(matches!(error, crate::PipelineError::Resolve(_)));
    fixture.keep();
}

#[test]
fn build_entries_takes_the_config_entries_and_every_route() {
    let fixture = Fixture::new();
    fixture.write("app.js", "export const a = 1;\n");
    let config = UniflowedConfig::default();
    let routes = vec![route("/", fixture.root.join("app/_uf.page.js").as_str())];

    let entries = build_entries(&config, &fixture.root, &routes);

    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.as_str())
            .collect::<Vec<_>>(),
        vec!["app/_uf.page.js", "app.js"]
    );
    fixture.keep();
}

#[test]
fn build_entries_skips_a_config_entry_that_does_not_exist() {
    let fixture = Fixture::new();
    let config = UniflowedConfig::default();

    assert!(build_entries(&config, &fixture.root, &[]).is_empty());
    fixture.keep();
}

#[test]
fn every_broadcast_hook_runs_without_failing() {
    let fixture = Fixture::new();
    let container = fixture.container();

    for hook in [
        PluginHook::BuildStart,
        PluginHook::BuildEnd,
        PluginHook::GenerateBundle,
        PluginHook::WriteBundle,
    ] {
        container.notify(hook).expect("notify runs");
    }
    fixture.keep();
}
