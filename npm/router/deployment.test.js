// @flow
//
// A tab opened on build N, after build N+1 is what answers it.
//
// `./internal/deployment.js` is the browser's half of version-skew protection,
// and each of its answers is driven here against the thing it protects: the
// document names its build, an action call and a payload request carry it, a
// refusal from another build becomes a hard navigation, a payload another build
// rendered is not rendered, and a chunk that is gone becomes a document load
// rather than an error page. The server's half — the `409` itself — is
// `npm/server/deployment.test.js`, and the two builds end to end, through
// `uf build` twice and `uf start`, are
// `a_tab_on_the_previous_build_keeps_working_or_loads_the_document_again` in
// `crates/uf_cli/tests/vite.rs`.

import * as React from "@uniflowed/react";
import { act, cleanup, render, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../npm/react-testing/internal/dom.js";
import { ServerActionError, createServerReference } from "./action.js";
import { encodeActionResult } from "./internal/action-wire.js";
import {
  DEPLOYMENT_HEADER,
  currentDeployment,
  failedOnAMissingChunk,
  forgetDeployment,
  fromAnotherDeployment,
  isChunkLoadFailure,
  rememberDeployment,
} from "./internal/deployment.js";
import { fetchFlight } from "./internal/flight-browser.js";
import type { FlightRoot } from "./internal/flight.js";
import {
  installFlightFetch,
  installNavigation,
  installRoutes,
  resolveMatch,
  routerView,
  useRoute,
  useRouter,
} from "./internal/runtime.js";
import { shellFor } from "./internal/shell.js";

const globals: $FlowFixMe = globalThis;

/** Puts back what [`pageAt`] replaced, when a test replaced anything. */
let restore: null | (() => void) = null;

afterEach(() => {
  restore?.();
  restore = null;
  if (globals.document != null) {
    cleanup();
    globals.document.body.replaceChildren();
    globals.document.head.querySelector('meta[name="uf:deployment"]')?.remove();
    globals.window.history.replaceState(null, "", "/");
  }
  uft.restoreAllMocks();
  installNavigation("client");
  forgetDeployment();
});

/** A document whose head says it is build `id`. */
function documentOf(id: string | null) {
  return {
    querySelector: (selector: string) =>
      id != null && selector === 'meta[name="uf:deployment"]'
        ? { getAttribute: (name: string) => (name === "content" ? id : null) }
        : null,
  };
}

/**
 * A page at `href`, on build `id`, whose `fetch` is `answer` and whose
 * `location.assign` records where it was sent.
 */
function pageAt(
  href: string,
  id: string | null,
  answer: (input?: mixed, init?: $FlowFixMe) => Promise<Response>,
): Array<string> {
  const loaded: Array<string> = [];
  const url = new URL(href);
  const location = {
    href: url.href,
    origin: url.origin,
    pathname: url.pathname,
    search: url.search,
    assign: (target: string) => {
      loaded.push(target);
    },
  };
  const previous = { fetch: globals.fetch, location: globals.location, window: globals.window };
  restore = () => {
    globals.fetch = previous.fetch;
    globals.location = previous.location;
    globals.window = previous.window;
  };
  globals.window = { location };
  globals.location = location;
  globals.fetch = answer;
  rememberDeployment(documentOf(id));
  return loaded;
}

/** Whether `promise` is still pending once everything already queued has run. */
async function stillPending(promise: Promise<mixed>): Promise<boolean> {
  const pending = {};
  const first = await Promise.race([
    promise,
    new Promise((resolve) => setTimeout(resolve, 20, pending)),
  ]);
  return first === pending;
}

describe("the document", () => {
  it("names its build in the head, before every URL that build wrote", () => {
    const shell = shellFor({
      scripts: ["/assets/client-abc.js"],
      styles: [],
      preloads: [],
      deployment: "3f9a1c0d2e4b5a67",
    });

    expect(shell.head.startsWith('<meta name="uf:deployment" content="3f9a1c0d2e4b5a67">')).toBe(
      true,
    );
  });

  it("names nothing where there is no build to be skewed against", () => {
    // `uf dev`, and a build recorded before the id existed.
    const shell = shellFor({ scripts: ["/src/client.js"], styles: [], preloads: [] });
    expect(shell.head).not.toContain("uf:deployment");
  });
});

describe("a server action called from a tab on the previous build", () => {
  it("says which build it is on", async () => {
    let sent = new Headers();
    pageAt("http://uf.test/counter", "build-n", async (_input, init) => {
      sent = new Headers(init?.headers);
      return new Response(encodeActionResult({ total: 9 }), { status: 200 });
    });

    const record = createServerReference("a".repeat(64), "app/actions.js#record");
    expect(await record(4)).toEqual({ total: 9 });

    expect(sent.get(DEPLOYMENT_HEADER)).toBe("build-n");
  });

  it("loads the page again instead of settling when the server is on another build", async () => {
    const loaded = pageAt("http://uf.test/counter?tab=1", "build-n", async () => {
      return new Response("409 Conflict\n", {
        status: 409,
        headers: { [DEPLOYMENT_HEADER]: "build-n1" },
      });
    });

    const record = createServerReference("a".repeat(64), "app/actions.js#record");
    const call = record(4);

    // Neither a result nor a rejection: the page is being replaced, and an
    // error boundary flashing up for the moment before it is would be a lie.
    expect(await stillPending(call)).toBe(true);
    expect(loaded).toEqual(["/counter?tab=1"]);
  });

  it("treats an application's own 409 as the failure it is", async () => {
    // No `uf-deployment` on the answer, so it is not a front door refusing a
    // build — it is whatever the action decided, and it throws as any status
    // that is not a success does.
    const loaded = pageAt("http://uf.test/counter", "build-n", async () => {
      return new Response("conflict", { status: 409 });
    });

    const record = createServerReference("a".repeat(64), "app/actions.js#record");
    const failure = await record(4).then(
      () => null,
      (error: mixed) => error,
    );

    expect(failure instanceof ServerActionError).toBe(true);
    expect(loaded).toEqual([]);
  });

  it("sends no build from a page that names none", async () => {
    let sent = new Headers();
    pageAt("http://uf.test/counter", null, async (_input, init) => {
      sent = new Headers(init?.headers);
      return new Response(encodeActionResult(null), { status: 200 });
    });

    await createServerReference("a".repeat(64), "app/actions.js#record")();

    expect(sent.has(DEPLOYMENT_HEADER)).toBe(false);
  });
});

describe("a navigation's payload, from a tab on the previous build", () => {
  it("says which build it is on, and loads the document when it is refused", async () => {
    let sent = new Headers();
    pageAt("http://uf.test/feed", "build-n", async (_input, init) => {
      sent = new Headers(init?.headers);
      const headers = new Headers({ "content-type": "text/plain; charset=utf-8" });
      headers.set(DEPLOYMENT_HEADER, "build-n1");
      const refused = new Response("409 Conflict\n", { status: 409, headers });
      Object.defineProperty(refused, "url", { value: "http://uf.test/account/__uf.flight" });
      return refused;
    });

    expect(await fetchFlight("/account")).toEqual({ kind: "document", url: "/account" });
    expect(sent.get(DEPLOYMENT_HEADER)).toBe("build-n");
  });
});

describe("what counts as another build", () => {
  it("compares only when both the page and the payload say", () => {
    rememberDeployment(documentOf("build-n"));
    expect(currentDeployment()).toBe("build-n");
    expect(fromAnotherDeployment("build-n1")).toBe(true);
    expect(fromAnotherDeployment("build-n")).toBe(false);
    expect(fromAnotherDeployment(null)).toBe(false);

    rememberDeployment(documentOf(null));
    expect(fromAnotherDeployment("build-n1")).toBe(false);
  });

  it("tells a chunk that is gone from a module that threw", () => {
    for (const message of [
      "Failed to fetch dynamically imported module: http://uf.test/assets/page-abc.js",
      "error loading dynamically imported module: http://uf.test/assets/page-abc.js",
      "Importing a module script failed.",
      "Unable to preload CSS for /assets/page-abc.css",
    ]) {
      expect(isChunkLoadFailure(new TypeError(message))).toBe(true);
    }
    expect(isChunkLoadFailure(new TypeError("cannot read properties of undefined"))).toBe(false);
    expect(isChunkLoadFailure("Failed to fetch dynamically imported module")).toBe(false);
  });

  it("reads a route's resolution error the same way, and only when the route threw", () => {
    const gone = new TypeError("Importing a module script failed.");
    expect(failedOnAMissingChunk({ kind: "thrown", error: gone })).toBe(true);
    expect(failedOnAMissingChunk({ kind: "thrown", error: new Error("boom") })).toBe(false);
    expect(failedOnAMissingChunk({ kind: "forbidden", error: gone })).toBe(false);
    expect(failedOnAMissingChunk(null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The routers
// ---------------------------------------------------------------------------

component Screen() {
  const router = useRouter();
  const route = useRoute();
  return (
    <main>
      <h1>{`page ${route.pathname}`}</h1>
      <button type="button" onClick={() => void router.push("/next")}>
        to next
      </button>
    </main>
  );
}

/** The payload root build `deployment` would have rendered for `url`. */
function rootFor(url: string, deployment: string | null): FlightRoot {
  const route: $FlowFixMe = {
    pathname: url,
    search: "",
    path: url,
    params: {},
    searchParams: {},
    data: undefined,
    deferred: null,
    metadata: {},
    viewTransition: null,
    status: 200,
    error: null,
    interception: null,
  };
  return { route, tree: <Screen />, deployment };
}

/** Mount a page on build `id` at `/`, and record the documents it loads. */
function onBuild(id: string): Array<string> {
  installDom();
  const meta = globals.document.createElement("meta");
  meta.setAttribute("name", "uf:deployment");
  meta.setAttribute("content", id);
  globals.document.head.append(meta);
  forgetDeployment();
  globals.window.history.replaceState(null, "", "/");
  const loaded: Array<string> = [];
  uft.spyOn(globals.window.location, "assign").mockImplementation((href: string) => {
    loaded.push(String(href));
  });
  return loaded;
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

describe("a page React Server Components rendered, on the previous build", () => {
  async function mount(id: string): Promise<Array<string>> {
    const loaded = onBuild(id);
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/" flight={Promise.resolve(rootFor("/", id))} />);
    });
    return loaded;
  }

  it("renders a payload its own build rendered", async () => {
    installFlightFetch((url) =>
      Promise.resolve({ kind: "flight", url, root: Promise.resolve(rootFor(url, "build-n")) }),
    );
    const loaded = await mount("build-n");

    await press("to next");

    await waitFor(() => expect(heading()).toBe("page /next"));
    expect(loaded).toEqual([]);
  });

  it("loads the document instead of rendering a payload another build prerendered", async () => {
    // A prerendered payload is a file, served by a host that refused nothing,
    // so the only thing that can tell is the payload.
    installFlightFetch((url) =>
      Promise.resolve({ kind: "flight", url, root: Promise.resolve(rootFor(url, "build-n1")) }),
    );
    const loaded = await mount("build-n");

    await press("to next");

    await waitFor(() => expect(loaded).toEqual(["http://localhost/next"]));
    expect(heading()).toBe("page /");
  });

  it("loads the document when a client module the payload names is gone", async () => {
    installFlightFetch((url) =>
      Promise.resolve({
        kind: "flight",
        url,
        root: Promise.reject<FlightRoot>(
          new TypeError("Failed to fetch dynamically imported module: /assets/Counter-abc.js"),
        ),
      }),
    );
    const loaded = await mount("build-n");

    await press("to next");

    await waitFor(() => expect(loaded).toEqual(["http://localhost/next"]));
  });
});

describe("a page rendered from its modules, on the previous build", () => {
  component Home() {
    const router = useRouter();
    return (
      <main>
        <h1>home</h1>
        <button type="button" onClick={() => void router.push("/next")}>
          to next
        </button>
      </main>
    );
  }

  async function mount(page: () => Promise<mixed>): Promise<Array<string>> {
    const loaded = onBuild("build-n");
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
        path: "/next",
        params: [],
        mdx: false,
        file: "app/next/$page.js",
        page,
        layouts: [],
        loading: [],
      },
    ];
    const table = { routes, notFound: [], errors: [] };
    installRoutes(table);
    const initial = await resolveMatch(table, "/");
    const App = routerView("./app");
    await act(async () => {
      render(<App url="/" initial={initial} />);
    });
    return loaded;
  }

  it("loads the document when the next route's chunk is gone", async () => {
    const loaded = await mount(() =>
      Promise.reject(
        new TypeError("Failed to fetch dynamically imported module: /assets/next-abc.js"),
      ),
    );

    await press("to next");

    await waitFor(() => expect(loaded).toEqual(["http://localhost/next"]));
    expect(heading()).toBe("home");
  });

  it("shows the error boundary for a module that threw, which a reload would not fix", async () => {
    const loaded = await mount(() => Promise.reject(new Error("the page module is broken")));

    await press("to next");

    await waitFor(() => expect(heading()).not.toBe("home"));
    expect(loaded).toEqual([]);
  });
});
