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

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/** Directories that are not this repository's prose. */
const SKIP = new Set(["node_modules", "dist", ".uf", "target", "upstream", ".git", "coverage"]);

/** Every Markdown and MDX file the repository owns. */
function documents(): Array<string> {
  const found = [];
  const walk = (dir: string) => {
    for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
      if (item.isDirectory()) {
        if (!SKIP.has(item.name)) {
          walk(path.join(dir, item.name));
        }
      } else if (item.name.endsWith(".md") || item.name.endsWith(".mdx")) {
        found.push(path.join(dir, item.name));
      }
    }
  };
  walk(REPO);
  return found;
}

/** `| --- | --- |`, in any of the alignment spellings. */
const DELIMITER = /^\|[\s:|-]+\|\s*$/;

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
    if (line.trimStart().startsWith("```")) {
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
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
