// @flow
//
// Which DOM subtree each boundary owns.
//
// An application has three boundaries a reader cannot see, and
// ubugeeei-prod/uf#636 answered one of them: `uf dev` says why a module is in
// the client bundle. The other two are Suspense and error, and the thing that
// was missing about them is not the *data* — the route table nests
// `$loading.js` and binds `$error.js` to the nearest ancestor, and
// `error-boundary.test.js` and `streaming.test.js` already hold both — but the
// *page*. A `<Suspense>` renders no element and neither does a class boundary,
// so what a boundary owns is a run of nodes in a parent that also holds the
// layout's own, and nothing on the page tells the two apart.
//
// So the render says so, and there is a section for each half of that. The
// marks delimit the run, which is the only claim worth testing hard: the test
// that matters is a boundary between a `<nav>` and a `<footer>` — a single
// opening mark would hand it the footer. The report is what a reader is told,
// and it is quiet until the answer changes, which is the other half.
//
// The module is internal to `@uniflowed/router` and is reached by path, the way
// `hydration.test.js` reaches `internal/hydration.js`: `internal/` is the half
// of the router that only exists under `uf dev`, and nothing outside the
// package has business rendering a mark.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { render } from "@uniflowed/react-testing";
import { RouteView, RouterProvider, resolveMatch, routerView } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
// The renderer with no document behind it, which is the point of the one test
// that uses it. `react-dom/server` reads nothing while it is imported, unlike
// `react-dom/client` — see `hydration.test.js`, which has to require that one.
import { renderToStaticMarkup } from "react-dom/server";
import { afterAll, beforeEach, describe, expect, it } from "@uniflowed/test";

import { elementIn, elementsIn } from "../../tests/library/dom.js";
import { routesModuleSource, scanRoutes } from "../../packages/vite/internal/routes.js";
import { DIAGNOSTIC_ENDPOINT } from "./internal/diagnostics.js";
import {
  type BoundaryFinding,
  type RouteBoundary,
  BOUNDARY_ATTRIBUTE,
  BOUNDARY_GLOBAL,
  BoundaryReporter,
  EDGE_ATTRIBUTE,
  ROOT_ERROR_ID,
  ROUTE_ERROR_ID,
  SOURCE_ATTRIBUTE,
  SYNTHESISED_SOURCE,
  boundaryFindings,
  describeElement,
  forgetBoundaries,
  formatBoundaries,
  insideBoundary,
  routeBoundaries,
  suspenseId,
} from "./internal/boundaries.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/**
 * Everything this file has put in the document, so it can take it out again.
 *
 * The document belongs to the process rather than to the file — `uf test` runs
 * several files in one worker — and a `[data-uf-boundary]` mark left behind
 * is a mark the next test in this file would find by `querySelector` and
 * attribute to its own render. See the same paragraph in `hydration.test.js`.
 */
const mountedContainers: Array<Element> = [];

beforeEach(() => {
  while (mountedContainers.length > 0) {
    mountedContainers.pop()?.remove();
  }
  // The module's own memory: which routes it has seen, and whether a mark that
  // mounts now should be in the DOM immediately. Both are latched per process,
  // and a test that inherited either from the test above it would be asserting
  // the order the file happens to be written in.
  forgetBoundaries();
});

/** Render `tree`, remembering the container so the next test starts clean. */
function mount(tree: React.Node): Element {
  const { container } = render(tree);
  mountedContainers.push(container);
  return container;
}

// --- The boundaries a route renders ----------------------------------------

/** A router root holding each named file. */
function appRoot(prefix: string, files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Part() {}\n");
  }
  return root;
}

/** A page module that renders one paragraph. */
const pageOf = (text: string) => () => Promise.resolve({ default: () => <p>{text}</p> });

/** A layout with something of its own on either side of the route. */
component Frame(children: React.Node) {
  return (
    <div id="shell">
      <nav className="masthead">the masthead</nav>
      {children}
      <footer className="colophon">the colophon</footer>
    </div>
  );
}

const loadFrame = () => Promise.resolve({ default: Frame });
const loadFallback = () => Promise.resolve({ default: () => <p>loading</p> });

describe("the boundaries a route renders", () => {
  it("finds the two error boundaries a route with no fallback has", async () => {
    // Two, and they are not the same thing twice: the outer one has no module
    // and renders the framework's page, and the route's own is placed at the
    // depth its file sits at. `RouteView`'s own comment makes the argument;
    // this is the map agreeing with it.
    const table = {
      routes: [
        {
          path: "/first",
          params: [],
          mdx: false,
          file: "app/first/$page.js",
          page: pageOf("the first page"),
          layouts: [loadFrame],
          loading: [],
          templates: [],
        },
      ],
      notFound: [],
      errors: [],
    };
    const resolved = await resolveMatch(table, "/first");

    const found = routeBoundaries(resolved, "app/$error.js");

    expect([...found.keys()]).toEqual([ROOT_ERROR_ID, ROUTE_ERROR_ID]);
    expect(found.get(ROOT_ERROR_ID)).toEqual({
      id: ROOT_ERROR_ID,
      kind: "error",
      above: 0,
      source: SYNTHESISED_SOURCE,
    });
    expect(found.get(ROUTE_ERROR_ID)?.source).toBe("app/$error.js");
  });

  it("gives every `$loading.js` its own boundary at the depth it sits at", async () => {
    // `above` is how many of the route's layouts are outside the fallback, and
    // it is the table's number rather than a second one computed here: one
    // vocabulary for where a thing sits in the stack, which is the rule
    // `LoadingRecord` and `errorBoundary.above` already follow.
    const table = {
      routes: [
        {
          path: "/deep",
          params: [],
          mdx: false,
          file: "app/deep/$page.js",
          page: pageOf("the deep page"),
          layouts: [loadFrame, loadFrame],
          loading: [
            { above: 0, module: loadFallback },
            { above: 2, module: loadFallback },
          ],
          templates: [],
        },
      ],
      notFound: [],
      errors: [],
    };
    const resolved = await resolveMatch(table, "/deep");

    const found = routeBoundaries(resolved, null);

    expect([...found.values()].map((it) => [it.id, it.kind, it.above])).toEqual([
      [ROOT_ERROR_ID, "error", 0],
      [ROUTE_ERROR_ID, "error", 0],
      [suspenseId(0), "suspense", 0],
      [suspenseId(1), "suspense", 2],
    ]);
    // No file, and that is the route table's asymmetry rather than this
    // module's: an error boundary is matched by path so the table carries one,
    // and putting a path on every loading record would ship it to every
    // visitor for a report only `uf dev` reads.
    expect(found.get(suspenseId(0))?.source).toBe(null);
  });

  it("leaves out the route's own boundary when the route is its error page", () => {
    // `RouteView` does not render one there — the page *is* the boundary's
    // component, and wrapping it in the same boundary would answer a throw
    // inside it with itself — so a map that named one would be pointing at a
    // subtree nothing owns.
    const resolved = {
      errorBoundary: { above: 1 },
      loading: [],
      error: { kind: "thrown", error: new Error("boom") },
    };

    const found = routeBoundaries(resolved, "app/$error.js");

    expect([...found.keys()]).toEqual([ROOT_ERROR_ID]);
  });

  it("spells the synthesised boundary's file the way the generated table does", () => {
    // Two spellings of one string, in a package that cannot import the other:
    // `@uniflowed/vite` is plain JavaScript loaded before any Flow transform
    // exists. A duplicated constant with a test on it is honest; one without is
    // how a report ends up naming a file nothing wrote. Same argument as
    // `devtools.test.js` makes about `__REACT_DEVTOOLS_GLOBAL_HOOK__`.
    const root = appRoot("uf-boundaries-source-", ["$layout.js", "$page.js"]);

    const source = routesModuleSource(scanRoutes(root));

    expect(source).toContain(`file: ${JSON.stringify(SYNTHESISED_SOURCE)}`);
  });
});

// --- The marks, and what they delimit --------------------------------------

const suspense = (id: string, above: number): RouteBoundary => ({
  id,
  kind: "suspense",
  above,
  source: null,
});

describe("the marks a boundary renders", () => {
  it("owns the run between them and not the layout's own nodes beside it", () => {
    // The test the pair exists for. A single opening mark cannot say where the
    // run ends, so the walk from it would reach the colophon — a boundary
    // credited with a subtree that stays on the page when it fails.
    const boundary = suspense(suspenseId(0), 1);

    const container = mount(
      <Frame>{insideBoundary(boundary, <article className="post">the post</article>)}</Frame>,
    );

    const [finding] = boundaryFindings(new Map([[boundary.id, boundary]]), globalThis.document);
    expect(finding.rendered).toBe(true);
    expect(finding.owns).toEqual(["article.post"]);
    // And the marks really are siblings of what they delimit, which is what
    // makes the walk `nextElementSibling` rather than a tree search.
    expect(elementsIn(container, "#shell > *").map((it) => it.tagName.toLowerCase())).toEqual([
      "nav",
      "span",
      "article",
      "span",
      "footer",
    ]);
  });

  it("nests, and neither boundary counts the other's marks as its own", () => {
    // Two boundaries in one parent, which is what a `$loading.js` inside
    // another one produces. The outer owns all three paragraphs and none of the
    // four marker elements; the inner owns exactly its own.
    const outer = suspense(suspenseId(0), 0);
    const inner = suspense(suspenseId(1), 0);

    mount(
      <Frame>
        {insideBoundary(
          outer,
          <>
            <p className="before">before</p>
            {insideBoundary(inner, <p className="middle">middle</p>)}
            <p className="after">after</p>
          </>,
        )}
      </Frame>,
    );

    const findings = boundaryFindings(
      new Map([
        [outer.id, outer],
        [inner.id, inner],
      ]),
      globalThis.document,
    );
    expect(findings[0].owns).toEqual(["p.before", "p.middle", "p.after"]);
    expect(findings[1].owns).toEqual(["p.middle"]);
  });

  it("counts what it does not name", () => {
    // A report with no ceiling on it is a report a page decides the size of,
    // which `docs/security.md` asks that nothing here be.
    const boundary = suspense(suspenseId(0), 0);

    mount(
      <Frame>
        {insideBoundary(
          boundary,
          <>
            <p id="one" />
            <p id="two" />
            <p id="three" />
            <p id="four" />
            <p id="five" />
          </>,
        )}
      </Frame>,
    );

    const [finding] = boundaryFindings(new Map([[boundary.id, boundary]]), globalThis.document);
    expect(finding.owns).toEqual(["p#one", "p#two", "p#three"]);
    expect(finding.more).toBe(2);
  });

  it("says a boundary with no marks on the page is showing its fallback", () => {
    // React removes a boundary's content while its fallback is up, and the
    // marks are part of that content. "Not in the document" is therefore an
    // answer rather than a gap, and it is the answer a reader wants.
    const boundary = suspense(suspenseId(0), 0);

    mount(<Frame>{null}</Frame>);

    const [finding] = boundaryFindings(new Map([[boundary.id, boundary]]), globalThis.document);
    expect(finding.rendered).toBe(false);
    expect(finding.owns).toEqual([]);
  });

  it("puts no mark in the markup a server renders", () => {
    // The whole of why a mark is a sibling that arrives after mount rather than
    // a wrapper that is there from the start: the tree React hydrates against
    // the server's markup has no mark in it, so the gate is free to answer
    // differently in the two processes. An effect is what makes a mark appear,
    // and a server render runs none.
    const boundary = suspense(suspenseId(0), 0);

    const html = renderToStaticMarkup(
      <Frame>{insideBoundary(boundary, <article className="post">the post</article>)}</Frame>,
    );

    expect(html).not.toContain(BOUNDARY_ATTRIBUTE);
    expect(html).toContain("the post");
  });

  it("returns the children untouched when there is no boundary to mark", () => {
    const container = mount(<Frame>{insideBoundary(null, <article>the post</article>)}</Frame>);

    expect(elementsIn(container, `[${BOUNDARY_ATTRIBUTE}]`)).toEqual([]);
  });

  it("names the file on the opening mark, where a reader will be looking", () => {
    // The inspector is the other half of "see it" and the cheaper half, so the
    // mark has to answer on its own: an id says which boundary and this says
    // which file wrote it. Only on the opening one — the closing mark is a
    // position, and a second copy of the answer is a second thing to keep true.
    const boundary: RouteBoundary = {
      id: ROUTE_ERROR_ID,
      kind: "error",
      above: 1,
      source: "app/docs/$error.js",
    };

    const container = mount(<Frame>{insideBoundary(boundary, <article>the post</article>)}</Frame>);

    expect(elementsIn(container, `[${SOURCE_ATTRIBUTE}="app/docs/$error.js"]`).length).toBe(1);
    expect(elementIn(container, `[${SOURCE_ATTRIBUTE}]`).getAttribute(EDGE_ATTRIBUTE)).toBe("open");
  });
});

// --- What a reader is told -------------------------------------------------

describe("the report", () => {
  it("names each boundary, where it sits and what it took over", () => {
    const findings: $ReadOnlyArray<BoundaryFinding> = [
      {
        boundary: { id: ROOT_ERROR_ID, kind: "error", above: 0, source: SYNTHESISED_SOURCE },
        owns: ["div#shell"],
        more: 0,
        rendered: true,
      },
      {
        boundary: { id: ROUTE_ERROR_ID, kind: "error", above: 1, source: "app/docs/$error.js" },
        owns: ["article.doc", "aside.toc"],
        more: 3,
        rendered: true,
      },
      {
        boundary: { id: suspenseId(0), kind: "suspense", above: 2, source: null },
        owns: [],
        more: 0,
        rendered: false,
      },
    ];

    const report = formatBoundaries("/docs/:slug", findings);

    expect(report.message).toBe("3 boundaries render /docs/:slug");
    expect(report.detail).toEqual([
      "error (uf's own error page), outside every layout — owns div#shell",
      "error (app/docs/$error.js), inside 1 layout — owns article.doc, aside.toc and 3 more",
      "suspense, inside 2 layouts — showing its fallback",
    ]);
  });

  it("says nothing about a route it is seeing for the first time, or seeing again", async () => {
    // Quiet on the first sighting, for the reason ubugeeei-prod/uf#636 is quiet
    // on the first scan: a listing of everything, at the moment somebody loaded
    // a page, is the banner nobody reads.
    const boundaries: Map<string, RouteBoundary> = new Map([
      [suspenseId(0), suspense(suspenseId(0), 0)],
    ]);

    const posted = withPoster(() => {
      mount(<BoundaryReporter path="/docs/:slug" boundaries={boundaries} />);
      mount(<BoundaryReporter path="/docs/:slug" boundaries={boundaries} />);
    });

    expect(posted).toEqual([]);
  });

  it("reports the moment the same route's boundaries change", async () => {
    // Which is the moment somebody added an `$error.js` and HMR handed the
    // page a new table — the one time a boundary map is what the reader wants.
    const before: Map<string, RouteBoundary> = new Map([
      [suspenseId(0), suspense(suspenseId(0), 0)],
    ]);
    const routeError: RouteBoundary = {
      id: ROUTE_ERROR_ID,
      kind: "error",
      above: 1,
      source: "app/$error.js",
    };
    const after: Map<string, RouteBoundary> = new Map([
      [ROUTE_ERROR_ID, routeError],
      [suspenseId(0), suspense(suspenseId(0), 0)],
    ]);

    const posted = withPoster(() => {
      mount(<BoundaryReporter path="/docs/:slug" boundaries={before} />);
      mount(<BoundaryReporter path="/docs/:slug" boundaries={after} />);
    });

    expect(posted.length).toBe(1);
    expect(posted[0].target).toBe(DIAGNOSTIC_ENDPOINT);
    const body = JSON.parse(String(posted[0].body));
    // `info`, not `warn`: nothing is wrong. It is a measurement of a decision
    // the build made, and a warning nobody can act on teaches a reader to
    // ignore the warnings.
    expect(body.severity).toBe("info");
    expect(body.message).toBe("2 boundaries render /docs/:slug");
    expect(body.detail).toEqual([
      "error (app/$error.js), inside 1 layout — showing its fallback",
      "suspense, outside every layout — showing its fallback",
    ]);
  });
});

describe("the on-demand report", () => {
  it("prints the page in front of you when the console asks for it", () => {
    // The change-driven report deliberately says nothing about a route it is
    // seeing for the first time, so this is the answer to "and what about the
    // page I am on". It returns the findings as well as printing them: the
    // browser shows the tree, the terminal keeps the line.
    const boundary = suspense(suspenseId(0), 1);
    const boundaries: Map<string, RouteBoundary> = new Map([[boundary.id, boundary]]);

    const posted = withPoster(() => {
      mount(
        <Frame>
          {insideBoundary(boundary, <article className="post">the post</article>)}
          <BoundaryReporter path="/docs/:slug" boundaries={boundaries} />
        </Frame>,
      );
      // Through a named object rather than `globalThis[…]`, which the checker
      // reads as a computed property on a namespace: the same narrowing
      // `@uniflowed/react-testing`'s `internal/dom.js` uses to read a global.
      const globals: { readonly [string]: mixed } = globalThis;
      const ask: $FlowFixMe = globals[BOUNDARY_GLOBAL];
      expect(typeof ask).toBe("function");
      expect(ask().map((it) => it.owns)).toEqual([["article.post"]]);
    });

    expect(posted.length).toBe(1);
    expect(JSON.parse(String(posted[0].body)).detail).toEqual([
      "suspense, inside 1 layout — owns article.post",
    ]);
  });
});

describe("naming an element", () => {
  it("prefers the id, then the first class, then the tag", () => {
    const document = globalThis.document ?? render(<div />).container.ownerDocument;
    const withId = document.createElement("section");
    withId.setAttribute("id", "shell");
    withId.setAttribute("class", "wide dark");
    const withClass = document.createElement("article");
    withClass.setAttribute("class", "post featured");
    const bare = document.createElement("p");

    expect(describeElement(withId)).toBe("section#shell");
    expect(describeElement(withClass)).toBe("article.post");
    expect(describeElement(bare)).toBe("p");
  });
});

// --- None of it is in a build ----------------------------------------------

describe("a bundle without `import.meta.hot`", () => {
  it("renders no mark at all", async () => {
    // The gate is `import.meta.hot != null`, which Vite replaces with
    // `undefined` in a build and Node never defines — so a host that imports
    // the router without a bundler gets the production path, and so does this
    // process. What is asserted here is that path: the same `RouteView` that
    // marks under `uf dev` renders the tree it rendered before any of this
    // existed.
    const table = {
      routes: [
        {
          path: "/plain",
          params: [],
          mdx: false,
          file: "app/plain/$page.js",
          page: pageOf("the plain page"),
          layouts: [loadFrame],
          loading: [{ above: 1, module: loadFallback }],
          templates: [],
        },
      ],
      notFound: [],
      errors: [],
    };
    const resolved = await resolveMatch(table, "/plain");

    const container = mount(
      <RouterProvider url="/plain" initial={resolved}>
        <RouteView />
      </RouterProvider>,
    );

    expect(elementsIn(container, `[${BOUNDARY_ATTRIBUTE}]`)).toEqual([]);
    expect(elementsIn(container, `[${EDGE_ATTRIBUTE}]`)).toEqual([]);
  });

  it("prerenders a document with none either", async () => {
    const table = {
      routes: [
        {
          path: "/plain",
          params: [],
          mdx: false,
          file: "app/plain/$page.js",
          page: pageOf("the plain page"),
          layouts: [loadFrame],
          loading: [],
          templates: [],
        },
      ],
      notFound: [],
      errors: [],
    };
    const { prerender } = createRenderer({ App: routerView("./app"), ...table });

    const result = await prerender("/plain", { scripts: [], styles: [], preloads: [] });

    expect(result.html).not.toContain(BOUNDARY_ATTRIBUTE);
  });
});

/** One post the reporter made, as the stub saw it. */
type Posted = {| readonly target: string, readonly body: mixed |};

/**
 * Run `body` with the page's `fetch` replaced, and return what was posted.
 *
 * The diagnostic channel is a `fetch` to a path on the page's own origin, and
 * this test process has a happy-dom window whose `fetch` would try to make the
 * request. Swapping it is also the assertion: a reporter that posted somewhere
 * else would show up here as nothing posted at all.
 */
function withPoster(body: () => void): $ReadOnlyArray<Posted> {
  const posted: Array<Posted> = [];
  const win: $FlowFixMe = globalThis.window;
  const previous = Object.getOwnPropertyDescriptor(win, "fetch");
  Object.defineProperty(win, "fetch", {
    value: (target: string, init: { readonly body: mixed, ... }) => {
      posted.push({ target, body: init.body });
      return Promise.resolve(null);
    },
    configurable: true,
    writable: true,
  });
  try {
    body();
  } finally {
    if (previous == null) {
      delete win.fetch;
    } else {
      Object.defineProperty(win, "fetch", previous);
    }
  }
  return posted;
}
