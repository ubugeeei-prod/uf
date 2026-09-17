use camino::Utf8Path;

use super::*;
use crate::{extract_config_object, parse_config_object, parse_config_projection};

const PATH: &str = "/project/uf.config.js";

/// `uf.config.js` holding `defineConfig(<body>)`, read the way every command
/// reads it.
fn load(body: &str) -> Result<UniflowedConfig, ConfigError> {
    let source = format!("export default defineConfig({body});");
    let object = extract_config_object(&source).expect("an object literal");
    parse_config_object(Utf8Path::new(PATH), &object)
}

fn loaded(body: &str) -> UniflowedConfig {
    load(body).unwrap_or_else(|error| panic!("{body} should load: {error}"))
}

fn refused(body: &str) -> ConfigError {
    match load(body) {
        Ok(_) => panic!("{body} should be refused"),
        Err(error) => error,
    }
}

fn runtime(written: &str) -> RuntimeSpec {
    RuntimeSpec::parse(written).unwrap_or_else(|error| panic!("{written}: {error}"))
}

/// The example ubugeeei-prod/uf#940 was written around, read as written.
#[test]
fn the_example_from_the_issue_reads_as_written() {
    let config = loaded(
        r#"{
          runtime: "node@26",
          packageManager: "pnpm@12.0.0",
          build: { runtime: "node@26", builder: "vite" },
          test: { runtime: "bun@1.4", runner: "bun@1.4" },
        }"#,
    );

    let tool = config.runtime_tool().expect("runtime");
    assert_eq!(tool.spec.name, CapabilityJsHost::Node);
    assert_eq!(tool.spec.version, ToolVersion::Prefix("26".into()));
    assert_eq!(tool.source, ToolSource::Key("runtime"));

    let manager = config.package_manager_tool().expect("packageManager");
    assert_eq!(manager.spec.name, PackageManagerName::Pnpm);
    assert_eq!(manager.spec.version, ToolVersion::Exact("12.0.0".into()));

    assert_eq!(
        config.build_runtime_tool().map(|tool| tool.source),
        Some(ToolSource::Key("build.runtime"))
    );
    assert_eq!(config.builder_tool().spec, BuilderSpec::Vite);
    assert_eq!(config.builder_tool().spec.module(), "@uniflowed/vite");

    let test = config.test_runtime_tool().expect("test runtime");
    assert_eq!(test.spec, runtime("bun@1.4"));
    assert_eq!(test.source, ToolSource::Key("test.runtime"));
    assert_eq!(
        config.test_runner_tool().spec,
        TestRunnerSpec::Bun(ToolVersion::Prefix("1.4".into()))
    );

    // And `uf inspect --json` prints what the project wrote, not a rendering
    // of what uf made of it.
    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(value["runtime"], "node@26");
    assert_eq!(value["packageManager"], "pnpm@12.0.0");
    assert_eq!(value["build"]["builder"], "vite");
    assert_eq!(value["test"]["runner"], "bun@1.4");
    assert!(config.tool_deprecations().is_empty());
}

/// A project that writes none of the new keys declares nothing, and is not
/// warned about anything.
#[test]
fn a_project_that_writes_none_of_this_declares_nothing() {
    let config = UniflowedConfig::default();

    assert_eq!(config.runtime_tool(), None);
    assert_eq!(config.build_runtime_tool(), None);
    assert_eq!(config.test_runtime_tool(), None);
    assert_eq!(config.package_manager_tool(), None);
    assert_eq!(
        config.test_runner_tool(),
        DeclaredTool {
            spec: TestRunnerSpec::Uf,
            source: ToolSource::Default
        }
    );
    assert_eq!(
        config.builder_tool(),
        DeclaredTool {
            spec: BuilderSpec::Vite,
            source: ToolSource::Default
        }
    );
    assert!(config.tool_deprecations().is_empty());

    // A written `null` is the key absent, not a spec.
    let config = loaded("{ runtime: null, test: { runner: null } }");
    assert_eq!(config.runtime_tool(), None);
    assert_eq!(config.test_runner_tool().source, ToolSource::Default);
}

/// The three things a version can be.
#[test]
fn a_version_is_nothing_a_prefix_or_an_exact_release() {
    assert_eq!(runtime("node").version, ToolVersion::OnPath);
    assert_eq!(runtime("node@26").version, ToolVersion::Prefix("26".into()));
    assert_eq!(
        runtime("bun@1.4").version,
        ToolVersion::Prefix("1.4".into())
    );
    assert_eq!(
        runtime("node@24.14.0").version,
        ToolVersion::Exact("24.14.0".into())
    );
    assert_eq!(
        runtime("bun@1.3.0-canary.2").version,
        ToolVersion::Exact("1.3.0-canary.2".into())
    );
    assert_eq!(
        PackageManagerSpec::parse("yarn@4.9.2+sha.20260910")
            .unwrap()
            .version,
        ToolVersion::Exact("4.9.2+sha.20260910".into())
    );
    // `0` is a major like any other.
    assert_eq!(runtime("deno@0").version, ToolVersion::Prefix("0".into()));

    // And a spec prints as it was written, which is what makes `Written`'s
    // text and its spec the same string.
    for written in [
        "node",
        "node@26",
        "bun@1.4",
        "deno@2.1.4",
        "bun@1.3.0-canary.2",
    ] {
        assert_eq!(runtime(written).to_string(), written);
    }
    assert_eq!(
        Written::from(runtime("node@26")).text(),
        "node@26",
        "a spec built in code has the text a project would have written"
    );
}

/// A range is not an environment, and the refusal says which prefix to write.
#[test]
fn a_range_is_refused_with_the_prefix_to_write_instead() {
    for range in [
        "^26",
        "~26.1",
        ">=26",
        "<27",
        "=26.1.0",
        "*",
        "26.x",
        "26.1.X",
        "25 || 26",
        "26.0.0 - 26.2.0",
        "26, 27",
    ] {
        let error = RuntimeSpec::parse(&format!("node@{range}")).unwrap_err();
        assert!(
            matches!(error, SpecError::Range { .. }),
            "{range}: {error:?}"
        );
    }

    let message = refused(r#"{ runtime: "node@^26" }"#).to_string();
    assert!(
        message.contains("runtime is `node@^26`, which is a range"),
        "{message}"
    );
    assert!(
        message.contains("a range is not an environment"),
        "{message}"
    );
    assert!(
        message.contains("`node@26` is the newest 26.x"),
        "{message}"
    );
    assert!(message.contains("uf.lock"), "{message}");

    // No number to suggest, so the shape of one.
    let message = refused(r#"{ runtime: "node@*" }"#).to_string();
    assert!(message.contains("`node@<major>`"), "{message}");
}

/// Something after the `@` that is not a version at all.
#[test]
fn a_tag_a_leading_v_and_an_empty_version_are_each_told_what_to_write() {
    for (written, expected) in [
        ("node@lts", "`lts` is not a version"),
        ("node@latest", "`latest` is not a version"),
        ("node@26.1.0.4", "`26.1.0.4` is not a version"),
        // A leading zero is not a numeric identifier, which is how semver
        // keeps `026` and `26` from being two spellings of one release.
        ("node@026", "`026` is not a version"),
        ("node@v26", "write `node@26`, without the `v`"),
        ("node@v24.14.0", "write `node@24.14.0`, without the `v`"),
        ("node@", "nothing follows the `@`"),
    ] {
        let message = refused(&format!("{{ runtime: {written:?} }}")).to_string();
        assert!(message.contains(expected), "{written}: {message}");
        assert!(
            message.contains(&format!("runtime is `{written}`")),
            "{message}"
        );
    }
}

/// Each key takes the names its role has, and says what they are.
#[test]
fn each_key_takes_the_names_of_its_own_role() {
    let message = refused(r#"{ runtime: "pnpm@9" }"#).to_string();
    assert!(message.contains("`pnpm` is not a runtime"), "{message}");
    assert!(message.contains("`node`, `bun` or `deno`"), "{message}");

    let message = refused(r#"{ packageManager: "deno" }"#).to_string();
    assert!(
        message.contains("`deno` is not a package manager"),
        "{message}"
    );
    assert!(
        message.contains("`npm`, `pnpm`, `yarn` or `bun`"),
        "{message}"
    );

    // Bun is both, and each key reads it as its own role.
    assert_eq!(
        loaded(r#"{ packageManager: "bun@1.2.0" }"#)
            .package_manager_tool()
            .unwrap()
            .spec
            .name,
        PackageManagerName::Bun
    );

    let message = refused(r#"{ test: { runner: "vitest" } }"#).to_string();
    assert!(
        message.contains("`vitest` is not a test runner"),
        "{message}"
    );
    assert!(message.contains("`uf` or `bun`"), "{message}");

    let message = refused(r#"{ test: { runner: "uf@1.0.0" } }"#).to_string();
    assert!(message.contains("no release of it to pin"), "{message}");

    let message = refused(r#"{ build: { builder: "" } }"#).to_string();
    assert!(
        message.contains("build.builder is ``, which names nothing"),
        "{message}"
    );

    let message = refused(r#"{ runtime: "@26" }"#).to_string();
    assert!(
        message.contains("names no tool before the `@`"),
        "{message}"
    );
}

/// The refusal names the key it was under, and it is not a parse failure.
///
/// Not being a parse failure is load-bearing rather than tidy: `uf dev` and
/// `uf build` answer `ConfigError::Parse` by starting a JavaScript host to
/// evaluate the file instead, so a typo in a version would have started a
/// process for nothing and then been reported as a config that could not be
/// evaluated.
#[test]
fn a_refused_spec_names_its_key_and_is_not_a_parse_failure() {
    for (body, key) in [
        (r#"{ runtime: "node@^26" }"#, "runtime"),
        (r#"{ packageManager: "pnpm@~9" }"#, "packageManager"),
        (r#"{ build: { runtime: "node@^26" } }"#, "build.runtime"),
        (r#"{ test: { runtime: "bun@>=1" } }"#, "test.runtime"),
        (r#"{ test: { runner: "bun@^1.4" } }"#, "test.runner"),
    ] {
        let error = refused(body);
        assert!(
            matches!(&error, ConfigError::ToolSpec { key: found, .. } if *found == key),
            "{body}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with(&format!("{PATH}: {key} is `")),
            "{error}"
        );
    }

    let message = refused("{ runtime: 26 }").to_string();
    assert!(
        message.contains("runtime is `26`, which is not a string"),
        "{message}"
    );
    assert!(message.contains("\"node@26\""), "{message}");
}

/// The config a builder evaluates is held to the same grammar as the one uf
/// reads without running it.
#[test]
fn the_evaluated_projection_is_held_to_the_same_grammar() {
    let path = Utf8Path::new(PATH);
    let error = parse_config_projection(
        path,
        serde_json::json!({ "build": { "runtime": "node@>=24" } }),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            ConfigError::ToolSpec {
                key: "build.runtime",
                ..
            }
        ),
        "{error:?}"
    );

    let config = parse_config_projection(
        path,
        serde_json::json!({ "runtime": "deno@2", "test": { "runner": "uf" } }),
    )
    .unwrap();
    assert_eq!(config.runtime_tool().unwrap().spec, runtime("deno@2"));
    assert_eq!(
        config.test_runner_tool().source,
        ToolSource::Key("test.runner")
    );
}

/// `uf dev`, `uf build` and `uf preview`: `build.runtime`, then `runtime`.
/// `uf start`, `uf run` and `uf exec`: `runtime` alone.
#[test]
fn the_build_runtime_is_build_runtime_then_runtime() {
    let config = loaded(r#"{ runtime: "node@26" }"#);
    let build = config.build_runtime_tool().unwrap();
    assert_eq!(build.spec, runtime("node@26"));
    assert_eq!(build.source, ToolSource::Key("runtime"));

    let config = loaded(r#"{ runtime: "node@26", build: { runtime: "node@24" } }"#);
    assert_eq!(
        config.build_runtime_tool().unwrap().source,
        ToolSource::Key("build.runtime")
    );
    assert_eq!(config.runtime_tool().unwrap().spec, runtime("node@26"));

    // The build's runtime is the build's, and `uf start` does not read it.
    let config = loaded(r#"{ build: { runtime: "node@24" } }"#);
    assert_eq!(config.runtime_tool(), None);
}

/// `uf test`: `test.runtime`, then the runtime its runner brings, then
/// `runtime`.
#[test]
fn the_test_runtime_is_test_runtime_then_the_runner_then_runtime() {
    let config = loaded(r#"{ runtime: "node@26" }"#);
    assert_eq!(
        config.test_runtime_tool().unwrap().source,
        ToolSource::Key("runtime")
    );

    let config = loaded(r#"{ runtime: "node@26", test: { runner: "bun@1.4" } }"#);
    let test = config.test_runtime_tool().unwrap();
    assert_eq!(test.spec, runtime("bun@1.4"));
    assert_eq!(test.source, ToolSource::ImpliedBy("test.runner"));

    let config = loaded(r#"{ runtime: "node@26", test: { runtime: "deno@2" } }"#);
    assert_eq!(
        config.test_runtime_tool().unwrap().source,
        ToolSource::Key("test.runtime")
    );

    // uf's own runner brings no runtime, so it does not stand in the way.
    let config = loaded(r#"{ runtime: "node@26", test: { runner: "uf" } }"#);
    assert_eq!(
        config.test_runtime_tool().unwrap().source,
        ToolSource::Key("runtime")
    );
}

/// A Bun runner runs on the Bun it names, so a test runtime that says
/// otherwise is one line too many.
#[test]
fn a_test_runtime_its_runner_contradicts_is_an_error_naming_both() {
    for body in [
        r#"{ test: { runtime: "node@26", runner: "bun@1.4" } }"#,
        r#"{ test: { runtime: "bun@1.3", runner: "bun@1.4" } }"#,
        r#"{ test: { runtime: "bun", runner: "bun@1.4" } }"#,
    ] {
        let error = refused(body);
        assert!(
            matches!(error, ConfigError::TestRuntimeContradictsRunner { .. }),
            "{body}: {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("test.runtime is"), "{message}");
        assert!(message.contains("test.runner is `bun@1.4`"), "{message}");
    }

    // Saying the same thing twice is allowed, if redundant.
    loaded(r#"{ test: { runtime: "bun@1.4", runner: "bun@1.4" } }"#);
    // And a runtime beside uf's own runner is the point of `test.runtime`.
    loaded(r#"{ test: { runtime: "node@26", runner: "uf" } }"#);
}

/// `build.builder` is a word for uf's own builder or a specifier for anyone's.
#[test]
fn the_builder_is_vite_or_a_module_specifier() {
    let config = loaded(r#"{ build: { builder: "./tools/my-builder" } }"#);
    let builder = config.builder_tool();
    assert_eq!(
        builder.spec,
        BuilderSpec::Module("./tools/my-builder".into())
    );
    assert_eq!(builder.spec.module(), "./tools/my-builder");
    assert_eq!(builder.source, ToolSource::Key("build.builder"));
}

/// `builder.module` still answers, says which key replaced it, and may not
/// disagree with that key.
#[test]
fn builder_module_still_answers_and_says_which_key_replaced_it() {
    let config = loaded(r#"{ builder: { module: "./tools/my-builder" } }"#);
    assert_eq!(
        config.builder_tool(),
        DeclaredTool {
            spec: BuilderSpec::Module("./tools/my-builder".into()),
            source: ToolSource::Deprecated("builder.module")
        }
    );
    let deprecation = config.builder_module_deprecation().expect("a sentence");
    assert!(deprecation.contains("build.builder"), "{deprecation}");
    assert!(
        deprecation.contains(r#"builder: "./tools/my-builder""#),
        "{deprecation}"
    );

    // uf's own builder, spelled as a package, moves as the word.
    let deprecation = loaded(r#"{ builder: { module: "@uniflowed/vite" } }"#)
        .builder_module_deprecation()
        .unwrap();
    assert!(deprecation.contains(r#"builder: "vite""#), "{deprecation}");

    // Both, agreeing, is a project half way through moving.
    let config =
        loaded(r#"{ build: { builder: "vite" }, builder: { module: "@uniflowed/vite" } }"#);
    assert_eq!(
        config.builder_tool().source,
        ToolSource::Key("build.builder")
    );
    assert!(config.builder_module_deprecation().is_some());

    let message =
        refused(r#"{ build: { builder: "vite" }, builder: { module: "./mine" } }"#).to_string();
    assert!(message.contains("build.builder is `vite`"), "{message}");
    assert!(message.contains("builder.module is `./mine`"), "{message}");
    assert!(message.contains("delete `builder.module`"), "{message}");
}

/// `pm.packageManager` still answers for a manager `packageManager` can name.
#[test]
fn pm_package_manager_still_answers_and_says_which_key_replaced_it() {
    let config = loaded(r#"{ pm: { packageManager: "pnpm" } }"#);
    assert_eq!(
        config.package_manager_tool(),
        Some(DeclaredTool {
            spec: PackageManagerSpec::parse("pnpm").unwrap(),
            source: ToolSource::Deprecated("pm.packageManager")
        })
    );
    let deprecation = config.package_manager_deprecation().unwrap();
    assert!(
        deprecation.contains(r#"packageManager: "pnpm""#),
        "{deprecation}"
    );

    // Classic is Yarn 1, which is what a prefix says.
    let deprecation = loaded(r#"{ pm: { packageManager: "yarn-classic" } }"#)
        .package_manager_deprecation()
        .unwrap();
    assert!(
        deprecation.contains(r#"packageManager: "yarn@1""#),
        "{deprecation}"
    );

    // `uf` has no spelling in the new key, so nobody is told to move it.
    let config = loaded(r#"{ pm: { packageManager: "uf" } }"#);
    assert_eq!(config.package_manager_tool(), None);
    assert_eq!(config.package_manager_deprecation(), None);
    assert_eq!(
        UniflowedConfig::default().package_manager_deprecation(),
        None
    );
}

/// Two names for the manager that disagree are an error, and Yarn's edition is
/// part of its name.
#[test]
fn package_manager_keys_that_disagree_are_an_error_naming_both() {
    let message =
        refused(r#"{ packageManager: "npm", pm: { packageManager: "pnpm" } }"#).to_string();
    assert!(message.contains("packageManager is `npm`"), "{message}");
    assert!(message.contains("pm.packageManager is `pnpm`"), "{message}");

    refused(r#"{ packageManager: "yarn@4", pm: { packageManager: "yarn-classic" } }"#);
    refused(r#"{ packageManager: "yarn@1.22.22", pm: { packageManager: "yarn-berry" } }"#);
    refused(r#"{ packageManager: "pnpm@10", pm: { packageManager: "uf" } }"#);

    loaded(r#"{ packageManager: "yarn@1", pm: { packageManager: "yarn-classic" } }"#);
    loaded(r#"{ packageManager: "yarn@4.9.2", pm: { packageManager: "yarn-berry" } }"#);
    loaded(r#"{ packageManager: "yarn", pm: { packageManager: "yarn-classic" } }"#);
    loaded(r#"{ packageManager: "pnpm@10", pm: { packageManager: "pnpm" } }"#);
}

/// The object form of `test.runner` still parses, is still read, and says
/// what replaced it.
#[test]
fn the_object_form_of_test_runner_still_answers() {
    let config = loaded(r#"{ test: { runner: { applicationTarget: "web" } } }"#);
    assert_eq!(
        config.test_runner_tool(),
        DeclaredTool {
            spec: TestRunnerSpec::Uf,
            source: ToolSource::Deprecated("test.runner")
        }
    );
    assert_eq!(
        config.test.native_runner().application_target,
        NativeTestApplicationTarget::Web
    );
    // The object prints as the object, so `uf inspect --json` still shows it.
    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(value["test"]["runner"]["applicationTarget"], "web");

    // Web is what a web project infers anyway, so the object can go.
    let deprecation = config.test_runner_deprecation().unwrap();
    assert!(deprecation.contains(r#"runner: "uf""#), "{deprecation}");
    assert!(deprecation.contains("can go"), "{deprecation}");

    // A string runner leaves the native settings at their defaults.
    let config = loaded(r#"{ test: { runner: "uf" } }"#);
    assert_eq!(
        config.test.native_runner().application_target,
        NativeTestApplicationTarget::Auto
    );
    assert_eq!(config.test_runner_deprecation(), None);

    // A typo inside the object is still a parse failure, and names the key.
    let message = refused(r#"{ test: { runner: { applicationTarget: "tv" } } }"#).to_string();
    assert!(message.contains("test.runner"), "{message}");
}

/// An object whose `applicationTarget` overrides the inference now names the
/// top-level spelling that keeps the same target.
#[test]
fn an_object_that_overrides_the_inferred_target_is_told_to_write_test_target() {
    let config = loaded(
        r#"{ app: { framework: "react-native" }, test: { runner: { applicationTarget: "web" } } }"#,
    );
    let deprecation = config.test_runner_deprecation().unwrap();
    assert!(
        deprecation.contains(r#"applicationTarget: "web""#),
        "{deprecation}"
    );
    assert!(
        deprecation.contains(r#"test.target: "web""#),
        "{deprecation}"
    );
    assert!(deprecation.contains(r#"runner: "uf""#), "{deprecation}");
}

/// `env.toolchain` is told the lines to write, and a pin that disagrees with
/// one of them is an error.
#[test]
fn env_toolchain_says_where_each_pin_goes() {
    let config = loaded(r#"{ env: { toolchain: { node: "24.14.0", pnpm: "9.15.0" } } }"#);
    let deprecation = config.toolchain_deprecation().unwrap();
    assert!(
        deprecation.contains(r#"`runtime: "node@24.14.0"`"#),
        "{deprecation}"
    );
    assert!(
        deprecation.contains(r#"`packageManager: "pnpm@9.15.0"`"#),
        "{deprecation}"
    );

    // Two runtimes is exactly the case the old key could not describe, so the
    // sentence names the keys rather than guessing which of them builds.
    let deprecation = loaded(r#"{ env: { toolchain: { node: "24.14.0", bun: "1.3.5" } } }"#)
        .toolchain_deprecation()
        .unwrap();
    assert!(deprecation.contains("`build.runtime`"), "{deprecation}");
    assert!(deprecation.contains("`test.runtime`"), "{deprecation}");
}

#[test]
fn an_env_toolchain_pin_that_disagrees_with_a_tool_key_is_an_error() {
    let error = refused(r#"{ runtime: "node@24", env: { toolchain: { node: "24.14.0" } } }"#);
    assert!(
        matches!(error, ConfigError::ToolKeysDisagree { .. }),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("runtime is `node@24`"), "{message}");
    assert!(
        message.contains("env.toolchain.node is `24.14.0`"),
        "{message}"
    );
    assert!(
        message.contains(r#"`runtime: "node@24.14.0"`"#),
        "{message}"
    );

    let message =
        refused(r#"{ test: { runner: "bun@1.2.0" }, env: { toolchain: { bun: "1.1.0" } } }"#)
            .to_string();
    assert!(message.contains("test.runner is `bun@1.2.0`"), "{message}");

    let message = refused(r#"{ packageManager: "pnpm", env: { toolchain: { pnpm: "9.15.0" } } }"#)
        .to_string();
    assert!(message.contains("packageManager is `pnpm`"), "{message}");

    // The same release, twice, agrees.
    loaded(r#"{ runtime: "node@24.14.0", env: { toolchain: { node: "24.14.0" } } }"#);
    // And a pin for a tool no key names is nobody's business but `uf env`'s.
    loaded(r#"{ runtime: "bun@1.3", env: { toolchain: { node: "24.14.0" } } }"#);
}

/// What `uf inspect` and `uf explain` print.
#[test]
fn every_role_reports_its_tool_and_the_key_it_came_from() {
    let config = loaded(
        r#"{
          runtime: "node@26",
          test: { runner: "bun@1.4" },
          builder: { module: "./tools/my-builder" },
        }"#,
    );
    let rows: Vec<(ToolRole, String)> = config
        .tool_declarations()
        .into_iter()
        .map(|row| (row.role, row.summary()))
        .collect();
    assert_eq!(
        rows,
        [
            (ToolRole::Runtime, "node@26 (runtime)".to_owned()),
            (ToolRole::BuildRuntime, "node@26 (runtime)".to_owned()),
            (
                ToolRole::TestRuntime,
                "bun@1.4 (implied by test.runner)".to_owned()
            ),
            (ToolRole::TestRunner, "bun@1.4 (test.runner)".to_owned()),
            (
                ToolRole::PackageManager,
                "not declared — detected: package.json#packageManager, then the lockfile"
                    .to_owned()
            ),
            (
                ToolRole::Builder,
                "./tools/my-builder (builder.module, deprecated)".to_owned()
            ),
        ]
    );

    let value = serde_json::to_value(config.tool_declaration(ToolRole::TestRuntime)).unwrap();
    assert_eq!(value["role"], "testRuntime");
    assert_eq!(value["spec"], "bun@1.4");
    assert_eq!(value["key"], "test.runner");
    assert_eq!(value["via"], "implied");
    assert_eq!(value["commands"], serde_json::json!(["test"]));
    assert_eq!(value["undeclared"], serde_json::Value::Null);

    let runner = UniflowedConfig::default().tool_declaration(ToolRole::TestRunner);
    assert_eq!(runner.summary(), "uf (uf's default)");
}

/// The roles each command reads, which is what `uf explain <command>` lists.
#[test]
fn each_command_reads_the_roles_the_issue_names() {
    let roles = |command| ToolRole::for_command(command).collect::<Vec<_>>();
    assert_eq!(roles("dev"), [ToolRole::BuildRuntime, ToolRole::Builder]);
    assert_eq!(roles("build"), [ToolRole::BuildRuntime, ToolRole::Builder]);
    assert_eq!(
        roles("preview"),
        [ToolRole::BuildRuntime, ToolRole::Builder]
    );
    assert_eq!(roles("start"), [ToolRole::Runtime, ToolRole::Builder]);
    assert_eq!(roles("run"), [ToolRole::Runtime]);
    assert_eq!(roles("exec"), [ToolRole::Runtime]);
    assert_eq!(roles("test"), [ToolRole::TestRuntime, ToolRole::TestRunner]);
    assert_eq!(roles("install"), [ToolRole::PackageManager]);
    assert_eq!(roles("add"), [ToolRole::PackageManager]);
    assert_eq!(roles("update"), [ToolRole::PackageManager]);
    assert_eq!(roles("fmt"), []);
}
