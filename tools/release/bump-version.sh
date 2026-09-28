#!/usr/bin/env sh
# Set every version in the repository to one value.
#
# Releases go through `uf run release -- <bump>`. It calls this internal
# helper after writing the changelog. This file owns the synchronized version
# fields in Cargo and npm manifests:
#
#   tools/release/bump-version.sh 0.0.0-alpha.2
#
# It rewrites the Cargo workspace version (every crate inherits it), every
# `npm/*/package.json` (its own version and its `@uniflowed/*`
# dependencies, which are pinned exactly so a release is internally
# consistent), the docs site's manifest, the `@uniflowed/*` pins in
# `examples/*/package.json` (and a `uf: "<version>"` pin in an example's
# `uf.config.js`), and then refreshes `Cargo.lock` and `package-lock.json` so
# `--locked` and `npm ci` stay green.
#
# An example that pins a published version is one a reader copies out of the
# repository, so it should name the release it ships with rather than whatever
# release somebody last remembered to move it to. `file:`, `link:` and
# `workspace:` specs point at this checkout and are left alone.
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

# And that the version is one that has not gone out yet.
#
# `uf release` refuses to rewrite the changelog section of a version that is
# already tagged, because the version it plans comes from the binary running it
# and an old binary plans a version the tree has already published (#457). This
# is the same refusal one step later, where the mistake is cheaper to see: a
# `uf@<version>` tag is a release that was built from a tree carrying that
# version, so *moving* the workspace onto it is either that same old binary's
# idea of "next" or a typo, and the command after this one in the runbook is
# `git tag`, which would fail once every file in the repository had already been
# rewritten.
#
# The workspace being on that version already is a different thing and stays a
# no-op that succeeds: that is how a release which failed halfway is finished,
# and by then it may well have tagged. So only a move is refused.
current="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml 2>/dev/null | head -1)"
if [ "$current" != "$version" ] &&
  git rev-parse --verify --quiet "refs/tags/uf@$version" >/dev/null 2>&1; then
  echo "uf@$version is already tagged: that release went out." >&2
  echo "The workspace is on $current, so this would move it onto a version that" >&2
  echo "has been published. If \`uf release\` planned $version, the binary that" >&2
  echo "planned it is older than this tree — build uf from this tree and run it" >&2
  echo "again. If the tag is wrong, delete it before bumping onto it." >&2
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
  ...fs.globSync("npm/*/package.json"),
  "docs/package.json",
];
for (const file of manifests) {
  const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
  if (file.startsWith("npm/")) manifest.version = version;
  for (const field of ["dependencies", "peerDependencies", "devDependencies", "optionalDependencies"]) {
    for (const name of Object.keys(manifest[field] ?? {})) {
      if (name.startsWith("@uniflowed/")) manifest[field][name] = version;
    }
  }
  fs.writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`${path.relative(".", file)} -> ${version}`);
}

// Examples: only the published pins. A `file:`, `link:` or `workspace:` spec
// is this checkout, and rewriting it to a version would point the example at
// npm instead. Edited in place rather than re-serialized, so an example
// manifest keeps whatever layout it was written in.
const local = /^(file|link|workspace):/;
for (const file of fs.globSync("examples/*/package.json")) {
  const source = fs.readFileSync(file, "utf8");
  const manifest = JSON.parse(source);
  const pinned = new Set();
  for (const field of ["dependencies", "peerDependencies", "devDependencies", "optionalDependencies"]) {
    for (const [name, spec] of Object.entries(manifest[field] ?? {})) {
      if (name.startsWith("@uniflowed/") && !local.test(spec)) pinned.add(name);
    }
  }
  if (pinned.size === 0) continue;
  const next = source.replace(/("(@uniflowed\/[^"]+)"\s*:\s*)"[^"]*"/g, (whole, key, name) =>
    pinned.has(name) ? `${key}"${version}"` : whole,
  );
  if (next !== source) fs.writeFileSync(file, next);
  console.log(`${path.relative(".", file)} -> ${version}`);
}

// An example's config can pin the toolchain it runs under. The same rule: a
// version string moves, anything else is left for a person to read.
for (const file of fs.globSync("examples/*/uf.config.{js,mjs,cjs,ts}")) {
  const source = fs.readFileSync(file, "utf8");
  const next = source.replace(
    /(^|[\s{,])(uf\s*:\s*)(["'])\d+\.\d+\.\d+[^"']*\3/gm,
    (_, lead, key, quote) => `${lead}${key}${quote}${version}${quote}`,
  );
  if (next === source) continue;
  fs.writeFileSync(file, next);
  console.log(`${path.relative(".", file)} -> ${version}`);
}
EOF

cargo metadata --format-version 1 >/dev/null
npm install --package-lock-only --no-audit --no-fund >/dev/null
echo "Cargo.lock and package-lock.json refreshed"
echo "next: validate and merge the release PR; the release command publishes after its checks pass"
