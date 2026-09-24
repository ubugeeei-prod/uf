// @flow
//
// Every Markdown table row in the repository is inside a table.
//
// `docs/security.md` is a document made of tables: a row is a threat, the
// decision that answers it, and the test that holds the decision — and its own
// preamble says a row whose test does not exist yet is marked `todo` and is a
// work item rather than a claim. So a row that has fallen out of its table is
// not a formatting nit. It renders as a line of text with pipes in it, the
// header that gave its three cells their meaning is gone, and the work item
// stops being visible as one.
//
// That is what happened: #473 inserted two `###` prose sections into the
// middle of the "Framework, RSC, and server actions" table, and the three rows
// after them were left below the prose — orphaned, and one of them a stale
// duplicate of a row the table already carried in its finished form.
//
// The check is structural rather than about that document: a run of lines
// beginning with `|` has to open with a header row and a delimiter row.
// Fenced code blocks are skipped, because a fence may legitimately contain a
// pipe table as an example of one.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import { documents as ownedDocuments } from "../../tools/docs/trees.js";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/**
 * Every Markdown and MDX file the repository owns — the same list, and the
 * same answer to "owns", as the tree check's; see `documents` there.
 */
function documents(): Array<string> {
  return ownedDocuments(REPO);
}

/**
 * `| --- | --- |`, in any of the alignment spellings.
 *
 * Every cell has to contain a hyphen: a character class alone would accept
 * `| : |` and `|   |`, which GitHub does not render as a table at all — so the
 * check would call a run of pipes a table and pass over exactly the shape it
 * exists to catch.
 */
const DELIMITER = /^\|(?:\s*:?-+:?\s*\|)+\s*$/;

/**
 * Every run of table rows in one file that does not open with a header and a
 * delimiter, as `{ line, text }` — the line number of the run's first row and
 * enough of it to recognise.
 */
function orphanedRuns(file: string): Array<{ line: number, text: string }> {
  const lines = fs.readFileSync(file, "utf8").split("\n");
  const orphaned = [];
  let run: Array<string> = [];
  let startedAt = 0;
  let fenced = false;
  const close = () => {
    if (run.length > 0 && (run.length < 2 || !DELIMITER.test(run[1]))) {
      orphaned.push({ line: startedAt, text: run[0].slice(0, 80) });
    }
    run = [];
  };
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    // Both fence spellings. CommonMark allows `~~~` as well as backticks, and
    // a document that used it would have its examples read as real tables.
    if (line.trimStart().startsWith("```") || line.trimStart().startsWith("~~~")) {
      close();
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      continue;
    }
    if (line.startsWith("|")) {
      if (run.length === 0) {
        startedAt = index + 1;
      }
      run.push(line);
    } else {
      close();
    }
  }
  close();
  return orphaned;
}

const INSTALL_COMMAND = /\b(?:npm\s+(?:install|i)|pnpm\s+add|yarn\s+add|bun\s+add)\b/g;
const UNTAGGED_UNIFLOWED_PACKAGE = /@uniflowed\/[a-z0-9-]+(?![a-z0-9-])(?!@)/g;

/** Untagged manual installs of packages that currently publish only alpha releases. */
function untaggedInstalls(file: string): Array<{ line: number, text: string }> {
  return fs
    .readFileSync(file, "utf8")
    .split("\n")
    .flatMap((line, index) => {
      if (!INSTALL_COMMAND.test(line)) {
        INSTALL_COMMAND.lastIndex = 0;
        return [];
      }
      INSTALL_COMMAND.lastIndex = 0;
      const found = [...line.matchAll(UNTAGGED_UNIFLOWED_PACKAGE)];
      UNTAGGED_UNIFLOWED_PACKAGE.lastIndex = 0;
      return found.length === 0 ? [] : [{ line: index + 1, text: line.trim() }];
    });
}

describe("the repository's Markdown tables", () => {
  it("has some to check", () => {
    // A walk that found nothing would pass every case below without looking at
    // anything, which is the failure mode a structural check like this has.
    expect(documents().length).toBeGreaterThan(20);
  });

  it("opens every run of table rows with a header and a delimiter", () => {
    const orphaned = [];
    for (const file of documents()) {
      for (const run of orphanedRuns(file)) {
        orphaned.push(`${path.relative(REPO, file)}:${run.line} ${run.text}`);
      }
    }
    expect(orphaned).toEqual([]);
  });

  it("does not suggest untagged installs of prerelease @uniflowed packages", () => {
    const untagged = [];
    for (const file of documents()) {
      for (const install of untaggedInstalls(file)) {
        untagged.push(`${path.relative(REPO, file)}:${install.line} ${install.text}`);
      }
    }
    expect(untagged).toEqual([]);
  });

  it("recognises an orphan when there is one", () => {
    // The check has to be able to fail, and the document that motivated it is
    // fixed — so the positive case is synthetic. It is written outside the
    // repository on purpose: a fixture with an orphaned row *in* `docs/` would
    // be found by the case above, and a run killed between writing and
    // removing it would leave the suite failing for a file it wrote itself.
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uf-docs-tables-"));
    const file = path.join(dir, "orphan.md");
    fs.writeFileSync(
      file,
      ["| a | b |", "| --- | --- |", "| 1 | 2 |", "", "prose", "", "| 3 | 4 |", ""].join("\n"),
    );
    try {
      expect(orphanedRuns(file)).toEqual([{ line: 7, text: "| 3 | 4 |" }]);
      // And a fenced table is not one: a code block may show a table as an
      // example of one.
      const fenced = path.join(dir, "fenced.md");
      fs.writeFileSync(fenced, ["```md", "| 3 | 4 |", "```", ""].join("\n"));
      expect(orphanedRuns(fenced)).toEqual([]);
      const tilde = path.join(dir, "tilde.md");
      fs.writeFileSync(tilde, ["~~~md", "| 3 | 4 |", "~~~", ""].join("\n"));
      expect(orphanedRuns(tilde)).toEqual([]);
      // And a delimiter row with no hyphen is not a delimiter row: GitHub
      // renders this as three lines of text, not a table.
      const colonly = path.join(dir, "colon.md");
      fs.writeFileSync(colonly, ["| a | b |", "| : | : |", "| 1 | 2 |", ""].join("\n"));
      expect(orphanedRuns(colonly)).toEqual([{ line: 1, text: "| a | b |" }]);
      const tagged = path.join(dir, "tagged.md");
      fs.writeFileSync(tagged, "npm install @uniflowed/ui@alpha\n");
      expect(untaggedInstalls(tagged)).toEqual([]);
      const untagged = path.join(dir, "untagged.md");
      fs.writeFileSync(untagged, "npm install @uniflowed/ui\n");
      expect(untaggedInstalls(untagged)).toEqual([{ line: 1, text: "npm install @uniflowed/ui" }]);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
