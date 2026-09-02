#!/bin/sh
# Merge the per-target manifests a release produced into the single
# `manifest.json` that setup.uniflowed.dev/metadata/latest.json serves.
set -eu

if [ "$#" -lt 1 ]; then
  echo "usage: $0 <release-dir>" >&2
  exit 1
fi

release_dir="$1"
[ -d "$release_dir" ] || {
  echo "build-manifest: no such directory: $release_dir" >&2
  exit 1
}

version="$(tr -d '[:space:]' < "${release_dir}/VERSION")"
[ -n "$version" ] || {
  echo "build-manifest: VERSION in ${release_dir} is empty" >&2
  exit 1
}

node -e '
const fs = require("node:fs");
const path = require("node:path");

const [dir, version] = process.argv.slice(1);
const names = fs.readdirSync(dir)
  .filter((n) => /^manifest-.+\.json$/.test(n))
  .sort();
if (names.length === 0) {
  console.error(`build-manifest: no manifest-*.json in ${dir}`);
  process.exit(1);
}

const targets = names.map((name) => {
  const entry = JSON.parse(fs.readFileSync(path.join(dir, name), "utf8"));
  for (const field of ["target", "archive", "sha256", "version"]) {
    if (!entry[field]) {
      console.error(`build-manifest: ${name} is missing ${field}`);
      process.exit(1);
    }
  }
  // A merged manifest that mixes versions would hand users a checksum for one
  // release and an archive from another.
  if (entry.version !== version) {
    console.error(
      `build-manifest: ${name} is version ${entry.version}, expected ${version}`,
    );
    process.exit(1);
  }
  // The archive the checksum describes has to be sitting next to it.
  if (!fs.existsSync(path.join(dir, entry.archive))) {
    console.error(`build-manifest: ${name} references a missing ${entry.archive}`);
    process.exit(1);
  }
  return {
    target: entry.target,
    archive: entry.archive,
    sha256: entry.sha256,
  };
});

const seen = new Set();
for (const { target } of targets) {
  if (seen.has(target)) {
    console.error(`build-manifest: duplicate target ${target}`);
    process.exit(1);
  }
  seen.add(target);
}

const manifest = { name: "uf", version, targets };
fs.writeFileSync(
  path.join(dir, "manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
' "$release_dir" "$version"

echo "${release_dir}/manifest.json"
