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
#
# A second argument of `unnumbered` gives `uf@0.0.0-alpha.5` the shape #443 is
# about: a commit GitHub did not stamp with a pull request number, because the
# title was edited on the way in or the merge was made another way — and a
# release commit that carries none either, which is what a release commit looks
# like until it is merged. `hero` is left holding the unnumbered commit, which
# the hash case below has to be able to name.
hero=""
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
  hero=""
  if [ "${2:-}" = unnumbered ]; then
    commit "docs: sharpen the hero copy" hero
    hero="$(git -C "$root" rev-parse --short=8 HEAD)"
    commit "chore(release): uf@0.0.0-alpha.5" release
  else
    commit "chore(release): uf@0.0.0-alpha.5 (#5)" release
  fi
  git -C "$root" tag "uf@0.0.0-alpha.5"
}

# `$complete` with one more line inside the `uf@0.0.0-alpha.5` section.
plus_line() {
  printf '%s' "$complete" | awk -v line="$1" '{ print } /the fourth thing/ { print line }'
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

# --- #443: a commit with no pull request number is in the release too --------
# `f3f743c docs: sharpen uf React hero copy` shipped in `uf@0.0.0-alpha.8` while
# this check reported sixteen pull requests in the range and sixteen named. It
# built the range's list by pulling `(#NNN)` out of each subject and dropping
# every subject that had none, so the seventeenth commit was not unmatched — it
# was not counted.
scratch unnumbered_missing unnumbered
changelog unnumbered_missing "$complete"
run unnumbered_missing
[ "$status" -ne 0 ] || fail "a commit with no pull request number was accepted: $out"
case "$out" in
  *"sharpen the hero copy"*) ;;
  *) fail "the refusal does not name the commit that has no number: $out" ;;
esac
pass "a commit whose subject carries no pull request number is not invisible"

# --- and the line `uf release` writes for it covers it -----------------------
# The generated section carries the commit's summary, so a section nobody
# rewrote passes without anybody having to do anything.
scratch unnumbered_generated unnumbered
changelog unnumbered_generated "$(plus_line '- sharpen the hero copy')"
run unnumbered_generated
[ "$status" -eq 0 ] || fail "the line uf release writes did not cover the commit: $out"
case "$out" in
  *"1 with no number"*) ;;
  *) fail "the pass does not say how many commits carried no number: $out" ;;
esac
pass "the summary \`uf release\` writes for an unnumbered commit covers it"

# --- the release's own commit has no number either, and is still exempt ------
# It is made locally, so it carries no number until it is merged, and the
# section is what it contains: a section that had to cite the commit writing it
# could never be written.
case "$out" in
  *"chore(release)"*) fail "the release's own unnumbered commit was reported: $out" ;;
  *) ;;
esac
pass "the release's own commit is exempt whether or not it carries a number"

# --- a section rewritten into prose cites the commit by hash -----------------
# `uf@0.0.0-alpha.8` folded its unnumbered commit into a sentence about another
# one, which is a reasonable thing to write and leaves the summary nowhere in
# the section. A commit with no pull request number has one other durable name.
scratch unnumbered_hash unnumbered
changelog unnumbered_hash "$(plus_line "- the hero copy, rewritten by hand ($hero)")"
run unnumbered_hash
[ "$status" -eq 0 ] || fail "an abbreviated hash did not cover the commit: $out"
pass "a section that cites the commit by its abbreviated hash covers it"

# --- and a hash that is not this commit's does not cover it ------------------
scratch unnumbered_wrong_hash unnumbered
changelog unnumbered_wrong_hash "$(plus_line '- the hero copy, rewritten by hand (0123456)')"
run unnumbered_wrong_hash
[ "$status" -ne 0 ] || fail "any hex word passed as a citation: $out"
pass "a hex word that is not this commit's hash does not cover it"

echo "test-changelog-covers: all checks passed"
