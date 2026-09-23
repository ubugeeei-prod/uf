// @flow
import { describe, expect, it } from "@uniflowed/test";
import { authorizeNativeAction } from "@uniflowed/server";
import { beginRequest } from "@uniflowed/server/host";
import { createActionDispatcher } from "./server.js";
import { createNativeActionClient, createRouteClient } from "./http-client.js";
import { createDispatcher } from "./handler.js";
import type { HandlerContext } from "./handler.js";

const ID = "a".repeat(64);
const dispatcher = createActionDispatcher({
  actions: [
    {
      id: ID,
      module: "app/actions.js",
      export: "increment",
      load: async () => ({ increment: async (value: number) => value + 1 }),
    },
  ],
});

/** `base`, with `extra`'s headers over it. */
function withExtra(
  base: { readonly [string]: string },
  extra: { readonly [string]: string },
): { [string]: string } {
  const headers: { [string]: string } = {};
  for (const name of Object.keys(base)) {
    headers[name] = base[name];
  }
  for (const name of Object.keys(extra)) {
    headers[name] = extra[name];
  }
  return headers;
}

function request(extra: { readonly [string]: string } = {}) {
  return new Request("https://app.test/counter", {
    method: "POST",
    headers: withExtra(
      {
        "uf-action": ID,
        "uf-native-action": "bearer-v1",
        authorization: "Bearer valid-token",
        "content-type": "application/json",
      },
      extra,
    ),
    body: '{"args":[41]}',
  });
}

async function hosted(input: Request, authorize: boolean = true) {
  const lifecycle = beginRequest(input);
  try {
    return await lifecycle.run(async () => {
      if (authorize)
        await authorizeNativeAction(
          input,
          async (token, id) => token === "valid-token" && id === ID,
        );
      const response = await dispatcher(input);
      if (response == null) throw new Error("the action was not dispatched");
      return response;
    });
  } finally {
    await lifecycle.settle();
  }
}

describe("native bearer actions", () => {
  it("calls the action through the client and the application's verifier", async () => {
    const call = createNativeActionClient({
      origin: "https://app.test",
      getToken: async () => "valid-token",
      fetch: async (url, options) => {
        expect(options.credentials).toBe("omit");
        expect(options.redirect).toBe("error");
        return hosted(new Request(url, { ...options, headers: { ...options.headers } }));
      },
    });
    expect(await call(ID, [41], "/counter")).toBe(42);
  });

  it("refuses absent authorization, invalid credentials and browser headers", async () => {
    expect((await hosted(request(), false)).status).toBe(403);
    for (const headers of [
      { authorization: "Bearer invalid" },
      { origin: "https://app.test" },
      { origin: "https://hostile.test" },
      { cookie: "session=ambient" },
      { cookie: "" },
      { "sec-fetch-site": "none" },
      { "sec-fetch-mode": "cors" },
    ])
      expect((await hosted(request(headers))).status).toBe(403);
  });

  it("does not accept a browser action without Origin", async () => {
    const input = request();
    input.headers.delete("uf-native-action");
    expect((await hosted(input)).status).toBe(403);
    input.headers.set("origin", "https://app.test");
    input.headers.set("host", "app.test");
    expect((await hosted(input)).status).toBe(200);
  });

  it("does not transfer proof to another Request, changed header or request scope", async () => {
    const first = request();
    const lifecycle = beginRequest(first);
    await lifecycle.run(async () => {
      expect(await authorizeNativeAction(first, async () => true)).toBe(true);
      expect((await dispatcher(request()))?.status).toBe(403);
      first.headers.set("authorization", "Bearer replacement");
      expect((await dispatcher(first))?.status).toBe(403);
    });
    expect((await hosted(request(), false)).status).toBe(403);
  });

  it("fails closed when the verifier throws and keeps the JSON boundary", async () => {
    const input = request();
    await beginRequest(input).run(async () => {
      expect(
        await authorizeNativeAction(input, async () => {
          throw new Error("invalid signature");
        }),
      ).toBe(false);
      expect((await dispatcher(input))?.status).toBe(403);
    });
    expect((await hosted(request({ "content-type": "text/plain" }))).status).toBe(415);
  });
});

describe("native route transport", () => {
  it("calls a BFF route handler with decoded parameters and explicit authorization", async () => {
    const dispatch = createDispatcher({
      handlers: [
        {
          path: "/api/users/:id",
          params: [{ name: "id", catchAll: false }],
          file: "app/api/users/[id]/$route.js",
          load: async () => ({
            GET: (request: Request, { params }: HandlerContext) =>
              Response.json({
                id: params.id,
                token: request.headers.get("authorization"),
              }),
          }),
        },
      ],
    });
    const client = createRouteClient({
      origin: "https://app.test",
      getToken: async () => "app-token",
      fetch: async (url, options) => {
        const input = new Request(url, { ...options, headers: { ...options.headers } });
        const response = await beginRequest(input).run(() => dispatch(input));
        if (response == null) throw new Error("route not found");
        return response;
      },
    });
    expect(await (await client("/api/users/a%20b")).json()).toEqual({
      id: "a b",
      token: "Bearer app-token",
    });
  });
  it("refuses origin escapes and caller-supplied ambient credentials", async () => {
    const client = createRouteClient({
      origin: "https://app.test",
      fetch: async () => {
        throw new Error("network must not be called");
      },
    });
    for (const path of ["//elsewhere.test/path", "https://elsewhere.test", "/\\elsewhere.test"]) {
      await expect(client(path)).rejects.toThrow();
    }
    for (const header of ["Cookie", "Authorization", "Origin", "Sec-Fetch-Site"]) {
      await expect(client("/api", { headers: { [header]: "bad" } })).rejects.toThrow();
    }
    expect(() => createRouteClient({ origin: "http://public.test" })).toThrow("HTTPS");
  });

  it("sends each request's current credential and preserves encoded route parameters", async () => {
    let token = "first";
    const seen = [];
    const client = createRouteClient({
      origin: "https://app.test",
      getToken: async () => token,
      fetch: async (url, options) => {
        seen.push([url, options.headers?.authorization]);
        return new Response("ok");
      },
    });
    await client("/api/users/a%20b", { method: "GET" });
    token = "second";
    await client("/api/users/c", { method: "GET" });
    expect(seen).toEqual([
      ["https://app.test/api/users/a%20b", "Bearer first"],
      ["https://app.test/api/users/c", "Bearer second"],
    ]);
  });
});
