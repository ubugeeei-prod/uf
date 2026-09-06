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

# The changelog is written first, by `uf release <bump>`, which computes the
# next version from the workspace it is *about to* leave. Run it after the bump
# and it plans one version too far — the section comes out headed alpha.6 while
# the tree says alpha.5, and the mistake is invisible until somebody reads the
# release. So the order is checked rather than remembered.
# `-Fx`, not a pattern: a version is full of dots, and as a regular expression
# a dot matches anything. `## uf@0x2y0` satisfied the check for `0.2.0`, which
# is a guard that passes on the one file it exists to read.
if [ -f CHANGELOG.md ] && ! grep -Fqx "## uf@$version" CHANGELOG.md; then
  echo "CHANGELOG.md has no section for uf@$version." >&2
  echo "Run \`uf release <bump>\` first: it writes the section for the version" >&2
  echo "after the one the workspace is on, which is the one you are bumping to." >&2
  exit 2
fi

node - "$version" <<'EOF'
const fs = require("node:fs");
const path = require("node:path");
const version = process.argv[2];

// Cargo: only the workspace version; crates inherit it.
const cargo = "Cargo.toml";
const toml = fs.readFileSync(cargo, "utf8");
const current = toml.match(/^version = "([^"]+)"$/m);
// Not "the replacement changed nothing": running the bump twice for the same
// version is a reasonable thing to do — a release that failed halfway is
// finished by running it again — and reporting "version not found" for a file
// that has it is a message that sends the reader to the wrong place.
if (current == null) throw new Error("workspace version not found in Cargo.toml");
fs.writeFileSync(cargo, toml.replace(/^version = "[^"]+"$/m, `version = "${version}"`));

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
