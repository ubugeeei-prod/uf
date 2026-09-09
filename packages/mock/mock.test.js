// @flow
//
// `@uniflowed/mock`.
//
// Every test here makes a real `fetch` call and asserts on what came back,
// because that is the only thing that proves the interception works: a test
// that called the dispatcher directly would pass with the global untouched.
//
// The network is stood in for rather than reached. `withNetwork` puts a
// recording `fetch` on `globalThis` *before* `listen()`, so the registry
// captures that one as the platform's — which is what makes `passthrough()`
// and `onUnhandledRequest: "bypass"` testable without a socket.

// uf-lint-disable fetch/no-global-override
//
// The rule is right about application code and wrong about this file: standing
// in for the network, and asserting that `close()` put the platform's `fetch`
// back, both require naming the global the package under test replaces.

import { afterAll, afterEach, beforeAll, describe, expect, it } from "@uniflowed/test";
import {
  HttpResponse,
  UnhandledRequestError,
  delay,
  http,
  mock,
  passthrough,
} from "@uniflowed/mock";

/** Run `body` with a listening registry, closed however it ends. */
async function withMock(handlers, options, body) {
  const api = mock(...handlers);
  api.listen(options);
  try {
    await body(api);
  } finally {
    api.close();
  }
}

/**
 * Stand in for the network, so a passthrough has somewhere to go.
 *
 * Installed before `listen()` on purpose: the interceptor captures whatever
 * `fetch` it replaced, and that is the function a bypassed request reaches.
 */
function withNetwork(answer) {
  const original = globalThis.fetch;
  const urls = [];
  globalThis.fetch = async (input) => {
    urls.push(input instanceof Request ? input.url : String(input));
    return answer();
  };
  return {
    urls,
    restore() {
      globalThis.fetch = original;
    },
  };
}

describe("answering", () => {
  it("answers a request the suite declared", async () => {
    await withMock(
      [http.get("https://api.test/health", () => HttpResponse.json({ ok: true }))],
      undefined,
      async () => {
        const response = await fetch("https://api.test/health");
        expect(response.status).toBe(200);
        expect(await response.json()).toEqual({ ok: true });
      },
    );
  });

  it("hands back a real Response, not something shaped like one", async () => {
    await withMock(
      [http.get("https://api.test/health", () => HttpResponse.json({ ok: true }))],
      undefined,
      async () => {
        const response = await fetch("https://api.test/health");
        expect(response instanceof Response).toBe(true);
        // A look-alike would fall over here: `clone` is the operation that
        // needs a real body stream behind it.
        expect(await response.clone().json()).toEqual({ ok: true });
        expect(response.headers.get("content-type")).toBe("application/json");
      },
    );
  });

  it("carries a status and a text body", async () => {
    await withMock(
      [http.get("https://api.test/gone", () => HttpResponse.text("no", { status: 410 }))],
      undefined,
      async () => {
        const response = await fetch("https://api.test/gone");
        expect(response.status).toBe(410);
        expect(response.ok).toBe(false);
        expect(await response.text()).toBe("no");
      },
    );
  });

  it("leaves a caller's content type alone", async () => {
    await withMock(
      [
        http.get("https://api.test/problem", () =>
          HttpResponse.json(
            { title: "nope" },
            {
              status: 422,
              headers: { "content-type": "application/problem+json" },
            },
          ),
        ),
      ],
      undefined,
      async () => {
        const response = await fetch("https://api.test/problem");
        expect(response.headers.get("content-type")).toBe("application/problem+json");
      },
    );
  });

  it("reads the request body in the resolver", async () => {
    await withMock(
      [
        http.post("https://api.test/users", async ({ request }) => {
          const body = await request.json();
          return HttpResponse.json({ id: "1", name: body.name }, { status: 201 });
        }),
      ],
      undefined,
      async () => {
        const response = await fetch("https://api.test/users", {
          method: "POST",
          body: JSON.stringify({ name: "ada" }),
        });
        expect(response.status).toBe(201);
        expect(await response.json()).toEqual({ id: "1", name: "ada" });
      },
    );
  });

  it("does not let a GET handler answer a POST", async () => {
    await withMock(
      [http.get("https://api.test/users", () => HttpResponse.json([]))],
      undefined,
      async () => {
        await expect(fetch("https://api.test/users", { method: "POST" })).rejects.toBeInstanceOf(
          UnhandledRequestError,
        );
      },
    );
  });

  it("answers any method through http.all", async () => {
    await withMock(
      [http.all("https://api.test/anything", ({ request }) => HttpResponse.text(request.method))],
      undefined,
      async () => {
        expect(await (await fetch("https://api.test/anything")).text()).toBe("GET");
        const posted = await fetch("https://api.test/anything", { method: "POST" });
        expect(await posted.text()).toBe("POST");
      },
    );
  });

  it("tries the next handler when a resolver returns nothing", async () => {
    await withMock(
      [
        http.post("https://api.test/users", async ({ request }) => {
          const body = await request.json();
          // Only claims the requests it recognises; the rest fall through.
          return body.name === "ada" ? HttpResponse.json({ known: true }) : undefined;
        }),
        http.post("https://api.test/users", () => HttpResponse.json({ known: false })),
      ],
      undefined,
      async () => {
        const known = await fetch("https://api.test/users", {
          method: "POST",
          body: JSON.stringify({ name: "ada" }),
        });
        expect(await known.json()).toEqual({ known: true });

        const other = await fetch("https://api.test/users", {
          method: "POST",
          body: JSON.stringify({ name: "grace" }),
        });
        expect(await other.json()).toEqual({ known: false });
      },
    );
  });

  it("does not hand one response to two requests that overlap", async () => {
    // The one-time handler is reserved before its resolver is awaited. Without
    // that, two requests in flight at the same time both find it unspent and
    // both get the response it was written to give once.
    await withMock(
      [
        http.get(
          "https://api.test/token",
          async () => {
            await new Promise((resolve) => setTimeout(resolve, 20));
            return HttpResponse.json({ token: "first" });
          },
          { once: true },
        ),
        http.get("https://api.test/token", () => HttpResponse.json({ token: "second" })),
      ],
      undefined,
      async () => {
        const [one, two] = await Promise.all([
          fetch("https://api.test/token").then((answer) => answer.json()),
          fetch("https://api.test/token").then((answer) => answer.json()),
        ]);

        expect([one.token, two.token].sort()).toEqual(["first", "second"]);
      },
    );
  });

  it("puts a once handler back when its resolver declines", async () => {
    // Reserving before the await must not spend a handler that then says the
    // request was not its after all.
    let asked = 0;
    await withMock(
      [
        http.get(
          "https://api.test/maybe",
          () => {
            asked += 1;
            return asked === 1 ? null : HttpResponse.json({ from: "once" });
          },
          { once: true },
        ),
        http.get("https://api.test/maybe", () => HttpResponse.json({ from: "fallback" })),
      ],
      undefined,
      async () => {
        expect(await (await fetch("https://api.test/maybe")).json()).toEqual({ from: "fallback" });
        expect(await (await fetch("https://api.test/maybe")).json()).toEqual({ from: "once" });
      },
    );
  });

  it("spends a once handler and moves on to the next", async () => {
    await withMock(
      [
        http.get("https://api.test/flaky", () => HttpResponse.text("boom", { status: 500 }), {
          once: true,
        }),
        http.get("https://api.test/flaky", () => HttpResponse.json({ ok: true })),
      ],
      undefined,
      async () => {
        expect((await fetch("https://api.test/flaky")).status).toBe(500);
        expect((await fetch("https://api.test/flaky")).status).toBe(200);
        expect((await fetch("https://api.test/flaky")).status).toBe(200);
      },
    );
  });
});

describe("paths", () => {
  it("captures a path parameter", async () => {
    await withMock(
      [
        http.get("https://api.test/users/:id", ({ params }) =>
          HttpResponse.json({ id: params.id }),
        ),
      ],
      undefined,
      async () => {
        const response = await fetch("https://api.test/users/42");
        expect(await response.json()).toEqual({ id: "42" });
      },
    );
  });

  it("captures more than one, and percent-decodes them", async () => {
    await withMock(
      [
        http.get("/orgs/:org/repos/:repo", ({ params }) =>
          HttpResponse.json({ org: params.org, repo: params.repo }),
        ),
      ],
      { origin: "https://api.test" },
      async () => {
        const response = await fetch("https://api.test/orgs/uf%20labs/repos/mock");
        expect(await response.json()).toEqual({ org: "uf labs", repo: "mock" });
      },
    );
  });

  it("gives a trailing wildcard the rest of the path", async () => {
    await withMock(
      [http.get("/files/*", ({ params }) => HttpResponse.json({ rest: params["*"] }))],
      { origin: "https://api.test" },
      async () => {
        expect(await (await fetch("https://api.test/files/a/b/c.txt")).json()).toEqual({
          rest: "a/b/c.txt",
        });
        // Zero segments match too, which is what makes `/files/*` a prefix
        // rather than "at least one more segment".
        expect(await (await fetch("https://api.test/files")).json()).toEqual({ rest: "" });
      },
    );
  });

  it("also names the wildcard the way MSW does, so ported code reads", async () => {
    await withMock(
      [http.get("/files/*", ({ params }) => HttpResponse.text(params["0"]))],
      { origin: "https://api.test" },
      async () => {
        expect(await (await fetch("https://api.test/files/report.pdf")).text()).toBe("report.pdf");
      },
    );
  });

  it("matches any origin when the pattern names none", async () => {
    await withMock(
      [http.get("/ping", ({ request }) => HttpResponse.text(new URL(request.url).origin))],
      undefined,
      async () => {
        expect(await (await fetch("https://one.test/ping")).text()).toBe("https://one.test");
        expect(await (await fetch("https://two.test/ping")).text()).toBe("https://two.test");
      },
    );
  });

  it("matches only the origin the pattern names", async () => {
    await withMock(
      [http.get("https://one.test/ping", () => HttpResponse.text("one"))],
      undefined,
      async () => {
        expect(await (await fetch("https://one.test/ping")).text()).toBe("one");
        await expect(fetch("https://two.test/ping")).rejects.toBeInstanceOf(UnhandledRequestError);
      },
    );
  });

  it("does not care about a trailing slash on either side", async () => {
    await withMock(
      [http.get("/users/", () => HttpResponse.text("ok"))],
      { origin: "https://api.test" },
      async () => {
        expect(await (await fetch("https://api.test/users")).text()).toBe("ok");
        expect(await (await fetch("https://api.test/users/")).text()).toBe("ok");
      },
    );
  });

  it("ignores a query string in the pattern and parses the one in the URL", async () => {
    await withMock(
      [http.get("/search?q=ignored", ({ query }) => HttpResponse.text(query.get("q") ?? ""))],
      { origin: "https://api.test" },
      async () => {
        expect(await (await fetch("https://api.test/search?q=flow")).text()).toBe("flow");
      },
    );
  });

  it("resolves a relative URL, which the host's own fetch cannot", async () => {
    await withMock(
      [http.get("/api/users", ({ request }) => HttpResponse.text(request.url))],
      { origin: "https://api.test" },
      async () => {
        // `fetch("/api/users")` throws on Node, because there is no document to
        // resolve against. Replacing `fetch` outright is what makes this work.
        expect(await (await fetch("/api/users")).text()).toBe("https://api.test/api/users");
      },
    );
  });
});

describe("overrides", () => {
  // The documented shape: one registry for the file, overrides per test, and a
  // reset in between. The two tests below are a pair — the second one exists to
  // prove the first one's override is gone.
  const api = mock(http.get("https://api.test/user", () => HttpResponse.json({ name: "default" })));

  beforeAll(() => {
    api.listen();
  });
  afterEach(() => {
    api.resetHandlers();
    api.clearRequests();
  });
  afterAll(() => {
    api.close();
  });

  it("lets one test replace a handler", async () => {
    api.use(http.get("https://api.test/user", () => HttpResponse.json({ name: "override" })));

    const response = await fetch("https://api.test/user");
    expect(await response.json()).toEqual({ name: "override" });
  });

  it("has the declared handler back in the next test", async () => {
    const response = await fetch("https://api.test/user");
    expect(await response.json()).toEqual({ name: "default" });
  });

  it("gives the most recent override the last word", async () => {
    api.use(http.get("https://api.test/user", () => HttpResponse.json({ name: "first" })));
    api.use(http.get("https://api.test/user", () => HttpResponse.json({ name: "second" })));

    expect(await (await fetch("https://api.test/user")).json()).toEqual({ name: "second" });
  });

  it("restores a handler a once had spent", async () => {
    api.use(
      http.get("https://api.test/user", () => HttpResponse.json({ name: "once" }), { once: true }),
    );

    expect(await (await fetch("https://api.test/user")).json()).toEqual({ name: "once" });
    expect(await (await fetch("https://api.test/user")).json()).toEqual({ name: "default" });
  });
});

describe("resetHandlers with a new set", () => {
  it("replaces the declared handlers rather than layering over them", async () => {
    const api = mock(http.get("https://api.test/a", () => HttpResponse.text("a")));
    api.listen();
    try {
      expect(await (await fetch("https://api.test/a")).text()).toBe("a");

      api.resetHandlers(http.get("https://api.test/b", () => HttpResponse.text("b")));

      expect(await (await fetch("https://api.test/b")).text()).toBe("b");
      await expect(fetch("https://api.test/a")).rejects.toBeInstanceOf(UnhandledRequestError);
    } finally {
      api.close();
    }
  });
});

describe("the record", () => {
  it("records what was asked, with the body the test expected", async () => {
    await withMock(
      [http.post("https://api.test/users", () => HttpResponse.json({ id: "1" }))],
      undefined,
      async (api) => {
        await fetch("https://api.test/users", {
          method: "POST",
          headers: { "content-type": "application/json", "x-trace": "abc" },
          body: JSON.stringify({ name: "ada" }),
        });

        expect(api.requests.length).toBe(1);
        const [recorded] = api.requests;
        expect(recorded.method).toBe("POST");
        expect(recorded.url).toBe("https://api.test/users");
        expect(recorded.pathname).toBe("/users");
        expect(recorded.headers["x-trace"]).toBe("abc");
        expect(recorded.body).toBe('{"name":"ada"}');
        expect(recorded.json()).toEqual({ name: "ada" });
        expect(recorded.handled).toBe(true);
      },
    );
  });

  it("leaves the body for the resolver to read as well", async () => {
    await withMock(
      [
        http.post("https://api.test/echo", async ({ request }) =>
          HttpResponse.json(await request.json()),
        ),
      ],
      undefined,
      async (api) => {
        const response = await fetch("https://api.test/echo", {
          method: "POST",
          body: JSON.stringify({ seen: "by both" }),
        });

        // The log drained a clone; the resolver still got the original.
        expect(await response.json()).toEqual({ seen: "by both" });
        expect(api.requests[0].json()).toEqual({ seen: "by both" });
      },
    );
  });

  it("is in request order, even when a later request answers first", async () => {
    await withMock(
      [
        http.get("https://api.test/slow", async () => {
          await delay(30);
          return HttpResponse.text("slow");
        }),
        http.get("https://api.test/fast", () => HttpResponse.text("fast")),
      ],
      undefined,
      async (api) => {
        const slow = fetch("https://api.test/slow");
        const fast = fetch("https://api.test/fast");
        expect(await (await fast).text()).toBe("fast");
        expect(await (await slow).text()).toBe("slow");

        expect(api.requests.map((request) => request.pathname)).toEqual(["/slow", "/fast"]);
      },
    );
  });

  it("records an unhandled request too, and says so", async () => {
    const network = withNetwork(() => new Response("from the network"));
    try {
      await withMock([], { onUnhandledRequest: "bypass" }, async (api) => {
        await fetch("https://api.test/nothing");
        expect(api.requests.length).toBe(1);
        expect(api.requests[0].handled).toBe(false);
      });
    } finally {
      network.restore();
    }
  });

  it("is emptied by clearRequests, in place", async () => {
    await withMock(
      [http.get("https://api.test/ping", () => HttpResponse.text("pong"))],
      undefined,
      async (api) => {
        const held = api.requests;
        await fetch("https://api.test/ping");
        expect(held.length).toBe(1);

        api.clearRequests();

        // The same array, not a new one: a test that destructured `requests`
        // is still looking at the live log.
        expect(held.length).toBe(0);
        expect(api.requests).toBe(held);
      },
    );
  });
});

describe("unhandled requests", () => {
  it("rejects by default, naming what was asked and what existed", async () => {
    await withMock(
      [http.get("https://api.test/users/:id", () => HttpResponse.json({}))],
      undefined,
      async () => {
        await expect(fetch("https://api.test/orders/1")).rejects.toThrow(
          /no handler for GET https:\/\/api\.test\/orders\/1/,
        );
        await expect(fetch("https://api.test/orders/1")).rejects.toThrow(
          /GET https:\/\/api\.test\/users\/:id/,
        );
      },
    );
  });

  it("carries the method and URL as fields, not only as prose", async () => {
    await withMock([], undefined, async () => {
      let raised = null;
      try {
        await fetch("https://api.test/orders/1", { method: "DELETE" });
      } catch (error) {
        raised = error;
      }
      expect(raised instanceof UnhandledRequestError).toBe(true);
      expect(raised?.method).toBe("DELETE");
      expect(raised?.url).toBe("https://api.test/orders/1");
    });
  });

  it("lets one through when asked to bypass", async () => {
    const network = withNetwork(() => new Response("from the network"));
    try {
      await withMock([], { onUnhandledRequest: "bypass" }, async () => {
        const response = await fetch("https://api.test/anything");
        expect(await response.text()).toBe("from the network");
        expect(network.urls).toEqual(["https://api.test/anything"]);
      });
    } finally {
      network.restore();
    }
  });

  it("warns and lets it through when asked to warn", async () => {
    const network = withNetwork(() => new Response("from the network"));
    const warned = [];
    const warn = console.warn;
    console.warn = (message) => {
      warned.push(String(message));
    };
    try {
      await withMock([], { onUnhandledRequest: "warn" }, async () => {
        expect(await (await fetch("https://api.test/anything")).text()).toBe("from the network");
      });
    } finally {
      console.warn = warn;
      network.restore();
    }
    expect(warned.length).toBe(1);
    expect(warned[0]).toContain("no handler for GET https://api.test/anything");
  });
});

describe("passthrough", () => {
  it("sends a matched request on to the network", async () => {
    const network = withNetwork(() => new Response("from the network"));
    try {
      await withMock(
        [http.get("https://api.test/real", () => passthrough())],
        undefined,
        async (api) => {
          const response = await fetch("https://api.test/real");
          expect(await response.text()).toBe("from the network");
          expect(network.urls).toEqual(["https://api.test/real"]);
          // A handler did claim it, even though it chose not to answer.
          expect(api.requests[0].handled).toBe(true);
        },
      );
    } finally {
      network.restore();
    }
  });

  it("forwards a body the resolver already read", async () => {
    // A resolver reads the body to decide, then hands the request on. The
    // body is a stream that can be read once, so the network gets a copy that
    // nothing has touched — otherwise `fetch` rejects on a consumed body.
    let sawBody = "";
    let forwarded = "";
    const network = withNetwork(() => new Response("from the network"));
    globalThis.fetch = async (input: $FlowFixMe) => {
      forwarded = await input.text();
      return new Response("from the network");
    };
    try {
      await withMock(
        [
          http.post("https://api.test/echo", async ({ request }) => {
            sawBody = await request.text();
            return passthrough();
          }),
        ],
        undefined,
        async () => {
          const answer = await fetch("https://api.test/echo", {
            method: "POST",
            body: "the payload",
          });
          expect(await answer.text()).toBe("from the network");
        },
      );
    } finally {
      network.restore();
    }

    expect(sawBody).toBe("the payload");
    expect(forwarded).toBe("the payload");
  });
});

describe("network errors", () => {
  it("rejects rather than resolving, the way fetch itself does", async () => {
    await withMock(
      [http.get("https://api.test/down", () => HttpResponse.error())],
      undefined,
      async () => {
        await expect(fetch("https://api.test/down")).rejects.toBeInstanceOf(TypeError);
      },
    );
  });
});

describe("delayed responses", () => {
  it("keeps a never-settling request in flight, so a loading state can be seen", async () => {
    await withMock(
      [
        http.get("https://api.test/slow", async () => {
          await delay("infinite");
          return HttpResponse.json({ never: true });
        }),
      ],
      undefined,
      async () => {
        // A loading state, modelled without a renderer: what matters is that
        // the request has not settled while the assertion runs.
        let loading = true;
        let data = null;
        const request = fetch("https://api.test/slow").then(async (response) => {
          data = await response.json();
          loading = false;
          return "settled";
        });

        const outcome = await Promise.race([request, delay(30).then(() => "still loading")]);

        expect(outcome).toBe("still loading");
        expect(loading).toBe(true);
        expect(data).toBe(null);
      },
    );
  });

  it("answers after a finite delay rather than before it", async () => {
    await withMock(
      [
        http.get("https://api.test/slow", async () => {
          await delay(25);
          return HttpResponse.json({ ok: true });
        }),
      ],
      undefined,
      async () => {
        const order = [];
        const request = fetch("https://api.test/slow").then(async (response) => {
          order.push("answered");
          return response.json();
        });
        order.push("still waiting");

        const started = Date.now();
        expect(await request).toEqual({ ok: true });
        expect(order).toEqual(["still waiting", "answered"]);
        expect(Date.now() - started >= 10).toBe(true);
      },
    );
  });
});

describe("lifetime", () => {
  it("puts the platform's fetch back on close", async () => {
    const before = globalThis.fetch;
    const api = mock(http.get("https://api.test/ping", () => HttpResponse.text("pong")));

    api.listen();
    expect(globalThis.fetch).not.toBe(before);
    await fetch("https://api.test/ping");
    api.close();

    expect(globalThis.fetch).toBe(before);
  });

  it("refuses a second listen rather than nesting", async () => {
    const outer = mock();
    outer.listen({ onUnhandledRequest: "bypass" });
    try {
      expect(() => mock().listen()).toThrow(/already intercepted/);
    } finally {
      outer.close();
    }
  });

  it("closes cleanly when it never listened", () => {
    const api = mock();
    expect(() => api.close()).not.toThrow();
  });

  it("intercepts nothing before listen", async () => {
    const network = withNetwork(() => new Response("from the network"));
    try {
      const api = mock(http.get("https://api.test/ping", () => HttpResponse.text("pong")));
      expect(await (await fetch("https://api.test/ping")).text()).toBe("from the network");
      expect(api.requests.length).toBe(0);
    } finally {
      network.restore();
    }
  });
});
