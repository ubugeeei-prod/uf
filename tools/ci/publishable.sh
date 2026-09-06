#!/bin/sh
# Fail when a package with real code behind it is not on its way to npm.
#
# `packages/` holds two kinds of directory. Most are declaration modules whose
# functions call `nativeRuntimeRequired(...)` — a contract with nothing behind
# it, and publishing one squats a name that cannot run. The rest are libraries
# somebody wrote, and every one of those has to reach a user somehow, or the
# work is done and nobody can have it.
#
# Ten of them could not. `@uniflowed/state`, `@uniflowed/effect`,
# `@uniflowed/form`, `@uniflowed/immer` and six more — about 22,000 lines of
# Flow between them, and `npm install @uniflowed/state` answered `ETARGET`.
# Nothing said so: `published-packages.txt` is a list somebody adds to, and a
# list somebody adds to is a list somebody forgets.
#
# So the rule is stated from the other side. A package that never calls
# `nativeRuntimeRequired` is a real implementation, and it must be named in
# `published-packages.txt` or in `pending-packages.txt` — the second being
# "implemented, and waiting on the one step a checkout cannot take", which is
# `npm trust` against a name the registry does not have yet. Being in neither
# is the case this refuses.
#
# The second rule is the same failure reached from the other direction. A
# package that *is* on npm must not depend on one that is not — whether it is
# waiting on `npm trust` or is a declaration that will never be published at
# all. The tarball names a version the registry does not have, so
# `npm install` answers `ETARGET` for a package that installs perfectly well
# from this workspace, which is exactly the blind spot #409 lived in.
#
# No published package does this today, and until ubugeeei-prod/uf#318 nothing said it must
# not. That issue declined a shared Web Storage helper in `@uniflowed/web`
# precisely because `@uniflowed/hooks` is published and `@uniflowed/web` is
# not, and an invariant an argument rests on is worth more as a check than as
# a paragraph.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

node - <<'EOF'
const fs = require("node:fs");

const names = (file) =>
  fs
    .readFileSync(file, "utf8")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "" && !line.startsWith("#"));

const published = names("tools/release/published-packages.txt");
const pending = names("tools/release/pending-packages.txt");

// A *call*, not a mention: `@uniflowed/story`'s header explains that it used
// to return `nativeRuntimeRequired(…)` and does not any more, and a package
// that says so in prose is exactly the kind this check must not excuse.
const CALL = /(?<![A-Za-z0-9_$.])nativeRuntimeRequired\s*\(/;

const modules = function* (directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) yield* modules(path);
    else if (entry.name.endsWith(".js")) yield path;
  }
};

/** Whether every function this package exports needs a runtime it does not have. */
const isDeclaration = (name) => {
  for (const file of modules(`packages/${name}`)) {
    for (const line of fs.readFileSync(file, "utf8").split("\n")) {
      const code = line.trimStart();
      // Comment lines only. A call inside a string would count, and that is the
      // safe direction: it means "declaration", which needs no publishing —
      // wrong, and quiet, rather than wrong and blocking a release.
      if (code.startsWith("//") || code.startsWith("*") || code.startsWith("/*")) continue;
      if (CALL.test(line)) return true;
    }
  }
  return false;
};

const directories = fs
  .readdirSync("packages", { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

const problems = [];

for (const name of directories) {
  if (isDeclaration(name)) continue;
  if (published.includes(name) || pending.includes(name)) continue;
  problems.push(
    `packages/${name} is implemented and is in neither list. Add it to ` +
      "tools/release/published-packages.txt if `npm trust` has bound it, and to " +
      "tools/release/pending-packages.txt if it has not.",
  );
}

for (const name of pending) {
  if (published.includes(name)) {
    problems.push(`${name} is in both lists; it is published, so take it out of the pending one.`);
  }
}

for (const [file, list] of [
  ["published-packages.txt", published],
  ["pending-packages.txt", pending],
]) {
  for (const name of list) {
    if (!fs.existsSync(`packages/${name}/package.json`)) {
      problems.push(`${file} names ${name}, and packages/${name} does not exist.`);
    }
  }
}

/** The `@uniflowed/*` a package declares, as directory names under `packages/`. */
const uniflowedDependencies = (name) => {
  const manifest = JSON.parse(fs.readFileSync(`packages/${name}/package.json`, "utf8"));
  const out = new Set();
  // `devDependencies` are deliberately not read: they are not installed for a
  // consumer, so a published package may depend on an unpublished one there.
  for (const field of ["dependencies", "peerDependencies", "optionalDependencies"]) {
    for (const dependency of Object.keys(manifest[field] ?? {})) {
      if (dependency.startsWith("@uniflowed/")) out.add(dependency.slice("@uniflowed/".length));
    }
  }
  return out;
};

// A name on npm may not need one that is not. Stated over "is it published"
// rather than over "is it pending", because a declaration package is in
// neither list and is deliberately never published either — depending on one
// of those is the same ETARGET by a different route.
for (const name of published) {
  if (!fs.existsSync(`packages/${name}/package.json`)) continue;
  for (const dependency of uniflowedDependencies(name)) {
    if (published.includes(dependency)) continue;
    const why = pending.includes(dependency)
      ? "is implemented and waiting on `npm trust`"
      : "is not in either release manifest";
    problems.push(
      `@uniflowed/${name} is published and depends on @uniflowed/${dependency}, which ` +
        `${why}: \`npm install @uniflowed/${name}\` would answer ETARGET. Bind ` +
        `${dependency} first, or keep the dependency out of ${name}.`,
    );
  }
}

if (problems.length > 0) {
  console.error("the release manifests do not describe packages/.");
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}

const implemented = directories.filter((name) => !isDeclaration(name));
// Counted over the implemented ones rather than over the lists, because
// `published-packages.txt` also holds packages that are part native —
// `@uniflowed/core` and `@uniflowed/stylex` both call `nativeRuntimeRequired`
// for the half of themselves that needs the binary, and belong on npm anyway.
const shipped = implemented.filter((name) => published.includes(name)).length;
console.log(
  `${implemented.length} implemented packages: ${shipped} published, ${pending.length} waiting on npm trust`,
);
EOF
