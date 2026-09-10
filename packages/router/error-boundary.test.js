// @flow
//
// `_uf.error.js`: what renders when a route does not.
//
// Before this there was no error boundary anywhere in uf. A component that
// threw took the whole response with it, `hydrateRoot` had nothing above it so
// a throw after hydration unmounted the document, and one page that threw
// during `uf build` failed the entire build with a message that named the
// exception and not the route. See ubugeeei-prod/uf#257.
//
// Three things have to be true and there is a section for each: the scanner
// finds the file, the resolver picks the nearest one and gives it the layouts
// above it, and a page that throws renders it — on the server, where React's
// class boundaries do not run, and in the browser, where they do and where
// `reset()` has to bring the page back.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { render, screen, userEvent } from "@uniflowed/react-testing";
import {
  RouteView,
  RouterProvider,
  forbidden,
  notFound,
  resolveMatch,
  routerView,
  unauthorized,
} from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterAll, describe, expect, it } from "@uniflowed/test";

import { reportRenderError } from "../../packages/vite/internal/events.js";
import { scanRoutes } from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root holding each named file. */
function appRoot(files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-error-boundary-"));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Boundary() {}\n");
  }
  return root;
}

/**
 * The scanned error boundaries, with paths cut back to the router root.
 *
 * A `module` of `null` is the record the scan synthesises for the router root
 * when a project declares none, so that the framework's error page renders
 * inside the site's own layouts rather than in place of them. See
 * ubugeeei-prod/uf#351, and `routing.test.js` for the not-found half.
 */
function boundaries(root: string) {
  return scanRoutes(root).errors.map((boundary) => ({
    path: boundary.path,
    module: boundary.module == null ? null : path.relative(root, boundary.module),
    layouts: boundary.layouts.map((layout) => path.relative(root, layout)),
  }));
}

describe("scanning for error boundaries", () => {
  it("finds one at every depth, with the layouts of its own directory", () => {
    const root = appRoot([
      "_uf.layout.js",
      "_uf.error.js",
      "guide/_uf.layout.js",
      "guide/_uf.error.js",
      "guide/deep/_uf.page.js",
    ]);

    expect(boundaries(root)).toEqual([
      { path: "/", module: "_uf.error.js", layouts: ["_uf.layout.js"] },
      {
        path: "/guide",
        module: path.join("guide", "_uf.error.js"),
        layouts: ["_uf.layout.js", path.join("guide", "_uf.layout.js")],
      },
    ]);
  });

  it("does not accept `.mdx`, which cannot take an error and a reset", () => {
    // A page may be content. A boundary is handed two arguments, which is a
    // component's contract and not a document's, so `.mdx` is not one of its
    // extensions and the file is simply not a boundary.
    const root = appRoot(["_uf.error.mdx"]);

    // The synthesised record, with no module: which is the scan saying the
    // project declared none, and is exactly the claim this test makes.
    expect(boundaries(root)).toEqual([{ path: "/", module: null, layouts: [] }]);
  });

  it("synthesises one at the router root for a project that declares none", () => {
    const root = appRoot(["_uf.layout.js", "_uf.page.js"]);

    expect(boundaries(root)).toEqual([{ path: "/", module: null, layouts: ["_uf.layout.js"] }]);
  });
});

// --- Resolution --------------------------------------------------------

const rootLayout = { metadata: { title: "site" } };
const guideLayout = { metadata: { title: "guide" } };
const loadRootLayout = () => Promise.resolve(rootLayout);
const loadGuideLayout = () => Promise.resolve(guideLayout);

component SiteError(error, reset) {
  return <p>site error</p>;
}

component GuideError(error, reset) {
  return <p>guide error</p>;
}

const rootBoundary = {
  path: "/",
  file: "app/_uf.error.js",
  module: () => Promise.resolve({ default: SiteError, metadata: { title: "site broke" } }),
  layouts: [loadRootLayout],
};

const guideBoundary = {
  path: "/guide",
  file: "app/guide/_uf.error.js",
  module: () => Promise.resolve({ default: GuideError, metadata: { title: "guide broke" } }),
  layouts: [loadRootLayout, loadGuideLayout],
};

/** A route under `/guide` whose loader calls `thrower`. */
const throwingRoute = (thrower: () => mixed) => ({
  path: "/guide/broken",
  params: [],
  mdx: false,
  file: "app/guide/broken/_uf.page.js",
  page: () => Promise.resolve({ loader: thrower }),
  layouts: [loadRootLayout, loadGuideLayout],
});

const tableWith = (route, errors) => ({ routes: [route], notFound: [], errors });

describe("a loader that throws", () => {
  it("resolves to the nearest boundary, with that boundary's layouts", async () => {
    const table = tableWith(
      throwingRoute(() => {
        throw new Error("the database is on fire");
      }),
      [rootBoundary, guideBoundary],
    );

    const resolved = await resolveMatch(table, "/guide/broken");

    expect(resolved.status).toBe(500);
    expect(resolved.error?.kind).toBe("thrown");
    expect(resolved.metadata.title).toBe("guide broke");
    expect(resolved.layouts).toEqual([rootLayout, guideLayout]);
  });

  it("falls back to the nearest above when a directory declares none", async () => {
    const table = tableWith(
      throwingRoute(() => {
        throw new Error("boom");
      }),
      [rootBoundary],
    );

    const resolved = await resolveMatch(table, "/guide/broken");

    expect(resolved.metadata.title).toBe("site broke");
    expect(resolved.layouts).toEqual([rootLayout]);
  });

  it("uses the framework's page when the project declares no boundary", async () => {
    const table = tableWith(
      throwingRoute(() => {
        throw new Error("boom");
      }),
      [],
    );

    const resolved = await resolveMatch(table, "/guide/broken");

    expect(resolved.status).toBe(500);
    expect(resolved.metadata.title).toBe("Something went wrong");
    expect(resolved.layouts).toEqual([]);
  });

  it("does not reject, which is what keeps a document on the screen", async () => {
    // `hydrate` awaits `resolveMatch` before `hydrateRoot`. A rejection there
    // is not an error page — it is no root at all, and the markup the server
    // sent stays on screen with nothing attached to it.
    const table = tableWith(
      throwingRoute(() => {
        throw new Error("boom");
      }),
      [],
    );

    await expect(resolveMatch(table, "/guide/broken")).resolves;
  });

  it("carries a redirect back out, because a redirect is not a page", async () => {
    const { redirect } = await import("@uniflowed/router");
    const table = tableWith(
      throwingRoute(() => redirect("/elsewhere")),
      [rootBoundary],
    );

    await expect(resolveMatch(table, "/guide/broken")).rejects.toThrow("redirect to /elsewhere");
  });

  it("still answers notFound() with the not-found page, not the error page", async () => {
    // The two boundaries are not the same thing: a 404 is an ordinary answer
    // and a throw is not, and `notFound()` must not become a 500 now that
    // there is somewhere for a 500 to go.
    const table = {
      routes: [throwingRoute(() => notFound())],
      notFound: [
        {
          path: "/",
          mdx: false,
          file: "app/_uf.not-found.js",
          page: () => Promise.resolve({ metadata: { title: "no such page" } }),
          layouts: [],
        },
      ],
      errors: [rootBoundary],
    };

    const resolved = await resolveMatch(table, "/guide/broken");

    expect(resolved.status).toBe(404);
    expect(resolved.metadata.title).toBe("no such page");
  });
});

describe("forbidden() and unauthorized()", () => {
  it("are the same boundary with a different status and no exception", async () => {
    // The reason there is one `_uf.error.js` and not three files: these differ
    // from a throw by a status and a sentence, which a union expresses and a
    // file convention repeats.
    const forbiddenRoute = await resolveMatch(
      tableWith(
        throwingRoute(() => forbidden()),
        [guideBoundary],
      ),
      "/guide/broken",
    );
    const unauthorizedRoute = await resolveMatch(
      tableWith(
        throwingRoute(() => unauthorized()),
        [guideBoundary],
      ),
      "/guide/broken",
    );

    expect(forbiddenRoute.status).toBe(403);
    expect(forbiddenRoute.error?.kind).toBe("forbidden");
    expect(unauthorizedRoute.status).toBe(401);
    expect(unauthorizedRoute.error?.kind).toBe("unauthorized");
    // Both reach the boundary the path is under, like any other failure.
    expect(forbiddenRoute.metadata.title).toBe("guide broke");
  });
});

describe("the boundary a route would be caught by", () => {
  it("counts the layouts above it, which are the ones that stay mounted", async () => {
    const working = {
      path: "/guide/ok",
      params: [],
      mdx: false,
      file: "app/guide/ok/_uf.page.js",
      page: () => Promise.resolve({ default: () => null }),
      layouts: [loadRootLayout, loadGuideLayout],
    };

    const resolved = await resolveMatch(
      { routes: [working], notFound: [], errors: [guideBoundary] },
      "/guide/ok",
    );

    // `app/guide/_uf.error.js` sits under both layouts, so both survive a
    // throw in the page and only the page is replaced.
    expect(resolved.errorBoundary.above).toBe(2);
    expect(resolved.errorBoundary.module).not.toBe(null);
  });

  it("is the root when the boundary is at the root", async () => {
    const working = {
      path: "/guide/ok",
      params: [],
      mdx: false,
      file: "app/guide/ok/_uf.page.js",
      page: () => Promise.resolve({ default: () => null }),
      layouts: [loadRootLayout, loadGuideLayout],
    };

    const resolved = await resolveMatch(
      { routes: [working], notFound: [], errors: [rootBoundary] },
      "/guide/ok",
    );

    // One shared layout: the site's. The guide's layout is below the boundary
    // and goes with the page.
    expect(resolved.errorBoundary.above).toBe(1);
  });

  it("never counts more layouts than the route has", async () => {
    // A `(group)` directory is not a URL segment, so a boundary can cover a
    // route with a shorter layout chain than its own. An `above` past the end
    // would have `RouteView` compose layouts out of nothing.
    const bare = {
      path: "/guide/ok",
      params: [],
      mdx: false,
      file: "app/guide/ok/_uf.page.js",
      page: () => Promise.resolve({ default: () => null }),
      layouts: [],
    };

    const resolved = await resolveMatch(
      { routes: [bare], notFound: [], errors: [guideBoundary] },
      "/guide/ok",
    );

    expect(resolved.errorBoundary.above).toBe(0);
  });
});

// --- Rendering ---------------------------------------------------------

component Boom() {
  throw new Error("the page threw");
}

component SiteLayout(children: React.Node) {
  return (
    <div>
      <nav>the navigation is still here</nav>
      {children}
    </div>
  );
}

const assets = { scripts: [], styles: [], preloads: [] };

describe("rendering on the server", () => {
  it("renders the boundary in place of the page and keeps the layouts", async () => {
    // React runs a class error boundary inside a `<Suspense>` and not outside
    // one, so a throw in the shell is `createRenderer`'s own catch. Without it
    // the whole response was the exception.
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [
        {
          path: "/broken",
          params: [],
          mdx: false,
          file: "app/broken/_uf.page.js",
          page: () => Promise.resolve({ default: Boom }),
          layouts: [() => Promise.resolve({ default: SiteLayout })],
        },
      ],
      notFound: [],
      errors: [
        {
          path: "/",
          file: "app/_uf.error.js",
          module: () => Promise.resolve({ default: SiteError }),
          layouts: [() => Promise.resolve({ default: SiteLayout })],
        },
      ],
    });

    const result = await prerender("/broken", assets);

    expect(result.status).toBe(500);
    expect(result.html).toContain("site error");
    expect(result.html).toContain("the navigation is still here");
    expect(result.html).not.toContain("the page threw");
  });

  it("reports the exception on the result, with nothing about it in the document", async () => {
    // How `uf build` learns the route failed. `uf dev` learns the same way for
    // a shell that threw, and through `render`'s `onError` for a boundary that
    // threw after the response had begun. The message is deliberately not in
    // the markup: it is written for whoever deployed the application, and the
    // markup goes to whoever asked for the page.
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [
        {
          path: "/broken",
          params: [],
          mdx: false,
          file: "app/broken/_uf.page.js",
          page: () => Promise.resolve({ default: Boom }),
          layouts: [],
        },
      ],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/broken", assets);

    expect(result.error instanceof Error && result.error.message).toBe("the page threw");
    expect(result.html).not.toContain("the page threw");
    expect(result.html).toContain("Something went wrong");
  });

  it("says nothing failed for a page that rendered", async () => {
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [
        {
          path: "/",
          params: [],
          mdx: false,
          file: "app/_uf.page.js",
          page: () => Promise.resolve({ default: SiteError }),
          layouts: [],
        },
      ],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/", assets);

    expect(result.status).toBe(200);
    expect(result.error).toBe(undefined);
  });

  it("does not report forbidden() as a failure, because it is an answer", async () => {
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [
        {
          path: "/secret",
          params: [],
          mdx: false,
          file: "app/secret/_uf.page.js",
          page: () => Promise.resolve({ loader: () => forbidden() }),
          layouts: [],
        },
      ],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/secret", assets);

    // A 403 in `dist/` is a page the application meant to write.
    expect(result.status).toBe(403);
    expect(result.error).toBe(undefined);
  });
});

describe("what `uf dev` says about it", () => {
  it("maps the stack onto the Flow source and names the URL in the terminal", () => {
    // The contained page is what a visitor sees; the exception is for whoever
    // is running the server, and uf owns the terminal. Both dev renderers call
    // this, which is why it is one function and not two messages.
    const fixed = [];
    const logged = [];
    const server = {
      ssrFixStacktrace: (error) => fixed.push(error),
      config: { logger: { error: (message) => logged.push(message) } },
    };
    const error = new Error("the page threw");

    reportRenderError(server, "/guide/broken", error);

    expect(fixed).toEqual([error]);
    expect(logged.length).toBe(1);
    expect(logged[0]).toContain("/guide/broken");
    expect(logged[0]).toContain("the page threw");
  });

  it("says something for a thrown value that is not an Error", () => {
    const logged = [];
    const server = {
      ssrFixStacktrace: () => {},
      config: { logger: { error: (message) => logged.push(message) } },
    };

    // `throw "nope"` is legal and a stack-trace mapper cannot be handed it.
    reportRenderError(server, "/odd", "nope");

    expect(logged[0]).toContain("nope");
  });
});

describe("rendering in the browser", () => {
  it("catches a throw and leaves the layouts above the boundary interactive", async () => {
    const resolved = await resolveMatch(
      {
        routes: [
          {
            path: "/broken",
            params: [],
            mdx: false,
            file: "app/broken/_uf.page.js",
            page: () => Promise.resolve({ default: Boom }),
            layouts: [() => Promise.resolve({ default: SiteLayout })],
          },
        ],
        notFound: [],
        errors: [
          {
            path: "/",
            file: "app/_uf.error.js",
            module: () => Promise.resolve({ default: SiteError }),
            layouts: [() => Promise.resolve({ default: SiteLayout })],
          },
        ],
      },
      "/broken",
    );

    render(
      <RouterProvider url="/broken" initial={resolved}>
        <RouteView />
      </RouterProvider>,
    );

    expect(screen.getByText("site error")).not.toBe(null);
    // The document is not blank, which is what a throw used to leave.
    expect(screen.getByText("the navigation is still here")).not.toBe(null);
  });

  it("brings the page back when reset() is called", async () => {
    let broken = true;
    component Flaky() {
      if (broken) {
        throw new Error("not yet");
      }
      return <p>the page recovered</p>;
    }
    component Retry(error, reset) {
      return (
        <button type="button" onClick={reset}>
          try again
        </button>
      );
    }

    const resolved = await resolveMatch(
      {
        routes: [
          {
            path: "/flaky",
            params: [],
            mdx: false,
            file: "app/flaky/_uf.page.js",
            page: () => Promise.resolve({ default: Flaky }),
            layouts: [],
          },
        ],
        notFound: [],
        errors: [
          {
            path: "/",
            file: "app/_uf.error.js",
            module: () => Promise.resolve({ default: Retry }),
            layouts: [],
          },
        ],
      },
      "/flaky",
    );

    render(
      <RouterProvider url="/flaky" initial={resolved}>
        <RouteView />
      </RouterProvider>,
    );
    broken = false;
    await userEvent.click(screen.getByRole("button", { name: "try again" }));

    // `reset()` re-renders the subtree the boundary replaced — the page is
    // mounted again, not merely un-hidden.
    expect(screen.getByText("the page recovered")).not.toBe(null);
  });
});
