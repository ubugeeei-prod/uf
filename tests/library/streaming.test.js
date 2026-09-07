// @flow
//
// `_uf.loading.js` and a renderer that streams.
//
// Before this the renderer was `renderToString`: the whole tree had to resolve
// before a byte left, there was no boundary to fall back to, and the route
// grammar had no name for one. `suspense: true` was in the config and nothing
// read it. See ubugeeei-prod/uf#254.
//
// The claim being tested is one sentence — a page that awaits, with a
// `_uf.loading.js` beside it, sends its layouts and the fallback before the
// await resolves, and `uf build` still writes a complete document for the same
// route — and it takes four sections: the scanner finds the file, the route
// table carries it, `RouteView` puts a `<Suspense>` where it belongs, and the
// renderer's chunks arrive in the right order.
//
// # Why the ordering is asserted on chunks rather than on a socket
//
// "The first bytes arrived before the page did" is a claim about time, and the
// honest way to make it is to record when each chunk of the document was
// produced and check that the one holding the fallback came first. A stream
// collected in process is still a stream: the chunk boundaries are React's, not
// the network's, and a test that binds a port would be asserting the same thing
// through an operating system that has nothing to do with it.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";
import { act, render, screen } from "@uniflowed/react-testing";
import { RouteView, RouterProvider, resolveMatch, routerView } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterAll, describe, expect, it } from "@uniflowed/test";

// Not a package export: the Web-standard branch of `renderDocument` is
// unreachable in this process — Node has `renderToPipeableStream` — so it is
// driven directly, with a renderer of the test's own.
import { renderWithReadableStream } from "../../packages/router/internal/stream.js";
import { RESERVED, routesModuleSource, scanRoutes } from "../../packages/vite/internal/routes.js";

// `@uniflowed/router/client` statically imports `react-dom/client`, which reads
// `document` while it is being evaluated — so the DOM has to exist before the
// *import* and not merely before the first render. `rsc-split.test.js` reaches
// for the same two for the same reason, and says so at greater length.
import { installDom } from "../../packages/react-testing/internal/dom.js";

async function clientModule() {
  installDom();
  return import("@uniflowed/router/client");
}

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root on disk, from a map of relative path to file contents. */
function appRoot(files: { readonly [string]: string }): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-streaming-"));
  roots.push(root);
  for (const [relative, contents] of Object.entries(files)) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, String(contents));
  }
  return root;
}

const assets = { scripts: [], styles: [], preloads: [] };

// ---------------------------------------------------------------------------
// The reserved name
// ---------------------------------------------------------------------------

describe("scanning for `_uf.loading.js`", () => {
  it("reserves the name", () => {
    // The other half of this is `every_name_the_build_router_reserves_is_a_role`
    // in `crates/uf_router/tests/reserved_names.rs`, which fails if `loading`
    // is a role here and not in `ReservedRole`, or the other way round.
    expect(RESERVED.loading).toBe("_uf.loading");
  });

  it("gives a route the boundary declared in its own segment", () => {
    const root = appRoot({
      "_uf.layout.js": "export default function Layout() {}",
      "slow/_uf.page.js": "export default function Page() {}",
      "slow/_uf.loading.js": "export default function Loading() {}",
    });

    const { routes } = scanRoutes(root);

    expect(routes.length).toBe(1);
    expect(routes[0].loading.length).toBe(1);
    // One layout is in scope at `slow/` — the root's — and it is outside the
    // boundary. That is what makes the layout part of the shell.
    expect(routes[0].loading[0].above).toBe(1);
    expect(routes[0].loading[0].module).toBe(path.join(root, "slow", "_uf.loading.js"));
  });

  it("counts a segment's own layout as outside its own fallback", () => {
    // The fallback shows *inside* the frame the segment draws, so a segment
    // that declares both a layout and a loading file has the layout above.
    const root = appRoot({
      "slow/_uf.layout.js": "export default function Layout() {}",
      "slow/_uf.loading.js": "export default function Loading() {}",
      "slow/_uf.page.js": "export default function Page() {}",
    });

    const { routes } = scanRoutes(root);

    expect(routes[0].layouts.length).toBe(1);
    expect(routes[0].loading[0].above).toBe(1);
  });

  it("nests the boundaries a route inherits, outermost first", () => {
    const root = appRoot({
      "_uf.layout.js": "export default function Layout() {}",
      "_uf.loading.js": "export default function Loading() {}",
      "docs/_uf.layout.js": "export default function Layout() {}",
      "docs/_uf.loading.js": "export default function Loading() {}",
      "docs/deep/_uf.page.js": "export default function Page() {}",
    });

    const { routes } = scanRoutes(root);

    expect(routes[0].loading.map((boundary) => boundary.above)).toEqual([1, 2]);
  });

  it("gives a route with no loading file above it none", () => {
    const root = appRoot({
      "_uf.page.js": "export default function Page() {}",
    });

    expect(scanRoutes(root).routes[0].loading).toEqual([]);
  });

  it("puts the modules in the generated table, deduplicated", () => {
    // One `app/_uf.loading.js` is the fallback of every route under it. Fifty
    // routes must not be fifty imports of the same file.
    const root = appRoot({
      "_uf.loading.js": "export default function Loading() {}",
      "a/_uf.page.js": "export default function Page() {}",
      "b/_uf.page.js": "export default function Page() {}",
    });

    const source = routesModuleSource(scanRoutes(root));

    expect(source).toContain("loading: [{ above: 0, module: loading0 }]");
    expect(source.split("_uf.loading.js").length - 1).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Where the boundary goes in the tree
// ---------------------------------------------------------------------------

/** A promise a test resolves by hand, plus the page that waits on it. */
function deferred(): {| readonly promise: Promise<string>, readonly resolve: () => void |} {
  let settle: (value: string) => void = () => {};
  const promise = new Promise<string>((resolve) => {
    settle = resolve;
  });
  return { promise, resolve: () => settle("the page is here") };
}

/** A route table of one route: a layout, a page that waits, and a fallback. */
function suspendingTable(waited: Promise<string>, options?: {| readonly loading?: boolean |}) {
  component SlowPage() {
    return <p>{use(waited)}</p>;
  }
  component SiteLayout(children: React.Node) {
    return (
      <div>
        <nav>the layout is here</nav>
        {children}
      </div>
    );
  }
  component Loading() {
    return <p>the fallback is here</p>;
  }
  return {
    routes: [
      {
        path: "/slow",
        params: [],
        mdx: false,
        file: "app/slow/_uf.page.js",
        page: () => Promise.resolve({ default: SlowPage }),
        layouts: [() => Promise.resolve({ default: SiteLayout })],
        loading:
          options?.loading === false
            ? []
            : [{ above: 1, module: () => Promise.resolve({ default: Loading }) }],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("the `<Suspense>` in the tree", () => {
  it("renders the fallback while the page waits, and the layout around both", async () => {
    const waited = deferred();
    const table = suspendingTable(waited.promise);
    const resolved = await resolveMatch(table, "/slow");

    // Rendered inside an awaited `act`, because the tree suspends on the way in:
    // `render`'s own scope is synchronous, and a component that suspends inside
    // one leaves React with an update it will land after the scope has closed.
    await act(async () => {
      render(
        <RouterProvider url="/slow" initial={resolved}>
          <RouteView />
        </RouterProvider>,
      );
    });

    expect(screen.getByText("the fallback is here")).toBeTruthy();
    expect(screen.getByText("the layout is here")).toBeTruthy();

    // Inside `act`, because settling the promise is what makes React re-render
    // the boundary and the assertion below is about the render, not the
    // promise.
    await act(async () => {
      waited.resolve();
      await waited.promise;
    });

    expect(await screen.findByText("the page is here")).toBeTruthy();
    // The layout survived the boundary resolving; it was never inside it.
    expect(screen.getByText("the layout is here")).toBeTruthy();
  });

  it("wraps nothing when the segment declares no fallback", async () => {
    // A `<Suspense fallback={null}>` inserted just in case would be worse than
    // none: a page that suspends with no boundary above it would render as an
    // empty document instead of failing the way React says it should.
    const waited = deferred();
    const table = suspendingTable(waited.promise, { loading: false });
    const resolved = await resolveMatch(table, "/slow");

    expect(resolved.loading).toEqual([]);
  });

  it("renders no boundary either, so the page is waited for rather than dropped", async () => {
    // The half the assertion above cannot make. `resolved.loading` is what the
    // route *declares*, and an unconditional `<Suspense fallback={null}>` in
    // `RouteView` would leave that empty list exactly as it is while replacing
    // a suspended page with an empty tree. The document would go out complete,
    // with a hole where the page was, and nothing would say so — which is the
    // shape of failure this whole pull request is named after.
    //
    // Driven through the renderer rather than through `render()`, because in a
    // client root a tree with no boundary and a tree with a `null` fallback
    // both paint nothing and the difference is not in the DOM. On a server it
    // is exactly visible: with no boundary React holds the shell until the page
    // resolves, and with one it sends the shell immediately and patches the
    // content in afterwards with `$RC(`.
    const waited = deferred();
    const table = suspendingTable(waited.promise, { loading: false });
    const renderer = createRenderer({ App: routerView("./app"), ...table });

    let settled = false;
    setTimeout(() => {
      settled = true;
      waited.resolve();
    }, 60);
    const result = await renderer.render("/slow", assets);

    // The claim, in one line: the shell could not leave before the page did.
    // A boundary — declared or inserted "just in case" — would have let it out
    // while this was still false.
    expect(settled).toBe(true);

    const document = await result.text();
    expect(document).toContain("the page is here");
    expect(document).toContain("the layout is here");
    // No boundary was patched in behind the reader's back.
    expect(document).not.toContain("$RC(");
  });
});

// ---------------------------------------------------------------------------
// The renderer
// ---------------------------------------------------------------------------

/** Read a document's chunks, recording when each one was produced. */
async function chunksOf(result: {
  readonly stream: () => ReadableStream,
  ...
}): Promise<Array<{| readonly at: number, readonly text: string |}>> {
  const started = Date.now();
  const decoder = new TextDecoder();
  const reader = result.stream().getReader();
  const out = [];
  while (true) {
    const { done, value } = await reader.read();
    if (done === true) {
      return out;
    }
    out.push({ at: Date.now() - started, text: decoder.decode(value, { stream: true }) });
  }
}

/**
 * `html` with the render anchor's envelope blanked out.
 *
 * The anchor is the one part of a document that is a fact about *this* render
 * rather than about the route — see `RenderProvider` in
 * `@uniflowed/hooks/render` — so two renders of one route differ there and
 * nowhere else. Blanking the value rather than deleting the element keeps the
 * element's position in the comparison: a document that hoisted the anchor
 * somewhere else would still fail.
 */
function withoutTheAnchor(html: string): string {
  return html.replace(/(<meta name="uf:render" content=")[^"]*"/, '$1"');
}

describe("rendering a route that suspends", () => {
  it("sends the layout and the fallback before the page resolves", async () => {
    const waited = deferred();
    const table = suspendingTable(waited.promise);
    const renderer = createRenderer({ App: routerView("./app"), ...table });

    const result = await renderer.render("/slow", assets);
    // Resolved before the page's promise has been settled at all: this is the
    // whole claim. `renderToString` could not have got here.
    expect(result.status).toBe(200);

    const reading = chunksOf(result);
    // Long enough that a renderer which waited for the page would have had to
    // wait for this, and short enough not to slow the suite down.
    setTimeout(waited.resolve, 120);
    const chunks = await reading;

    const shell = chunks[0];
    expect(shell.text).toContain("the layout is here");
    expect(shell.text).toContain("the fallback is here");
    expect(shell.text).not.toContain("the page is here");
    // The shell is out before the page's promise settles, not merely first in
    // the list: a renderer that buffered would have produced both chunks after
    // the timer.
    expect(shell.at < 120).toBe(true);

    const rest = chunks
      .slice(1)
      .map((chunk) => chunk.text)
      .join("");
    expect(rest).toContain("the page is here");
    expect(chunks.length > 1).toBe(true);
  });

  it("writes the head before any of the body", async () => {
    // `packages/web/head.js` documents this from the other side, as the reason
    // `useHead` does nothing on a server. It has to be true of the bytes.
    const waited = deferred();
    const table = suspendingTable(waited.promise);
    const renderer = createRenderer({ App: routerView("./app"), ...table });

    const result = await renderer.render("/slow", {
      scripts: ["/assets/client.js"],
      styles: ["/assets/app.css"],
      preloads: [],
    });
    setTimeout(waited.resolve, 0);
    const document = (await chunksOf(result)).map((chunk) => chunk.text).join("");

    expect(document).toContain('<link rel="stylesheet" href="/assets/app.css">');
    expect(document.indexOf("</head>")).toBeLessThan(document.indexOf("the layout is here"));
    expect(document.indexOf("/assets/app.css")).toBeLessThan(document.indexOf("</head>"));
    expect(document.indexOf("/assets/client.js")).toBeLessThan(document.indexOf("</head>"));
  });

  it("gives the same document however the host takes it", async () => {
    // A route that does not suspend, so the three renders are comparable: with
    // a boundary in play React legitimately writes a different document
    // depending on whether the page had already resolved when the shell was
    // ready, and that difference is the feature rather than something to assert
    // against.
    //
    // The one thing that does differ between them is the render anchor.
    // `routerView` renders a `RenderProvider` above every application
    // (ubugeeei-prod/uf#559), and what it fixes is *this* render's instant and
    // *this* render's seed — so three renders carry three envelopes, by
    // construction rather than by accident. `withoutTheAnchor` takes that one
    // element out and the rest is compared byte for byte; that every document
    // has exactly one of them is asserted rather than assumed, because "the
    // anchor is missing" and "the anchor is different" would otherwise look
    // the same here.
    component Page() {
      return <p>the page is here</p>;
    }
    const renderer = createRenderer({ App: routerView("./app"), ...tableOf(Page, []) });

    const piped = [];
    await (
      await renderer.render("/", assets)
    ).pipe({ write: (chunk) => piped.push(chunk), end: () => {} });

    const collected = await (await renderer.render("/", assets)).text();
    const streamed = await new Response((await renderer.render("/", assets)).stream()).text();

    for (const document of [piped.join(""), collected, streamed]) {
      expect(document.split('name="uf:render"').length - 1).toBe(1);
    }
    expect(withoutTheAnchor(piped.join(""))).toBe(withoutTheAnchor(collected));
    expect(withoutTheAnchor(streamed)).toBe(withoutTheAnchor(collected));
    expect(collected).toContain("the page is here");
  });
});

// ---------------------------------------------------------------------------
// The loader, which is the thing a page most often waits for
// ---------------------------------------------------------------------------
//
// Everything above is a page that suspends while *rendering*. A page whose
// `loader` is slow could not stream at all: `resolveRoute` awaited it before it
// returned, so by the time React saw the tree the data was already in hand and
// the fallback beside it showed for no time at all. See ubugeeei-prod/uf#373.

/**
 * A one-route table whose page has a loader the test settles by hand.
 *
 * `page` is what the loader's value renders as, so an assertion on the markup
 * is an assertion about the data having arrived rather than about a component
 * having run.
 */
function loaderTable(
  waited: Promise<string>,
  options?: {|
    readonly loading?: boolean,
    readonly generateMetadata?: boolean,
  |},
) {
  component DataPage(data: mixed) {
    return <p>{typeof data === "string" ? data : "no data"}</p>;
  }
  component SiteLayout(children: React.Node) {
    return (
      <div>
        <nav>the layout is here</nav>
        {children}
      </div>
    );
  }
  component Loading() {
    return <p>the fallback is here</p>;
  }
  const page = {
    default: DataPage,
    loader: () => waited,
    ...(options?.generateMetadata === true
      ? {
          generateMetadata: (args: { readonly data: mixed, ... }) => ({ title: String(args.data) }),
        }
      : {}),
  };
  return {
    routes: [
      {
        path: "/slow",
        params: [],
        mdx: false,
        file: "app/slow/_uf.page.js",
        page: () => Promise.resolve(page),
        layouts: [() => Promise.resolve({ default: SiteLayout })],
        loading:
          options?.loading === false
            ? []
            : [{ above: 1, module: () => Promise.resolve({ default: Loading }) }],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("rendering a route whose loader is slow", () => {
  it("sends the layout and the fallback before the loader resolves", async () => {
    // The bug, in one assertion. `resolveMatch` awaited the loader, so
    // `renderer.render` did not resolve until the loader had — and every route's
    // time to first byte was its slowest loader however many `_uf.loading.js`
    // files were beside it.
    const waited = deferred();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...loaderTable(waited.promise),
    });

    let settled = false;
    setTimeout(() => {
      settled = true;
      waited.resolve();
    }, 120);
    const result = await renderer.render("/slow", assets);

    // The shell is ready while the loader is still running. Before this it
    // could not be: the loader was awaited before React saw the tree at all.
    expect(settled).toBe(false);
    expect(result.status).toBe(200);

    const chunks = await chunksOf(result);
    const shell = chunks[0];
    expect(shell.text).toContain("the layout is here");
    expect(shell.text).toContain("the fallback is here");
    expect(shell.text).not.toContain("the page is here");
    expect(shell.at < 120).toBe(true);

    const rest = chunks
      .slice(1)
      .map((chunk) => chunk.text)
      .join("");
    expect(rest).toContain("the page is here");
  });

  it("waits for a loader whose data the route's metadata is generated from", async () => {
    // The rule the design states rather than hides: metadata goes in the head,
    // the head is written before the body, so a title computed from the data
    // genuinely cannot be deferred. A page that wants to stream keeps its
    // metadata static.
    const waited = deferred();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...loaderTable(waited.promise, { generateMetadata: true }),
    });

    let settled = false;
    setTimeout(() => {
      settled = true;
      waited.resolve();
    }, 40);
    const result = await renderer.render("/slow", assets);

    expect(settled).toBe(true);
    const document = (await chunksOf(result)).map((chunk) => chunk.text).join("");
    expect(document).toContain("<title>the page is here</title>");
  });

  it("waits for a loader the segment declares no fallback for", async () => {
    // Nothing to defer into. A page held back by a boundary that does not exist
    // is a page React holds the whole shell for, which is what it already did —
    // so the loader is awaited and the document is the one this route had
    // before any of this.
    const waited = deferred();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...loaderTable(waited.promise, { loading: false }),
    });

    let settled = false;
    setTimeout(() => {
      settled = true;
      waited.resolve();
    }, 40);
    const result = await renderer.render("/slow", assets);

    expect(settled).toBe(true);
    const document = await result.text();
    expect(document).toContain("the page is here");
    expect(document).not.toContain("$RC(");
  });

  it("embeds the deferred data, which the head had already gone without", async () => {
    // `dataScript` wrote the loader's answer into the head, and a deferred
    // answer is not known when the head goes out. Losing it would mean the
    // browser running every deferred route's loader a second time on the way
    // in, so it is rendered in the tree instead — on both sides, which is what
    // keeps hydration matching.
    const waited = deferred();
    const renderer = createRenderer({
      App: routerView("./app"),
      ...loaderTable(waited.promise),
    });

    const result = await renderer.render("/slow", assets);
    setTimeout(waited.resolve, 0);
    const document = (await chunksOf(result)).map((chunk) => chunk.text).join("");

    expect(document).toContain('<script id="__uf_data" type="application/json">');
    expect(document).toContain('"the page is here"');
  });

  it("writes the same data element whether it deferred the loader or not", async () => {
    // Which is what makes the hydration test below cover both. A deferred
    // render sends the element inside the boundary and patches it into place
    // with `$RC()`; a static one writes it where it lands. The document a
    // browser holds once the stream has finished is the same either way, and
    // this is that claim without running React's patch script in a test.
    const streamedWait = deferred();
    const streamed = createRenderer({
      App: routerView("./app"),
      ...loaderTable(streamedWait.promise),
    });
    const result = await streamed.render("/slow", assets);
    setTimeout(streamedWait.resolve, 0);
    const document = (await chunksOf(result)).map((chunk) => chunk.text).join("");

    const staticWait = deferred();
    setTimeout(staticWait.resolve, 0);
    const built = await createRenderer({
      App: routerView("./app"),
      ...loaderTable(staticWait.promise),
    }).prerender("/slow", assets);

    const element = /<script id="__uf_data"[^>]*>[^<]*<\/script>/;
    const fromStream = document.match(element);
    expect(fromStream).toBeTruthy();
    expect(built.html).toContain(fromStream?.[0] ?? "no data element was streamed");
  });
});

// ---------------------------------------------------------------------------
// And the browser, which has to be able to pick it up again
// ---------------------------------------------------------------------------

describe("hydrating a route whose loader answered on the server", () => {
  /**
   * One table, and a count of how often its loader ran.
   *
   * Built once and shared by both renders on purpose: `loadOnce` caches a
   * module by the identity of the function that loads it, and hydration is
   * React comparing two renders of the same components. Two tables would be two
   * sets of components and the comparison would mean nothing.
   */
  function countedTable(seen: { loads: number, ... }) {
    component DataPage(data: mixed) {
      return <p>{typeof data === "string" ? data : "no data"}</p>;
    }
    component Loading() {
      return <p>the fallback is here</p>;
    }
    return {
      routes: [
        {
          path: "/slow",
          params: [],
          mdx: false,
          file: "app/slow/_uf.page.js",
          page: () =>
            Promise.resolve({
              default: DataPage,
              loader: () => {
                seen.loads += 1;
                return "the page is here";
              },
            }),
          layouts: [],
          loading: [{ above: 0, module: () => Promise.resolve({ default: Loading }) }],
        },
      ],
      notFound: [],
      errors: [],
    };
  }

  it("reads the embedded data rather than running the loader a second time", async () => {
    // The half of ubugeeei-prod/uf#373 that is about the browser. The loader's
    // answer used to be written into the head, where `hydrate` reads it before
    // `hydrateRoot`; a deferred answer does not exist when the head goes out, so
    // it moved into the tree. If the browser could not find it there, every
    // deferred route would fetch its data twice — once for the document and
    // once on the way in — and nothing would say so.
    const seen = { loads: 0 };
    const table = countedTable(seen);
    const renderer = createRenderer({ App: routerView("./app"), ...table });

    const { html } = await renderer.prerender("/slow", assets);
    expect(seen.loads).toBe(1);
    expect(html).toContain('<script id="__uf_data"');

    installDom();
    const parsed = new globalThis.DOMParser().parseFromString(html, "text/html");
    const rendered = parsed.getElementById("uf-root");
    const root = globalThis.document.createElement("div");
    root.id = "uf-root";
    root.innerHTML = rendered?.innerHTML ?? "";
    globalThis.document.body.replaceChildren(root);
    globalThis.window.history.pushState(null, "", "/slow");

    const { hydrate } = await clientModule();
    await act(async () => {
      await hydrate({ App: routerView("./app"), ...table });
    });

    // Still one: the browser found the answer the document carried.
    expect(seen.loads).toBe(1);
    expect(globalThis.document.getElementById("uf-root")?.textContent).toContain(
      "the page is here",
    );
  });

  it("hoists the route's metadata into the head it is hydrated beside", async () => {
    // The two fixes meeting. `Head` renders a `<title>` in the tree, uf's shell
    // lifts it into the real head (#547), and React on the client claims that
    // element rather than making a second one — so a hydrated document has one
    // title, in the head, and the body has none.
    const seen = { loads: 0 };
    const table = countedTable(seen);
    table.routes[0].page = () =>
      Promise.resolve({
        default: (props: { readonly data: mixed, ... }) => (
          <p>{typeof props.data === "string" ? props.data : "no data"}</p>
        ),
        loader: () => "the page is here",
        metadata: { title: "The manual", canonical: "https://docs.uniflowed.dev/slow" },
      });
    const { html } = await createRenderer({
      App: routerView("./app"),
      ...table,
    }).prerender("/slow", assets);

    installDom();
    const parsed = new globalThis.DOMParser().parseFromString(html, "text/html");
    expect(parsed.head.querySelectorAll("title").length).toBe(1);
    expect(parsed.head.querySelector('link[rel="canonical"]')).toBeTruthy();
    expect(parsed.getElementById("uf-root")?.querySelector("title")).toBe(null);
  });
});

describe("prerendering the same route", () => {
  it("waits for the loader, because a file in dist/ has nothing to stream to", async () => {
    // `uf build`'s half of ubugeeei-prod/uf#373. Deferring costs the loader its
    // say in the response — a status is decided when the shell goes out — and
    // buys a first paint that a file being written to disk does not have. So
    // the static renderer resolves the loader exactly as it always did.
    const waited = deferred();
    setTimeout(waited.resolve, 20);
    const renderer = createRenderer({
      App: routerView("./app"),
      ...loaderTable(waited.promise),
    });

    const result = await renderer.prerender("/slow", assets);

    expect(result.status).toBe(200);
    expect(result.html).toContain("the page is here");
    expect(result.html).not.toContain("the fallback is here");
    expect(result.html).not.toContain("$RC(");
  });
});

describe("prerendering a route that suspends", () => {
  it("writes a document with the resolved page in it, not a fallback", async () => {
    // `uf build`'s half. A file whose slow parts are `<template>` elements
    // waiting for `$RC()` is a page that is blank to a crawler and to `curl`,
    // which is most of what a file in `dist/` is for.
    const waited = deferred();
    setTimeout(waited.resolve, 20);
    const table = suspendingTable(waited.promise);
    const renderer = createRenderer({ App: routerView("./app"), ...table });

    const result = await renderer.prerender("/slow", assets);

    expect(result.status).toBe(200);
    expect(result.html).toContain("the page is here");
    expect(result.html).toContain("the layout is here");
    expect(result.html).not.toContain("the fallback is here");
    expect(result.html).not.toContain("$RC(");
    expect(
      result.html.startsWith("<!DOCTYPE html>") || result.html.startsWith("<!doctype html>"),
    ).toBe(true);
    expect(result.html).toContain("</html>");
  });
});

// ---------------------------------------------------------------------------
// The head the shell lifts out of the app's markup
// ---------------------------------------------------------------------------

describe("hoisting head elements into the shell's own head", () => {
  // `head` is empty because it is the other shape's: an app that renders its
  // own `<html>` gets uf's tags spliced before the `</head>` React wrote, and
  // an app that does not gets them from `open` and `body` instead.
  const shell = {
    head: "",
    open: "<!doctype html><html><head>",
    body: "</head><body>",
    close: "</body></html>",
  };

  /** A `renderToReadableStream` that hands back exactly these chunks. */
  function writing(chunks: $ReadOnlyArray<string>) {
    const encoder = new TextEncoder();
    let index = 0;
    return async () => ({
      getReader: () => ({
        read: () =>
          Promise.resolve(
            index < chunks.length
              ? { done: false, value: encoder.encode(chunks[index++]) }
              : { done: true },
          ),
        releaseLock: () => {},
      }),
    });
  }

  /** The document those chunks assemble into. */
  async function documentOf(chunks: $ReadOnlyArray<string>): Promise<string> {
    const body = await renderWithReadableStream(writing(chunks), <p>unused</p>, {
      shell,
      onError: () => {},
    });
    return body.text();
  }

  it("puts them in the head wherever React split its output", async () => {
    // The reason this is driven through a renderer of the test's own rather
    // than through React: where the chunk boundaries fall is React's decision,
    // and a scanner that only worked when a tag arrived whole would be a bug
    // that appeared under load and nowhere else. Every split below is the same
    // document.
    const whole = '<title>a page</title><meta name="description" content="a>b"/><main>body</main>';
    const splits = [
      [whole],
      ["<title>a pa", "ge</title><meta name=", '"description" content="a>b"/><main>body</main>'],
      ['<title>a page</title><meta name="description" content="a>b"/>', "<main>body</main>"],
      ["<", "title>a page</title>", '<meta name="description" content="a>b"/><main>body</main>'],
    ];

    for (const chunks of splits) {
      const html = await documentOf(chunks);
      expect(html.indexOf("<title>a page</title>")).toBeLessThan(html.indexOf("</head>"));
      expect(html.indexOf('name="description"')).toBeLessThan(html.indexOf("</head>"));
      expect(html.indexOf("<main>body</main>")).toBeGreaterThan(html.indexOf("<body>"));
    }
  });

  it("stops at the first thing that is not one, however much it looks like one", async () => {
    // `<titlebar>` is somebody's component. The run ends there, and the two
    // elements after it stay in the body where they were written — hoisting
    // them would mean holding the document to look for them, which is
    // streaming in shape and buffering in fact.
    const html = await documentOf([
      '<title>a page</title><titlebar><meta name="late" content="x"/></titlebar>',
    ]);

    expect(html.indexOf("<title>a page</title>")).toBeLessThan(html.indexOf("</head>"));
    expect(html.indexOf('name="late"')).toBeGreaterThan(html.indexOf("<body>"));
  });

  it("writes a document for markup that is nothing but head elements", async () => {
    // The run never ends, so nothing tells the scanner to stop until the
    // chunks do. It must still close the document rather than wait forever.
    const html = await documentOf(["<title>only this</title>"]);

    expect(html).toBe(
      "<!doctype html><html><head><title>only this</title></head><body></body></html>",
    );
  });
});

// ---------------------------------------------------------------------------
// The other renderer, which no host in this process has
// ---------------------------------------------------------------------------

describe("the Web-standard renderer", () => {
  /**
   * A `renderToReadableStream` that starts a document and never finishes it.
   *
   * `read()` never settles, which is a page whose slowest boundary has not
   * resolved — the only state in which cancelling means anything. React's own
   * cannot be used here: this branch is reached only on a host with no
   * `renderToPipeableStream`, and Node is not one.
   */
  function neverFinishing() {
    const seen: { signal: AbortSignal | null } = { signal: null };
    const render = async (node, settings) => {
      seen.signal = settings.signal;
      return {
        getReader: () => ({
          read: () => new Promise(() => {}),
          releaseLock: () => {},
        }),
      };
    };
    return { render, seen };
  }

  const shell = {
    head: "",
    open: "<!doctype html><html><head>",
    body: "</head><body>",
    close: "</body></html>",
  };

  it("stops the render when the consumer gives up on it", async () => {
    // The Node path holds `renderToPipeableStream`'s `abort` and calls it from
    // exactly here. This one had no handle at all: `releaseLock` detaches the
    // reader and React goes on rendering into a stream nobody will read again.
    // A `HEAD` cancels, and so does a browser that navigates away, so on a
    // worker that is a render burning a metered CPU budget for a request that
    // ended.
    const { render, seen } = neverFinishing();
    const body = await renderWithReadableStream(render, <p>anything</p>, {
      shell,
      onError: () => {},
    });

    await body.stream().cancel();

    expect(seen.signal).toBeTruthy();
    expect(seen.signal?.aborted).toBe(true);
  });

  it("leaves the render alone while somebody is still reading", async () => {
    // The half that makes the above a cancellation rather than a wall: a body
    // nobody has given up on must not be aborted.
    const { render, seen } = neverFinishing();
    await renderWithReadableStream(render, <p>anything</p>, { shell, onError: () => {} });

    expect(seen.signal?.aborted).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The two document shapes, which is what `assemble` used to decide afterwards
// ---------------------------------------------------------------------------

/** A one-route table whose page is `Page` and whose layouts are `layouts`. */
function tableOf(Page: React.ComponentType<empty>, layouts: $ReadOnlyArray<mixed>) {
  return {
    routes: [
      {
        path: "/",
        params: [],
        mdx: false,
        file: "app/_uf.page.js",
        page: () => Promise.resolve({ default: Page }),
        layouts: layouts.map((layout) => () => Promise.resolve({ default: layout })),
        loading: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("the document uf writes around the app", () => {
  it("puts uf's tags in the head an app renders for itself", async () => {
    component Document(children: React.Node) {
      return (
        <html lang="en">
          <head>
            <meta charSet="utf-8" />
          </head>
          <body>{children}</body>
        </html>
      );
    }
    component Page() {
      return <main>owned</main>;
    }
    const renderer = createRenderer({
      App: routerView("./app"),
      ...tableOf(Page, [Document]),
    });

    const html = await (
      await renderer.render("/", { scripts: ["/c.js"], styles: [], preloads: [] })
    ).text();

    expect(html).toContain('<html lang="en">');
    expect(html).not.toContain('id="uf-root"');
    expect(html.indexOf("/c.js")).toBeLessThan(html.indexOf("</head>"));
  });

  it("wraps an app that renders only content in a shell it can hydrate", async () => {
    component Page() {
      return <main>content</main>;
    }
    const renderer = createRenderer({ App: routerView("./app"), ...tableOf(Page, []) });

    const html = await (
      await renderer.render("/", { scripts: ["/c.js"], styles: [], preloads: [] })
    ).text();

    expect(html).toContain('<div id="uf-root">');
    expect(html).toContain("content");
    expect(html.indexOf("/c.js")).toBeLessThan(html.indexOf("<body>"));
    expect(html.startsWith("<!doctype html>\n")).toBe(true);
    expect(html.endsWith("</html>\n")).toBe(true);
  });
});
