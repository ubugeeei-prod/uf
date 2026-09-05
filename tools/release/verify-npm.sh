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
absent=""
for name in $packages; do
  url="https://registry.npmjs.org/@uniflowed%2f$name/$version"
  status="$(curl -fsS -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || echo 000)"
  if [ "$status" = "200" ]; then
    printf '  on npm      @uniflowed/%s@%s\n' "$name" "$version"
  else
    printf '  MISSING     @uniflowed/%s@%s\n' "$name" "$version"
    absent="$absent $name"
  fi
done

if [ -n "$absent" ]; then
  cat >&2 <<MESSAGE

Not on the registry at $version:$absent

The tag says this version was released. npm does not have it, so nobody can
install it. Re-run the publish job once the names are bound; npm is additive,
so the ones that did go out stay.
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
