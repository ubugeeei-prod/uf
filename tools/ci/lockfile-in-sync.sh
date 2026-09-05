#!/bin/sh
# Fail when `package-lock.json` no longer describes the workspace manifests.
#
# `npm ci` refuses a lock that disagrees with the manifests, and it refuses it
# in every job that installs — six of them went red at once when a new package
# was added and the lock was not regenerated, all with the same message about a
# package none of those jobs is about. This says it once, in the job whose
# subject is the manifests, and says what to run.
#
# `--package-lock-only` writes the lock and touches nothing else, so the check
# is "regenerating it changes nothing".
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

# Beside the lock rather than in the system temp: a restricted CI image may
# refuse `mktemp` there, and this file is deleted either way.
before="package-lock.json.before"
trap 'rm -f "$before"' EXIT
cp package-lock.json "$before"

npm install --package-lock-only --no-audit --no-fund >/dev/null

if ! diff -q "$before" package-lock.json >/dev/null; then
  cp "$before" package-lock.json
  echo "package-lock.json does not match the workspace manifests." >&2
  echo "Run: npm install --package-lock-only --no-audit --no-fund" >&2
  exit 1
fi

echo "package-lock.json matches the workspace manifests"
