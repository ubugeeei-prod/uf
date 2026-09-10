// @flow
//
// `app.rendering.modes: ["csr"]`: one shell, and the browser renders the rest.
//
// Every other rendering plan writes markup somewhere — a build writes a
// document per route, a server writes one per request — and the browser's job
// is to attach to it. This one writes a document that is no route's: the
// stylesheets, the module script, and an empty root. So the two halves below
// are the two things that have no counterpart anywhere else in uf.
//
// **The shell.** It has to be empty, and "empty" is an assertion rather than an
// observation: a shell that accidentally carried a route's markup would be a
// document served for *every* URL with one route's content baked into it, and
// the only symptom is the wrong page appearing for a moment on every other one.
//
// **The mount.** `render` rather than `hydrate`, because hydration is React
// comparing what it renders against markup a server sent and there is none.
// The tests below are written from the shell outwards for that reason: the
// document a test starts from is the one `shellDocument` produced, so what is
// being proved is that the two halves of this plan fit each other rather than
// that each half does what its own comment says.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { act, cleanup, userEvent } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { installNavigation, redirect, routerView } from "./internal/runtime.js";
import { clientModuleSource } from "../../packages/vite/internal/routes.js";

// ---------------------------------------------------------------------------
// The generated entry
// ---------------------------------------------------------------------------

describe("the client entry a single-page build gets", () => {
  it("hydrates by default, which is what every other plan wants", () => {
    const source = clientModuleSource("/app.js");

    expect(source).toContain('import { hydrate } from "@uniflowed/router/client";');
    expect(source).not.toContain("render(");
  });

  it("renders when the build wrote a shell", () => {
    const source = clientModuleSource("/app.js", { mount: "render" });

    // One import or the other, rather than one import and a branch: which
    // function is in the module decides whether `createRoot` or `hydrateRoot`
    // is in the bundle.
    expect(source).toContain('import { render } from "@uniflowed/router/client";');
    expect(source).not.toContain("hydrate");
    expect(source).toContain("render({ App, routes, notFound, errors });");
  });
});

// ---------------------------------------------------------------------------
// The shell, and what renders into it
// ---------------------------------------------------------------------------

async function clientModule() {
  installDom();
  return import("@uniflowed/router/client");
}

async function serverModule() {
  installDom();
  return import("@uniflowed/router/server");
}

const ASSETS = {
  scripts: ["/assets/client.js"],
  styles: ["/assets/app.css"],
  preloads: [],
};

let built: mixed = null;

function tables() {
  if (built != null) {
    return built;
  }

  component Home() {
    const [count, setCount] = useState<number>(0);
    return (
      <section>
        <h1>home</h1>
        <output>{count}</output>
        <button type="button" onClick={() => setCount(count + 1)}>
          add one
        </button>
      </section>
    );
  }

  component Missing() {
    return <h1>nothing here</h1>;
  }

  const home = {
    path: "/",
    params: [],
    mdx: false,
    file: "app/_uf.page.js",
    page: () => Promise.resolve({ default: Home }),
    layouts: [],
    loading: [],
  };
  const away = {
    path: "/away",
    params: [],
    mdx: false,
    file: "app/away/_uf.page.js",
    // A loader that redirects, which on a server is a 307 before a byte is
    // written and here has to become the browser's own navigation. It is an
    // export of the page *module*, the way a project writes one, rather than a
    // field on the route record.
    page: () => Promise.resolve({ default: Home, loader: () => redirect("/") }),
    layouts: [],
    loading: [],
  };
  const notFound = [
    {
      path: "/",
      mdx: false,
      file: "app/_uf.not-found.js",
      page: () => Promise.resolve({ default: Missing }),
      layouts: [],
    },
  ];

  built = { routes: [home, away], notFound };
  return built;
}

/** Put the shell a `["csr"]` build writes into the live DOM, and go to `url`. */
async function serveShell(url: string): Promise<string> {
  const { shellDocument } = await serverModule();
  const html = shellDocument(ASSETS);
  globalThis.document.documentElement.innerHTML = html
    .replace(/^[\s\S]*?<html[^>]*>/, "")
    .replace(/<\/html>\s*$/, "");
  globalThis.window.history.pushState(null, "", url);
  return html;
}

async function renderHere(): Promise<void> {
  const { render } = await clientModule();
  const { routes, notFound } = tables();
  await act(async () => {
    await render({ App: routerView("./app"), routes, notFound, errors: [] });
  });
}

function ufRoot(): Element | null {
  return globalThis.document.getElementById("uf-root");
}

afterEach(() => {
  if (globalThis.document == null) {
    return;
  }
  cleanup();
  globalThis.document.body.replaceChildren();
  installNavigation("client");
});

describe("the shell a single-page build writes", () => {
  it("carries the assets and an empty root", async () => {
    const html = await serveShell("/");

    expect(html).toContain('<link rel="stylesheet" href="/assets/app.css">');
    expect(html).toContain('<script type="module" src="/assets/client.js"></script>');
    // The assertion the whole plan rests on: nothing between the root's tags.
    // A shell with a route's markup in it is served for every URL.
    expect(html).toContain('<div id="uf-root"></div>');
  });

  it("says nothing a crawler could read, which is the cost", async () => {
    // Written down as a test rather than only in the guide. A document with no
    // title and no content is what `["csr"]` means, and the day it stops being
    // true — because something started rendering into the shell — is a day
    // this plan has quietly become a different one.
    const html = await serveShell("/");

    expect(html).not.toContain("<title>");
    expect(html).not.toContain("home");
  });
});

describe("rendering into it", () => {
  it("renders the route and it is interactive", async () => {
    await serveShell("/");
    expect(ufRoot()?.textContent).toBe("");

    await renderHere();

    expect(ufRoot()?.textContent).toContain("home");
    const button = ufRoot()?.querySelector("button");
    if (button == null) {
      throw new Error(`no button in:\n${ufRoot()?.innerHTML ?? "(no root)"}`);
    }
    await act(async () => {
      await userEvent.click(button);
    });
    expect(ufRoot()?.querySelector("output")?.textContent).toBe("1");
  });

  it("renders the not-found boundary for a URL the application does not have", async () => {
    // The ordinary arrival rather than the exception: a static host served this
    // shell as its error document, so "nothing matched" is most of what this
    // entry point sees.
    await serveShell("/no-such-page");

    await renderHere();

    expect(ufRoot()?.textContent).toContain("nothing here");
  });

  it("hands a redirect from a loader to the browser", async () => {
    // On a server this is a 307 before a byte is written. There is no response
    // to put a status on here, so the same instruction has to reach the same
    // browser a different way.
    await serveShell("/away");
    const replaced: Array<string> = [];
    const location = globalThis.window.location;
    const original = location.replace;
    Object.defineProperty(location, "replace", {
      configurable: true,
      writable: true,
      value: (to: string) => {
        replaced.push(String(to));
      },
    });

    try {
      await renderHere();
    } finally {
      Object.defineProperty(location, "replace", {
        configurable: true,
        writable: true,
        value: original,
      });
    }

    expect(replaced).toEqual(["/"]);
    // And nothing was rendered: the browser is leaving.
    expect(ufRoot()?.textContent).toBe("");
  });
});
