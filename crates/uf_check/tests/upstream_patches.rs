//! What `tools/upstream/patches/flow` buys, and what a dropped patch costs.
//!
//! Every patch in that directory has a test here. It is the other half of the
//! mechanism: the sync makes a patch impossible to skip, and these say what
//! the answers would be if one were. They pass, and a patch that stops being
//! applied turns them red rather than quietly changing what `uf check` says.
//!
//! When a patch lands upstream and the submodule is bumped past it, the patch
//! file goes and **these tests stay** — they are what proves the fix survived
//! the bump.
//!
//! ```sh
//! cargo test -p uf_check --test upstream_patches
//! ```
//!
//! # 0001: a match argument keeps its type variable
//!
//! ubugeeei-prod/uf#205. `match (box) { {kind: "one", value: const value} =>
//! change(value) }` over a `Box<out T>` bound `value` as `mixed` — printed as
//! `unknown` — where `switch` on the same tag bound it as `T`. `change(value)`
//! was then rejected with "unknown [1] is incompatible with empty [2]": both
//! sides of that are a type variable reduced to its bounds.
//!
//! The identity was dropped when the subject became the value union that
//! drives exhaustiveness. `value_union_builder::of_type_inner` concretizes
//! each member with `ConcretizeForMatchArg`, and `handle_generic`
//! (`flow_typing_flow_js/src/flow_js/helpers.rs`) lists three concretization
//! kinds and not that one — so a `GenericT` fell through to the
//! generic-erasing catch-all in `dispatch.rs`, which sits *above* the arm that
//! collects a match argument. `switch` worked, and this is the whole
//! asymmetry, because its collector sits *below* that catch-all and
//! `predicate_kit::concretize_and_run_predicate` unwraps the variable, filters
//! the bound and wraps every survivor back up.
//!
//! Letting a `GenericT` reach the collector is necessary and not sufficient,
//! which is what `match_over_a_bounded_type_variable_is_still_exhaustive`
//! below is here to say: with only the `dispatch.rs` arm, that test reports
//! `match-not-exhaustive` with both literal arms unused, because the
//! exhaustiveness analysis then has an opaque value where it used to have the
//! bound's members. So the patch also teaches the value union to remember the
//! variable it unwrapped, and `ValueUnion::to_type` — which is where a pattern
//! binding's type comes from — to wrap it back up.
//!
//! Both of these were measured against `flow` itself before the patch was
//! written: `flow-bin@0.330.0`, the release `upstream/flow` is pinned to,
//! reported the same diagnostics as `uf check` through `flow focus-check`. The
//! defect is Flow's typing rule rather than an unfaithful port, which is why
//! the patch is a stopgap and the fix belongs in Meta's repository.

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

/// 0001 — the reproduction ubugeeei-prod/uf#205 was filed with.
#[test]
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

/// 0001 — the same defect with nothing else in the way.
///
/// Neither the union nor the object is needed: a single-arm `match` that binds
/// the subject itself lost `T` too, which is what said the defect was in how a
/// pattern's subject becomes a value union rather than in object patterns or
/// in tag refinement. It is the smallest thing the patch has to keep working.
#[test]
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

/// 0001 — the half of the patch that is not the `dispatch.rs` arm.
///
/// A `match` over a type variable bounded by a union of literals has to stay
/// exhaustive. An earlier attempt that only let the `GenericT` reach the
/// collector made this report `match-not-exhaustive` with both literal arms
/// unused, because the exhaustiveness analysis then holds an opaque value
/// where it used to hold the bound's members.
///
/// So this is not a control that happens to pass: it is the assertion that
/// the value union still unwraps the variable to analyse it, and only wraps
/// it back up on the way to a binding.
#[test]
fn match_over_a_bounded_type_variable_is_still_exhaustive() {
    assert_clean(
        "exhaustive.js",
        concat!(
            "// @flow\n",
            "export function name<T extends \"a\" | \"b\">(tag: T): string {\n",
            "  return match (tag) {\n",
            "    \"a\" => \"first\",\n",
            "    \"b\" => \"second\",\n",
            "  };\n",
            "}\n",
        ),
    );
}

/// 0001 — `switch` on the same tag, which was never broken.
///
/// A patch that "fixed" `match` by making both of them lose the type variable
/// would pass every test above. This is what stops that.
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
