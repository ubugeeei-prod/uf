// @flow
//
// Answering a browser that asks for a route's payload.
//
// `internal/flight.js`'s `flightResponse` is what every front door calls for
// `<route>/__uf.flight`, so what it reports and what it declines are decided
// once, here. See ubugeeei-prod/uf#519.

import { describe, expect, it } from "@uniflowed/test";

import { INTERCEPTED_FROM_HEADER, flightResponse } from "./internal/flight.js";

/** A server bundle whose `flight` answers with `answer`, and records its URL. */
function appAnswering(answer: mixed): $FlowFixMe {
  const asked: Array<string> = [];
  const options: Array<$FlowFixMe> = [];
  return {
    asked,
    options,
    flight: async (url: string, flightOptions: mixed) => {
      asked.push(url);
      options.push(flightOptions);
      return answer;
    },
  };
}

describe("answering a payload request", () => {
  it("reports the failure a route resolved to its error boundary for", async () => {
    // A loader that threw is caught while the route resolves, before anything
    // renders, so nothing else ever tells the host about it.
    const failure = new Error("the loader failed");
    const app = appAnswering({
      status: 500,
      headers: { "content-type": "text/x-component" },
      stream: null,
      error: failure,
    });
    const reported: Array<mixed> = [];

    const response = await flightResponse(app, new Request("http://uf.test/guide/__uf.flight"), {
      onError: (error) => {
        reported.push(error);
      },
    });

    expect(response?.status).toBe(500);
    expect(reported).toEqual([failure]);
    expect(app.asked).toEqual(["/guide"]);
  });

  it("reports nothing for a route that rendered", async () => {
    const app = appAnswering({ status: 200, headers: {}, stream: null });
    const reported: Array<mixed> = [];

    await flightResponse(app, new Request("http://uf.test/__uf.flight?tab=2"), {
      onError: (error) => {
        reported.push(error);
      },
    });

    expect(reported).toEqual([]);
    expect(app.asked).toEqual(["/?tab=2"]);
  });

  it("declines a request that is not for a payload", async () => {
    const app = appAnswering({ status: 200, headers: {}, stream: null });

    const response = await flightResponse(app, new Request("http://uf.test/guide"), {
      onError: () => {},
    });

    expect(response).toBe(null);
    expect(app.asked).toEqual([]);
  });

  it("passes the intercepted-from page to the renderer", async () => {
    const app = appAnswering({ status: 200, headers: {}, stream: null });

    await flightResponse(
      app,
      new Request("http://uf.test/feed/photo/1/__uf.flight", {
        headers: { [INTERCEPTED_FROM_HEADER]: "/feed" },
      }),
      {
        onError: () => {},
      },
    );

    expect(app.asked).toEqual(["/feed/photo/1"]);
    expect(app.options.map((option) => option?.interceptedFrom)).toEqual(["/feed"]);
  });
});
