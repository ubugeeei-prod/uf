// @flow
//
// The server's half of version-skew protection: a request that names another
// build is refused before any of this one runs.
//
// `./internal/deployment.js` argues the rule. What is asserted here is the part
// a regression would break quietly — that *nothing* runs for a refused request:
// not the middleware, not the action, not the render — and that everything a
// browser sends without naming a build is answered exactly as before. The
// browser's half is `packages/router/deployment.test.js`; every front door
// giving the same answer is `tests/library/deploy.test.js`.

import { describe, expect, it } from "@uniflowed/test";

import { DEPLOYMENT_HEADER, createFetchHandler } from "./fetch.js";
import type { Application } from "./fetch.js";
import { beginRequest } from "./host.js";

/** A server bundle that counts what ran. */
function countingApp(): {| app: Application, ran: Array<string> |} {
  const ran: Array<string> = [];
  const app: Application = {
    beginRequest,
    runMiddleware: async () => {
      ran.push("middleware");
      return null;
    },
    callAction: async (request: Request) => {
      if (request.headers.get("uf-action") == null) return null;
      ran.push("action");
      return Response.json({ total: 9 });
    },
    dispatch: async () => null,
    render: async (url: string) => {
      ran.push(`render ${url}`);
      const html = `<!doctype html><p>${url}</p>`;
      return {
        status: 200,
        pipe: () => {},
        stream: () =>
          new ReadableStream({
            start(controller: ReadableStreamDefaultController<Uint8Array>) {
              controller.enqueue(new TextEncoder().encode(html));
              controller.close();
            },
          }),
      };
    },
  };
  return { app, ran };
}

const document = { scripts: [], styles: [], preloads: [], deployment: "build-n1" };

/** Answer `request` the way a host does: inside a request it began. */
async function answer(handle: (request: Request) => Promise<Response>, request: Request) {
  const { run, settle } = beginRequest(request);
  try {
    return await run(() => handle(request));
  } finally {
    await settle();
  }
}

const actionCall = (extra: { [string]: string }) => {
  // Key by key rather than spread after named keys, which Flow cannot type
  // for an indexer. `extra` still wins.
  const headers: { [string]: string } = {
    origin: "http://localhost",
    "content-type": "application/json",
    "uf-action": "a".repeat(64),
  };
  for (const name of Object.keys(extra)) {
    headers[name] = extra[name];
  }
  return new Request("http://localhost/counter", {
    method: "POST",
    headers,
    body: JSON.stringify({ args: [4] }),
  });
};

describe("a request from another build", () => {
  it("is refused with a 409 that names this build, and nothing of this build runs", async () => {
    const { app, ran } = countingApp();
    const handle = createFetchHandler({ app, document });

    const response = await answer(handle, actionCall({ [DEPLOYMENT_HEADER]: "build-n" }));

    expect(response.status).toBe(409);
    expect(response.headers.get(DEPLOYMENT_HEADER)).toBe("build-n1");
    // Never kept: a cached refusal would send a tab on this build away too.
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(ran).toEqual([]);
  });

  it("is refused for a payload request too", async () => {
    const { app, ran } = countingApp();
    const handle = createFetchHandler({ app, document });

    const response = await answer(
      handle,
      new Request("http://localhost/guide/__uf.flight", {
        headers: { accept: "text/x-component", [DEPLOYMENT_HEADER]: "build-n" },
      }),
    );

    expect(response.status).toBe(409);
    // Not a payload, which is what makes the router load the document.
    expect(response.headers.get("content-type")).toBe("text/plain; charset=utf-8");
    expect(ran).toEqual([]);
  });
});

describe("every other request", () => {
  it("runs as it always did when it names this build", async () => {
    const { app, ran } = countingApp();
    const handle = createFetchHandler({ app, document });

    const response = await answer(handle, actionCall({ [DEPLOYMENT_HEADER]: "build-n1" }));

    expect(response.status).toBe(200);
    expect(ran).toEqual(["middleware", "action"]);
  });

  it("runs as it always did when it names no build", async () => {
    // `curl`, a crawler, a page from before the id existed.
    const { app, ran } = countingApp();
    const handle = createFetchHandler({ app, document });

    const response = await answer(handle, actionCall({}));

    expect(response.status).toBe(200);
    expect(ran).toEqual(["middleware", "action"]);
  });

  it("is never compared by a build that has no id", async () => {
    // `uf dev`: there is no other build to be skewed against.
    const { app, ran } = countingApp();
    const handle = createFetchHandler({
      app,
      document: { scripts: [], styles: [], preloads: [] },
    });

    const response = await answer(handle, actionCall({ [DEPLOYMENT_HEADER]: "build-n" }));

    expect(response.status).toBe(200);
    expect(ran).toEqual(["middleware", "action"]);
  });
});
