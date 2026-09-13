#!/usr/bin/env sh
# Refuse to start an OIDC publish when a listed package name is absent on npm.
#
# `publish.yml` sends the names in `published-packages.txt` one by one. If one
# of those names has never been created, npm refuses it after the packages before
# it have already gone out. That is recoverable, because npm is additive, but it
# is still a half-sent release.
#
# This guard asks the cheap registry question before the first `npm publish`.
# It does not prove the trusted-publisher binding is correct: npm exposes that
# as a user-authenticated account read, and this workflow is not that user. It
# does catch the missing-name state that `release:preflight` catches by hand.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

list="tools/release/published-packages.txt"
missing=""
count=0

for package in $(grep -vE '^[[:space:]]*(#|$)' "$list"); do
  count=$((count + 1))
  name="@uniflowed/${package}"
  if npm view "$name" name >/dev/null 2>&1; then
    printf '  on npm      %s\n' "$name"
  else
    printf '  missing     %s\n' "$name"
    missing="${missing} ${package}"
  fi
done

printf '\n%s package names checked\n' "$count"

if [ -n "$missing" ]; then
  cat >&2 <<MESSAGE

These package names are listed for OIDC publishing but do not exist on npm yet:
${missing}

Create them once from a logged-in npm session, bind them to this repository's
publish workflow, then rerun the publish:

  npm login
  tools/release/bootstrap-publish.sh
  tools/release/trust-npm.sh

Failing here is deliberately before any package is published. Without this, the
publish job would send every earlier package and then fail at the missing name.
MESSAGE
  exit 1
fi

echo "publish-names-exist: every listed package name exists on npm"
