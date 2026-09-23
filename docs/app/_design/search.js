// @flow
//
// Finding a page by what it says.
//
// The index is written by `tools/docs/search-index.js` from the *built* site,
// not from the `.mdx` sources: an `id` in it is one the markdown pipeline
// rendered, and a page that exists only as JavaScript — the home page, the
// component reference's registry — is indexed by what it actually printed. One
// entry per heading, so a result lands on the paragraph that answered it
// rather than at the top of a two-thousand-line page.
//
// The ranking lives here rather than in the dialog so it can be tested without
// a browser (`tests/library/docs-search.test.js`) and so the build and the page
// agree on what an entry is. There is no fuzzy matching and no stemming. A
// manual's vocabulary is its commands, flags and export names, and a reader
// who typed `--frozen` wants `--frozen-lockfile`, not `frozen` spelled three
// ways — so a term matches where some word *starts* with it, and that is all.

/** One heading of one page, and the text under it up to the next heading. */
export type SearchEntry = {|
  /** Where the result goes: the page, plus `#id` for a heading below the title. */
  readonly href: string,
  /** The page's title, without the site's suffix. */
  readonly page: string,
  /** The section of the manual the page is listed in, or `""` outside it. */
  readonly section: string,
  /** The heading, or `null` for the part of the page above its first heading. */
  readonly heading: string | null,
  /** The text under the heading, whitespace collapsed. */
  readonly text: string,
|};

export type SearchIndex = {|
  /** Bumped when an entry's shape changes, so a stale cached index is refused. */
  readonly version: 1,
  readonly entries: $ReadOnlyArray<SearchEntry>,
|};

export type SearchResult = {|
  readonly entry: SearchEntry,
  readonly score: number,
  /** A stretch of `text` around the first match, or the text's opening. */
  readonly excerpt: string,
|};

export const INDEX_VERSION: 1 = 1;

/** Where the built site serves the index from. */
export const INDEX_PATH = "/search-index.json";

const EXCERPT_BEFORE = 48;
const EXCERPT_LENGTH = 160;

/** The words a query is made of: lower case, split on whitespace. */
export function terms(query: string): Array<string> {
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter((term) => term.length > 0);
}

/**
 * Where `term` first starts a word in `haystack` (already lower case) at or
 * after `start`, or -1.
 *
 * A word starts at the beginning, or after anything that is not a letter or a
 * digit — so `frozen` finds `--frozen-lockfile`, `lock` finds `frozen-lockfile`,
 * and `use` does not find `because`.
 */
export function wordStart(haystack: string, term: string, start: number = 0): number {
  if (term.length === 0) {
    return -1;
  }
  let from = start;
  while (from <= haystack.length - term.length) {
    const at = haystack.indexOf(term, from);
    if (at === -1) {
      return -1;
    }
    if (at === 0 || !/[\p{L}\p{N}]/u.test(haystack[at - 1])) {
      return at;
    }
    from = at + 1;
  }
  return -1;
}

/**
 * The entries every term of `query` matches, best first.
 *
 * Every term has to appear somewhere in the entry — its page title, its
 * heading or its text — or the entry is out. Among the rest a heading match
 * outweighs a title match, which outweighs any number of matches in the text:
 * a reader who typed a phrase that is somebody's heading wants that section,
 * and one that is only mentioned in passing is a worse answer even when it is
 * mentioned often. Ties keep the index's order, which is the manual's.
 */
export function search(
  entries: $ReadOnlyArray<SearchEntry>,
  query: string,
  limit: number = 12,
): Array<SearchResult> {
  const words = terms(query);
  if (words.length === 0) {
    return [];
  }
  const phrase = words.join(" ");
  const scored: Array<{| result: SearchResult, order: number |}> = [];

  entries.forEach((entry, order) => {
    const heading = (entry.heading ?? "").toLowerCase();
    const page = entry.page.toLowerCase();
    const text = entry.text.toLowerCase();
    let score = 0;
    let firstInText = -1;

    for (const word of words) {
      const inHeading = wordStart(heading, word);
      const inPage = wordStart(page, word);
      const inText = wordStart(text, word);
      if (inHeading === -1 && inPage === -1 && inText === -1) {
        return;
      }
      if (inHeading !== -1) {
        score += 8;
      }
      if (inPage !== -1) {
        // A page's title matches on every one of its entries, so on its own it
        // must not lift a passing mention above a heading elsewhere. It lifts
        // the page's own opening most, which is where the title is answered.
        score += entry.heading == null ? 6 : 2;
      }
      if (inText !== -1) {
        // One for being there at all, and under one more for how often: a
        // section that is about the term says it more than once, and between
        // two passing mentions that is the only difference there is. Capped
        // below the weight of a title match, so repetition never outranks one.
        let count = 1;
        let next = wordStart(text, word, inText + word.length);
        while (next !== -1 && count < 10) {
          count += 1;
          next = wordStart(text, word, next + word.length);
        }
        score += 1 + (count - 1) / 10;
        if (firstInText === -1 || inText < firstInText) {
          firstInText = inText;
        }
      }
    }

    if (words.length > 1 && heading.includes(phrase)) {
      score += 10;
    } else if (words.length > 1 && text.includes(phrase)) {
      score += 3;
    }
    if (heading === phrase || (entry.heading == null && page === phrase)) {
      score += 12;
    }

    scored.push({
      result: { entry, score, excerpt: excerpt(entry.text, firstInText) },
      order,
    });
  });

  scored.sort((a, b) => b.result.score - a.result.score || a.order - b.order);
  return scored.slice(0, limit).map((item) => item.result);
}

/** About a line of `text` around `at`, with an ellipsis where it was cut. */
export function excerpt(text: string, at: number): string {
  if (at <= EXCERPT_BEFORE) {
    const head = text.slice(0, EXCERPT_LENGTH);
    return head.length < text.length ? `${head.trimEnd()}…` : head;
  }
  // Start at a word boundary, so the excerpt does not open on half a word.
  const space = text.indexOf(" ", at - EXCERPT_BEFORE);
  const start = space === -1 || space >= at ? at : space + 1;
  const body = text.slice(start, start + EXCERPT_LENGTH);
  return `…${body.trimEnd()}${start + EXCERPT_LENGTH < text.length ? "…" : ""}`;
}

/**
 * The index, if `value` is one this page can read.
 *
 * The index is fetched from the host, so this is the boundary where a value of
 * unknown shape becomes a typed one. A version this module did not write is
 * refused rather than half-read: a cached index from an older deployment is
 * better as "no results" than as results that link to the wrong places.
 */
export function readIndex(value: mixed): SearchIndex | null {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  if (value.version !== INDEX_VERSION || !Array.isArray(value.entries)) {
    return null;
  }
  const entries: Array<SearchEntry> = [];
  for (const item of value.entries) {
    if (item == null || typeof item !== "object" || Array.isArray(item)) {
      return null;
    }
    const { href, page, section, heading, text } = item;
    if (
      typeof href !== "string" ||
      typeof page !== "string" ||
      typeof section !== "string" ||
      (heading !== null && typeof heading !== "string") ||
      typeof text !== "string"
    ) {
      return null;
    }
    entries.push({ href, page, section, heading, text });
  }
  return { version: INDEX_VERSION, entries };
}

/** A stretch of text, and whether it is where a term matched. */
export type Segment = {|
  readonly text: string,
  readonly match: boolean,
  /** Where in the value it starts, which is also what tells two apart. */
  readonly start: number,
|};

/**
 * `value` cut into the stretches the terms of `query` match and the rest, for
 * a result to mark what it was found by. Only word starts are marked, the same
 * rule `search` matched by, so the mark never points at a place the match was
 * not.
 */
export function segments(value: string, query: string): Array<Segment> {
  const words = terms(query);
  const lower = value.toLowerCase();
  const marked: Array<boolean> = Array.from({ length: value.length }, () => false);
  for (const word of words) {
    let from = 0;
    while (from < lower.length) {
      const at = wordStart(lower, word, from);
      if (at === -1) {
        break;
      }
      for (let i = at; i < at + word.length; i += 1) {
        marked[i] = true;
      }
      from = at + word.length;
    }
  }
  const out: Array<Segment> = [];
  let start = 0;
  for (let i = 1; i <= value.length; i += 1) {
    if (i === value.length || marked[i] !== marked[start]) {
      if (i > start) {
        out.push({ text: value.slice(start, i), match: marked[start], start });
      }
      start = i;
    }
  }
  return out;
}
