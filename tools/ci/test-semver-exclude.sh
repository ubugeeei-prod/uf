#!/usr/bin/env sh
# `semver-exclude.sh`, against a workspace with a defect planted in it.
#
# The list it used to print was maintained by hand and went stale the moment a
# crate was added that reaches `uf_flow`. These cases are the shapes that
# staleness took, so it cannot take them again.
set -eu

root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"

work="${TMPDIR:-/tmp}/uf-semver-exclude-test.$$"
rm -rf "$work"
mkdir -p "$work/crates" "$work/tools/ci"
trap 'rm -rf "$work"' EXIT
cp "$root/tools/ci/semver-exclude.sh" "$work/tools/ci/"
( cd "$work" && git init -q . && git config user.email t@t && git config user.name t )

write() {
  name=$1; shift
  mkdir -p "$work/crates/$name"
  {
    echo '[package]'
    echo "name = \"$name\""
    echo '[dependencies]'
    for dep in "$@"; do echo "$dep = { path = \"../$dep\" }"; done
  } > "$work/crates/$name/Cargo.toml"
}

failures=0
expect() {
  want=$1; why=$2
  got=$( cd "$work" && sh tools/ci/semver-exclude.sh "$BASE" )
  if [ "$got" != "$want" ]; then
    echo "FAIL: $why" >&2
    echo "  wanted: $want" >&2
    echo "  got:    $got" >&2
    failures=$((failures + 1))
  fi
}

write uf_flow
write uf_check uf_flow
write uf_infra
write uf_assets uf_infra
( cd "$work" && git add -A && git commit -qm base )
BASE=$(cd "$work" && git rev-parse HEAD)

echo "the seed and its direct dependents, and nothing else"
expect "uf_check,uf_flow" "uf_assets reaches neither"

echo "a crate that reaches uf_flow through two hops joins"
write uf_fmt uf_flow
write uf_router uf_fmt
( cd "$work" && git add -A && git commit -qm two-hops )
BASE=$(cd "$work" && git rev-parse HEAD)
expect "uf_check,uf_flow,uf_fmt,uf_router" "the closure is transitive, not one level"

echo "a crate that did not exist at the baseline is excluded too"
write uf_i18n uf_flow
expect "uf_check,uf_flow,uf_fmt,uf_i18n,uf_router" "new at HEAD, absent at the baseline"

echo "and stays excluded once the baseline catches up, because it reaches uf_flow"
( cd "$work" && git add -A && git commit -qm caught-up )
BASE=$(cd "$work" && git rev-parse HEAD)
expect "uf_check,uf_flow,uf_fmt,uf_i18n,uf_router" "this is the case that broke the job"

echo "a new crate that reaches nothing rejoins the gate by itself"
write uf_term
expect "uf_check,uf_flow,uf_fmt,uf_i18n,uf_router,uf_term" "new, so excluded for now"
( cd "$work" && git add -A && git commit -qm term )
BASE=$(cd "$work" && git rev-parse HEAD)
expect "uf_check,uf_flow,uf_fmt,uf_i18n,uf_router" "the baseline has it and it reaches nothing"

if [ "$failures" -ne 0 ]; then
  echo "$failures case(s) failed" >&2
  exit 1
fi
echo "semver-exclude: every case behaved"
