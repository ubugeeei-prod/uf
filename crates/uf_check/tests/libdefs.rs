//! A project's own library definitions, and what they are worth.
//!
//! `.flowconfig`'s `[libs]` is where a project says what an untyped dependency
//! exports and what its globals are. Until uf read it, every one of those
//! names was an `any`-typed value: `import type { EdgeProps } from
//! "@xyflow/react"` was not a type import at all, and each use of the type was
//! an error. ubugeeei-prod/uf#480 measured 506 such errors against
//! `flow check`'s 20 on one tree.
//!
//! Each test here has the *same* batch checked twice — once with the libdefs
//! and once without — because the assertion that matters is the difference.
//! A test that only checked with them could pass against a checker that
//! declared everything `any`, which is the bug.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckError, CheckLimits, Source, check_sources};

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

/// A `flow-typed` directory of the shape a project that depends on an untyped
/// package has.
const PACKAGES: &str = r#"declare module "@xyflow/react" {
  declare export type EdgeProps = { readonly id: string };
  declare export function useReactFlow(): EdgeProps;
}
"#;

/// The other half of what `[libs]` is for: globals.
const GLOBALS: &str = "declare type UmlEdgeKind = \"assoc\" | \"inherit\";\n";

/// A file that uses a libdef type *as a type*, which is the shape #480 is
/// about.
const USES_A_LIBDEF_TYPE: &str = r#"// @flow
import type { EdgeProps } from "@xyflow/react";

export function label(props: EdgeProps): string {
  return props.id;
}
"#;

const USES_A_LIBDEF_GLOBAL: &str = r#"// @flow
export function describe(kind: UmlEdgeKind): string {
  return kind;
}
"#;

fn codes(sources: &[Source<'_>], libs: &[Source<'_>]) -> Vec<String> {
    check_sources(sources, libs, &limits())
        .expect("the checker runs")
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.unwrap_or("<none>").to_owned())
        .collect()
}

fn inferred_codes(sources: &[Source<'_>], libs: &[Source<'_>]) -> Vec<String> {
    check_sources(sources, libs, &limits())
        .expect("the checker runs")
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind != uf_check::DiagnosticKind::Parse)
        .map(|diagnostic| diagnostic.code.unwrap_or("<none>").to_owned())
        .collect()
}

#[test]
fn a_type_a_libdef_declares_is_a_type_and_not_an_any_typed_value() {
    let sources = [Source::new("src/Edge.js", USES_A_LIBDEF_TYPE)];
    let libs = [Source::new("flow-typed/packages.js", PACKAGES)];

    // Without the libdefs, exactly #480's error, once per use.
    assert!(
        codes(&sources, &[]).contains(&"value-as-type".to_owned()),
        "the fixture must fail without its library definitions, or this test proves nothing"
    );

    assert_eq!(codes(&sources, &libs), Vec::<String>::new());
}

#[test]
fn a_libdef_type_is_the_type_it_declares_and_not_any() {
    // The half a libdef read as `any` would still pass: the declared type has
    // to *reject* something.
    let sources = [Source::new(
        "src/Edge.js",
        "// @flow\nimport type { EdgeProps } from \"@xyflow/react\";\n\
         export const wrong: EdgeProps = { id: 1 };\n",
    )];
    let libs = [Source::new("flow-typed/packages.js", PACKAGES)];

    assert_eq!(codes(&sources, &libs), ["incompatible-type"]);
}

#[test]
fn a_global_a_libdef_declares_resolves() {
    let sources = [Source::new("src/Kind.js", USES_A_LIBDEF_GLOBAL)];
    let libs = [Source::new("flow-typed/globals.js", GLOBALS)];

    assert_eq!(codes(&sources, &[]), ["cannot-resolve-name"]);
    assert_eq!(codes(&sources, &libs), Vec::<String>::new());
}

#[test]
fn a_module_a_libdef_declares_is_not_reported_as_untyped() {
    let sources = [Source::new("src/Edge.js", USES_A_LIBDEF_TYPE)];
    let libs = [Source::new("flow-typed/packages.js", PACKAGES)];

    let without = check_sources(&sources, &[], &limits()).expect("the checker runs");
    assert_eq!(without.untyped_modules, ["@xyflow/react"]);

    let with = check_sources(&sources, &libs, &limits()).expect("the checker runs");
    assert!(
        with.untyped_modules.is_empty(),
        "a module the project declared is not a hole: {:?}",
        with.untyped_modules
    );
}

#[test]
fn a_libdef_declare_module_wins_over_a_project_manifest_with_the_same_name() {
    // A vendored package or wrapper can publish the same name a libdef
    // declares. In that case the declaration is the type surface the project
    // asked for; the manifest should not shadow it and turn namespace members
    // back into missing properties or any-typed values.
    let sources = [
        Source::new(
            "src/probe.js",
            "// @flow\nimport * as Editor from \"editor-pkg\";\n\
             export const provider: Editor.languages.FoldingRangeProvider = { kind: \"x\" };\n",
        ),
        Source::new(
            "vendor/editor-pkg/index.js",
            "// @flow\nexport const anything: string = \"runtime\";\n",
        ),
        Source::new(
            "vendor/editor-pkg/package.json",
            r#"{ "name": "editor-pkg", "main": "index.js" }"#,
        ),
    ];
    let libs = [Source::new(
        "flow-typed/editor.js",
        "// @flow\ndeclare module \"editor-pkg\" {\n  declare export namespace languages {\n    declare type FoldingRangeProvider = { kind: string };\n  }\n}\n",
    )];

    let without = inferred_codes(&sources, &[]);
    assert!(
        without.iter().any(|code| code == "prop-missing")
            && without.iter().any(|code| code == "value-as-type"),
        "the fixture must reproduce the manifest shadow before the libdef wins: {without:?}"
    );

    assert_eq!(inferred_codes(&sources, &libs), Vec::<String>::new());
}

#[test]
fn a_later_libdef_shadows_an_earlier_one() {
    // Merge order is what `[libs]` declaration order means, and it is the
    // reason the caller may not sort the list for tidiness.
    let sources = [Source::new(
        "src/Kind.js",
        "// @flow\nexport const kind: UmlEdgeKind = 7;\n",
    )];
    let first = Source::new("flow-typed/a.js", "declare type UmlEdgeKind = number;\n");
    let second = Source::new("flow-typed/b.js", "declare type UmlEdgeKind = string;\n");

    assert_eq!(codes(&sources, &[first, second]), ["incompatible-type"]);
    assert_eq!(codes(&sources, &[second, first]), Vec::<String>::new());
}

#[test]
fn one_project_s_library_definitions_do_not_leak_into_another_s() {
    // The merged environment is memoised for the whole process, so this is the
    // test that the memo is keyed rather than shared: two batches, one file
    // each, checked in one process with different libdefs.
    let sources = [Source::new("src/Kind.js", USES_A_LIBDEF_GLOBAL)];
    let declared = [Source::new("flow-typed/globals.js", GLOBALS)];

    assert_eq!(codes(&sources, &declared), Vec::<String>::new());
    assert_eq!(codes(&sources, &[]), ["cannot-resolve-name"]);
    assert_eq!(codes(&sources, &declared), Vec::<String>::new());
}

#[test]
fn a_library_definition_that_does_not_parse_is_reported_against_itself() {
    let sources = [Source::new("src/app.js", "// @flow\nexport const a = 1;\n")];
    let libs = [Source::new(
        "flow-typed/broken.js",
        "declare type Broken = ;\n",
    )];

    let error = check_sources(&sources, &libs, &limits()).expect_err("the libdef is not Flow");

    // Named, and named as the project's — not as "Flow's builtins failed to
    // load", which would send a reader to the submodule for a file they wrote.
    match error {
        CheckError::LibDef { path, .. } => assert_eq!(path, "flow-typed/broken.js"),
        other => panic!("expected a libdef failure, got {other}"),
    }
}
