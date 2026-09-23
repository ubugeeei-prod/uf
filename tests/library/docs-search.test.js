// @flow
//
// The manual's search: the index `tools/docs/search-index.js` writes from the
// built site, and the ranking `docs/app/_design/search.js` runs in the dialog.
//
// Both are pure functions over strings, so they are tested here, without a
// browser and without a build. What the dialog does with them — open on `/`,
// move with the arrow keys, follow a result to its heading — was checked in a
// browser against a built site when it was written; `docs:hydration` loads
// every built page and would fail on a dialog that broke hydration.

import { describe, expect, it } from "@uniflowed/test";

import {
  excerpt,
  readIndex,
  search,
  segments,
  terms,
  wordStart,
} from "../../docs/app/_design/search.js";
import type { SearchEntry } from "../../docs/app/_design/search.js";
import { entriesFromHtml, text } from "../../tools/docs/search-index.js";

function entry(fields: {|
  href: string,
  page: string,
  heading?: string | null,
  text?: string,
|}): SearchEntry {
  return {
    href: fields.href,
    page: fields.page,
    section: "",
    heading: fields.heading ?? null,
    text: fields.text ?? "",
  };
}

describe("wordStart", () => {
  it("finds a term only where a word starts", () => {
    expect(wordStart("uf install --frozen-lockfile", "frozen")).toBe(13);
    expect(wordStart("uf install --frozen-lockfile", "lockfile")).toBe(20);
    expect(wordStart("because", "use")).toBe(-1);
    expect(wordStart("use because", "use")).toBe(0);
  });

  it("looks past a match inside a word for one at a word start", () => {
    expect(wordStart("reuse use", "use")).toBe(6);
  });

  it("starts where it is told to", () => {
    expect(wordStart("use and use", "use", 1)).toBe(8);
  });

  it("matches nothing for an empty term", () => {
    expect(wordStart("anything", "")).toBe(-1);
  });
});

describe("search", () => {
  const entries = [
    entry({
      href: "/a",
      page: "Caching",
      text: "The route cache, and a passing mention of middleware.",
    }),
    entry({
      href: "/b#middleware",
      page: "Answering requests",
      heading: "Middleware",
      text: "Runs first.",
    }),
    entry({ href: "/c", page: "Middleware", text: "A page about it." }),
  ];

  it("finds nothing for an empty query", () => {
    expect(search(entries, "   ")).toEqual([]);
  });

  it("requires every term to match somewhere in the entry", () => {
    expect(search(entries, "route middleware").map((r) => r.entry.href)).toEqual(["/a"]);
    expect(search(entries, "route nothing")).toEqual([]);
  });

  it("puts a heading and a page title above a passing mention", () => {
    expect(search(entries, "middleware").map((r) => r.entry.href)).toEqual([
      "/b#middleware",
      "/c",
      "/a",
    ]);
  });

  it("keeps the manual's order between equals", () => {
    const twins = [
      entry({ href: "/1", page: "One", text: "snapshot" }),
      entry({ href: "/2", page: "Two", text: "snapshot" }),
    ];
    expect(search(twins, "snapshot").map((r) => r.entry.href)).toEqual(["/1", "/2"]);
  });

  it("prefers the section that says the term more often, among passing mentions", () => {
    const mentions = [
      entry({ href: "/once", page: "Scope", text: "shard is listed once" }),
      entry({ href: "/often", page: "Testing", text: "shard one; shard two; shard three" }),
    ];
    expect(search(mentions, "shard").map((r) => r.entry.href)).toEqual(["/often", "/once"]);
  });

  it("never lets repetition in the text outrank a title", () => {
    const loud = entry({ href: "/loud", page: "Other", text: "cache ".repeat(50) });
    const titled = entry({ href: "/titled", page: "Cache", heading: "Lifetimes", text: "" });
    expect(search([loud, titled], "cache").map((r) => r.entry.href)).toEqual(["/titled", "/loud"]);
  });

  it("returns at most `limit` results", () => {
    const many = Array.from({ length: 30 }, (_, i) =>
      entry({ href: `/${i}`, page: `P${i}`, text: "flag" }),
    );
    expect(search(many, "flag", 5).length).toBe(5);
  });
});

describe("excerpt", () => {
  const long = `${"a ".repeat(100)}needle ${"b ".repeat(100)}`;

  it("opens a match near the start at the start", () => {
    expect(excerpt("short text", 0)).toBe("short text");
  });

  it("cuts around a match further in, on a word boundary, and says it cut", () => {
    const cut = excerpt(long, long.indexOf("needle"));
    expect(cut.startsWith("…a ")).toBe(true);
    expect(cut.endsWith("…")).toBe(true);
    expect(cut.includes("needle")).toBe(true);
  });
});

describe("segments", () => {
  it("marks each word start a term matched, and nothing else", () => {
    expect(segments("Reuse the use hook", "use")).toEqual([
      { text: "Reuse the ", match: false, start: 0 },
      { text: "use", match: true, start: 10 },
      { text: " hook", match: false, start: 13 },
    ]);
  });

  it("marks every term", () => {
    expect(segments("--frozen-lockfile", "lock frozen")).toEqual([
      { text: "--", match: false, start: 0 },
      { text: "frozen", match: true, start: 2 },
      { text: "-", match: false, start: 8 },
      { text: "lock", match: true, start: 9 },
      { text: "file", match: false, start: 13 },
    ]);
  });

  it("is the text unmarked for a query of no terms", () => {
    expect(segments("text", "")).toEqual([{ text: "text", match: false, start: 0 }]);
    expect(terms("  ")).toEqual([]);
  });
});

describe("readIndex", () => {
  const good = {
    version: 1,
    entries: [{ href: "/x", page: "X", section: "S", heading: null, text: "t" }],
  };

  it("reads an index of the version it knows", () => {
    expect(readIndex(good)?.entries.length).toBe(1);
  });

  it("refuses another version, or an entry of the wrong shape", () => {
    expect(readIndex({ ...good, version: 2 })).toBe(null);
    expect(readIndex({ version: 1, entries: [{ href: "/x" }] })).toBe(null);
    expect(readIndex(null)).toBe(null);
    expect(readIndex([])).toBe(null);
  });
});

describe("the index a built page contributes", () => {
  const page = `<!doctype html><html><body>
    <nav class="manual-nav"><a href="/elsewhere">Sidebar text</a></nav>
    <main class="prose seam" id="content">
      <p class="eyebrow">Build an app</p>
      <h1 id="state">State</h1>
      <div class="lede"><p>Atoms &amp; stores.</p></div>
      <h2 id="in-react">In <code>React</code></h2>
      <p>Read it<br>with a hook.</p>
      <pre><code>const noise = 1;</code></pre>
      <h3 id="deep">Deeper</h3>
      <table><tr><td>one</td><td>two</td></tr></table>
      <h2>No id, so part of the section above</h2>
      <p class="next-page"><a href="/guide/form">Next: Forms</a></p>
      <p class="page-source"><a href="https://example.test">Edit this page</a></p>
    </main></body></html>`;

  it("is one entry per heading with an id, under the page's title", () => {
    expect(entriesFromHtml(page, "/guide/state", "Build an app")).toEqual([
      {
        href: "/guide/state",
        page: "State",
        section: "Build an app",
        heading: null,
        text: "Atoms & stores.",
      },
      {
        href: "/guide/state#in-react",
        page: "State",
        section: "Build an app",
        heading: "In React",
        text: "Read it with a hook.",
      },
      {
        href: "/guide/state#deep",
        page: "State",
        section: "Build an app",
        heading: "Deeper",
        text: "one two No id, so part of the section above",
      },
    ]);
  });

  it("refuses a page with no article", () => {
    expect(() => entriesFromHtml("<html></html>", "/x", "")).toThrow("no <main");
  });

  it("decodes entities and separates block boundaries", () => {
    expect(text("<p>a&lt;b</p><p>&#x27;c&#39;</p>")).toBe("a<b 'c'");
    expect(text('<li><a href="/x">Install</a><span>Install uf.</span></li>')).toBe(
      "Install Install uf.",
    );
  });
});
