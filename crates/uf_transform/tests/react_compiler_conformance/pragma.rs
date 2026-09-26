#![allow(clippy::disallowed_macros)]

//! A fixture's first line, as the options its snapshot was compiled with.
//!
//! Three upstream functions decide those options, and this module is those
//! three in order, as they read at the pinned commit:
//!
//! 1. `parseConfigPragmaForTests`
//!    (`compiler/packages/babel-plugin-react-compiler/src/Utils/TestUtils.ts`)
//!    splits the first line on `@`, reads `key` and `key:value`, and sets an
//!    environment flag when the key is one of `EnvironmentConfigSchema`'s and a
//!    plugin option when it is one of `defaultOptions`' — defaulting
//!    `compilationMode` to `all` and `panicThreshold` to `all_errors`.
//!    `parsePluginOptions` then lowercases every string value.
//! 2. `makePluginOptions` (`compiler/packages/snap/src/compiler.ts`) overrides
//!    what the test runner always sets: `target: "19"`,
//!    `assertValidMutableRanges`, a `validatePreserveExistingMemoizationGuarantees`
//!    that is on exactly when the first line *contains* that word, and the
//!    `shared-runtime` module type provider.
//! 3. `resolveOptions` and `serializeEnvironment`
//!    (`compiler/packages/babel-plugin-react-compiler-rust/src/options.ts`) turn
//!    that into the JSON the Rust crate reads: the type provider is called once
//!    for each module the file imports, and the answers are passed as a map.
//!
//! Nothing here chooses an option. A fixture whose options this cannot build
//! is reported as a harness failure, never compiled with a guess.

use serde_json::{Map, Value, json};

/// The keys of `EnvironmentConfigSchema` in
/// `compiler/packages/babel-plugin-react-compiler/src/HIR/Environment.ts`.
/// A pragma naming anything else is not an environment flag.
const ENVIRONMENT_KEYS: [&str; 40] = [
    "customHooks",
    "moduleTypeProvider",
    "customMacros",
    "enableResetCacheOnSourceFileChanges",
    "enablePreserveExistingMemoizationGuarantees",
    "validatePreserveExistingMemoizationGuarantees",
    "validateExhaustiveMemoizationDependencies",
    "validateExhaustiveEffectDependencies",
    "enableForest",
    "flowTypeProvider",
    "enableOptionalDependencies",
    "enableNameAnonymousFunctions",
    "validateHooksUsage",
    "validateRefAccessDuringRender",
    "validateNoSetStateInRender",
    "enableUseKeyedState",
    "validateNoSetStateInEffects",
    "validateNoDerivedComputationsInEffects",
    "validateNoDerivedComputationsInEffects_exp",
    "validateNoJSXInTryStatements",
    "validateStaticComponents",
    "validateNoCapitalizedCalls",
    "validateBlocklistedImports",
    "validateSourceLocations",
    "validateNoImpureFunctionsInRender",
    "validateNoFreezingKnownMutableFunctions",
    "enableAssumeHooksFollowRulesOfReact",
    "enableTransitivelyFreezeFunctionExpressions",
    "enableEmitHookGuards",
    "enableFunctionOutlining",
    "enableJsxOutlining",
    "enableEmitInstrumentForget",
    "assertValidMutableRanges",
    "throwUnknownException__testonly",
    "enableCustomTypeDefinitionForReanimated",
    "enableTreatRefLikeIdentifiersAsRefs",
    "enableTreatSetIdentifiersAsStateSetters",
    "validateNoVoidUseMemo",
    "enableAllowSetStateFromRefsInEffects",
    "enableVerboseNoSetStateInEffect",
];

/// The keys of `defaultOptions` in
/// `compiler/packages/babel-plugin-react-compiler/src/Entrypoint/Options.ts`.
const PLUGIN_OPTION_KEYS: [&str; 15] = [
    "compilationMode",
    "panicThreshold",
    "environment",
    "logger",
    "gating",
    "noEmit",
    "outputMode",
    "dynamicGating",
    "eslintSuppressionRules",
    "flowSuppressions",
    "ignoreUseNoForget",
    "sources",
    "enableReanimatedCheck",
    "customOptOutDirectives",
    "target",
];

/// The language the test runner parsed a fixture as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// `@flow` on the first line: `hermes-parser`, Babel-shaped.
    Flow,
    /// Anything else: `@babel/parser` with `typescript` and `jsx`.
    TypeScript,
}

/// `parseLanguage` in `snap/src/compiler.ts`, which looks at the first line.
pub fn language(first_line: &str) -> Language {
    if first_line.contains("@flow") {
        Language::Flow
    } else {
        Language::TypeScript
    }
}

/// The path the plugin is told it is compiling: `transformFixtureInput` gives
/// Babel `'/' + basename`, with `.ts` appended for a TypeScript fixture.
pub fn filename(stem: &str, language: Language) -> String {
    match language {
        Language::Flow => format!("/{stem}"),
        Language::TypeScript => format!("/{stem}.ts"),
    }
}

/// The first line as `parseConfigPragmaForTests` reads it: everything before
/// the first `\n`, and nothing at all when there is no `\n`.
pub fn first_line(source: &str) -> &str {
    source.find('\n').map_or("", |end| &source[..end])
}

/// The `PluginOptions` JSON for one fixture.
///
/// `file` is the Babel `File` the module was converted to; its imports are what
/// the module type provider is asked about.
pub fn plugin_options(
    first_line: &str,
    filename: &str,
    source: &str,
    file: &Value,
) -> Result<Value, String> {
    let mut environment = Map::new();
    for (key, value) in split_pragma(first_line) {
        if !ENVIRONMENT_KEYS.contains(&key) {
            continue;
        }
        let is_set = value.is_none_or(|value| value == "true");
        if is_set && let Some(default) = complex_environment_default(key) {
            environment.insert(key.to_owned(), default);
        } else if is_set {
            environment.insert(key.to_owned(), Value::Bool(true));
        } else if value == Some("false") {
            environment.insert(key.to_owned(), Value::Bool(false));
        } else if let Some(value) = value.filter(|value| !value.is_empty()) {
            let parsed = parse_value(value)?;
            if key == "customMacros"
                && let Value::String(macro_name) = &parsed
            {
                let head = macro_name.split('.').next().unwrap_or_default();
                environment.insert(key.to_owned(), json!([head]));
                continue;
            }
            environment.insert(key.to_owned(), parsed);
        }
    }
    if environment
        .get("enableResetCacheOnSourceFileChanges")
        .is_none_or(Value::is_null)
    {
        environment.insert(
            "enableResetCacheOnSourceFileChanges".to_owned(),
            Value::Bool(false),
        );
    }

    let mut options = Map::new();
    options.insert("compilationMode".to_owned(), json!("all"));
    options.insert("panicThreshold".to_owned(), json!("all_errors"));
    options.insert("gating".to_owned(), Value::Null);
    options.insert("noEmit".to_owned(), json!(false));
    options.insert("outputMode".to_owned(), Value::Null);
    options.insert("dynamicGating".to_owned(), Value::Null);
    options.insert("eslintSuppressionRules".to_owned(), Value::Null);
    options.insert("flowSuppressions".to_owned(), json!(true));
    options.insert("ignoreUseNoForget".to_owned(), json!(false));
    options.insert("customOptOutDirectives".to_owned(), Value::Null);
    for (key, value) in split_pragma(first_line) {
        if !PLUGIN_OPTION_KEYS.contains(&key) {
            continue;
        }
        let is_set = value.is_none_or(|value| value == "true");
        let parsed = if is_set && key == "gating" {
            json!({
                "source": "ReactForgetFeatureFlag",
                "importSpecifierName": "isForgetEnabled_Fixtures",
            })
        } else if is_set {
            Value::Bool(true)
        } else if value == Some("false") {
            Value::Bool(false)
        } else if let Some(value) = value {
            parse_value(value)?
        } else {
            continue;
        };
        // `parsePluginOptions`: "normalize string configs to be case insensitive".
        let parsed = match parsed {
            Value::String(text) => Value::String(text.to_lowercase()),
            other => other,
        };
        options.insert(key.to_owned(), parsed);
    }
    for unsupported in ["environment", "logger", "sources"] {
        if options.remove(unsupported).is_some() {
            return Err(format!(
                "`@{unsupported}` sets a plugin option the Rust crate has no JSON form for"
            ));
        }
    }

    // `makePluginOptions`, which runs after the pragma and wins.
    options.remove("enableReanimatedCheck");
    options.insert("target".to_owned(), json!("19"));
    environment.insert("assertValidMutableRanges".to_owned(), Value::Bool(true));
    environment.insert(
        "validatePreserveExistingMemoizationGuarantees".to_owned(),
        Value::Bool(first_line.contains("@validatePreserveExistingMemoizationGuarantees")),
    );

    // `serializeEnvironment`: the provider, asked once per imported module.
    environment.remove("flowTypeProvider");
    environment.remove("moduleTypeProvider");
    let module_types = module_types(file);
    if !module_types.is_empty() {
        environment.insert("moduleTypeProvider".to_owned(), Value::Object(module_types));
    }

    // `resolveOptions`. `sources` is the default, which accepts any path
    // outside `node_modules`; `enableReanimatedCheck` is false; `isDev` reads
    // `__DEV__` and `NODE_ENV`, neither of which the runner sets.
    options.insert("shouldCompile".to_owned(), json!(true));
    options.insert("enableReanimated".to_owned(), json!(false));
    options.insert("isDev".to_owned(), json!(false));
    options.insert("filename".to_owned(), json!(filename));
    options.insert("environment".to_owned(), Value::Object(environment));
    options.insert("__sourceCode".to_owned(), json!(source));
    Ok(Value::Object(options))
}

/// `splitPragma`: every `@`-separated entry, trimmed, as `key` or
/// `key:value`. A key without a value stops at its first space.
fn split_pragma(pragma: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
    pragma.split('@').map(|entry| {
        let entry = entry.trim();
        match entry.find(':') {
            None => (entry.split(' ').next().unwrap_or_default(), None),
            Some(colon) => (&entry[..colon], Some(&entry[colon + 1..])),
        }
    })
}

/// `tryParseTestPragmaValue`: a double-quoted string with no quote inside is
/// that string, and anything else is JSON.
fn parse_value(value: &str) -> Result<Value, String> {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        && !inner.contains('"')
    {
        return Ok(Value::String(inner.to_owned()));
    }
    serde_json::from_str(value).map_err(|error| format!("pragma value `{value}`: {error}"))
}

/// `testComplexConfigDefaults`: what a bare `@key` means for the flags whose
/// value is not a boolean.
fn complex_environment_default(key: &str) -> Option<Value> {
    match key {
        "validateNoCapitalizedCalls" => Some(json!([])),
        "enableEmitInstrumentForget" => Some(json!({
            "fn": {
                "source": "react-compiler-runtime",
                "importSpecifierName": "useRenderCounter",
            },
            "gating": {
                "source": "react-compiler-runtime",
                "importSpecifierName": "shouldInstrument",
            },
            "globalGating": "DEV",
        })),
        "enableEmitHookGuards" => Some(json!({
            "source": "react-compiler-runtime",
            "importSpecifierName": "$dispatcherGuard",
        })),
        _ => None,
    }
}

/// The provider's answer for each module `file` imports at the top level.
fn module_types(file: &Value) -> Map<String, Value> {
    let mut types = Map::new();
    let body = file["program"]["body"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for statement in body {
        if statement["type"] != "ImportDeclaration" {
            continue;
        }
        let Some(module) = statement["source"]["value"].as_str() else {
            continue;
        };
        if types.contains_key(module) {
            continue;
        }
        if let Some(config) = shared_runtime_type_provider(module) {
            types.insert(module.to_owned(), config);
        }
    }
    types
}

/// `makeSharedRuntimeTypeProvider` in
/// `compiler/packages/snap/src/sprout/shared-runtime-type-provider.ts`, with its
/// `Effect`, `ValueKind` and `ValueReason` members written as the strings those
/// enums hold.
fn shared_runtime_type_provider(module: &str) -> Option<Value> {
    let primitive_function = |positional: Value, rest: Value| {
        json!({
            "kind": "function",
            "calleeEffect": "read",
            "positionalParams": positional,
            "restParam": rest,
            "returnType": {"kind": "type", "name": "Primitive"},
            "returnValueKind": "primitive",
        })
    };
    let signature = |params: Value, returns: &str, effects: Value| {
        json!({
            "receiver": "@receiver",
            "params": params,
            "rest": null,
            "returns": returns,
            "temporaries": [],
            "effects": effects,
        })
    };
    match module {
        "shared-runtime" => Some(json!({
            "kind": "object",
            "properties": {
                "default": primitive_function(json!([]), json!("read")),
                "graphql": primitive_function(json!([]), json!("read")),
                "typedArrayPush": primitive_function(json!(["store", "capture"]), json!("capture")),
                "typedLog": primitive_function(json!([]), json!("read")),
                "useFreeze": {
                    "kind": "hook",
                    "returnType": {"kind": "type", "name": "Any"},
                },
                "useFragment": {
                    "kind": "hook",
                    "returnType": {"kind": "type", "name": "MixedReadonly"},
                    "noAlias": true,
                },
                "useNoAlias": {
                    "kind": "hook",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "noAlias": true,
                },
                "typedIdentity": {
                    "kind": "function",
                    "positionalParams": ["read"],
                    "restParam": null,
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "aliasing": signature(
                        json!(["@value"]),
                        "@return",
                        json!([{"kind": "Assign", "from": "@value", "into": "@return"}]),
                    ),
                },
                "typedAssign": {
                    "kind": "function",
                    "positionalParams": ["read"],
                    "restParam": null,
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "aliasing": signature(
                        json!(["@value"]),
                        "@return",
                        json!([{"kind": "Assign", "from": "@value", "into": "@return"}]),
                    ),
                },
                "typedAlias": {
                    "kind": "function",
                    "positionalParams": ["read"],
                    "restParam": null,
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "aliasing": signature(
                        json!(["@value"]),
                        "@return",
                        json!([
                            {
                                "kind": "Create",
                                "into": "@return",
                                "value": "mutable",
                                "reason": "known-return-signature",
                            },
                            {"kind": "Alias", "from": "@value", "into": "@return"},
                        ]),
                    ),
                },
                "typedCapture": {
                    "kind": "function",
                    "positionalParams": ["read"],
                    "restParam": null,
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Array"},
                    "returnValueKind": "mutable",
                    "aliasing": signature(
                        json!(["@value"]),
                        "@return",
                        json!([
                            {
                                "kind": "Create",
                                "into": "@return",
                                "value": "mutable",
                                "reason": "known-return-signature",
                            },
                            {"kind": "Capture", "from": "@value", "into": "@return"},
                        ]),
                    ),
                },
                "typedCreateFrom": {
                    "kind": "function",
                    "positionalParams": ["read"],
                    "restParam": null,
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "aliasing": signature(
                        json!(["@value"]),
                        "@return",
                        json!([{"kind": "CreateFrom", "from": "@value", "into": "@return"}]),
                    ),
                },
                "typedMutate": {
                    "kind": "function",
                    "positionalParams": ["read", "capture"],
                    "restParam": null,
                    "calleeEffect": "store",
                    "returnType": {"kind": "type", "name": "Primitive"},
                    "returnValueKind": "primitive",
                    "aliasing": signature(
                        json!(["@object", "@value"]),
                        "@return",
                        json!([
                            {
                                "kind": "Create",
                                "into": "@return",
                                "value": "primitive",
                                "reason": "known-return-signature",
                            },
                            {"kind": "Mutate", "value": "@object"},
                            {"kind": "Capture", "from": "@value", "into": "@object"},
                        ]),
                    ),
                },
                "PanResponder": {
                    "kind": "object",
                    "properties": {
                        "create": {
                            "kind": "function",
                            "positionalParams": ["freeze"],
                            "restParam": null,
                            "calleeEffect": "read",
                            "returnType": {"kind": "type", "name": "Any"},
                            "returnValueKind": "frozen",
                            "aliasing": signature(
                                json!(["@config"]),
                                "@returns",
                                json!([
                                    {
                                        "kind": "Freeze",
                                        "value": "@config",
                                        "reason": "known-return-signature",
                                    },
                                    {
                                        "kind": "Create",
                                        "into": "@returns",
                                        "value": "frozen",
                                        "reason": "known-return-signature",
                                    },
                                    {
                                        "kind": "ImmutableCapture",
                                        "from": "@config",
                                        "into": "@returns",
                                    },
                                ]),
                            ),
                        },
                    },
                },
            },
        })),
        "ReactCompilerKnownIncompatibleTest" => Some(json!({
            "kind": "object",
            "properties": {
                "useKnownIncompatible": {
                    "kind": "hook",
                    "positionalParams": [],
                    "restParam": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "knownIncompatible": "useKnownIncompatible is known to be incompatible",
                },
                "useKnownIncompatibleIndirect": {
                    "kind": "hook",
                    "positionalParams": [],
                    "restParam": "read",
                    "returnType": {
                        "kind": "object",
                        "properties": {
                            "incompatible": {
                                "kind": "function",
                                "positionalParams": [],
                                "restParam": "read",
                                "calleeEffect": "read",
                                "returnType": {"kind": "type", "name": "Any"},
                                "returnValueKind": "mutable",
                                "knownIncompatible": "useKnownIncompatibleIndirect returns an incompatible() function that is known incompatible",
                            },
                        },
                    },
                },
                "knownIncompatible": {
                    "kind": "function",
                    "positionalParams": [],
                    "restParam": "read",
                    "calleeEffect": "read",
                    "returnType": {"kind": "type", "name": "Any"},
                    "returnValueKind": "mutable",
                    "knownIncompatible": "useKnownIncompatible is known to be incompatible",
                },
            },
        })),
        "ReactCompilerTest" => Some(json!({
            "kind": "object",
            "properties": {
                "useHookNotTypedAsHook": {"kind": "type", "name": "Any"},
                "notAhookTypedAsHook": {
                    "kind": "hook",
                    "returnType": {"kind": "type", "name": "Any"},
                },
            },
        })),
        "useDefaultExportNotTypedAsHook" => Some(json!({
            "kind": "object",
            "properties": {
                "default": {"kind": "type", "name": "Any"},
            },
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{first_line, plugin_options};

    fn options(line: &str) -> serde_json::Value {
        let source = format!("{line}\nfunction f() {{}}\n");
        plugin_options(
            first_line(&source),
            "/f.ts",
            &source,
            &json!({"program": {"body": []}}),
        )
        .expect("options")
    }

    #[test]
    fn a_fixture_without_pragmas_compiles_everything_and_throws_on_any_error() {
        let options = options("function f() {}");
        assert_eq!(options["compilationMode"], "all");
        assert_eq!(options["panicThreshold"], "all_errors");
        assert_eq!(options["target"], "19");
        assert_eq!(
            options["environment"]["validatePreserveExistingMemoizationGuarantees"],
            false
        );
        assert_eq!(options["environment"]["assertValidMutableRanges"], true);
    }

    #[test]
    fn pragmas_set_plugin_options_and_environment_flags() {
        let options = options(
            r#"// @flow @compilationMode:"INFER" @enableJsxOutlining @validateNoSetStateInRender:false @customMacros:"idx.a""#,
        );
        assert_eq!(options["compilationMode"], "infer");
        assert_eq!(options["environment"]["enableJsxOutlining"], true);
        assert_eq!(options["environment"]["validateNoSetStateInRender"], false);
        assert_eq!(options["environment"]["customMacros"], json!(["idx"]));
    }

    #[test]
    fn a_bare_gating_pragma_is_the_fixture_gate_and_target_is_always_19() {
        let options = options(r#"// @gating @target:"18""#);
        assert_eq!(options["gating"]["source"], "ReactForgetFeatureFlag");
        assert_eq!(options["target"], "19");
    }

    #[test]
    fn the_preserve_memo_validation_follows_the_word_not_its_value() {
        let options = options("// @validatePreserveExistingMemoizationGuarantees:false");
        assert_eq!(
            options["environment"]["validatePreserveExistingMemoizationGuarantees"],
            true
        );
    }

    #[test]
    fn the_options_deserialize_into_the_crate() {
        let source = "import {useFreeze} from 'shared-runtime';\n";
        let file = json!({"program": {"body": [
            {"type": "ImportDeclaration", "source": {"type": "StringLiteral", "value": "shared-runtime"}},
        ]}});
        let options = plugin_options("", "/f.ts", source, &file).expect("options");
        assert!(options["environment"]["moduleTypeProvider"]["shared-runtime"].is_object());
        serde_json::from_value::<react_compiler::entrypoint::PluginOptions>(options)
            .expect("the crate accepts the options");
    }
}
