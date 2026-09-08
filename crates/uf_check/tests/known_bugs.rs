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
//! The defects below are not `uf`'s. This crate is an embedding: it hands
//! Flow's own inference a source and converts the diagnostics back, and it owns
//! no typing rule that could decide what a pattern binds. Each test therefore
//! records where upstream loses the type, so that the next person to open this
//! file starts from the line rather than from the symptom.
//!
//! # Measured against `flow` itself, not assumed
//!
//! "Not uf's" is the first thing both issues asked to establish, and it is
//! established by running the same source through the `flow` binary. Every
//! reproduction below was checked with `flow-bin@0.330.0` — the release
//! `upstream/flow` is pinned to — using `flow focus-check`, which type checks
//! in the foreground. **`flow check` and `uf check` report the same
//! diagnostics: the same count, the same inferred types, the same words.**
//!
//! That rules out an alternative that would otherwise be open: the Rust port
//! being an unfaithful port. It is faithful here. The typing rule is what is
//! wrong, in Flow, in both of its implementations — so the fix belongs in
//! Meta's repository and nowhere in this one.
//!
//! It cannot be fixed here, and it must not be fixed in `upstream/flow` either:
//! that submodule is checked out fresh by `tools/upstream/sync.sh`, which has
//! no patch step, so a diff in it is deleted by the next sync. The change
//! belongs upstream in Meta's repository, and these tests are what says when it
//! has arrived.
//!
//! # ubugeeei-prod/uf#205: `match` over a generic union
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
//!
//! # ubugeeei-prod/uf#300: a spread in a conditional type's tuple pattern
//!
//! The issue reports one defect in `P extends [infer K, ...infer Rest]`, in two
//! halves. Measuring the halves separately says they are two defects with two
//! causes, and that only one of them is about the spread:
//!
//! | | inferred | should be |
//! | --- | --- | --- |
//! | `First<["address", "city"]>`, applied directly | `unknown` | `"address"` |
//! | `Tail<["address", "city"]>`, applied directly | `["city"]` | `["city"]` — **correct** |
//! | `rest(["address", "city"])`, `P` inferred | `Array<"address" \| "city">` | `["city"]` |
//! | `rest<["address", "city"]>([…])`, `P` written | `["city"]` | `["city"]` — **correct** |
//!
//! So the conditional type's tuple matcher does hold the tuple: pinning `P` by
//! hand is enough to make the trailing `...infer Rest` bind the tail, in order
//! and with its arity. What the issue measured as "the spread loses the tuple"
//! is the *call* losing it — an array literal argument is inferred as an array
//! rather than as a tuple, and `[infer K, ...infer Rest]` against a plain
//! `Array<T>` binds the whole array. The leading `infer K` is the one real
//! defect in the pattern, and it is there whether `P` was inferred or written.
//!
//! **Where the leading `infer K` becomes `unknown`.** A pattern that contains a
//! spread is not built as a tuple type at all:
//! `flow_js_utils::mk_tuple_type_with_env`
//! (`upstream/flow/rust_port/crates/flow_typing_flow_common/src/flow_js_utils.rs:8876`)
//! turns it into an `EvalT` with a `SpreadTupleType` destructor, where the
//! same function's no-spread branch at 8906 builds a real `TupleAT` — which is
//! why `Only<P>` binds correctly and `First<P>` does not. Evaluating that
//! destructor blocks on the unresolved `Rest` tvar
//! (`.../flow_typing_flow_js/src/eval_helpers.rs:219`), so the decomposition
//! that would constrain `K` (same file, 1110) never runs and the speculative
//! subtyping in `instantiation_solver::solve_conditional_type_targs`
//! (`.../flow_typing_flow_js/src/implicit_instantiation.rs:2540`) succeeds
//! against nothing. `K` is then pinned with no bounds at all, so
//! `on_missing_bounds` (same file, 1310) falls back to the tparam's declared
//! bound — and an `infer` with no annotation is declared `mixed`
//! (`.../flow_typing_statement/src/type_annotation.rs:6429`), which is what
//! `unknown` in the diagnostic is.
//!
//! **Where the tail is widened.** The rest is recovered by an ad-hoc reverse in
//! `t_of_use_t` (`implicit_instantiation.rs:556`), which uses only
//! `resolved_rev.len()` as an index into an `ArrRestT`. That rule
//! (`.../flow_typing_flow_js/src/flow_js/dispatch.rs:7652`) returns a plain
//! array unchanged when there is no arity to slice, and for an array literal it
//! slices `tuple_view.elements` correctly but copies `elem_t` verbatim (7669) —
//! so the tail knows it is `["city"]` and still reports the union of every
//! original element. The normalizer then renders it as `Array<elem_t>`, because
//! it only prints an `ArrayAT` with a tuple view as a tuple when the reason is
//! `RRestArrayLit` (`.../flow_typing_ty_normalizer/src/normalizer.rs:1992` and
//! 2044).
//!
//! **The third arm, measured and not reproduced below.** A spread that is not
//! an `infer` — `P extends [infer K, ...ReadonlyArray<unknown>]` — does not
//! match a two-element tuple at all. That one is deliberate rather than lost:
//! the destructor base is concrete, so `ResolveSpreadT` does fire, and
//! spreading a non-tuple array into a tuple type is `ETupleInvalidTypeSpread`
//! (`dispatch.rs:3841`), which is a speculation failure and takes the
//! conditional's false branch. It is not a binding that went wrong, so it is
//! not a binding this file can assert.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(
        Source::new(path, source),
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
#[ignore = "ubugeeei-prod/uf#205, upstream — `flow check` 0.330.0 agrees with uf"]
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
#[ignore = "ubugeeei-prod/uf#205, upstream — `flow check` 0.330.0 agrees with uf"]
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

/// The three type aliases ubugeeei-prod/uf#300 is about, as a preamble.
///
/// `ReadonlyArray` and `unknown` rather than `$ReadOnlyArray` and `mixed`:
/// the deprecated spellings raise `deprecated-utility` of their own, which
/// would be two diagnostics to read past in a file whose point is the one.
const PATHS: &str = concat!(
    "// @flow\n",
    "type First<P> = P extends [infer K, ...infer Rest] ? K : empty;\n",
    "type Tail<P> = P extends [infer K, ...infer Rest] ? Rest : empty;\n",
    "type Only<P> = P extends [infer K] ? K : empty;\n",
    "declare function first<P extends ReadonlyArray<unknown>>(path: P): First<P>;\n",
    "declare function rest<P extends ReadonlyArray<unknown>>(path: P): Tail<P>;\n",
    "declare function only<P extends ReadonlyArray<unknown>>(path: P): Only<P>;\n",
);

/// The control for ubugeeei-prod/uf#300: a tuple pattern with no spread in it,
/// and a spread pattern applied to a tuple written by hand.
///
/// Not ignored, because both pass. They are here because they are what makes
/// the two ignored tests below say something: the conditional type does receive
/// the tuple, and it does bind the tail of one correctly. A "fix" that made
/// `Only` or a pinned `Tail` bind `unknown` too would be a regression that the
/// ignored tests alone could not tell from progress.
#[test]
fn a_tuple_pattern_binds_the_element_and_a_written_tuple_binds_its_tail() {
    assert_clean(
        "only.js",
        &format!("{PATHS}export const c: \"address\" = only([\"address\"]);\n"),
    );
    assert_clean(
        "written.js",
        &format!(
            "{PATHS}\
             const tail = rest<[\"address\", \"city\"]>([\"address\", \"city\"]);\n\
             export const city: \"city\" = tail[0];\n\
             export const size: 1 = tail.length;\n"
        ),
    );
}

/// ubugeeei-prod/uf#300 — the element before a spread binds as `unknown`.
///
/// `First<["address", "city"]>` is `unknown` where `Only<["address"]>` is
/// `"address"`, and adding the spread is the whole difference: the pattern
/// stops being a `TupleAT` and becomes a destructor that never resolves, so
/// `K` is pinned to the bound an unannotated `infer` is declared with, which is
/// `mixed`. See the module documentation for the four lines that happens on.
///
/// Written as a *direct* application rather than through a call, because that
/// is the smaller reproduction: `P` is the tuple by construction, so nothing
/// about argument inference is in the way and what is left is the pattern.
#[test]
#[ignore = "ubugeeei-prod/uf#300, upstream — `flow check` 0.330.0 agrees with uf"]
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

/// ubugeeei-prod/uf#300 — an array literal argument reaches the conditional as
/// an array, so `...infer Rest` binds the whole thing.
///
/// The half of #300 that is *not* about the spread. `rest<["address",
/// "city"]>(…)` binds `["city"]`, so the pattern can do this; it is
/// `rest(["address", "city"])` that cannot, because the literal is inferred as
/// an array rather than as the tuple it is written as, and
/// `[infer K, ...infer Rest]` against a plain array binds the array.
///
/// Both halves of "the tail" are asserted, because the two fail for different
/// reasons and a fix could arrive for one: `tail[0]` is the element type and
/// the order, and `tail.length` is the arity — a tuple's `length` is its own
/// literal, an array's is `number`.
#[test]
#[ignore = "ubugeeei-prod/uf#300, upstream — `flow check` 0.330.0 agrees with uf"]
fn a_tuple_argument_keeps_its_shape_on_the_way_into_a_conditional_type() {
    assert_clean(
        "rest.js",
        &format!(
            "{PATHS}\
             const tail = rest([\"address\", \"city\"]);\n\
             export const city: \"city\" = tail[0];\n\
             export const size: 1 = tail.length;\n"
        ),
    );
}
