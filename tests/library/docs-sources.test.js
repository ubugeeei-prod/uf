// @flow
//
// A sample that names its source is a verbatim excerpt of that source.
//
// `tools/docs/snippets.js` holds every sample in the manual to *parsing*. That
// is as far as a sample that is only prose can be held. A sample that is a
// piece of a file CI runs can be held further: to *being* that piece. The guide
// that shows how to test a server action is only worth reading if the test it
// shows is one that passes, so its fences say where they come from:
//
//   ```js source=crates/uf_cli/tests/fixtures/rsc-test-app/tests/notes-actions.test.js
//
// and this check fails the moment the file and the page disagree — a renamed
// helper, a changed assertion, a line reformatted by `uf fmt`. The fix is to
// copy the file's text back into the page, never to loosen the page.
//
// "Excerpt" rather than "the whole file": a page shows the part that makes
// its point. Every line of the fence must appear in the file, as one
// contiguous run, with only trailing whitespace ignored.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import { type Fence, fences } from "../../tools/docs/snippets.js";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const PAGES = path.join(REPO, "docs/app");

/** The path a fence's info string names with `source=`, or `null`. */
function sourceOf(fence: Fence): string | null {
  const named = /(?:^|\s)source=(\S+)/.exec(fence.meta);
  return named == null ? null : named[1];
}

/** Whether `excerpt` is a contiguous run of `file`'s lines, trailing blanks aside. */
function isExcerpt(excerpt: string, file: string): boolean {
  const lines = (text: string) => text.split("\n").map((line) => line.replace(/\s+$/, ""));
  // Blank lines at either end are the fence's, not the file's; indentation is kept.
  const wanted = lines(excerpt.replace(/^\s*\n/, "").replace(/\n\s*$/, ""));
  const haystack = lines(file);
  for (let start = 0; start + wanted.length <= haystack.length; start += 1) {
    if (wanted.every((line, offset) => haystack[start + offset] === line)) {
      return true;
    }
  }
  return false;
}

function pagesUnder(dir: string): Array<string> {
  const found = [];
  for (const name of fs.readdirSync(dir).map(String)) {
    const full = path.join(dir, name);
    if (fs.statSync(full).isDirectory()) {
      found.push(...pagesUnder(full));
    } else if (name.endsWith(".mdx")) {
      found.push(full);
    }
  }
  return found.sort();
}

describe("isExcerpt", () => {
  const file = "a\nb  \nc\nd\n";

  it("accepts a contiguous run of lines, trailing whitespace aside", () => {
    expect(isExcerpt("b\nc", file)).toBe(true);
    expect(isExcerpt("a\nb\nc\nd", file)).toBe(true);
  });

  it("refuses lines that are there but not together, or not there at all", () => {
    expect(isExcerpt("a\nc", file)).toBe(false);
    expect(isExcerpt("b\nC", file)).toBe(false);
    expect(isExcerpt("  b", "  a\n  b\n")).toBe(true);
    expect(isExcerpt("b", "  a\n  b\n")).toBe(false);
  });
});

describe("every sample that names its source", () => {
  const named = pagesUnder(PAGES).flatMap((page) =>
    fences(path.relative(REPO, page), fs.readFileSync(page, "utf8")).filter(
      (fence) => sourceOf(fence) != null,
    ),
  );

  it("exists, so the check is checking something", () => {
    expect(named.length).toBeGreaterThan(0);
  });

  it("is a verbatim excerpt of the file it names", () => {
    const wrong = named
      .filter((fence) => {
        const source = path.join(REPO, sourceOf(fence) ?? "");
        return !fs.existsSync(source) || !isExcerpt(fence.code, fs.readFileSync(source, "utf8"));
      })
      .map((fence) => `${fence.page}:${String(fence.line)} ≠ ${sourceOf(fence) ?? ""}`);
    expect(wrong).toEqual([]);
  });
});
