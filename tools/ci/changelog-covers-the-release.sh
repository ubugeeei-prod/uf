#!/bin/sh
# Fail when a released version's changelog section leaves a commit out.
#
# The section is written by `uf release <bump>` from the commits that exist
# when it runs, and then the release branch sits in the queue while more land.
# `uf@0.0.0-alpha.5` went out with twenty-two commits in it and twelve in its
# changelog: the ten that merged while the release was waiting for CI are in
# the tarball, on npm, and in nobody's notes.
#
# So the section is compared against the range its own tag names. The unit is
# the *commit*, not the pull request number in its subject. That distinction is
# the whole of #443: this check used to build the range's list by pulling
# `(#NNN)` out of each subject and dropping every subject that had none, so a
# commit GitHub did not stamp — the title was edited on the way in, or the
# merge was made another way — was not merely unmatched, it was not counted.
# `uf@0.0.0-alpha.8` shipped `docs: sharpen uf React hero copy` that way, and
# this check reported sixteen of sixteen and passed.
#
# A commit is covered when the section can be shown to be about it:
#
#   * with a pull request number, by that number appearing anywhere in the
#     section — how it is grouped and worded is a judgement, and *whether it is
#     there* is not;
#   * with no number, by its summary appearing in the section, which is what
#     `uf release` writes for it, or by its abbreviated hash. A commit with no
#     pull request number has exactly one durable name, and that is its hash;
#     a section that rewrites the generated line into prose says which commit
#     it rewrote by citing it.
#
# The release's own commits are exempt. They are made locally, so they carry no
# number until they are merged, and the section is what they contain: a section
# that had to cite the commit that writes it could never be written.
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

// A commit the release itself made. Local commits have no pull request number
// until they are merged, so without this rule every release would report its
// own two commits as uncited.
const isReleaseCommit = (subject) => subject.startsWith("chore(release):");

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const tags = new Set(git("tag", "--list", "uf@*").split("\n").filter(Boolean));

const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
const headings = [...changelog.matchAll(/^## uf@(\S+)$/gm)].map((match) => ({
  version: match[1],
  at: match.index,
}));

// Prose wraps, and a bullet's continuation line is indented. Comparing text
// against a section means comparing it against the words, not the line breaks
// somebody's editor chose.
const flatten = (text) => text.toLowerCase().replace(/\s+/g, " ");

// `type(scope)!: summary` -> `summary`, and anything else unchanged. The same
// reading `crates/uf_cli/src/changelog.rs` does, because the line this looks
// for in the section is the line that file wrote.
const summaryOf = (subject) =>
  /^[a-z]+(?:\([^)]*\))?!?: (.+)$/.exec(subject)?.[1] ?? subject;

// Whether `section` can be shown to be about a commit that carries no pull
// request number: it repeats the summary `uf release` wrote for it, or it
// cites the commit by an abbreviation of its hash.
const mentions = (section, { hash, subject }) => {
  if (flatten(section).includes(flatten(summaryOf(subject)))) return true;
  return [...section.matchAll(/\b[0-9a-fA-F]{7,40}\b/g)].some((match) =>
    hash.startsWith(match[0].toLowerCase()),
  );
};

const problems = [];
let numbered = 0;
let unnumbered = 0;

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

  const commits = git("log", "--format=%H%x09%s", `uf@${previous.version}..${tag}`)
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const tab = line.indexOf("\t");
      const subject = line.slice(tab + 1);
      return { hash: line.slice(0, tab), subject, pull: /\(#(\d+)\)$/.exec(subject)?.[1] };
    })
    .filter(({ subject }) => !isReleaseCommit(subject));

  for (const commit of commits) {
    if (commit.pull != null) {
      numbered += 1;
      if (!named.has(commit.pull)) {
        problems.push(`${tag} does not mention: ${commit.subject}`);
      }
      continue;
    }
    unnumbered += 1;
    if (!mentions(section, commit)) {
      const short = commit.hash.slice(0, 7);
      problems.push(
        `${tag} does not mention, and it carries no pull request number: ${short} ${commit.subject}`,
      );
    }
  }
}

if (problems.length > 0) {
  console.error("a released changelog section leaves out what the release contains.");
  for (const problem of problems) console.error(`  ${problem}`);
  console.error("");
  console.error("Add them to the section. `git log <previous tag>..<tag>` is the list.");
  console.error(
    "A commit with no `(#NNN)` is cited by the summary `uf release` wrote for it,",
  );
  console.error("or by its abbreviated hash, which is the only other name it has.");
  process.exit(1);
}

const checked = headings.filter((h) => tags.has(`uf@${h.version}`) && h.version >= FROM);
console.log(
  `every commit in ${checked.length} released version(s) is named in the changelog ` +
    `(${numbered} by pull request number, ${unnumbered} with no number)`,
);
EOF
