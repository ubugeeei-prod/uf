//! Minimal reproductions of checker bugs that are filed and not yet fixed.
//!
//! The ignored tests here assert the *correct* behaviour, so they fail today.
//! `#[ignore]` keeps CI green while keeping the reproduction where the fix
//! will be written rather than in an issue body nobody greps.
//!
//! ```sh
//! cargo test -p uf_check --test known_bugs -- --ignored
//! ```
//!
//! When one is fixed, delete the `#[ignore]`. When all of them are, delete the
//! file — its job is to be empty.
//!
//! The defect below is not `uf`'s. This crate is an embedding: it hands Flow's
//! own inference a source and converts the diagnostics back, and it owns no
//! typing rule that could decide what a pattern binds. Each test therefore
//! records where upstream loses the type, so that the next person to open this
//! file starts from the line rather than from the symptom.
//!
//! It cannot be fixed here, and it must not be fixed in `upstream/flow` either:
//! that submodule is checked out fresh by `tools/upstream/sync.sh`, which has
//! no patch step, so a diff in it is deleted by the next sync. The change
//! belongs upstream in Meta's repository, and these tests are what says when it
//! has arrived.
//!
//! The shape of that change was measured rather than guessed. Letting a
//! `GenericT` reach the match-argument collector is necessary but not
//! sufficient: on its own it makes `match (x: T)` over `T extends "a" | "b"`
//! report `match-not-exhaustive` and both literal arms unused, because the
//! exhaustiveness analysis then has an opaque value where it used to have the
//! bound's members. The analysis has to keep unwrapping the variable and the
//! *value union* has to remember it, so that `ValueUnion::to_type` — which is
//! also what a pattern binding's type comes from — can wrap it back up. That
//! is the same unwrap/filter/re-wrap `predicate_kit` already does for `switch`.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(
        Source::new(path, source),
        &[],
        &CheckLimits::default().without_timeout(),
    )
    .expect("the checker runs")
}

fn assert_clean(path: &str, source: &str) {
    let diagnostics = check(path, source);

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {:#?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text()))
            .collect::<Vec<_>>()
    );
}

/// The control for ubugeeei-prod/uf#205: reading the same tag with `switch`.
///
/// Not ignored, because it passes. It is here so that a change which "fixes"
/// `match` by making both of them lose the type variable cannot pass as a fix.
#[test]
fn switch_over_a_generic_union_refines_the_payload_to_the_type_variable() {
    assert_clean(
        "with_switch.js",
        concat!(
            "// @flow\n",
            "type Box<out T> =\n",
            "  | { readonly kind: \"one\", readonly value: T }\n",
            "  | { readonly kind: \"none\" };\n",
            "\n",
            "export function withSwitch<T, U>(box: Box<T>, change: (T) => U): ?U {\n",
            "  switch (box.kind) {\n",
            "    case \"one\":\n",
            "      return change(box.value);\n",
            "    default:\n",
            "      return null;\n",
            "  }\n",
            "}\n",
        ),
    );
}

/// ubugeeei-prod/uf#205 — a `match` object pattern over a generic container
/// binds the payload as `mixed`, where `switch` on the same tag binds it as
/// the type variable.
///
/// `change(value)` is rejected with "unknown [1] is incompatible with empty
/// [2]": `unknown` is how Flow prints `mixed`, and `empty` is the lower bound
/// of the `T` in `change`'s parameter. Both are what a `GenericT` decays to
/// once its identity has been dropped and only its bounds are left.
///
/// The identity is dropped when the subject is turned into the value union
/// that drives exhaustiveness. `Root::MatchCaseRoot` resolves a case's bindings
/// through `exhaustive::filter_by_pattern_union`
/// (`upstream/flow/rust_port/crates/flow_typing/src/env_resolution.rs:1340`),
/// whose `value_union_builder::of_type_inner`
/// (`.../flow_typing_utils/src/exhaustive.rs:931`) concretizes each member with
/// `ConcretizeForMatchArg`. That concretization has no rule for `GenericT`, so
/// it falls through to the generic-erasing catch-all
/// (`.../flow_typing_flow_js/src/flow_js/dispatch.rs:9439`) — which sits above
/// the arm that collects a match argument, at 10314 — and the collector
/// receives the bound. `ValueUnion::to_type` then rebuilds a type out of what
/// survived, and `T` is not in it.
///
/// `switch` refines through `predicate_kit` instead, and that path *does* have
/// the rule: `concretize_and_run_predicate`
/// (`.../flow_typing_utils/src/predicate_kit.rs:269`) unwraps a `GenericT` at
/// line 288, runs the predicate against the bound, and wraps every surviving
/// type back up in the same generic. That asymmetry is the whole bug.
#[test]
#[ignore = "ubugeeei-prod/uf#205, upstream"]
fn match_over_a_generic_union_binds_the_payload_as_the_type_variable() {
    assert_clean(
        "with_match.js",
        concat!(
            "// @flow\n",
            "type Box<out T> =\n",
            "  | { readonly kind: \"one\", readonly value: T }\n",
            "  | { readonly kind: \"none\" };\n",
            "\n",
            "export function withMatch<T, U>(box: Box<T>, change: (T) => U): ?U {\n",
            "  return match (box) {\n",
            "    {kind: \"one\", value: const value} => change(value),\n",
            "    _ => null,\n",
            "  };\n",
            "}\n",
        ),
    );
}

/// ubugeeei-prod/uf#205, smaller than the report: neither the union nor the
/// object is needed.
///
/// A single-arm `match` that binds the subject itself loses `T` the same way,
/// which is what says the defect is in how a pattern's subject is turned into a
/// value union rather than in object patterns or in tag refinement. Kept
/// alongside the filed reproduction because it is the one to fix against: it
/// reaches the same code with nothing else in the way.
#[test]
#[ignore = "ubugeeei-prod/uf#205, upstream"]
fn match_binding_a_bare_generic_subject_keeps_the_type_variable() {
    assert_clean(
        "bare_generic.js",
        concat!(
            "// @flow\n",
            "export function apply<T, U>(value: T, change: (T) => U): U {\n",
            "  return match (value) {\n",
            "    const bound => change(bound),\n",
            "  };\n",
            "}\n",
        ),
    );
}
