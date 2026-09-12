// @flow
//
// Parallel routes: `@slot` directories, and the `$default.js` that fills one
// the URL said nothing about.
//
// uf's route tree had exactly one page per URL, so there was no way to render a
// route somewhere other than where its path said. A directory named `@team` was
// the second of the two Next.js spellings that fell through to "an ordinary URL
// segment" — `app/@team/$page.js` served `/@team` — and then, with #472, a
// name refused by both routers. It is a route now, and this file is what says
// so: the segment contributes no URL, its pages are matched against the same
// path the page is, and the layout of the segment that declares it receives the
// result as a prop beside `children`. See ubugeeei-prod/uf#267.
//
// Four things have to be true and there is a section for each: the scan finds
// slots and keeps them out of the URL, it refuses the ways one can be written
// without being renderable, the composed tree hands each slot to the right
// layout, and a navigation moves the slots with the page.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import * as React from "@uniflowed/react";
import { render, screen, userEvent } from "@uniflowed/react-testing";
import { RouteView, RouterProvider, resolveMatch, routerView, useRouter } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { afterAll, describe, expect, it } from "@uniflowed/test";

import {
  LAYOUT_PROP_NAMES,
  routesModuleSource,
  scanRoutes,
} from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root holding each named file. */
function appRoot(files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-slots-"));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Part() {}\n");
  }
  return root;
}

/** What `scanRoutes` threw for this tree, or `null` when it did not. */
function refusal(files: $ReadOnlyArray<string>): ?string {
  try {
    scanRoutes(appRoot(files));
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

/** A dashboard with two slots, which is the shape the whole feature is for. */
const DASHBOARD = [
  "$layout.js",
  "dashboard/$layout.js",
  "dashboard/$page.js",
  "dashboard/members/$page.js",
  "dashboard/@team/$default.js",
  "dashboard/@team/$layout.js",
  "dashboard/@team/members/$page.js",
  "dashboard/@analytics/$page.js",
];

describe("scanning a router root that holds slots", () => {
  it("gives a slot no URL of its own", () => {
    // The whole of the first half. `@team` used to be the literal segment
    // `/@team`; now it is not a path at all, and the paths in the table are
    // exactly the ones the ordinary directories name.
    const { routes } = scanRoutes(appRoot(DASHBOARD));

    expect(routes.map((route) => route.path)).toEqual(["/dashboard", "/dashboard/members"]);
  });

  it("matches a slot's pages against the URL the segment is matched against", () => {
    // `app/dashboard/@team/members/$page.js` is what `/dashboard/members`
    // puts in the `team` slot — the same path, not `/dashboard/@team/members`
    // and not a second route at `/dashboard/members`.
    const root = appRoot(DASHBOARD);
    const { routes } = scanRoutes(root);
    const team = routes[0].slots.find((slot) => slot.name === "team");

    expect(team?.routes.map((route) => route.path)).toEqual(["/dashboard/members"]);
    expect(team?.routes[0].layouts.map((file) => path.relative(root, file))).toEqual([
      path.join("dashboard", "@team", "$layout.js"),
    ]);
  });

  it("puts the slot on the layout of the segment that declares it", () => {
    // `above` counts the layouts outside the slot, taken after the declaring
    // segment's own layout is added — so `layouts[above - 1]` is the layout
    // that receives the prop. The same number, spelled the same way, as a
    // template's.
    const { routes } = scanRoutes(appRoot(DASHBOARD));

    expect(routes[0].layouts.length).toBe(2);
    expect(routes[0].slots.map((slot) => ({ name: slot.name, above: slot.above }))).toEqual([
      { name: "analytics", above: 2 },
      { name: "team", above: 2 },
    ]);
  });

  it("carries a slot down to every route under the segment", () => {
    // A slot belongs to the layout, and the layout wraps everything below it,
    // so `/dashboard/members` has both slots too — the way a template does.
    const { routes } = scanRoutes(appRoot(DASHBOARD));

    expect(routes[1].path).toBe("/dashboard/members");
    expect(routes[1].slots.map((slot) => slot.name)).toEqual(["analytics", "team"]);
  });

  it("finds the slot's default and says when it has none", () => {
    const root = appRoot(DASHBOARD);
    const { routes } = scanRoutes(root);
    const [analytics, team] = routes[0].slots;

    expect(team.defaultPage == null ? null : path.relative(root, team.defaultPage)).toBe(
      path.join("dashboard", "@team", "$default.js"),
    );
    expect(analytics.defaultPage).toBe(null);
  });

  it("gives a project with no slot the empty list it had before", () => {
    const { routes } = scanRoutes(appRoot(["$layout.js", "$page.js"]));

    expect(routes[0].slots).toEqual([]);
  });

  it("carries them into the generated module, once each", () => {
    // Deduplicated by identity rather than by file, for the reason layouts are
    // deduplicated: one slot record is shared by every route below the segment
    // that declares it, and emitting it per route would put the slot's whole
    // table in the bundle once per route.
    const source = routesModuleSource(scanRoutes(appRoot(DASHBOARD)));

    expect(source.match(/const slot\d = \{/g)?.length).toBe(2);
    expect(source).toContain("slots: [slot0, slot1]");
    expect(source).toContain('name: "team"');
    expect(source).toContain("defaultPage: () => import(");
  });

  it("nests a slot inside a slot", () => {
    // The recursion is the shape of the feature rather than a special case: a
    // slot's own layout may declare slots, measured against the slot's layouts.
    const root = appRoot([
      "$layout.js",
      "$page.js",
      "@aside/$layout.js",
      "@aside/$page.js",
      "@aside/@inner/$page.js",
    ]);

    const { routes } = scanRoutes(root);
    const aside = routes[0].slots[0];

    expect(aside.name).toBe("aside");
    expect(aside.routes[0].slots.map((slot) => ({ name: slot.name, above: slot.above }))).toEqual([
      { name: "inner", above: 1 },
    ]);
  });
});

describe("what a slot may not be written as", () => {
  it("refuses a slot on a segment with no layout of its own", () => {
    // A slot *is* a prop the segment's own layout receives. Rendering it into
    // an inherited layout would hand a prop to a layout that never declared
    // it, on every route below.
    const message = refusal(["$layout.js", "dashboard/$page.js", "dashboard/@team/$page.js"]);

    expect(message).not.toBe(null);
    expect(message ?? "").toContain("@team");
    expect(message ?? "").toContain("no layout of its own");
  });

  it("refuses a slot named after a prop the layout already has", () => {
    // `@children` is the one somebody actually writes: `children` is what
    // Next.js calls its implicit slot. Whichever way the collision were
    // resolved, one of the two would silently go missing.
    for (const name of LAYOUT_PROP_NAMES) {
      const message = refusal(["$layout.js", "$page.js", path.join(`@${name}`, "$page.js")]);

      expect(message).not.toBe(null);
      expect(message ?? "").toContain(name);
      expect(message ?? "").toContain("silently");
    }
  });

  it("refuses a default that is not inside a slot", () => {
    // uf has no `default` for `children`: `children` is the page the URL
    // matched, and a URL that matches no page is a 404. A `$default.js`
    // outside a slot is a file nothing would ever open.
    const message = refusal(["$layout.js", "$page.js", "$default.js"]);

    expect(message).not.toBe(null);
    expect(message ?? "").toContain("$default.js");
    expect(message ?? "").toContain("404");
  });

  it("refuses a second default deeper inside a slot", () => {
    const message = refusal([
      "$layout.js",
      "$page.js",
      "@aside/$page.js",
      "@aside/deep/$default.js",
    ]);

    expect(message).not.toBe(null);
    expect(message ?? "").toContain("deeper");
  });

  it("refuses a boundary inside a slot, naming what is missing", () => {
    // Per-slot boundaries are the part of parallel routes uf has not built.
    // The refusal is where somebody writing the file finds that out.
    for (const [file, role] of [
      ["@aside/$loading.js", "loading"],
      ["@aside/$error.js", "error"],
      ["@aside/$not-found.js", "not-found"],
      ["@aside/$template.js", "template"],
    ]) {
      const message = refusal(["$layout.js", "$page.js", "@aside/$page.js", file]);

      expect(message).not.toBe(null);
      expect(message ?? "").toContain(role);
      expect(message ?? "").toContain("267");
    }
  });

  it("refuses a handler or a guard inside a slot", () => {
    // A slot directory contributes no URL segment, so either would claim the
    // declaring segment's path — which belongs to somebody else.
    for (const file of ["@aside/$route.js", "@aside/$middleware.js"]) {
      const message = refusal(["$layout.js", "$page.js", file]);

      expect(message).not.toBe(null);
      expect(message ?? "").toContain("@aside");
    }
  });

  it("still refuses an intercepting route", () => {
    // The other half of #267, and it is still a name without a route:
    // interception needs a navigation to carry where it came from.
    const message = refusal(["$layout.js", "feed/(.)photo/$page.js"]);

    expect(message).not.toBe(null);
    expect(message ?? "").toContain("intercepting route");
  });
});

// --- Composition -------------------------------------------------------

component Frame(children: React.Node, team: React.Node) {
  return (
    <div>
      <nav>the sidebar</nav>
      <aside data-testid="team">{team ?? "nothing in the slot"}</aside>
      <main>{children}</main>
    </div>
  );
}

const loadFrame = () => Promise.resolve({ default: Frame });
const pageOf = (text: string) => () => Promise.resolve({ default: () => <p>{text}</p> });

const slot = {
  name: "team",
  above: 1,
  defaultPage: pageOf("the team default"),
  defaultMdx: false,
  routes: [
    {
      path: "/dashboard/members",
      params: [],
      mdx: false,
      file: "app/dashboard/@team/members/$page.js",
      page: pageOf("the team members"),
      layouts: [],
      slots: [],
    },
  ],
};

const page = (routePath: string, text: string) => ({
  path: routePath,
  params: [],
  mdx: false,
  file: `app${routePath}/$page.js`,
  page: pageOf(text),
  layouts: [loadFrame],
  loading: [],
  templates: [],
  slots: [slot],
});

const table = {
  routes: [page("/dashboard", "the dashboard"), page("/dashboard/members", "the members page")],
  notFound: [],
  errors: [],
};

const assets = { scripts: [], styles: [], preloads: [] };

describe("rendering a route that has one", () => {
  it("gives the layout the slot beside its children", async () => {
    // One URL, two subtrees: the page the path names as `children`, and the
    // slot's page for the same path as `team`. This is the whole feature.
    const { prerender } = createRenderer({ App: routerView("./app"), ...table });

    const result = await prerender("/dashboard/members", assets);

    expect(result.html).toContain("the members page");
    expect(result.html).toContain("the team members");
    // Inside the layout, in the place the layout put it: before `children`,
    // because that is where the `<aside>` is written.
    expect(result.html.indexOf("the team members")).toBeLessThan(
      result.html.indexOf("the members page"),
    );
  });

  it("renders the slot's default when the URL says nothing about it", async () => {
    // `/dashboard` matches no route in the slot's table, and the slot has a
    // `$default.js`. Without one there would be a hole in the page, which
    // is the reason the file exists.
    const { prerender } = createRenderer({ App: routerView("./app"), ...table });

    const result = await prerender("/dashboard", assets);

    expect(result.html).toContain("the dashboard");
    expect(result.html).toContain("the team default");
  });

  it("passes the current route params to a slot default", async () => {
    // A default is still rendered *at the current URL*. Under a dynamic
    // segment, the page filling the slot needs the same params the layout that
    // receives it sees, even though no route inside the slot matched.
    component TeamDefault(params: { org?: string }) {
      return <p>{`team default for ${params.org ?? "missing"}`}</p>;
    }
    const dynamicSlot = {
      ...slot,
      defaultPage: () => Promise.resolve({ default: TeamDefault }),
      routes: [],
    };
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [
        {
          ...page("/dashboard/:org", "the org dashboard"),
          params: [{ name: "org", catchAll: false }],
          slots: [dynamicSlot],
        },
      ],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/dashboard/acme", assets);

    expect(result.html).toContain("the org dashboard");
    expect(result.html).toContain("team default for acme");
  });

  it("renders nothing for an unaddressed slot with no default", async () => {
    // The layout still receives the prop — as `null` — so a project can write
    // `{team ?? <Empty />}` and rely on it.
    const withoutDefault = {
      ...table,
      routes: [
        {
          ...page("/dashboard", "the dashboard"),
          slots: [{ ...slot, defaultPage: null }],
        },
      ],
    };
    const { prerender } = createRenderer({ App: routerView("./app"), ...withoutDefault });

    const result = await prerender("/dashboard", assets);

    expect(result.html).toContain("nothing in the slot");
    expect(result.html).not.toContain("the team default");
  });

  it("renders the same tree as before for a route with no slot", async () => {
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: [{ ...page("/dashboard", "the dashboard"), slots: [] }],
      notFound: [],
      errors: [],
    });

    const result = await prerender("/dashboard", assets);

    expect(result.html).toContain("the dashboard");
    expect(result.html).toContain("nothing in the slot");
  });

  it("refuses a slot page that exports a loader, rather than dropping it", async () => {
    // A slot's data has nowhere to be embedded for hydration, so running the
    // loader would fetch again in the browser — and a server-only loader would
    // render one tree on the server and another in the page. Saying so is the
    // point: a convention quietly ignored is what #267 is about.
    const withLoader = {
      ...table,
      routes: [
        {
          ...page("/dashboard", "the dashboard"),
          slots: [
            {
              ...slot,
              defaultPage: () =>
                Promise.resolve({ default: () => <p>x</p>, loader: () => ({ a: 1 }) }),
            },
          ],
        },
      ],
    };

    const resolved = await resolveMatch(withLoader, "/dashboard");

    expect(resolved.status).toBe(500);
    expect(resolved.error?.kind).toBe("thrown");
  });
});

// --- Navigation --------------------------------------------------------

component Go() {
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() => {
        router.push("/dashboard/members", { scroll: false }).catch(() => {});
      }}
    >
      go
    </button>
  );
}

describe("navigating between two routes that share a slot", () => {
  it("matches the slot again and swaps what it holds", async () => {
    // The slot is matched against the URL, so a navigation moves it: the
    // default gives way to the slot's own page for the new path, while the
    // layout holding both is never unmounted.
    createRenderer({ App: routerView("./app"), ...table });
    const resolved = await resolveMatch(table, "/dashboard");

    render(
      <RouterProvider url="/dashboard" initial={resolved}>
        <RouteView />
        <Go />
      </RouterProvider>,
    );
    expect(screen.getByTestId("team").textContent).toContain("the team default");

    await userEvent.click(screen.getByRole("button", { name: "go" }));

    expect(screen.getByText("the members page")).not.toBe(null);
    expect(screen.getByTestId("team").textContent).toContain("the team members");
  });
});
