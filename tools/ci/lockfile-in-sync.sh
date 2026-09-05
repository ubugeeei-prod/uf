#!/bin/sh
# Fail when `package-lock.json` no longer describes the workspace manifests.
#
# `npm ci` refuses a lock that disagrees with the manifests, and it refuses it
# in every job that installs — six of them went red at once when a new package
# was added and the lock was not regenerated, all with the same message about a
# package none of those jobs is about. This says it once, in the job whose
# subject is the manifests, and says which manifest and which field.
#
# It reads the lock rather than regenerating it. The first version of this
# check ran `npm install --package-lock-only` and asked whether the file
# changed, which is a different question: it also asks whether today's npm,
# resolving today's registry on this platform, would write the same bytes.
# That version passed on the machine it was written on, under two npm builds
# and a cold cache, and failed in CI — and a check whose failure cannot be
# reproduced where the fix has to be made is not a check anyone can act on.
# The property worth enforcing is the one `npm ci` enforces, and it is
# decidable from the two files alone.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

node - <<'EOF'
const fs = require("node:fs");

const lock = JSON.parse(fs.readFileSync("package-lock.json", "utf8"));
const root = JSON.parse(fs.readFileSync("package.json", "utf8"));
const entries = lock.packages ?? {};

const problems = [];
const note = (where, what) => problems.push(`${where}: ${what}`);

// The workspace directories, from the root manifest's own globs, so a new
// `packages/*` is in scope the moment it exists rather than when someone
// remembers to add it here.
const directories = (root.workspaces ?? [])
  .flatMap((pattern) => (pattern.includes("*") ? fs.globSync(pattern) : [pattern]))
  .filter((dir) => fs.existsSync(`${dir}/package.json`))
  .sort();

const FIELDS = ["dependencies", "peerDependencies", "devDependencies", "optionalDependencies"];

for (const dir of directories) {
  const manifest = JSON.parse(fs.readFileSync(`${dir}/package.json`, "utf8"));
  const entry = entries[dir];
  if (entry == null) {
    note(dir, "the lock has no entry for this workspace");
    continue;
  }

  if (entry.name !== manifest.name) {
    note(dir, `the lock calls it ${entry.name ?? "nothing"}, the manifest calls it ${manifest.name}`);
  }

  // npm records a version for a package it could publish and omits it for a
  // private one, so the lock having none is only wrong when the manifest is
  // publishable — which is every `packages/*`, and the case a release bump
  // that forgot the lock would land in.
  if (manifest.private !== true && entry.version !== manifest.version) {
    note(dir, `the lock says ${entry.version ?? "no version"}, the manifest says ${manifest.version}`);
  }

  for (const field of FIELDS) {
    const wanted = manifest[field] ?? {};
    const locked = entry[field] ?? {};
    for (const [name, range] of Object.entries(wanted)) {
      if (!(name in locked)) note(dir, `${field}.${name} is in the manifest and not in the lock`);
      else if (locked[name] !== range) {
        note(dir, `${field}.${name} is ${locked[name]} in the lock and ${range} in the manifest`);
      }
    }
    for (const name of Object.keys(locked)) {
      if (!(name in wanted)) note(dir, `${field}.${name} is in the lock and not in the manifest`);
    }
  }

  // The link is how everything else in the tree resolves the package: without
  // it a sibling that depends on it reaches for the registry, where an
  // unreleased version does not exist.
  const link = entries[`node_modules/${manifest.name}`];
  if (link == null) note(dir, `nothing links node_modules/${manifest.name} to it`);
  else if (link.resolved !== dir) {
    note(dir, `node_modules/${manifest.name} resolves to ${link.resolved ?? "nothing"}`);
  }
}

// And the other direction: a package that was renamed or deleted leaves an
// entry behind, which `npm ci` installs and nobody maintains.
const known = new Set(directories);
for (const key of Object.keys(entries)) {
  if (key === "" || key.startsWith("node_modules/") || known.has(key)) continue;
  note(key, "the lock has an entry for a workspace that does not exist");
}

if (problems.length > 0) {
  console.error("package-lock.json does not match the workspace manifests.");
  for (const problem of problems) console.error(`  ${problem}`);
  console.error("Run: npm install --package-lock-only --no-audit --no-fund");
  process.exit(1);
}

console.log(`package-lock.json matches all ${directories.length} workspace manifests`);
EOF
