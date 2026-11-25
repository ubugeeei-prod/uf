#!/bin/sh
# `rustc-wrapper.sh` and `parallel-frontend-env.sh` against a fake compiler.
#
# The wrapper changes how crates compile, so the cases that must stay unchanged
# matter as much as the one that changes: a registry dependency, a sibling
# checkout whose path starts with this one, a build-script probe, Cargo's `-vV`
# query, and the frontend turned off.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-rustc-wrapper.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM
work="$(CDPATH='' cd -- "$work" && pwd -P)"

fail() {
  echo "test-rustc-wrapper: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A copy of the wrapper in a scratch checkout, a fake compiler that prints its
# arguments one per line, and a fake kache that forwards to it.
checkout="$work/checkout"
mkdir -p "$checkout/tools/ci" "$checkout/crates/uf_infra"
cp "$repo_root/tools/ci/rustc-wrapper.sh" "$checkout/tools/ci/rustc-wrapper.sh"
cp "$repo_root/tools/ci/parallel-frontend-env.sh" "$checkout/tools/ci/parallel-frontend-env.sh"
chmod +x "$checkout/tools/ci/rustc-wrapper.sh"
wrapper="$checkout/tools/ci/rustc-wrapper.sh"
rustc="$work/fake-rustc"
printf '#!/bin/sh\nprintf "%%s\\n" "$@"\n' > "$rustc"
kache="$work/fake-kache"
printf '#!/bin/sh\necho via-kache\nexec "$@"\n' > "$kache"
chmod +x "$rustc" "$kache"
member="$checkout/crates/uf_infra"

# Runs the wrapper with only the given variables set, and prints its arguments
# joined by spaces.
run() {
  env -i PATH="$PATH" "$@" "$wrapper" "$rustc" $args | tr '\n' ' ' | sed 's/ $//'
}

expect() {
  label="$1"
  expected="$2"
  shift 2
  actual="$(run "$@")"
  [ "$actual" = "$expected" ] || fail "$label: expected '$expected', got '$actual'"
  pass "$label"
}

args="--crate-name uf_infra --edition=2024 crates/uf_infra/src/lib.rs"
expect "a crate from this checkout gets the parallel frontend" \
  "$args -Zunstable-options --jobs-frontend=8" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$member" CARGO_CRATE_NAME=uf_infra
expect "the Flow port under upstream/ is part of this checkout" \
  "$args -Zunstable-options --jobs-frontend=8" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$checkout/upstream/flow/rust_port/crates/flow_parser" CARGO_CRATE_NAME=uf_infra
expect "a sibling checkout whose path extends this one is not" \
  "$args" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$checkout-base/crates/uf_infra" CARGO_CRATE_NAME=uf_infra
for jobs in 1 0 ''; do
  expect "UF_RUSTC_FRONTEND_JOBS='$jobs' leaves the crate alone" \
    "$args" \
    UF_RUSTC_FRONTEND_JOBS="$jobs" CARGO_MANIFEST_DIR="$member" CARGO_CRATE_NAME=uf_infra
done

args="--crate-name proc_macro2 --edition=2021 src/lib.rs"
expect "a registry dependency compiles as Cargo asked" \
  "$args" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$HOME/.cargo/registry/src/proc-macro2-1.0.107" CARGO_CRATE_NAME=proc_macro2

args="--crate-name probe --crate-type=lib -"
expect "a build-script probe that inherits a member's environment is left alone" \
  "$args" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$member" CARGO_CRATE_NAME=build_script_build

args="-vV"
expect "Cargo's version query passes through" \
  "-vV" \
  UF_RUSTC_FRONTEND_JOBS=8 CARGO_MANIFEST_DIR="$member"

args="--crate-name uf_infra crates/uf_infra/src/lib.rs"
expect "kache receives the call, with the frontend flags" \
  "via-kache $args -Zunstable-options --jobs-frontend=4" \
  UF_KACHE="$kache" UF_RUSTC_FRONTEND_JOBS=4 CARGO_MANIFEST_DIR="$member" CARGO_CRATE_NAME=uf_infra
args="-vV"
expect "kache receives Cargo's queries too" "via-kache -vV" UF_KACHE="$kache"
expect "an empty UF_KACHE turns kache off" "-vV" UF_KACHE=

# The CI half: what `.github/actions/rust-cache` appends to `$GITHUB_ENV`.
enable() {
  github_env="$work/github-env"
  : > "$github_env"
  env -i PATH="$PATH" GITHUB_WORKSPACE="$checkout" GITHUB_ENV="$github_env" "$@" \
    sh "$checkout/tools/ci/parallel-frontend-env.sh" >/dev/null 2>&1 || return 1
  cat "$github_env"
}

actual="$(enable UF_RUSTC_FRONTEND_JOBS=8)"
expected="RUSTC_WRAPPER=$wrapper
UF_RUSTC_FRONTEND_JOBS=8"
[ "$actual" = "$expected" ] || fail "CI export: expected '$expected', got '$actual'"
pass "a CI job that opts in routes Cargo through the wrapper"

actual="$(enable UF_RUSTC_FRONTEND_JOBS=8 RUSTC_WRAPPER=sccache)"
[ -z "$actual" ] || fail "a job's own RUSTC_WRAPPER was replaced: '$actual'"
pass "a job that chose its own wrapper keeps it"

actual="$(enable UF_RUSTC_FRONTEND_JOBS=8 GITHUB_WORKSPACE="$work")"
[ -z "$actual" ] || fail "a checkout without the wrapper was routed to it: '$actual'"
pass "a checkout without the wrapper is left alone"

for jobs in '' 0 08 '8 -Zother' x; do
  if enable UF_RUSTC_FRONTEND_JOBS="$jobs" >/dev/null; then
    fail "UF_RUSTC_FRONTEND_JOBS='$jobs' was accepted"
  fi
done
pass "a thread count that is not a positive integer is refused"

# And the wiring: the dev shell and the Rust jobs actually use it.
grep -qF 'export RUSTC_WRAPPER="$uf_root/tools/ci/rustc-wrapper.sh"' "$repo_root/flake.nix" ||
  fail "the dev shell does not route rustc through the wrapper"
grep -qF "export UF_KACHE=\"''\${UF_KACHE-\${kache}/bin/kache}\"" "$repo_root/flake.nix" ||
  fail "the dev shell does not hand rustc to kache"
pass "the dev shell routes rustc through the wrapper and kache"

action="$repo_root/.github/actions/rust-cache/action.yml"
grep -q "inputs.frontend-jobs != '1'" "$action" ||
  fail "the rust-cache action does not gate the wrapper on frontend-jobs"
for key in toolchain clippy test upstream-typecheck; do
  awk -v key="$key" '
    $0 ~ "key: " key "$" { found = 1; next }
    found && /frontend-jobs: "8"/ { ok = 1 }
    found && /- (uses|run|name):/ { exit }
    END { exit ok ? 0 : 1 }
  ' "$repo_root/.github/workflows/ci.yml" ||
    fail "the $key job does not set frontend-jobs: \"8\""
done
# Not the benchmark, which measures, nor Upstream Flow, which is the canary for
# the moving nightly and should see that compiler as it ships.
for key in bench-compile upstream-flow; do
  if awk -v key="$key" '
    $0 ~ "key: " key "$" { found = 1; next }
    found && /frontend-jobs/ { bad = 1 }
    found && /- (uses|run|name):/ { exit }
    END { exit bad ? 0 : 1 }
  ' "$repo_root/.github/workflows/ci.yml"; then
    fail "the $key job runs with the parallel frontend"
  fi
done
pass "the Rust compile jobs opt in; the benchmark and the moving-nightly canary do not"

echo "test-rustc-wrapper: all cases passed"
