use uf_config::{RuleLevel, UniflowedConfig};
use uf_infra::CompactString;

use super::*;

fn source(source: &str) -> SourceFile {
    SourceFile {
        path: "src/app/page.jsx".to_string(),
        source: source.to_string(),
    }
}

fn at(path: &str, source: &str) -> SourceFile {
    SourceFile {
        path: path.to_string(),
        source: source.to_string(),
    }
}

/// A config with exactly one rule enabled, so a rule's tests cannot be muddied by
/// another rule firing on the same fixture.
fn only(rule: &str) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config
        .lint
        .rules
        .insert(CompactString::from(rule), RuleLevel::Error);
    config
}

/// Diagnostics produced by `rule` alone for `path`/`text`.
fn lint_one(rule: &str, path: &str, text: &str) -> Vec<Diagnostic> {
    lint_source(&at(path, text), &only(rule))
        .expect("lint")
        .diagnostics
}

/// Diagnostics produced by `rule` alone for a default `.js` module.
fn lint_js(rule: &str, text: &str) -> Vec<Diagnostic> {
    lint_one(rule, "app/index.js", text)
}

fn fired(diagnostics: &[Diagnostic], rule: &str) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.rule == rule)
}

// ---------------------------------------------------------------------------
// Catalogue / config agreement
// ---------------------------------------------------------------------------

#[test]
fn every_default_rule_has_a_descriptor() {
    let config = UniflowedConfig::default();
    for id in config.lint.rules.keys() {
        assert!(
            rule(id.as_str()).is_some(),
            "{id} is configured by default but has no RuleDescriptor"
        );
    }
}

#[test]
fn every_descriptor_has_a_default_rule_level() {
    let config = UniflowedConfig::default();
    for descriptor in rules() {
        assert!(
            config.lint.rules.contains_key(descriptor.id),
            "{} has a RuleDescriptor but no default level",
            descriptor.id
        );
    }
}

#[test]
fn default_levels_match_the_descriptor_catalogue() {
    let config = UniflowedConfig::default();
    for descriptor in rules() {
        assert_eq!(
            config.lint.rules.get(descriptor.id).copied(),
            Some(descriptor.default_level),
            "{} disagrees between uf_config and uf_lint",
            descriptor.id
        );
    }
}

#[test]
fn catalogue_and_config_have_the_same_size() {
    assert_eq!(UniflowedConfig::default().lint.rules.len(), rules().len());
}

#[test]
fn every_flow_builtin_lint_is_enabled_or_deliberately_off() {
    let config = UniflowedConfig::default();
    for lint in FlowBuiltinLint::all() {
        assert!(
            config.lint.rules.contains_key(lint.as_rule_id()),
            "{} has no default level",
            lint.as_rule_id()
        );
    }
}

#[test]
fn rules_are_enumerable_for_inspect() {
    let descriptor = rule("flow/unclear-type").expect("descriptor");
    assert_eq!(descriptor.category, RuleCategory::Flow);
    assert_eq!(descriptor.requirement, RuleRequirement::SourceText);
    assert!(!descriptor.description.is_empty());
}

// ---------------------------------------------------------------------------
// Type-checker-dependent rules are surfaced, never silently skipped
// ---------------------------------------------------------------------------

#[test]
fn type_checker_rules_are_reported_as_unavailable() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");

    assert!(
        report
            .unavailable
            .iter()
            .any(|entry| entry.rule == "flow/sketchy-null")
    );
    assert!(
        report
            .unavailable
            .iter()
            .all(|entry| entry.requirement == RuleRequirement::TypeChecker)
    );
    assert!(
        report
            .unavailable
            .iter()
            .all(|entry| entry.level.is_enabled())
    );
}

#[test]
fn unavailable_rules_do_not_count_as_errors() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");

    assert!(!report.unavailable.is_empty());
    assert!(!report.has_errors());
}

#[test]
fn disabled_type_checker_rules_are_not_reported_as_unavailable() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    let report = lint_source(&source("// @flow\n"), &config).expect("lint");

    assert!(report.unavailable.is_empty());
}

#[test]
fn unavailable_rules_explain_themselves() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");
    let entry = report
        .unavailable
        .iter()
        .find(|entry| entry.rule == "flow/unused-promise")
        .expect("unused-promise is enabled by default");

    assert!(entry.reason().contains("type inference"));
}

#[test]
fn unavailable_rules_are_listed_once_regardless_of_file_count() {
    let files = (0..8)
        .map(|index| at(&format!("app/{index}.js"), "// @flow\n"))
        .collect::<Vec<_>>();
    let report = lint_sources(&files, &UniflowedConfig::default()).expect("lint");

    let sketchy = report
        .unavailable
        .iter()
        .filter(|entry| entry.rule == "flow/sketchy-null")
        .count();
    assert_eq!(sketchy, 1);
    assert_eq!(report.files_checked, 8);
}

// ---------------------------------------------------------------------------
// The deprecated `flow/type-aware/no-explicit-any` alias
// ---------------------------------------------------------------------------

#[test]
fn deprecated_any_rule_id_still_configures_unclear_type() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Error,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(fired(&report.diagnostics, "flow/unclear-type"));
    assert!(!fired(
        &report.diagnostics,
        "flow/type-aware/no-explicit-any"
    ));
}

#[test]
fn deprecated_any_rule_id_can_still_switch_the_rule_off() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(report.diagnostics.is_empty());
}

#[test]
fn the_canonical_id_wins_over_the_deprecated_alias() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(report.diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// flow/* built-ins implemented from source text
// ---------------------------------------------------------------------------

#[test]
fn unclear_type_rejects_any_object_and_function() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype A = any;\ntype B = Object;\ntype C = Function;\n",
    );

    assert_eq!(diagnostics.len(), 3);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 10));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 10));
}

#[test]
fn unclear_type_accepts_precise_annotations() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype A = mixed;\ntype B = { +id: string, ... };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_value_positions() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst keys = Object.keys(props);\nconst ok = list.any;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_identifiers_that_merely_contain_any() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\nconst company = 1;\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_comments() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\n// TODO: replace any here\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn deprecated_type_rejects_the_bool_alias() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\ntype A = bool;\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
}

#[test]
fn deprecated_type_accepts_boolean() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype A = boolean;\nconst o = { bool: true };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn internal_type_rejects_flow_internals() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\ntype N = React$Node;\ntype T = $TEMPORARY$object;\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].line, 2);
    assert_eq!(diagnostics[1].line, 3);
}

#[test]
fn internal_type_accepts_the_public_equivalents() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nimport type { Node } from '@uniflowed/react';\ntype N = Node;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn ambiguous_object_type_rejects_unmarked_object_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype Props = { id: string };\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 14));
}

#[test]
fn ambiguous_object_type_accepts_exact_and_explicitly_inexact_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype A = {| id: string |};\ntype B = { id: string, ... };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn ambiguous_object_type_reaches_nested_object_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype Props = {\n  id: string,\n  meta: { title: string },\n  ...\n};\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn ambiguous_object_type_ignores_object_literals() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\nconst defaults = { id: 'x' };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unsafe_getters_setters_rejects_accessors() {
    let diagnostics = lint_js(
        "flow/unsafe-getters-setters",
        "// @flow\nclass Box {\n  get value(): number { return 1; }\n  set value(next: number) {}\n}\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 3));
}

#[test]
fn unsafe_getters_setters_accepts_plain_methods() {
    let diagnostics = lint_js(
        "flow/unsafe-getters-setters",
        "// @flow\nclass Box {\n  getValue(): number { return 1; }\n}\nconst v = map.get(key);\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unsafe_object_assign_rejects_object_assign() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst merged = Object.assign({}, base, patch);\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 16));
}

#[test]
fn unsafe_object_assign_accepts_object_spread() {
    let diagnostics = lint_js(
        "flow/unsafe-object-assign",
        "// @flow\nconst merged = { ...base, ...patch };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unnecessary_optional_chain_rejects_optional_this() {
    let diagnostics = lint_js(
        "flow/unnecessary-optional-chain",
        "// @flow\nclass A { run() { return this?.value; } }\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn unnecessary_optional_chain_accepts_chains_on_nullable_bases() {
    let diagnostics = lint_js(
        "flow/unnecessary-optional-chain",
        "// @flow\nconst v = props?.meta?.title;\nconst w = this.value;\n",
    );

    assert!(diagnostics.is_empty());
}

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

#[test]
fn nested_component_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Outer() {\n  component Inner() { return null; }\n  return <Inner />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 3));
}

#[test]
fn sibling_component_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Inner() { return null; }\ncomponent Outer() { return <Inner />; }\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn nested_hook_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\ncomponent Outer() {\n  hook useInner(): number { return 1; }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn top_level_hook_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\nhook useInner(): number { return 1; }\ncomponent Outer() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// react/*
// ---------------------------------------------------------------------------

#[test]
fn hooks_rules_reject_conditional_hook_calls() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page(flag: boolean) {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
    assert!(diagnostics[0].message.contains("top level"));
}

#[test]
fn hooks_rules_reject_hook_calls_in_callbacks() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent List(items: Array<string>) {\n  items.forEach(() => {\n    useEffect(() => {});\n  });\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn hooks_rules_reject_hook_calls_outside_any_component() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nconst value = useState(0);\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("component"));
}

#[test]
fn hooks_rules_reject_hook_calls_in_plain_functions() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nfunction helper() {\n  return useState(0);\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_component() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  const [a, setA] = useState(0);\n  useEffect(() => {});\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_hook_declaration() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nhook useThing(): number {\n  const [a] = useState(0);\n  return a;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_use_prefixed_function() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nexport const useThing = (): number => {\n  const [a] = useState(0);\n  return a;\n};\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_do_not_treat_a_declaration_as_a_call() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nfunction useThing(): number {\n  return 1;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_ignore_property_reads_that_look_like_hooks() {
    let diagnostics = lint_js("// @flow", "// @flow\n");
    assert!(diagnostics.is_empty());

    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page(api: Api) {\n  api.useThing();\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_tolerate_jsx_expression_containers() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  return <main>{useThing()}</main>;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_are_not_confused_by_braces_inside_strings() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  const s = \"}\";\n  const [a] = useState(0);\n  return s;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn no_default_export_component_rejects_a_default_export() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\ncomponent Page() { return null; }\nexport default Page;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 1));
}

#[test]
fn no_default_export_component_accepts_named_exports() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\nexport component Page() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn no_default_export_component_covers_reserved_router_modules() {
    let diagnostics = lint_one(
        "react/no-default-export-component",
        "app/_uf.page.js",
        "// @flow\nexport default function Page() { return null; }\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn no_default_export_component_leaves_plain_modules_alone() {
    let diagnostics = lint_js(
        "react/no-default-export-component",
        "// @flow\nexport default { id: 1 };\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// server/*
// ---------------------------------------------------------------------------

#[test]
fn client_modules_may_not_import_server_only_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { db } from '@uniflowed/server';\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn client_modules_may_not_import_dot_server_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { load } from './data.server.js';\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn server_modules_may_import_server_only_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\nimport { db } from '@uniflowed/server';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn client_modules_may_import_shared_modules() {
    let diagnostics = lint_js(
        "server/no-server-only-import-in-client",
        "// @flow\n'use client';\nimport { format } from './format.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn a_boundary_directive_must_lead_the_module() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\nimport { a } from './a.js';\n'use client';\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 1));
}

#[test]
fn a_leading_boundary_directive_is_accepted() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\n'use client';\nimport { a } from './a.js';\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn an_inline_use_server_directive_is_not_a_module_directive() {
    let diagnostics = lint_js(
        "server/use-client-directive-position",
        "// @flow\nexport async function save() {\n  'use server';\n}\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// uniflowed/*
// ---------------------------------------------------------------------------

#[test]
fn npm_script_invocations_are_rejected() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nspawn('npm run build');\nspawn('pnpm install');\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 8));
}

#[test]
fn uf_task_invocations_are_accepted() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nexport const tasks = { build: 'uf build' };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn npm_mentions_in_comments_are_not_invocations() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\n// migrated away from npm run build\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// security/*
// ---------------------------------------------------------------------------

#[test]
fn dangerously_set_inner_html_is_rejected_without_a_sanitizer() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\ncomponent Body(html: string) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
    assert!(diagnostics[0].message.contains("XSS"));
}

#[test]
fn dangerously_set_inner_html_is_accepted_via_a_markdown_helper() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(md: string) {\n  return <div dangerouslySetInnerHTML={{ __html: renderMarkdown(md) }} />;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn dangerously_set_inner_html_allows_the_value_on_the_next_line() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(md: string) {\n  return (\n    <div\n      dangerouslySetInnerHTML={{\n        __html: renderMarkdown(md),\n      }}\n    />\n  );\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn a_markdown_import_does_not_whitelist_an_unrelated_value() {
    let diagnostics = lint_js(
        "security/no-dangerously-set-inner-html",
        "// @flow\nimport { renderMarkdown } from '@uniflowed/markdown';\ncomponent Body(html: string) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn eval_and_friends_are_rejected() {
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\neval(input);\nconst f = new Function('return 1');\nsetTimeout('tick()', 10);\nsetInterval(`tick()`, 10);\n",
    );

    assert_eq!(diagnostics.len(), 4);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 1));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 11));
    assert_eq!(diagnostics[2].line, 4);
    assert_eq!(diagnostics[3].line, 5);
}

#[test]
fn safe_timers_and_ordinary_identifiers_are_accepted() {
    let diagnostics = lint_js(
        "security/no-eval",
        "// @flow\nsetTimeout(() => tick(), 10);\nconst evaluate = 1;\nconst f = function () {};\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// Suppression comments, end to end
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_suppresses_the_diagnostic() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = any;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn disable_next_line_does_not_leak_to_later_lines() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = any;\ntype B = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn block_suppression_covers_a_range() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable flow/unclear-type\ntype A = any;\n// uf-lint-enable flow/unclear-type\ntype B = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn suppressing_one_rule_leaves_others_reporting() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/deprecated-type"),
        RuleLevel::Error,
    );

    let report = lint_source(
        &at(
            "app/index.js",
            "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = { a: any, b: bool };\n",
        ),
        &config,
    )
    .expect("lint");

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].rule, "flow/deprecated-type");
}

#[test]
fn an_unknown_suppression_rule_id_is_its_own_diagnostic() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/unknown-lint-suppression"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Error,
    );

    let report = lint_source(
        &at(
            "app/index.js",
            "// @flow\n// uf-lint-disable-next-line flow/unclear-typo\ntype A = any;\n",
        ),
        &config,
    )
    .expect("lint");

    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(
        report.diagnostics[0].rule,
        "uniflowed/unknown-lint-suppression"
    );
    assert_eq!(report.diagnostics[0].line, 2);
    assert_eq!(report.diagnostics[1].rule, "flow/unclear-type");
}

#[test]
fn an_unknown_suppression_rule_id_never_suppresses_anything() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-typo\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, "flow/unclear-type");
}

#[test]
fn the_deprecated_alias_works_in_suppression_comments() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/type-aware/no-explicit-any\ntype A = any;\n",
    );

    assert!(diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// Positions and edge-case inputs
// ---------------------------------------------------------------------------

#[test]
fn empty_input_produces_no_diagnostics() {
    let report = lint_source(&at("app/index.js", ""), &UniflowedConfig::default()).expect("lint");

    assert!(report.diagnostics.is_empty());
    assert_eq!(report.files_checked, 1);
}

#[test]
fn crlf_line_endings_do_not_shift_positions() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\r\ntype A = any;\r\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
}

#[test]
fn crlf_line_endings_are_not_trailing_whitespace() {
    let diagnostics = lint_js("uniflowed/no-trailing-whitespace", "let a = 1;\r\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn a_byte_order_mark_does_not_shift_later_lines() {
    let diagnostics = lint_js("flow/unclear-type", "\u{feff}// @flow\ntype A = any;\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn non_ascii_content_keeps_line_numbers_correct() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst s = 'ようこそ — добро пожаловать';\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn a_file_without_a_trailing_newline_is_linted() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\ntype A = any;");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn very_large_input_is_linted_without_blowing_up() {
    let mut text = String::from("// @flow\n");
    for index in 0..5_000 {
        text.push_str("type A");
        text.push_str(&index.to_string());
        text.push_str(" = any;\n");
    }
    let diagnostics = lint_js("flow/unclear-type", &text);

    assert_eq!(diagnostics.len(), 5_000);
    assert_eq!(diagnostics[4_999].line, 5_001);
}

#[test]
fn diagnostics_are_sorted_by_position() {
    let report = lint_source(
        &at("app/index.js", "// @flow\ntype A = any;\ntype B = bool;\n"),
        &UniflowedConfig::default(),
    )
    .expect("lint");

    let positions = report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column))
        .collect::<Vec<_>>();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted);
}

#[test]
fn linting_is_idempotent() {
    let file = at("app/index.js", "// @flow\ntype A = { v: any };\n");
    let config = UniflowedConfig::default();

    let first = lint_source(&file, &config).expect("lint");
    let second = lint_source(&file, &config).expect("lint");

    assert_eq!(first, second);
}

#[test]
fn parallel_and_sequential_linting_agree() {
    let files = vec![
        at("app/a.js", "// @flow\ntype A = any;\n"),
        at("app/b.js", "// @flow\ntype B = bool;\n"),
    ];
    let config = UniflowedConfig::default();

    let parallel = lint_sources(&files, &config).expect("lint");
    let mut sequential = files
        .iter()
        .flat_map(|file| lint_source(file, &config).expect("lint").diagnostics)
        .collect::<Vec<_>>();
    sort_diagnostics(&mut sequential);

    assert_eq!(parallel.diagnostics, sequential);
    assert_eq!(parallel.files_checked, 2);
}

// ---------------------------------------------------------------------------
// Rules that existed before the Flow built-in set landed
// ---------------------------------------------------------------------------

#[test]
fn reports_tabs_and_trailing_whitespace() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-tabs"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-trailing-whitespace"),
        RuleLevel::Error,
    );

    let report =
        lint_source(&source("// @flow\n\tconst x: number = 1;  \n"), &config).expect("lint");

    assert!(report.has_errors());
    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(report.diagnostics[0].rule, "uniflowed/no-tabs");
    assert_eq!(
        report.diagnostics[1].rule,
        "uniflowed/no-trailing-whitespace"
    );
}

#[test]
fn rule_levels_can_disable_builtin_rules() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-tabs"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\n\tconst x: number = 1;\n"), &config).expect("lint");

    assert!(!report.has_errors());
    assert!(report.diagnostics.is_empty());
}

#[test]
fn reports_flow_parse_errors() {
    let diagnostics = lint_one("flow/syntax", "src/app/page.jsx", "// @flow\ntype = ;\n");

    assert!(!diagnostics.is_empty());
    assert_eq!(diagnostics[0].rule, "flow/syntax");
}

#[test]
fn flow_syntax_rule_ignores_declaration_file_extensions() {
    for path in [
        "src/app/page.js.flow",
        "src/app/types.flow",
        "src/app/page.server.flow",
    ] {
        let diagnostics = lint_one("flow/syntax", path, "// @flow\ntype = ;\n");

        assert!(
            !fired(&diagnostics, "flow/syntax"),
            "{path} must not be treated as Flow source"
        );
    }
}

#[test]
fn flow_syntax_rule_still_matches_js_spellings() {
    for path in [
        "src/app/page.js",
        "src/app/page.jsx",
        "src/app/page.mjs",
        "src/app/page.cjs",
    ] {
        let diagnostics = lint_one("flow/syntax", path, "// @flow\ntype = ;\n");

        assert!(
            fired(&diagnostics, "flow/syntax"),
            "{path} must be treated as Flow source"
        );
    }
}

#[test]
fn type_aware_rule_blocks_explicit_any() {
    let report = lint_source(
        &source("// @flow\ntype Props = { value: any };\n"),
        &UniflowedConfig::default(),
    )
    .expect("lint");

    assert!(report.has_errors());
    assert!(fired(&report.diagnostics, "flow/unclear-type"));
}

#[test]
fn framework_rule_prefers_component_syntax() {
    let diagnostics = lint_one(
        "react/component-syntax",
        "src/app/page.jsx",
        "// @flow\nimport * as React from '@uniflowed/react';\nfunction Button(): React.Node { return null; }\n",
    );

    assert!(fired(&diagnostics, "react/component-syntax"));
}

#[test]
fn hook_rule_prefers_flow_hook_syntax() {
    let diagnostics = lint_one(
        "react/hook-syntax",
        "src/app/page.jsx",
        "// @flow\nfunction useThing(): number { return 1; }\n",
    );

    assert!(fired(&diagnostics, "react/hook-syntax"));
}

#[test]
fn server_rule_rejects_secret_reads_in_client_modules() {
    let diagnostics = lint_one(
        "server/no-client-secret",
        "src/app/page.jsx",
        "// @flow\n'use client';\nconst token = process.env.PRIVATE_TOKEN;\n",
    );

    assert!(fired(&diagnostics, "server/no-client-secret"));
}

#[test]
fn server_actions_require_use_server_directive() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "server/actions.js",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn server_actions_accept_use_server_directive() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "server/actions.js",
        "\"use server\";\n// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn server_action_rule_ignores_the_removed_server_flow_spelling() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "actions.server.flow",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(!fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn server_action_rule_matches_dot_server_js_modules() {
    let diagnostics = lint_one(
        "server/use-server-actions",
        "actions.server.js",
        "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n",
    );

    assert!(fired(&diagnostics, "server/use-server-actions"));
}

#[test]
fn client_module_may_import_a_server_flow_named_module() {
    let diagnostics = lint_one(
        "server/no-server-only-import-in-client",
        "app/page.js",
        "// @flow\n'use client';\nimport { save } from './actions.server.flow';\n",
    );

    assert!(!fired(
        &diagnostics,
        "server/no-server-only-import-in-client"
    ));
}

#[test]
fn client_module_may_not_import_a_dot_server_js_module() {
    let diagnostics = lint_one(
        "server/no-server-only-import-in-client",
        "app/page.js",
        "// @flow\n'use client';\nimport { save } from './actions.server.js';\n",
    );

    assert!(fired(
        &diagnostics,
        "server/no-server-only-import-in-client"
    ));
}

#[test]
fn router_reserved_files_are_constrained() {
    let diagnostics = lint_one("router/reserved-files", "app/_uf.route.js", "// @flow\n");

    assert!(fired(&diagnostics, "router/reserved-files"));
}

/// `uf create app react` generates `_uf.page.native.js` and `_uf.page.test.js`,
/// and the rule used to reject both — a freshly scaffolded project failed its
/// own linter. The grammar now lives in `uf_router::reserved`, so the scaffold,
/// the router, and this rule cannot drift apart again.
#[test]
fn router_reserved_files_accepts_platform_and_test_variants() {
    for name in [
        "app/_uf.layout.js",
        "app/_uf.page.js",
        "app/_uf.middleware.js",
        "app/_uf.page.native.js",
        "app/_uf.page.ios.js",
        "app/_uf.page.android.js",
        "app/_uf.page.web.js",
        "app/_uf.page.test.js",
        "app/_uf.layout.test.js",
    ] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/reserved-files"),
            "{name} should be accepted"
        );
    }
}

#[test]
fn router_reserved_files_still_rejects_names_uf_does_not_define() {
    for name in [
        "app/_uf.route.js",
        "app/_uf.page.server.js",
        "app/_uf.page.native.test.js",
        "app/_uf.page.jsx",
    ] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            fired(&diagnostics, "router/reserved-files"),
            "{name} should be rejected"
        );
    }
}

#[test]
fn router_reserved_files_leaves_project_owned_names_alone() {
    for name in ["app/page.js", "app/client/Counter.js", "app/_private.js"] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/reserved-files"),
            "{name} should be untouched"
        );
    }
}

#[test]
fn package_json_scripts_are_rejected() {
    let diagnostics = lint_one(
        "package/no-npm-scripts",
        "package.json",
        "{\n  \"scripts\": { \"dev\": \"vite\" }\n}\n",
    );

    assert!(fired(&diagnostics, "package/no-npm-scripts"));
}

#[test]
fn global_fetch_override_is_rejected() {
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nglobalThis.fetch = () => Promise.resolve();\n",
    );

    assert!(fired(&diagnostics, "fetch/no-global-override"));
}

#[test]
fn render_side_effects_are_errors_by_default() {
    let diagnostics = lint_one(
        "react/no-render-side-effects",
        "src/app/page.jsx",
        "// @flow\ncomponent Clock() { return <p>{Date.now()}</p>; }\n",
    );

    assert!(fired(&diagnostics, "react/no-render-side-effects"));
}

#[test]
fn react_native_rule_prefers_platform_files() {
    let diagnostics = lint_one(
        "react-native/platform-split",
        "src/app/Button.jsx",
        "// @flow\nimport { Platform } from '@uniflowed/react-native';\nconst name = Platform.OS;\n",
    );

    assert!(fired(&diagnostics, "react-native/platform-split"));
}

/// Regression test for the QuickJS stack-budget trap.
///
/// Before `uf_flow::prepare_thread` was broadcast to the pool, linting a few
/// hundred perfectly valid files in parallel failed with
/// `Flow parser runtime error: SyntaxError: stack overflow`, because rayon ran
/// later jobs several frames deeper than the one that created each worker's
/// parser. The failure scaled with parallelism rather than with input size, so
/// a small project never saw it and a real one always did.
#[test]
fn lints_hundreds_of_files_in_parallel_without_exhausting_the_parser_stack() {
    let config = UniflowedConfig::default();
    let files = (0..800)
        .map(|index| SourceFile {
            path: format!("app/route{index}/_uf.page.js"),
            source: format!(
                "// @flow\ncomponent Page{index}() renders React.Node {{\n  return <main>hello</main>;\n}}\n"
            ),
        })
        .collect::<Vec<_>>();

    let report = lint_sources(&files, &config).expect("lint must not fail on valid sources");

    assert_eq!(report.files_checked, 800);
    let syntax = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == "flow/syntax")
        .collect::<Vec<_>>();
    assert!(
        syntax.is_empty(),
        "unexpected syntax diagnostics: {syntax:?}"
    );
}
