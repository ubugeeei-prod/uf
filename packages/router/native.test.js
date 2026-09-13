// @flow
//
// React Native uses the route table, not the browser history API.

import {
  NativeNavigationError,
  createNativeScreenManifest,
  createNativeRouter,
  createNativeScreenRouter,
  nativeScreenName,
  nativeScreenNavigationState,
  nativeScreenPayload,
  resolveNativeNavigation,
} from "@uniflowed/router/native";
import { describe, expect, it } from "@uniflowed/test";

function module(name: string): () => Promise<{| readonly default: string |}> {
  return () => Promise.resolve({ default: name });
}

function table() {
  return {
    routes: [
      {
        path: "/",
        params: [],
        mdx: false,
        file: "app/$page.native.js",
        page: module("home"),
        layouts: [],
      },
      {
        path: "/users/:id",
        params: [{ name: "id", catchAll: false }],
        mdx: false,
        file: "app/users/[id]/$page.native.js",
        page: module("user"),
        layouts: [module("layout")],
      },
      {
        path: "/server-only",
        params: [],
        mdx: false,
        file: "app/server-only/$page.js",
        layouts: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("@uniflowed/router/native", () => {
  it("resolves a route table entry into a native navigation event", () => {
    expect(resolveNativeNavigation(table(), "/users/42?tab=posts", "replace")).toEqual({
      kind: "replace",
      href: "/users/42?tab=posts",
      pathname: "/users/42",
      search: "?tab=posts",
      route: "/users/:id",
      params: { id: "42" },
    });
  });

  it("maps a native route event into an app-owned screen payload", () => {
    const event = resolveNativeNavigation(table(), "/users/42?tab=posts", "replace");

    const payload = nativeScreenPayload(event, {
      "/": "Home",
      "/users/:id": "UserProfile",
    });

    expect(payload).toEqual({
      screen: "UserProfile",
      href: "/users/42?tab=posts",
      pathname: "/users/42",
      search: "?tab=posts",
      route: "/users/:id",
      params: { id: "42" },
    });
    expect(nativeScreenNavigationState(payload)).toEqual({
      params: { id: "42" },
      href: "/users/42?tab=posts",
      pathname: "/users/42",
      search: "?tab=posts",
      route: "/users/:id",
    });
  });

  it("derives a native screen manifest from the route table", () => {
    const manifest = createNativeScreenManifest(table(), {
      name: (route) => (route.path === "/users/:id" ? "UserProfile" : nativeScreenName(route.path)),
    });

    expect(manifest).toEqual({
      screens: {
        "/": "Home",
        "/users/:id": "UserProfile",
      },
      entries: [
        { screen: "Home", route: "/", file: "app/$page.native.js" },
        {
          screen: "UserProfile",
          route: "/users/:id",
          file: "app/users/[id]/$page.native.js",
        },
      ],
    });
  });

  it("uses the generated screen manifest with the native screen router", async () => {
    const events = [];
    const manifest = createNativeScreenManifest(table(), {
      name: (route) => (route.path === "/users/:id" ? "UserProfile" : nativeScreenName(route.path)),
    });
    const router = createNativeScreenRouter(table(), manifest.screens, {
      push: (screen, state) => {
        events.push([screen, state.href, state.params]);
      },
    });

    await router.push("/users/42?tab=posts");

    expect(events).toEqual([["UserProfile", "/users/42?tab=posts", { id: "42" }]]);
  });

  it("names routes predictably when an app does not supply screen names", () => {
    expect(nativeScreenName("/")).toBe("Home");
    expect(nativeScreenName("/settings")).toBe("Settings");
    expect(nativeScreenName("/users/:id")).toBe("UsersById");
    expect(nativeScreenName("/docs/:slug*")).toBe("DocsAllSlug");
  });

  it("refuses duplicate generated native screen names", () => {
    let thrown: ?NativeNavigationError = null;
    try {
      createNativeScreenManifest(table(), { name: () => "Screen" });
    } catch (error) {
      if (error instanceof NativeNavigationError) {
        thrown = error;
      }
    }

    expect(thrown?.code).toBe("duplicate-screen");
    expect(thrown?.route).toBe("/users/:id");
  });

  it("refuses empty generated native screen names", () => {
    let thrown: ?NativeNavigationError = null;
    try {
      createNativeScreenManifest(table(), { name: () => "" });
    } catch (error) {
      if (error instanceof NativeNavigationError) {
        thrown = error;
      }
    }

    expect(thrown?.code).toBe("missing-screen");
    expect(thrown?.route).toBe("/");
  });

  it("refuses a native route event whose screen mapping is missing", () => {
    const event = resolveNativeNavigation(table(), "/users/42");
    let thrown: ?NativeNavigationError = null;
    try {
      nativeScreenPayload(event, { "/": "Home" });
    } catch (error) {
      if (error instanceof NativeNavigationError) {
        thrown = error;
      }
    }

    expect(thrown?.code).toBe("missing-screen");
    expect(thrown?.route).toBe("/users/:id");
    expect(thrown?.href).toBe("/users/42");
  });

  it("refuses destinations a native navigator cannot own", () => {
    expect(() => resolveNativeNavigation(table(), "settings")).toThrow(/relative/);
    expect(() => resolveNativeNavigation(table(), "https://example.com/users/42")).toThrow(
      /external URL/,
    );
    expect(() => resolveNativeNavigation(table(), "/users/42#bio")).toThrow(/fragment/);
  });

  it("refuses a path outside the generated native table", () => {
    expect(() => resolveNativeNavigation(table(), "/missing")).toThrow(/does not match/);
  });

  it("refuses a route that has no native page module", () => {
    let thrown: ?NativeNavigationError = null;
    try {
      resolveNativeNavigation(table(), "/server-only");
    } catch (error) {
      if (error instanceof NativeNavigationError) {
        thrown = error;
      }
    }

    expect(thrown?.code).toBe("server-only-route");
    expect(thrown?.route).toBe("/server-only");
  });

  it("hands push and replace to the native navigator", async () => {
    const events = [];
    const screens = { "/users/:id": "UserProfile" };
    const router = createNativeRouter(table(), {
      push: (event) => {
        const payload = nativeScreenPayload(event, screens);
        events.push(["push", payload.screen, payload.href, payload.params]);
      },
      replace: (event) => {
        const payload = nativeScreenPayload(event, screens);
        events.push(["replace", payload.screen, payload.href, payload.params]);
      },
    });

    await router.push("/users/1");
    await router.replace("/users/2?tab=posts");

    expect(events).toEqual([
      ["push", "UserProfile", "/users/1", { id: "1" }],
      ["replace", "UserProfile", "/users/2?tab=posts", { id: "2" }],
    ]);
  });

  it("hands screen names and route state to an app-owned native navigator", async () => {
    const events = [];
    const router = createNativeScreenRouter(
      table(),
      {
        "/users/:id": "UserProfile",
      },
      {
        push: (screen, state) => {
          events.push(["push", screen, state.href, state.params]);
        },
        replace: (screen, state) => {
          events.push(["replace", screen, state.href, state.params]);
        },
      },
    );

    await router.push("/users/1");
    await router.replace("/users/2?tab=posts");

    expect(events).toEqual([
      ["push", "UserProfile", "/users/1", { id: "1" }],
      ["replace", "UserProfile", "/users/2?tab=posts", { id: "2" }],
    ]);
  });

  it("loads the matched modules before a native prefetch callback", async () => {
    const loaded = [];
    const routes = table().routes.map((route) =>
      route.path === "/users/:id"
        ? {
            ...route,
            page: () => {
              loaded.push("page");
              return Promise.resolve({ default: "user" });
            },
            layouts: [
              () => {
                loaded.push("layout");
                return Promise.resolve({ default: "layout" });
              },
            ],
          }
        : route,
    );
    const events = [];
    const router = createNativeRouter(
      { ...table(), routes },
      {
        prefetch: (event) => {
          events.push(event.href);
        },
      },
    );

    await router.prefetch("/users/42");

    expect(loaded.sort()).toEqual(["layout", "page"]);
    expect(events).toEqual(["/users/42"]);
  });

  it("loads modules before a native screen prefetch callback", async () => {
    const loaded = [];
    const routes = table().routes.map((route) =>
      route.path === "/users/:id"
        ? {
            ...route,
            page: () => {
              loaded.push("page");
              return Promise.resolve({ default: "user" });
            },
            layouts: [
              () => {
                loaded.push("layout");
                return Promise.resolve({ default: "layout" });
              },
            ],
          }
        : route,
    );
    const events = [];
    const router = createNativeScreenRouter(
      { ...table(), routes },
      { "/users/:id": "UserProfile" },
      {
        prefetch: (screen, state) => {
          events.push([screen, state.href]);
        },
      },
    );

    await router.prefetch("/users/42");

    expect(loaded.sort()).toEqual(["layout", "page"]);
    expect(events).toEqual([["UserProfile", "/users/42"]]);
  });

  it("reports a missing navigator method before silently dropping a navigation", async () => {
    const router = createNativeRouter(table(), { replace: () => {} });

    await expect(router.push("/users/42")).rejects.toThrow(/does not implement push/);
  });

  it("reports a missing native screen navigator method", async () => {
    const router = createNativeScreenRouter(table(), { "/users/:id": "UserProfile" }, {});

    await expect(router.push("/users/42")).rejects.toThrow(/does not implement push/);
  });
});
