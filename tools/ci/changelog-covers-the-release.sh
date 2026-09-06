#!/bin/sh
# Fail when a released version's changelog section leaves a pull request out.
#
# The section is written by `uf release <bump>` from the commits that exist
# when it runs, and then the release branch sits in the queue while more land.
# `uf@0.0.0-alpha.5` went out with twenty-two commits in it and twelve in its
# changelog: the ten that merged while the release was waiting for CI are in
# the tarball, on npm, and in nobody's notes.
#
# So the section is compared against the range its own tag names. Every pull
# request merged between the previous tag and this one has to be mentioned
# somewhere in the section — by number, anywhere, because how it is grouped and
# worded is a judgement and *whether it is there* is not.
#
# The release's own commit is not required to name itself.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

node - <<'EOF'
const fs = require("node:fs");
const { execFileSync } = require("node:child_process");

// Where this check begins, and it only ever moves forward.
//
// `uf@0.0.0-alpha.3` and earlier were written before this existed, and
// bringing them up to it is archaeology rather than a guarantee — alpha.3's
// section names 40 of the 130 pull requests in its range, and its tag is not
// even on `main`, which is the defect `tools/release/tag-release` now prevents.
// Nothing is gained by rewriting them and something is lost by pretending they
// were checked.
const FROM = "0.0.0-alpha.4";

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const tags = new Set(git("tag", "--list", "uf@*").split("\n").filter(Boolean));

const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
const headings = [...changelog.matchAll(/^## uf@(\S+)$/gm)].map((match) => ({
  version: match[1],
  at: match.index,
}));

const problems = [];

for (const [index, heading] of headings.entries()) {
  const tag = `uf@${heading.version}`;
  // An unreleased section is the one being written; there is nothing to
  // compare it against yet.
  if (!tags.has(tag)) continue;
  if (heading.version < FROM) continue;

  const previous = headings[index + 1];
  if (previous == null || !tags.has(`uf@${previous.version}`)) {
    problems.push(`${tag} has no previous tag in the changelog to measure from`);
    continue;
  }

  const end = index + 1 < headings.length ? headings[index + 1].at : changelog.length;
  const section = changelog.slice(heading.at, end);
  const named = new Set([...section.matchAll(/#(\d+)/g)].map((match) => match[1]));

  const merged = git("log", "--format=%s", `uf@${previous.version}..${tag}`)
    .split("\n")
    .map((subject) => ({ subject, pull: /\(#(\d+)\)$/.exec(subject)?.[1] }))
    .filter(({ pull }) => pull != null)
    // The release commit is the section itself; it does not cite itself.
    .filter(({ subject }) => !subject.startsWith("chore(release):"));

  const missing = merged.filter(({ pull }) => !named.has(pull));
  for (const { subject } of missing) {
    problems.push(`${tag} does not mention: ${subject}`);
  }
}

if (problems.length > 0) {
  console.error("a released changelog section leaves out what the release contains.");
  for (const problem of problems) console.error(`  ${problem}`);
  console.error("");
  console.error("Add them to the section. `git log <previous tag>..<tag>` is the list.");
  process.exit(1);
}

const checked = headings.filter((h) => tags.has(`uf@${h.version}`) && h.version >= FROM);
console.log(
  `every pull request in ${checked.length} released version(s) is named in the changelog`,
);
EOF
