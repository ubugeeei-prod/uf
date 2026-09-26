// @flow
//
// Intercepting routes: a client navigation that renders the route it reaches
// in a slot of the page it started on.
//
// `app/feed/@modal/(.)photo/[id]/$page.js` is the shape. A reader on `/feed`
// follows a link to `/feed/photo/1`: the address bar says `/feed/photo/1`, the
// feed stays exactly where it was, and the photo opens in the `modal` slot of
// `app/feed/$layout.js`. A reader who reloads that URL, or who was sent it,
// gets `app/feed/photo/[id]/$page.js` instead — the page the URL names —
// because a request carries where it is going and not what was on screen when
// it was made. See ubugeeei-prod/uf#267.
//
// The table is written by hand, the way `tests/library/parallel-routes.test.js`
// writes its own, because this file is about the runtime: what a navigation
// renders, what its history entry remembers, and what back and forward put
// back. There is a section for each.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { act, cleanup, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../npm/react-testing/internal/dom.js";
import type { RouteTable } from "./internal/runtime.js";
import {
  Link,
  RouteView,
  RouterProvider,
  installRoutes,
  resolveMatch,
  routerView,
  useRouter,
} from "./internal/runtime.js";
import { createRenderer } from "./server.js";

afterEach(() => {
  cleanup();
});

// --- The table ---------------------------------------------------------

component FeedFrame(children: React.Node, modal: React.Node) {
  return (
    <div>
      <main>{children}</main>
      <aside data-testid="modal">{modal ?? "nothing in the modal"}</aside>
    </div>
  );
}

component Feed() {
  // State the reader built up on the page underneath. An interception that
  // remounted the feed would lose it, and that is the failure this is here to
  // catch.
  const [likes, setLikes] = useState<number>(0);
  return (
    <section>
      <h1>the feed</h1>
      <button type="button" onClick={() => setLikes(likes + 1)}>
        like
      </button>
      <output data-testid="likes">{likes}</output>
      <Link to="/feed/photo/1" prefetch="off">
        photo one
      </Link>
    </section>
  );
}

component PhotoPage(params: { readonly id: string }) {
  return <h1>{`the photo page for ${params.id}`}</h1>;
}

component PhotoModal(params: { readonly id: string }) {
  return <p>{`photo ${params.id}, in the modal`}</p>;
}

component Settings() {
  return <h1>the settings</h1>;
}

component About() {
  return <h1>about</h1>;
}

const loadFrame = () => Promise.resolve({ default: FeedFrame });
const idParam = [{ name: "id", catchAll: false }];

/**
 * `app/feed/@modal/`: no page and no default of its own, so it holds nothing
 * until a navigation intercepts — which is the shape of every modal slot.
 */
const modal = {
  name: "modal",
  above: 1,
  defaultPage: null,
  routes: [],
  intercepts: [
    {
      // `(.)photo/[id]`: the segment's own level.
      path: "/feed/photo/:id",
      params: idParam,
      mdx: false,
      file: "app/feed/@modal/(.)photo/[id]/$page.js",
      page: () => Promise.resolve({ default: PhotoModal }),
      layouts: [],
      slots: [],
    },
    {
      // `(..)photo/[id]`: one level above the segment that declares the slot.
      path: "/photo/:id",
      params: idParam,
      mdx: false,
      file: "app/feed/@modal/(..)photo/[id]/$page.js",
      page: () => Promise.resolve({ default: PhotoModal }),
      layouts: [],
      slots: [],
    },
  ],
};

const table: RouteTable = {
  routes: [
    {
      path: "/feed",
      params: [],
      mdx: false,
      file: "app/feed/$page.js",
      page: () => Promise.resolve({ default: Feed }),
      layouts: [loadFrame],
      loading: [],
      templates: [],
      slots: [modal],
    },
    {
      // The page a hard load of `/feed/photo/1` renders. It is under the feed's
      // layout too, so it has the slot — empty, because nothing intercepted.
      path: "/feed/photo/:id",
      params: idParam,
      mdx: false,
      file: "app/feed/photo/[id]/$page.js",
      page: () => Promise.resolve({ default: PhotoPage }),
      layouts: [loadFrame],
      loading: [],
      templates: [],
      slots: [modal],
    },
    {
      path: "/feed/settings",
      params: [],
      mdx: false,
      file: "app/feed/settings/$page.js",
      page: () => Promise.resolve({ default: Settings }),
      layouts: [loadFrame],
      loading: [],
      templates: [],
      slots: [modal],
    },
    {
      path: "/photo/:id",
      params: idParam,
      mdx: false,
      file: "app/photo/[id]/$page.js",
      page: () => Promise.resolve({ default: PhotoPage }),
      layouts: [],
      loading: [],
      templates: [],
      slots: [],
    },
    {
      path: "/about",
      params: [],
      mdx: false,
      file: "app/about/$page.js",
      page: () => Promise.resolve({ default: About }),
      layouts: [],
      loading: [],
      templates: [],
      slots: [],
    },
  ],
  notFound: [],
  errors: [],
};

const assets = { scripts: [], styles: [], preloads: [] };

// --- Driving it --------------------------------------------------------

component Go(to: string) {
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() => {
        // `scroll: false` because this is about what is mounted, not about
        // where the window is.
        router.push(to, { scroll: false }).catch(() => {});
      }}
    >
      {`go to ${to}`}
    </button>
  );
}

component Refresh() {
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() => {
        router.refresh().catch(() => {});
      }}
    >
      refresh
    </button>
  );
}

component Prefetch(to: string) {
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() => {
        router.prefetch(to).catch(() => {});
      }}
    >
      {`prefetch ${to}`}
    </button>
  );
}

/**
 * Arrive at `url` the way a document request does: resolved for that URL with
 * nothing intercepted, and a history entry of its own.
 */
async function arrive(
  url: string,
  controls?: React.Node,
  routes?: RouteTable = table,
): Promise<void> {
  installDom();
  installRoutes(routes);
  globalThis.window.history.pushState(null, "", url);
  const initial = await resolveMatch(routes, url);
  render(
    <RouterProvider url={url} initial={initial}>
      <RouteView />
      {controls}
    </RouterProvider>,
  );
}

/** Click something a query found, which is always an element of the page. */
async function press(element: Element): Promise<void> {
  if (!(element instanceof HTMLElement)) {
    throw new Error(`expected an HTML element, got ${element.nodeName}`);
  }
  await userEvent.click(element);
}

function inModal(): string {
  return screen.getByTestId("modal").textContent;
}

/** The browser's back or forward button, and the render it leads to. */
async function travel(direction: "back" | "forward"): Promise<void> {
  await act(async () => {
    if (direction === "back") {
      globalThis.window.history.back();
    } else {
      globalThis.window.history.forward();
    }
    // The document dispatches `popstate` after the call returns, and the router
    // resolves the entry's route before it commits it.
    await new Promise((resolve) => setTimeout(resolve, 30));
  });
}

// --- What a document request gets --------------------------------------

describe("a document request for an intercepted URL", () => {
  it("renders the page the URL names, with nothing in the slot", async () => {
    // A reload, a shared link and a crawler: none of them was on the feed, so
    // none of them is intercepted. The server never reads `intercepts`.
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: table.routes,
      notFound: table.notFound,
      errors: table.errors,
    });

    const result = await prerender("/feed/photo/1", assets);

    expect(result.html).toContain("the photo page for 1");
    expect(result.html).toContain("nothing in the modal");
    expect(result.html).not.toContain("photo 1, in the modal");
  });
});

// --- What a client navigation renders ----------------------------------

describe("a client navigation from a page the slot is on", () => {
  it("renders the interception in the slot and leaves the page underneath where it was", async () => {
    await arrive("/feed");
    await press(screen.getByRole("button", { name: "like" }));
    expect(screen.getByTestId("likes").textContent).toBe("1");

    await press(screen.getByRole("link", { name: "photo one" }));

    expect(inModal()).toContain("photo 1, in the modal");
    // The feed was never unmounted: the like it was holding is still there,
    // and the page the URL names is not what `children` renders.
    expect(screen.getByText("the feed")).not.toBe(null);
    expect(screen.getByTestId("likes").textContent).toBe("1");
    expect(screen.queryByText("the photo page for 1")).toBe(null);
    // And the address bar says where the reader went.
    expect(globalThis.window.location.pathname).toBe("/feed/photo/1");
  });

  it("intercepts a URL above the segment that declares the slot", async () => {
    // `(..)photo`: the slot is on `/feed` and the URL is not under it. What
    // decides is that the page the navigation started on renders the slot.
    await arrive("/feed", <Go to="/photo/3" />);

    await press(screen.getByRole("button", { name: "go to /photo/3" }));

    expect(inModal()).toContain("photo 3, in the modal");
    expect(screen.getByText("the feed")).not.toBe(null);
    expect(globalThis.window.location.pathname).toBe("/photo/3");
  });

  it("intercepts again from inside the interception, over the same page", async () => {
    // The next photo, from a photo already open in the modal. The page
    // underneath is still the feed — not a feed with a modal in it — so the
    // second interception is from the feed too.
    await arrive("/feed", <Go to="/feed/photo/2" />);
    await press(screen.getByRole("link", { name: "photo one" }));

    await press(screen.getByRole("button", { name: "go to /feed/photo/2" }));

    expect(inModal()).toContain("photo 2, in the modal");
    expect(screen.getByText("the feed")).not.toBe(null);
    expect(globalThis.window.history.state).toEqual({ "uf:intercepted-from": "/feed" });
  });

  it("closes when a navigation goes somewhere nothing intercepts", async () => {
    await arrive("/feed", <Go to="/feed/settings" />);
    await press(screen.getByRole("link", { name: "photo one" }));
    expect(inModal()).toContain("photo 1, in the modal");

    await press(screen.getByRole("button", { name: "go to /feed/settings" }));

    // An ordinary navigation: the page the URL names, and every slot matched
    // against it again — this one to nothing.
    expect(screen.getByText("the settings")).not.toBe(null);
    expect(inModal()).toContain("nothing in the modal");
    expect(globalThis.window.history.state).toBe(null);
  });

  it("is the error page when the intercepting page exports a loader", async () => {
    // A slot page's loader does not run, and one that exports it says so
    // rather than opening a modal with no data in it — for an intercepting
    // page exactly as for any other slot page.
    const withLoader: RouteTable = {
      ...table,
      routes: table.routes.map((route) =>
        route.path !== "/feed"
          ? route
          : {
              ...route,
              slots: [
                {
                  ...modal,
                  intercepts: [
                    {
                      ...modal.intercepts[0],
                      page: () => Promise.resolve({ default: PhotoModal, loader: () => ({}) }),
                    },
                  ],
                },
              ],
            },
      ),
    };
    await arrive("/feed", <Go to="/feed/photo/1" />, withLoader);

    await press(screen.getByRole("button", { name: "go to /feed/photo/1" }));

    expect(screen.getByText("Something went wrong")).not.toBe(null);
  });

  it("prefetches the intercepting page, not the page the URL names", async () => {
    // What the next render will need. From the feed that is the modal's page;
    // loading the photo page would be bytes for a render that is not coming.
    const asked: Array<string> = [];
    const intercepting = {
      ...modal,
      intercepts: [
        {
          ...modal.intercepts[0],
          page: () => {
            asked.push("the modal's page");
            return Promise.resolve({ default: PhotoModal });
          },
        },
      ],
    };
    const counted: RouteTable = {
      ...table,
      routes: table.routes.map((route) => {
        if (route.path === "/feed") {
          return { ...route, slots: [intercepting] };
        }
        if (route.path === "/feed/photo/:id") {
          return {
            ...route,
            page: () => {
              asked.push("the photo page");
              return Promise.resolve({ default: PhotoPage });
            },
          };
        }
        return route;
      }),
    };
    await arrive("/feed", <Prefetch to="/feed/photo/1" />, counted);

    await press(screen.getByRole("button", { name: "prefetch /feed/photo/1" }));

    await waitFor(() => {
      expect(asked).toEqual(["the modal's page"]);
    });
  });
});

describe("a client navigation from a page the slot is not on", () => {
  it("is an ordinary navigation to the page the URL names", async () => {
    // `/about` renders no feed layout, so there is no `modal` slot on screen
    // to render into, and nothing to intercept with.
    await arrive("/about", <Go to="/feed/photo/1" />);

    await press(screen.getByRole("button", { name: "go to /feed/photo/1" }));

    expect(screen.getByText("the photo page for 1")).not.toBe(null);
    expect(inModal()).toContain("nothing in the modal");
    expect(globalThis.window.history.state).toBe(null);
  });
});

// --- What the history entry remembers ----------------------------------

describe("the history entry an interception writes", () => {
  it("says where the navigation came from, and nothing else", async () => {
    await arrive("/feed");

    await press(screen.getByRole("link", { name: "photo one" }));

    // A URL, because the state survives a reload and is structured-cloned:
    // nothing with a module in it could be kept there.
    expect(globalThis.window.history.state).toEqual({ "uf:intercepted-from": "/feed" });
  });

  it("puts the page underneath back on back, and the interception on forward", async () => {
    await arrive("/feed");
    await press(screen.getByRole("button", { name: "like" }));
    await press(screen.getByRole("link", { name: "photo one" }));
    expect(inModal()).toContain("photo 1, in the modal");

    await travel("back");

    await waitFor(() => {
      expect(inModal()).toContain("nothing in the modal");
    });
    expect(globalThis.window.location.pathname).toBe("/feed");
    // The feed never went anywhere, so neither did its state.
    expect(screen.getByTestId("likes").textContent).toBe("1");

    await travel("forward");

    await waitFor(() => {
      expect(inModal()).toContain("photo 1, in the modal");
    });
    expect(globalThis.window.location.pathname).toBe("/feed/photo/1");
    expect(screen.getByText("the feed")).not.toBe(null);
    expect(screen.getByTestId("likes").textContent).toBe("1");
  });

  it("goes back from the second photo to the first without leaving the feed", async () => {
    await arrive("/feed", <Go to="/feed/photo/2" />);
    await press(screen.getByRole("button", { name: "like" }));
    await press(screen.getByRole("link", { name: "photo one" }));
    await press(screen.getByRole("button", { name: "go to /feed/photo/2" }));
    expect(inModal()).toContain("photo 2, in the modal");

    await travel("back");

    await waitFor(() => {
      expect(inModal()).toContain("photo 1, in the modal");
    });
    expect(screen.getByTestId("likes").textContent).toBe("1");
  });

  it("keeps the interception through a refresh", async () => {
    // A refresh is the same thing on screen, resolved again. Resolving only the
    // address bar's URL would close the modal, which is a navigation nobody
    // asked for.
    await arrive("/feed", <Refresh />);
    await press(screen.getByRole("link", { name: "photo one" }));

    await press(screen.getByRole("button", { name: "refresh" }));

    await waitFor(() => {
      expect(inModal()).toContain("photo 1, in the modal");
    });
    expect(screen.getByText("the feed")).not.toBe(null);
  });

  it("is forgotten by a reload, which renders the page the URL names", async () => {
    // The browser keeps `history.state` across a reload, and the document it
    // reloads is the ordinary page. Leaving the mark would make back-then-
    // forward open a modal over a page the reader never saw it on.
    installDom();
    installRoutes(table);
    globalThis.window.history.pushState({ "uf:intercepted-from": "/feed" }, "", "/feed/photo/1");
    const initial = await resolveMatch(table, "/feed/photo/1");

    render(
      <RouterProvider url="/feed/photo/1" initial={initial}>
        <RouteView />
      </RouterProvider>,
    );

    expect(screen.getByText("the photo page for 1")).not.toBe(null);
    expect(inModal()).toContain("nothing in the modal");
    await waitFor(() => {
      expect(globalThis.window.history.state).toBe(null);
    });
  });
});
