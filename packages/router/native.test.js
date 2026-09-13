// @flow
//
// React Native uses the route table, not the browser history API.

import {
  NativeNavigationError,
  createNativeRouter,
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

    expect(
      nativeScreenPayload(event, {
        "/": "Home",
        "/users/:id": "UserProfile",
      }),
    ).toEqual({
      screen: "UserProfile",
      href: "/users/42?tab=posts",
      pathname: "/users/42",
      search: "?tab=posts",
      route: "/users/:id",
      params: { id: "42" },
    });
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

  it("reports a missing navigator method before silently dropping a navigation", async () => {
    const router = createNativeRouter(table(), { replace: () => {} });

    await expect(router.push("/users/42")).rejects.toThrow(/does not implement push/);
  });
});
