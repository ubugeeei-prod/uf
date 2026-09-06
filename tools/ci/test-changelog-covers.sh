#!/bin/sh
# `changelog-covers-the-release.sh` against a repository whose history it can
# be told to have.
#
# The check reads `git log` between two tags, so testing it needs a repository
# with tags — a scratch one, built here, rather than assertions about this
# project's own history, which would change every release.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/changelog-covers-the-release.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-changelog.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-changelog-covers: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A repository with two releases in it: one before the version this check
# begins at, and one after.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci"
  cp "$script" "$root/tools/ci/changelog-covers-the-release.sh"

  git -C "$root" init --quiet
  git -C "$root" config user.email uf@example.com
  git -C "$root" config user.name uf
  commit() {
    printf '%s\n' "$2" > "$root/file.txt"
    git -C "$root" add file.txt
    git -C "$root" -c commit.gpgsign=false commit --quiet -m "$1"
  }
  commit "feat: the first thing (#1)" one
  git -C "$root" tag "uf@0.0.0-alpha.3"
  commit "feat: the second thing (#2)" two
  git -C "$root" tag "uf@0.0.0-alpha.4"
  commit "fix: the third thing (#3)" three
  commit "fix: the fourth thing (#4)" four
  commit "chore(release): uf@0.0.0-alpha.5 (#5)" release
  git -C "$root" tag "uf@0.0.0-alpha.5"
}

run() {
  set +e
  out="$(cd "$work/$1" && ./tools/ci/changelog-covers-the-release.sh 2>&1)"
  status=$?
  set -e
}

changelog() {
  printf '%s' "$2" > "$work/$1/CHANGELOG.md"
}

complete='# Changelog

## uf@0.0.0-alpha.5

- the third thing (#3)
- the fourth thing (#4)

## uf@0.0.0-alpha.4

- the second thing (#2)

## uf@0.0.0-alpha.3

- the first thing (#1)
'

# --- a section that names everything in its range ----------------------------
scratch complete
changelog complete "$complete"
run complete
[ "$status" -eq 0 ] || fail "a complete section was refused: $out"
case "$out" in
  *"2 released version(s)"*) ;;
  *) fail "the pass does not say how many releases it checked: $out" ;;
esac
pass "a section that names every pull request in its range passes"

# --- the case that started this ---------------------------------------------
# `uf@0.0.0-alpha.5` went out with twenty-two commits and twelve in its
# changelog: the ones that merged while the release waited for CI.
scratch missing
changelog missing "$(printf '%s' "$complete" | grep -v 'the fourth thing')"
run missing
[ "$status" -ne 0 ] || fail "a section missing a merged pull request was accepted: $out"
case "$out" in
  *"the fourth thing (#4)"*) ;;
  *) fail "the refusal does not name what is missing: $out" ;;
esac
pass "a pull request that merged while the release waited is missed"

# --- the release does not have to cite itself --------------------------------
scratch itself
changelog itself "$complete"
run itself
[ "$status" -eq 0 ] || fail "the release commit was required to name itself: $out"
case "$out" in
  *"#5"*) fail "the release commit was reported: $out" ;;
  *) ;;
esac
pass "the release's own commit is not something the section has to mention"

# --- and the boundary holds --------------------------------------------------
# `uf@0.0.0-alpha.3` is below where this check begins, so its section may say
# anything. Older releases were written before the check existed.
scratch boundary
changelog boundary "$(printf '%s' "$complete" | grep -v 'the first thing')"
run boundary
[ "$status" -eq 0 ] || fail "a version below the starting point was checked: $out"
pass "a release from before this check began is not checked"

echo "test-changelog-covers: all checks passed"
