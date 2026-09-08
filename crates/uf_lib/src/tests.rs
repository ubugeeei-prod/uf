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
    // presentational, which is ubugeeei-prod/uf#298's list.
    for planned in ["Alert", "Avatar", "Breadcrumb", "Separator", "Skeleton"] {
        assert_eq!(
            named(planned).readiness,
            UiReadiness::Planned,
            "{planned} has a decision in it and nobody has written it yet"
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

    let planned = components
        .iter()
        .filter(|component| component.readiness == UiReadiness::Planned)
        .count();
    assert!(planned > 3, "the roadmap has been emptied: {planned}");
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

/// The components `@uniflowed/ui` ships, and the parts each one exposes.
///
/// Read from `packages/ui/index.js`, which is where the parts are: a subpath
/// exports `TabsList` and `TabsTab`, and only the barrel says those two are
/// `Tabs.List` and `Tabs.Tab`. Two shapes, because the package has two.
///
/// * A namespace object — `export const Tabs = { Root: TabsRoot, … }` — whose
///   keys are the parts, in the order the module lists them.
/// * A bare value: `Checkbox`, `Progress`, `Switch` and `Toggle` are one
///   component each with nothing to compose, and the table spells that
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
/// `Dialog`, and the entry says so — while `Alert` is listed and simply not
/// written yet. Both are absent from `packages/ui` and only one of them is
/// drift, so the check has to be able to tell them apart, and a comment is not
/// something it can read.
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
