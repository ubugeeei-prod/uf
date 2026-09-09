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
//! When one is fixed, delete the `#[ignore]` — and if the fix is a patch to the
//! port, move the test to `crates/uf_check/tests/upstream_patches.rs`, which is
//! where a test that pins a patch lives. When all of them are gone, delete this
//! file: its job is to be empty.
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
//! Meta's repository.
//!
//! Until it lands there it can be carried here, which is the one thing that
//! changed since this file was written: `tools/upstream/patches/flow` is
//! applied to the submodule by `tools/upstream/sync.sh` on every checkout, and
//! that directory's README says how a patch is added, refreshed and — the part
//! that matters — deleted. **ubugeeei-prod/uf#205 left this file that way**:
//! its reproductions pass now, and they live in
//! `crates/uf_check/tests/upstream_patches.rs` beside the patch that fixes
//! them. What is left below is the defect no patch has been written for.
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
