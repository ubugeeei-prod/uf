// @flow
//
// The head `uf dev` hands to Vite, and what comes back.
//
// Two bugs, one seam. Vite's development HTML hook is written for an
// `index.html`, and a head React rendered is not one:
//
//   * under `app.router.basePath` the hook joins the base onto URLs that
//     already carried it, so uf's own stylesheet and anything built from
//     `import.meta.env.BASE_URL` came out as `/docs/docs/…` (ubugeeei-prod/uf#1677);
//   * the tags it prepends went ahead of the layout's own head children, and
//     React 19 paired `/@vite/client` with the layout's `<script>` and
//     reported a hydration mismatch (ubugeeei-prod/uf#1682).
//
// The pure halves are asserted directly; the round trip is driven through a
// real Vite server, as `dev-head-transform.test.js` beside this file is, because
// what matters is what *Vite* does with the result.
//
// `npm/vite/internal/` by path, not a package export: this is the dev server's
// own wiring rather than an interface anything outside the package calls.

import { describe, expect, it } from "@uniflowed/test";

import {
  markDevHead,
  settleDevHead,
  stripBaseForDevHook,
  transformRenderedHead,
} from "./internal/dev-head.js";

const ENTRY = "/@id/__x00__virtual:uf/client";

/** A head the way React and uf's document shell write one under `/docs/`. */
const HEAD =
  '<!doctype html><html lang="en"><head><title>Example</title>' +
  '<link rel="icon" href="/docs/logo.svg"/>' +
  '<script src="/docs/theme.js"></script>' +
  `<script type="module" src="${ENTRY}"></script>` +
  '<link rel="stylesheet" href="/docs/app/app.css">' +
  "</head><body>";

describe("taking the base off before Vite's hook", () => {
  it("strips it from the src and href of the tags the hook rewrites", () => {
    const out = stripBaseForDevHook(HEAD, "/docs/");
    expect(out).toContain('href="/logo.svg"');
    expect(out).toContain('src="/theme.js"');
    expect(out).toContain('href="/app/app.css"');
    // Written without the base, and left for the hook to prefix as before.
    expect(out).toContain(`src="${ENTRY}"`);
  });

  it("strips it from every candidate of a srcset", () => {
    const out = stripBaseForDevHook(
      '<head><link rel="preload" as="image" imagesrcset="/docs/a.png 1x, /docs/b.png 2x"></head>',
      "/docs/",
    );
    expect(out).toContain('imagesrcset="/a.png 1x, /b.png 2x"');
  });

  it("leaves alone what the hook leaves alone", () => {
    const untouched =
      "<head>" +
      // Not an asset attribute Vite knows.
      '<link rel="canonical" href="https://example.com/docs/"/>' +
      '<meta name="description" content="/docs/ is where the docs live"/>' +
      // An element that opted out of the hook.
      '<script vite-ignore src="/docs/raw.js"></script>' +
      // Text in an inline script, which is not markup.
      "<script>const tag = '<link href=\"/docs/x.css\">';</script>" +
      // A URL outside the base.
      '<link rel="icon" href="/other/icon.svg"/>' +
      "</head>";
    expect(stripBaseForDevHook(untouched, "/docs/")).toBe(untouched);
  });

  it("strips a URL meta's content and no other meta's", () => {
    const out = stripBaseForDevHook(
      '<head><meta property="og:image" content="/docs/og.png"/>' +
        '<meta name="author" content="/docs/me"/></head>',
      "/docs/",
    );
    expect(out).toContain('content="/og.png"');
    expect(out).toContain('content="/docs/me"');
  });

  it("does nothing at the root base", () => {
    expect(stripBaseForDevHook(HEAD, "/")).toBe(HEAD);
  });
});

describe("moving the prepended tags past the layout's head", () => {
  it("puts what was prepended right before uf's client entry", () => {
    const marked = markDevHead(HEAD, `<script type="module" src="${ENTRY}"`);
    // What Vite's `injectToHead(…, prepend)` does: after the opening tag.
    const injected = marked.replace(
      "<head>",
      '<head>\n  <script type="module" src="/@vite/client"></script>\n',
    );
    const out = settleDevHead(injected);

    expect(out).not.toContain("uf:dev-head");
    expect(out.indexOf("/@vite/client")).toBeGreaterThan(out.indexOf("/docs/theme.js"));
    expect(out.indexOf("/@vite/client")).toBeLessThan(out.indexOf(ENTRY));
    // The layout's own children lead the head again.
    expect(out).toContain('<head><title>Example</title><link rel="icon"');
  });

  it("falls back to the end of the head without a client entry", () => {
    const head = "<html><head><title>t</title></head><body>";
    const injected = markDevHead(head, null).replace(
      "<head>",
      '<head><script src="/@vite/client"></script>',
    );
    expect(settleDevHead(injected)).toBe(
      '<html><head><title>t</title><script src="/@vite/client"></script></head><body>',
    );
  });
});

describe("the round trip through Vite", () => {
  async function transformed(base: string, html: string): Promise<string> {
    const { createServer } = await import("vite");
    const server = await createServer({
      root: process.cwd(),
      base,
      logLevel: "silent",
      server  : { middlewareMode: true },
      plugins: [
        {
          name: "test:preamble",
          transformIndexHtml() {
            return [
              {
                tag     : "script",
                attrs   : { "data-uf-dev-head-preamble": "react-devtools" },
                children: "hook()",
                injectTo: "head-prepend",
              },
            ];
          },
        },
      ],
    });
    try {
      return await transformRenderedHead(server, "/", html, ENTRY);
    } finally {
      await server.close();
    }
  }

  it("writes every URL with the base exactly once", async () => {
    const out = await transformed("/docs/", HEAD);
    expect(out).not.toContain("/docs/docs/");
    expect(out).toContain('href="/docs/logo.svg"');
    expect(out).toContain('src="/docs/theme.js"');
    expect(out).toContain('href="/docs/app/app.css"');
    expect(out).toContain(`src="/docs${ENTRY}"`);
    expect(out).toContain('src="/docs/@vite/client"');
  }, 60_000);

  it("leaves the layout's script as the first script in the head", async () => {
    // The shape React 19 hydrates: it walks the head's children in order and
    // compares a `<script>` with the first `<script>` it finds. The layout's has
    // to be that one, and everything Vite and uf injected has to come after it.
    const out = await transformed("/", HEAD.replaceAll("/docs/", "/"));
    const head = out.slice(out.indexOf("<head>"), out.indexOf("</head>"));
    const scripts = [...head.matchAll(/<script\b[^>]*>/g)].map(([tag]) => tag);
    expect(scripts[0]).toBe('<script src="/theme.js">');
    expect(scripts.at(-1)).toContain(ENTRY);
    expect(scripts.some((tag) => tag.includes("/@vite/client"))).toBe(true);
    expect(scripts.some((tag) => tag.includes("data-uf-dev-head-preamble"))).toBe(true);
    // And the order the scripts run in is unchanged: every injection before
    // the client entry, which is still last.
    expect(head.indexOf("hook()")).toBeLessThan(head.indexOf(ENTRY));
  }, 60_000);
});
