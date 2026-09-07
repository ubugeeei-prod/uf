// @flow
//
// `_uf.template.js`: a layout that remounts.
//
// A layout persists across navigation — that is the point of one, and it is
// why a sidebar keeps its scroll position when the page under it changes.
// Sometimes persistence is the wrong default: an enter animation should play
// again, an effect should run again, a form should start empty. Next.js calls
// that file `template.js` and uf had no equivalent, which is the third of the
// three features ubugeeei-prod/uf#267 asked for and the cheapest of them: the
// table already nests layouts, so a template is that shape plus a `key`.
//
// Three things have to be true and there is a section for each: the scanner
// finds the file and places it, the composed tree puts it inside its own
// segment's layout, and a navigation rebuilds it while the layout above it
// stays.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { render, screen, userEvent } from "@uniflowed/react-testing";
import { RouteView, RouterProvider, resolveMatch, routerView, useRouter } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterAll, beforeEach, describe, expect, it } from "@uniflowed/test";

import { routesModuleSource, scanRoutes } from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root holding each named file. */
function appRoot(files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-template-"));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Wrapper() {}\n");
  }
  return root;
}

/** The scanned templates of the one route, cut back to the router root. */
function templatesOf(root: string) {
  return scanRoutes(root).routes[0].templates.map((entry) => ({
    above: entry.above,
    module: path.relative(root, entry.module),
  }));
}

describe("scanning for templates", () => {
  it("puts one inside its own segment's layout", () => {
    // `above` counts the layouts outside the wrapper, taken *after* the
    // segment's own layout is added — the same number, spelled the same way,
    // as a loading boundary's. A template that sat outside its own layout
    // would remount the frame it is supposed to be inside.
    const root = appRoot([
      "_uf.layout.js",
      "guide/_uf.layout.js",
      "guide/_uf.template.js",
      "guide/_uf.page.js",
    ]);

    expect(templatesOf(root)).toEqual([
      { above: 2, module: path.join("guide", "_uf.template.js") },
    ]);
  });

  it("nests them the way layouts nest, root first", () => {
    const root = appRoot([
      "_uf.template.js",
      "guide/_uf.layout.js",
      "guide/_uf.template.js",
      "guide/_uf.page.js",
    ]);

    expect(templatesOf(root)).toEqual([
      { above: 0, module: "_uf.template.js" },
      { above: 1, module: path.join("guide", "_uf.template.js") },
    ]);
  });

  it("gives a route with none an empty list, not a wrapper", () => {
    // A segment with no template contributes nothing at all: a project that
    // declares none renders exactly the tree it rendered before the file
    // existed.
    expect(templatesOf(appRoot(["_uf.layout.js", "_uf.page.js"]))).toEqual([]);
  });

  it("carries them into the generated module as lazy imports", () => {
    const root = appRoot(["_uf.template.js", "_uf.page.js"]);

    const source = routesModuleSource(scanRoutes(root));

    expect(source).toContain("_uf.template.js");
    expect(source).toContain("templates: [{ above: 0, module: template0 }]");
  });
});

// --- Composition -------------------------------------------------------

const mounted: Array<string> = [];

beforeEach(() => {
  mounted.length = 0;
});

component Frame(children: React.Node) {
  React.useEffect(() => {
    mounted.push("layout");
  }, []);
  return (
    <div>
      <nav>the sidebar</nav>
      {children}
    </div>
  );
}

component Enter(children: React.Node) {
  React.useEffect(() => {
    mounted.push("template");
  }, []);
  return <section data-testid="template">{children}</section>;
}

const loadFrame = () => Promise.resolve({ default: Frame });
const loadEnter = () => Promise.resolve({ default: Enter });

/** Two routes under one layout and one template, which is the whole point. */
const page = (routePath: string, text: string) => ({
  path: routePath,
  params: [],
  mdx: false,
  file: `app${routePath}/_uf.page.js`,
  page: () => Promise.resolve({ default: () => <p>{text}</p> }),
  layouts: [loadFrame],
  loading: [],
  templates: [{ above: 1, module: loadEnter }],
});

const table = {
  routes: [page("/first", "the first page"), page("/second", "the second page")],
  notFound: [],
  errors: [],
};

const assets = { scripts: [], styles: [], preloads: [] };

describe("rendering a route that has one", () => {
  it("puts the template inside the layout and outside the page", async () => {
    const { prerender } = createRenderer({ App: routerView("./app"), ...table });

    const result = await prerender("/first", assets);

    // The sidebar is above the template, and the page is inside it: a
    // template is a layout in every way but how long it lives.
    const layoutAt = result.html.indexOf("the sidebar");
    const templateAt = result.html.indexOf("<section");
    const pageAt = result.html.indexOf("the first page");
    expect(layoutAt).toBeGreaterThan(-1);
    expect(templateAt).toBeGreaterThan(layoutAt);
    expect(pageAt).toBeGreaterThan(templateAt);
  });

  it("renders the same tree as before for a route with no template", async () => {
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [{ ...page("/first", "the first page"), templates: [] }],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/first", assets);

    expect(result.html).toContain("the first page");
    expect(result.html).not.toContain("<section");
  });
});

component Go() {
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() => {
        // `scroll: false` because jsdom has no scrolling and this test is
        // about what is mounted, not about where the window is.
        router.push("/second", { scroll: false }).catch(() => {});
      }}
    >
      go
    </button>
  );
}

describe("navigating between two routes that share both", () => {
  it("rebuilds the template and leaves the layout alone", async () => {
    // The whole of what a template is. `RouteView` keys the element on the
    // pathname, so React throws the subtree away and builds it again; the
    // layout above has no key and is the same element in the same place, so it
    // is never unmounted.
    createRenderer({ App: routerView("./app"), ...table });
    const resolved = await resolveMatch(table, "/first");

    render(
      <RouterProvider url="/first" initial={resolved}>
        <RouteView />
        <Go />
      </RouterProvider>,
    );
    expect(mounted.filter((what) => what === "layout").length).toBe(1);
    expect(mounted.filter((what) => what === "template").length).toBe(1);

    await userEvent.click(screen.getByRole("button", { name: "go" }));

    expect(screen.getByText("the second page")).not.toBe(null);
    expect(mounted.filter((what) => what === "layout").length).toBe(1);
    expect(mounted.filter((what) => what === "template").length).toBe(2);
  });
});
