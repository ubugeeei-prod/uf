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
    assert!(config.app.rsc);
    assert!(config.app.server_actions);
    assert_eq!(config.app.runtime.default, RuntimeEngine::Uf);
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
            .contains(&RuntimeEngine::Bun)
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
            .deploy
            .adapters
            .contains(&DeployAdapter::Edge)
    );
    assert!(
        config
            .app
            .runtime
            .deploy
            .adapters
            .contains(&DeployAdapter::Serverless)
    );
    assert!(!config.app.rendering.cache.fetch);
    assert!(!config.app.rendering.cache.route);
    assert!(config.app.rendering.modes.contains(&RenderingMode::Ppr));
    assert!(config.app.rendering.modes.contains(&RenderingMode::Isr));
    assert!(config.app.builtins.flow_cell);
    assert!(config.app.builtins.react_testing_library);
    assert!(config.app.builtins.relay);
    assert!(config.app.orm.native);
    assert!(config.app.orm.generated_flow_types);
    assert!(config.app.orm.prepared_by_default);
    assert_eq!(config.app.builtins.style, StyleEngine::StyleX);
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
    assert_eq!(config.rm.version, "uf@0.1.0");
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
    assert_eq!(config.release.command, "uf release minor");
    assert!(config.release.publish);
    assert_eq!(config.task_runner.engine, TaskRunnerEngine::UfTask);
    assert!(!config.task_runner.allow_package_scripts);
    assert_eq!(config.test.module, "@uniflowed/test");
    assert_eq!(
        config.test.runner.runtime,
        NativeTestRuntimeConfig::UfSelfHosted
    );
    assert_eq!(
        config.test.runner.performance_target,
        NativeTestPerformanceTarget::FasterThanBun
    );
    assert!(config.test.runner.official_flow_parser);
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
    assert!(config.app.targets.contains(&RuntimeTarget::ReactNative));
    assert!(config.docs.enabled);
    assert!(config.docs.static_build);
    assert_eq!(config.docs.deploy, DeployTarget::Void);
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
    // The built-in deny list lives in `uf_devserver`, not here: configuring
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
          dev: { port: 4111 },
          lint: {
            rules: {
              "uniflowed/no-tabs": "off",
              "react/component-syntax": "error",
            },
          },
          taskRunner: {
            engine: "uf-task",
            allowPackageScripts: false,
          },
          test: {
            runner: {
              performanceTarget: "faster-than-bun",
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
    assert_eq!(parsed.lint.rules["uniflowed/no-tabs"], RuleLevel::Off);
    assert_eq!(
        parsed.lint.rules["react/component-syntax"],
        RuleLevel::Error
    );
    assert_eq!(parsed.tasks["storybook"].command(), "vite --host 0.0.0.0");
    assert_eq!(parsed.task_runner.engine, TaskRunnerEngine::UfTask);
    assert!(!parsed.task_runner.allow_package_scripts);
    assert_eq!(
        parsed.test.runner.performance_target,
        NativeTestPerformanceTarget::FasterThanBun
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
              app: { builtins: { flowCell: false } },
              std: { modules: ["tui"] },
            });
        "#,
    )
    .unwrap();

    let config = load_config_file(&path).unwrap();

    assert_eq!(config.dev.port, 3000);
    assert!(!config.app.builtins.flow_cell);
    assert!(config.std.modules.contains(&StdModuleConfig::Tui));
    assert!(config.app.builtins.native_test_runner);
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
