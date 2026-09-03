#!/usr/bin/env sh
# Set every version in the repository to one value.
#
# A release is one tag, `uf@<version>`, and `release.yml` refuses to build a
# tag whose version disagrees with the workspace. This script is the one place
# that knows every file carrying the version, so a bump is a single command:
#
#   tools/release/bump-version.sh 0.0.0-alpha.2
#
# It rewrites the Cargo workspace version (every crate inherits it), every
# `packages/*/package.json` (its own version and its `@uniflowed/*`
# dependencies, which are pinned exactly so a release is internally
# consistent), the docs site's manifest, and then refreshes `Cargo.lock` and
# `package-lock.json` so `--locked` and `npm ci` stay green.
set -eu

version="${1:-}"
if [ -z "$version" ]; then
  echo "usage: tools/release/bump-version.sh <version>" >&2
  exit 2
fi
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "not a semantic version: $version" >&2; exit 2 ;;
esac

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

node - "$version" <<'EOF'
const fs = require("node:fs");
const path = require("node:path");
const version = process.argv[2];

// Cargo: only the workspace version; crates inherit it.
const cargo = "Cargo.toml";
const toml = fs.readFileSync(cargo, "utf8");
const bumped = toml.replace(/^version = "[^"]+"$/m, `version = "${version}"`);
if (bumped === toml) throw new Error("workspace version not found in Cargo.toml");
fs.writeFileSync(cargo, bumped);

// npm: every shipped package, and every manifest that depends on one.
const manifests = [
  ...fs.globSync("packages/*/package.json"),
  "docs/package.json",
];
for (const file of manifests) {
  const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
  if (file.startsWith("packages/")) manifest.version = version;
  for (const field of ["dependencies", "peerDependencies", "devDependencies", "optionalDependencies"]) {
    for (const name of Object.keys(manifest[field] ?? {})) {
      if (name.startsWith("@uniflowed/")) manifest[field][name] = version;
    }
  }
  fs.writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`${path.relative(".", file)} -> ${version}`);
}
EOF

cargo metadata --format-version 1 >/dev/null
npm install --package-lock-only --no-audit --no-fund >/dev/null
echo "Cargo.lock and package-lock.json refreshed"
echo "next: commit, then \`git tag uf@${version} && git push origin uf@${version}\`"
