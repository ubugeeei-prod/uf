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
//!
//! # 0002: the elements before a tuple pattern's spread
//!
//! ubugeeei-prod/uf#300, and the half of it that answered wrongly rather than
//! failing. `P extends [infer K, ...infer Rest] ? K : empty` bound `K` as
//! `mixed` — printed as `unknown` — where `P extends [infer K] ? K : empty`
//! bound the element. So a recursion over a tuple, which is what a path type
//! or a "at most one `FormData`" constraint is made of, walked over a list of
//! `unknown`s and answered `true` for every tuple. `packages/router/action.js`
//! records that as the reason `ActionArguments` cannot hold a call to one
//! form.
//!
//! A pattern with a spread in it is not built as a tuple type at all:
//! `flow_js_utils::mk_tuple_type_with_env` turns it into an `EvalT` whose base
//! is the *spread's* type and whose `SpreadTupleType` destructor carries the
//! elements before it, where the same function's no-spread branch builds a
//! real `TupleAT`. Subtyping a tuple against a `TupleAT` decomposes
//! element-wise and hands each `infer` its element; against the destructor
//! nothing decomposes, because evaluating it blocks on the unresolved spread
//! variable — so `solve_conditional_type_targs`' speculative subtyping
//! succeeds against nothing, `K` is pinned with no bounds at all, and
//! `on_missing_bounds` falls back to the bound an unannotated `infer` is
//! declared with.
//!
//! The *spread* variable was already solved, by that destructor's reverse in
//! `implicit_instantiation::t_of_use_t` — it slices the tuple the output was
//! handed with `ArrRestT`. The patch is one addition there: element `i` of
//! that tuple flows into element `i` of the pattern, which is the flow the
//! no-spread branch gets for free.
//!
//! Two limits it deliberately does not lift, so a test below does not claim
//! them: arity is still unchecked, so `[X, Y, ...R]` still matches a
//! one-element tuple rather than taking the false branch; and the other half
//! of #300 — an array literal argument reaching a conditional as an array
//! rather than as the tuple it was written as — stays in
//! `crates/uf_check/tests/known_bugs.rs`, ignored, because a plain `Array<T>`
//! has no elements to hand over and is left alone rather than guessed at.
//!
//! Measured against `flow` itself before it was written, the same way 0001
//! was: `flow-bin@0.330.0` reports `unknown` for every leading `infer` below
//! through `flow check`, in the same words as an unpatched `uf check`.
//!
//! # 0003: an SSA normal form costs one set, not one per node
//!
//! ubugeeei-prod/uf#678, and the only patch here whose test is not in this
//! file. It changes no answer — it is why `uf check` allocates a fifth less to
//! reach the same one — so there is no diagnostic to assert and nothing to add
//! below. Every test in this file passing *is* its correctness evidence, and
//! what would fail without it is a count: `tests/upstream_patch_allocations.rs`,
//! which is its own binary because the counters are in the allocator and a
//! neighbouring test's allocations would land in the figure.

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

/// 0006 — the renderer exports ViewTransition; both React import spellings
/// must admit its real transition classes and reject invalid props.
#[test]
fn react_view_transition_has_typed_named_and_namespace_exports() {
    assert_clean(
        "transition.js",
        "// @flow\nimport * as React from 'react'; import {ViewTransition} from 'react';\nexport component App() { return <ViewTransition name=\"note\" enter={{default: 'appear', navigation: 'slide'}} exit=\"none\"><React.ViewTransition update=\"resize\"><div /></React.ViewTransition></ViewTransition>; }",
    );
    let diagnostics = check(
        "invalid_transition.js",
        "// @flow\nimport * as React from 'react'; import {ViewTransition} from 'react';\nexport component App() { return <ViewTransition name={42} enter={false}><div /></ViewTransition>; }",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
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

/// The three type aliases ubugeeei-prod/uf#300 is about, as a preamble.
///
/// `ReadonlyArray` and `unknown` rather than `$ReadOnlyArray` and `mixed`:
/// the deprecated spellings raise `deprecated-utility` of their own, which
/// would be two diagnostics to read past in a file whose point is the one.
const PATHS: &str = concat!(
    "// @flow\n",
    "type First<P> = P extends [infer K, ...infer Rest] ? K : empty;\n",
    "type Only<P> = P extends [infer K] ? K : empty;\n",
    "declare function only<P extends ReadonlyArray<unknown>>(path: P): Only<P>;\n",
);

/// 0002 — the element before a spread binds the type it was handed.
///
/// The reproduction ubugeeei-prod/uf#300 was filed with, as a *direct*
/// application rather than through a call: `P` is the tuple by construction,
/// so nothing about argument inference is in the way and what is left is the
/// pattern. `First<["address", "city"]>` was `unknown` and is `"address"`.
#[test]
fn the_element_before_a_spread_binds_the_type_it_was_handed() {
    assert_clean(
        "first.js",
        &format!(
            "{PATHS}\
             declare const head: First<[\"address\", \"city\"]>;\n\
             export const address: \"address\" = head;\n"
        ),
    );
}

/// 0002 — the same pattern with no spread in it, which was never broken.
///
/// A "fix" that made a spread pattern bind its element by making every tuple
/// pattern bind `unknown` would pass the test above. This is what stops that,
/// the way `switch_over_a_generic_union_refines_the_payload_to_the_type_variable`
/// does for 0001: `Only<P>` binds correctly today and has to keep doing so.
#[test]
fn a_tuple_pattern_with_no_spread_still_binds_its_element() {
    assert_clean(
        "only.js",
        &format!("{PATHS}export const c: \"address\" = only([\"address\"]);\n"),
    );
}

/// 0002 — two elements before the spread, in the order they were written.
///
/// The elements are read out of the `SpreadTupleType` destructor, which stores
/// them the way `ResolveSpreadT` accumulates them — last one first. A patch
/// that forgot that would pass the one-element test above and quietly hand
/// `[infer X, infer Y, ...]` a swapped pair, which is a wrong answer of
/// exactly the kind this issue is about. So more than one element is asserted,
/// and by name rather than by shape.
#[test]
fn the_elements_before_a_spread_keep_the_order_they_were_written_in() {
    assert_clean(
        "order.js",
        concat!(
            "// @flow\n",
            "type Head<P> = P extends [infer X, infer Y, ...infer R] ? X : empty;\n",
            "type Next<P> = P extends [infer X, infer Y, ...infer R] ? Y : empty;\n",
            "type Rest<P> = P extends [infer X, infer Y, ...infer R] ? R : empty;\n",
            "declare const x: Head<[\"one\", \"two\", \"three\"]>;\n",
            "declare const y: Next<[\"one\", \"two\", \"three\"]>;\n",
            "declare const r: Rest<[\"one\", \"two\", \"three\"]>;\n",
            "export const first: \"one\" = x;\n",
            "export const second: \"two\" = y;\n",
            "export const tail: [\"three\"] = r;\n",
        ),
    );
}

/// 0002 — the recursion the issue is really about, answering rather than
/// falling through.
///
/// `NoForm` is the walk `packages/router/action.js` says it cannot write:
/// at most one `FormData` in an argument list. It did not fail, it answered
/// `true` for `[string, FormData]` — every tuple walked over a head of
/// `unknown`, no head was ever a `FormData`, and the recursion ran out on the
/// empty tuple. A constraint built on that would have reported every signature
/// as fine, which is worse than not having one.
///
/// Both answers are asserted. A patch that made the walk answer `false` for
/// everything would fix the reported case and break the other one.
#[test]
fn a_recursive_walk_over_a_tuple_answers_rather_than_falling_through() {
    assert_clean(
        "no_form.js",
        concat!(
            "// @flow\n",
            "type NoForm<T> = T extends []\n",
            "  ? true\n",
            "  : T extends [infer H, ...infer R]\n",
            "    ? (H extends FormData ? false : NoForm<R>)\n",
            "    : true;\n",
            "declare const two: NoForm<[string, FormData]>;\n",
            "declare const one: NoForm<[FormData]>;\n",
            "declare const none: NoForm<[string, number]>;\n",
            "declare const empty: NoForm<[]>;\n",
            "export const carriesAForm: false = two;\n",
            "export const isAForm: false = one;\n",
            "export const carriesNone: true = none;\n",
            "export const nothingToCarry: true = empty;\n",
        ),
    );
}
