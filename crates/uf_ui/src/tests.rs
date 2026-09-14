//! The registry held to the repository it is embedded from, and the decisions
//! in the crate's header held to what the code does.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use uf_lib::{UiReadiness, ui_components};

use crate::diff::unified;
use crate::project::{
    AddAction, AddError, CopyState, apply, inspect, package_spec, plan_add, survey,
};
use crate::registry::{
    EMBEDDED, REGISTRY_VERSION, Registry, RegistryError, description, imports, is_component_name,
};
use crate::stamp::{Copy, Stamp, digest, stamped};

/// This checkout, found by walking out of the crate rather than by counting.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the repository")
}

/// Every file name in `registry/ui/`.
fn registry_files() -> BTreeSet<String> {
    let directory = repository_root().join("registry/ui");
    fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{} cannot be listed: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .collect()
}

/// The components the directory holds: every `.js` that is not an example or a
/// test.
fn components_on_disk() -> BTreeSet<String> {
    registry_files()
        .into_iter()
        .filter(|file| !file.ends_with(".example.js") && !file.ends_with(".test.js"))
        .filter_map(|file| file.strip_suffix(".js").map(ToOwned::to_owned))
        .collect()
}

/// A component name for a `@uniflowed/ui` name: `AlertDialog` is `alert-dialog`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (at, character) in name.char_indices() {
        if character.is_ascii_uppercase() {
            if at > 0 {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// A temporary project directory, with `camino` paths.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8Path::from_path(dir.path())
        .expect("a UTF-8 temporary directory")
        .to_path_buf();
    (dir, root)
}

// --- The registry and the repository -----------------------------------------

/// The list `include_str!` embeds is the directory, in both directions.
///
/// A component added to `registry/ui/` and not to `EMBEDDED` would be tested,
/// documented and never shipped; one removed from the directory and still in
/// the list does not compile, but one renamed in both halves of a hurry would.
#[test]
fn the_embedded_list_is_the_directory() {
    let embedded: BTreeSet<String> = EMBEDDED.iter().map(|entry| entry.name.to_owned()).collect();
    assert_eq!(
        embedded,
        components_on_disk(),
        "`registry::EMBEDDED` and `registry/ui/` disagree about which components exist"
    );

    let names: Vec<&str> = EMBEDDED.iter().map(|entry| entry.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "`EMBEDDED` is kept in alphabetical order");
}

/// Every component has the two files the header says it has, and nothing in the
/// directory is an example or a test of a component that is not there.
#[test]
fn every_component_has_an_example_and_a_test_and_nothing_else_does() {
    let files = registry_files();
    let components = components_on_disk();
    let root = repository_root().join("registry/ui");

    for name in &components {
        let example = format!("{name}.example.js");
        let test = format!("{name}.test.js");
        assert!(files.contains(&example), "`{name}` has no {example}");
        assert!(files.contains(&test), "`{name}` has no {test}");

        let example_source = fs::read_to_string(root.join(&example)).expect("the example");
        assert!(
            example_source.contains("export component Example("),
            "{example} does not export the `Example` the reference renders"
        );
        let test_source = fs::read_to_string(root.join(&test)).expect("the test");
        assert!(
            test_source.contains("toHaveNoAxeViolations"),
            "{test} never runs the accessibility audit"
        );
    }

    for file in &files {
        let owner = file
            .strip_suffix(".example.js")
            .or_else(|| file.strip_suffix(".test.js"))
            .or_else(|| file.strip_suffix(".js"));
        match owner {
            Some(owner) => assert!(
                components.contains(owner),
                "registry/ui/{file} belongs to no component"
            ),
            None => panic!("registry/ui/{file} is not a component, an example or a test"),
        }
    }
}

#[test]
fn the_embedded_registry_reads() {
    let registry = Registry::embedded().expect("the embedded registry reads");
    assert_eq!(registry.components().len(), EMBEDDED.len());
    for component in registry.components() {
        assert!(
            !component.description.is_empty(),
            "`{}` has no description",
            component.name
        );
        assert!(is_component_name(component.name), "`{}`", component.name);
    }
}

/// Every module `@uniflowed/ui` ships has a styled component, except the ones
/// named below — and that list may only shrink.
///
/// It is a list with a direction on purpose. ubugeeei-prod/uf#947 is done when
/// it is empty, and a list that could also grow would let a new headless module
/// arrive with no styled half and nobody noticing.
#[test]
fn every_module_the_headless_package_ships_has_a_component() {
    const NOT_YET: &[&str] = &[
        "accordion",
        "alert",
        "avatar",
        "breadcrumb",
        "calendar",
        "carousel",
        "checkbox",
        "collapsible",
        "combobox",
        "context-menu",
        "date-picker",
        "field",
        "input-otp",
        "menu",
        "menubar",
        "navigation-menu",
        "pagination",
        "progress",
        "radio-group",
        "resizable",
        "scroll-area",
        "separator",
        "sidebar",
        "skeleton",
        "slider",
        "switch",
        "table",
        "toast",
        "toggle",
        "toggle-group",
    ];

    let modules = headless_modules();
    let components = components_on_disk();

    for pending in NOT_YET {
        assert!(
            modules.contains(*pending),
            "`{pending}` is waiting for a component, and `@uniflowed/ui` has no such module"
        );
        assert!(
            !components.contains(*pending),
            "`{pending}` has a component now; take it off `NOT_YET`"
        );
    }
    let missing: Vec<&String> = modules
        .iter()
        .filter(|module| !components.contains(*module) && !NOT_YET.contains(&module.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these `@uniflowed/ui` modules have no styled component in `registry/ui/`: {missing:?}"
    );
}

/// A component with no headless module is the styled answer to a component the
/// headless table declined on purpose — `Button` is the example — and nothing
/// else.
#[test]
fn a_component_with_no_module_answers_one_the_headless_package_declined() {
    let modules = headless_modules();
    let declined: BTreeSet<String> = ui_components()
        .iter()
        .filter(|component| component.readiness == UiReadiness::Declined)
        .map(|component| kebab(&component.name))
        .collect();

    for name in components_on_disk() {
        if modules.contains(&name) {
            continue;
        }
        assert!(
            declined.contains(&name),
            "`{name}` has no module in `@uniflowed/ui` and is not a component its table declined"
        );
    }
}

/// The modules `packages/ui` ships, by file name.
fn headless_modules() -> BTreeSet<String> {
    let directory = repository_root().join("packages/ui");
    let modules: BTreeSet<String> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{} cannot be listed: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .filter(|file| file != "index.js" && !file.ends_with(".test.js"))
        .filter_map(|file| file.strip_suffix(".js").map(ToOwned::to_owned))
        .collect();
    assert!(
        modules.len() > 20,
        "`packages/ui` listed almost nothing, so this is not checking anything: {modules:?}"
    );
    modules
}

#[test]
fn the_registry_has_no_cycles() {
    let registry = Registry::embedded().expect("the embedded registry reads");
    for component in registry.components() {
        let mut stack: Vec<&str> = component
            .requires
            .iter()
            .map(|name| name.as_str())
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(next) = stack.pop() {
            assert_ne!(next, component.name, "`{}` requires itself", component.name);
            if seen.insert(next) {
                let dependency = registry.get(next).expect("a required component exists");
                stack.extend(dependency.requires.iter().map(|name| name.as_str()));
            }
        }
    }
}

/// A registry component is built on uf's own packages, which is what lets
/// `uf ui add` pin each one to the uf that wrote the file.
#[test]
fn every_package_a_component_imports_is_one_uf_publishes() {
    let registry = Registry::embedded().expect("the embedded registry reads");
    for component in registry.components() {
        for dependency in &component.dependencies {
            assert!(
                dependency.starts_with("@uniflowed/"),
                "`{}` imports `{dependency}`; a component outside uf's packages needs a decision \
                 about which version `uf ui add` asks for first",
                component.name
            );
        }
    }
}

#[test]
fn no_registry_source_carries_a_stamp_and_each_ends_with_one_newline() {
    for entry in EMBEDDED {
        assert!(
            Copy::read(entry.source).stamp.is_none(),
            "registry/ui/{}.js carries a stamp, which only a copy should",
            entry.name
        );
        assert!(
            entry.source.ends_with('\n') && !entry.source.ends_with("\n\n"),
            "registry/ui/{}.js does not end with exactly one newline",
            entry.name
        );
    }
}

/// The import scanner reads every registry source the way it is written: each
/// line outside a comment that names a module is an import it found.
#[test]
fn the_import_scanner_misses_nothing_in_the_registry() {
    for entry in EMBEDDED {
        let written = entry
            .source
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//") && !line.starts_with('*'))
            .filter(|line| line.contains(" from \"") || line.starts_with("import \""))
            .count();
        assert_eq!(
            imports(entry.source).len(),
            written,
            "registry/ui/{}.js has an import the scanner does not read",
            entry.name
        );
    }
}

#[test]
fn dialog_needs_button_and_the_packages_it_imports() {
    let registry = Registry::embedded().expect("the embedded registry reads");
    let dialog = registry.get("dialog").expect("a dialog");
    assert_eq!(dialog.requires, ["button"]);
    assert_eq!(
        dialog.dependencies,
        ["@uniflowed/react", "@uniflowed/stylex", "@uniflowed/ui"]
    );

    let order: Vec<&str> = registry
        .closure(&["dialog", "button"])
        .expect("both exist")
        .iter()
        .map(|component| component.name)
        .collect();
    assert_eq!(order, ["button", "dialog"], "requirements come first, once");
}

// --- Reading a source --------------------------------------------------------

#[test]
fn imports_are_read_in_every_shape_the_formatter_writes() {
    let source = r#""use client";
// @flow
//
// Thing: a thing.
//
// import { Ignored } from "a-comment";

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import {
  DialogRoot,
  DialogTrigger,
} from "@uniflowed/ui/dialog";
import "./side-effect.js";
export { Button } from "./button.js";

const text = <p>taken from "the manual"</p>;
"#;
    assert_eq!(
        imports(source),
        [
            "@uniflowed/react",
            "@uniflowed/stylex",
            "@uniflowed/ui/dialog",
            "./side-effect.js",
            "./button.js",
        ]
    );
}

#[test]
fn a_description_is_the_first_header_paragraph_after_its_title() {
    assert_eq!(
        description(
            "\"use client\";\n// @flow\n//\n// Dialog: a modal dialog.\n//\n// Later: not this.\n"
        )
        .as_deref(),
        Some("a modal dialog.")
    );
    assert_eq!(
        description("// @flow\n// Button: a button.\n").as_deref(),
        Some("a button.")
    );
    // Wrapped at the header's width, and ended by the paragraph break.
    assert_eq!(
        description(
            "// @flow\n//\n// Sheet: a panel on one edge,\n// for filters.\n//\n// More.\n"
        )
        .as_deref(),
        Some("a panel on one edge, for filters.")
    );
    assert_eq!(description("// @flow\n\nimport x from \"y\";\n"), None);
    assert_eq!(description("// @flow\n// No title here.\n"), None);
}

#[test]
fn a_source_that_cannot_be_read_says_why() {
    assert_eq!(
        Registry::from_sources([("lonely", "// @flow\nexport const x = 1;\n")]),
        Err(RegistryError::NoDescription {
            component: "lonely"
        })
    );
    assert!(matches!(
        Registry::from_sources([(
            "climber",
            "// @flow\n// Climber: reaches up.\nimport { x } from \"../outside.js\";\n"
        )]),
        Err(RegistryError::UnresolvedImport {
            component: "climber",
            ..
        })
    ));
    assert!(matches!(
        Registry::from_sources([(
            "needy",
            "// @flow\n// Needy: needs another.\nimport { x } from \"./absent.js\";\n"
        )]),
        Err(RegistryError::MissingComponent {
            component: "needy",
            ..
        })
    ));
}

#[test]
fn a_package_is_named_by_its_package_name() {
    let registry = Registry::from_sources([(
        "thing",
        "// @flow\n// Thing: a thing.\nimport a from \"@uniflowed/ui/dialog\";\nimport b from \"left-pad\";\nimport c from \"@scope/pkg/deep/file.js\";\nimport d from \"@uniflowed/ui\";\n",
    )])
    .expect("reads");
    assert_eq!(
        registry.components()[0].dependencies,
        ["@scope/pkg", "@uniflowed/ui", "left-pad"]
    );
}

#[test]
fn a_component_name_is_lower_case_words_and_hyphens() {
    for good in ["button", "alert-dialog", "input-otp", "h1"] {
        assert!(is_component_name(good), "{good}");
    }
    for bad in [
        "",
        "Button",
        "-lead",
        "trail-",
        "two--dashes",
        "dot.js",
        "under_score",
    ] {
        assert!(!is_component_name(bad), "{bad}");
    }
}

#[test]
fn an_unknown_name_is_named_back() {
    let registry = Registry::embedded().expect("reads");
    let error = registry
        .closure(&["button", "buton"])
        .expect_err("buton is not a component");
    assert_eq!(error.name, "buton");
}

// --- The stamp ---------------------------------------------------------------

#[test]
fn a_stamped_file_reads_back_as_its_source_untouched() {
    let source = "// @flow\n// Thing: a thing.\nexport const answer = 42;\n";
    let written = stamped("thing", source);
    assert!(written.starts_with(source));
    assert!(written.ends_with(".\n"));

    let copy = Copy::read(&written);
    assert_eq!(copy.content, source);
    assert!(copy.is_untouched());
    let stamp = copy.stamp.expect("a stamp");
    assert_eq!(stamp.component, "thing");
    assert_eq!(stamp.version, REGISTRY_VERSION);
    assert_eq!(stamp.digest, digest(source));
    assert_eq!(Stamp::parse(&stamp.line()), Some(stamp));
}

/// Git on Windows checks a file out with `\r\n` by default. That is not an edit.
#[test]
fn a_checkout_with_windows_line_endings_is_still_untouched() {
    let source = "// @flow\n// Thing: a thing.\nexport const answer = 42;\n";
    let converted = stamped("thing", source).replace('\n', "\r\n");
    assert!(Copy::read(&converted).is_untouched());
}

#[test]
fn an_edited_copy_is_not_untouched_wherever_the_edit_is() {
    let source = "// @flow\n// Thing: a thing.\nexport const answer = 42;\n";
    let written = stamped("thing", source);

    let changed = written.replace("42", "43");
    assert!(!Copy::read(&changed).is_untouched());

    // Code added below the stamp: still a copy, and an edited one.
    let appended = format!("{written}export const more = 1;\n");
    let copy = Copy::read(&appended);
    assert!(copy.stamp.is_some());
    assert!(!copy.is_untouched());
}

#[test]
fn a_line_that_only_looks_like_a_stamp_is_not_one() {
    let good = Stamp::new("thing", "x").line();
    assert!(Stamp::parse(&good).is_some());

    let short = good.replace(&digest("x").to_string(), "abc123");
    let shouting = good.replace(&digest("x").to_string(), &digest("x").to_uppercase());
    for line in [
        short,
        shouting,
        good.replace("thing", "Thing"),
        good.trim_end_matches('.').to_owned(),
        good.replace("from uf ", "from uf \t"),
        format!("  {good}"),
    ] {
        assert_eq!(Stamp::parse(&line), None, "{line}");
    }
}

// --- A project ---------------------------------------------------------------

/// A registry of three components, `panel` needing `knob`, with sources as
/// short as a test can make them.
fn tiny_registry(knob: &'static str) -> Registry {
    Registry::from_sources([
        ("knob", knob),
        (
            "panel",
            "// @flow\n// Panel: holds a knob.\nimport { Knob } from \"./knob.js\";\nimport * as React from \"@uniflowed/react\";\n",
        ),
        (
            "plain",
            "// @flow\n// Plain: needs nothing.\nimport * as UI from \"@uniflowed/ui/tabs\";\n",
        ),
    ])
    .expect("the tiny registry reads")
}

const KNOB: &str = "// @flow\n// Knob: turns.\nexport const knob = 1;\n";
const KNOB_LATER: &str = "// @flow\n// Knob: turns, and clicks.\nexport const knob = 2;\n";

#[test]
fn adding_writes_the_component_and_what_it_needs_and_names_the_missing_packages() {
    let (_guard, root) = project();
    fs::write(
        root.join("package.json"),
        "{ \"dependencies\": { \"@uniflowed/stylex\": \"1\" } }",
    )
    .expect("a manifest");
    let directory = root.join("app/components/ui");
    let registry = tiny_registry(KNOB);

    let plan = plan_add(&root, &directory, &registry, &["panel"], false).expect("a plan");
    let actions: Vec<(&str, bool, &AddAction)> = plan
        .steps
        .iter()
        .map(|step| (step.component, step.requested, &step.action))
        .collect();
    assert_eq!(
        actions,
        [
            ("knob", false, &AddAction::Create),
            ("panel", true, &AddAction::Create)
        ]
    );
    assert_eq!(plan.packages, ["@uniflowed/react"]);

    apply(&plan).expect("written");
    let knob = registry.get("knob").expect("knob");
    assert!(matches!(
        inspect(&directory, knob).expect("inspect").state,
        CopyState::Current { .. }
    ));
    assert_eq!(
        fs::read_to_string(directory.join("knob.js")).expect("knob.js"),
        stamped("knob", KNOB)
    );
}

#[test]
fn adding_again_changes_nothing() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    let registry = tiny_registry(KNOB);
    apply(&plan_add(&root, &directory, &registry, &["panel"], false).expect("plan"))
        .expect("written");

    let again = plan_add(&root, &directory, &registry, &["panel"], false).expect("plan");
    assert!(!again.writes());
    assert!(
        again
            .steps
            .iter()
            .all(|step| step.action == AddAction::Unchanged)
    );
}

/// A copy nobody touched is brought up to this registry's version, named
/// or not, because nothing of anybody's is lost by it.
#[test]
fn an_untouched_copy_of_an_older_registry_is_updated() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    apply(&plan_add(&root, &directory, &tiny_registry(KNOB), &["panel"], false).expect("plan"))
        .expect("written");

    let later = tiny_registry(KNOB_LATER);
    let knob = later.get("knob").expect("knob");
    assert!(matches!(
        inspect(&directory, knob).expect("inspect").state,
        CopyState::Outdated { .. }
    ));

    let plan = plan_add(&root, &directory, &later, &["panel"], false).expect("plan");
    assert!(matches!(plan.steps[0].action, AddAction::Update { .. }));
    apply(&plan).expect("written");
    assert_eq!(
        fs::read_to_string(directory.join("knob.js")).expect("knob.js"),
        stamped("knob", KNOB_LATER)
    );
}

/// An edited copy of a component that was asked for is refused by name, and
/// the refusal writes nothing — including the files that had no conflict.
#[test]
fn an_edited_copy_that_was_asked_for_is_refused_and_nothing_is_written() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    let registry = tiny_registry(KNOB);
    apply(&plan_add(&root, &directory, &registry, &["knob"], false).expect("plan"))
        .expect("written");
    let path = directory.join("knob.js");
    let edited = fs::read_to_string(&path)
        .expect("knob.js")
        .replace("= 1", "= 7");
    fs::write(&path, &edited).expect("an edit");

    let error = plan_add(&root, &directory, &registry, &["panel", "knob"], false)
        .expect_err("the edit is protected");
    match error {
        AddError::Conflicts(conflicts) => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].component, "knob");
            assert_eq!(conflicts[0].path, path);
            assert!(conflicts[0].edited);
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(
        !directory.join("panel.js").exists(),
        "a refused run wrote a file"
    );
    assert_eq!(fs::read_to_string(&path).expect("knob.js"), edited);
}

#[test]
fn overwrite_replaces_an_edited_copy_that_was_asked_for() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    let registry = tiny_registry(KNOB);
    apply(&plan_add(&root, &directory, &registry, &["knob"], false).expect("plan"))
        .expect("written");
    let path = directory.join("knob.js");
    fs::write(&path, "// mine now\n").expect("a foreign file");

    let plan = plan_add(&root, &directory, &registry, &["knob"], true).expect("a plan");
    assert_eq!(plan.steps[0].action, AddAction::Replace);
    apply(&plan).expect("written");
    assert_eq!(
        fs::read_to_string(&path).expect("knob.js"),
        stamped("knob", KNOB)
    );
}

/// An edited file that was only *needed* is kept: it still satisfies the import.
#[test]
fn an_edited_requirement_is_kept_rather_than_refused() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    fs::create_dir_all(&directory).expect("the directory");
    fs::write(
        directory.join("knob.js"),
        "// @flow\nexport const knob = \"mine\";\n",
    )
    .expect("a knob of the project's own");

    let plan = plan_add(&root, &directory, &tiny_registry(KNOB), &["panel"], false)
        .expect("a requirement does not conflict");
    assert_eq!(plan.steps[0].action, AddAction::Keep { edited: false });
    assert_eq!(plan.steps[1].action, AddAction::Create);
}

#[test]
fn a_copy_stamped_as_another_component_is_not_this_one() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    fs::create_dir_all(&directory).expect("the directory");
    fs::write(directory.join("knob.js"), stamped("plain", KNOB)).expect("a mislabelled copy");

    let registry = tiny_registry(KNOB);
    let knob = registry.get("knob").expect("knob");
    assert_eq!(
        inspect(&directory, knob).expect("inspect").state,
        CopyState::Foreign
    );
}

#[test]
fn an_edited_copy_says_whether_the_registry_moved_too() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    apply(&plan_add(&root, &directory, &tiny_registry(KNOB), &["knob"], false).expect("plan"))
        .expect("written");
    let path = directory.join("knob.js");
    let edited = fs::read_to_string(&path)
        .expect("knob.js")
        .replace("= 1", "= 7");
    fs::write(&path, edited).expect("an edit");

    let same = tiny_registry(KNOB);
    let later = tiny_registry(KNOB_LATER);
    assert!(matches!(
        inspect(&directory, same.get("knob").expect("knob"))
            .expect("inspect")
            .state,
        CopyState::Edited {
            registry_moved: false,
            ..
        }
    ));
    assert!(matches!(
        inspect(&directory, later.get("knob").expect("knob"))
            .expect("inspect")
            .state,
        CopyState::Edited {
            registry_moved: true,
            ..
        }
    ));
}

#[test]
fn a_survey_covers_every_component_whether_or_not_it_was_added() {
    let (_guard, root) = project();
    let directory = root.join("app/components/ui");
    let registry = tiny_registry(KNOB);
    apply(&plan_add(&root, &directory, &registry, &["plain"], false).expect("plan"))
        .expect("written");

    let states: Vec<(&str, &str)> = survey(&directory, &registry)
        .expect("survey")
        .iter()
        .map(|copy| (copy.component, copy.state.as_str()))
        .collect();
    assert_eq!(
        states,
        [
            ("knob", "missing"),
            ("panel", "missing"),
            ("plain", "current")
        ]
    );
}

#[test]
fn a_manifest_that_is_not_json_is_reported_with_its_path() {
    let (_guard, root) = project();
    fs::write(root.join("package.json"), "not json").expect("a broken manifest");
    let directory = root.join("app/components/ui");
    let error = plan_add(&root, &directory, &tiny_registry(KNOB), &["plain"], false)
        .expect_err("the manifest cannot be read");
    assert!(error.to_string().contains("package.json"), "{error}");
}

#[test]
fn a_uniflowed_package_is_asked_for_at_this_version() {
    assert_eq!(
        package_spec("@uniflowed/ui"),
        format!("@uniflowed/ui@{REGISTRY_VERSION}")
    );
    assert_eq!(package_spec("left-pad"), "left-pad");
}

// --- The diff ----------------------------------------------------------------

#[test]
fn a_diff_is_the_copy_against_the_registry() {
    assert_eq!(unified("registry", "copy", "a\nb\n", "a\nb\n"), None);

    let diff = unified(
        "registry/knob.js",
        "app/components/ui/knob.js",
        "a\nb\nc\n",
        "a\nB\nc\n",
    )
    .expect("they differ");
    assert!(diff.contains("--- registry/knob.js"), "{diff}");
    assert!(diff.contains("+++ app/components/ui/knob.js"), "{diff}");
    assert!(diff.contains("-b\n"), "{diff}");
    assert!(diff.contains("+B\n"), "{diff}");
}
