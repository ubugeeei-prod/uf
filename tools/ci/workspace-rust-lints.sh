#!/usr/bin/env sh
# Every Rust crate in the workspace inherits the workspace lint policy.
#
# `cargo clippy -- -D warnings` already made the CI job fail on warning, but
# the command line was the policy. A contributor who ran `cargo clippy` without
# that suffix could still get a screen full of warnings and a zero exit code.
# The policy now lives in `Cargo.toml`; this check keeps new crates from
# quietly opting out by omission.
set -eu

root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

errors=0
fail() {
  echo "workspace-rust-lints: $1" >&2
  errors=$((errors + 1))
}

grep -q '^\[workspace\.lints\.rust\]$' Cargo.toml \
  || fail "Cargo.toml is missing [workspace.lints.rust]"
grep -q '^warnings = "deny"$' Cargo.toml \
  || fail "Cargo.toml does not deny rust warnings"
grep -q '^\[workspace\.lints\.clippy\]$' Cargo.toml \
  || fail "Cargo.toml is missing [workspace.lints.clippy]"
grep -q '^all = "deny"$' Cargo.toml \
  || fail "Cargo.toml does not deny clippy::all"

for manifest in crates/*/Cargo.toml; do
  if ! awk '
    /^\[lints\]$/ { inside = 1; next }
    /^\[/ { inside = 0 }
    inside && $0 == "workspace = true" { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$manifest"; then
    fail "$manifest does not inherit workspace lints"
  fi
done

if [ "$errors" -ne 0 ]; then
  exit 1
fi

echo "workspace-rust-lints: every crate inherits denied warnings"
