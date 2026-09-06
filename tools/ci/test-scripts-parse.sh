#!/bin/sh
# `scripts-parse.sh` against a repository it can safely make wrong.
#
# The check is one line of real work, so what is worth testing is that it
# *notices*. A guard that reports success over an empty list is the failure it
# was written to prevent, wearing a green tick — and the bug it exists for,
# `uf@0.0.0-alpha.8`'s `verify-npm.sh`, was a script that had passed every
# review and every local run.
#
# So: the exact shape that broke, planted in a scratch repository, and a check
# that the guard fails on it and passes without it.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/scripts-parse.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-scripts-parse.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-scripts-parse: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A repository with the check in it and one script beside it. `git ls-files` is
# what the check reads, so this has to be a real repository with real commits.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci"
  cp "$script" "$root/tools/ci/scripts-parse.sh"
  printf '#!/bin/sh\necho fine\n' > "$root/tools/ci/subject.sh"
  ( cd "$root" \
    && git init -q . \
    && git add -A \
    && git -c user.email=t@e -c user.name=t commit -qm one ) >/dev/null 2>&1
  echo "$root"
}

# 1. A repository whose scripts all parse.
root="$(scratch clean)"
( cd "$root" && sh tools/ci/scripts-parse.sh >/dev/null 2>&1 ) \
  || fail "rejected a repository in which every script parses"
pass "passes when every script parses"

# 2. The shape that broke the alpha.8 release: an unquoted here-document whose
#    body has a backtick around something containing `>`.
root="$(scratch broken)"
cat > "$root/tools/ci/subject.sh" <<'SUBJECT'
#!/bin/sh
cat >&2 <<MESSAGE
look for `npm notice @uniflowed/<name>`
MESSAGE
SUBJECT
if ( cd "$root" && sh tools/ci/scripts-parse.sh >/dev/null 2>&1 ); then
  fail "accepted an unquoted here-document with a backtick redirection in it"
fi
pass "rejects the here-document that broke uf@0.0.0-alpha.8"

# 3. And it is the *parse* it objects to, not the file's name or its presence:
#    quoting the delimiter is the fix, and the same file then passes.
cat > "$root/tools/ci/subject.sh" <<'SUBJECT'
#!/bin/sh
cat >&2 <<'MESSAGE'
look for `npm notice @uniflowed/<name>`
MESSAGE
SUBJECT
( cd "$root" && sh tools/ci/scripts-parse.sh >/dev/null 2>&1 ) \
  || fail "still rejected the file after the delimiter was quoted"
pass "accepts the same body once the delimiter is quoted"

# 4. An untracked script is not this repository's to answer for, and a check
#    that read the working tree instead of the index would fail on one.
printf '#!/bin/sh\nif [ ; then\n' > "$root/tools/ci/untracked.sh"
( cd "$root" && sh tools/ci/scripts-parse.sh >/dev/null 2>&1 ) \
  || fail "read an untracked file that git does not track"
pass "ignores an untracked script"

echo "test-scripts-parse: ok"
