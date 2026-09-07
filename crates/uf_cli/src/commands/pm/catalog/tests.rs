//! What the catalogue is, and what it says when the manifests disagree.

use super::*;
use compact_str::ToCompactString;
use uf_term::{Capabilities, Renderer};

fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

fn declaration(manifest: &str, name: &str, range: &str) -> Declaration {
    Declaration {
        manifest: Utf8PathBuf::from(format!("/p/{manifest}")),
        field: "dependencies",
        name: name.to_compact_string(),
        range: range.to_compact_string(),
    }
}

fn catalogue(declared: &[Declaration]) -> Catalogue {
    entries(Utf8Path::new("/p"), declared)
}

fn drawn(catalogue: &Catalogue) -> String {
    let mut out = String::new();
    render(&plain(), &mut out, catalogue);
    out
}

/// A package one manifest declares has nothing to agree with, so it is not an
/// entry — otherwise "the catalogue" would just be the dependency list.
#[test]
fn an_entry_is_a_package_more_than_one_manifest_declares() {
    let declared = [
        declaration("package.json", "react", "^18.2.0"),
        declaration("packages/ui/package.json", "react", "^18.2.0"),
        declaration("package.json", "vitest", "~1.0.0"),
    ];

    let catalogue = catalogue(&declared);

    assert_eq!(catalogue.entries.len(), 1);
    assert_eq!(catalogue.entries[0].name, "react");
    assert_eq!(catalogue.entries[0].agreed().map(CompactString::as_str), Some("^18.2.0"));
}

/// The bug a catalogue exists to prevent, and the one nothing else reports.
#[test]
fn manifests_that_disagree_are_the_finding() {
    let declared = [
        declaration("package.json", "eslint", "^9.0.0"),
        declaration("packages/ui/package.json", "eslint", "^8.57.0"),
    ];

    let catalogue = catalogue(&declared);
    assert!(catalogue.entries[0].agreed().is_none());

    let out = drawn(&catalogue);
    assert!(out.contains("disagreements"), "{out}");
    assert!(out.contains("differs"), "the shared table hid the finding: {out}");
    assert!(out.contains("^9.0.0") && out.contains("^8.57.0"), "{out}");
    assert!(out.contains("1 package declared at more than one range"), "{out}");
    assert!(out.contains("uf catalog set eslint <range>"), "{out}");
}

/// The rows that need a decision go first.
#[test]
fn a_disagreement_sorts_above_an_agreement() {
    let declared = [
        declaration("package.json", "aaa-agrees", "^1.0.0"),
        declaration("packages/ui/package.json", "aaa-agrees", "^1.0.0"),
        declaration("package.json", "zzz-differs", "^1.0.0"),
        declaration("packages/ui/package.json", "zzz-differs", "^2.0.0"),
    ];

    let out = drawn(&catalogue(&declared));

    let differs = out.find("zzz-differs").expect("the disagreement");
    let agrees = out.find("aaa-agrees").expect("the agreement");
    assert!(differs < agrees, "{out}");
}

#[test]
fn a_workspace_that_agrees_says_so_rather_than_printing_an_empty_section() {
    let declared = [
        declaration("package.json", "react", "^18.2.0"),
        declaration("packages/ui/package.json", "react", "^18.2.0"),
    ];

    let out = drawn(&catalogue(&declared));

    assert!(out.contains("1 package shared, and every manifest agrees"), "{out}");
    assert!(!out.contains("disagreements"), "{out}");
}

/// A single-package project has no catalogue question, and should be told that
/// rather than "nothing is shared".
#[test]
fn one_manifest_is_not_a_workspace_with_nothing_shared() {
    let declared = [
        declaration("package.json", "react", "^18.2.0"),
        declaration("package.json", "vitest", "~1.0.0"),
    ];

    let out = drawn(&catalogue(&declared));

    assert!(out.contains("nothing for a catalogue to keep in step"), "{out}");
}

#[test]
fn a_workspace_that_shares_nothing_says_which_nothing() {
    let declared = [
        declaration("package.json", "react", "^18.2.0"),
        declaration("packages/ui/package.json", "vitest", "~1.0.0"),
    ];

    let out = drawn(&catalogue(&declared));

    assert!(
        out.contains("no package is declared by more than one of this workspace's 2 manifests"),
        "{out}"
    );
}

/// A pnpm project keeps its own catalogue; uf reports it and does not fight it.
#[test]
fn pnpms_own_catalog_is_reported_rather_than_reinterpreted() {
    let declared = [
        declaration("package.json", "react", "catalog:"),
        declaration("packages/ui/package.json", "react", "catalog:"),
        declaration("packages/ui/package.json", "vue", "catalog:react17"),
    ];

    let catalogue = catalogue(&declared);
    assert_eq!(catalogue.pnpm_catalog, 3);

    let out = drawn(&catalogue);
    assert!(out.contains("3 declarations written as pnpm's `catalog:`"), "{out}");
    assert!(out.contains("pnpm resolves itself"), "{out}");
}

#[test]
fn the_root_manifest_is_a_dot_and_a_package_is_its_directory() {
    let root = Utf8Path::new("/p");
    assert_eq!(relative(root, Utf8Path::new("/p/package.json")), ".");
    assert_eq!(
        relative(root, Utf8Path::new("/p/packages/ui/package.json")),
        "packages/ui"
    );
}
