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

import { notFound, resolveMatch } from "@uniflowed/router";
import { afterAll, describe, expect, it } from "@uniflowed/test";

// Reached by path rather than by package name: `internal/` is not in
// `@uniflowed/vite`'s exports, and it should not be — this is the build's own
// router, not something a project imports. `highlight.test.js` reaches into
// the same package the same way.
import { scanRoutes } from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

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

/** A scanned boundary, with the absolute paths cut back to the router root. */
function boundaries(root: string) {
  return scanRoutes(root).notFound.map((boundary) => ({
    path: boundary.path,
    page: path.relative(root, boundary.page),
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

    expect(boundaries(root).map((boundary) => boundary.path)).toEqual(["/posts/:slug"]);
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

  it("reports none for a project that declares none", () => {
    expect(boundaries(appRoot(["_uf.page.js"]))).toEqual([]);
  });
});

// --- The resolver ------------------------------------------------------
//
// The modules below carry only what the resolver reads. A `metadata.title` is
// how a test says which file was picked: it survives into the resolved route,
// and a page's own metadata wins over its layouts', so the title is the
// boundary's own.

const rootLayout = { metadata: { title: "site" } };
const guideLayout = { metadata: { title: "guide section" } };

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
