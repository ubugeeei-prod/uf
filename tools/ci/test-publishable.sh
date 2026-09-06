#!/bin/sh
# `publishable.sh` against a `packages/` it can safely make wrong.
#
# The check exists because ten implemented packages — about 22,000 lines of
# Flow, `@uniflowed/state` and `@uniflowed/effect` among them — were in no
# release manifest at all, so `npm install @uniflowed/state` answered `ETARGET`
# and nothing said why. A list somebody adds to is a list somebody forgets, so
# the rule is stated from the other side and enforced from a scratch tree here.
#
# What is worth testing is the discriminator. "Is this a declaration or a
# library" is the only judgement the check makes, and both ways of getting it
# wrong are quiet: calling a real package a declaration leaves it unpublished
# for another release, and calling a declaration real puts a name on npm with
# nothing behind it.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/publishable.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-publishable.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-publishable: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A `packages/` with one of each: a library, a declaration, and a library that
# is implemented but not yet bound on npm.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci" "$root/tools/release" \
    "$root/packages/core" "$root/packages/orm" "$root/packages/state"
  cp "$script" "$root/tools/ci/publishable.sh"
  printf '// @flow\nexport const add = (a: number, b: number): number => a + b;\n' \
    > "$root/packages/core/index.js"
  printf '// @flow\nimport { nativeRuntimeRequired } from "@uniflowed/core";\nexport const open = (): empty => nativeRuntimeRequired("orm");\n' \
    > "$root/packages/orm/index.js"
  printf '// @flow\nexport const atom = <T>(value: T): { value: T } => ({ value });\n' \
    > "$root/packages/state/index.js"
  for name in core orm state; do
    printf '{ "name": "@uniflowed/%s", "version": "0.0.0" }\n' "$name" > "$root/packages/$name/package.json"
  done
  printf '# published\ncore\n' > "$root/tools/release/published-packages.txt"
  printf '# pending\nstate\n' > "$root/tools/release/pending-packages.txt"
}

run() {
  set +e
  out="$("$work/$1/tools/ci/publishable.sh" 2>&1)"
  status=$?
  set -e
}

refuses() {
  [ "$status" -ne 0 ] || fail "$1 was accepted: $out"
  case "$out" in
    *"$2"*) ;;
    *) fail "$1 is refused without naming \`$2\`: $out" ;;
  esac
  pass "$1"
}

# --- a tree that is described -----------------------------------------------
scratch clean
run clean
[ "$status" -eq 0 ] || fail "a described tree was refused: $out"
case "$out" in
  *"2 implemented packages: 1 published, 1 waiting"*) ;;
  *) fail "the summary does not count what it checked: $out" ;;
esac
pass "a library published, a library waiting, and a declaration in neither list"

# --- the case that started this ---------------------------------------------
scratch forgotten
printf '# published\ncore\n' > "$work/forgotten/tools/release/pending-packages.txt"
run forgotten
refuses "an implemented package in neither list" "packages/state"

# --- a declaration is not required to be anywhere ---------------------------
# The other direction of the same judgement: `packages/orm` is in no list and
# must stay that way, because a name with `nativeRuntimeRequired` behind it
# squats the name and cannot run.
scratch declaration
run declaration
[ "$status" -eq 0 ] || fail "a declaration in no list was refused: $out"
pass "a declaration is not required to be published"

# --- and prose about it is not a declaration --------------------------------
# `@uniflowed/story`'s header explains that it used to return
# `nativeRuntimeRequired(…)` and does not any more. A check that read comments
# would have excused the largest package in the pending list.
scratch prose
printf '// @flow\n//\n// It used to call nativeRuntimeRequired(name) and does not any more.\nexport const play = (): number => 1;\n' \
  > "$work/prose/packages/state/index.js"
printf '# pending\n' > "$work/prose/tools/release/pending-packages.txt"
run prose
refuses "a package that only mentions the runtime in a comment" "packages/state"

# --- a nested module counts too ---------------------------------------------
scratch nested
mkdir -p "$work/nested/packages/orm/internal"
printf '// @flow\nexport const open = (): empty => nativeRuntimeRequired("orm");\n' \
  > "$work/nested/packages/orm/internal/driver.js"
printf '// @flow\nexport { open } from "./internal/driver.js";\n' > "$work/nested/packages/orm/index.js"
run nested
[ "$status" -eq 0 ] || fail "a declaration whose call is one directory down was refused: $out"
pass "a declaration is recognized from any module in it, not only its index"

# --- the lists have to describe the tree ------------------------------------
scratch both
printf '# pending\ncore\nstate\n' > "$work/both/tools/release/pending-packages.txt"
run both
refuses "a package in both lists" "take it out of the pending one"

scratch ghost
printf '# published\ncore\nkoru\n' > "$work/ghost/tools/release/published-packages.txt"
run ghost
refuses "a published name with no package behind it" "packages/koru does not exist"

scratch ghost-pending
printf '# pending\nkoru\n' > "$work/ghost-pending/tools/release/pending-packages.txt"
run ghost-pending
refuses "a pending name with no package behind it" "packages/koru does not exist"

echo "test-publishable: all checks passed"
