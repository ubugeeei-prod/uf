// @flow
//
// `app.rendering.staleTime`: the routes a navigation keeps.
//
// A navigation asked the server every time. A prefetch served one click and was
// thrown away, and a second visit or the back button waited on the network
// again (ubugeeei-prod/uf#960). With a stale time, a route a navigation fetched
// or prefetched is what every navigation to it reads while it is fresh, with no
// request at all, and the first navigation after that asks again.
//
// Four halves. The cache itself, `./internal/navigation-cache.js`, with the
// clock stubbed. The router for an application React Server Components render,
// with the payload fetch replaced by one that counts: `installFlightFetch` is
// the seam `hydrateFlight` uses, so nothing here loads React's Flight client or
// goes near a network. The router for an application rendered from its
// modules, where what a prefetch keeps is the loader's answer. And the client
// entries `@uniflowed/vite` writes, which say nothing for the default.

import * as React from "@uniflowed/react";
import { act, cleanup, render, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, beforeEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../npm/react-testing/internal/dom.js";
import { flightClientSource } from "../../npm/vite/internal/flight.js";
import { clientModuleSource } from "../../npm/vite/internal/routes.js";
import { createServerReference } from "./action.js";
import { encodeActionResult } from "./internal/action-wire.js";
import { installRouting } from "./internal/base-path.js";
import type { FetchedFlight, FlightRoot } from "./internal/flight.js";
import {
  NAVIGATION_CACHE_LIMIT,
  flightNavigations,
  inspectNavigationCache,
  installStaleTime,
  keepsNavigations,
  navigationKey,
  routeNavigations,
} from "./internal/navigation-cache.js";
import {
  type RouteTable,
  Link,
  installFlightFetch,
  installNavigation,
  installRoutes,
  resolveMatch,
  routerView,
  useRoute,
  useLinkStatus,
  useRouter,
} from "./internal/runtime.js";

const globals: $FlowFixMe = globalThis;

/** The clock the cache reads, moved by hand. */
let now = 1_000_000;
let clock = null;

beforeEach(() => {
  now = 1_000_000;
  clock = uft.spyOn(Date, "now").mockImplementation(() => now);
});

afterEach(() => {
  clock?.mockRestore();
  clock = null;
  // Module state, like the navigation mode: a worker runs many files out of one
  // module registry. A stale time of `0` also empties both caches.
  installStaleTime(0);
  installRouting({});
  installNavigation("client");
  if (globals.document != null) {
    cleanup();
    globals.document.body.replaceChildren();
    globals.window.history.replaceState(null, "", "/");
  }
});

// ---------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------

describe("the cache a navigation reads first", () => {
  it("inspects expiration without exposing payloads or removing stale entries", () => {
    installStaleTime(30);
    flightNavigations.store("/guide", answer("/guide"));
    expect(inspectNavigationCache()).toEqual({
      staleTime: 30,
      flight: [{ key: "/guide", cachedAt: now, staleAt: now + 30_000, fresh: true }],
      routes: [],
    });
    now += 30_000;
    expect(inspectNavigationCache().flight[0].fresh).toBe(false);
    expect(flightNavigations.size()).toBe(1);
    installStaleTime(0);
    expect(inspectNavigationCache()).toEqual({ staleTime: 0, flight: [], routes: [] });
  });
  const answer = (url: string): Promise<FetchedFlight> =>
    Promise.resolve({ kind: "document", url });

  it("keeps nothing until a project sets a stale time", () => {
    expect(keepsNavigations()).toBe(false);

    flightNavigations.store("/guide", answer("/guide"));

    expect(flightNavigations.size()).toBe(0);
    expect(flightNavigations.read("/guide")).toBe(null);
  });

  it("serves an entry while it is fresh, and drops it once it is stale", () => {
    installStaleTime(30);
    const kept = answer("/guide");
    flightNavigations.store("/guide", kept);

    now += 29_999;
    expect(flightNavigations.read("/guide")).toBe(kept);
    now += 1;
    expect(flightNavigations.read("/guide")).toBe(null);
    expect(flightNavigations.size()).toBe(0);
  });

  it("keys a route by its application path and query, without the base path", () => {
    installRouting({ basePath: "/docs" });

    expect(navigationKey("/docs/guide", "?tab=api")).toBe("/guide?tab=api");
    expect(navigationKey("/docs", "")).toBe("/");
  });

  it("keeps a bounded number of routes, dropping the oldest", () => {
    installStaleTime(30);
    const resolved: $FlowFixMe = Promise.resolve({ status: 200 });
    for (let index = 0; index <= NAVIGATION_CACHE_LIMIT; index += 1) {
      routeNavigations.store(`/posts/${String(index)}`, resolved);
    }

    expect(routeNavigations.size()).toBe(NAVIGATION_CACHE_LIMIT);
    expect(routeNavigations.read("/posts/0")).toBe(null);
    expect(routeNavigations.read(`/posts/${String(NAVIGATION_CACHE_LIMIT)}`)).toBe(resolved);
  });

  it("forgets an entry only while it is still the one that was kept", () => {
    installStaleTime(30);
    const first = answer("/guide");
    const second = answer("/guide");
    flightNavigations.store("/guide", first);
    flightNavigations.store("/guide", second);

    flightNavigations.forget("/guide", first);

    expect(flightNavigations.read("/guide")).toBe(second);
  });
});

// ---------------------------------------------------------------------------
// An application React Server Components render
// ---------------------------------------------------------------------------

component LinkProgress() {
  const { pending } = useLinkStatus();
  return <span>{pending ? "opening" : "open slow"}</span>;
}

component Screen() {
  const router = useRouter();
  const route = useRoute();
  return (
    <main>
      <h1>{`page ${route.pathname}`}</h1>
      <Link to="/slow" prefetch="off">
        <LinkProgress />
      </Link>
      <Link to="/other" prefetch="off">
        other link
      </Link>
      <button type="button" onClick={() => void router.push("/slow")}>
        to slow
      </button>
      <button type="button" onClick={() => void router.push("/")}>
        to home
      </button>
      <button type="button" onClick={() => void router.push("/broken")}>
        to broken
      </button>
      <button type="button" onClick={() => void router.prefetch("/slow")}>
        prefetch slow
      </button>
      <button type="button" onClick={() => void router.refresh()}>
        refresh
      </button>
    </main>
  );
}

/** The payload root a server would have rendered for `url`. */
function rootFor(url: string, status: number = 200, interceptedFrom?: ?string): FlightRoot {
  const question = url.indexOf("?");
  const pathname = question === -1 ? url : url.slice(0, question);
  const route: $FlowFixMe = {
    pathname,
    search: question === -1 ? "" : url.slice(question),
    path: pathname,
    params: {},
    searchParams: {},
    data: undefined,
    deferred: null,
    metadata: {},
    viewTransition: null,
    status,
    error: null,
    interception:
      interceptedFrom == null
        ? null
        : {
            pathname,
            search: question === -1 ? "" : url.slice(question),
            from: interceptedFrom,
          },
  };
  return { route, tree: <Screen /> };
}

/**
 * Install a payload fetch that answers every URL, and return the URLs it was
 * asked for. `/broken` answers with its error boundary, as a route whose render
 * threw does.
 */
function countedFetches(): Array<string> {
  const asked: Array<string> = [];
  installFlightFetch((url) => {
    asked.push(url);
    const status = url.split("?")[0].endsWith("/broken") ? 500 : 200;
    return Promise.resolve({ kind: "flight", url, root: Promise.resolve(rootFor(url, status)) });
  });
  return asked;
}

async function mountFlight(url: string): Promise<void> {
  installDom();
  globals.window.history.replaceState(null, "", url);
  const App = routerView("./app");
  await act(async () => {
    render(<App url={url} flight={Promise.resolve(rootFor(url))} />);
  });
}

function heading(): string {
  return globals.document.querySelector("h1")?.textContent ?? "(no heading)";
}

async function press(label: string): Promise<void> {
  const button = [...globals.document.querySelectorAll("button")].find(
    (element) => element.textContent === label,
  );
  if (button == null) {
    throw new Error(`no ${label} button in:\n${globals.document.body.innerHTML}`);
  }
  await act(async () => {
    await userEvent.click(button);
  });
}

/** Visit `/slow` and come back home, so `/slow` is a route the page has seen. */
async function visitSlowAndReturn(): Promise<void> {
  await press("to slow");
  await waitFor(() => expect(heading()).toBe("page /slow"));
  await press("to home");
  await waitFor(() => expect(heading()).toBe("page /"));
}

describe("an application React Server Components render, with a stale time", () => {
  it("marks only the clicked link pending until its response arrives", async () => {
    let finish: (value: FetchedFlight) => void = () => {};
    installFlightFetch(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await mountFlight("/");
    const clicked = globals.document.querySelector('a[href="/slow"]');
    const other = globals.document.querySelector('a[href="/other"]');
    await act(async () => {
      clicked.click();
    });
    await waitFor(() => expect(clicked.textContent).toBe("opening"));
    expect(clicked.getAttribute("aria-busy")).toBe("true");
    expect(other.hasAttribute("aria-busy")).toBe(false);
    await act(async () => {
      finish({ kind: "flight", url: "/slow", root: Promise.resolve(rootFor("/slow")) });
    });
    await waitFor(() => expect(heading()).toBe("page /slow"));
    expect(globals.document.querySelector('a[href="/slow"]').textContent).toBe("open slow");
  });
  it("follows a link to a route it prefetched without a request", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");

    await press("prefetch slow");
    await waitFor(() => expect(asked).toEqual(["/slow"]));
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("page /slow"));

    expect(asked).toEqual(["/slow"]);
  });

  it("visits a route it kept again without a request", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    await press("to slow");
    await waitFor(() => expect(heading()).toBe("page /slow"));

    expect(asked).toEqual(["/slow", "/"]);
  });

  it("goes back to a route it kept without a request", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    // What the back button does: the history entry moves, then `popstate` tells
    // the page.
    await act(async () => {
      globals.window.history.replaceState(null, "", "/slow");
      globals.window.dispatchEvent(new globals.window.Event("popstate"));
    });
    await waitFor(() => expect(heading()).toBe("page /slow"));

    expect(asked).toEqual(["/slow", "/"]);
  });

  it("asks again once what it kept is stale", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    now += 30_000;
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("page /slow"));

    expect(asked).toEqual(["/slow", "/", "/slow"]);
  });

  it("keeps one entry for a prefetch and a navigation under a base path", async () => {
    installRouting({ basePath: "/docs" });
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/docs");

    await press("prefetch slow");
    await waitFor(() => expect(asked).toEqual(["/docs/slow"]));
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("page /docs/slow"));

    expect(asked).toEqual(["/docs/slow"]);
    expect(flightNavigations.read("/slow")).not.toBe(null);
  });

  it("does not keep a route that answered with its error boundary", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await press("to broken");
    await waitFor(() => expect(heading()).toBe("page /broken"));
    await press("to home");
    await waitFor(() => expect(heading()).toBe("page /"));

    await press("to broken");

    await waitFor(() => expect(asked).toEqual(["/broken", "/", "/broken"]));
  });

  it("asks again for every route after refresh()", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    await press("refresh");
    await waitFor(() => expect(asked).toEqual(["/slow", "/", "/"]));
    await press("to slow");

    await waitFor(() => expect(asked).toEqual(["/slow", "/", "/", "/slow"]));
  });

  it("asks again for every route after a server action", async () => {
    installStaleTime(30);
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    const saved = { fetch: globals.fetch, location: globals.location };
    globals.fetch = async () => new Response(encodeActionResult({ saved: true }), { status: 200 });
    globals.location = globals.window.location;
    try {
      const save = createServerReference("0".repeat(64), "app/actions.js#save");
      expect(await save()).toEqual({ saved: true });
    } finally {
      globals.fetch = saved.fetch;
      globals.location = saved.location;
    }
    await press("to slow");

    await waitFor(() => expect(asked).toEqual(["/slow", "/", "/slow"]));
  });
});

describe("an application React Server Components render, with no stale time", () => {
  it("asks the server for every navigation, which is what it always did", async () => {
    const asked = countedFetches();
    await mountFlight("/");
    await visitSlowAndReturn();

    await press("to slow");

    await waitFor(() => expect(asked).toEqual(["/slow", "/", "/slow"]));
  });
});

describe("an application React Server Components render, with an intercepted payload", () => {
  function interceptedFetches(): Array<{| readonly url: string, readonly from: mixed |}> {
    const asked: Array<{| readonly url: string, readonly from: mixed |}> = [];
    installFlightFetch((url, options) => {
      const from = options?.interceptedFrom ?? null;
      asked.push({ url, from });
      return Promise.resolve({
        kind: "flight",
        url,
        root: Promise.resolve(rootFor(url, 200, from === "/" ? from : null)),
      });
    });
    return asked;
  }

  it("asks the server to render over the current page, and remembers that page", async () => {
    const asked = interceptedFetches();
    await mountFlight("/");

    await press("to slow");
    await waitFor(() => expect(heading()).toBe("page /slow"));

    expect(asked).toEqual([{ url: "/slow", from: "/" }]);
    expect(globals.window.history.state).toEqual({ "uf:intercepted-from": "/" });
  });

  it("uses the remembered page when back or forward enters an intercepted entry", async () => {
    const asked = interceptedFetches();
    await mountFlight("/");

    await act(async () => {
      globals.window.history.replaceState({ "uf:intercepted-from": "/" }, "", "/slow");
      globals.window.dispatchEvent(new globals.window.Event("popstate"));
    });

    await waitFor(() => expect(heading()).toBe("page /slow"));
    expect(asked).toEqual([{ url: "/slow", from: "/" }]);
  });

  it("forgets an intercepted marker kept across a document reload", async () => {
    installDom();
    globals.window.history.replaceState({ "uf:intercepted-from": "/" }, "", "/slow");
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/slow" flight={Promise.resolve(rootFor("/slow"))} />);
    });

    await waitFor(() => expect(globals.window.history.state).toBe(null));
  });
});

// ---------------------------------------------------------------------------
// An application rendered from its modules
// ---------------------------------------------------------------------------

/** How many times `/slow`'s loader has run. */
let loads = 0;

function moduleTable(): RouteTable {
  component Home() {
    const router = useRouter();
    return (
      <main>
        <h1>home</h1>
        <button type="button" onClick={() => void router.push("/slow")}>
          to slow
        </button>
        <button type="button" onClick={() => void router.prefetch("/slow")}>
          prefetch slow
        </button>
      </main>
    );
  }

  component Slow(data: mixed) {
    const router = useRouter();
    const answer: $FlowFixMe = data;
    return (
      <main>
        <h1>{`slow ${String(answer?.load)}`}</h1>
        <button type="button" onClick={() => void router.push("/")}>
          to home
        </button>
      </main>
    );
  }

  // New page loaders per table, so no module a previous test loaded is reused.
  const routes: $FlowFixMe = [
    {
      path: "/",
      params: [],
      mdx: false,
      file: "app/$page.js",
      page: () => Promise.resolve({ default: Home }),
      layouts: [],
      loading: [],
    },
    {
      path: "/slow",
      params: [],
      mdx: false,
      file: "app/slow/$page.js",
      page: () =>
        Promise.resolve({
          default: Slow,
          loader: () => {
            loads += 1;
            return { load: loads };
          },
        }),
      layouts: [],
      loading: [],
    },
  ];
  return { routes, notFound: [], errors: [] };
}

async function mountModules(): Promise<void> {
  installDom();
  loads = 0;
  const table = moduleTable();
  installRoutes(table);
  globals.window.history.replaceState(null, "", "/");
  const initial = await resolveMatch(table, "/");
  const App = routerView("./app");
  await act(async () => {
    render(<App url="/" initial={initial} />);
  });
}

describe("an application rendered from its modules", () => {
  it("runs a prefetched route's loader once, and not again for the click", async () => {
    installStaleTime(30);
    await mountModules();

    await press("prefetch slow");
    await waitFor(() => expect(loads).toBe(1));
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("slow 1"));

    expect(loads).toBe(1);
  });

  it("runs the loader again for a visit once what it kept is stale", async () => {
    installStaleTime(30);
    await mountModules();
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("slow 1"));
    await press("to home");
    await waitFor(() => expect(heading()).toBe("home"));

    await press("to slow");
    await waitFor(() => expect(heading()).toBe("slow 1"));
    await press("to home");
    await waitFor(() => expect(heading()).toBe("home"));
    now += 30_000;
    await press("to slow");

    await waitFor(() => expect(heading()).toBe("slow 2"));
  });

  it("loads only the modules on a prefetch when no stale time is set", async () => {
    await mountModules();

    await press("prefetch slow");
    await press("to slow");
    await waitFor(() => expect(heading()).toBe("slow 1"));
    await press("to home");
    await waitFor(() => expect(heading()).toBe("home"));
    await press("to slow");

    await waitFor(() => expect(heading()).toBe("slow 2"));
  });
});

// ---------------------------------------------------------------------------
// The client entries
// ---------------------------------------------------------------------------

describe("the client entries @uniflowed/vite writes", () => {
  it("say nothing about a stale time a project did not set", () => {
    expect(clientModuleSource("/app.js", {})).toContain(
      "hydrate({ App, routes, notFound, errors });",
    );
    expect(flightClientSource("/app.js", { staleTime: 0 })).toContain("hydrateFlight({ App });");
  });

  it("write a stale time in as a constant", () => {
    expect(clientModuleSource("/app.js", { staleTime: 30 })).toContain(
      "hydrate({ App, routes, notFound, errors, staleTime: 30 });",
    );
    expect(flightClientSource("/app.js", { staleTime: 30 })).toContain(
      "hydrateFlight({ App, staleTime: 30 });",
    );
  });
});
