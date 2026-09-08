use super::*;

#[test]
fn zero_config_defaults_to_flow_react_app_stack() {
    let config = UniflowedConfig::default();

    assert!(config.app.router.enabled);
    assert_eq!(config.app.router.entry, "app.js");
    assert_eq!(config.app.router.root, "app");
    assert_eq!(config.app.component_default, ComponentBoundary::Server);
    assert_eq!(config.app.react.version, "19");
    assert!(config.app.react.async_react);
    assert!(config.app.react.suspense);
    assert!(config.app.react.use_hook);
    // Strict Mode is on unless a project turns it off, and `uf dev` is the only
    // command that acts on it. ubugeeei-prod/uf#516.
    assert!(config.app.react.strict_mode);
    assert!(config.app.rsc);
    assert!(config.app.server_actions);
    assert_eq!(config.app.runtime.default, RuntimeEngine::Node);
    assert!(config.app.runtime.deploy.enabled);
    assert!(
        config
            .app
            .runtime
            .compatibility
            .contains(&RuntimeEngine::Node)
    );
    assert!(
        config
            .app
            .runtime
            .compatibility
            .contains(&RuntimeEngine::Deno)
    );
    assert!(
        config
            .app
            .runtime
            .compatibility
            .contains(&RuntimeEngine::Bun)
    );
    assert_eq!(
        config.app.runtime.capability_js_host.default,
        CapabilityJsHost::Node
    );
    assert_eq!(
        config.app.runtime.capability_js_host.hosts,
        vec![
            CapabilityJsHost::Node,
            CapabilityJsHost::Deno,
            CapabilityJsHost::Bun,
        ]
    );
    assert!(config.app.runtime.capability_js_host.auto_detect);
    // The defaults list what uf can produce, not what it would like to. This
    // asserted that `edge` and `serverless` were in the list, which they were
    // and which nothing read: `DeployAdapter` had no consumer outside this
    // crate at all. Asserting the honest list is what makes the next adapter's
    // author change this line rather than inherit a claim.
    assert_eq!(
        config.app.runtime.deploy.adapters,
        vec![
            DeployAdapter::Node,
            DeployAdapter::Edge,
            DeployAdapter::Serverless,
            DeployAdapter::Static,
            DeployAdapter::Container,
        ]
    );
    assert_eq!(config.app.runtime.deploy.adapter, None);
    assert!(DeployAdapter::Node.is_implemented());
    assert!(DeployAdapter::Edge.is_implemented());
    assert!(DeployAdapter::Serverless.is_implemented());
    assert!(DeployAdapter::Container.is_implemented());
    // `static` joined them by growing the only thing it was ever going to be:
    // the refusal. It links nothing, and a project that needs a server is
    // named and rejected rather than half-published.
    assert!(DeployAdapter::Static.is_implemented());
    assert_eq!(DeployAdapter::Static.tracking_issue(), None);
    // And the two that are not, each of which is waiting for something named
    // rather than for somebody's attention.
    for adapter in [DeployAdapter::Bun, DeployAdapter::Deno] {
        assert!(!adapter.is_implemented(), "{}", adapter.as_str());
        assert_eq!(adapter.tracking_issue(), Some(391));
        assert!(
            adapter.unimplemented_because().is_some(),
            "{}",
            adapter.as_str()
        );
    }
    assert_eq!(DeployAdapter::Node.unimplemented_because(), None);
    assert!(!config.app.rendering.cache.fetch);
    assert!(!config.app.rendering.cache.route);
    assert!(config.app.rendering.modes.contains(&RenderingMode::Ppr));
    assert!(config.app.rendering.modes.contains(&RenderingMode::Isr));
    assert!(config.app.builtins.cell);
    assert!(config.app.builtins.react_testing_library);
    assert!(config.app.builtins.relay);
    assert!(config.app.orm.native);
    assert!(config.app.orm.generated_flow_types);
    assert!(config.app.orm.prepared_by_default);
    assert_eq!(config.app.builtins.style, StyleEngine::StyleX);
    assert_eq!(config.fmt.flow.parser, FlowFormatParser::OfficialFlowRust);
    assert_eq!(config.fmt.flow.printer, FlowFormatPrinter::UfRust);
    assert_eq!(config.fmt.non_flow.formatter, NonFlowFormatter::Biome);
    assert!(
        !config.fmt.non_flow.chosen_by_project,
        "a default is uf's suggestion, not the project's requirement"
    );
    assert_eq!(config.fmt.quotes, QuoteStyle::Double);
    assert!(config.fmt.semicolons);
    assert_eq!(config.server.engine, ServerEngine::NativeRust);
    assert!(config.server.native.streaming);
    assert!(config.server.native.zero_copy_http);
    assert!(
        config
            .server
            .native
            .adapters
            .contains(&NativeServerAdapter::Deno)
    );
    assert_eq!(config.package.generator, PackageGenerator::NapiRs);
    assert!(config.package.typescript_declarations_to_flow);
    assert!(config.package.targets.contains(&PackageTarget::NodeNapi));
    assert_eq!(config.pm.module, "@uniflowed/pm");
    assert_eq!(config.pm.resolver, PackageManagerResolver::UfNative);
    assert_eq!(config.pm.lockfile, "uf.lock");
    assert_eq!(config.pm.store_dir, ".uf/store");
    assert!(!config.pm.allow_lifecycle_scripts);
    assert_eq!(config.pm.package_manager, PackageManagerPreference::Auto);
    assert_eq!(config.rm.module, "@uniflowed/rm");
    assert!(config.rm.infer_from_config);
    assert_eq!(config.rm.version, "node@system");
    assert!(config.rm.auto_switch);
    assert_eq!(config.rm.acquisition, RuntimeManagerAcquisition::Auto);
    assert_eq!(config.rm.apply, RuntimeManagerApply::ConfigAndHost);
    assert!(config.rm.doctor);
    assert_eq!(config.std.module, "@uniflowed/std");
    assert!(config.std.wintertc_aligned);
    assert!(config.std.native_bindings);
    assert!(config.std.modules.contains(&StdModuleConfig::Vfs));
    assert!(config.std.modules.contains(&StdModuleConfig::Crypto));
    assert!(config.std.modules.contains(&StdModuleConfig::Os));
    assert!(config.std.modules.contains(&StdModuleConfig::Net));
    assert!(config.std.modules.contains(&StdModuleConfig::Dns));
    assert!(config.std.modules.contains(&StdModuleConfig::Path));
    assert!(config.std.modules.contains(&StdModuleConfig::Stream));
    assert!(config.std.modules.contains(&StdModuleConfig::Url));
    assert!(config.std.modules.contains(&StdModuleConfig::Wasm));
    assert!(config.std.modules.contains(&StdModuleConfig::Glob));
    assert!(config.std.modules.contains(&StdModuleConfig::Motion));
    assert!(config.std.modules.contains(&StdModuleConfig::Tui));
    assert!(config.std.modules.contains(&StdModuleConfig::Cron));
    assert!(config.std.modules.contains(&StdModuleConfig::S3));
    assert!(config.std.modules.contains(&StdModuleConfig::Sigv4));
    assert!(config.std.modules.contains(&StdModuleConfig::Functions));
    assert!(config.std.modules.contains(&StdModuleConfig::ImportMeta));
    assert!(config.std.modules.contains(&StdModuleConfig::Defer));
    assert_eq!(config.publish.first_publish.mode, FirstPublishMode::Local);
    assert!(config.publish.first_publish.local_bootstrap);
    assert!(config.publish.trusted_publish.enabled);
    assert_eq!(
        config.publish.trusted_publish.provider,
        TrustedPublishProvider::GitHubActionsOidc
    );
    assert!(config.publish.trusted_publish.tokenless);
    assert_eq!(
        config.publish.trusted_publish.trigger,
        TrustedPublishTrigger::TagPush
    );
    assert_eq!(config.release.tag_prefix, "uf@");
    assert_eq!(config.release.command, "uf release alpha");
    assert!(config.release.publish);
    assert_eq!(config.task_runner.engine, TaskRunnerEngine::ViteTask);
    assert!(!config.task_runner.allow_package_scripts);
    assert_eq!(config.test.module, "@uniflowed/test");
    assert_eq!(
        config.test.runner.runtime,
        NativeTestRuntimeConfig::CapabilityJsHost
    );
    assert_eq!(
        config.test.runner.performance_target,
        NativeTestPerformanceTarget::FasterThanBun
    );
    assert!(config.test.runner.official_flow_parser);
    assert_eq!(
        config.test.runner.js_hosts,
        vec![
            CapabilityJsHost::Node,
            CapabilityJsHost::Deno,
            CapabilityJsHost::Bun,
        ]
    );
    assert!(config.test.react_testing_library_native);
    assert_eq!(config.app.builtins.data, DataEngine::UniflowedQuery);
    assert_eq!(config.app.builtins.effect, EffectEngine::UniflowedEffect);
    assert_eq!(config.app.builtins.fetch.module, "@uniflowed/fetch");
    assert!(!config.app.builtins.fetch.override_global_fetch);
    assert_eq!(config.app.builtins.graphql.module, "@uniflowed/graphql");
    assert!(config.app.builtins.graphql.relay_base);
    assert_eq!(config.app.builtins.loader.module, "@uniflowed/loader");
    assert_eq!(config.app.builtins.loader.state_module, "@uniflowed/state");
    assert_eq!(config.app.builtins.loader.cache, CacheModeConfig::OptIn);
    assert_eq!(config.app.builtins.web.module, "@uniflowed/web");
    assert!(config.app.builtins.web.typed_routes);
    assert_eq!(config.app.builtins.web.cache, CacheModeConfig::OptIn);
    assert_eq!(
        config.app.builtins.markdown.engine,
        MarkdownEngineConfig::OxContentWasm
    );
    assert!(config.app.builtins.markdown.mdx.enabled);
    assert_eq!(
        config.app.builtins.markdown.mdx.extensions,
        vec![CompactString::const_new(".mdx")]
    );
    assert_eq!(
        config.app.builtins.markdown.mdx.jsx_import_source,
        "@uniflowed/jsx-runtime"
    );
    assert_eq!(
        config.app.builtins.markdown.mdx.pipeline_plugin,
        MdxPipelinePluginConfig::BuiltIn
    );
    assert_eq!(config.app.builtins.markdown.cache, CacheModeConfig::OptIn);
    assert_eq!(config.app.builtins.motion.module, "@uniflowed/motion");
    assert_eq!(
        config.app.builtins.motion.engine,
        MotionEngineConfig::UfNative
    );
    assert!(config.app.builtins.motion.compiler_safe);
    assert!(config.app.builtins.motion.server_component_safe);
    assert!(config.app.builtins.motion.reduced_motion_default);
    assert_eq!(config.app.builtins.tui.module, "@uniflowed/tui");
    assert_eq!(config.app.builtins.tui.std_module, "@uniflowed/std/tui");
    assert_eq!(config.app.builtins.tui.standard, TuiStandardConfig::OpenTui);
    assert!(config.app.builtins.tui.native_renderer);
    assert!(config.app.builtins.tui.beat_react_ink);
    assert!(config.app.builtins.tui.rich_media);
    assert!(config.app.builtins.tui.in_memory_tests);
    assert_eq!(config.app.builtins.temporal.module, "@uniflowed/temporal");
    assert!(config.app.builtins.temporal.lite);
    assert_eq!(config.app.builtins.pwa.module, "@uniflowed/pwa");
    assert!(!config.app.builtins.pwa.enabled_by_default);
    assert_eq!(config.app.builtins.pwa.cache, CacheModeConfig::OptIn);
    assert!(config.story.enabled);
    assert_eq!(config.story.module, "@uniflowed/story");
    assert_eq!(config.story.mocks.module, "@uniflowed/mock");
    assert!(config.story.mocks.msw_compatible);
    assert_eq!(config.story.browser.module, "@uniflowed/browser");
    assert!(config.story.browser.playwright_compatible);
    assert!(config.vrt.enabled);
    assert_eq!(config.vrt.module, "@uniflowed/vrt");
    assert_eq!(config.vrt.baselines, "__uf_vrt__");
    assert_eq!(config.vrt.threshold, 0);
    assert_eq!(
        config.app.builtins.react_compiler.mode,
        ReactCompilerMode::Syntax
    );
    assert_eq!(
        config.app.builtins.react_compiler.implementation,
        ReactCompilerImplementation::OfficialRust
    );
    assert!(config.app.targets.contains(&RuntimeTarget::ReactNative));
    assert!(config.docs.enabled);
    assert!(config.docs.static_build);
    assert_eq!(config.docs.deploy, DeployTarget::Void);
    assert_eq!(config.lint.engine, LintEngine::Rust);
    assert_eq!(config.lint.flow.builtins, FlowBuiltinLintMode::Mixed);
    assert_eq!(config.lint.flow.parser, FlowLintParser::OfficialFlowRust);
    assert_eq!(config.lint.rules["flow/syntax"], RuleLevel::Error);
}

#[test]
fn parses_the_dev_server_access_control_surface() {
    let source = r#"
        export default defineConfig({
          dev: {
            port: 5173,
            fs: {
              allow: ["../shared"],
              deny: ["*.secret"],
            },
            allowedHosts: ["dev.internal"],
            allowedOrigins: ["http://dev.internal:5173"],
          },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.dev.fs.allow, vec!["../shared"]);
    assert_eq!(parsed.dev.fs.deny, vec!["*.secret"]);
    assert_eq!(parsed.dev.allowed_hosts, vec!["dev.internal"]);
    assert_eq!(parsed.dev.allowed_origins, vec!["http://dev.internal:5173"]);
}

#[test]
fn dev_server_access_control_defaults_to_nothing_extra() {
    // The built-in deny list is Vite's, not uf's: configuring
    // `dev.fs.deny` adds to it and cannot shrink it.
    let dev = DevConfig::default();
    assert_eq!(dev.host, "127.0.0.1");
    assert!(dev.fs.allow.is_empty());
    assert!(dev.fs.deny.is_empty());
    assert!(dev.allowed_hosts.is_empty());
    assert!(dev.allowed_origins.is_empty());
}

#[test]
fn extracts_vite_style_define_config_object() {
    let source = r#"
        // @flow
        import { defineConfig } from "@uniflowed/config";

        export default defineConfig({
          app: {
            builtins: {
              markdown: {
                mdx: {
                  enabled: true,
                  extensions: [".mdx"],
                  jsxImportSource: "@uniflowed/jsx-runtime",
                  pipelinePlugin: "built-in",
                },
              },
            },
          },
          dev: { port: 4111 },
          lint: {
            rules: {
              "uniflowed/no-tabs": "off",
              "react/component-syntax": "error",
            },
          },
          taskRunner: {
            engine: "vite-task",
            allowPackageScripts: false,
          },
          test: {
            runner: {
              runtime: "capability-js-host",
              performanceTarget: "faster-than-bun",
              jsHosts: ["node", "deno", "bun"],
            },
          },
          rm: {
            inferFromConfig: true,
          },
          pm: {
            allowLifecycleScripts: false,
            packageManager: "pnpm",
          },
          tasks: {
            storybook: {
              command: "vite --host 0.0.0.0",
            },
          },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.dev.port, 4111);
    assert!(parsed.app.builtins.markdown.mdx.enabled);
    assert_eq!(
        parsed.app.builtins.markdown.mdx.extensions,
        vec![CompactString::const_new(".mdx")]
    );
    assert_eq!(
        parsed.app.builtins.markdown.mdx.jsx_import_source,
        "@uniflowed/jsx-runtime"
    );
    assert_eq!(
        parsed.app.builtins.markdown.mdx.pipeline_plugin,
        MdxPipelinePluginConfig::BuiltIn
    );
    assert_eq!(parsed.lint.rules["uniflowed/no-tabs"], RuleLevel::Off);
    assert_eq!(
        parsed.lint.rules["react/component-syntax"],
        RuleLevel::Error
    );
    assert_eq!(parsed.tasks["storybook"].command(), "vite --host 0.0.0.0");
    assert_eq!(parsed.task_runner.engine, TaskRunnerEngine::ViteTask);
    assert!(!parsed.task_runner.allow_package_scripts);
    assert_eq!(
        parsed.test.runner.performance_target,
        NativeTestPerformanceTarget::FasterThanBun
    );
    assert_eq!(
        parsed.test.runner.js_hosts,
        vec![
            CapabilityJsHost::Node,
            CapabilityJsHost::Deno,
            CapabilityJsHost::Bun,
        ]
    );
    assert!(parsed.rm.infer_from_config);
    assert!(!parsed.pm.allow_lifecycle_scripts);
    assert_eq!(parsed.pm.package_manager, PackageManagerPreference::Pnpm);
}

#[test]
fn parses_every_package_manager_preference() {
    for (value, expected) in [
        ("auto", PackageManagerPreference::Auto),
        ("uf", PackageManagerPreference::Uf),
        ("npm", PackageManagerPreference::Npm),
        ("pnpm", PackageManagerPreference::Pnpm),
        ("yarn", PackageManagerPreference::Yarn),
        ("yarn-classic", PackageManagerPreference::YarnClassic),
        ("yarn-berry", PackageManagerPreference::YarnBerry),
        ("bun", PackageManagerPreference::Bun),
    ] {
        let source =
            format!(r#"export default defineConfig({{ pm: {{ packageManager: "{value}" }} }});"#);
        let object = extract_config_object(&source).expect("object");
        let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

        assert_eq!(parsed.pm.package_manager, expected, "{value}");
    }
}

#[test]
fn rejects_an_unknown_package_manager_preference() {
    let source = r#"export default defineConfig({ pm: { packageManager: "deno" } });"#;
    let object = extract_config_object(source).expect("object");

    assert!(json5::from_str::<UniflowedConfig>(&object).is_err());
}

#[test]
fn extracts_plain_export_default_object_with_satisfies_tail() {
    let source = r#"
        export default {
          fmt: { lineWidth: 88 },
        } satisfies UniflowedConfig;
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.fmt.line_width, 88);
}

#[test]
fn parses_flow_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("uf.config.js")).unwrap();
    fs::write(
        &path,
        r#"
            export default defineConfig({
              dev: { port: 3000 },
              app: { builtins: { cell: false } },
              std: { modules: ["tui"] },
            });
        "#,
    )
    .unwrap();

    let config = load_config_file(&path).unwrap();

    assert_eq!(config.dev.port, 3000);
    assert!(!config.app.builtins.cell);
    assert!(config.std.modules.contains(&StdModuleConfig::Tui));
    assert!(config.app.builtins.native_test_runner);
}

/// The two cache switches uf implements are read and carried.
///
/// They reach `@uniflowed/vite`'s generated server entry and `uf preview`, and
/// from there `createFetchHandler`'s `cache` option. Before ubugeeei-prod/uf#277
/// they reached `dist/uf-build-manifest.json` and nothing else, which is
/// indistinguishable from this assertion passing over a switch nobody reads —
/// so the assertion that matters is in `tests/library/cache.test.js`, and this
/// one only says the value survives the parse.
#[test]
fn reads_the_two_cache_switches_that_are_implemented() {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("uf.config.js")).unwrap();
    fs::write(
        &path,
        r#"
            export default defineConfig({
              app: { rendering: { cache: { route: true, fetch: true } } },
            });
        "#,
    )
    .unwrap();

    let config = load_config_file(&path).unwrap();

    assert!(config.app.rendering.cache.route);
    assert!(config.app.rendering.cache.fetch);
    assert!(!config.app.rendering.cache.data);
    assert!(!config.app.rendering.cache.actions);
}

/// A cache uf does not have is refused where it is asked for.
///
/// Not ignored and not warned about. `rendering.cache.data: true` used to load
/// cleanly, reach the build manifest and change nothing anywhere — the exact
/// shape ubugeeei-prod/uf#277 objects to. A project that asks for a data cache
/// has to be told there is not one, and the config file is the only place where
/// telling them costs nothing.
#[test]
fn refuses_a_cache_switch_uf_does_not_implement() {
    for key in ["data", "actions"] {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("uf.config.js")).unwrap();
        fs::write(
            &path,
            format!(
                "export default defineConfig({{ app: {{ rendering: {{ cache: {{ {key}: true }} }} }} }});"
            ),
        )
        .unwrap();

        let error = load_config_file(&path).expect_err("a cache uf does not have is refused");

        assert!(
            matches!(&error, ConfigError::UnimplementedCache { key: named, .. } if *named == key),
            "{key}: {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains(&format!("rendering.cache.{key}")),
            "{message}"
        );
        assert!(message.contains("277"), "{message}");
    }
}

/// `false` is the default and says the same thing with or without a cache.
#[test]
fn a_cache_switch_that_is_off_is_never_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("uf.config.js")).unwrap();
    fs::write(
        &path,
        r#"
            export default defineConfig({
              app: { rendering: { cache: { data: false, actions: false } } },
            });
        "#,
    )
    .unwrap();

    let config = load_config_file(&path).unwrap();

    assert!(!config.app.rendering.cache.data);
}

#[test]
fn discovers_config_from_child_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("uf.config.js"),
        "export default defineConfig({});",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/app")).unwrap();

    let resolved = load_config(root.join("src/app")).unwrap();

    assert_eq!(resolved.root, root);
    assert_eq!(
        resolved.config_path.unwrap().file_name(),
        Some("uf.config.js")
    );
}

#[test]
fn budgets_are_unset_by_default() {
    assert!(UniflowedConfig::default().build.budgets.is_empty());
}

#[test]
fn reads_human_readable_budgets_from_the_config_object() {
    let source = r#"
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  build: {
budgets: {
  total: { max: "1.5 MB" },
  initialJs: { max: "180kb" },
  perAsset: { max: 250000, metric: "brotli" },
},
  },
});
"#;
    let object = extract_config_object(source).expect("config object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let budgets = parsed.build.budgets;
    assert_eq!(budgets.total.expect("total").max.bytes(), 1_500_000);
    assert_eq!(budgets.initial_js.expect("initialJs").max.bytes(), 180_000);
    let per_asset = budgets.per_asset.expect("perAsset");
    assert_eq!(per_asset.max.bytes(), 250_000);
    assert_eq!(per_asset.metric, BudgetMetric::Brotli);
    assert_eq!(
        budgets.initial_js.expect("initialJs").metric,
        BudgetMetric::Gzip,
        "gzip is the default metric"
    );
    assert!(budgets.per_route.is_none());
}

#[test]
fn rejects_a_budget_with_an_unparseable_size() {
    let source = r#"
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  build: { budgets: { total: { max: "10 terabytes" } } },
});
"#;
    let object = extract_config_object(source).expect("config object");

    assert!(json5::from_str::<UniflowedConfig>(&object).is_err());
}

#[test]
fn fmt_config_reads_max_blank_lines_from_the_config_file() {
    let source = r#"
        export default {
          fmt: { maxBlankLines: 0, indentWidth: 4 },
        };
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.fmt.max_blank_lines, 0);
    assert_eq!(parsed.fmt.indent_width, 4);
    assert_eq!(parsed.fmt.line_width, 100);
}

#[test]
fn parses_runtime_agnostic_tooling_surface() {
    let source = r#"
        export default defineConfig({
          app: {
            runtime: {
              default: "deno",
              capabilityJsHost: {
                default: "deno",
                hosts: ["node", "deno", "bun"],
                autoDetect: true,
              },
            },
          },
          fmt: {
            flow: {
              parser: "official-flow-rust",
              printer: "uf-rust",
            },
            nonFlow: {
              formatter: "biome",
            },
          },
          lint: {
            engine: "rust",
            flow: {
              builtins: "mixed",
              parser: "official-flow-rust",
            },
          },
          test: {
            runner: {
              runtime: "capability-js-host",
              jsHosts: ["node", "deno", "bun"],
            },
          },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.app.runtime.default, RuntimeEngine::Deno);
    assert_eq!(
        parsed.app.runtime.capability_js_host.default,
        CapabilityJsHost::Deno
    );
    assert_eq!(
        parsed.app.runtime.capability_js_host.hosts,
        vec![
            CapabilityJsHost::Node,
            CapabilityJsHost::Deno,
            CapabilityJsHost::Bun,
        ]
    );
    assert_eq!(parsed.fmt.flow.parser, FlowFormatParser::OfficialFlowRust);
    assert_eq!(parsed.fmt.flow.printer, FlowFormatPrinter::UfRust);
    assert_eq!(parsed.fmt.non_flow.formatter, NonFlowFormatter::Biome);
    assert!(
        parsed.fmt.non_flow.chosen_by_project,
        "a project that wrote the formatter down asked for it"
    );
    assert_eq!(parsed.lint.engine, LintEngine::Rust);
    assert_eq!(parsed.lint.flow.builtins, FlowBuiltinLintMode::Mixed);
    assert_eq!(parsed.lint.flow.parser, FlowLintParser::OfficialFlowRust);
    assert_eq!(
        parsed.test.runner.runtime,
        NativeTestRuntimeConfig::CapabilityJsHost
    );
}

/// The same formatter, from two different places, is two different situations.
///
/// `biome` written down by a project is a requirement it stated; `biome`
/// because uf's default is `biome` is uf's suggestion. Serde's `default` fills
/// the field in either way, so the difference has to be kept as it is read or
/// it cannot be recovered afterwards — and it decides whether a missing binary
/// fails `uf fmt`. See ubugeeei-prod/uf#441.
#[test]
fn a_formatter_the_project_named_is_not_the_same_as_uf_s_default() {
    let named = json5::from_str::<UniflowedConfig>(
        &extract_config_object(
            "export default defineConfig({ fmt: { nonFlow: { formatter: \"biome\" } } });",
        )
        .expect("object"),
    )
    .expect("config");
    let silent = json5::from_str::<UniflowedConfig>(
        &extract_config_object("export default defineConfig({ fmt: { indentWidth: 2 } });")
            .expect("object"),
    )
    .expect("config");
    let empty = json5::from_str::<UniflowedConfig>(
        &extract_config_object("export default defineConfig({ fmt: { nonFlow: {} } });")
            .expect("object"),
    )
    .expect("config");

    assert_eq!(named.fmt.non_flow.formatter, NonFlowFormatter::Biome);
    assert_eq!(silent.fmt.non_flow.formatter, NonFlowFormatter::Biome);
    assert_eq!(empty.fmt.non_flow.formatter, NonFlowFormatter::Biome);
    assert!(named.fmt.non_flow.chosen_by_project);
    assert!(!silent.fmt.non_flow.chosen_by_project);
    assert!(
        !empty.fmt.non_flow.chosen_by_project,
        "an empty `nonFlow` block names nothing"
    );
}

/// Where the choice came from is uf's own bookkeeping, and `uf inspect --json`
/// answers "which formatter will run" with one field, as it always has.
#[test]
fn where_the_choice_came_from_is_not_part_of_the_configuration_it_describes() {
    let mut config = FmtConfig::default();
    config.non_flow.chosen_by_project = true;
    config.non_flow.arguments = vec!["--css-parse-tailwind-directives=true".into()];

    let json = serde_json::to_value(&config.non_flow).expect("serializes");

    // Named rather than compared against the whole object: `chosenByProject`
    // being absent is the fact this test is about, and asserting the exact
    // shape made it fail the day the configuration grew a key it *should*
    // carry.
    assert_eq!(json.get("chosenByProject"), None, "{json}");
    assert_eq!(json["formatter"], "biome");
    assert_eq!(
        json["arguments"],
        serde_json::json!(["--css-parse-tailwind-directives=true"]),
        "a setting the project wrote has to survive the round trip"
    );
}

/// ubugeeei-prod/uf#475: naming one rule switched the other fifty-three off.
///
/// Silently, and the ones that vanished included `flow/syntax` — so a project
/// that lowered `unclear-type` to a warning also stopped being told its sources
/// do not parse.
#[test]
fn naming_one_rule_leaves_the_others_where_they_were() {
    let config: UniflowedConfig = serde_json::from_value(serde_json::json!({
        "lint": { "rules": { "flow/unclear-type": "warn" } }
    }))
    .expect("a config naming one rule");

    let defaults = UniflowedConfig::default();
    assert_eq!(
        config.lint.rules.len(),
        defaults.lint.rules.len(),
        "the table shrank"
    );
    assert_eq!(
        config.lint.rules.get("flow/unclear-type"),
        Some(&RuleLevel::Warn),
        "the one rule that was named did not change"
    );
    for (rule, level) in &defaults.lint.rules {
        if rule == "flow/unclear-type" {
            continue;
        }
        assert_eq!(config.lint.rules.get(rule), Some(level), "{rule} moved");
    }
}

/// Switching a rule off is what `"off"` is for, and it still works.
#[test]
fn a_rule_is_switched_off_by_saying_so() {
    let config: UniflowedConfig = serde_json::from_value(serde_json::json!({
        "lint": { "rules": { "flow/unclear-type": "off" } }
    }))
    .expect("a config switching one rule off");

    assert_eq!(
        config.lint.rules.get("flow/unclear-type"),
        Some(&RuleLevel::Off)
    );
    assert!(config.lint.rules.len() > 50, "the table shrank");
}

/// The permission set a project declares, read as written.
#[test]
fn parses_a_permission_set() {
    let source = r#"
        export default defineConfig({
          permissions: {
            read: ["./fixtures"],
            write: ["./.uf"],
            net: ["registry.npmjs.org:443"],
            env: ["CI"],
            run: ["uf"],
          },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let permissions = parsed.permissions.expect("declared");
    assert_eq!(permissions.read, vec!["./fixtures"]);
    assert_eq!(permissions.write, vec!["./.uf"]);
    assert_eq!(permissions.net, vec!["registry.npmjs.org:443"]);
    assert_eq!(permissions.env, vec!["CI"]);
    assert_eq!(permissions.run, vec!["uf"]);
}

/// No block and an empty block are opposite instructions.
///
/// A project with no `permissions` key runs the way uf has always run. A
/// project that writes `permissions: {}` has said "nothing beyond what uf
/// itself needs", which is a sandbox and has to survive as one — an
/// `is_empty()` test on a defaulted struct would have read the second as the
/// first and quietly run it unsandboxed.
#[test]
fn an_absent_permission_block_and_an_empty_one_are_different_answers() {
    let absent: UniflowedConfig =
        json5::from_str(&extract_config_object("export default defineConfig({});").unwrap())
            .expect("config");
    assert_eq!(absent.permissions, None);

    let empty: UniflowedConfig = json5::from_str(
        &extract_config_object("export default defineConfig({ permissions: {} });").unwrap(),
    )
    .expect("config");
    assert_eq!(empty.permissions, Some(Permissions::default()));
}

/// A typo in this block is a security bug, so it is an error.
///
/// Every other section of `uf.config.js` ignores a key it does not know, which
/// is the right trade where an unknown key means an option from a newer uf.
/// Here it means a permission nobody granted and nobody was told about:
/// `permissions: { nett: [...] }` would parse to a set with no network at all,
/// and the project would run believing it had declared one.
#[test]
fn refuses_a_misspelled_permission_rather_than_granting_nothing_quietly() {
    let object =
        extract_config_object("export default defineConfig({ permissions: { nett: [\"a\"] } });")
            .expect("object");
    let error = json5::from_str::<UniflowedConfig>(&object).expect_err("refused");
    assert!(error.to_string().contains("nett"), "{error}");
}

#[test]
fn the_registry_uf_reads_from_defaults_to_the_one_it_publishes_to() {
    // ubugeeei-prod/uf#540: a project that has only ever set `publish.registry`
    // keeps resolving against it, and is told which key to move to.
    let source =
        r#"export default defineConfig({ publish: { registry: "https://npm.company.example" } });"#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let read = parsed.read_registry();
    assert_eq!(read.url, "https://npm.company.example");
    assert_eq!(read.source, RegistrySource::PublishFallback);
    assert!(read.source.is_deprecated());
    assert!(
        read.source
            .deprecation()
            .is_some_and(|line| line.contains("pm.registry"))
    );
}

#[test]
fn pm_registry_is_the_one_uf_resolves_against_and_publish_registry_stays_publish() {
    let source = r#"
        export default defineConfig({
          pm: { registry: "https://mirror.company.example" },
          publish: { registry: "https://npm.company.example" },
        });
    "#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let read = parsed.read_registry();
    assert_eq!(read.url, "https://mirror.company.example");
    assert_eq!(read.source, RegistrySource::Pm);
    assert!(!read.source.is_deprecated());
    assert_eq!(read.source.deprecation(), None);
    // The publish target is untouched: the two settings answer two questions.
    assert_eq!(parsed.publish.registry, "https://npm.company.example");
}

#[test]
fn a_project_that_sets_neither_registry_is_not_warned_about_a_key_it_never_wrote() {
    let config = UniflowedConfig::default();
    let read = config.read_registry();

    assert_eq!(read.url, DEFAULT_REGISTRY);
    assert_eq!(read.source, RegistrySource::Default);
    assert!(!read.source.is_deprecated());
}

#[test]
fn a_scope_can_be_bound_to_a_registry_and_provenance_can_be_turned_off() {
    let source = r#"
        export default defineConfig({
          pm: {
            registry: "https://mirror.company.example",
            scopes: { "@company": "https://npm.company.example" },
            provenance: "off",
          },
        });
    "#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(
        parsed.scope_registries().collect::<Vec<_>>(),
        vec![("@company", "https://npm.company.example")]
    );
    assert_eq!(parsed.pm.provenance, ProvenanceMode::Off);
    assert!(!parsed.pm.provenance.reads_attestations());
    // The default is the other way round: attestations are read.
    assert!(
        UniflowedConfig::default()
            .pm
            .provenance
            .reads_attestations()
    );
}

/// Naming one rule changes that rule, not the size of the linter.
///
/// The regression for ubugeeei-prod/uf#475. `lint.rules` used to *be* the rule
/// table, so the config below described a one-rule linter: `flow/syntax`,
/// `react/hooks-rules` and fifty others were not lowered, they were gone, and
/// the run that no longer looked for them passed.
#[test]
fn naming_one_lint_rule_keeps_the_rest_of_uf_s_table() {
    let source = r#"
        import { defineConfig } from "@uniflowed/config";

        export default defineConfig({
          lint: { rules: { "flow/unclear-type": "warn" } },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let defaults = UniflowedConfig::default();
    assert_eq!(parsed.lint.rules.len(), defaults.lint.rules.len());
    assert_eq!(parsed.lint.rules["flow/unclear-type"], RuleLevel::Warn);
    // The three named in the issue, and the one whose loss is worst: a project
    // that lowers a rule to a warning must not stop being told its sources do
    // not parse.
    for kept in [
        "flow/syntax",
        "react/hooks-rules",
        "flow/nested-component",
        "flow/mixed-import-and-require",
    ] {
        assert_eq!(
            parsed.lint.rules.get(kept).copied(),
            defaults.lint.rules.get(kept).copied(),
            "{kept} lost its default level"
        );
    }
}

/// Switching a rule off is still spelled `"off"`, and still works.
#[test]
fn a_rule_set_to_off_is_off_and_its_neighbours_are_not() {
    let source = r#"
        export default {
          lint: { rules: { "uniflowed/no-tabs": "off" } },
        };
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(parsed.lint.rules["uniflowed/no-tabs"], RuleLevel::Off);
    assert_eq!(
        parsed.lint.rules["uniflowed/no-trailing-whitespace"],
        RuleLevel::Error
    );
}

/// A rule id this uf does not know is carried rather than refused.
///
/// `uf_lint` owns the catalogue and reports an unknown id where it can say
/// something useful about it. Refusing the config here would make a uf
/// downgrade — or a config written for the next release — unrunnable.
#[test]
fn an_unknown_rule_id_is_kept_rather_than_rejected() {
    let source = r#"
        export default {
          lint: { rules: { "future/rule-from-a-newer-uf": "error" } },
        };
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    assert_eq!(
        parsed.lint.rules["future/rule-from-a-newer-uf"],
        RuleLevel::Error
    );
    assert_eq!(
        parsed.lint.rules.len(),
        UniflowedConfig::default().lint.rules.len() + 1
    );
}

/// Serializing a config and reading it back is the identity.
///
/// Merging makes this true rather than accidental: the serialized map names
/// every rule, so laying it over the defaults reproduces it exactly. Anything
/// that round-trips a config through JSON — `uf inspect --json`, a plugin host
/// — depends on it.
#[test]
fn a_serialized_config_reads_back_with_the_same_rule_table() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Off,
    );

    let json = serde_json::to_string(&config).expect("serialize");
    let parsed: UniflowedConfig = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.lint.rules, config.lint.rules);
}

/// A project that says nothing about accessibility gets the dev audit and no
/// narrowing, and the workers are told nothing at all.
#[test]
fn accessibility_defaults_to_every_rule_and_an_audit_in_dev() {
    let config = UniflowedConfig::default();

    assert!(config.accessibility.dev_audit);
    assert!(config.accessibility.axe.tags.is_empty());
    assert!(config.accessibility.axe.disabled_rules.is_empty());
    assert_eq!(config.accessibility.axe.min_impact, None);
    // `None` rather than `"{}"`: the variable is absent, which the matcher
    // reads as "run every rule". A present-but-empty value would be a project
    // that had said something, and it has not.
    assert_eq!(config.accessibility.axe.as_json(), None);
}

/// What a project *does* say reaches the workers in axe's own vocabulary.
#[test]
fn an_axe_rule_set_travels_as_the_json_the_matcher_reads() {
    let axe = AxeConfig {
        tags: vec![CompactString::const_new("wcag2aa")],
        disabled_rules: vec![CompactString::const_new("color-contrast")],
        min_impact: Some(AxeImpact::Serious),
        ..AxeConfig::default()
    };

    let json = axe.as_json().expect("a configured rule set is carried");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["tags"][0], "wcag2aa");
    assert_eq!(read["disabledRules"][0], "color-contrast");
    // kebab-case, because that is what axe calls its own impacts and the JSON
    // is read by `packages/test/internal/axe.js` rather than by Rust.
    assert_eq!(read["minImpact"], "serious");
}

/// One narrowing is enough to be worth telling a worker about.
#[test]
fn a_single_disabled_rule_is_still_something_to_say() {
    let axe = AxeConfig {
        disabled_rules: vec![CompactString::const_new("region")],
        ..AxeConfig::default()
    };
    assert!(axe.as_json().is_some());
}

/// The project-wide ignore list has a project-wide name.
///
/// ubugeeei-prod/uf#575: `lint.ignore` was read by `uf fmt`, `uf lint`,
/// `uf check`, `uf test` and `uf doc` — five commands, one of which the key was
/// named after. The list is `ignore`, at the top level, where nothing about the
/// name says which command it is for.
#[test]
fn the_ignore_list_is_read_from_the_key_that_is_not_named_after_one_command() {
    let source = r#"export default defineConfig({ ignore: ["vendor"] });"#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let ignore = parsed.project_ignore();
    assert_eq!(ignore.entries, ["vendor"]);
    assert_eq!(ignore.source, IgnoreSource::Project);
    assert!(!ignore.source.is_deprecated());
    assert_eq!(ignore.source.deprecation(), None);
}

/// A project that has not moved yet keeps the behaviour it had, and is told.
///
/// The same shape ubugeeei-prod/uf#540 used for `publish.registry`: the old key
/// still answers, and the reader is told once which key it should be writing.
/// There is no release in which a config that says `lint.ignore` starts walking
/// the directory it excluded.
#[test]
fn lint_ignore_still_answers_and_says_which_key_it_is() {
    let source = r#"export default defineConfig({ lint: { ignore: ["vendor"] } });"#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let ignore = parsed.project_ignore();
    assert_eq!(ignore.entries, ["vendor"]);
    assert_eq!(ignore.source, IgnoreSource::LintFallback);
    assert!(ignore.source.is_deprecated());
    let deprecation = ignore.source.deprecation().expect("a sentence to print");
    // Both keys, because "deprecated" without the replacement costs a search,
    // and the five commands, because that is the fact the old name hid.
    assert!(deprecation.contains("lint.ignore"), "{deprecation}");
    assert!(deprecation.contains("ignore"), "{deprecation}");
    assert!(deprecation.contains("uf fmt"), "{deprecation}");
}

/// The new key wins outright, rather than being merged with the old one.
///
/// A union of two lists is a rule nobody can predict from either file, and a
/// project migrating wants to see exactly what it moved.
#[test]
fn the_new_ignore_key_replaces_the_old_one_rather_than_joining_it() {
    let source = r#"
        export default defineConfig({
          ignore: ["vendor"],
          lint: { ignore: ["legacy"] },
        });
    "#;
    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let ignore = parsed.project_ignore();
    assert_eq!(ignore.entries, ["vendor"]);
    assert_eq!(ignore.source, IgnoreSource::Project);
}

/// A project that wrote neither key is not warned about one it never wrote.
#[test]
fn a_project_that_names_no_ignore_list_gets_ufs_own_and_no_deprecation() {
    let config = UniflowedConfig::default();
    let ignore = config.project_ignore();

    assert_eq!(ignore.entries, DEFAULT_IGNORE);
    assert_eq!(ignore.source, IgnoreSource::Default);
    assert!(!ignore.source.is_deprecated());
}

/// An empty list is a project's instruction, not the absence of one.
///
/// `ignore: []` says "walk everything uf would otherwise skip", which is a
/// different thing from saying nothing — and the reason both keys are held in
/// an `Option` rather than defaulted to a list.
#[test]
fn an_empty_ignore_list_is_not_the_same_as_no_ignore_list() {
    let object = extract_config_object("export default defineConfig({ ignore: [] });").unwrap();
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

    let ignore = parsed.project_ignore();
    assert!(ignore.entries.is_empty());
    assert_eq!(ignore.source, IgnoreSource::Project);
}

/// `app.builtins.markdown.mdx.highlight` is read, which it was not.
///
/// `HighlightConfig` was declared, exported, and documented in the
/// configuration reference — and was a field of no struct. Nothing
/// deserialized those three keys and nothing read them, so a project that set
/// a theme got no error and no effect. `packages/vite` had been reading
/// `mdxConfig.highlight` the whole time and getting `undefined`. See
/// ubugeeei-prod/uf#646.
#[test]
fn parses_the_mdx_highlighting_surface() {
    let source = r#"
        // @flow
        import { defineConfig } from "@uniflowed/config";

        export default defineConfig({
          app: {
            builtins: {
              markdown: {
                mdx: {
                  highlight: {
                    enabled: true,
                    themes: { light: "solarized-light", dark: "nord" },
                    langs: ["nix", "toml"],
                  },
                },
              },
            },
          },
        });
    "#;

    let object = extract_config_object(source).expect("object");
    let parsed: UniflowedConfig = json5::from_str(&object).expect("config");
    let highlight = &parsed.app.builtins.markdown.mdx.highlight;

    assert!(highlight.enabled);
    assert_eq!(highlight.themes.light, "solarized-light");
    assert_eq!(highlight.themes.dark, "nord");
    assert_eq!(
        highlight.langs,
        vec![
            CompactString::const_new("nix"),
            CompactString::const_new("toml")
        ]
    );
}

/// And the defaults are the ones the reference documents, so a project that
/// configures nothing still gets colour.
#[test]
fn mdx_highlighting_defaults_to_both_github_themes() {
    let mdx = crate::MdxConfig::default();
    assert!(mdx.highlight.enabled);
    assert_eq!(mdx.highlight.themes.light, "github-light");
    assert_eq!(mdx.highlight.themes.dark, "github-dark-dimmed");
    assert!(mdx.highlight.langs.is_empty());
}

/// The key path the reference documents is the one that serializes.
///
/// The reference is read as a contract, and the failure this guards is the one
/// #646 was: a struct that exists, is exported, and is reachable from no key.
#[test]
fn the_documented_highlight_key_path_is_the_one_uf_serializes() {
    let config = UniflowedConfig::default();
    let json = serde_json::to_value(&config).expect("the config serializes");
    let highlight = json
        .pointer("/app/builtins/markdown/mdx/highlight")
        .expect("app.builtins.markdown.mdx.highlight is a key uf writes");

    for key in ["enabled", "themes", "langs"] {
        assert!(
            highlight.get(key).is_some(),
            "the reference documents {key} and the config does not carry it: {highlight}"
        );
    }
}
