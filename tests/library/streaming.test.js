// @flow
//
// `_uf.loading.js` and a renderer that streams.
//
// Before this the renderer was `renderToString`: the whole tree had to resolve
// before a byte left, there was no boundary to fall back to, and the route
// grammar had no name for one. `suspense: true` was in the config and nothing
// read it. See ubugeeei-prod/uf#254.
//
// Two halves here: the scanner finds the file and the route table carries it,
// and `RouteView` puts a `<Suspense>` where the file said. The renderer is still
// `renderToString`, so a boundary on the server shows its fallback and never
// replaces it — that is the next commit, and the tests for it arrive with it.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";
import { act, render, screen } from "@uniflowed/react-testing";
import { RouteView, RouterProvider, resolveMatch } from "@uniflowed/router";
import { afterAll, describe, expect, it } from "@uniflowed/test";

import { RESERVED, routesModuleSource, scanRoutes } from "../../packages/vite/internal/routes.js";

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
});
