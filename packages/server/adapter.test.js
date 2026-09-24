// @flow
//
// `@uniflowed/server/adapter`: the contract, held against the handler a build
// writes.
//
// A platform outside this repository writes the wrapper and nothing else, so
// the two things worth asserting are the two it relies on: that a module is
// recognised as a uf handler or refused by name, and that `answerWith` answers
// the way the hosts in this repository do — one request, begun by the
// handler's own `beginRequest`, settled where the host says, and a bare `500`
// for a throw. `tests/library/deploy.test.js` is where the six in-repository
// hosts are compared with each other.

import { describe, expect, it } from "@uniflowed/test";

import { HANDLER_FILE, STATIC_DIRECTORY, answerWith, handlerModule } from "./adapter.js";
import { installLogger, recordingLogger } from "./log.js";

/** A handler module shaped the way `uf build --adapter` writes one. */
function writtenHandler(answer: (request: Request) => Promise<Response>) {
  const events: Array<string> = [];
  const beginRequest = (request: Request) => {
    events.push(`begin ${new URL(request.url).pathname}`);
    return {
      context: {} as $FlowFixMe,
      run: async <T>(body: () => Promise<T>): Promise<T> => {
        events.push("run");
        return await body();
      },
      settle: async () => {
        events.push("settle");
      },
    } as $FlowFixMe;
  };
  const fetch = async (request: Request) => {
    events.push("fetch");
    return await answer(request);
  };
  const module = {
    fetch,
    beginRequest,
    routing: {},
    default: { fetch, beginRequest, routing: {} },
  };
  return { module, events };
}

describe("the module a host loads", () => {
  it("is where the contract says it is", () => {
    expect(HANDLER_FILE).toBe("handler.js");
    expect(STATIC_DIRECTORY).toBe("static");
  });

  it("is recognised as a uf handler by its default export", () => {
    const { module } = writtenHandler(async () => new Response("ok"));
    const handler = handlerModule(module);
    expect(typeof handler.fetch).toBe("function");
    expect(typeof handler.beginRequest).toBe("function");
  });

  it("is refused by name when it is the wrapper rather than the handler", () => {
    // `worker.js`'s default export has a `fetch` and no `beginRequest`.
    const refusal = (() => {
      try {
        handlerModule({ default: { fetch: async () => new Response("ok") } });
        return "";
      } catch (error) {
        return String(error);
      }
    })();
    expect(refusal).toContain("`beginRequest` is not a function");
    expect(refusal).toContain("handler.js");
  });
});

describe("answering through the contract", () => {
  it("begins, runs and hands the settle to the host, in that order", async () => {
    const { module, events } = writtenHandler(async () => new Response("ok"));
    const answer = answerWith(handlerModule(module));
    const settles: Array<() => Promise<void>> = [];

    const response = await answer(new Request("http://localhost/guide"), (settle) => {
      settles.push(settle);
    });

    expect(await response.text()).toBe("ok");
    // Not settled yet: the host has not said the body is out.
    expect(events).toEqual(["begin /guide", "run", "fetch"]);
    await settles[0]();
    expect(events).toEqual(["begin /guide", "run", "fetch", "settle"]);
  });

  it("answers a throw with the bare 500 every host writes, and logs it", async () => {
    const { logger, records } = recordingLogger();
    installLogger(logger);
    try {
      const { module } = writtenHandler(async () => {
        throw new Error("the database is on fire at 0x7f4a2b10");
      });
      const answer = answerWith(handlerModule(module));
      const settles: Array<() => Promise<void>> = [];

      const response = await answer(new Request("http://localhost/api"), (settle) => {
        settles.push(settle);
      });

      expect(response.status).toBe(500);
      expect(await response.text()).toBe("500 Internal Server Error\n");
      expect(records.some((record) => record.level === "error")).toBe(true);
      // A request that failed still happened, so its settle is still owed.
      expect(settles.length).toBe(1);
    } finally {
      installLogger(null);
    }
  });
});
