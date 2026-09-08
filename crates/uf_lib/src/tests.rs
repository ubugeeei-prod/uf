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
        "ScrollBox",
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

    assert_eq!(contract.components.len(), 4);
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

/// Every name a module exports, split by the namespace it lives in.
///
/// Two lists rather than one because Flow has two namespaces and a barrel
/// re-export has to name the right one: `export type { Queries }` from a module
/// whose `Queries` is a value is as broken as naming nothing at all, and the
/// error a reader gets is the same either way.
#[derive(Debug, Default)]
struct Exports {
    /// Names importable with a plain `import { … }`.
    values: Vec<String>,
    /// Names importable only with `import type { … }`.
    types: Vec<String>,
}

/// Every value a package's entry point exports, by name.
///
/// Parsed rather than matched: uf ships a Flow parser, and a regular
/// expression over `export` would be a second and worse answer to a question
/// this repository has already answered. It also gets `export { a as b }`,
/// `export type`, and the difference between them wrong in ways nobody would
/// notice until the check was believed.
///
/// Types are deliberately not collected here — see
/// [`the_registry_names_exactly_what_each_package_exports`]. They are collected
/// by [`exported_names`], which answers a different question:
/// [`a_barrel_re_export_names_something_its_source_has`] has to know both
/// namespaces because a barrel re-exports from both.
fn exported_values(source: &str) -> Result<Option<Vec<String>>, String> {
    Ok(exported_names(source)?.map(|exports| exports.values))
}

/// Both namespaces of one module, or [`None`] when it cannot be read.
///
/// `None` is `export * from "react"`: another package's whole surface, whose
/// names are not in this file. It is the one answer that is neither "exports
/// it" nor "does not", and every caller has to decide for itself what to do
/// about it rather than being handed an empty list that reads like a fact.
fn exported_names(source: &str) -> Result<Option<Exports>, String> {
    use uf_flow::ast::statement::{self, ExportKind};

    let parsed = uf_flow::parse(source).map_err(|error| format!("{error:?}"))?;
    if !parsed.diagnostics.is_empty() {
        return Err(format!("{:?}", parsed.diagnostics));
    }
    let mut exports = Exports::default();
    for node in parsed.program.statements.iter() {
        let statement::StatementInner::ExportNamedDeclaration { inner, .. } = &**node else {
            continue;
        };
        // `export { useIdle } from "./idle.js"` is an export of this package
        // and the name is right there, so a `source` is not a reason to skip —
        // these barrel modules are written almost entirely that way.
        if let Some(declaration) = &inner.declaration {
            if inner.export_kind == ExportKind::ExportType {
                exports.types.extend(declared_type_names(declaration));
            } else {
                exports.values.extend(declared_value_names(declaration));
                // `export type Alias = string` is written with the statement's
                // own kind on some spellings and with a `TypeAlias`
                // declaration under a value export on others; a declaration
                // that binds a type binds a type either way.
                exports.types.extend(declared_type_names(declaration));
            }
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
                let exported = specifier.exported.as_ref().unwrap_or(&specifier.local);
                let name = exported.name.to_string();
                if inner.export_kind == ExportKind::ExportType
                    || specifier.export_kind == ExportKind::ExportType
                {
                    exports.types.push(name);
                } else {
                    exports.values.push(name);
                }
            }
        }
    }
    exports.values.sort();
    exports.values.dedup();
    exports.types.sort();
    exports.types.dedup();
    Ok(Some(exports))
}

/// The names a declaration binds, when it binds types.
fn declared_type_names(node: &uf_flow::ast::statement::Statement<Loc, Loc>) -> Vec<String> {
    use uf_flow::ast::statement::StatementInner;

    match &**node {
        StatementInner::TypeAlias { inner, .. } => vec![inner.id.name.to_string()],
        StatementInner::OpaqueType { inner, .. } => vec![inner.id.name.to_string()],
        StatementInner::InterfaceDeclaration { inner, .. } => vec![inner.id.name.to_string()],
        _ => Vec::new(),
    }
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

/// One `export … from "…"` written in a module.
#[derive(Debug)]
struct ReExport {
    /// The name as the module it comes from spells it, before any `as`.
    local: String,
    /// That module's specifier, verbatim.
    from: String,
    /// Whether it was written `export type` — which namespace it claims.
    is_type: bool,
}

/// Every `export { … } from "…"` in one module.
///
/// Unlike [`exported_names`], an `export * from` is not a reason to stop.
/// This is about the names a module writes down, and a batch specifier writes
/// none: it neither adds a claim to check nor invalidates the ones beside it.
fn re_exports(source: &str) -> Result<Vec<ReExport>, String> {
    use uf_flow::ast::statement::{self, ExportKind};

    let parsed = uf_flow::parse(source).map_err(|error| format!("{error:?}"))?;
    if !parsed.diagnostics.is_empty() {
        return Err(format!("{:?}", parsed.diagnostics));
    }
    let mut found = Vec::new();
    for node in parsed.program.statements.iter() {
        let statement::StatementInner::ExportNamedDeclaration { inner, .. } = &**node else {
            continue;
        };
        let Some((_, from)) = &inner.source else {
            continue;
        };
        let Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) =
            &inner.specifiers
        else {
            continue;
        };
        for specifier in specifiers {
            found.push(ReExport {
                // The *local* name, not the exported one: `export { a as b }
                // from "./x.js"` asks `./x.js` for `a`, and renaming it here
                // says nothing about what that module has.
                local: specifier.local.name.to_string(),
                from: from.value.to_string(),
                is_type: inner.export_kind == ExportKind::ExportType
                    || specifier.export_kind == ExportKind::ExportType,
            });
        }
    }
    Ok(found)
}

/// The file a module specifier names, when it is one this repository owns.
///
/// [`None`] for anything else — `react`, `react-relay`, `relay-runtime`. A
/// re-export from a dependency is that dependency's business, and reading
/// `node_modules` would make this test's answer depend on what happens to be
/// installed rather than on what is in the repository.
///
/// A `@uniflowed/…` specifier is resolved through the package's own `exports`
/// map rather than by assuming `index.js`, because `@uniflowed/core/temporal`
/// is a subpath a package chose to publish and the manifest is the only thing
/// that knows which file is behind it.
fn resolve_specifier(packages: &Path, importer: &Path, specifier: &str) -> Option<PathBuf> {
    if specifier.starts_with('.') {
        return Some(importer.parent()?.join(specifier));
    }
    let rest = specifier.strip_prefix("@uniflowed/")?;
    let (package, subpath) = match rest.split_once('/') {
        Some((package, subpath)) => (package, format!("./{subpath}")),
        None => (rest, ".".to_owned()),
    };
    let directory = packages.join(package);
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(directory.join("package.json")).ok()?).ok()?;
    let target = manifest.get("exports")?.get(&subpath)?.as_str()?;
    Some(directory.join(target))
}

/// A re-export names something the module it names actually exports.
///
/// # Why this exists
///
/// `@uniflowed/testing` re-exported a type called `Screen` from
/// `@uniflowed/react-testing`, which exports `Queries` and has never exported a
/// `Screen`. Nothing said so: a barrel is the one kind of module where a wrong
/// name costs nothing to write and produces no error where it is written —
/// Flow reports the missing binding at the *import*, in somebody else's
/// project, as a name that resolves to nothing useful. That was the second
/// list in one day to have drifted from what a package really has
/// (ubugeeei-prod/uf#561 is the other), which is what
/// ubugeeei-prod/uf#574 asked for a check about.
///
/// # What it checks, and in which namespace
///
/// Every `export { … } from "…"` and `export type { … } from "…"` under
/// `packages/`, against the module the specifier names. The namespace is part
/// of the claim and is checked as part of it: `export type { X }` from a module
/// whose `X` is a value is as wrong as a name that is not there at all, and it
/// fails the same way for the same reader.
///
/// Two things are deliberately not failures. A specifier this repository does
/// not own is skipped, and so is a source module that hands on somebody else's
/// surface with `export * from` — there, "the name is absent" is not something
/// the file can be read to find out, and a check that guessed would be worse
/// than one that says which modules it could not read.
#[test]
fn a_barrel_re_export_names_something_its_source_has() {
    let root = repository_root();
    let packages = root.join("packages");
    let mut broken = Vec::new();
    let mut checked = 0usize;

    for entry in walkdir::WalkDir::new(&packages)
        .into_iter()
        .filter_map(Result::ok)
    {
        let importer = entry.path();
        if !importer.is_file() || importer.extension().and_then(|it| it.to_str()) != Some("js") {
            continue;
        }
        let Ok(source) = fs::read_to_string(importer) else {
            continue;
        };
        let here = importer.strip_prefix(&root).unwrap_or(importer).display();
        let written =
            re_exports(&source).unwrap_or_else(|error| panic!("{here} does not parse: {error}"));
        for written in written {
            let Some(target) = resolve_specifier(&packages, importer, &written.from) else {
                continue;
            };
            let Ok(target_source) = fs::read_to_string(&target) else {
                broken.push(format!(
                    "{here} re-exports `{}` from \"{}\", and there is no such module",
                    written.local, written.from
                ));
                continue;
            };
            let named = target.strip_prefix(&root).unwrap_or(&target).display();
            let Some(exports) = exported_names(&target_source)
                .unwrap_or_else(|error| panic!("{named} does not parse: {error}"))
            else {
                continue;
            };
            checked += 1;
            let (wanted, other) = if written.is_type {
                (&exports.types, &exports.values)
            } else {
                (&exports.values, &exports.types)
            };
            if wanted.contains(&written.local) {
                continue;
            }
            // The other namespace is worth saying, because a re-export written
            // with the wrong `type` keyword and one written with a name that
            // does not exist are different mistakes with different fixes.
            let hint = if other.contains(&written.local) {
                let (has, wrote) = if written.is_type {
                    ("a value", "export type")
                } else {
                    ("a type", "export")
                };
                format!(", and it is {has} there rather than what `{wrote}` asks for")
            } else {
                String::new()
            };
            broken.push(format!(
                "{here} re-exports `{}` from \"{}\", which does not export it{hint}",
                written.local, written.from
            ));
        }
    }

    assert!(
        checked > 200,
        "the walk found almost no re-exports, so it is not checking anything: {checked}"
    );
    assert!(
        broken.is_empty(),
        "{} re-export(s) name something that is not there:\n  - {}",
        broken.len(),
        broken.join("\n  - ")
    );
}
