#!/bin/sh
# `lockfile-in-sync.sh` against locks that disagree with their manifests.
#
# The check exists because six jobs went red at once over a lock that was not
# regenerated, each reporting a package none of them was about. So the thing
# worth testing is not that it passes on this repository — it does, and that
# proves nothing — but that each way a lock can fall behind is caught, and
# named precisely enough that the reader knows which file to open.
#
# Its first version asked npm to regenerate the lock and compared bytes, which
# also asked whether today's npm and today's registry agree with the committed
# file. That passed here and failed in CI, and could not be reproduced here
# afterwards. Every case below is decided from the two files alone, so a
# failure means the same thing on every machine.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/lockfile-in-sync.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-lockfile.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-lockfile-in-sync: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A repository shaped like this one: two publishable packages that pin each
# other exactly, and a private workspace that depends on them without being
# one — npm records no version for that entry, which the check has to allow.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci" "$root/packages/core" "$root/packages/cli" "$root/docs"
  cp "$script" "$root/tools/ci/lockfile-in-sync.sh"
  cat > "$root/package.json" <<'JSON'
{ "name": "uf-workspace", "workspaces": ["packages/*", "docs"], "devDependencies": { "biome": "^2.5.0" } }
JSON
  cat > "$root/packages/core/package.json" <<'JSON'
{ "name": "@uniflowed/core", "version": "0.1.0", "dependencies": { "react": "^19.0.0" } }
JSON
  cat > "$root/packages/cli/package.json" <<'JSON'
{ "name": "@uniflowed/cli", "version": "0.1.0", "dependencies": { "@uniflowed/core": "0.1.0" } }
JSON
  cat > "$root/docs/package.json" <<'JSON'
{ "name": "uf-docs", "private": true, "version": "0.0.0", "dependencies": { "@uniflowed/core": "0.1.0" } }
JSON
  cat > "$root/package-lock.json" <<'JSON'
{
  "name": "uf-workspace",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "uf-workspace", "workspaces": ["packages/*", "docs"], "devDependencies": { "biome": "^2.5.0" } },
    "docs": { "name": "uf-docs", "dependencies": { "@uniflowed/core": "0.1.0" } },
    "packages/cli": { "name": "@uniflowed/cli", "version": "0.1.0", "dependencies": { "@uniflowed/core": "0.1.0" } },
    "packages/core": { "name": "@uniflowed/core", "version": "0.1.0", "dependencies": { "react": "^19.0.0" } },
    "node_modules/@uniflowed/cli": { "resolved": "packages/cli", "link": true },
    "node_modules/@uniflowed/core": { "resolved": "packages/core", "link": true },
    "node_modules/uf-docs": { "resolved": "docs", "link": true },
    "node_modules/react": { "version": "19.2.8", "resolved": "https://registry.npmjs.org/react/-/react-19.2.8.tgz" }
  }
}
JSON
}

# Rewrite one path inside the scratch lock or a manifest.
edit() {
  node -e '
    const fs = require("node:fs");
    const [file, change] = process.argv.slice(1);
    const json = JSON.parse(fs.readFileSync(file, "utf8"));
    new Function("json", change)(json);
    fs.writeFileSync(file, JSON.stringify(json, null, 2));
  ' "$1" "$2"
}

run() {
  set +e
  out="$("$work/$1/tools/ci/lockfile-in-sync.sh" 2>&1)"
  status=$?
  set -e
}

# The message has to name the thing to open, or the reader is back to guessing
# which of forty-nine manifests moved.
refuses() {
  name="$1"
  needle="$2"
  [ "$status" -ne 0 ] || fail "$name was accepted: $out"
  case "$out" in
    *"$needle"*) ;;
    *) fail "$name is refused without naming \`$needle\`: $out" ;;
  esac
  pass "$name"
}

# --- a lock that describes its manifests -------------------------------------
scratch clean
run clean
[ "$status" -eq 0 ] || fail "a matching pair was refused: $out"
case "$out" in
  *"4 manifests"*) ;;
  *) fail "the pass does not say how many manifests it checked: $out" ;;
esac
pass "a lock that describes its manifests passes, root and private workspace and all"

# --- and the root manifest is one of them ------------------------------------
# `npm ci` reads the root first, and it was the one path this did not look at:
# a changed range there passed here and was refused by every job that installs,
# which is the failure the whole check exists to catch.
scratch rootdep
edit "$work/rootdep/package.json" 'json.devDependencies.biome = "^3.0.0"'
run rootdep
refuses "a root dependency the lock has not caught up with" "package.json"

scratch rootadd
edit "$work/rootadd/package.json" 'json.devDependencies.prettier = "^3.0.0"'
run rootadd
refuses "a root dependency the manifest has and the lock does not" "prettier"

# --- a package that was added and never locked -------------------------------
# The case that motivated the check: six jobs red over one missing entry.
scratch added
mkdir -p "$work/added/packages/story"
printf '{ "name": "@uniflowed/story", "version": "0.1.0" }\n' > "$work/added/packages/story/package.json"
run added
refuses "a package with no entry in the lock" "packages/story"

# --- a version bumped in the manifest and not in the lock --------------------
scratch bumped
edit "$work/bumped/packages/core/package.json" 'json.version = "0.2.0"'
run bumped
refuses "a version the lock has not caught up with" "0.2.0"

# --- dependencies that drifted, in both directions ---------------------------
scratch depadded
edit "$work/depadded/packages/core/package.json" 'json.dependencies["react-dom"] = "^19.0.0"'
run depadded
refuses "a dependency the manifest has and the lock does not" "react-dom"

scratch depdropped
edit "$work/depdropped/packages/core/package.json" 'delete json.dependencies.react'
run depdropped
refuses "a dependency the lock has and the manifest does not" "react"

scratch deprange
edit "$work/deprange/packages/cli/package.json" 'json.dependencies["@uniflowed/core"] = "0.2.0"'
run deprange
refuses "a pin the two files disagree about" "@uniflowed/core"

# --- a package that was renamed ---------------------------------------------
# The lock keeps the old name, and `npm ci` then installs a package under a
# name nothing imports. The rename in #131 is why this case is here.
scratch renamed
edit "$work/renamed/packages/core/package.json" 'json.name = "@uniflowed/koru"'
run renamed
# On the name rather than on the new name alone: the missing link fires for a
# rename too, and an assertion that either message satisfies is an assertion
# that lets the name comparison be deleted.
refuses "a package the two files call by different names" "the lock calls it @uniflowed/core"

# --- a workspace that is gone, and a link that is missing --------------------
scratch removed
rm -rf "$work/removed/packages/cli"
run removed
refuses "an entry for a workspace that no longer exists" "packages/cli"

scratch unlinked
edit "$work/unlinked/package-lock.json" 'delete json.packages["node_modules/@uniflowed/core"]'
run unlinked
refuses "a package nothing links into node_modules" "node_modules/@uniflowed/core"

scratch mislinked
edit "$work/mislinked/package-lock.json" 'json.packages["node_modules/@uniflowed/core"].resolved = "packages/koru"'
run mislinked
refuses "a link that resolves somewhere else" "packages/koru"

# --- and the private workspace stays out of it -------------------------------
# `docs` is private, so npm records no version for it. A check that demanded
# one would fail on every clean checkout, which is how this rule was written
# the wrong way round first.
scratch private
edit "$work/private/docs/package.json" 'json.version = "9.9.9"'
run private
[ "$status" -eq 0 ] || fail "a private workspace's version was compared: $out"
pass "a private workspace's version is not compared, because npm does not record one"

echo "test-lockfile-in-sync: all checks passed"
