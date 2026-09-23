// @flow
//
// `app.router.basePath` and `app.router.trailingSlash`, as the router reads
// them.
//
// The route table has no base in it — `/guide` is `/guide` whether the site is
// served at `/` or at `/docs` — and the address bar has one.
// `./internal/base-path.js` is where those two spellings meet, and this file
// asks the three places the meeting shows.
//
// **The spelling.** An application path becomes an address, and an address
// becomes an application path again, by one rule. `@uniflowed/server`'s
// `internal/routing.js` spells the same rule for every front door, and
// `packages/server/routing.test.js` holds the two to one answer; what is here
// is the router's half on its own.
//
// **The generated entries.** `@uniflowed/vite` writes the base and the policy
// into `virtual:uf/client` as constants, and nothing at all for a project at
// the root with the default policy, so that project's entry stays the module
// it always was.
//
// **The runtime.** A `Link` writes the address, so a link followed before
// hydration, or opened in a new tab, goes where a click goes. A navigation asks
// the table about the application path and writes the address into history,
// and hydration reads the address back.
//
// The runtime half needs a document, so `@uniflowed/router/client` is imported
// after `installDom`, for the reason `navigation.test.js` gives.

import * as React from "@uniflowed/react";
import { act, cleanup, userEvent, waitFor } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { flightClientSource } from "../../packages/vite/internal/flight.js";
import {
  clientModuleSource,
  routingRulesOf,
  serverModuleSource,
} from "../../packages/vite/internal/routes.js";
// By path, for the reason `navigation.test.js` gives for `installNavigation`:
// the settings are module state, and the reset below has to reach the instance
// the runtime reads.
import {
  type TrailingSlash,
  addressOf,
  applicationPathOf,
  basePath,
  canonicalAddress,
  installRouting,
  trailingSlash,
} from "./internal/base-path.js";
import {
  type RouteTable,
  Link,
  installNavigation,
  redirect,
  routerView,
  useRouter,
} from "./internal/runtime.js";

import { createServerReference } from "./action.js";
import { ACTION_OUTCOME_HEADER } from "./internal/action-wire.js";

const POLICIES: $ReadOnlyArray<TrailingSlash> = ["never", "always"];

/**
 * The root the last `hydrateWith` created, unmounted after each test.
 *
 * `hydrate` mounts an application for the life of the document, and a worker
 * runs many files in one: left mounted, this file's router, its `popstate`
 * listener and its effects outlive the file, and the next file's server action
 * redirect was answered by pushing this router rather than by loading the
 * document it asked for (the failure in `action-outcomes.test.js` that only a
 * shared worker showed).
 */
let hydrated: { unmount(): void } | null = null;

afterEach(async () => {
  const root = hydrated;
  hydrated = null;
  if (root != null) {
    await act(async () => {
      root.unmount();
    });
  }
  // Module state, like the navigation mode: a worker runs many files out of one
  // module registry, and a base left installed here would be written into the
  // links of whichever file the scheduler ran next.
  installRouting({});
  if (globalThis.document == null) {
    return;
  }
  cleanup();
  globalThis.document.body.replaceChildren();
  globalThis.window.history.replaceState(null, "", "/");
  installNavigation("client");
});

// ---------------------------------------------------------------------------
// The spelling
// ---------------------------------------------------------------------------

describe("an application nothing was installed for", () => {
  it("is at the root, and writes every path the way it was given", () => {
    // What every application did before the two settings existed, and what a
    // test that installs nothing still gets.
    expect(basePath()).toBe("");
    expect(trailingSlash()).toBe("ignore");
    for (const to of ["/", "/guide", "/guide/", "/posts/a?b=1#c"]) {
      expect(addressOf(to)).toBe(to);
    }
    expect(applicationPathOf("/guide")).toBe("/guide");
    expect(canonicalAddress("/guide/")).toBe("/guide/");
  });
});

describe("an address under a base path", () => {
  it("is the application path with the base in front, the query and fragment kept", () => {
    installRouting({ basePath: "/docs" });

    expect(addressOf("/guide")).toBe("/docs/guide");
    expect(addressOf("/guide/install?tab=api#npm")).toBe("/docs/guide/install?tab=api#npm");
  });

  it("spells the application's root as the base itself, with the slash only under always", () => {
    installRouting({ basePath: "/docs" });
    expect(addressOf("/")).toBe("/docs");
    expect(addressOf("/?q=uf")).toBe("/docs?q=uf");

    installRouting({ basePath: "/docs", trailingSlash: "always" });
    expect(addressOf("/")).toBe("/docs/");
  });

  it("leaves what the browser resolves for itself as it was written", () => {
    installRouting({ basePath: "/docs", trailingSlash: "never" });

    // Another origin, a protocol-relative URL, another scheme, and three
    // references the browser resolves against the address already in the bar.
    for (const to of [
      "https://example.com/guide/",
      "//cdn.example.com/app.js",
      "mailto:hello@example.com",
      "?page=2",
      "#install",
      "../up/",
    ]) {
      expect(addressOf(to)).toBe(to);
    }
  });

  it("forgives the trailing slash on a base a hand-written entry installs", () => {
    installRouting({ basePath: "/docs/" });

    expect(basePath()).toBe("/docs");
    expect(addressOf("/guide")).toBe("/docs/guide");
  });
});

describe("an application path read back from an address", () => {
  it("has the base taken off, and both spellings of the base are the root", () => {
    installRouting({ basePath: "/docs" });

    expect(applicationPathOf("/docs/guide")).toBe("/guide");
    expect(applicationPathOf("/docs")).toBe("/");
    expect(applicationPathOf("/docs/")).toBe("/");
  });

  it("is null outside the base, and a base is whole segments", () => {
    installRouting({ basePath: "/docs" });

    expect(applicationPathOf("/guide")).toBe(null);
    expect(applicationPathOf("/")).toBe(null);
    expect(applicationPathOf("/docsx/guide")).toBe(null);
  });
});

describe("the trailing-slash policy", () => {
  it("drops the slash under never, so following a link is not a redirect", () => {
    installRouting({ trailingSlash: "never" });

    expect(addressOf("/guide/")).toBe("/guide");
    expect(addressOf("/guide/?tab=api")).toBe("/guide?tab=api");
    // The root of an application at the root has one spelling.
    expect(addressOf("/")).toBe("/");
  });

  it("adds the slash under always", () => {
    installRouting({ trailingSlash: "always" });

    expect(addressOf("/guide")).toBe("/guide/");
    expect(addressOf("/guide#install")).toBe("/guide/#install");
  });

  it("leaves a path that names a file alone under either", () => {
    for (const policy of POLICIES) {
      installRouting({ trailingSlash: policy });
      expect(addressOf("/sitemap.xml")).toBe("/sitemap.xml");
      expect(addressOf("/downloads/uf-1.0.tar.gz")).toBe("/downloads/uf-1.0.tar.gz");
    }
  });

  it("spells the address a navigation reads back from a payload URL", () => {
    // A payload URL names its document without the slash, and the history
    // entry has to be the address the server answers without a `308`.
    installRouting({ basePath: "/docs", trailingSlash: "always" });

    expect(canonicalAddress("/docs/guide")).toBe("/docs/guide/");
    expect(canonicalAddress("/docs")).toBe("/docs/");
    // Outside the base, it is not this application's address to spell.
    expect(canonicalAddress("/elsewhere")).toBe("/elsewhere");
  });
});

// ---------------------------------------------------------------------------
// The generated entries
// ---------------------------------------------------------------------------

describe("the entries @uniflowed/vite generates", () => {
  it("say nothing about routing for a project at the root with the default policy", () => {
    const routing = routingRulesOf({});

    expect(clientModuleSource("/app.js", { routing })).toContain(
      "hydrate({ App, routes, notFound, errors });",
    );
    expect(flightClientSource("/app.js", { routing })).toContain("hydrateFlight({ App });");
  });

  it("write the base and the policy into the client entry as constants", () => {
    const routing = routingRulesOf({ basePath: "/docs", trailingSlash: "never" });

    expect(clientModuleSource("/app.js", { routing })).toContain(
      'hydrate({ App, routes, notFound, errors, basePath: "/docs", trailingSlash: "never" });',
    );
    expect(flightClientSource("/app.js", { routing })).toContain(
      'hydrateFlight({ App, basePath: "/docs", trailingSlash: "never" });',
    );
  });

  it("install them in the server entry before the first render", () => {
    const source = serverModuleSource("/app.js", routingRulesOf({ basePath: "/docs/" }));

    expect(source).toContain("installRouting(routing);");
    // Normalised where the config is read, so every host compares one spelling.
    expect(source).toContain('"basePath":"/docs"');
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

const NO_ASSETS = { scripts: [], styles: [], preloads: [] };

/**
 * The route table, built once, for the reason `navigation.test.js` memoises
 * its own: the server render and the hydration have to be the same components.
 */
let built: ?RouteTable["routes"] = null;

function routes(): RouteTable["routes"] {
  if (built != null) {
    return built;
  }

  component Home() {
    const router = useRouter();
    return (
      <section>
        <h1>home</h1>
        <Link to="/other">other</Link>
        <button type="button" onClick={() => void router.push("/other")}>
          push
        </button>
        <button type="button" onClick={() => void router.push("https://example.com/elsewhere")}>
          leave
        </button>
      </section>
    );
  }

  component Other() {
    return <h1>other</h1>;
  }

  component Moved() {
    return <h1>moved</h1>;
  }

  const table = [
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
      path: "/other",
      params: [],
      mdx: false,
      file: "app/other/$page.js",
      page: () => Promise.resolve({ default: Other }),
      layouts: [],
      loading: [],
    },
    {
      path: "/moved",
      params: [],
      mdx: false,
      file: "app/moved/$page.js",
      page: () => Promise.resolve({ default: Moved, loader: () => redirect("/other") }),
      layouts: [],
      loading: [],
    },
  ];
  built = table;
  return table;
}

/**
 * Put the document the server writes for `url` into the live DOM, at
 * `address`.
 *
 * `url` is an application path, because that is what every front door hands
 * the renderer once it has taken the base off; `address` is what the browser
 * asked for.
 */
async function serve(url: string, address: string): Promise<void> {
  const { createRenderer, ROOT_ID } = await serverModule();
  const renderer = createRenderer({
    App: routerView("./app"),
    routes: routes(),
    notFound: [],
    errors: [],
  });
  const { html } = await renderer.prerender(url, NO_ASSETS);

  const parsed = new globalThis.DOMParser().parseFromString(html, "text/html");
  const rendered = parsed.getElementById(ROOT_ID);
  if (rendered == null) {
    throw new Error(`the server wrote no #${ROOT_ID}:\n${html}`);
  }
  const root = globalThis.document.createElement("div");
  root.id = ROOT_ID;
  root.innerHTML = rendered.innerHTML;
  globalThis.document.body.replaceChildren(root);
  globalThis.window.history.pushState(null, "", address);
}

/** Hydrate the current document the way `virtual:uf/client` would for these settings. */
async function hydrateWith(settings: {|
  readonly basePath?: string,
  readonly trailingSlash?: TrailingSlash,
  readonly navigation?: "client" | "document",
|}): Promise<void> {
  const { hydrate } = await clientModule();
  await act(async () => {
    hydrated = await hydrate({
      App: routerView("./app"),
      routes: routes(),
      notFound: [],
      errors: [],
      ...settings,
    });
  });
}

function ufRoot(): Element | null {
  return globalThis.document.getElementById("uf-root");
}

function heading(): string {
  return ufRoot()?.querySelector("h1")?.textContent ?? "(no heading)";
}

function linkTo(href: string): Element {
  const link = ufRoot()?.querySelector(`a[href="${href}"]`);
  if (link == null) {
    throw new Error(`no link to ${href} in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
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

/** What a navigation asked the browser to load, with `location.assign` put back after. */
async function assignedDuring(run: () => Promise<void>): Promise<Array<string>> {
  const assigned: Array<string> = [];
  const location = globalThis.window.location;
  const original = location.assign;
  Object.defineProperty(location, "assign", {
    configurable: true,
    writable: true,
    value: (to: string) => {
      assigned.push(String(to));
    },
  });
  try {
    await run();
  } finally {
    Object.defineProperty(location, "assign", {
      configurable: true,
      writable: true,
      value: original,
    });
  }
  return assigned;
}

describe("a Link under a base path", () => {
  it("writes the address, in the server's markup and after hydration", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/", "/docs");

    // Before any script has run: the anchor a crawler follows, and the one a
    // visitor opens in a new tab, goes where a click will.
    linkTo("/docs/other");

    await hydrateWith({ basePath: "/docs" });

    expect(heading()).toBe("home");
    linkTo("/docs/other");
  });

  it("writes it in the trailing-slash policy's spelling", async () => {
    installRouting({ basePath: "/docs", trailingSlash: "always" });
    await serve("/", "/docs/");

    linkTo("/docs/other/");
  });
});

describe("a navigation under a base path", () => {
  it("renders the route the application path names, and puts the address in history", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/", "/docs");
    await hydrateWith({ basePath: "/docs" });

    await act(async () => {
      await userEvent.click(linkTo("/docs/other"));
    });
    await waitFor(() => {
      expect(heading()).toBe("other");
    });

    expect(globalThis.window.location.pathname).toBe("/docs/other");
  });

  it("hydrates the route the address names once the base is taken off", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/other", "/docs/other");
    await hydrateWith({ basePath: "/docs" });

    // Asked of the table as `/docs/other`, this would be no route at all.
    expect(heading()).toBe("other");
  });

  it("hands a document navigation the address, base included", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/", "/docs");
    await hydrateWith({ basePath: "/docs", navigation: "document" });

    const assigned = await assignedDuring(async () => {
      await act(async () => {
        await userEvent.click(buttonSaying("push"));
      });
    });

    expect(assigned.length).toBe(1);
    expect(assigned[0].endsWith("/docs/other")).toBe(true);
  });

  it("leaves an address outside the base to the browser", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/", "/docs");
    await hydrateWith({ basePath: "/docs" });

    const assigned = await assignedDuring(async () => {
      await act(async () => {
        await userEvent.click(buttonSaying("leave"));
      });
    });

    expect(assigned).toEqual(["https://example.com/elsewhere"]);
    expect(heading()).toBe("home");
  });
});

describe("a loader's redirect under a base path", () => {
  it("answers with the address, which is what a browser follows", async () => {
    // `redirect("/other")` names an application path, like a `Link`'s `to`.
    installRouting({ basePath: "/docs" });
    const { createRenderer } = await serverModule();
    const renderer = createRenderer({
      App: routerView("./app"),
      routes: routes(),
      notFound: [],
      errors: [],
    });

    const answered = await renderer.prerender("/moved", NO_ASSETS);

    expect(answered.status).toBe(307);
    expect(answered.headers?.Location).toBe("/docs/other");
    expect(answered.html).toContain('href="/docs/other"');
  });
});

// The other half of the unmount above, and the product rule under it: taking
// an application down forgets its router. A server action that redirects after
// that has no router to push, and loads the document instead — which is what
// the next file in a worker, or a page that unmounted its root, is owed.
describe("an application whose root was unmounted", () => {
  it("leaves no router for a server action's redirect to push", async () => {
    installRouting({ basePath: "/docs" });
    await serve("/", "/docs");
    await hydrateWith({ basePath: "/docs" });
    expect(heading()).toBe("home");

    const root = hydrated;
    hydrated = null;
    await act(async () => {
      root?.unmount();
    });

    const loaded: Array<string> = [];
    uft.spyOn(globalThis.window.location, "assign").mockImplementation((href: string) => {
      loaded.push(String(href));
    });
    uft.spyOn(globalThis, "fetch").mockImplementation(
      async () =>
        new Response(null, {
          status: 204,
          headers: { [ACTION_OUTCOME_HEADER]: "redirect", location: "/docs/other" },
        }),
    );
    try {
      await createServerReference("a".repeat(64), "app/_actions.js#save")();
    } finally {
      uft.restoreAllMocks();
    }
    expect(loaded).toEqual(["http://localhost/docs/other"]);
  });
});
