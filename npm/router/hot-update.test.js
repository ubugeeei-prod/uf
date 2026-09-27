// @flow
//
// A dev server's edit, arriving as a render rather than a reload.
//
// `@uniflowed/vite` generates three calls into `virtual:uf/client` for a dev
// server — `acceptHotRouteModules`, `refreshForHotUpdate` and
// `replaceRoutesForHotUpdate` — and these mount a router the way a hydrated
// application has one and make those calls the way the dev server's updates
// do. What each case asks is the whole point of the feature: the edit is on
// screen, and the `useState` next to it survived.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { act, cleanup, render, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../npm/react-testing/internal/dom.js";
import { loadOnce, retainHotModules } from "./internal/resolve.js";
import {
  type RouteTable,
  acceptHotRouteModules,
  installRoutes,
  refreshForHotUpdate,
  replaceRoutesForHotUpdate,
  resolveMatch,
  routerView,
} from "./internal/runtime.js";

const globals: $FlowFixMe = globalThis;

afterEach(() => {
  if (globals.document != null) {
    cleanup();
    globals.document.body.replaceChildren();
    globals.window.history.replaceState(null, "", "/");
  }
  // A replaced module is module state, and a worker runs many files out of
  // one module registry. Forgotten directly rather than through
  // `replaceRoutesForHotUpdate`, which would schedule a refresh — and, with no
  // router left mounted, a reload of whatever document the next file uses.
  retainHotModules(null);
});

component Counter() {
  const [count, setCount] = useState<number>(0);
  return (
    <button type="button" onClick={() => setCount((value) => value + 1)}>
      {`count ${count}`}
    </button>
  );
}

component Home(data: mixed) {
  const answer: $FlowFixMe = data;
  return (
    <main>
      <h1>{`greeting ${String(answer?.greeting)}`}</h1>
      <Counter />
    </main>
  );
}

/** A module whose loader answers `greeting`, as a route module exports one. */
function homeModule(greeting: string): { ... } {
  return { default: Home, loader: () => ({ greeting }) };
}

/** A table whose page loader is tagged the way the dev server tags it. */
function table(): RouteTable {
  const page: $FlowFixMe = () => Promise.resolve(homeModule("first"));
  page.ufHotFile = "app/$page.js";
  const routes: $FlowFixMe = [
    { path: "/", params: [], mdx: false, file: "app/$page.js", page, layouts: [], loading: [] },
  ];
  return { routes, notFound: [], errors: [] };
}

async function mount(routes: RouteTable): Promise<void> {
  installDom();
  installRoutes(routes);
  globals.window.history.replaceState(null, "", "/");
  const initial = await resolveMatch(routes, "/");
  const App = routerView("./app");
  await act(async () => {
    render(<App url="/" initial={initial} />);
  });
}

function heading(): string {
  return globals.document.querySelector("h1")?.textContent ?? "(no heading)";
}

function counter(): string {
  return globals.document.querySelector("button")?.textContent ?? "(no counter)";
}

async function increment(): Promise<void> {
  await act(async () => {
    await userEvent.click(globals.document.querySelector("button"));
  });
}

describe("a route module replaced by the dev server", () => {
  it("runs its new loader and keeps the page's state", async () => {
    await mount(table());
    acceptHotRouteModules();
    await increment();
    expect(heading()).toBe("greeting first");
    expect(counter()).toBe("count 1");

    // What the Fast Refresh wrapper of `app/$page.js` calls with the module's
    // next exports.
    await act(async () => {
      globals.window.__UF_HOT_ROUTE__("app/$page.js", homeModule("second"));
    });

    await waitFor(() => expect(heading()).toBe("greeting second"));
    expect(counter()).toBe("count 1");
  });

  it("answers that module's loader with the next exports", async () => {
    const routes = table();
    await mount(routes);
    acceptHotRouteModules();
    const replaced = homeModule("second");
    await act(async () => {
      globals.window.__UF_HOT_ROUTE__("app/$page.js", replaced);
    });
    await waitFor(() => expect(heading()).toBe("greeting second"));
    const load: $FlowFixMe = routes.routes[0].page;
    expect(await loadOnce(load)).toBe(replaced);
  });

  it("outlives a new table that still routes to it", async () => {
    // A rebuilt table imports the module under a URL of its own; answering
    // with that second evaluation would be a `Page` React has never seen.
    const routes = table();
    await mount(routes);
    acceptHotRouteModules();
    await act(async () => {
      globals.window.__UF_HOT_ROUTE__("app/$page.js", homeModule("second"));
    });
    await waitFor(() => expect(heading()).toBe("greeting second"));
    await increment();
    await act(async () => {
      replaceRoutesForHotUpdate({ ...table(), hotFiles: new Set(["app/$page.js"]) });
    });
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(heading()).toBe("greeting second");
    expect(counter()).toBe("count 1");
  });

  it("is forgotten once no table routes to it", async () => {
    const routes = table();
    await mount(routes);
    acceptHotRouteModules();
    await act(async () => {
      globals.window.__UF_HOT_ROUTE__("app/$page.js", homeModule("second"));
    });
    await waitFor(() => expect(heading()).toBe("greeting second"));
    await act(async () => {
      replaceRoutesForHotUpdate({ ...routes, hotFiles: new Set() });
    });
    await waitFor(() => expect(heading()).toBe("greeting first"));
    const load: $FlowFixMe = routes.routes[0].page;
    const loaded: $FlowFixMe = await loadOnce(load);
    expect(loaded.loader()).toEqual({ greeting: "first" });
  });
});

describe("uf:refresh", () => {
  it("renders the URL on screen again without losing state", async () => {
    let greeting = "first";
    const page: $FlowFixMe = () => Promise.resolve({ default: Home, loader: () => ({ greeting }) });
    await mount({
      routes: [
        { path: "/", params: [], mdx: false, file: "app/$page.js", page, layouts: [], loading: [] },
      ],
      notFound: [],
      errors: [],
    });
    await increment();
    await increment();

    greeting = "edited";
    await act(async () => {
      refreshForHotUpdate();
    });

    await waitFor(() => expect(heading()).toBe("greeting edited"));
    expect(counter()).toBe("count 2");
  });
});

describe("a route table rebuilt by the dev server", () => {
  it("is installed in place and rendered against", async () => {
    await mount(table());
    await increment();

    const rebuilt = table();
    // $FlowFixMe[cannot-write] a new module behind the same path, as a rebuilt table has.
    rebuilt.routes[0].page = () => Promise.resolve(homeModule("rebuilt"));
    await act(async () => {
      replaceRoutesForHotUpdate(rebuilt);
    });

    await waitFor(() => expect(heading()).toBe("greeting rebuilt"));
    expect(counter()).toBe("count 1");
  });
});

describe("a client reference after a hot update", () => {
  it("resolves to the module's newest evaluation", async () => {
    // A Server Components payload names `/app/counter.js`; after a hot update
    // the component React has mounted is the one the updated module exported,
    // and a reference that answered with the first evaluation would remount it.
    installDom();
    const { installBrowserModules } = await import("./internal/flight-browser.js");
    installBrowserModules();
    const newest = { Counter: () => null };
    globals.window.__UF_LATEST_MODULES__ = new Map([["/app/counter.js", newest]]);
    try {
      expect(globals.parcelRequire("/app/counter.js")).toBe(newest);
    } finally {
      delete globals.window.__UF_LATEST_MODULES__;
    }
    // And nothing is invented where the registry does not exist, as in a build.
    expect(() => globals.parcelRequire("/app/counter.js")).toThrow("was required before it loaded");
  });
});
