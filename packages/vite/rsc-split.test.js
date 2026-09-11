// @flow
//
// The server/client split, on both sides of the line it draws.
//
// `crates/uf_rsc` has always been able to say which modules a `"use client"`
// boundary is reachable from, and until now nothing read the answer: the
// generated route table gave every route a `page: () => import(<file>)`, and
// `virtual:uf/client` imported that table, so every page in an application was
// a chunk of the *client* bundle whether or not a browser had anything to do
// with it. See ubugeeei-prod/uf#252 and ubugeeei-prod/uf#350.
//
// Two halves, and the second is the one a file listing cannot show.
//
// **The table.** `@uniflowed/vite` generates the browser's copy of
// `virtual:uf/routes` without the page of any route no boundary reaches, so
// Rollup has nothing left that pulls the module in. `crates/uf_cli/tests/vite.rs`
// asserts the consequence on a real build; this asserts the decision, which is
// where a wrong answer would come from.
//
// **The runtime.** A route whose page is not in the bundle is a route this
// router cannot render, and pretending otherwise is a silent break: a link into
// it would resolve to nothing and leave the visitor where they were. So
// `hydrate` declines to mount it, a navigation into it becomes the browser's,
// and — the half that must not regress — a route that *does* have a client
// boundary still hydrates and is still interactive.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { act, cleanup, userEvent } from "@uniflowed/react-testing";
// `@uniflowed/router`, imported statically into a process that has no document
// yet — deliberately, because that is the order a `uf test` worker produces on
// its own. One worker serves many files out of one module registry, and six
// other files in this suite (`streaming`, `request-lifecycle`,
// `error-boundary`, `routing`, `middleware`, `route-handler`) import this
// package at the top of the file, long before anything installs a DOM; which
// of them shares a worker with this one changes with the schedule.
//
// ubugeeei-prod/uf#445 was the runtime answering "is there a document" once,
// at module scope, under that ordering. Hydration still worked and React still
// called the `Link`'s handler; the navigation it asked for was dropped in
// silence. Importing it here the way a server-side file does puts that
// ordering in front of the last case below on every run, rather than on the
// runs the scheduler happened to arrange it.
import { Link, routerView } from "@uniflowed/router";
import { afterAll, afterEach, describe, expect, it } from "@uniflowed/test";

// Reached by path rather than by package name, the way `routing.test.js` and
// `error-boundary.test.js` reach for the same package: `internal/` is the
// build's own router and its own split, not something a project imports.
import { installDom } from "../../packages/react-testing/internal/dom.js";
import uniflowed from "./index.js";
import { clientRouteFilter, readRscManifest } from "./internal/rsc.js";
import { routesModuleSource, scanRoutes } from "./internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/**
 * A project holding each named file, with `source` when one is given.
 *
 * Real files, because `scanRoutes` is `readdirSync` and `statSync`: a fixture
 * that replaced them would prove the sort order and nothing about which page
 * the split kept.
 */
function project(files: { readonly [string]: string }): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-rsc-split-"));
  roots.push(root);
  for (const relative of Object.keys(files)) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, files[relative]);
  }
  return root;
}

/** One module, as the manifest describes it. */
function manifestModule(modulePath: string, reaches: boolean) {
  return {
    path: modulePath,
    environment: "server",
    reachability: "server-only",
    proximity: reaches ? "reaches-boundary" : "isolated",
    imports: [],
    externalImports: [],
    exports: ["default"],
  };
}

/**
 * Write a manifest into `root` and read it back through the real reader.
 *
 * Through the file rather than as an object, because the file is the contract:
 * `uf build` writes it in Rust and the plugin reads it in JavaScript, and the
 * version check is part of what is being tested.
 */
function manifestIn(root: string, manifest: mixed): mixed {
  const file = path.join(root, ".uf", "rsc", "uf-rsc-manifest.json");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
  return readRscManifest(file);
}

/** A project with a static route, an interactive one, and one shared layout. */
function splitProject(): string {
  const page = "// @flow\nexport default function Page() {}\n";
  return project({
    "app/$layout.js": page,
    "app/$page.js": page,
    "app/counter/$page.js": page,
  });
}

/** The manifest that project's analysis would produce. */
function splitManifest(version: number = 2) {
  return {
    version,
    engine: "uf-native",
    buildFingerprint: "0".repeat(64),
    modules: [
      manifestModule("app/$layout.js", false),
      manifestModule("app/$page.js", false),
      manifestModule("app/counter/$page.js", true),
    ],
    clientBoundaries: [],
    clientBundleRoots: [],
    serverActions: [],
    diagnostics: [],
  };
}

describe("the client route table", () => {
  it("keeps the page of a route that reaches a client boundary", () => {
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));
    const shipsPage = clientRouteFilter(manifestIn(root, splitManifest()), root, table);

    const source = routesModuleSource(table, { shipsPage });

    expect(source).toContain("app/counter/$page.js");
    // The layout too: the browser re-renders the whole matched tree, so a
    // route that hydrates needs everything above the boundary as well.
    expect(source).toContain("app/$layout.js");
  });

  it("drops the page of a route that reaches none, which is the whole point", () => {
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));
    const shipsPage = clientRouteFilter(manifestIn(root, splitManifest()), root, table);

    const source = routesModuleSource(table, { shipsPage });

    // The path stays: the router still has to *match* the URL, because that is
    // what tells a `Link` the destination is a document to fetch rather than a
    // 404. What goes is the `import()`, which is the only thing in this table
    // a bundler follows.
    expect(source).toContain('path: "/"');
    expect(source).not.toContain(`import(${JSON.stringify(path.join(root, "app/$page.js"))})`);
  });

  it("still imports a dropped page for its side effects, which is its stylesheet", () => {
    // The half that was missing the first time this was written. A uf build
    // links the stylesheets it finds in the *client* graph, so a route removed
    // from that graph outright loses its rules — from every page of the site,
    // because the linked sheets are the whole graph's. A bare import with
    // nothing read from it keeps the stylesheet and leaves the components,
    // helpers and data as unused exports for the bundler to drop.
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));
    const shipsPage = clientRouteFilter(manifestIn(root, splitManifest()), root, table);

    const source = routesModuleSource(table, { shipsPage });

    expect(source).toContain(`import ${JSON.stringify(path.join(root, "app/$page.js"))};`);
    // Not the layout: `/counter` keeps it, so it is already in the table as a
    // lazy import, and a second static one would pull it into the entry chunk.
    expect(source).not.toContain(`import ${JSON.stringify(path.join(root, "app/$layout.js"))};`);
  });

  it("ships every page when there is no manifest to read", () => {
    // A project driving Vite itself, with no `uf build` or `uf dev` to write
    // the analysis. It gets the table it has always had rather than a split
    // guessed at from nothing.
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));

    const source = routesModuleSource(table, {
      shipsPage: clientRouteFilter(readRscManifest(undefined), root, table),
    });

    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/$page.js"))})`);
  });

  it("ships every page when the manifest is older than the field it needs", () => {
    // Version 1 published the boundaries and nothing that said which modules
    // sat above one. Read optimistically it would answer `undefined` for every
    // module, compare unequal to `"reaches-boundary"`, and drop the whole
    // application from the browser.
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));
    const shipsPage = clientRouteFilter(manifestIn(root, splitManifest(1)), root, table);

    const source = routesModuleSource(table, { shipsPage });

    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/$page.js"))})`);
  });

  it("ships a page the analysis never saw", () => {
    // `.mdx` is a page and is not `.js`, so it is in no manifest. The honest
    // reading of a module uf did not analyse is that it might reach a
    // boundary, and every unknown answers so — which is why this split can
    // only ever drop a route uf positively decided needs no browser.
    const root = project({
      "app/$layout.js": "// @flow\nexport default function Layout() {}\n",
      "app/$page.mdx": "# home\n",
    });
    const table = scanRoutes(path.join(root, "app"));
    const manifest = manifestIn(root, {
      ...splitManifest(),
      modules: [manifestModule("app/$layout.js", false)],
    });

    const source = routesModuleSource(table, {
      shipsPage: clientRouteFilter(manifest, root, table),
    });

    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/$page.mdx"))})`);
  });

  it("generates the whole table by default, which is what the server gets", () => {
    const root = splitProject();

    const source = routesModuleSource(scanRoutes(path.join(root, "app")));

    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/$page.js"))})`);
    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/counter/$page.js"))})`);
  });

  it("states each route's file relative to the project when asked", () => {
    // Every `import()` in this table is a specifier the bundler rewrites to a
    // chunk URL, so no absolute path survives a build through one. `file` is a
    // string, and it does: uf's own manual shipped the build machine's
    // absolute path for each of thirty-four routes to every visitor, which
    // says where the machine keeps its files and who its user is, to a
    // browser that has no filesystem to resolve any of it against.
    //
    // `relativeTo` is what the client call passes and the server call does
    // not. The `import()` specifier is untouched — it still has to resolve —
    // and only the string a reader sees changes.
    const root = splitProject();
    const table = scanRoutes(path.join(root, "app"));

    const source = routesModuleSource(table, { relativeTo: root });

    expect(source).toContain(`file: ${JSON.stringify("app/$page.js")}`);
    expect(source).toContain(`file: ${JSON.stringify("app/counter/$page.js")}`);
    expect(source).toContain(`import(${JSON.stringify(path.join(root, "app/$page.js"))})`);
    expect(source.includes(`file: ${JSON.stringify(path.join(root, "app/$page.js"))}`)).toBe(false);
  });

  it("leaves the file absolute when nothing asked for a root", () => {
    // The server's copy, which is read on the machine that has those files and
    // where a diagnostic naming an absolute path is the useful one.
    const root = splitProject();

    const source = routesModuleSource(scanRoutes(path.join(root, "app")));

    expect(source).toContain(`file: ${JSON.stringify(path.join(root, "app/$page.js"))}`);
  });
});

// ---------------------------------------------------------------------------
// The runtime
// ---------------------------------------------------------------------------

/**
 * The two entry points that sit either side of a request, imported only once a
 * document exists.
 *
 * `@uniflowed/router/client` statically imports `react-dom/client`, which
 * reads `document` while it is being evaluated. For these two the DOM has to
 * exist before the *import* and not merely before the first render, which is
 * the whole of what these wrappers are for. `installDom` is idempotent, and
 * each dynamic import is evaluated once, on the first call.
 */
async function clientModule() {
  installDom();
  return import("@uniflowed/router/client");
}

async function serverModule() {
  installDom();
  return import("@uniflowed/router/server");
}

/**
 * The two route tables, built once, with the components that need the router.
 *
 * Memoised because `loadOnce` in the runtime caches a module by the identity
 * of the function that loads it, and because the server render and the
 * hydration below have to be the same components for the comparison React
 * makes to mean anything.
 */
let built: mixed = null;

function tables() {
  if (built != null) {
    return built;
  }

  /** A counter with state, standing in for a `"use client"` component. */
  component Counter() {
    const [count, setCount] = useState<number>(0);
    return (
      <p>
        <output>{count}</output>
        <button type="button" onClick={() => setCount(count + 1)}>
          add one
        </button>
      </p>
    );
  }

  component CounterPage() {
    return (
      <section>
        <h1>counter</h1>
        <Counter />
        <Link to="/">home</Link>
      </section>
    );
  }

  component StaticPage() {
    return <h1>static, and nothing attaches to it</h1>;
  }

  const counterPage = { default: CounterPage };
  const staticPage = { default: StaticPage };

  const home = {
    path: "/",
    params: [],
    mdx: false,
    file: "app/$page.js",
    page: () => Promise.resolve(staticPage),
    layouts: [],
    loading: [],
  };
  const counter = {
    path: "/counter",
    params: [],
    mdx: false,
    file: "app/counter/$page.js",
    page: () => Promise.resolve(counterPage),
    layouts: [],
    loading: [],
  };

  built = {
    // Every route has its page: this is what the server renders from.
    server: [home, counter],
    // The browser's copy. `/` kept its path — the router still has to match
    // the URL — and lost the one property a bundler follows.
    client: [
      { path: home.path, params: home.params, mdx: home.mdx, file: home.file, layouts: [] },
      counter,
    ],
  };
  return built;
}

/**
 * Put the document the server would have written into the live DOM, and go to
 * that URL.
 *
 * The markup comes from the real server renderer rather than being written by
 * hand, because hydration is React comparing what it renders against what the
 * server sent: markup a test invented would prove that `hydrateRoot` was
 * called and nothing about whether it matched.
 */
async function serve(url: string): Promise<void> {
  const { createRenderer, ROOT_ID } = await serverModule();
  const { server } = tables();
  const renderer = createRenderer({
    App: routerView("./app"),
    routes: server,
    notFound: [],
    errors: [],
  });
  const { html } = await renderer.prerender(url, { scripts: [], styles: [], preloads: [] });

  const parsed = new globalThis.DOMParser().parseFromString(html, "text/html");
  const rendered = parsed.getElementById(ROOT_ID);
  if (rendered == null) {
    throw new Error(`the server wrote no #${ROOT_ID}:\n${html}`);
  }
  const root = globalThis.document.createElement("div");
  root.id = ROOT_ID;
  root.innerHTML = rendered.innerHTML;
  globalThis.document.body.replaceChildren(root);
  globalThis.window.history.pushState(null, "", url);
}

/** Hydrate the current document with the browser's copy of the table. */
async function hydrateHere(): Promise<void> {
  const { hydrate } = await clientModule();
  const { client } = tables();
  await act(async () => {
    await hydrate({ App: routerView("./app"), routes: client, notFound: [], errors: [] });
  });
}

/** What is on the page right now. */
function ufRoot(): Element | null {
  return globalThis.document.getElementById("uf-root");
}

afterEach(() => {
  // Only once a document exists: the table tests above never install one, and
  // every hook in this file runs for every test in it.
  if (globalThis.document == null) {
    return;
  }
  cleanup();
  globalThis.document.body.replaceChildren();
});

describe("hydration across the boundary", () => {
  it("hydrates a route whose page is in the bundle, and it works", async () => {
    await serve("/counter");
    expect(ufRoot()?.textContent).toContain("counter");

    await hydrateHere();

    // Interactive, which is the only proof that React attached to the server's
    // markup rather than beside it.
    expect(ufRoot()?.querySelector("output")?.textContent).toBe("0");
    const button = ufRoot()?.querySelector("button");
    if (button == null) {
      throw new Error(`no button in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
    }
    await act(async () => {
      await userEvent.click(button);
    });
    expect(ufRoot()?.querySelector("output")?.textContent).toBe("1");
  });

  it("mounts nothing on a route whose page is not in the bundle", async () => {
    await serve("/");
    const served = ufRoot()?.innerHTML ?? "";
    expect(served).toContain("static, and nothing attaches to it");

    await hydrateHere();

    // Untouched: not blanked by a client render that found no page, and not
    // replaced by an error boundary either. The document the server wrote is
    // the whole of this route.
    expect(ufRoot()?.innerHTML).toBe(served);
  });
});

describe("navigating into a route that ships no page", () => {
  it("hands the URL to the browser instead of rendering nothing", async () => {
    await serve("/counter");
    await hydrateHere();

    // Every `Link` renders a real anchor and only takes over a plain left
    // click, so this is the click a visitor makes. Without the check in
    // `navigate` it resolves a route with no page, the resolution fails, and
    // the visitor is left on the page they clicked from — which is the silent
    // break this test exists for.
    const assigned: Array<string> = [];
    const location = globalThis.window.location;
    const original = location.assign;
    Object.defineProperty(location, "assign", {
      configurable: true,
      writable: true,
      value: (to: string) => {
        assigned.push(String(to));
      },
    });

    try {
      const link = ufRoot()?.querySelector('a[href="/"]');
      if (link == null) {
        throw new Error(`no link home in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
      }
      await act(async () => {
        await userEvent.click(link);
      });
    } finally {
      Object.defineProperty(location, "assign", {
        configurable: true,
        writable: true,
        value: original,
      });
    }

    expect(assigned.length).toBe(1);
    expect(assigned[0].endsWith("/")).toBe(true);
    // And what was showing is still showing: a document navigation is the
    // browser's to perform, so nothing here unmounted anything.
    expect(ufRoot()?.textContent).toContain("counter");
  });
});

it("keeps HTTP handlers out of the browser route graph, including interactive routes", () => {
  const root = project({
    "app/$page.js": '"use client"; export component Page() { return <p>Home</p>; }',
    "app/auth/session/$route.js":
      'import {randomBytes} from "node:crypto"; export function POST() { return new Response(randomBytes(32)); }',
  });
  const plugin = uniflowed()[0];
  plugin.configResolved({ root, base: "/" });
  const client = plugin.load("\0virtual:uf/routes", { ssr: false });
  const server = plugin.load("\0virtual:uf/routes", { ssr: true });
  expect(client).not.toContain("$route.js");
  expect(client).toContain("$page.js");
  expect(server).toContain("/auth/session/$route.js");
});
