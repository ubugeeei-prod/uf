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
  for name in core state; do
    printf '{ "name": "@uniflowed/%s", "version": "0.0.0" }\n' "$name" > "$root/packages/$name/package.json"
  done
  # `orm` is the declaration in neither list, so a correct tree says so in the
  # manifest — that is the third rule this script now checks, and this fixture is
  # what a repository looks like once it holds.
  printf '{ "name": "@uniflowed/orm", "version": "0.0.0", "private": true }\n' > "$root/packages/orm/package.json"
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

# --- and it has to say it is not for the registry ---------------------------
# The rule the third check adds. A declaration in neither list is never
# published, and until the manifest says `private` the only thing that ever
# refuses is `npm install`, with `ETARGET` and no explanation. Taking the field
# back out is exactly the state all twenty of these packages were in.
scratch unmarked
printf '{ "name": "@uniflowed/orm", "version": "0.0.0" }\n' > "$work/unmarked/packages/orm/package.json"
run unmarked
refuses "a declaration in neither list with no private" "packages/orm"

# --- unless a list claims it -------------------------------------------------
# Membership outranks the scan, which is what keeps `core` and `stylex`
# publishable: both look like declarations to `isDeclaration` and both are
# published on purpose. A package a list names is never asked to be private.
scratch claimed
printf '{ "name": "@uniflowed/orm", "version": "0.0.0" }\n' > "$work/claimed/packages/orm/package.json"
printf '# pending\nstate\norm\n' > "$work/claimed/tools/release/pending-packages.txt"
run claimed
[ "$status" -eq 0 ] || fail "a declaration a list claims was refused: $out"
pass "a list claiming a declaration outranks the scan"

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

# --- a published package may not need an unpublished one --------------------
# The failure that started the check, reached from the other direction: the
# tarball for a published name would ask npm for a version of a name npm does
# not have. It installs from this workspace and nowhere else, and the only
# person who finds out is a user typing `npm install`.
scratch depends-on-pending
printf '{ "name": "@uniflowed/core", "version": "0.0.0", "dependencies": { "@uniflowed/state": "0.0.0" } }\n' \
  > "$work/depends-on-pending/packages/core/package.json"
run depends-on-pending
refuses "a published package depending on a pending one" "would answer ETARGET"

# A peer dependency reaches the same registry and counts the same way.
scratch peer-on-pending
printf '{ "name": "@uniflowed/core", "version": "0.0.0", "peerDependencies": { "@uniflowed/state": "0.0.0" } }\n' \
  > "$work/peer-on-pending/packages/core/package.json"
run peer-on-pending
refuses "a published package peer-depending on a pending one" "would answer ETARGET"

# And a declaration is in neither list *because* it will never be published,
# so depending on one is the same ETARGET by a different route. Stating the
# rule over "is it pending" rather than "is it published" would have missed
# this one entirely.
scratch depends-on-declaration
printf '{ "name": "@uniflowed/core", "version": "0.0.0", "dependencies": { "@uniflowed/orm": "0.0.0" } }\n' \
  > "$work/depends-on-declaration/packages/core/package.json"
run depends-on-declaration
refuses "a published package depending on a declaration" "is not in either release manifest"

# --- and the edges that are fine stay fine ----------------------------------
# A pending package may depend on a published one — that is the ordinary
# direction — and a dev dependency is not installed for a consumer at all.
scratch allowed-edges
printf '{ "name": "@uniflowed/state", "version": "0.0.0", "dependencies": { "@uniflowed/core": "0.0.0" } }\n' \
  > "$work/allowed-edges/packages/state/package.json"
printf '{ "name": "@uniflowed/core", "version": "0.0.0", "devDependencies": { "@uniflowed/state": "0.0.0" } }\n' \
  > "$work/allowed-edges/packages/core/package.json"
run allowed-edges
[ "$status" -eq 0 ] || fail "an ordinary dependency direction was refused: $out"
pass "pending may depend on published, and a devDependency is not a consumer's"

echo "test-publishable: all checks passed"
