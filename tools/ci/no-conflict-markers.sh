#!/bin/sh
# Fail when a tracked file still holds a merge-conflict marker.
#
# A conflict resolved by hand can leave one line of itself behind, and nothing
# else notices. #1093 merged a closing `>>>>>>> origin/main` into the testing
# guide; the docs build rendered it as a paragraph, and every check was green.
#
# Only the opening and closing markers are looked for: seven `<` or `>` at the
# start of a line, then a space or the end of the line. The middle marker is
# left alone because a line of `=======` is also how Markdown underlines a
# heading, and a stray closing marker is what a hand resolution leaves behind.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

# Tracked files only. `git grep` does not descend into submodules, and the
# vendored Flow checkout under `upstream/` is upstream's to keep clean.
set +e
found="$(git grep -n -I -E '^(<<<<<<<|>>>>>>>)( |$)' -- . ':(exclude)upstream')"
rc=$?
set -e

case "$rc" in
  0)
    echo "conflict markers left in tracked files:" >&2
    printf '%s\n' "$found" >&2
    exit 1
    ;;
  1)
    echo "no conflict markers in tracked files"
    ;;
  *)
    echo "no-conflict-markers: git grep failed with exit status $rc" >&2
    exit "$rc"
    ;;
esac
