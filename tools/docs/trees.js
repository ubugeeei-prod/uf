// @flow
//
// Every directory tree the documentation draws is sorted.
//
//   node --import @uniflowed/host/register tools/docs/trees.js
//
// A route file is spelled `$page.js`, `$layout.js`, `$not-found.js` so that it
// sorts first: `$` is U+0024, below every digit and letter, so a directory
// listing puts the files that make a route above the components that happen to
// live beside them. That only works if the listings in the manual are sorted
// the way a file system lists them. A tree drawn in the order somebody thought
// of the files — `settings/` above `$layout.js`, `Counter.js` above
// `$page.test.js` — teaches the reader an order no tool will ever show them.
//
// So the order is the one `LC_ALL=C ls` gives: at every level, entries by code
// point (the bytes of their UTF-8 names), files and directories interleaved as
// that order puts them. A directory's trailing `/` is how a listing marks it,
// not part of its name, so `app/` sorts as `app`, above `app.js`. CONTRIBUTING.md
// ("Directory trees in documentation") is where the rule is written down for
// people; this file is the part that holds it.
//
// What counts as a tree, inside a fenced code block of any `.md` or `.mdx` file
// the repository owns:
//
//   - branches — rows drawn with `├──`/`└──`/`├─`/`└─` or uf's ASCII `|-`/`` `- ``,
//     in any fence, because command output shown in a `console` block is
//     still a listing a reader compares with their own disk;
//   - indented — a plain (`text`, `txt`, `tree` or unlabelled) fence whose
//     every line is one name, optionally followed by two spaces and a comment,
//     indented under a `name/` directory;
//   - paths — a plain fence whose every line is a relative path with at least
//     one `/`, ending in a file with an extension or in a directory's `/`.
//
// The shapes are narrow on purpose. A check that guessed at every indented
// block would flag JSON and YAML; one that only read ```tree fences would miss
// the listings that exist. Anything outside the three shapes is not read, and
// the check prints how many listings it read so a shape that stops being
// recognised shows up as a smaller number rather than as silence.
//
// A tree that is not a file system — a component hierarchy, a call graph —
// has an order that means something, and says so in its info string:
//
//   ```text unsorted
//
// `unsorted` is the only opt-out, and the check counts those too.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { fences } from "./snippets.js";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");

/** Directories that are not this repository's prose: dependencies, output, the Flow port. */
const SKIP: $ReadOnlySet<string> = new Set([
  "node_modules",
  "dist",
  ".uf",
  "target",
  "upstream",
  ".git",
  "coverage",
]);

/** Fence languages whose indented and path listings are read. Branch rows are read in any fence. */
const PLAIN: $ReadOnlyArray<string> = ["", "text", "txt", "plain", "plaintext", "tree"];

/**
 * One file or directory name as a listing writes it. Deliberately without
 * spaces, `=`, `:`, `{` or quotes, which is what keeps a YAML or JSON block
 * from being read as an indented listing.
 */
const NAME = /^[\w.$@()[\]+~-]+$/;

/**
 * A branch row: a lead of indentation and trunks (`│`, `|`), a branch glyph,
 * and the entry. The lead's length is the entry's depth, so every drawing
 * style works as long as siblings line up, which a drawn tree always does.
 */
const BRANCH = /^([\s│|]*?)(├──|└──|├─|└─|\|--|\|-|`--|`-)\s+(\S.*)$/;

/** An indented row: indentation, a name, an optional `/`, an optional two-space comment. */
const INDENTED = /^( *)(\S+?)(\/?)(?:\s{2,}\S.*)?$/;

/** A path row: segments joined by `/`, an optional trailing `/`, an optional comment. */
const PATH = /^(\S+?)(\/?)(?:\s{2,}\S.*)?$/;

/**
 * Two names in the order `LC_ALL=C ls` lists them: by the bytes of their UTF-8
 * encoding, which is code-point order.
 *
 * Not `localeCompare`, which folds case and ignores punctuation depending on
 * the machine — and ignoring punctuation is exactly what would lose the `$`.
 * Not `<` on strings either: that compares UTF-16 code units, which puts a
 * character above U+FFFF before U+E000–U+FFFF where code-point order does not.
 * Returns a negative number, zero or a positive number, like any comparator.
 */
export function compareNames(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

/**
 * Two slash-separated paths in the order a tree walk visits them: segment by
 * segment with {@link compareNames}, a directory before anything inside it.
 *
 * Comparing whole strings would be subtly different — `/` is U+002F, above
 * `-` and `.`, so `a-b/x.js` would sort above `a/x.js` as strings while `a`
 * lists above `a-b` in a tree. A flat list of paths is a tree written out, so
 * it sorts like one.
 */
export function comparePaths(left: string, right: string): number {
  const a = left.split("/").filter(Boolean);
  const b = right.split("/").filter(Boolean);
  for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
    const order = compareNames(a[index], b[index]);
    if (order !== 0) {
      return order;
    }
  }
  return a.length - b.length;
}

/** A file or directory in a listing, with the page line it is written on. */
export type Entry = {|
  /** The name alone: no branch glyphs, no trailing `/`, no comment. */
  readonly name: string,
  /** 1-based line of the page, so a failure points at the row to move. */
  readonly line: number,
  readonly children: Array<Entry>,
|};

/** One listing found in a page. */
export type Listing = {|
  /** The page, relative to the repository. */
  readonly page: string,
  /** The line of the page the listing's first row is on, 1-based. */
  readonly line: number,
  readonly kind: "branches" | "indented" | "paths",
  /**
   * The listing as a tree. Its own name is the row above the branches, or the
   * empty string when the listing has several top-level entries.
   */
  readonly root: Entry,
|};

/** An entry written above one that sorts before it. */
export type Disorder = {|
  readonly page: string,
  /** The line of the entry that is out of place. */
  readonly line: number,
  /** The directory both entries are in, `""` for the top level of a listing. */
  readonly parent: string,
  /** The entry above it that should be below it. */
  readonly previous: string,
  readonly entry: string,
|};

/** A listing's name for an entry: the text up to a comment, without a directory's `/`. */
function entryName(text: string): string {
  const name = text.split(/\s{2,}|\s#|\s\/\/|\s←|\s<-/)[0].trim();
  return name.length > 1 && name.endsWith("/") ? name.slice(0, -1) : name;
}

function entry(name: string, line: number): Entry {
  return { name, line, children: [] };
}

/**
 * Nests entries by column: an entry's parent is the nearest entry above it
 * with a smaller column. Shared by drawn and indented listings, which differ
 * only in how a row says its column.
 */
function nest(root: Entry, rows: $ReadOnlyArray<{| column: number, entry: Entry |}>): Entry {
  const stack: Array<{| column: number, entry: Entry |}> = [];
  for (const row of rows) {
    while (stack.length > 0 && stack[stack.length - 1].column >= row.column) {
      stack.pop();
    }
    const parent = stack.length > 0 ? stack[stack.length - 1].entry : root;
    parent.children.push(row.entry);
    stack.push(row);
  }
  return root;
}

/** Every run of branch rows in a fence, each with the line above it as its root. */
function branchListings(
  page: string,
  first: number,
  lines: $ReadOnlyArray<string>,
): Array<Listing> {
  const out: Array<Listing> = [];
  let index = 0;
  while (index < lines.length) {
    if (!BRANCH.test(lines[index])) {
      index += 1;
      continue;
    }
    const start = index;
    const rows = [];
    while (index < lines.length) {
      const match = lines[index].match(BRANCH);
      if (match == null) {
        break;
      }
      rows.push({ column: match[1].length, entry: entry(entryName(match[3]), first + index) });
      index += 1;
    }
    const above = start > 0 ? lines[start - 1].trim() : "";
    const root = entry(above === "" ? "" : entryName(above), first + start - 1);
    out.push({ page, line: first + start, kind: "branches", root: nest(root, rows) });
  }
  return out;
}

/**
 * A fence that is one indented listing, or `null`. Every row has to be a name,
 * and indentation may only deepen under a `name/` row — a block that breaks
 * either rule is something else, and is left alone.
 */
function indentedListing(
  page: string,
  first: number,
  lines: $ReadOnlyArray<string>,
): Listing | null {
  const rows = [];
  let previous: {| column: number, directory: boolean |} | null = null;
  // The first row's indentation, which is the listing's left edge: a fence
  // inside a list item is indented as a whole.
  let base = 0;
  let nested = false;
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trim() === "") {
      continue;
    }
    const match = lines[index].match(INDENTED);
    if (match == null || !NAME.test(match[2])) {
      return null;
    }
    const column = match[1].length;
    const directory = match[3] === "/";
    if (previous == null) {
      base = column;
    } else if (column < base || (column > previous.column && !previous.directory)) {
      return null;
    }
    if (previous != null && column > previous.column) {
      nested = true;
    }
    rows.push({ column, entry: entry(match[2], first + index) });
    previous = { column, directory };
  }
  if (!nested || rows.length < 2) {
    return null;
  }
  return {
    page,
    line: rows[0].entry.line,
    kind: "indented",
    root: nest(entry("", first - 1), rows),
  };
}

/**
 * A fence that is one list of relative paths, or `null`. The last segment has
 * to look like a file (`name.ext`) or be marked a directory, which is what
 * keeps a list of package names such as `@uniflowed/router` out.
 */
function pathListing(page: string, first: number, lines: $ReadOnlyArray<string>): Listing | null {
  const root = entry("", first - 1);
  let count = 0;
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trim() === "") {
      continue;
    }
    const match = lines[index].match(PATH);
    if (match == null) {
      return null;
    }
    const segments = match[1].split("/");
    const last = segments[segments.length - 1];
    if (
      segments.length < 2 ||
      !segments.every((segment) => NAME.test(segment)) ||
      (match[2] !== "/" && !/.\.\w+$/.test(last))
    ) {
      return null;
    }
    // The whole path is one row: the order that matters is the order of the
    // rows, so the check compares each path with the row above it.
    root.children.push(entry(match[1], first + index));
    count += 1;
  }
  return count < 2 ? null : { page, line: root.children[0].line, kind: "paths", root };
}

/** Whether a fence opted out of the check with `unsorted` in its info string. */
function optedOut(meta: string): boolean {
  return /(^|\s)unsorted(\s|$)/.test(meta);
}

/**
 * Every listing in one Markdown page, in page order, and how many fences that
 * held one said `unsorted`. `page` is only carried into the results, so a
 * caller can name the page however it reports it.
 */
export function listings(
  page: string,
  markdown: string,
): {| listings: Array<Listing>, unsorted: number |} {
  const out: Array<Listing> = [];
  let unsorted = 0;
  for (const fence of fences(page, markdown)) {
    const lines = fence.code.split("\n");
    const found = [...branchListings(page, fence.line, lines)];
    if (found.length === 0 && PLAIN.includes(fence.lang)) {
      const listing =
        indentedListing(page, fence.line, lines) ?? pathListing(page, fence.line, lines);
      if (listing != null) {
        found.push(listing);
      }
    }
    if (found.length > 0 && optedOut(fence.meta)) {
      unsorted += 1;
    } else {
      out.push(...found);
    }
  }
  return { listings: out, unsorted };
}

/**
 * Every place a listing is out of order: each entry that sorts before the
 * sibling (or, in a path listing, the row) written above it. One report per
 * misplaced entry, so a reversed pair is one line of output, not two.
 */
export function disorders(listing: Listing): Array<Disorder> {
  const out: Array<Disorder> = [];
  const compare = listing.kind === "paths" ? comparePaths : compareNames;
  const walk = (node: Entry, parent: string) => {
    node.children.forEach((child, index) => {
      const previous = node.children[index - 1];
      if (previous != null && compare(previous.name, child.name) > 0) {
        out.push({
          page: listing.page,
          line: child.line,
          parent,
          previous: previous.name,
          entry: child.name,
        });
      }
      walk(child, parent === "" ? child.name : `${parent}/${child.name}`);
    });
  };
  walk(listing.root, listing.root.name);
  return out;
}

/**
 * Every `.md` and `.mdx` file under `root` the repository owns, sorted: the
 * manual, every README, the example and fixture READMEs. Dependencies, build
 * output and the Flow port are skipped by directory name ({@link SKIP}).
 * Symbolic links are not followed: a link inside the repository points at a
 * file the walk reaches anyway, one outside it is not this repository's
 * prose, and a dangling one would otherwise throw.
 */
export function documents(root: string): Array<string> {
  const found: Array<string> = [];
  const walk = (dir: string) => {
    for (const name of fs.readdirSync(dir)) {
      const full = path.join(dir, name);
      const stat = fs.lstatSync(full);
      if (stat.isSymbolicLink()) {
        continue;
      }
      if (stat.isDirectory()) {
        if (!SKIP.has(name)) {
          walk(full);
        }
      } else if (name.endsWith(".md") || name.endsWith(".mdx")) {
        found.push(full);
      }
    }
  };
  walk(root);
  return found.sort(compareNames);
}

/** The whole check over a repository: what it read, and what is out of order. */
export function check(root: string): {|
  listings: number,
  unsorted: number,
  disorders: Array<Disorder>,
|} {
  let count = 0;
  let unsorted = 0;
  const out: Array<Disorder> = [];
  for (const file of documents(root)) {
    const found = listings(path.relative(root, file), fs.readFileSync(file, "utf8"));
    count += found.listings.length;
    unsorted += found.unsorted;
    for (const listing of found.listings) {
      out.push(...disorders(listing));
    }
  }
  return { listings: count, unsorted, disorders: out };
}

/** One line a person can act on: where, what, and which way it moves. */
export function describe(disorder: Disorder): string {
  const where = disorder.parent === "" ? "" : ` in \`${disorder.parent}/\``;
  return `${disorder.page}:${disorder.line}: \`${disorder.entry}\` sorts before \`${disorder.previous}\`${where}`;
}

function main(): void {
  const result = check(REPO);
  process.stdout.write(
    `docs-trees: ${result.listings} directory listings read, ${result.unsorted} marked unsorted\n`,
  );
  if (result.disorders.length > 0) {
    process.stdout.write(
      `\nThese listings are not in code-point order (the order \`LC_ALL=C ls\` gives, which puts \`$page.js\` first). See "Directory trees in documentation" in CONTRIBUTING.md:\n${result.disorders
        .map((disorder) => `  ${describe(disorder)}`)
        .join("\n")}\n`,
    );
    process.exitCode = 1;
  }
}

if (process.argv[1] != null && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
