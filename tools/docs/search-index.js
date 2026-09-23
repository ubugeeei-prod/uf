// @flow
//
// Write the manual's search index from the built site.
//
//   node --import @uniflowed/host/register tools/docs/search-index.js docs/dist/docs
//
// `tools/docs/build.sh` runs this after `uf build#docs`, and it writes
// `search-index.json` into the same directory, which is what the search dialog
// fetches (`docs/app/_design/search-dialog.js`).
//
// From the built HTML rather than the `.mdx` sources, for the reason the link
// check reads the output too: an `id` the index names is one the markdown
// pipeline actually rendered, so a result cannot land on an anchor that does
// not exist, and a page written as JavaScript is indexed by what it printed.
//
// The pages indexed are the ones `nav.js` lists — the manual — and the API
// reference's page for each package.
// A listed page the build did not write is an error rather than a gap in the
// index: `tests/library/docs-nav.test.js` already says every listed page has a
// source, so a missing document is the build disagreeing with itself.

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { pages, sectionFor } from "../../docs/app/_design/nav.js";
import { INDEX_VERSION } from "../../docs/app/_design/search.js";
import type { SearchEntry, SearchIndex } from "../../docs/app/_design/search.js";

/** Headings that start an entry. `h1` is the page itself. */
const HEADING = /<h([234])\b[^>]*\bid="([^"]+)"[^>]*>([\s\S]*?)<\/h\1>/g;

/**
 * The entries one built page contributes: its opening, then one per `h2`–`h4`
 * that has an id, each with the text up to the next such heading.
 *
 * `html` is the whole document; only the article (`<main id="content">`) is
 * read, so the sidebar, the masthead and the colophon — which are on every
 * page — are not in every page's text.
 */
export function entriesFromHtml(html: string, href: string, section: string): Array<SearchEntry> {
  const main = html.match(/<main\b[^>]*\bid="content"[^>]*>([\s\S]*?)<\/main>/);
  if (main == null) {
    throw new Error(`${href}: no <main id="content"> in the built page`);
  }
  const article = main[1]
    // The running head, the links to the pages either side, the way down to
    // the contents and the link to the page's source are
    // chrome that happens to sit inside the article; none is something the
    // page says.
    .replace(/<p class="(eyebrow|next-page|page-source)">[\s\S]*?<\/p>/g, "")
    .replace(/<nav class="page-turn"[^>]*>[\s\S]*?<\/nav>/g, "")
    .replace(/<a class="to-contents"[^>]*>[\s\S]*?<\/a>/g, "")
    .replace(/<(script|style|template)\b[\s\S]*?<\/\1>/g, "")
    // Code blocks are left out. They are half the manual's bytes and most of
    // what they would match — `import`, `const`, `return` — is noise; the names
    // a reader searches for are in the prose around them, in `<code>`, which
    // stays.
    .replace(/<pre\b[\s\S]*?<\/pre>/g, " ");

  const title = article.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/);
  if (title == null) {
    throw new Error(`${href}: no <h1> in the built page`);
  }
  const page = text(title[1]);
  const afterTitle = (title.index ?? 0) + title[0].length;

  const entries: Array<SearchEntry> = [];
  let heading: string | null = null;
  let anchor: string | null = null;
  let from = afterTitle;
  for (const match of article.matchAll(HEADING)) {
    const at = match.index ?? 0;
    if (at < afterTitle) {
      continue;
    }
    entries.push(entry(href, anchor, page, section, heading, article.slice(from, at)));
    anchor = match[2];
    heading = text(match[3]);
    from = at + match[0].length;
  }
  entries.push(entry(href, anchor, page, section, heading, article.slice(from)));
  return entries;
}

function entry(
  href: string,
  anchor: string | null,
  page: string,
  section: string,
  heading: string | null,
  html: string,
): SearchEntry {
  return {
    href: anchor == null ? href : `${href}#${anchor}`,
    page,
    section,
    heading,
    text: text(html),
  };
}

/** The text of an HTML fragment: tags dropped, entities decoded, spaces collapsed. */
export function text(html: string): string {
  return decode(
    html
      .replace(/<!--[\s\S]*?-->/g, "")
      // A link followed by its blurb, as a landing page's contents list is.
      .replace(/<\/a>\s*<span\b/g, "</a> <span")
      // A block boundary is a word boundary, even where the markup has no
      // whitespace between two cells or two list items.
      .replace(/<\/?(p|div|li|tr|td|th|pre|h[1-6]|br|table|ul|ol|dt|dd|blockquote)\b[^>]*>/g, " ")
      .replace(/<[^>]+>/g, ""),
  )
    .replace(/\s+/g, " ")
    .trim();
}

const NAMED: { readonly [string]: string } = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

function decode(value: string): string {
  return value.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === "#") {
      const code =
        name[1] === "x" || name[1] === "X"
          ? Number.parseInt(name.slice(2), 16)
          : Number.parseInt(name.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    return NAMED[name.toLowerCase()] ?? whole;
  });
}

/** The index for the site built into `site`. */
export function buildIndex(site: string): SearchIndex {
  const entries: Array<SearchEntry> = [];
  for (const listed of pages) {
    const file = path.join(site, listed.href, "index.html");
    if (!fs.existsSync(file)) {
      throw new Error(
        `${listed.href} is listed in docs/app/_design/nav.js and the build wrote no ${path.relative(site, file)}`,
      );
    }
    const html = fs.readFileSync(file, "utf8");
    entries.push(...entriesFromHtml(html, listed.href, sectionFor(listed.href)?.title ?? ""));
  }
  // The API reference has a page per package, prerendered from its parameter
  // rather than listed one by one in `nav.js`, and an export's name is the
  // thing a reader most often types into a search box.
  const api = path.join(site, "reference", "api");
  if (fs.existsSync(api)) {
    for (const name of fs.readdirSync(api).sort()) {
      const file = path.join(api, name, "index.html");
      if (fs.existsSync(file)) {
        entries.push(
          ...entriesFromHtml(fs.readFileSync(file, "utf8"), `/reference/api/${name}`, "API"),
        );
      }
    }
  }
  return { version: INDEX_VERSION, entries };
}

function main(argv: $ReadOnlyArray<string>): void {
  const site = argv[0];
  if (site == null) {
    process.stderr.write("usage: tools/docs/search-index.js SITE_DIR\n");
    process.exitCode = 2;
    return;
  }
  const index = buildIndex(site);
  const out = path.join(site, "search-index.json");
  const body = JSON.stringify(index);
  fs.writeFileSync(out, body);
  process.stdout.write(
    `search-index: ${index.entries.length} entries from ${pages.length} pages, ${Math.round(body.length / 1024)} kB\n`,
  );
}

if (process.argv[1] != null && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2));
}
