#!/bin/sh
# Every shell script in this repository parses under a POSIX shell.
#
# `uf@0.0.0-alpha.8` published all seventeen packages and the release was
# reported as failed, because `tools/release/verify-npm.sh` would not parse:
#
#     tools/release/verify-npm.sh: 1: Syntax error: end of file unexpected
#
# The script had been correct for weeks under the shell it was written with.
# These scripts say `#!/usr/bin/env sh` or `#!/bin/sh`, and what that means
# depends on the machine: `/bin/sh` is bash on macOS, where every contributor
# writes them, and dash on Ubuntu, where CI runs them. Bash accepts things dash
# refuses, so a script can be committed, reviewed and merged, and first fail in
# the one job nobody wants to be the first to run it.
#
# In that case the difference was a here-document opened with an unquoted
# delimiter, whose body used backticks as punctuation. To a shell a backtick in
# an unquoted here-document is command substitution, and the body contained
# `@uniflowed/<name>` — a redirection with nothing after the `>`. Bash deferred
# the complaint; dash refused the file. Printing that message would also have
# *run* `npm trust` and `tools/release/bootstrap-publish.sh`, which is the
# reason this check is worth having beyond the parse itself.
#
# `sh -n` reads a script and does not run it, so this costs a few milliseconds
# per file and can gate every pull request. It answers only "does it parse" —
# `shellcheck` is the tool for the rest, and this is deliberately not that:
# a parse failure is not a style opinion, and it should never need a waiver.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

# The shell CI actually uses. `dash` where it exists, so a contributor on macOS
# gets the answer the Ubuntu runner will give rather than the one bash gives.
shell=sh
if command -v dash >/dev/null 2>&1; then
  shell=dash
fi
echo "scripts-parse: checking with $shell"

# Tracked files only: a scratch directory or a dependency's script is not this
# repository's to answer for. `upstream/flow` is a submodule and is Meta's.
scripts="$(git ls-files '*.sh' | grep -v '^upstream/')"

failed=""
count=0
for script in $scripts; do
  count=$((count + 1))
  if ! output="$("$shell" -n "$script" 2>&1)"; then
    printf '  broken     %s\n' "$script"
    printf '             %s\n' "$output"
    failed="$failed $script"
  fi
done

if [ -n "$failed" ]; then
  printf '\nThese do not parse under %s:%s\n' "$shell" "$failed" >&2
  printf '\nThey may still run under bash. `/bin/sh` is bash on macOS and dash on the\n' >&2
  printf 'CI runner, so a script that only bash accepts fails where it is least\n' >&2
  printf 'convenient. Run `%s -n <script>` to see it locally.\n' "$shell" >&2
  exit 1
fi

printf '  %s scripts parse\n' "$count"
