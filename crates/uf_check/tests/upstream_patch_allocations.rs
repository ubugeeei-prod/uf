//! What patch 0003 buys, counted rather than described.
//!
//! `tools/upstream/patches/flow/0003-ssa-normal-forms-cost-one-set.patch` is a
//! performance patch, so its other half cannot be a diagnostic: it changes no
//! answer, and every test in `upstream_patches.rs`, `typecheck.rs` and the rest
//! of this directory says so by continuing to pass. What it changes is how many
//! times the SSA builder allocates to compute a normal form, and the only test
//! that can fail without it is one that counts.
//!
//! # What it counts
//!
//! This test's thread, and the check thread `uf_check` checks the module on,
//! which hands its allocations back (`uf_profiler::Handover`). Not the process.
//! The allocator's counters are process-wide, and this test used to be the
//! only one in its binary so that nothing could add to them while it measured
//! — an arrangement that held only for as long as nobody added a second test
//! here. A `uf_profiler::ThreadWindow` does not depend on it: another thread's
//! allocations are not in the figure at all.
//!
//! ```sh
//! cargo test -p uf_check --test upstream_patch_allocations
//! ```

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, check_source};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// How many `if`/`if`/`while` blocks the module below is made of.
///
/// Enough that the per-call cost of forcing a builtin environment — 136,236
/// allocations, and the same whatever is being checked — is a sixth of the
/// figure rather than most of it, and small enough that the module checks
/// twice in a couple of seconds in a debug build.
const BLOCKS: usize = 80;

/// The ceiling this test defends, in allocations.
///
/// What this test itself measures over exactly the module
/// [`merge_heavy_module`] builds: **810,800** allocations with the patch
/// reverted, **697,196** with it applied. The same module through
/// `ubugeeei-prod/uf#678`'s harness — `cargo run --example alloc_report -p
/// uf_check` — reads 810,137 and 696,533; the few hundred between them are the
/// test harness's own, inside the window.
///
/// The ceiling sits between the two with room on both sides — 8% above what the
/// patch costs, 7% below what its absence costs — so it is neither a
/// transcription of today's number nor close enough to the old one to pass by
/// accident.
///
/// It is a ceiling and not an equality on purpose. Anything that makes the
/// checker allocate less keeps this green; the thing it is here to catch is the
/// patch going missing, which puts the figure back above it.
const CEILING: u64 = 750_000;

/// A module whose every statement merges the same four bindings.
///
/// The patch is about `Val::normalize`, which turns a PHI — the SSA builder's
/// word for "this binding was written in more than one place" — into the set of
/// writes that reach it. So the module is written to make PHIs: three control
/// flow joins per block, over four bindings that each read what the others
/// wrote, so the set of writes reaching any one of them grows with the block
/// count rather than staying at two.
///
/// It type checks clean, which the test also asserts. A module that had errors
/// in it would be measuring the error path.
fn merge_heavy_module() -> String {
    let mut source = String::from(concat!(
        "// @flow\n",
        "export function fold(n: number): number {\n",
        "  let a = 0;\n",
        "  let b = 0;\n",
        "  let c = 0;\n",
        "  let d = 0;\n",
    ));
    for block in 0..BLOCKS {
        source.push_str(&format!(
            "  if (n > {}) {{ a = b; }} else {{ b = a; }}\n",
            block
        ));
        source.push_str(&format!(
            "  if (n > {}) {{ c = a + b; }} else {{ d = c + a; }}\n",
            block + 1
        ));
        source.push_str(&format!(
            "  while (n > {}) {{ a = c; c = d; d = b; b = a; }}\n",
            block + 2
        ));
    }
    source.push_str("  return a + b + c + d;\n}\n");
    source
}

fn check(source: &str) -> Vec<uf_check::TypeDiagnostic> {
    check_source(
        Source::new("merge_heavy.js", source),
        &[],
        // Tests must not race the wall clock; a loaded CI box is not a type
        // error, and a check that gave up early would allocate less than one
        // that finished.
        &CheckLimits::default().without_timeout(),
    )
    .expect("the checker runs")
}

#[test]
fn a_normal_form_costs_one_set_and_not_one_per_node() {
    let source = merge_heavy_module();

    // The builtin environment and the master context are memoized for the
    // process and built on first use. Forcing them here keeps a cost that is
    // paid once out of a figure that is about this module.
    let warm = check(&source);
    assert!(
        warm.is_empty(),
        "the module under measurement must type check clean, got {warm:#?}"
    );

    let window = ThreadWindow::open();
    let diagnostics = check(&source);
    let delta = window.close();

    assert!(
        diagnostics.is_empty(),
        "the module under measurement must type check clean, got {diagnostics:#?}"
    );

    assert!(
        delta.allocations > 0,
        "the counting allocator recorded nothing, so this test measured nothing"
    );
    assert!(
        delta.allocations <= CEILING,
        "checking a {} byte module of {BLOCKS} merge blocks took {} allocations, over the \
         {CEILING} ceiling. Either `tools/upstream/patches/flow/0003-*.patch` is not applied — \
         run `tools/upstream/sync.sh` — or something else in the checker started allocating per \
         node. `cargo run --example alloc_report -p uf_check -- <file> --phases` says which \
         phase.",
        source.len(),
        delta.allocations,
    );
}
