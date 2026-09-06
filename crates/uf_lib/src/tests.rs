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
fn exported_values(source: &str) -> Result<Option<Vec<String>>, String> {
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
        // these barrel modules are written almost entirely that way.
        if let Some(declaration) = &inner.declaration {
            names.extend(declared_value_names(declaration));
        }
        // `export * from "react"` hands on another package's whole surface,
        // and the names are not in this file to be read. Recognised from the
        // tree rather than from a line of text, because `export\n  * from` is
        // the same statement and a string match would call the module checked
        // and then compare its registry entry against nothing.
        if let Some(statement::export_named_declaration::Specifier::ExportBatchSpecifier(_)) =
            &inner.specifiers
        {
            return Ok(None);
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
    Ok(Some(names))
}

/// The names a declaration binds, when it binds values.
fn declared_value_names(node: &uf_flow::ast::statement::Statement<Loc, Loc>) -> Vec<String> {
    use uf_flow::ast::{pattern, statement::StatementInner};

    match &**node {
        StatementInner::FunctionDeclaration { inner, .. } => inner
            .id
            .as_ref()
            .map(|id| vec![id.name.to_string()])
            .unwrap_or_default(),
        StatementInner::ComponentDeclaration { inner, .. } => vec![inner.id.name.to_string()],
        StatementInner::ClassDeclaration { inner, .. } => inner
            .id
            .as_ref()
            .map(|id| vec![id.name.to_string()])
            .unwrap_or_default(),
        StatementInner::EnumDeclaration { inner, .. } => vec![inner.id.name.to_string()],
        StatementInner::VariableDeclaration { inner, .. } => {
            // Every declarator. `export const a = 1, b = 2;` is not written
            // anywhere in `packages/` today, and reading only the first would
            // be a silent hole in the one check that exists to find silent
            // holes — the day somebody writes it, this must notice `b`.
            let mut names = Vec::new();
            for declarator in inner.declarations.iter() {
                if let pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                    names.push(inner.name.name.to_string());
                }
            }
            names
        }
        _ => Vec::new(),
    }
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
        // Every entry in the registry has one today, so an unreadable entry
        // point is a failure rather than a skip. A `continue` here would let a
        // renamed or deleted package go unchecked while `checked` still looked
        // healthy, which is the shape of hole this test exists to close.
        let source = fs::read_to_string(&entry).unwrap_or_else(|error| {
            panic!(
                "{} names {}, and {} cannot be read: {error}",
                "the registry",
                module.specifier,
                entry.display()
            )
        });

        let Some(exported) = exported_values(&source)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", entry.display()))
        else {
            // `export * from "react"` hands on somebody else's whole surface,
            // and the names are not in this file to be read. The entry is then
            // what uf itself adds plus the names worth suggesting, and it is
            // checked by nothing — which is worth knowing rather than hiding,
            // so the list of these is asserted below.
            exempt.push(module.specifier.to_string());
            continue;
        };
        checked += 1;
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

/// The reader that [`the_registry_names_exactly_what_each_package_exports`]
/// trusts, held to the shapes that would make it quietly wrong.
///
/// A check whose reader has a hole reports "no drift" for a package it never
/// really read, which is worse than not having the check: it is the same
/// answer with a signature on it.
#[test]
fn the_export_reader_sees_every_shape_a_package_uses() {
    // Every binding of one declaration, not the first. Nothing in `packages/`
    // is written this way today, which is exactly why the reader has to be.
    assert_eq!(
        exported_values("export const a = 1, b = 2;\n").unwrap(),
        Some(vec!["a".to_owned(), "b".to_owned()])
    );

    // The renamed name is the exported one.
    assert_eq!(
        exported_values("const inner = 1;\nexport { inner as outer };\n").unwrap(),
        Some(vec!["outer".to_owned()])
    );

    // A re-export from a relative file is an export of this package, and the
    // name is right there — the barrel modules are written almost entirely
    // this way.
    assert_eq!(
        exported_values("export { useIdle } from \"./idle.js\";\n").unwrap(),
        Some(vec!["useIdle".to_owned()])
    );

    // Types are not in this list.
    assert_eq!(
        exported_values("export type Alias = string;\nexport const value = 1;\n").unwrap(),
        Some(vec!["value".to_owned()])
    );
    assert_eq!(
        exported_values("const a = 1;\nexport type { A };\nexport { a };\n").unwrap(),
        Some(vec!["a".to_owned()])
    );

    // And the one that decides whether a module can be checked at all. Both
    // spellings are the same statement, and a reader that matched a line of
    // text would call the second one checked and then compare the registry
    // against nothing.
    assert_eq!(exported_values("export * from \"react\";\n").unwrap(), None);
    assert_eq!(
        exported_values("export\n  * from \"react\";\n").unwrap(),
        None
    );
}
