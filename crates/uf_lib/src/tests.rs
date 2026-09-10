use super::*;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use compact_str::CompactString;

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
    assert_eq!(dialog.readiness, UiReadiness::Implemented);
    // Named rather than asserted to be non-empty: `preset_style` used to be a
    // `bool` that `UiComponent::new` set and nothing unset, so it said "styled"
    // about every entry in the table. A test that only asked whether the list
    // had something in it would be the same claim in a longer form.
    assert_eq!(
        dialog
            .preset_styles
            .iter()
            .map(CompactString::as_str)
            .collect::<Vec<_>>(),
        ["backdropStyles", "dialogStyles"]
    );
    assert!(dialog.has_part("Body"));
    assert!(dialog.has_part("Trigger"));
}

/// The table is a roadmap, and this is the half of it that is not an inventory.
///
/// The assertion that used to be here was `components.len() >= 40`, which is
/// the shape of claim ubugeeei-prod/uf#249 is about: it held the *size* of the
/// catalogue rather than the existence of anything in it, and it passed just as
/// happily on the day the table named fifty-one components and the package
/// shipped seven. What the size is worth checking for is that the roadmap has
/// not quietly been emptied — so it is asserted here about the entries that are
/// *not* implemented, and `the_ui_table_names_exactly_what_the_package_ships`
/// holds the ones that are against the package.
#[test]
fn ui_registry_keeps_the_roadmap_it_is_not_an_inventory_of() {
    let components = ui_components();
    let named = |name: &str| {
        components
            .iter()
            .find(|component| component.name == name)
            .unwrap_or_else(|| panic!("the table names {name}"))
    };

    // Deliberately not implemented, each with the decision on the entry. These
    // are the ones the check below has to tell from a gap, which is why the
    // fact is in the data rather than in a comment (ubugeeei-prod/uf#561).
    for declined in ["Command", "DataTable", "Chart", "Form", "AspectRatio"] {
        assert_eq!(
            named(declined).readiness,
            UiReadiness::Declined,
            "{declined} is a decision, not a gap"
        );
    }
    // And the five presentational-looking components that are not
    // presentational, which is ubugeeei-prod/uf#298's list. They were the whole
    // of the `Planned` half until they shipped, which is why the size assertion
    // below counts everything that is not implemented rather than only them.
    for shipped in ["Alert", "Avatar", "Breadcrumb", "Separator", "Skeleton"] {
        assert_eq!(
            named(shipped).readiness,
            UiReadiness::Implemented,
            "{shipped} had a decision in it and #298 shipped it"
        );
    }
    // A second name for a component that exists is not an entry: a dropdown
    // menu is `Menu` and Sonner is `Toast`, and both said so by carrying parts
    // no import could reach.
    for gone in ["DropdownMenu", "Sonner"] {
        assert!(
            !components.iter().any(|component| component.name == gone),
            "{gone} is another name for a component this table already has"
        );
    }

    // Counted over everything that is not implemented rather than over the
    // planned entries alone, because #298 shipped the last five planned ones
    // and an assertion about that half alone would now be an assertion about
    // zero. What it is really holding is unchanged: this table carries the
    // decisions as well as the inventory, and an entry deleted rather than
    // decided would take one of them out of the count.
    let undelivered = components
        .iter()
        .filter(|component| component.readiness != UiReadiness::Implemented)
        .count();
    assert!(
        undelivered > 3,
        "the roadmap has been emptied: {undelivered}"
    );
}

#[test]
fn form_registry_is_validator_backed_and_react_compiler_safe() {
    let form = ui_components()
        .into_iter()
        .find(|component| component.name == "Form")
        .expect("Form");
    let contract = form.form.clone().expect("form contract");

    // The contract is about `@uniflowed/form` and `Field` together, and the
    // entry it hangs on is declined — which is the correction
    // ubugeeei-prod/uf#249 asked for by name. Every assertion below was true of
    // a component that did not exist; the readiness is what says which of them
    // is a claim about something a caller can import.
    assert_eq!(form.readiness, UiReadiness::Declined);
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

/// The std registry's shipping entries are exactly `packages/std`'s subpaths,
/// and each one names exactly what its file exports.
///
/// The same check as [`the_registry_names_exactly_what_each_package_exports`],
/// against the other table — and it had to be written because that one cannot
/// see this drift. `uf_lib::builtin_modules()` carries `@uniflowed/std` and
/// stops there; `uf_std::std_modules()` carries the subpaths, and until
/// ubugeeei-prod/uf#710 it named forty-four of them, of which zero had a file
/// and none of the six that `packages/std` actually ships was among them.
/// `uf inspect --json` printed all forty-four under `stdModules`, so a reader
/// — or an agent — was told this project had an `@uniflowed/std/sql` with a
/// migration runner in it.
///
/// Four edges, so that no addition can be half-made:
///
/// 1. every [`StdStatus::Ships`] specifier is a subpath the manifest exports,
/// 2. every subpath the manifest exports is a shipping specifier,
/// 3. every shipping entry's export list is exactly its file's values, and
/// 4. every `.js` file in the package is one of them.
///
/// The last one is not redundant with the first three. A file added without a
/// manifest entry is unreachable from outside the workspace and resolves
/// perfectly inside it, which is the shape of hole `@uniflowed/ui` lived in.
#[test]
fn the_std_registry_names_exactly_what_the_std_package_exports() {
    let package = repository_root().join("packages").join("std");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(package.join("package.json")).expect("packages/std/package.json"),
    )
    .expect("packages/std/package.json parses");

    // `./package.json` is a subpath every package publishes so that a consumer
    // can read the manifest, and `.` is the declaration surface, which has its
    // own status. Neither is a module this table is about.
    let published: BTreeSet<String> = manifest["exports"]
        .as_object()
        .expect("packages/std declares exports")
        .keys()
        .filter(|key| key.as_str() != "." && key.as_str() != "./package.json")
        .map(|key| format!("@uniflowed/std/{}", key.trim_start_matches("./")))
        .collect();

    let modules = std_module_descriptors();
    let shipping: BTreeSet<String> = modules
        .iter()
        .filter(|module| module.status == StdStatus::Ships)
        .map(|module| module.specifier.to_string())
        .collect();

    assert!(
        !published.is_empty(),
        "packages/std exports no subpath, so this is not checking anything"
    );
    assert_eq!(
        shipping,
        published,
        "the std registry and packages/std disagree\n  \
         the registry says ships and the manifest does not export: {:?}\n  \
         the manifest exports and the registry does not call shipped: {:?}",
        shipping.difference(&published).collect::<Vec<_>>(),
        published.difference(&shipping).collect::<Vec<_>>()
    );

    let mut drifted = Vec::new();
    let mut files = BTreeSet::new();
    for module in modules.iter() {
        if module.status != StdStatus::Ships {
            continue;
        }
        let subpath = module
            .specifier
            .strip_prefix("@uniflowed/std/")
            .expect("a shipping specifier is a subpath");
        let file = package.join(format!("{subpath}.js"));
        files.insert(file.clone());
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", file.display()));
        let exported = exported_values(&source)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", file.display()))
            // `export * from` is refused a few tests down for every shipped
            // module, so a std module cannot be the one that hands on somebody
            // else's surface.
            .unwrap_or_else(|| panic!("{} re-exports a whole module", file.display()));

        let mut listed: Vec<String> = module
            .exports
            .iter()
            .map(|export| export.to_string())
            .collect();
        listed.sort();
        listed.dedup();
        if listed != exported {
            let missing: Vec<&String> = exported.iter().filter(|n| !listed.contains(n)).collect();
            let absent: Vec<&String> = listed.iter().filter(|n| !exported.contains(n)).collect();
            drifted.push(format!(
                "{}\n      exports but the registry does not name: {missing:?}\n      the registry names but does not export: {absent:?}",
                module.specifier
            ));
        }
    }
    assert!(
        drifted.is_empty(),
        "the std registry disagrees with {} module(s):\n  - {}",
        drifted.len(),
        drifted.join("\n  - ")
    );

    // The fourth edge. `index.js` is the declaration surface and is named by
    // `.`; everything else in the package has to be a module the table knows
    // about.
    let mut unaccounted = Vec::new();
    for entry in fs::read_dir(&package).expect("packages/std is readable") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_some_and(|extension| extension == "js")
            && path.file_name().is_some_and(|name| name != "index.js")
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".test.js"))
            && !files.contains(&path)
        {
            unaccounted.push(path.display().to_string());
        }
    }
    assert!(
        unaccounted.is_empty(),
        "packages/std has {} module(s) the std registry does not name: {}",
        unaccounted.len(),
        unaccounted.join(", ")
    );
}

/// The string-literal members of one `export type Name = "a" | "b";`.
///
/// Scanned from the declaration's own span rather than from the file, which is
/// the difference between reading a union and reading every quoted word in a
/// module — including the ones in the comment above it. The Flow parser next
/// door answers "what does this module export", which is a different question;
/// walking a type annotation to answer this one would be a bigger change than
/// the thing it checks, and the span is unambiguous: `=` to `;`.
fn string_union(source: &str, name: &str) -> BTreeSet<String> {
    let head = format!("export type {name} =");
    let start = source
        .find(&head)
        .unwrap_or_else(|| panic!("packages/std/index.js declares {name}"))
        + head.len();
    let body = &source[start..];
    let end = body
        .find(';')
        .unwrap_or_else(|| panic!("{name}'s declaration is terminated"));

    let mut members = BTreeSet::new();
    let mut rest = &body[..end];
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        members.insert(after[..close].to_owned());
        rest = &after[close + 1..];
    }
    members
}

/// `packages/std/index.js`'s unions name the registry's own statuses and
/// categories.
///
/// `StdModule` is declared twice — once as a Rust struct that `uf inspect
/// --json` serialises and once as the Flow type describing what `modules()`
/// returns — and a union in one of them that the other does not have is a
/// declaration that lies to the checker about a value it will be handed. The
/// `status` field is new in ubugeeei-prod/uf#710, so this is the guard that
/// comes with it rather than the care that would have to.
///
/// Compared against the names the registry actually *uses*, not against a list
/// of variants written down a third time. An enum variant no entry carries is
/// not a fact about the package, and a list written here to be complete is the
/// second place with the same names in it that #710 is about.
#[test]
fn the_flow_declaration_names_the_same_statuses_and_categories_as_the_registry() {
    let source = fs::read_to_string(
        repository_root()
            .join("packages")
            .join("std")
            .join("index.js"),
    )
    .expect("packages/std/index.js");

    let name_of = |value: serde_json::Value| {
        value
            .as_str()
            .expect("a kebab-case name, not a struct")
            .to_owned()
    };
    let mut statuses = BTreeSet::new();
    let mut categories = BTreeSet::new();
    for module in std_module_descriptors() {
        statuses.insert(name_of(
            serde_json::to_value(module.status).expect("a status serialises"),
        ));
        categories.insert(name_of(
            serde_json::to_value(module.category).expect("a category serialises"),
        ));
    }

    assert_eq!(string_union(&source, "StdStatus"), statuses);
    assert_eq!(string_union(&source, "StdCategory"), categories);
}

/// `hook_descriptors()` names exactly the hooks `@uniflowed/hooks` exports.
///
/// The companion to [`the_registry_names_exactly_what_each_package_exports`],
/// and it exists because that test could not see this drift. The registry's
/// export list for `@uniflowed/hooks` was complete; the hook table beside it
/// was six short, so an editor offering completions was right and `uf inspect`
/// — which reports `hook_descriptors().len()` — was quietly wrong, and nothing
/// compared the two lists that were supposed to agree.
///
/// Hooks and nothing else: the package also exports `RENDER_META`,
/// `RenderProvider` and `browserWindow`, which are a string, a component and a
/// function. A hook is a `use` followed by a capital, which is React's own rule
/// for the name and the same one `uf_rsc`'s scanner applies.
#[test]
fn the_hook_table_names_exactly_the_hooks_the_package_exports() {
    let entry = repository_root()
        .join("packages")
        .join("hooks")
        .join("index.js");
    let source = fs::read_to_string(&entry)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", entry.display()));
    let exported = exported_values(&source)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", entry.display()))
        .unwrap_or_else(|| panic!("{} hands on another package's surface", entry.display()));

    let exports_hooks: Vec<&str> = exported
        .iter()
        .map(String::as_str)
        .filter(|name| is_hook_name(name))
        .collect();
    let described: Vec<String> = hook_descriptors()
        .into_iter()
        .map(|hook| hook.name.to_string())
        .collect();
    let mut table: Vec<&str> = described.iter().map(String::as_str).collect();
    table.sort_unstable();
    table.dedup();

    let missing: Vec<&&str> = exports_hooks
        .iter()
        .filter(|name| !table.contains(name))
        .collect();
    let absent: Vec<&&str> = table
        .iter()
        .filter(|name| !exports_hooks.contains(name))
        .collect();
    assert!(
        missing.is_empty() && absent.is_empty(),
        "hook_descriptors() and @uniflowed/hooks disagree\n  \
         the package exports and the table does not name: {missing:?}\n  \
         the table names and the package does not export: {absent:?}"
    );
    // A floor as well as an equality, so a barrel that stopped parsing into
    // anything cannot make two empty lists agree.
    assert!(
        exports_hooks.len() > 50,
        "the barrel yielded almost no hooks, so this is not checking anything: {}",
        exports_hooks.len()
    );
}

/// React's rule for the name, which is the whole of what makes a hook one.
fn is_hook_name(name: &str) -> bool {
    name.strip_prefix("use")
        .is_some_and(|rest| rest.chars().next().is_some_and(char::is_uppercase))
}

/// The components `@uniflowed/ui` ships, and the parts each one exposes.
///
/// Read from `packages/ui/index.js`, which is where the parts are: a subpath
/// exports `TabsList` and `TabsTab`, and only the barrel says those two are
/// `Tabs.List` and `Tabs.Tab`. Two shapes, because the package has two.
///
/// * A namespace object — `export const Tabs = { Root: TabsRoot, … }` — whose
///   keys are the parts, in the order the module lists them.
/// * A bare value: `Checkbox`, `Progress`, `Separator`, `Switch` and `Toggle`
///   are one component each with nothing to compose, and the table spells that
///   `["Root"]` — the component itself is the part. It is not a special case
///   in the data; `Toggle`'s entry has said so in prose since it was written.
///
/// `toast`, `dismissToast`, `dismissAllToasts` and `updateToast` are exported
/// beside them and are not components. They are told apart the way React tells
/// them apart, and the way JSX requires: a component's name is capitalised.
///
/// Parsed rather than matched, for
/// [`the_registry_names_exactly_what_each_package_exports`]'s reasons — and for
/// one more here. An object literal is nested, so a regular expression would
/// have to decide where it ends, and the first component to hold a nested
/// object in a namespace would silently contribute its inner keys as parts.
fn ui_components_shipped(source: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    use uf_flow::ast::expression::{ExpressionInner, object};
    use uf_flow::ast::pattern;
    use uf_flow::ast::statement::{self, ExportKind};

    let parsed = uf_flow::parse(source).map_err(|error| format!("{error:?}"))?;
    if !parsed.diagnostics.is_empty() {
        return Err(format!("{:?}", parsed.diagnostics));
    }

    let mut shipped = BTreeMap::new();
    for node in parsed.program.statements.iter() {
        let statement::StatementInner::ExportNamedDeclaration { inner, .. } = &**node else {
            continue;
        };
        if inner.export_kind == ExportKind::ExportType {
            continue;
        }

        if let Some(declaration) = &inner.declaration
            && let statement::StatementInner::VariableDeclaration { inner, .. } = &**declaration
        {
            for declarator in inner.declarations.iter() {
                let pattern::Pattern::Identifier { inner, .. } = &declarator.id else {
                    continue;
                };
                let name = inner.name.name.to_string();
                if !starts_capitalised(&name) {
                    continue;
                }
                let Some(init) = &declarator.init else {
                    continue;
                };
                let ExpressionInner::Object { inner, .. } = &**init else {
                    continue;
                };
                let mut parts = Vec::new();
                for property in inner.properties.iter() {
                    if let object::Property::NormalProperty(object::NormalProperty::Init {
                        key: object::Key::Identifier(key),
                        ..
                    }) = property
                    {
                        parts.push(key.name.to_string());
                    }
                }
                shipped.insert(name, parts);
            }
        }

        if let Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) =
            &inner.specifiers
        {
            for specifier in specifiers {
                if specifier.export_kind == ExportKind::ExportType {
                    continue;
                }
                let exported = specifier.exported.as_ref().unwrap_or(&specifier.local);
                let name = exported.name.to_string();
                if starts_capitalised(&name) {
                    shipped.insert(name, vec!["Root".to_owned()]);
                }
            }
        }
    }
    Ok(shipped)
}

/// A component's name, in the one sense JSX enforces.
fn starts_capitalised(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// `AlertDialog` as `alert-dialog`: the subpath a component is imported from.
///
/// A rule rather than a table, and it holds for every one of them — `InputOtp`
/// is `input-otp`, `DatePicker` is `date-picker`, `NavigationMenu` is
/// `navigation-menu`. A component whose module wanted a name this does not
/// produce would fail the check below rather than be quietly exempted, which is
/// the right way round: two spellings of one component is the thing the table
/// was full of.
fn subpath_of(name: &str) -> String {
    let mut subpath = String::with_capacity(name.len() + 2);
    for (index, character) in name.char_indices() {
        if character.is_ascii_uppercase() {
            if index != 0 {
                subpath.push('-');
            }
            subpath.push(character.to_ascii_lowercase());
        } else {
            subpath.push(character);
        }
    }
    subpath
}

/// The `Implemented` entries are exactly the components the package ships.
///
/// # What was wrong
///
/// `crates/uf_lib/src/registry.rs` mirrors every package's *export* list and
/// [`the_registry_names_exactly_what_each_package_exports`] holds it to the real
/// entry point. `ui.rs` mirrors every component's *parts* list and, until this
/// test, nothing held it to anything — so `Combobox` grew `Group` and
/// `GroupLabel` in ubugeeei-prod/uf#558 and the table went on saying seven parts
/// with every test passing. It was found by reading, which is not a check. That
/// is ubugeeei-prod/uf#561.
///
/// One level up, the same hole was bigger: the table named fifty-one components
/// and the package shipped seven of them, and `uf inspect` reported the
/// fifty-one as a project fact (ubugeeei-prod/uf#249). A reader of
/// `uf inspect --json` was told uf had an `AlertDialog` with nine parts.
///
/// # Why the readiness has to be data
///
/// `Command` is the case that decides the shape of this test. It is listed and
/// *deliberately* not implemented — a command palette is a `Combobox` in a
/// `Dialog`, and the entry says so — while `Alert` was listed and simply not
/// written yet, until ubugeeei-prod/uf#298 wrote it. Both were absent from
/// `packages/ui` and only one of them was drift, so the check has to be able to
/// tell them apart, and a comment is not something it can read.
///
/// # Three sources, not two
///
/// The subpaths in `package.json` are what a caller can import, the namespace
/// objects in `index.js` are what they get, and this table is what
/// `uf inspect` says they have. All three are checked against each other,
/// because two of them agreeing is how the third goes stale.
#[test]
fn the_ui_table_names_exactly_what_the_package_ships() {
    let root = repository_root();
    let barrel = root.join("packages/ui/index.js");
    let source = fs::read_to_string(&barrel)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", barrel.display()));
    let shipped = ui_components_shipped(&source)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", barrel.display()));

    let manifest = root.join("packages/ui/package.json");
    let package: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&manifest)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", manifest.display())),
    )
    .unwrap_or_else(|error| panic!("{} does not parse: {error}", manifest.display()));
    let subpaths: BTreeSet<String> = package["exports"]
        .as_object()
        .expect("`exports` is a map")
        .keys()
        .filter(|key| *key != ".")
        .map(|key| key.trim_start_matches("./").to_owned())
        .collect();

    let components = ui_components();
    let implemented: BTreeSet<String> = components
        .iter()
        .filter(|component| component.readiness == UiReadiness::Implemented)
        .map(|component| component.name.to_string())
        .collect();

    // Without this the test would pass on a barrel somebody had emptied, or on
    // a table that had given up and marked everything planned.
    assert!(
        implemented.len() > 20,
        "the walk found almost nothing, so it is not checking anything: {}",
        implemented.len()
    );

    let exported: BTreeSet<String> = shipped.keys().cloned().collect();
    let unlisted: Vec<&str> = exported
        .difference(&implemented)
        .map(String::as_str)
        .collect();
    let unbuilt: Vec<&str> = implemented
        .difference(&exported)
        .map(String::as_str)
        .collect();
    assert!(
        unlisted.is_empty() && unbuilt.is_empty(),
        "`@uniflowed/ui` and the table disagree:\n  \
         exported and not listed as implemented: {unlisted:?}\n  \
         listed as implemented and not exported: {unbuilt:?}"
    );

    // And every one of them is importable on its own, which is the other half
    // of the claim: `sideEffects: false` and one subpath per component is what
    // `@uniflowed/ui` offers instead of a copy step.
    let expected: BTreeSet<String> = implemented.iter().map(|name| subpath_of(name)).collect();
    assert_eq!(
        expected, subpaths,
        "`packages/ui/package.json` and the table disagree about the subpaths"
    );

    // Nothing planned or declined is quietly shipped. This is the direction
    // `Field` failed in for three releases: the module existed, the barrel
    // exported it, and the table had never heard of it.
    let claimed: Vec<&str> = components
        .iter()
        .filter(|component| component.readiness != UiReadiness::Implemented)
        .map(|component| component.name.as_str())
        .filter(|name| shipped.contains_key(*name))
        .collect();
    assert!(
        claimed.is_empty(),
        "the package exports these and the table does not call them implemented: {claimed:?}"
    );
}

/// The parts of an implemented component are the ones its namespace holds.
///
/// The level below [`the_ui_table_names_exactly_what_the_package_ships`], and
/// the one ubugeeei-prod/uf#561 was actually opened about: a component can be
/// in both lists and still describe something else. Two entries did.
/// `Checkbox` was `Root` + `Indicator` and `Switch` was `Root` + `Thumb`, and
/// both ship as one component with no namespace and no second part — which was
/// a real question rather than a typo, because a headless checkbox that draws
/// nothing has nothing for an `Indicator` to be. The entries now say so.
///
/// Compared as sets. The order in the table is editorial — `Toast` leads with
/// `Region` because that is the part the component exists for, and the barrel
/// happens to agree — and a test that pinned it would fail on a rewording of
/// the module rather than on a change to what it exports.
#[test]
fn the_parts_of_an_implemented_component_are_the_ones_it_exports() {
    let root = repository_root();
    let barrel = root.join("packages/ui/index.js");
    let source = fs::read_to_string(&barrel)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", barrel.display()));
    let shipped = ui_components_shipped(&source)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", barrel.display()));

    let mut drifted = Vec::new();
    let mut checked = 0usize;
    for component in ui_components() {
        if component.readiness != UiReadiness::Implemented {
            continue;
        }
        // A missing namespace is the other test's failure, not this one's.
        let Some(parts) = shipped.get(component.name.as_str()) else {
            continue;
        };
        checked += 1;

        let exports: BTreeSet<&str> = parts.iter().map(String::as_str).collect();
        let listed: BTreeSet<&str> = component.parts.iter().map(CompactString::as_str).collect();
        if exports != listed {
            let missing: Vec<&str> = exports.difference(&listed).copied().collect();
            let absent: Vec<&str> = listed.difference(&exports).copied().collect();
            drifted.push(format!(
                "{}\n      exports but the table does not name: {missing:?}\n      the table names but does not export: {absent:?}",
                component.name
            ));
        }
    }

    assert!(
        checked > 20,
        "the walk found almost nothing, so it is not checking anything: {checked}"
    );
    assert!(
        drifted.is_empty(),
        "the table disagrees with {} component(s):\n  - {}",
        drifted.len(),
        drifted.join("\n  - ")
    );
}

/// Every preset style a component names exists, and every preset style is
/// named by a component.
///
/// `preset_style` was a `bool` that [`UiComponent::new`] set to `true` and
/// nothing ever set to `false`, so the table said "styled" about every entry
/// in it while `@uniflowed/stylex/preset` shipped twelve functions. That
/// is the second half of ubugeeei-prod/uf#249, and a smaller bool would not
/// have fixed it: the honest field is which functions, because that is the
/// answer a reader wants and the only version of it a test can check.
///
/// Both directions, and the second is the one that does work. It is what makes
/// the table state ubugeeei-prod/uf#298's decision rather than imply it: a
/// component declined for being only a class list has to name where the class
/// list went, and a preset function that dresses nothing in this table is
/// either a component missing from it or a style nobody can use.
#[test]
fn the_preset_styles_a_component_names_are_exports_of_the_preset() {
    let root = repository_root();
    let preset = root.join("packages/stylex/preset.js");
    let source = fs::read_to_string(&preset)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", preset.display()));
    let exported: BTreeSet<String> = exported_values(&source)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", preset.display()))
        .expect("the preset does not re-export somebody else's surface")
        .into_iter()
        .collect();
    assert!(
        exported.len() > 8,
        "the preset reader found almost nothing: {exported:?}"
    );

    let components = ui_components();
    let claimed: BTreeSet<String> = components
        .iter()
        .flat_map(|component| component.preset_styles.iter())
        .map(|style| style.to_string())
        .collect();

    let invented: Vec<&str> = claimed.difference(&exported).map(String::as_str).collect();
    assert!(
        invented.is_empty(),
        "the table names preset styles `packages/stylex/preset.js` does not export: {invented:?}"
    );

    let orphaned: Vec<&str> = exported.difference(&claimed).map(String::as_str).collect();
    assert!(
        orphaned.is_empty(),
        "`packages/stylex/preset.js` exports these and no component in the table \
         claims them, so either a component is missing or a style is: {orphaned:?}"
    );
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
