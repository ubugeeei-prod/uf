// @flow
//
// `app.rendering.navigation`: what a link does.
//
// uf has always shipped one answer — the client router takes a plain left
// click over, resolves the next route and commits it into the page that is
// already open. `"document"` is the other one: the browser follows the link,
// the way it follows a link on a page with no JavaScript on it at all.
//
// Two halves, and they fail in different places.
//
// **The generated entry.** `@uniflowed/vite` writes the mode into
// `virtual:uf/client` as a constant, so a build has nothing to decide and a
// project that never set the key gets the module it has always got. Getting
// this wrong is a build that hydrates in a mode the config did not ask for,
// and nothing in `dist/` shows which one happened.
//
// **The runtime.** A `Link` under document navigation has to be an *ordinary*
// anchor — no handler of uf's on it, nothing calling `preventDefault` — rather
// than a handler that performs the navigation itself. The two look identical
// for a plain left click and are not the same link: a scripted navigation
// loses `download`, loses a `target`, and decides for the browser what it
// should do with a gesture uf has not heard of. So what is asserted below is
// that the click event comes out of React *unprevented*, which is the only
// observable difference between a link and an imitation of one.
//
// The runtime half needs a document, so the imports are split the way
// `rsc-split.test.js` splits them: `@uniflowed/router/client` reads `document`
// while it is being evaluated, so it is imported after `installDom` and not at
// the top of the file.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { act, cleanup, userEvent } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
// By path rather than through `@uniflowed/router`, and it is not a shortcut:
// `installNavigation` is module state, so a test that reached it through the
// package specifier and a runtime that reached it relatively would have to be
// the same module instance for the reset below to reset anything. Taking both
// from the one path is how that stops being a property of the resolver.
import { Link, installNavigation, routerView, useRouter } from "./internal/runtime.js";
import { clientModuleSource } from "../../packages/vite/internal/routes.js";

// ---------------------------------------------------------------------------
// The generated entry
// ---------------------------------------------------------------------------

describe("the client entry @uniflowed/vite generates", () => {
  it("says nothing about navigation when the project said nothing", () => {
    // Byte for byte the module a project got before this option existed. A
    // default that is spelled out is a default that shows up in every diff of
    // every project the day it is added.
    const source = clientModuleSource("/app.js");

    expect(source).toContain("hydrate({ App, routes, notFound, errors });");
    expect(source).not.toContain("navigation");
  });

  it("says nothing when the project asked for the client router", () => {
    const source = clientModuleSource("/app.js", { navigation: "client" });

    expect(source).toContain("hydrate({ App, routes, notFound, errors });");
  });

  it("writes the mode in when the project asked for document navigation", () => {
    const source = clientModuleSource("/app.js", { navigation: "document" });

    expect(source).toContain('hydrate({ App, routes, notFound, errors, navigation: "document" });');
  });

  it("carries Strict Mode alongside it", () => {
    // The two are independent — one is a development-only double render, the
    // other is what a link does in every environment — and the entry has to be
    // able to say both.
    const source = clientModuleSource("/app.js", { strictMode: true, navigation: "document" });

    expect(source).toContain(
      'hydrate({ App, routes, notFound, errors, strictMode: true, navigation: "document" });',
    );
  });
});

// ---------------------------------------------------------------------------
// The runtime
// ---------------------------------------------------------------------------

async function clientModule() {
  installDom();
  return import("@uniflowed/router/client");
}

async function serverModule() {
  installDom();
  return import("@uniflowed/router/server");
}

/** How many times each route's module was asked for, by path. */
const loaded: { [string]: number } = {};

/** The route the prefetch tests aim at, and nothing else does. */
const PREFETCHED = "/prefetched";

/**
 * The route table, built once.
 *
 * Memoised for the reason `rsc-split.test.js` memoises its own: `loadOnce`
 * caches a module by the identity of the function that loads it, and the
 * server render and the hydration have to be the same components for React's
 * comparison to mean anything.
 */
let built: mixed = null;

function tables() {
  if (built != null) {
    return built;
  }

  component Home() {
    const router = useRouter();
    const [clicked, setClicked] = useState<number>(0);
    return (
      <section>
        <h1>home</h1>
        <Link to="/other">other</Link>
        <Link to="/other" data-testid="with-onclick" onClick={() => setClicked(clicked + 1)}>
          other, with an onClick
        </Link>
        <output>{clicked}</output>
        <button type="button" onClick={() => void router.push("/other")}>
          push
        </button>
        <button type="button" onClick={() => void router.replace("/other")}>
          replace
        </button>
        <button type="button" onClick={() => void router.prefetch(PREFETCHED)}>
          prefetch
        </button>
      </section>
    );
  }

  component Other() {
    return <h1>other</h1>;
  }

  const home = {
    path: "/",
    params: [],
    mdx: false,
    file: "app/$page.js",
    page: () => {
      loaded["/"] = (loaded["/"] ?? 0) + 1;
      return Promise.resolve({ default: Home });
    },
    layouts: [],
    loading: [],
  };
  const other = {
    path: "/other",
    params: [],
    mdx: false,
    file: "app/other/$page.js",
    page: () => {
      loaded["/other"] = (loaded["/other"] ?? 0) + 1;
      return Promise.resolve({ default: Other });
    },
    layouts: [],
    loading: [],
  };

  built = { routes: [home, other], other };
  return built;
}

/**
 * The prefetch target, built fresh for each hydration.
 *
 * `loadOnce` in the runtime caches a module by the identity of the function
 * that loads it, and that cache is a module-level one that outlives a test. A
 * memoised loader would therefore be a cache hit for every test after the
 * first, and "the client router prefetched it" would be indistinguishable
 * from "something earlier in this file already had it". A new function per
 * hydration is a new cache key, so each test asks the question from cold.
 */
function prefetchRoute() {
  const { other } = tables();
  return {
    path: PREFETCHED,
    params: [],
    mdx: false,
    file: "app/prefetched/$page.js",
    page: () => {
      loaded[PREFETCHED] = (loaded[PREFETCHED] ?? 0) + 1;
      return other.page();
    },
    layouts: [],
    loading: [],
  };
}

/** Put the document the server would have written into the live DOM. */
async function serve(url: string): Promise<void> {
  const { createRenderer, ROOT_ID } = await serverModule();
  const { routes } = tables();
  const renderer = createRenderer({
    App: routerView("./app"),
    routes,
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

/** Hydrate the current document in one of the two modes. */
async function hydrateHere(navigation: "client" | "document"): Promise<void> {
  const { hydrate } = await clientModule();
  const { routes } = tables();
  await act(async () => {
    await hydrate({
      App: routerView("./app"),
      routes: [...routes, prefetchRoute()],
      notFound: [],
      errors: [],
      navigation,
    });
  });
}

function ufRoot(): Element | null {
  return globalThis.document.getElementById("uf-root");
}

function linkTo(href: string): Element {
  const link = ufRoot()?.querySelector(`a[href="${href}"]`);
  if (link == null) {
    throw new Error(`no link to ${href} in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
  }
  return link;
}

/** The second link home renders: the same destination, plus an `onClick`. */
function linkWithOnClick(): Element {
  const link = ufRoot()?.querySelector('a[data-testid="with-onclick"]');
  if (link == null) {
    throw new Error(`no link with an onClick in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
  }
  return link;
}

function buttonSaying(label: string): Element {
  for (const button of ufRoot()?.querySelectorAll("button") ?? []) {
    if (button.textContent === label) {
      return button;
    }
  }
  throw new Error(`no ${label} button in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
}

/**
 * Click `element` and report whether anything called `preventDefault`.
 *
 * The listener is on the document, so it runs after React's own — React
 * attaches at the root container — and it prevents the default itself once it
 * has read the answer, because the default for an anchor is a navigation and
 * this is a test, not a browser.
 */
async function clickAndReportPrevention(element: Element): Promise<boolean> {
  let prevented = null;
  const listener = (event: Event) => {
    prevented = event.defaultPrevented;
    event.preventDefault();
  };
  globalThis.document.addEventListener("click", listener);
  try {
    await act(async () => {
      await userEvent.click(element);
    });
  } finally {
    globalThis.document.removeEventListener("click", listener);
  }
  if (prevented == null) {
    throw new Error("the click never reached the document");
  }
  return prevented;
}

/** Record what a navigation asked the browser to do, and put it back after. */
async function watchingLocation(
  run: () => Promise<void>,
): Promise<{ assigned: Array<string>, replaced: Array<string> }> {
  const assigned: Array<string> = [];
  const replaced: Array<string> = [];
  const location = globalThis.window.location;
  const originalAssign = location.assign;
  const originalReplace = location.replace;
  const stub = (into: Array<string>) => (to: string) => {
    into.push(String(to));
  };
  Object.defineProperty(location, "assign", {
    configurable: true,
    writable: true,
    value: stub(assigned),
  });
  Object.defineProperty(location, "replace", {
    configurable: true,
    writable: true,
    value: stub(replaced),
  });
  try {
    await run();
  } finally {
    Object.defineProperty(location, "assign", {
      configurable: true,
      writable: true,
      value: originalAssign,
    });
    Object.defineProperty(location, "replace", {
      configurable: true,
      writable: true,
      value: originalReplace,
    });
  }
  return { assigned, replaced };
}

afterEach(() => {
  if (globalThis.document == null) {
    return;
  }
  cleanup();
  globalThis.document.body.replaceChildren();
  // The mode is module state on the runtime, and a worker runs many files out
  // of one module registry: a test that left `"document"` installed would be
  // deciding what a `Link` does in whichever file the scheduler put next. See
  // ubugeeei-prod/uf#445, which is the same hazard one question over.
  installNavigation("client");
  for (const key of Object.keys(loaded)) {
    delete loaded[key];
  }
});

describe("a link under document navigation", () => {
  it("is left for the browser, unprevented", async () => {
    await serve("/");
    await hydrateHere("document");

    const prevented = await clickAndReportPrevention(linkTo("/other"));

    expect(prevented).toBe(false);
    // And nothing was rendered over the top of it: the page a document
    // navigation leaves behind is the page that was there.
    expect(ufRoot()?.textContent).toContain("home");
  });

  it("is taken over under the default, which is what it has always done", async () => {
    // The other half of the pair, and the one that must not regress: the same
    // markup, the same click, and the router answers it.
    await serve("/");
    await hydrateHere("client");

    const prevented = await clickAndReportPrevention(linkTo("/other"));

    expect(prevented).toBe(true);
  });

  it("still runs the caller's own onClick", async () => {
    // Dropping uf's behaviour is not dropping the application's. An
    // application that closes a menu when a link is clicked has not asked uf
    // to take the navigation over.
    await serve("/");
    await hydrateHere("document");

    const prevented = await clickAndReportPrevention(linkWithOnClick());

    expect(prevented).toBe(false);
    expect(ufRoot()?.querySelector("output")?.textContent).toBe("1");
  });
});

describe("prefetching under document navigation", () => {
  it("loads nothing, because there is no next render in this page", async () => {
    // `prefetch="intent"` is `Link`'s default, so under the client router the
    // destination's chunks are loaded on hover. A document navigation throws
    // this page away, so those bytes would be spent on a page that is leaving.
    await serve("/");
    await hydrateHere("document");

    await act(async () => {
      await userEvent.click(buttonSaying("prefetch"));
    });

    expect(loaded[PREFETCHED] ?? 0).toBe(0);
  });

  it("loads the destination under the client router", async () => {
    // The pair, so that the assertion above is about the mode rather than
    // about a prefetch that never worked.
    await serve("/");
    await hydrateHere("client");

    await act(async () => {
      await userEvent.click(buttonSaying("prefetch"));
    });

    expect(loaded[PREFETCHED] ?? 0).toBe(1);
  });
});

describe("the router under document navigation", () => {
  it("hands push to the browser", async () => {
    await serve("/");
    await hydrateHere("document");

    const { assigned, replaced } = await watchingLocation(async () => {
      await act(async () => {
        await userEvent.click(buttonSaying("push"));
      });
    });

    expect(assigned.length).toBe(1);
    expect(assigned[0].endsWith("/other")).toBe(true);
    expect(replaced.length).toBe(0);
    // Not `history.pushState`: the browser owns the entry, and it will own it
    // again when the document it is fetching arrives.
    expect(globalThis.window.location.pathname).toBe("/");
  });

  it("hands replace to the browser as a replace", async () => {
    await serve("/");
    await hydrateHere("document");

    const { assigned, replaced } = await watchingLocation(async () => {
      await act(async () => {
        await userEvent.click(buttonSaying("replace"));
      });
    });

    expect(replaced.length).toBe(1);
    expect(replaced[0].endsWith("/other")).toBe(true);
    expect(assigned.length).toBe(0);
  });
});
