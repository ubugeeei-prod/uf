#!/usr/bin/env sh
# What actually reached npm, checked from the registry rather than from here.
#
# `uf@0.0.0-alpha.2` has a git tag and a GitHub release. npm has nothing from
# it: every run of `publish.yml` has failed, and until this existed nothing
# between the tag and a user noticed. See ubugeeei-prod/uf#142.
#
#   tools/release/verify-npm.sh                 # the version in the tree
#   tools/release/verify-npm.sh 0.0.0-alpha.3
#   tools/release/verify-npm.sh --closure-only  # no network
#
# Two phases, and the first needs no network:
#
#   1. Every `@uniflowed/*` dependency of a published package is itself
#      published. `@uniflowed/router` depends on `@uniflowed/server`, so a
#      release that sends one without the other produces a package that
#      resolves to nothing — `ETARGET` on the first thing a user types.
#   2. Every listed name is on the registry at this version, and installs
#      into an empty directory with its dependencies resolved.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

list="tools/release/published-packages.txt"
closure_only=false
version=""

for argument in "$@"; do
  case "$argument" in
    --closure-only) closure_only=true ;;
    -*) echo "verify-npm: unknown option: $argument" >&2; exit 2 ;;
    *) version="$argument" ;;
  esac
done

packages="$(grep -vE '^[[:space:]]*(#|$)' "$list")"

if [ -z "$version" ]; then
  version="$(node -p "require('./packages/core/package.json').version")"
fi

# ---- 1. The closure ------------------------------------------------------

echo "verify-npm: checking the dependency closure of $list"
missing_closure=""
for name in $packages; do
  manifest="packages/$name/package.json"
  [ -f "$manifest" ] || { echo "verify-npm: no $manifest" >&2; exit 1; }
  for dependency in $(node -e "
    const m = require('./$manifest');
    const all = { ...m.dependencies, ...m.peerDependencies, ...m.optionalDependencies };
    for (const key of Object.keys(all)) if (key.startsWith('@uniflowed/')) console.log(key.slice('@uniflowed/'.length));
  "); do
    if ! echo "$packages" | grep -qx "$dependency"; then
      printf '  %-18s depends on @uniflowed/%s, which is not published\n' "$name" "$dependency"
      missing_closure="$missing_closure $name->$dependency"
    fi
  done
done

if [ -n "$missing_closure" ]; then
  cat >&2 <<MESSAGE

Not closed:$missing_closure

A package whose dependency is not published resolves to nothing. Either add
the dependency to $list — and bind it with
tools/release/trust-npm.sh — or take the dependent out.
MESSAGE
  exit 1
fi
echo "  every @uniflowed/* dependency is itself published"

if [ "$closure_only" = true ]; then
  exit 0
fi

# ---- 2. What the registry has --------------------------------------------

echo
echo "verify-npm: checking @uniflowed/* at $version on the registry"

# npm's read path is eventually consistent: `npm publish` returns before every
# replica can answer for the version it just accepted. Asking once, straight
# after the publish job, reported `@uniflowed/host` missing from the
# `uf@0.0.0-alpha.5` release while it was in fact published — a release that
# had gone out perfectly, failed by the step that exists to say so.
#
# So the names that are not there yet are asked again, backing off, and only a
# name still absent after all of it is missing.
#
# The waiting is shared rather than per name: a first pass asks everything
# once, and each round after it asks only what is still missing. Backing off
# per name would multiply the wait by the length of the list — 17 packages at
# half a minute each is nine minutes to report a release that never went out,
# which is the case that most wants to be quick.
# One deadline for the whole wait, and every request bounded by what is left of
# it. `curl` with no `--max-time` will sit on a stalled connection for as long
# as the kernel lets it, so a message promising "a minute of asking" would be a
# claim nothing enforced — and seventeen names times six rounds is a lot of
# chances to be stalled.
#
# Three minutes, not one. `uf@0.0.0-alpha.7` published all seventeen names and
# this job failed anyway: sixteen were on the registry within three seconds and
# `@uniflowed/host` was not there at 55 seconds, when the backoff ran out of
# deadline. It was there a few minutes later, unchanged and correct. A release
# that went out and is reported as broken costs more than a release that never
# went out being reported slowly — and the case this budget was tightened for,
# a publish that produced nothing, is still bounded, because every name is
# asked in the same rounds rather than one after another.
WAIT_SECONDS=180
REQUEST_SECONDS=10
deadline=$(( $(date +%s) + WAIT_SECONDS ))

remaining() {
  left=$(( deadline - $(date +%s) ))
  [ "$left" -lt 0 ] && left=0
  [ "$left" -gt "$REQUEST_SECONDS" ] && left=$REQUEST_SECONDS
  echo "$left"
}

present_line() {
  printf '  on npm      @uniflowed/%s@%s\n' "$1" "$version"
}

on_registry() {
  budget="$(remaining)"
  # Out of time is not "not there": the caller stops asking and reports what it
  # knows, rather than reporting a name absent because the clock ran out.
  [ "$budget" -eq 0 ] && return 1
  curl -sS -o /dev/null -w '%{http_code}' \
    --connect-timeout "$budget" --max-time "$budget" \
    "https://registry.npmjs.org/@uniflowed%2f$1/$version" 2>/dev/null \
    | grep -q '^200$'
}

absent=""
for name in $packages; do
  if on_registry "$name"; then present_line "$name"; else absent="$absent $name"; fi
done

delay=1
while [ -n "$absent" ] && [ "$(remaining)" -gt 0 ]; do
  # Never sleep past the deadline: a 32-second nap that starts with 3 seconds
  # left is 29 seconds of doing nothing before reporting.
  left=$(( deadline - $(date +%s) ))
  [ "$delay" -gt "$left" ] && delay="$left"
  [ "$delay" -le 0 ] && break
  printf '  waiting     %ss for%s\n' "$delay" "$absent"
  sleep "$delay"
  delay=$((delay * 2))
  still=""
  for name in $absent; do
    if on_registry "$name"; then present_line "$name"; else still="$still $name"; fi
  done
  absent="$still"
done

for name in $absent; do
  printf '  MISSING     @uniflowed/%s@%s\n' "$name" "$version"
done

if [ -n "$absent" ]; then
  cat >&2 <<MESSAGE

Not on the registry at $version:$absent

The tag says this version was released. npm does not have it after retrying for
$WAIT_SECONDS seconds, so nobody can install it.

Two things this can be, and they are told apart by the publish job above:

  * npm never accepted the name — look for `npm notice 📦  @uniflowed/<name>`
    in that job. If it is absent, the name is not bound by `npm trust` and
    `tools/release/bootstrap-publish.sh` has not been run for it. See #210.
  * npm accepted it and the registry had not caught up. The notice is there,
    and `npm view @uniflowed/<name>@$version` answers now. Re-run this job.

Either way npm is additive, so the names that did go out stay.
MESSAGE
  exit 1
fi

# ---- 3. That they install ------------------------------------------------

echo
echo "verify-npm: installing them into an empty project"
# With a template: BSD `mktemp -d` ignores `TMPDIR` without one, and this
# script is run by hand on macOS as often as by CI on Linux.
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-verify-npm.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM
cd "$work"
npm init -y >/dev/null 2>&1

specifiers=""
for name in $packages; do
  specifiers="$specifiers @uniflowed/$name@$version"
done

# shellcheck disable=SC2086 # deliberate word splitting: one argument per package
if npm install --no-audit --no-fund $specifiers >"$work/install.log" 2>&1; then
  echo "  installed $(echo "$packages" | wc -l | tr -d ' ') packages"
else
  echo "verify-npm: install failed" >&2
  tail -20 "$work/install.log" >&2
  exit 1
fi

for name in $packages; do
  entry="$work/node_modules/@uniflowed/$name/package.json"
  [ -f "$entry" ] || { echo "verify-npm: @uniflowed/$name is not in node_modules" >&2; exit 1; }
done

cd "$repo_root"
echo
echo "verify-npm: @uniflowed/* at $version is on the registry and installs"
