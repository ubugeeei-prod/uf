use super::*;

use std::fs;
use std::path::{Path, PathBuf};

use uf_flow::Loc;

/// This checkout, found by a file rather than by counting `..`.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the repository")
}

#[test]
fn exposes_test_api_from_root_uf_module() {
    let root = module_by_specifier("@uniflowed/core").expect("root module");

    assert_eq!(root.kind, NativeModuleKind::Runtime);
    assert!(root.flow_exports.iter().any(|export| export == "describe"));
    assert!(root.flow_exports.iter().any(|export| export == "it"));
}

#[test]
fn includes_react_flow_app_builtins() {
    let modules = builtin_modules();
    let specs = modules
        .iter()
        .map(|module| module.specifier.as_str())
        .collect::<Vec<_>>();

    assert!(specs.contains(&"@uniflowed/router"));
    assert!(specs.contains(&"@uniflowed/react"));
    assert!(specs.contains(&"@uniflowed/react-native"));
    assert!(specs.contains(&"@uniflowed/brand"));
    assert!(specs.contains(&"@uniflowed/testing"));
    assert!(specs.contains(&"@uniflowed/lib"));
    assert!(specs.contains(&"@uniflowed/lint"));
    assert!(specs.contains(&"@uniflowed/server"));
    assert!(specs.contains(&"@uniflowed/hooks"));
    assert!(specs.contains(&"@uniflowed/query"));
    assert!(specs.contains(&"@uniflowed/fetch"));
    assert!(specs.contains(&"@uniflowed/loader"));
    assert!(specs.contains(&"@uniflowed/effect"));
    assert!(specs.contains(&"@uniflowed/relay"));
    assert!(specs.contains(&"@uniflowed/graphql"));
    assert!(specs.contains(&"@uniflowed/web"));
    assert!(specs.contains(&"@uniflowed/markdown"));
    assert!(specs.contains(&"@uniflowed/temporal"));
    assert!(specs.contains(&"@uniflowed/pwa"));
    assert!(specs.contains(&"@uniflowed/prepare"));
    assert!(specs.contains(&"@uniflowed/stylex"));
    assert!(specs.contains(&"@uniflowed/ui"));
    assert!(specs.contains(&"@uniflowed/react-compiler"));
    assert!(specs.contains(&"@uniflowed/cell"));
    assert!(specs.contains(&"@uniflowed/state"));
    assert!(specs.contains(&"@uniflowed/validator"));
    assert!(specs.contains(&"@uniflowed/mock"));
    assert!(specs.contains(&"@uniflowed/browser"));
    assert!(specs.contains(&"@uniflowed/story"));
    assert!(specs.contains(&"@uniflowed/vrt"));
    assert!(specs.contains(&"@uniflowed/motion"));
    assert!(specs.contains(&"@uniflowed/tui"));
    assert!(specs.contains(&"@uniflowed/cli"));
}

#[test]
fn tui_registry_names_what_the_package_exports() {
    let module = module_by_specifier("@uniflowed/tui").expect("tui module");
    let contract = tui_contract();

    assert_eq!(module.kind, NativeModuleKind::Ui);
    // `render` and `testRender`, not `renderTui` — and no `contract`. Those
    // were the names of a declaration that threw; these are the names of the
    // renderer that replaced it in ubugeeei-prod/uf#247.
    for export in [
        "render",
        "testRender",
        "Box",
        "Text",
        "Input",
        "useKeyboard",
    ] {
        assert!(
            module.flow_exports.iter().any(|name| name == export),
            "@uniflowed/tui exports {export}"
        );
    }
    for gone in ["renderTui", "contract", "FrameBuffer"] {
        assert!(
            !module.flow_exports.iter().any(|name| name == gone),
            "@uniflowed/tui no longer exports {gone}"
        );
    }

    assert_eq!(contract.components.len(), 3);
    assert!(!contract.react_ink_target.replacement_ready);
}

#[test]
fn ui_registry_uses_compound_parts_for_complex_components() {
    let dialog = ui_components()
        .into_iter()
        .find(|component| component.name == "Dialog")
        .expect("Dialog");

    assert_eq!(dialog.runtime, UiRuntime::Split);
    assert!(dialog.preset_style);
    assert!(dialog.has_part("Body"));
    assert!(dialog.has_part("Trigger"));
}

#[test]
fn ui_registry_covers_shadcn_style_catalog() {
    let components = ui_components();
    let names = components
        .iter()
        .map(|component| component.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"Accordion"));
    assert!(names.contains(&"Command"));
    assert!(names.contains(&"DataTable"));
    assert!(names.contains(&"Sheet"));
    assert!(names.contains(&"Tooltip"));
    assert!(components.len() >= 40);
}

#[test]
fn form_registry_is_validator_backed_and_react_compiler_safe() {
    let form = ui_components()
        .into_iter()
        .find(|component| component.name == "Form")
        .expect("Form");
    let contract = form.form.expect("form contract");

    assert_eq!(form.runtime, UiRuntime::Split);
    assert_eq!(contract.validator_module, "@uniflowed/validator");
    assert_eq!(contract.schema_kind, SchemaKind::Object);
    assert!(contract.compiler_safe);
    assert!(contract.render_idempotent);
    assert_eq!(
        contract.mutation_phase,
        FormMutationPhase::EventOrServerAction
    );
}

#[test]
fn hooks_registry_prefers_react_idempotency() {
    let hooks = hook_descriptors();

    assert!(hooks.iter().all(|hook| hook.idempotent_render));
    assert!(hooks.iter().any(|hook| hook.name == "useStableCallback"));
    assert!(hooks.iter().any(|hook| hook.server_component_safe));
}

/// Every value a package's entry point exports, by name.
///
/// Parsed rather than matched: uf ships a Flow parser, and a regular
/// expression over `export` would be a second and worse answer to a question
/// this repository has already answered. It also gets `export { a as b }`,
/// `export type`, and the difference between them wrong in ways nobody would
/// notice until the check was believed.
///
/// Types are deliberately not collected — see
/// [`the_registry_names_exactly_what_each_package_exports`].
fn exported_values(source: &str) -> Result<Vec<String>, String> {
    use uf_flow::ast::statement::{self, ExportKind};

    let parsed = uf_flow::parse(source).map_err(|error| format!("{error:?}"))?;
    if !parsed.diagnostics.is_empty() {
        return Err(format!("{:?}", parsed.diagnostics));
    }
    let mut names = Vec::new();
    for node in parsed.program.statements.iter() {
        let statement::StatementInner::ExportNamedDeclaration { inner, .. } = &**node else {
            continue;
        };
        if inner.export_kind == ExportKind::ExportType {
            continue;
        }
        // `export { useIdle } from "./idle.js"` is an export of this package
        // and the name is right there, so a `source` is not a reason to skip —
        // these barrel modules are written almost entirely that way. Only
        // `export * from`, whose names are in another file, cannot be read,
        // and `re_exports_everything` excuses the whole module for it.
        if let Some(declaration) = &inner.declaration {
            names.extend(declared_value_name(declaration));
        }
        if let Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) =
            &inner.specifiers
        {
            for specifier in specifiers {
                if specifier.export_kind == ExportKind::ExportType {
                    continue;
                }
                let exported = specifier.exported.as_ref().unwrap_or(&specifier.local);
                names.push(exported.name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// The name a declaration binds, when it binds a value.
fn declared_value_name(node: &uf_flow::ast::statement::Statement<Loc, Loc>) -> Option<String> {
    use uf_flow::ast::{pattern, statement::StatementInner};

    match &**node {
        StatementInner::FunctionDeclaration { inner, .. } => {
            Some(inner.id.as_ref()?.name.to_string())
        }
        StatementInner::ComponentDeclaration { inner, .. } => Some(inner.id.name.to_string()),
        StatementInner::ClassDeclaration { inner, .. } => Some(inner.id.as_ref()?.name.to_string()),
        StatementInner::EnumDeclaration { inner, .. } => Some(inner.id.name.to_string()),
        StatementInner::VariableDeclaration { inner, .. } => {
            // The first declarator only: `export const a = 1, b = 2;` is not
            // written anywhere in `packages/`, and guessing at it would be a
            // rule nobody had decided.
            let declarator = inner.declarations.first()?;
            match &declarator.id {
                pattern::Pattern::Identifier { inner, .. } => Some(inner.name.name.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether a module hands on another package's whole surface.
fn re_exports_everything(source: &str) -> bool {
    source
        .lines()
        .any(|line| line.trim_start().starts_with("export * from"))
}

/// The registry's list for a package is exactly the values that package
/// exports.
///
/// # What the list is
///
/// `registry.rs` says it records "what Flow names it exports", and this is the
/// test that makes that true rather than aspirational. Two readings were
/// possible — every export, or the names an editor should suggest — and the
/// data settles it: with types excluded, twenty-seven of the forty-four
/// entries already matched their package exactly, including the four largest
/// (`@uniflowed/validator` at 73, `@uniflowed/effect` at 71, `@uniflowed/hooks`
/// at 52, `@uniflowed/state` at 24). A hand-curated list does not come out
/// exact for seventy-three names by accident. The seventeen that disagreed
/// were drift.
///
/// # Why values and not types
///
/// No entry has ever listed a type, and there is a reason beyond habit: a type
/// is imported with `import type`, which the checker resolves through the file
/// itself, so the registry is not what answers for it. The registry answers
/// "what can I import and call", which is the question `uf lint`'s
/// `flow/native-module` rule and the editor's completion both ask.
#[test]
fn the_registry_names_exactly_what_each_package_exports() {
    let root = repository_root();
    let mut drifted = Vec::new();
    let mut checked = 0usize;
    let mut exempt = Vec::new();

    for module in builtin_modules() {
        let Some(name) = module.specifier.strip_prefix("@uniflowed/") else {
            continue;
        };
        let entry = root.join("packages").join(name).join("index.js");
        let Ok(source) = fs::read_to_string(&entry) else {
            // A declaration-only package with no entry point of its own. The
            // registry is the only description of it there is, so there is
            // nothing to compare it against.
            continue;
        };
        if re_exports_everything(&source) {
            // `export * from "react"` hands on somebody else's whole surface,
            // and the names are not in this file to be read. The entry is then
            // what uf itself adds plus the names worth suggesting, and it is
            // checked by nothing — which is worth knowing rather than hiding,
            // so the list of these is asserted below.
            exempt.push(module.specifier.to_string());
            continue;
        }
        checked += 1;

        let exported = exported_values(&source)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", entry.display()));
        let listed: Vec<String> = module
            .flow_exports
            .iter()
            .map(|export| export.to_string())
            .collect();
        let mut sorted = listed.clone();
        sorted.sort();
        sorted.dedup();

        if sorted != exported {
            let missing: Vec<&String> = exported.iter().filter(|n| !sorted.contains(n)).collect();
            let absent: Vec<&String> = sorted.iter().filter(|n| !exported.contains(n)).collect();
            drifted.push(format!(
                "{}\n      exports but the registry does not name: {missing:?}\n      the registry names but does not export: {absent:?}",
                module.specifier
            ));
        }
    }

    assert!(
        checked > 30,
        "the walk found almost nothing, so it is not checking anything: {checked}"
    );
    assert!(
        drifted.is_empty(),
        "the registry disagrees with {} package(s):\n  - {}",
        drifted.len(),
        drifted.join("\n  - ")
    );
    // Named rather than counted: each one is a package whose entry this test
    // cannot check, and adding a third should be a decision somebody makes.
    exempt.sort();
    assert_eq!(exempt, ["@uniflowed/react", "@uniflowed/relay"]);
}
