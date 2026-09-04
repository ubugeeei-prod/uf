#!/usr/bin/env sh
# What a `uf@*` tag would publish, and what would stop it.
#
# `publish.yml` publishes every name in `published-packages.txt` over OIDC, and
# a name that `npm trust` has not bound to that workflow fails there — after the
# names before it have already gone out. That is recoverable, because npm is
# additive and re-running the job publishes the rest, but it means a release can
# be half-sent by adding a package to the list and tagging without running
# `trust-npm.sh`.
#
# This is the check to run before tagging. It cannot read the bindings — `npm
# trust list` reads them as the *user*, and an unauthenticated read returns
# nothing — so it reports what is on the registry instead. A name that is not
# there yet is a name to confirm is bound, not proof that it is not.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

list="tools/release/published-packages.txt"
missing=""
count=0

for name in $(grep -v '^#' "$list" | grep -v '^[[:space:]]*$'); do
  count=$((count + 1))
  url="https://registry.npmjs.org/@uniflowed%2f$name"
  status="$(curl -fsS -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || echo 000)"
  if [ "$status" = "200" ]; then
    printf '  on npm      @uniflowed/%s\n' "$name"
  else
    printf '  not yet     @uniflowed/%s\n' "$name"
    missing="$missing $name"
  fi
done

printf '\n%s packages listed\n' "$count"

if [ -n "$missing" ]; then
  cat >&2 <<MESSAGE

Not on the registry yet:$missing

Each has to be bound to this repository's publish workflow before a tag can
send it. The binding is made as you, not as the workflow, so it is one command
on a machine you are logged in on:

  npm login
  tools/release/trust-npm.sh

It is idempotent — a name that is already bound is reported and left alone —
and it binds names that have never been published, which is what lets the first
release go out over OIDC like every one after it.

Tagging before that leaves a release half-sent: the bound names publish and the
first unbound one fails the job.
MESSAGE
  exit 1
fi

printf 'every listed package is on the registry\n'
