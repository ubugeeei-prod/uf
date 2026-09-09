// @flow
//
// What Vite's `transformIndexHtml` does with the one chunk `uf dev` gives it.
//
// `uf dev` streams (ubugeeei-prod/uf#374): `internal/stream.js` holds the
// opening chunk back until the head is complete, hands *that* to
// `transformIndexHtml`, and forwards the rest untouched. Vite's hook is a
// whole-document hook, so this file is the answer to what a document that
// stops inside `<body>` does to it — the question #374 called "the parse step
// is the risk", and the limitation the streaming trade buys.
//
// Driven against a real Vite server rather than a stub, because the whole
// point is what *Vite* does. A stub would assert what this file already
// believes.

import { describe, expect, it } from "@uniflowed/test";

/** A document, and the same document cut where a first chunk would end. */
const WHOLE =
  "<!doctype html><html><head><title>t</title></head><body><main>x</main></body></html>";
const STREAMED = "<!doctype html><html><head><title>t</title></head><body>";

/** A plugin that asks for every position, so each can be found by name. */
const injector = {
  name: "test:inject",
  transformIndexHtml() {
    return ["head-prepend", "head", "body-prepend", "body"].map((at) => ({
      tag: "script",
      attrs: { "data-at": at },
      injectTo: at,
    }));
  },
};

async function transformed(html: string): Promise<string> {
  const { createServer } = await import("vite");
  const server = await createServer({
    root: process.cwd(),
    logLevel: "silent",
    server: { middlewareMode: true },
    plugins: [injector],
  });
  try {
    return await server.transformIndexHtml("/", html);
  } finally {
    await server.close();
  }
}

/** Where each injection landed, as an order rather than an offset. */
function order(html: string): Array<string> {
  return ["head-prepend", "head", "body-prepend", "body"]
    .map((at) => [at, html.indexOf(`data-at="${at}"`)])
    .filter(([, at]) => at !== -1)
    .sort(([, a], [, b]) => Number(a) - Number(b))
    .map(([name]) => String(name));
}

describe("the head chunk `uf dev` hands to Vite", () => {
  it("keeps the transform working on a document that stops inside the body", async () => {
    // The question #374 was blocked on. If Vite refused a partial document, or
    // dropped its injections, `uf dev` could not stream at all and the fix
    // would have had to be a Vite-side one.
    const out = await transformed(STREAMED);

    expect(out).toContain("/@vite/client");
    expect(order(out)).toEqual(["head-prepend", "head", "body-prepend", "body"]);
    // The chunk itself survives: this is a rewrite, not a re-render.
    expect(out).toContain("<title>t</title>");
  }, 60_000);

  it("moves an `injectTo: body` tag to the top of the body, and nothing else", async () => {
    // The cost of streaming, measured rather than assumed, and the reason it is
    // written down on `RenderOptions.transformHead`.
    //
    // Three of the four positions are decided by markup the head chunk already
    // holds — `<head>`, `</head>`, `<body>` — so they land where they always
    // did. `body` means *before `</body>`*, and there is no `</body>` yet, so
    // it goes to the end of what there is: the top of the body.
    //
    // Nothing is dropped either way. A `<script>` that expects a complete DOM
    // is the case this costs, and no uf injection is one — uf uses `head` and
    // `head-prepend`, and Vite's client is head-injected.
    const whole = await transformed(WHOLE);
    const streamed = await transformed(STREAMED);

    // In a whole document the body tag comes after the content.
    expect(whole.indexOf('data-at="body"')).toBeGreaterThan(whole.indexOf("<main>"));

    // In the streamed chunk it is last only because the content is not there.
    expect(order(streamed)).toEqual(["head-prepend", "head", "body-prepend", "body"]);
    expect(streamed).not.toContain("<main>");
    // And it sits immediately after `body-prepend`, which is what "the top of
    // the body" means when nothing has been rendered into it yet.
    const prepend = streamed.indexOf('data-at="body-prepend"');
    const body = streamed.indexOf('data-at="body"');
    expect(streamed.slice(prepend, body)).not.toContain("<main>");
  }, 60_000);
});
