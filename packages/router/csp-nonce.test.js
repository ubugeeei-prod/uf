// @flow
//
// Every `<script>` in a uf document, against the response's CSP nonce.
//
// The claim is one sentence — a document rendered for a request that has a
// nonce carries that nonce on every script a browser would execute — and the
// only honest way to test it is to scan the document for `<script` rather than
// to assert the three elements uf happens to write today. A test that listed
// them would pass on the day somebody adds a fourth, which is the day it was
// written to fail.
//
// # What the scan admits, and why that is not a loophole
//
// Two kinds of `<script>` are allowed through without a nonce, and both are
// non-executable *data blocks*: `application/json`, which carries the loader's
// payload and its deferred rows, and `application/ld+json`, which carries
// JSON-LD. HTML classifies a script whose type is neither a JavaScript MIME
// type nor `module` as a data block and never executes it.
//
// They are exempt here for a reason that is about hydration rather than about
// CSP. Those three elements are rendered *by React, on both sides* — see
// `internal/runtime.js`'s `payloadElements` and `PayloadRow` — and React
// compares a `nonce` during hydration by reading the DOM node's `nonce` IDL
// property. A nonce written by the server and not repeated by the client is
// therefore a hydration difference, and the client has no way to learn the
// value that does not defeat the browser's own mitigation: browsers blank the
// `nonce` content attribute precisely so that an HTML injection cannot read a
// nonce back out of the document, and any carrier uf added to hand it to the
// client — a `<meta>`, say — would hand it to the injection too.
//
// So the exemption is a closed list of two types, and the list is the thing
// the test pins. A new `<script>` with no type, or with `type="module"`, or
// with any type outside that list, fails here — which is every script a nonce
// policy would actually block.
//
// `packages/router/internal/flight-chunks.js` writes its payload elements as
// *text* rather than through React, so there is no second render to disagree
// with and those do carry the nonce. They are asserted directly at the bottom.

import * as React from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/test";
import { routerView } from "@uniflowed/router";
import { beginRequest, createRenderer } from "@uniflowed/router/server";
import { nonce } from "@uniflowed/server";

// Not a package export: these are the elements uf writes into the stream
// itself, and the nonce on them is this module's claim as much as the
// document's.
import { createChunkEncoder, flightChunkElement } from "./internal/flight-chunks.js";

const assets = { scripts: ["/assets/client-abc123.js"], styles: [], preloads: [] };

/** A route table of one route: a layout and a page, nothing deferred. */
function plainTable() {
  component Page() {
    return <p>the page is here</p>;
  }
  component SiteLayout(children: React.Node) {
    return (
      <div>
        <nav>the layout is here</nav>
        {children}
      </div>
    );
  }
  return {
    routes: [
      {
        path: "/",
        params: [],
        mdx: false,
        file: "app/$page.js",
        page: () => Promise.resolve({ default: Page }),
        layouts: [() => Promise.resolve({ default: SiteLayout })],
        loading: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

/**
 * Every `<script …>` start tag in `html`, as the attribute text inside it.
 *
 * A scan rather than a parse, and deliberately a loose one: it matches the
 * literal `<script` wherever it appears in the document uf produced, so a new
 * element written by any means at all is caught. The documents under test hold
 * no user text, so there is nothing here for a `<script` inside a string to
 * confuse.
 */
function scriptTags(html: string): Array<string> {
  const tags = [];
  const pattern = /<script\b([^>]*)>/g;
  let found = pattern.exec(html);
  while (found != null) {
    tags.push(found[1]);
    found = pattern.exec(html);
  }
  return tags;
}

/** The two non-executable types a script may carry instead of a nonce. */
const DATA_BLOCKS = ['type="application/json"', 'type="application/ld+json"'];

function isDataBlock(tag: string): boolean {
  return DATA_BLOCKS.some((type) => tag.includes(type));
}

/** Render `/` for a request, with `before` run inside it first. */
async function documentFor(before: () => void): Promise<string> {
  const renderer = createRenderer({ App: routerView("./app"), ...plainTable() });
  const lifecycle = beginRequest(new Request("http://localhost/"));
  try {
    return await lifecycle.run(async () => {
      before();
      const result = await renderer.render("/", assets);
      return await result.text();
    });
  } finally {
    await lifecycle.settle();
  }
}

describe("a document rendered for a request that has a nonce", () => {
  it("carries it on every script a browser would execute", async () => {
    let minted = "";
    const html = await documentFor(() => {
      minted = nonce();
    });

    // The premise. Without this the scan below would pass on a document with
    // no scripts in it at all.
    expect(minted.length > 0).toBe(true);
    const tags = scriptTags(html);
    expect(tags.length > 0).toBe(true);

    const carried = `nonce="${minted}"`;
    for (const tag of tags) {
      if (isDataBlock(tag)) {
        continue;
      }
      // The message names the tag, because "expected true to be false" on a
      // document somebody just added a script to says nothing about which one.
      expect(`${tag.includes(carried) ? "nonced" : "UNNONCED"}: <script${tag}>`).toBe(
        `nonced: <script${tag}>`,
      );
    }
  });

  it("puts it on the client entry, which is how a nonce policy admits it", async () => {
    let minted = "";
    const html = await documentFor(() => {
      minted = nonce();
    });
    expect(html).toContain(
      `<script type="module" src="/assets/client-abc123.js" nonce="${minted}"></script>`,
    );
  });

  it("uses a different nonce for every response", async () => {
    let first = "";
    let second = "";
    await documentFor(() => {
      first = nonce();
    });
    await documentFor(() => {
      second = nonce();
    });
    expect(first === second).toBe(false);
  });
});

describe("a document rendered for a request that has none", () => {
  it("is the document uf wrote before nonces existed", async () => {
    // Nothing asked, so nothing was minted — and the point of the feature
    // costing a project nothing is that this document is unchanged.
    const html = await documentFor(() => {});
    for (const tag of scriptTags(html)) {
      expect(tag.includes("nonce=")).toBe(false);
    }
  });
});

describe("the payload chunk elements, which uf writes as text", () => {
  it("carry the nonce", () => {
    expect(flightChunkElement("row", "N0NCE")).toContain('nonce="N0NCE"');
    expect(flightChunkElement(null, "N0NCE")).toContain('nonce="N0NCE"');
  });

  it("carry none when the response has none", () => {
    expect(flightChunkElement("row").includes("nonce=")).toBe(false);
  });

  it("carry it on every element one encoder writes, the end marker included", () => {
    const encoder = createChunkEncoder("N0NCE");
    const written = encoder.encode(new TextEncoder().encode('"a chunk"')) + encoder.end();
    for (const tag of scriptTags(written)) {
      expect(tag.includes('nonce="N0NCE"')).toBe(true);
    }
  });

  it("escapes a nonce that a project supplied, so it cannot end the attribute", () => {
    expect(flightChunkElement(null, 'a" onload="x')).toContain('nonce="a&quot; onload=&quot;x"');
  });
});
