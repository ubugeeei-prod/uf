// @flow
//
// Strict Mode, which `uf dev` turns on and a build does not.
//
// React's Strict Mode renders a component twice, runs a state initialiser and a
// `useMemo` factory twice, and mounts every effect, unmounts it and mounts it
// again. It is the check React ships for the two things nothing else can see: a
// component whose render is not pure, and an effect whose cleanup does not undo
// its setup. A uf application was developed without it. See
// ubugeeei-prod/uf#516.
//
// Two halves here, because turning it on is two decisions.
//
// The first is *whether*: the flag is generated into `virtual:uf/client`, so a
// development entry carries it, a build's does not, and `uf.config.js` can turn
// it off. That is asserted on the generated source, which is the artefact the
// decision actually reaches.
//
// The second is *what it costs*, and it is the half worth having: the router
// itself now runs doubled on every uf project's development machine. So this
// hydrates a real server-rendered document through `@uniflowed/router/client`
// with the flag on and asks what a double invocation actually does to it.
//
// One thing that turned up while writing those cases is worth having in the
// file rather than in a pull request nobody will find again: **React does not
// double-invoke effects on a root that hydrated.** The render is doubled and
// the state initialiser is doubled, but the mount/unmount/mount pass is skipped
// for the hydration commit, because the DOM it would tear down is the server's
// markup. So in a framework that server-renders, "Strict Mode finds the effect
// you forgot to clean up" is true of every mount *after* the first paint —
// every navigation, every conditional branch, every list row — and is not true
// of the page load itself. Both are asserted below, because a reader who
// expects the second one and does not see it will conclude the flag is not
// working.
//
// Turning it on found two real bugs, and both are fixed rather than described.
// `@uniflowed/web/vitals` reported TTFB from the collector Strict Mode throws
// away *and* from the one it keeps, so every page load produced two of them.
// `@uniflowed/query` treated the strict unsubscribe as the last observer
// leaving, cancelled the request in flight and asked again, so every `useQuery`
// on every development page made two requests and aborted one of them.
// `web-vitals.test.js` and `query.test.js` own those regressions.

import * as React from "@uniflowed/react";
import { StrictMode, useEffect, useState } from "@uniflowed/react";
import { act, cleanup, render, userEvent } from "@uniflowed/react-testing";
import { Link, routerView } from "@uniflowed/router";
import { afterEach, describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { VIRTUAL, clientModuleSource } from "../../packages/vite/internal/routes.js";
import uniflowed from "../../packages/vite/index.js";

// ---------------------------------------------------------------------------
// Whether: the generated client entry
// ---------------------------------------------------------------------------

/** The `uf:flow` plugin, told which command it is running under. */
function flowPlugin(command: "serve" | "build", config?: $FlowFixMe): $FlowFixMe {
  const plugins: $FlowFixMe = uniflowed({ root: process.cwd(), config: config ?? {} });
  const flow = plugins[0];
  flow.config(
    { root: process.cwd() },
    { mode: command === "build" ? "production" : "development", command },
  );
  flow.configResolved({ root: process.cwd(), base: "/" });
  return flow;
}

/**
 * The source of `virtual:uf/client` as that plugin would generate it.
 *
 * Through `resolveId` rather than by spelling the NUL-prefixed id out here: the
 * prefix is Vite's convention and the plugin owns it, and an id written out in
 * this file would go on matching after the plugin stopped answering to it.
 */
function clientEntry(command: "serve" | "build", config?: $FlowFixMe): string {
  const flow = flowPlugin(command, config);
  return flow.load(flow.resolveId(VIRTUAL.client));
}

describe("the flag the client entry carries", () => {
  it("hydrates under Strict Mode when the dev server generated the entry", () => {
    expect(clientEntry("serve")).toContain("strictMode: true");
  });

  it("says nothing about it in a build", () => {
    // Not `strictMode: false` — absent. A literal in the generated module is
    // what keeps a production bundle from carrying the decision at all, and it
    // is why this is a generated constant rather than a runtime read of
    // `import.meta.hot`.
    const built = clientEntry("build");
    expect(built).toContain("hydrate({ App, routes, notFound, errors });");
    expect(built).not.toContain("strictMode");
  });

  it("lets a project turn it off", () => {
    // The escape hatch #516 asks for, and the whole of it: a project that
    // disagrees gets a dev server that hydrates the way its deployment does.
    const off = clientEntry("serve", { app: { react: { strictMode: false } } });
    expect(off).not.toContain("strictMode");
    // And a project that says so explicitly gets what it asked for, rather than
    // "anything that is not `false`" quietly meaning "off".
    expect(clientEntry("serve", { app: { react: { strictMode: true } } })).toContain(
      "strictMode: true",
    );
  });

  it("generates the same module for a build whatever the project said", () => {
    // Strict Mode is a development default, so `strictMode: true` in a config
    // must not reach a visitor's browser: what the project is choosing is
    // whether their own dev server doubles, not what their site does.
    expect(clientEntry("build", { app: { react: { strictMode: true } } })).not.toContain(
      "strictMode",
    );
  });

  it("takes the flag from its caller rather than from a global", () => {
    // `clientModuleSource` is the seam, so it is worth one assertion of its
    // own: the plugin decides, and this function only writes it down.
    expect(clientModuleSource("/app.js", { strictMode: true })).toContain("strictMode: true");
    expect(clientModuleSource("/app.js")).not.toContain("strictMode");
  });
});

// ---------------------------------------------------------------------------
// What it costs: the router, hydrated twice over
// ---------------------------------------------------------------------------

/**
 * The client and server halves of `@uniflowed/router`, imported once a document
 * exists.
 *
 * `@uniflowed/router/client` statically imports `react-dom/client`, which reads
 * `document` while it is being evaluated — so the DOM has to exist before the
 * *import* and not merely before the first render. The same wrappers, for the
 * same reason, as `rsc-split.test.js`.
 */
async function clientModule(): Promise<$FlowFixMe> {
  installDom();
  return import("@uniflowed/router/client");
}

async function serverModule(): Promise<$FlowFixMe> {
  installDom();
  return import("@uniflowed/router/server");
}

/**
 * Every effect setup and cleanup the page under test performed, in order.
 *
 * The subject of the second half of this file: under Strict Mode React runs
 * setup, cleanup, setup, so a correct effect leaves this list balanced and one
 * that leaks leaves it one short.
 */
let effectLog: Array<string> = [];

/** How many times a state initialiser was evaluated. */
let initialiserRuns = 0;

afterEach(() => {
  effectLog = [];
  initialiserRuns = 0;
  // The roots first and the document after. A React root whose container is
  // merely detached is still a live root: it re-renders, its effects run again,
  // and the next case in this file counts them — which is how this suite
  // reported "multiple renderers concurrently rendering the same context
  // provider" and an initialiser that ran twice with Strict Mode off. Guarded
  // because the cases above install no document at all.
  if (globalThis.document == null) {
    return;
  }
  cleanup();
  globalThis.document.body.replaceChildren();
});

/**
 * The one page both halves render, with a counter and an effect in it.
 *
 * Deliberately the shape `uf create` scaffolds — a `"use client"` counter over
 * `useState`, plus a `Link` — because that is the application whose cleanliness
 * under Strict Mode had to be true before the default could be flipped, and
 * because a page built out of nothing would prove only that `hydrateRoot` was
 * called.
 */
component Counter() {
  const [count, setCount] = useState<number>(() => {
    initialiserRuns += 1;
    return 0;
  });

  useEffect(() => {
    effectLog.push("home:setup");
    return () => {
      effectLog.push("home:cleanup");
    };
  }, []);

  return (
    <p>
      <output>{count}</output>
      <button type="button" onClick={() => setCount((value) => value + 1)}>
        add one
      </button>
    </p>
  );
}

component HomePage() {
  return (
    <section>
      <h1>home</h1>
      <Counter />
      <Link to="/about">about</Link>
    </section>
  );
}

/**
 * The other page, with an effect of its own.
 *
 * It exists to be mounted *after* hydration, which is the only place a uf
 * application sees Strict Mode's effect double-invocation — see the second case
 * below for why the hydration itself does not.
 */
component AboutPage() {
  useEffect(() => {
    effectLog.push("about:setup");
    return () => {
      effectLog.push("about:cleanup");
    };
  }, []);
  return <h1>about</h1>;
}

const home = {
  path: "/",
  params: [],
  mdx: false,
  file: "app/_uf.page.js",
  page: () => Promise.resolve({ default: HomePage }),
  layouts: [],
  loading: [],
};

const about = {
  path: "/about",
  params: [],
  mdx: false,
  file: "app/about/_uf.page.js",
  page: () => Promise.resolve({ default: AboutPage }),
  layouts: [],
  loading: [],
};

const routes = [home, about];

/**
 * Put the document the server would have written into the live DOM.
 *
 * The markup comes from the real server renderer rather than being written by
 * hand: hydration is React comparing what it renders against what the server
 * sent, and invented markup would prove nothing about whether the two agree.
 */
async function serve(url: string): Promise<void> {
  const { createRenderer, ROOT_ID } = await serverModule();
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

/** Hydrate the served document the way `uf dev`'s generated entry does. */
async function hydrateHere(strictMode: boolean): Promise<void> {
  const { hydrate } = await clientModule();
  await act(async () => {
    await hydrate({
      App: routerView("./app"),
      routes,
      notFound: [],
      errors: [],
      strictMode,
    });
  });
}

/** The rendered root, as text. */
function pageText(): string {
  return globalThis.document.body.textContent ?? "";
}

describe("the router, hydrated under Strict Mode", () => {
  it("renders once for a hydration the flag is off for", async () => {
    // The baseline, and it runs first on purpose: a hydrated root that is left
    // behind by an earlier case goes on listening to the router's store, so
    // every count in this file is only trustworthy while the case that took it
    // is the first to hydrate. Two initialiser runs, not one — the server
    // rendered this page before the browser did, and that is the render this
    // number is counting alongside the client's.
    await serve("/");
    await hydrateHere(false);

    expect(initialiserRuns).toBe(2);
    expect(effectLog).toEqual(["home:setup"]);
  });

  it("renders a hydrated page twice, and mounts its effects once", async () => {
    // Both halves of what React actually does, because only one of them is the
    // half people expect.
    //
    // The render is doubled: three initialiser runs where the case above had
    // two, so the impure component and the state initialiser with a side effect
    // in it are caught on the first page load, which is the point.
    //
    // The effects are not. React skips the mount/unmount/mount pass for a root
    // that hydrated, because the DOM it would tear down and rebuild is the
    // server's markup and rebuilding it is the thing hydration exists to avoid.
    // So the effect check arrives on the next mount rather than on this one —
    // which is the case below, and is worth stating here because "Strict Mode
    // finds an effect that was never cleaned up" is the sentence everybody
    // knows and it is not true of a page's first load in a framework that
    // server-renders.
    await serve("/");
    await hydrateHere(true);

    expect(initialiserRuns).toBe(3);
    expect(effectLog).toEqual(["home:setup"]);
  });

  it("mounts an effect twice for anything that mounts after hydration", async () => {
    // The half that does the work, and the reason the default is worth having:
    // every navigation, every conditional branch and every list row mounted
    // after the first paint gets the setup/cleanup/setup treatment. `/about`
    // has an effect of its own and is reached by clicking a `Link`, which is
    // the ordinary way a uf application mounts anything.
    //
    // The counter's effect is cleaned up on the way out and never set up again,
    // which is the other half of a balanced log.
    await serve("/");
    await hydrateHere(true);
    effectLog.length = 0;

    const link = globalThis.document.querySelector("a");
    if (link == null) {
      throw new Error("the link did not hydrate");
    }
    await act(async () => {
      await userEvent.click(link);
    });

    expect(pageText()).toContain("about");
    expect(effectLog).toEqual(["home:cleanup", "about:setup", "about:cleanup", "about:setup"]);
  });

  it("hydrates the server's markup and counts once per click", async () => {
    // `<StrictMode>` renders no element of its own, so the tree React compares
    // against the server's markup is unchanged by the wrapper — if it were not,
    // every page in development would hydrate into a mismatch rather than into
    // a working page.
    //
    // And the doubling people are afraid of, with the reason to be less afraid:
    // a pure component rendered twice produces the same tree, and a state
    // initialiser evaluated twice still initialises one piece of state.
    await serve("/");
    await hydrateHere(true);

    expect(pageText()).toContain("home");
    const output = globalThis.document.querySelector("output");
    const button = globalThis.document.querySelector("button");
    if (output == null || button == null) {
      throw new Error("the counter did not hydrate");
    }
    expect(output.textContent).toBe("0");

    await act(async () => {
      await userEvent.click(button);
    });

    expect(output.textContent).toBe("1");
  });
});

// ---------------------------------------------------------------------------
// Where the wrapper has to be
// ---------------------------------------------------------------------------

describe("where `<StrictMode>` has to be", () => {
  // The reason `client.js` passes the wrapper to `hydrateRoot` rather than
  // putting it anywhere inside `<App>`, pinned so that a refactor which moves
  // it fails here instead of quietly turning the check off.
  //
  // React decides whether to double-invoke a mount's effects at the topmost
  // fiber it is placing, and stops there: a fiber that is not itself in Strict
  // Mode is not descended into for this purpose. So a `<StrictMode>` under
  // anything at all doubles the renders beneath it — which comes from the
  // fiber's own mode and is easy to see — and doubles no effect, which is the
  // half that finds the bug. `@uniflowed/query`'s double fetch was found
  // because this is true and missed at first because it is not obvious.

  /** What one component did, in order, for one mount. */
  function record(): {| log: Array<string>, Probe: React.ComponentType<{||}> |} {
    const log: Array<string> = [];
    component Probe() {
      log.push("render");
      useEffect(() => {
        log.push("setup");
        return () => {
          log.push("cleanup");
        };
      }, []);
      return null;
    }
    return { log, Probe: (Probe: $FlowFixMe) };
  }

  it("doubles renders and effects when it is the root's own child", () => {
    installDom();
    const { log, Probe } = record();

    render(
      <StrictMode>
        <Probe />
      </StrictMode>,
    );

    expect(log).toEqual(["render", "render", "setup", "cleanup", "setup"]);
  });

  it("doubles only the renders when anything is above it", () => {
    // Not an assertion about what *should* happen — it is React's rule, and
    // this is here so a reader who moves the wrapper sees the cost written
    // down rather than a suite that still passes.
    installDom();
    const { log, Probe } = record();

    component Shell(children: React.Node) {
      return <div>{children}</div>;
    }

    render(
      <Shell>
        <StrictMode>
          <Probe />
        </StrictMode>
      </Shell>,
    );

    expect(log).toEqual(["render", "render", "setup"]);
  });
});
