// @flow
//
// A navigation that is a transition rather than a cut.
//
// `document.startViewTransition` is opt-in per navigation, so something has to
// call it, and nothing in uf did: every client navigation replaced the tree and
// the browser painted the new one. See ubugeeei-prod/uf#502.
//
// The router is what replaces the tree, so the router is what calls it — and
// the interesting half of that decision is what happens when it cannot. Three
// of the cases below are about *not* transitioning: a browser that has no such
// method, a reader who asked for less motion, and a caller that said this
// navigation is a change of state rather than a change of place. Each has to
// arrive at the same page it arrived at before any of this existed, because a
// feature that degrades into a broken link is worse than no feature.
//
// The transition itself is stubbed rather than run. A real one is compositor
// work in a browser that happy-dom is not; what a test can hold the router to
// is that it asked, that it said which transition it was asking for, and that
// it stopped saying so afterwards.

import * as React from "@uniflowed/react";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";
import {
  Link,
  RouteView,
  RouterProvider,
  type RouteTable,
  resolveMatch,
  routerView,
} from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterEach, describe, expect, it } from "@uniflowed/test";

// Reached by path rather than by package name, the way `rsc-split.test.js`
// reaches for the same module: the document has to exist before a stub can be
// put on it, and `render` installs one only when it is called.
import { installDom } from "../../packages/react-testing/internal/dom.js";

component Home() {
  return (
    <main>
      <h1>home</h1>
      <Link to="/guide">go to the manual</Link>
      <Link to="/guide" transition={false}>
        go to the manual without animating
      </Link>
    </main>
  );
}

component Guide() {
  return <h1>the manual</h1>;
}

component GuideLayout(children: React.Node) {
  return <div className="guide">{children}</div>;
}

/**
 * A site with a home page and a manual, where the manual names its transition.
 *
 * Built fresh per test rather than shared, because the runtime caches a loaded
 * module by the identity of the function that loads it — two tests sharing one
 * table would share whichever page the first of them resolved.
 */
function site(options?: {| readonly on?: "page" | "layout" |}): RouteTable {
  const where = options?.on ?? "page";
  const guide = { default: Guide, viewTransition: where === "page" ? "manual" : undefined };
  const layout = {
    default: GuideLayout,
    viewTransition: where === "layout" ? "manual" : undefined,
  };
  return {
    routes: [
      {
        path: "/",
        params: [],
        mdx: false,
        file: "app/_uf.page.js",
        page: () => Promise.resolve({ default: Home }),
        layouts: [],
      },
      {
        path: "/guide",
        params: [],
        mdx: false,
        file: "app/guide/_uf.page.js",
        page: () => Promise.resolve(guide),
        layouts: [() => Promise.resolve(layout)],
      },
    ],
    notFound: [],
    errors: [],
  };
}

/** What one call to `startViewTransition` looked like from the router's side. */
type Started = {|
  /** The name on the document element at the moment the transition began. */
  readonly named: ?string,
  /** What was on the page once the update callback had returned. */
  readonly painted: string,
|};

/** The one thing the router reads off a transition: that it is over. */
type StubTransition = {| readonly finished: Promise<void> |};

/**
 * The document, as something this file can put a `startViewTransition` on.
 *
 * Flow's `Document` has no such property, and that a browser may not have it is
 * the entire reason the router feature-detects it — so installing a stub has to
 * go through a type that says it may be there. An `interface` because a
 * `Document` is a class instance and class instances are not subtypes of object
 * types, which is the same reason the runtime declares its own.
 */
interface StubDocument {
  startViewTransition?: (update: () => mixed) => StubTransition;
}

/** The document, with the property this file installs on it declared. */
function stubDocument(): StubDocument {
  installDom();
  return globalThis.document;
}

/**
 * A `startViewTransition` that records the call and applies the update.
 *
 * The name is read *inside* the callback rather than afterwards, because when
 * it is set is the whole of what it is for: a browser captures the old frame
 * before calling back, and an attribute added after that is an attribute no
 * `::view-transition-old` selector ever saw.
 *
 * `painted` is read after the callback for the mirror-image reason. A browser
 * captures the new frame from whatever the DOM holds when the callback
 * settles, so a router that scheduled the commit instead of performing it
 * hands the browser two captures of the same page and calls it a transition.
 *
 * `finished` is settled by the caller so a test can hold the transition open
 * and ask what the document says while it is running.
 */
function installViewTransitions(): {|
  readonly started: $ReadOnlyArray<Started>,
  readonly finish: () => Promise<void>,
|} {
  const started: Array<Started> = [];
  let settle: () => void = () => {};
  const finished = new Promise<void>((resolve) => {
    settle = resolve;
  });

  stubDocument().startViewTransition = (update: () => mixed) => {
    const named = root().getAttribute("data-uf-view-transition");
    update();
    started.push({ named, painted: root().textContent ?? "" });
    return { finished };
  };

  return {
    started,
    finish: async () => {
      settle();
      // Two turns: one for `finished` itself, one for the `.then` the router
      // took off it. A single `await` would ask what the document says while
      // the handler that changes it is still queued.
      await finished;
      await Promise.resolve();
    },
  };
}

/** Say that the reader has asked their system for less motion. */
function installReducedMotion(matches: boolean): void {
  installDom();
  Object.defineProperty(globalThis.window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: matches && query.includes("prefers-reduced-motion"),
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }),
  });
}

/** The document element, which is where a transition's name is written. */
function root(): Element {
  const element = globalThis.document.documentElement;
  if (element == null) {
    throw new Error("the document has no root element");
  }
  return element;
}

/**
 * Install the table and mount the site at `/`, ready to navigate.
 *
 * `createRenderer` is doing a second job here. The router holds one route
 * table per process, installed by whichever entry started the app, and
 * `RouterProvider` resolves the *next* route out of that table rather than out
 * of a prop — so a test that only rendered the provider navigates into a
 * router with no table at all. The runtime says so rather than silently doing
 * nothing, which is how this was found.
 */
async function atHome(table: RouteTable): Promise<void> {
  installDom();
  createRenderer({
    App: routerView("./app"),
    routes: table.routes,
    notFound: table.notFound,
    errors: table.errors,
  });
  globalThis.window.history.pushState(null, "", "/");
  const resolved = await resolveMatch(table, "/");
  render(
    <RouterProvider url="/" initial={resolved}>
      <RouteView />
    </RouterProvider>,
  );
}

/** Follow a link, letting everything the navigation queued run. */
async function follow(name: string): Promise<void> {
  await act(async () => {
    await userEvent.click(screen.getByRole("link", { name }));
  });
}

/**
 * The heading of the page that is on screen.
 *
 * By role, so "the manual arrived" cannot be satisfied by the link that points
 * at it — which is the way a navigation that silently did nothing would pass
 * every assertion in this file.
 */
function heading(): string {
  return screen.getByRole("heading").textContent ?? "";
}

afterEach(() => {
  cleanup();
  // The stub is an own property put on a document that is installed once per
  // worker and shared with every other file scheduled onto it: one left behind
  // is one the next file runs under. `undefined` rather than `delete`, because
  // it is what the router's own feature detection reads — a property that is
  // not a function is a browser that does not have the API.
  if (globalThis.document != null) {
    stubDocument().startViewTransition = undefined;
    root().removeAttribute("data-uf-view-transition");
  }
});

describe("a navigation in a browser that has view transitions", () => {
  it("goes through one", async () => {
    const transitions = installViewTransitions();
    await atHome(site());

    await follow("go to the manual");

    expect(transitions.started.length).toBe(1);
    expect(heading()).toBe("the manual");
  });

  it("hands the browser a page that has already changed", async () => {
    // The reason the commit inside a transition is `flushSync` and not
    // `startTransition`: the browser captures the new frame from whatever the
    // DOM holds when the callback settles, and a scheduled commit has changed
    // nothing by then. Two captures of the same page is not a transition, and
    // nothing about it looks broken — it looks like the animation not firing.
    const transitions = installViewTransitions();
    await atHome(site());

    await follow("go to the manual");

    expect(transitions.started[0].painted).toContain("the manual");
    expect(transitions.started[0].painted).not.toContain("home");
  });

  it("says which transition it is, from the page that named it", async () => {
    // The name is what lets one stylesheet animate an arrival into the manual
    // differently from an arrival anywhere else. Without it a project has one
    // transition for the whole site, which is the same as having none it can
    // design.
    const transitions = installViewTransitions();
    await atHome(site());

    await follow("go to the manual");

    expect(transitions.started[0].named).toBe("manual");
  });

  it("takes the name from a layout, for every route under it", async () => {
    // The rule `metadata` already follows. A section that animates the same
    // way throughout should say so once, in the file that is the section.
    const transitions = installViewTransitions();
    await atHome(site({ on: "layout" }));

    await follow("go to the manual");

    expect(transitions.started[0].named).toBe("manual");
  });

  it("stops saying so once the transition has ended", async () => {
    // The attribute is global state on the document element. Left behind, the
    // next navigation animates under the previous page's name — and the page
    // after that under one nothing on screen ever declared.
    const transitions = installViewTransitions();
    await atHome(site());
    await follow("go to the manual");
    expect(root().getAttribute("data-uf-view-transition")).toBe("manual");

    await act(async () => {
      await transitions.finish();
    });

    expect(root().getAttribute("data-uf-view-transition")).toBe(null);
  });
});

describe("a navigation that must not transition", () => {
  it("still arrives in a browser that has no startViewTransition", async () => {
    // The case the whole feature is measured against. Nothing is installed, so
    // `document.startViewTransition` is undefined — which is what every
    // browser older than the API is, and what one that never ships it stays.
    await atHome(site());

    await follow("go to the manual");

    expect(heading()).toBe("the manual");
    expect(root().getAttribute("data-uf-view-transition")).toBe(null);
  });

  it("is a cut for a reader who asked for less motion, without being asked to be", async () => {
    // `prefers-reduced-motion` is honoured by the router rather than by the
    // application. An application that had to remember would forget, and the
    // reader who set the preference is the one person who cannot see that it
    // was forgotten.
    const transitions = installViewTransitions();
    installReducedMotion(true);
    try {
      await atHome(site());

      await follow("go to the manual");

      expect(transitions.started.length).toBe(0);
      expect(heading()).toBe("the manual");
    } finally {
      installReducedMotion(false);
    }
  });

  it("is a cut when the link says this one is not a change of place", async () => {
    const transitions = installViewTransitions();
    await atHome(site());

    await follow("go to the manual without animating");

    expect(transitions.started.length).toBe(0);
    expect(heading()).toBe("the manual");
  });
});

describe("the server", () => {
  it("renders nothing about a transition, because a transition is not markup", async () => {
    // A transition is a client-only concern, and the moment one reaches the
    // markup it is a hydration difference instead: the server would write an
    // attribute the first client render does not, on the one element React
    // cannot re-render its way out of.
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: site().routes,
      notFound: [],
      errors: [],
    });

    const result = await prerender("/guide", { scripts: [], styles: [], preloads: [] });

    expect(result.status).toBe(200);
    expect(result.html).toContain("the manual");
    expect(result.html).not.toContain("data-uf-view-transition");
  });
});
