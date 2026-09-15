#!/bin/sh
# `no-conflict-markers.sh` against repositories it can safely make wrong.
#
# A check that never fails looks exactly like one that works. So each marker it
# is meant to catch is planted in a scratch repository, along with the lines it
# must let through: a heading underline, a marker quoted mid-sentence, and a
# marker in a file git does not track.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/no-conflict-markers.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-conflict-markers.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-no-conflict-markers: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A throwaway repository named $1 whose one tracked page holds the text $2,
# with the script copied to the same relative path so it checks that
# repository. Prints the path of the copy.
repo() {
  dir="$work/$1"
  mkdir -p "$dir/tools/ci"
  cp "$script" "$dir/tools/ci/no-conflict-markers.sh"
  printf '%s\n' "$2" > "$dir/page.md"
  git -C "$dir" init -q
  git -C "$dir" add -A
  printf '%s\n' "$dir/tools/ci/no-conflict-markers.sh"
}

check=$(repo clean "# Title

A paragraph.")
sh "$check" >/dev/null 2>&1 || fail "a clean repository was refused"
pass "a clean repository passes"

check=$(repo heading "Title
=======

A paragraph.")
sh "$check" >/dev/null 2>&1 || fail "a Markdown heading underline was taken for a marker"
pass "a line of equals signs is a heading, not a marker"

check=$(repo quoted "A sentence that quotes >>>>>>> in the middle is not a marker.")
sh "$check" >/dev/null 2>&1 || fail "a marker in the middle of a line was refused"
pass "a marker that does not start its line passes"

n=0
for marker in '>>>>>>> origin/main' '<<<<<<< HEAD' '>>>>>>>' '<<<<<<<'; do
  n=$((n + 1))
  check=$(repo "planted-$n" "Some text.
$marker
More text.")
  if sh "$check" >/dev/null 2>&1; then
    fail "a planted '$marker' passed"
  fi
  pass "a planted '$marker' is refused"
done

check=$(repo untracked "Nothing to see.")
printf '%s\n' '>>>>>>> origin/main' > "$work/untracked/scratch.txt"
sh "$check" >/dev/null 2>&1 || fail "a file git does not track was read"
pass "a file git does not track is not read"

echo "test-no-conflict-markers: all passed"
