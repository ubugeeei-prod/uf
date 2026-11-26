#!/bin/sh
# Cargo's `RUSTC_WRAPPER` in the dev shell and in the Rust CI jobs.
#
# Cargo runs: rustc-wrapper.sh <rustc or clippy-driver> [rustc] <arguments>
#
# Two accelerators, both on the pinned nightly and both invisible to Cargo's
# fingerprints, so existing target directories and sticky disks stay warm:
#
# - `UF_RUSTC_FRONTEND_JOBS=N` (N > 1) compiles each crate built from this
#   checkout with rustc's parallel frontend, `--jobs-frontend=N`. That is the
#   workspace and the Flow port under `upstream/`, which are the crates an edit
#   recompiles. Registry dependencies, build-script probes and Cargo's `-vV`
#   queries reach rustc unchanged: dependencies already compile side by side,
#   so extra frontend threads there only spend CPU.
# - `UF_KACHE=<path to kache>` hands every call to kache, which shares
#   compiled crates across every worktree and target directory on the machine.
#   The dev shell sets it; CI does not, because kache rewrites the paths it
#   hashes and its output is not byte-identical to an uncached build.
#
# See "Build speed" in `docs/rust-performance.md`.
case "${UF_RUSTC_FRONTEND_JOBS:-1}" in
  '' | 0 | 1) ;;
  *)
    workspace="${0%/tools/ci/rustc-wrapper.sh}"
    case "${CARGO_MANIFEST_DIR:-}" in
      "$workspace"/*)
        # A build-script probe inherits its package's environment but names a
        # crate of its own, so only the package's own crate qualifies.
        previous=
        for argument in "$@"; do
          if [ "$previous" = --crate-name ]; then
            if [ "$argument" = "${CARGO_CRATE_NAME:-}" ]; then
              set -- "$@" -Zunstable-options "--jobs-frontend=$UF_RUSTC_FRONTEND_JOBS"
            fi
            break
          fi
          previous="$argument"
        done
        ;;
    esac
    ;;
esac
if [ -n "${UF_KACHE:-}" ]; then
  exec "$UF_KACHE" "$@"
fi
exec "$@"
