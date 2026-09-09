// @flow
//
// Which file the router picks when nothing matched.
//
// `_uf.not-found.js` is a segment file — every directory may declare one and a
// path gets the nearest one above it — and it was read at the router root
// only, so a nested one was never in the table and a reader who followed a
// stale link into the manual was answered outside it. The bug had two halves
// and so does this file: the scanner did not find the file, and the resolver
// had one nullable record and so nothing to choose between. See
// ubugeeei-prod/uf#263.
//
// The scanning half writes real files into a temporary directory rather than
// handing the walk a fake tree. `scanRoutes` is `readdirSync` and `statSync`;
// a fixture that replaced them would prove the sort order and nothing about
// which file is found.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { notFound, resolveMatch, routerView } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterAll, describe, expect, it } from "@uniflowed/test";

// Reached by path rather than by package name: `internal/` is not in
// `@uniflowed/vite`'s exports, and it should not be — this is the build's own
// router, not something a project imports. `highlight.test.js` reaches into
// the same package the same way.
import { scanRoutes } from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

const assets = { scripts: [], styles: [], preloads: [] };

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root holding each named file, empty unless contents are given. */
function appRoot(files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-routing-"));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Page() {}\n");
  }
  return root;
}

/**
 * A scanned boundary, with the absolute paths cut back to the router root.
 *
 * A `page` of `null` is the record the scan synthesises for the router root
 * when a project declares none, and it stays `null` here: it names no file, and
 * a placeholder path would read as a file somebody could go and open.
 */
function boundaries(root: string) {
  return scanRoutes(root).notFound.map((boundary) => ({
    path: boundary.path,
    page: boundary.page == null ? null : path.relative(root, boundary.page),
    layouts: boundary.layouts.map((layout) => path.relative(root, layout)),
  }));
}

/** The same, for the error boundaries. */
function errorBoundaries(root: string) {
  return scanRoutes(root).errors.map((boundary) => ({
    path: boundary.path,
    module: boundary.module == null ? null : path.relative(root, boundary.module),
    layouts: boundary.layouts.map((layout) => path.relative(root, layout)),
  }));
}

describe("scanning for not-found boundaries", () => {
  it("finds the one at the router root", () => {
    const root = appRoot(["_uf.page.js", "_uf.not-found.js"]);

    expect(boundaries(root)).toEqual([{ path: "/", page: "_uf.not-found.js", layouts: [] }]);
  });

  it("finds a nested one, which is the whole bug", () => {
    // `app/guide/_uf.not-found.js` was never looked for: the scan asked for it
    // at `depth === 0` and nowhere else, so this list held one entry.
    const root = appRoot([
      "_uf.page.js",
      "_uf.not-found.js",
      "guide/_uf.page.js",
      "guide/_uf.not-found.js",
    ]);

    expect(boundaries(root).map((boundary) => boundary.path)).toEqual(["/", "/guide"]);
  });

  it("gives a boundary the layouts of the directory that declares it", () => {
    // Not the layouts of any route: this is what wraps the boundary when it
    // renders, and it is why a nested 404 keeps the manual's sidebar.
    const root = appRoot([
      "_uf.layout.js",
      "guide/_uf.layout.js",
      "guide/_uf.not-found.js",
      "guide/deep/_uf.page.js",
    ]);

    expect(boundaries(root)).toEqual([
      // The synthesised record: nothing is declared at `/`, so `/nope` has the
      // root's layout to render the framework's page inside. See #351.
      { path: "/", page: null, layouts: ["_uf.layout.js"] },
      {
        path: "/guide",
        page: "guide/_uf.not-found.js",
        layouts: ["_uf.layout.js", "guide/_uf.layout.js"],
      },
    ]);
  });

  it("names a boundary by its route path, so a group does not appear in it", () => {
    // `(marketing)` organises files without being a URL segment, exactly as it
    // does for a page — so this boundary answers `/nope`, not `/(marketing)/nope`.
    const root = appRoot(["(marketing)/_uf.not-found.js", "(marketing)/about/_uf.page.js"]);

    expect(boundaries(root).map((boundary) => boundary.path)).toEqual(["/"]);
  });

  it("finds one under a parameter segment", () => {
    const root = appRoot(["posts/[slug]/_uf.page.js", "posts/[slug]/_uf.not-found.js"]);

    expect(boundaries(root).map((boundary) => boundary.path)).toEqual(["/", "/posts/:slug"]);
  });

  it("puts the shallower of two boundaries at one path first", () => {
    // A `(group)` is not a URL segment, so both of these are at `/` and the
    // URL cannot say which tree it meant. The resolver takes the first, and
    // the sort is stable over a walk that records a directory before
    // descending — so it is the site's own 404 rather than one section's idea
    // of it. Each group owning one needs parallel-route trees (#267).
    const root = appRoot(["_uf.not-found.js", "(marketing)/_uf.not-found.js"]);

    expect(boundaries(root).map((boundary) => boundary.page)).toEqual([
      "_uf.not-found.js",
      path.join("(marketing)", "_uf.not-found.js"),
    ]);
  });

  it("synthesises one at the router root for a project that declares none", () => {
    // This read `toEqual([])`, and an empty list is what left the resolver with
    // no layouts to render the framework's 404 inside: a site whose root layout
    // owns the masthead answered a stale link with a white page saying 404. The
    // record has no page — the framework's component renders — and the root's
    // layouts, which is the whole of the fix. See ubugeeei-prod/uf#351.
    const root = appRoot(["_uf.layout.js", "_uf.page.js"]);

    expect(boundaries(root)).toEqual([{ path: "/", page: null, layouts: ["_uf.layout.js"] }]);
  });

  it("synthesises nothing when the project declares its own at the root", () => {
    // One answer per path. A second record at `/` would be a boundary the URL
    // cannot choose between, which is the problem `(group)` boundaries already
    // have and #267 is about.
    const root = appRoot(["_uf.not-found.js", "_uf.page.js"]);

    expect(boundaries(root).map((boundary) => boundary.path)).toEqual(["/"]);
  });

  it("counts a boundary a route group declares as the one at the root", () => {
    // `app/(marketing)/_uf.not-found.js` is at `/` — a group is not a URL
    // segment — so the project has declared the root's 404 and nothing is
    // synthesised beside it.
    const root = appRoot(["(marketing)/_uf.not-found.js", "(marketing)/about/_uf.page.js"]);

    expect(boundaries(root).map((boundary) => boundary.page)).toEqual([
      path.join("(marketing)", "_uf.not-found.js"),
    ]);
  });
});

describe("scanning for error boundaries", () => {
  it("synthesises one at the router root, with the same shape and for the same reason", () => {
    // `_uf.error.js` had the second half of the same bug: a project that
    // declared none got the framework's error page with no layouts around it,
    // so a 500 lost the site as completely as a 404 did.
    const root = appRoot(["_uf.layout.js", "_uf.page.js"]);

    expect(errorBoundaries(root)).toEqual([
      { path: "/", module: null, layouts: ["_uf.layout.js"] },
    ]);
  });

  it("leaves a declared root boundary alone", () => {
    const root = appRoot(["_uf.layout.js", "_uf.error.js", "_uf.page.js"]);

    expect(errorBoundaries(root)).toEqual([
      { path: "/", module: "_uf.error.js", layouts: ["_uf.layout.js"] },
    ]);
  });
});

// --- The resolver ------------------------------------------------------
//
// The modules below carry only what the resolver reads. A `metadata.title` is
// how a test says which file was picked: it survives into the resolved route,
// and a page's own metadata wins over its layouts', so the title is the
// boundary's own.

const rootLayout = { metadata: { title: "site", description: "the site" } };
const guideLayout = { metadata: { title: "guide section" } };

/**
 * The record the build synthesises at the router root for a project that
 * declares no `_uf.not-found.js`: the root's layouts, and no page to import.
 */
const synthesisedRoot = {
  path: "/",
  mdx: false,
  file: "@uniflowed/router",
  page: null,
  layouts: [() => Promise.resolve(rootLayout)],
};

const rootNotFound = {
  path: "/",
  mdx: false,
  file: "app/_uf.not-found.js",
  page: () => Promise.resolve({ metadata: { title: "site 404" } }),
  layouts: [() => Promise.resolve(rootLayout)],
};

const guideNotFound = {
  path: "/guide",
  mdx: false,
  file: "app/guide/_uf.not-found.js",
  page: () => Promise.resolve({ metadata: { title: "guide 404" } }),
  layouts: [() => Promise.resolve(rootLayout), () => Promise.resolve(guideLayout)],
};

describe("resolving an unmatched path", () => {
  it("takes the nearest boundary above it", async () => {
    const resolved = await resolveMatch(
      { routes: [], notFound: [rootNotFound, guideNotFound], errors: [] },
      "/guide/nope",
    );

    expect(resolved.status).toBe(404);
    expect(resolved.metadata.title).toBe("guide 404");
  });

  it("wraps it in that boundary's layouts", async () => {
    const resolved = await resolveMatch(
      { routes: [], notFound: [rootNotFound, guideNotFound], errors: [] },
      "/guide/nope",
    );

    // The layouts of the boundary, root first — so the manual's sidebar is
    // still around the 404 that says the page is not in the manual.
    expect(resolved.layouts).toEqual([rootLayout, guideLayout]);
  });

  it("falls back to the nearest above when a directory declares none", async () => {
    // This is what worked before the change and has to keep working:
    // `/reference` has no boundary of its own on uf's own site.
    const resolved = await resolveMatch(
      { routes: [], notFound: [rootNotFound, guideNotFound], errors: [] },
      "/reference/nope",
    );

    expect(resolved.metadata.title).toBe("site 404");
    expect(resolved.layouts).toEqual([rootLayout]);
  });

  it("answers the boundary's own path with it", async () => {
    // `/guide` covers `/guide`, not only what is under it — a directory with a
    // boundary and no page is a 404 the boundary answers.
    const resolved = await resolveMatch(
      { routes: [], notFound: [guideNotFound], errors: [] },
      "/guide",
    );

    expect(resolved.metadata.title).toBe("guide 404");
  });

  it("does not use a boundary the path is not under", async () => {
    const resolved = await resolveMatch(
      { routes: [], notFound: [guideNotFound], errors: [] },
      "/nope",
    );

    // Nothing covers `/nope`, so the framework's default answers rather than
    // the manual's 404 telling a visitor to the home page to read the manual.
    expect(resolved.status).toBe(404);
    expect(resolved.metadata.title).toBe("Not found");
    expect(resolved.layouts).toEqual([]);
  });

  it("renders the framework's page inside the root's layouts when nothing is declared", async () => {
    // The record the build synthesises, and what it buys: the framework's 404
    // arrives inside the site's own masthead, so the reader can leave. Before
    // it, `resolveNotFound` had no record at all and answered `layouts: []` —
    // a white page with `404` on it and no navigation, which is what every
    // project got until it wrote `app/_uf.not-found.js`, because `uf create`
    // scaffolds neither boundary. See ubugeeei-prod/uf#351.
    const resolved = await resolveMatch(
      { routes: [], notFound: [synthesisedRoot], errors: [] },
      "/nope",
    );

    expect(resolved.status).toBe(404);
    expect(resolved.metadata.title).toBe("Not found");
    expect(resolved.layouts).toEqual([rootLayout]);
  });

  it("keeps the root layout's own metadata under the framework's title", async () => {
    // The framework's page merges like any page: a `metadataBase` or an
    // `og:site_name` on the root layout still applies to the 404 that renders
    // inside it.
    const resolved = await resolveMatch(
      { routes: [], notFound: [synthesisedRoot], errors: [] },
      "/nope",
    );

    expect(resolved.metadata.title).toBe("Not found");
    expect(resolved.metadata.description).toBe("the site");
  });

  it("matches a boundary under a parameter segment", async () => {
    const perPost = {
      path: "/posts/:slug",
      mdx: false,
      file: "app/posts/[slug]/_uf.not-found.js",
      page: () => Promise.resolve({ metadata: { title: "no such section" } }),
      layouts: [],
    };
    const table = { routes: [], notFound: [rootNotFound, perPost], errors: [] };

    expect((await resolveMatch(table, "/posts/hello/nope")).metadata.title).toBe("no such section");
    expect((await resolveMatch(table, "/posts")).metadata.title).toBe("site 404");
  });
});

// --- The framework's own pages, in the site they belong to -----------------

describe("the framework's pages when a project declares no boundary", () => {
  /** A root layout with something in it a reader would notice losing. */
  component Masthead(children: React.Node) {
    return (
      <div>
        <nav>the masthead is here</nav>
        {children}
      </div>
    );
  }

  /** A one-route app whose only layout is the masthead, with both root records. */
  function site(page: () => Promise<mixed>) {
    return {
      App: routerView("./app"),
      routes: [
        {
          path: "/",
          params: [],
          mdx: false,
          file: "app/_uf.page.js",
          page,
          layouts: [() => Promise.resolve({ default: Masthead })],
          loading: [],
        },
      ],
      // What `scanRoutes` synthesises for a project that declares neither file.
      notFound: [
        {
          path: "/",
          mdx: false,
          file: "@uniflowed/router",
          page: null,
          layouts: [() => Promise.resolve({ default: Masthead })],
        },
      ],
      errors: [
        {
          path: "/",
          file: "@uniflowed/router",
          module: null,
          layouts: [() => Promise.resolve({ default: Masthead })],
        },
      ],
    };
  }

  it("answers an unmatched URL with a 404 the reader can leave", async () => {
    component Page() {
      return <p>the home page</p>;
    }
    const { prerender } = createRenderer(site(() => Promise.resolve({ default: Page })));

    const result = await prerender("/nope", assets);

    expect(result.status).toBe(404);
    expect(result.html).toContain("the masthead is here");
    expect(result.html).toContain("404");
  });

  it("answers a loader that threw with an error page inside the same layouts", async () => {
    // The other half of ubugeeei-prod/uf#351: `resolveError` had the same empty
    // layout list, so a 500 lost the site as completely as a 404 did.
    const { prerender } = createRenderer(
      site(() =>
        Promise.resolve({
          default: () => null,
          loader: () => {
            throw new Error("the loader threw");
          },
        }),
      ),
    );

    const result = await prerender("/", assets);

    expect(result.status).toBe(500);
    expect(result.html).toContain("the masthead is here");
    expect(result.html).toContain("Something went wrong");
    // Not the thrown message: that is written for whoever deployed the site.
    expect(result.html).not.toContain("the loader threw");
  });
});

describe("notFound() thrown from a page", () => {
  it("lands on the nearest boundary above the page, not the root one", async () => {
    // The second way into a 404, and it went to the same wrong place. Here the
    // URL *did* match a route — `/guide/:slug` — and the loader said the slug
    // names nothing; the root 404 then answered it outside the manual.
    const slugPage = {
      path: "/guide/:slug",
      params: [{ name: "slug", catchAll: false }],
      mdx: false,
      file: "app/guide/[slug]/_uf.page.js",
      page: () => Promise.resolve({ loader: () => notFound() }),
      layouts: [() => Promise.resolve(rootLayout), () => Promise.resolve(guideLayout)],
    };

    const resolved = await resolveMatch(
      { routes: [slugPage], notFound: [rootNotFound, guideNotFound], errors: [] },
      "/guide/missing",
    );

    expect(resolved.status).toBe(404);
    expect(resolved.metadata.title).toBe("guide 404");
    expect(resolved.layouts).toEqual([rootLayout, guideLayout]);
  });
});
