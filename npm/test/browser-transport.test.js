// @flow
//
// The browser runner's own requests do not go through a test's request mock.
// ubugeeei-prod/uf#1394.
//
// A page reports results and drives `createBrowser` over `fetch`, and the file
// under test runs in that page. A file that calls `api.listen()` owns
// `globalThis.fetch` from then on, and under `@uniflowed/mock`'s default policy
// it rejects any request no handler claimed — the runner's included. These
// drive the page-side transport on Node, with a mock listening, and with the
// `fetch` the harness would have captured stood in for by a recording one: no
// browser is needed to see which `fetch` a request went through.

import { describe, expect, it } from "@uniflowed/test";
import { mock } from "@uniflowed/mock";

import { createTransport } from "./internal/browser/page-transport.js";
import { RUNNER_FETCH, runnerFetch } from "./internal/browser/runner-fetch.js";

const TOKEN = Symbol.for("uf.test.browser.token");

/**
 * Run `body` as a page served by `uf test --browser` would: a token, and a
 * captured `fetch` that answers every command with `value`.
 */
async function asHarness(
  value: string,
  body: (seen: Array<string>) => Promise<void>,
): Promise<void> {
  const seen: Array<string> = [];
  const captured = async (input: RequestInfo): Promise<Response> => {
    seen.push(input instanceof Request ? input.url : String(input));
    return Response.json({ value });
  };
  Reflect.set(globalThis, TOKEN, "token");
  Reflect.set(globalThis, RUNNER_FETCH, captured);
  try {
    await body(seen);
  } finally {
    Reflect.deleteProperty(globalThis, TOKEN);
    Reflect.deleteProperty(globalThis, RUNNER_FETCH);
  }
}

describe("the browser runner's transport", () => {
  it("reaches uf while a request mock is listening with the default policy", async () => {
    await asHarness("page-1", async (seen) => {
      const api = mock();
      api.listen({ origin: "http://127.0.0.1" });
      try {
        const transport = await createTransport({});
        expect(transport.id).toBe("page-1");
        expect(seen).toEqual(["/uf-test/browser"]);
        // The mock never saw it: not answered, not recorded, not rejected.
        expect(api.requests.length).toBe(0);
      } finally {
        api.close();
      }
    });
  });

  it("uses the platform's fetch when no harness captured one", async () => {
    const api = mock();
    api.listen({ origin: "http://127.0.0.1" });
    try {
      let raised: mixed = null;
      try {
        await runnerFetch("http://127.0.0.1/anything");
      } catch (error) {
        raised = error;
      }
      // Outside a harness there is nothing to bypass the mock with, so the
      // global — here, the mock — is what answers.
      expect(raised instanceof Error && raised.name).toBe("UnhandledRequestError");
    } finally {
      api.close();
    }
  });
});
