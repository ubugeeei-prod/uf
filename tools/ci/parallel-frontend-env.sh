#!/bin/sh
# Routes the rest of a CI job's Cargo calls through `tools/ci/rustc-wrapper.sh`
# with `UF_RUSTC_FRONTEND_JOBS` threads, by appending both to `$GITHUB_ENV`.
#
# `.github/actions/rust-cache` runs this when a job sets `frontend-jobs` above
# 1. A job that already chose its own wrapper, or checked out a tree without
# this one, keeps its compiler invocation unchanged.
set -eu

case "${UF_RUSTC_FRONTEND_JOBS:-}" in
  '' | 0* | *[!0-9]*)
    echo "parallel-frontend-env: UF_RUSTC_FRONTEND_JOBS must be a positive integer, not '${UF_RUSTC_FRONTEND_JOBS:-}'" >&2
    exit 1
    ;;
esac

wrapper="$GITHUB_WORKSPACE/tools/ci/rustc-wrapper.sh"
if [ -z "${RUSTC_WRAPPER:-}" ] && [ -x "$wrapper" ]; then
  {
    printf 'RUSTC_WRAPPER=%s\n' "$wrapper"
    printf 'UF_RUSTC_FRONTEND_JOBS=%s\n' "$UF_RUSTC_FRONTEND_JOBS"
  } >> "$GITHUB_ENV"
fi
