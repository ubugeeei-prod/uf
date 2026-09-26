// @flow
//
// `notFound()` in a hydrated server action shows the not-found page.
//
// After ubugeeei-prod/uf#1475 a reference whose action called `notFound()`
// throws `NotFoundError` in the browser, and React hands that to the route's
// error boundary. A loader's `notFound()` shows the project's not-found page
// because the resolver resolves the route again, not because a boundary
// catches it, so the boundary used to take the error for a crash and show the
// error view. What is held here is that it shows the page a loader's
// `notFound()` would have, both from a route resolved in the browser and from
// one React Server Components rendered, and that it still shows the error view
// when there is no not-found page to be had. See ubugeeei-prod/uf#1489.

import * as React from "@uniflowed/react";
import { act, cleanup, render, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../npm/react-testing/internal/dom.js";
import { createServerReference } from "./action.js";
import { ACTION_OUTCOME_HEADER } from "./internal/action-wire.js";
import { RouteErrorBoundary } from "./internal/error-view.js";
import type { FlightFetchOptions, FlightRoot } from "./internal/flight.js";
import {
  installFlightFetch,
  installNavigation,
  installRoutes,
  resolveMatch,
  routerView,
} from "./internal/runtime.js";

const globals: $FlowFixMe = globalThis;
const ID = "a1b2c3d4e5f6".repeat(6).slice(0, 64);

afterEach(() => {
  if (globals.document != null) {
    cleanup();
    globals.document.body.replaceChildren();
    globals.window.history.replaceState(null, "", "/");
  }
  uft.restoreAllMocks();
  installNavigation("client");
});

/** A page at `/notes/7` whose every action call is answered `notFound()`. */
function onNotePage(): void {
  installDom();
  globals.window.history.replaceState(null, "", "/notes/7");
  uft.spyOn(globals, "fetch").mockImplementation(
    async () =>
      new Response(JSON.stringify({ outcome: "not-found" }), {
        status: 404,
        headers: { [ACTION_OUTCOME_HEADER]: "not-found" },
      }),
  );
}

const remove = createServerReference(ID, "app/notes/_actions.js#remove");

/**
 * The note, with a form whose action is the reference. `useActionState`,
 * because it is the component that throws an action's error again on every
 * render until it is remounted, which is the case the boundary has to survive.
 */
component Note() {
  const [, dispatch, pending] = React.useActionState(async () => {
    await remove();
    return null;
  }, null);
  return (
    <main>
      <h1>note seven</h1>
      <button type="button" disabled={pending} onClick={() => React.startTransition(dispatch)}>
        delete
      </button>
    </main>
  );
}

component NoteMissing() {
  return <h1>no such note</h1>;
}

component NotesError() {
  return <h1>the notes broke</h1>;
}

function heading(): string {
  return globals.document.querySelector("h1")?.textContent ?? "(no heading)";
}

async function pressDelete(): Promise<void> {
  // The action's error reaches the boundary through React, which reports it.
  uft.spyOn(console, "error").mockImplementation(() => {});
  await act(async () => {
    await userEvent.click(globals.document.querySelector("button"));
  });
}

describe("notFound() in a hydrated action, on a page rendered from its modules", () => {
  async function mount(notFound: $ReadOnlyArray<mixed>): Promise<void> {
    onNotePage();
    const table: $FlowFixMe = {
      routes: [
        {
          path: "/notes/7",
          params: [],
          mdx: false,
          file: "app/notes/7/$page.js",
          page: () => Promise.resolve({ default: Note }),
          layouts: [],
          loading: [],
        },
      ],
      notFound,
      errors: [
        {
          path: "/notes",
          file: "app/notes/$error.js",
          module: () => Promise.resolve({ default: NotesError }),
          layouts: [],
        },
      ],
    };
    installRoutes(table);
    const initial = await resolveMatch(table, "/notes/7");
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/notes/7" initial={initial} />);
    });
  }

  it("shows the nearest not-found page for the URL, not the error view", async () => {
    await mount([
      {
        path: "/notes",
        mdx: false,
        file: "app/notes/$not-found.js",
        page: () => Promise.resolve({ default: NoteMissing }),
        layouts: [],
      },
    ]);
    expect(heading()).toBe("note seven");

    await pressDelete();

    await waitFor(() => expect(heading()).toBe("no such note"));
    // The URL is still the one the visitor is on, as with a loader's notFound().
    expect(globals.window.location.pathname).toBe("/notes/7");
    expect(globals.document.body.textContent).not.toContain("the notes broke");
  });

  it("shows the framework's not-found page where the project declares none", async () => {
    await mount([]);

    await pressDelete();

    await waitFor(() => expect(heading()).toBe("404"));
    expect(globals.document.body.textContent).not.toContain("the notes broke");
  });
});

/** The payload root for `/notes/7`: its tree inside a route boundary, as the server composes it. */
function rootOf(tree: React.Node, status: 200 | 404): FlightRoot {
  const route: $FlowFixMe = {
    pathname: "/notes/7",
    search: "",
    path: status === 404 ? "*" : "/notes/7",
    params: {},
    searchParams: {},
    data: undefined,
    deferred: null,
    metadata: {},
    viewTransition: null,
    status,
    error: null,
    interception: null,
  };
  return {
    route,
    tree: (
      <RouteErrorBoundary module={{ default: NotesError }} resetKey="/notes/7">
        {tree}
      </RouteErrorBoundary>
    ),
    deployment: null,
  };
}

describe("notFound() in a hydrated action, on a page React Server Components rendered", () => {
  async function mount(answer: FlightRoot): Promise<Array<[string, ?FlightFetchOptions]>> {
    onNotePage();
    const asked: Array<[string, ?FlightFetchOptions]> = [];
    installFlightFetch((url, options) => {
      asked.push([url, options]);
      return Promise.resolve({ kind: "flight", url, root: Promise.resolve(answer) });
    });
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/notes/7" flight={Promise.resolve(rootOf(<Note />, 200))} />);
    });
    return asked;
  }

  it("asks the server for the URL's not-found payload and shows it", async () => {
    const asked = await mount(rootOf(<NoteMissing />, 404));
    expect(heading()).toBe("note seven");

    await pressDelete();

    await waitFor(() => expect(heading()).toBe("no such note"));
    expect(asked).toEqual([["/notes/7", { notFound: true }]]);
    expect(globals.window.location.pathname).toBe("/notes/7");
    expect(globals.document.body.textContent).not.toContain("the notes broke");
  });

  it("shows the error view when the answer is not a not-found page", async () => {
    // A host that ignored the header and answered the route itself.
    await mount(rootOf(<h1>note seven, again</h1>, 200));

    await pressDelete();

    await waitFor(() => expect(heading()).toBe("the notes broke"));
  });
});
