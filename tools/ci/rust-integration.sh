#!/bin/sh
# The Rust integration tests a pull request can break, run on that pull request.
#
#   RUST_TESTS=<scope> RUST_VITE_TESTS=<true|false> tools/ci/rust-integration.sh
#
# <scope> is `rust_tests` from `tools/ci/change-scope.cjs`: "workspace" for
# every crate's `tests/`, or the crates a change touched, space-separated, which
# always include `uf_cli`. Its tests hold what no one crate owns — every command
# is classified for `uf explain`, has a completion, answers `--help` — and a
# change to any crate can break them. Unit tests ran in the step before this.
#
# These used to run only in the release queue. #1417 added `uf sqlc` without
# describing it to `uf explain`, merged green, and the release PR for uf@0.2.0
# was the first run to see `explain_describes_every_command_that_delegates`
# fail (#1434).
#
# One exception: `uf_cli`'s `vite.rs` builds and serves a fixture per case, and
# at six minutes is most of the whole suite's time. It runs when the change
# reaches what it tests — the adapters, the router, the build, which is the
# deploy matrix's scope (`RUST_VITE_TESTS`) — and in the release queue always.
#
# `--no-fail-fast` throughout: `cargo test` otherwise stops at the first test
# binary that fails, and hides what the ones after it would have said until the
# next run.
set -eu

scope="${RUST_TESTS:-}"
vite="${RUST_VITE_TESTS:-true}"
if [ -z "$scope" ]; then
  echo "rust-integration: no crate this change touches has integration tests to run"
  exit 0
fi

cd "$(dirname "$0")/../.."

# Crates by directory: every one of them is named after its directory.
if [ "$scope" = "workspace" ]; then
  crates=$(cd crates && ls)
else
  crates=$scope
fi

# Test targets by name, found the way Cargo finds them — `tests/<name>.rs` and
# `tests/<name>/main.rs` — so a file added later is included without anyone
# listing it here.
targets=""
for crate in $crates; do
  case "$crate" in
    *[!a-z0-9_]*)
      echo "rust-integration: '$crate' is not a crate name" >&2
      exit 2
      ;;
  esac
  for file in crates/"$crate"/tests/*.rs crates/"$crate"/tests/*/main.rs; do
    [ -f "$file" ] || continue
    case "$file" in
      */main.rs) name=$(basename "$(dirname "$file")") ;;
      *) name=$(basename "$file" .rs) ;;
    esac
    if [ "$crate" = "uf_cli" ] && [ "$name" = "vite" ] && [ "$vite" != "true" ]; then
      echo "rust-integration: leaving out uf_cli's vite.rs, which this change does not reach (the release queue runs it)"
      continue
    fi
    case "$targets " in
      *" --test $name "*) ;;
      *) targets="$targets --test $name" ;;
    esac
  done
done
if [ -z "$targets" ]; then
  echo "rust-integration: none of $scope has integration tests"
  exit 0
fi

# `--workspace` with the targets named, rather than `-p <crate>`: Cargo unifies
# features across the packages it selects, so selecting fewer would build the
# dependencies a second way instead of reusing what the unit tests just built.
# A name two crates share (`typecheck`, `goldens`) runs in both, which costs
# seconds.
echo "rust-integration: cargo test --workspace --all-features --profile ci --no-fail-fast$targets"
# shellcheck disable=SC2086 # one word per flag and name
exec cargo test --workspace --all-features --profile ci --no-fail-fast $targets
