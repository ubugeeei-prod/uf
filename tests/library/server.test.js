// @flow
//
// `@uniflowed/server`: the request a server function is inside.
//
// The interesting cases are the ones a module-level variable would get wrong,
// so most of what is below is about isolation: two requests in flight at once,
// a context that ends when its request does, and work deferred past the
// response.

import { describe, expect, it } from "@uniflowed/testing";
import { after, cookies, draftMode, headers } from "@uniflowed/server";
import { contextFor, drainDeferred, parseCookies, runWithContext } from "@uniflowed/server/host";

/** A request with the given headers. */
function request(init: { readonly [string]: string }): Request {
  return new Request("https://uniflowed.dev/", { headers: init });
}

/** Run `body` as if handling a request carrying `init`. */
function handling<T>(init: { readonly [string]: string }, body: () => T): T {
  return runWithContext(contextFor(request(init)), body);
}

describe("headers", () => {
  it("reads the request's headers", () => {
    handling({ "x-uf": "1", accept: "text/html" }, () => {
      expect(headers().get("x-uf")).toBe("1");
      expect(headers().has("accept")).toBe(true);
    });
  });

  it("answers null for a header that was not sent", () => {
    handling({}, () => {
      expect(headers().get("x-missing")).toBe(null);
      expect(headers().has("x-missing")).toBe(false);
    });
  });

  it("throws outside a request, and names the binding", () => {
    // A component that calls this during a static prerender has made a mistake
    // worth naming — not one worth answering `null` for.
    expect(() => headers()).toThrow();
  });
});

describe("cookies", () => {
  it("reads the cookies the request carried", () => {
    handling({ cookie: "session=abc; theme=dark" }, () => {
      expect(cookies().get("session")).toBe("abc");
      expect(cookies().get("theme")).toBe("dark");
      expect(cookies().has("session")).toBe(true);
    });
  });

  it("answers null for a cookie that was not sent", () => {
    handling({ cookie: "a=1" }, () => {
      expect(cookies().get("b")).toBe(null);
      expect(cookies().has("b")).toBe(false);
    });
  });

  it("throws outside a request", () => {
    expect(() => cookies()).toThrow();
  });
});

describe("parsing a cookie header", () => {
  it("reads a single pair", () => {
    expect(parseCookies("a=1").a).toBe("1");
  });

  it("reads several, ignoring the spacing", () => {
    const out = parseCookies("a=1;b=2;   c=3");

    expect(out.a).toBe("1");
    expect(out.b).toBe("2");
    expect(out.c).toBe("3");
  });

  it("percent-decodes values", () => {
    expect(parseCookies("path=%2Fa%2Fb").path).toBe("/a/b");
  });

  it("leaves a value alone when it is not valid encoding", () => {
    // A malformed cookie is not a reason to fail a request; the value is simply
    // not what the sender meant.
    expect(parseCookies("broken=100%").broken).toBe("100%");
  });

  it("unwraps a quoted value", () => {
    expect(parseCookies('q="spaced value"').q).toBe("spaced value");
  });

  it("keeps the first of a duplicated name", () => {
    expect(parseCookies("a=first; a=second").a).toBe("first");
  });

  it("has no prototype, so a cookie cannot poison one", () => {
    // `__proto__` is a name an attacker can set, and on an ordinary object it
    // would not be a key at all.
    const out = parseCookies("__proto__=polluted");

    expect(Object.getPrototypeOf(out)).toBe(null);
    expect(({}: mixed).polluted).toBe(undefined);
  });

  it("ignores entries with no value and an empty header", () => {
    expect(Object.keys(parseCookies("novalue; =x; a=1"))).toEqual(["a"]);
    expect(Object.keys(parseCookies(""))).toEqual([]);
    expect(Object.keys(parseCookies(null))).toEqual([]);
  });
});

describe("draftMode", () => {
  it("is off until it is turned on", () => {
    handling({}, () => {
      expect(draftMode().isEnabled).toBe(false);
      draftMode().enable();
      expect(draftMode().isEnabled).toBe(true);
      draftMode().disable();
      expect(draftMode().isEnabled).toBe(false);
    });
  });

  it("belongs to the request, not to the module", () => {
    handling({}, () => {
      draftMode().enable();
    });

    handling({}, () => {
      expect(draftMode().isEnabled).toBe(false);
    });
  });
});

describe("after", () => {
  it("runs deferred work when the response has gone", async () => {
    const done: Array<string> = [];
    const context = contextFor(request({}));

    runWithContext(context, () => {
      after(() => {
        done.push("first");
      });
      after(() => {
        done.push("second");
      });
    });

    expect(done).toEqual([]);
    await drainDeferred(context);
    expect(done).toEqual(["first", "second"]);
  });

  it("awaits work that returns a promise", async () => {
    const done: Array<string> = [];
    const context = contextFor(request({}));

    runWithContext(context, () => {
      after(async () => {
        await Promise.resolve();
        done.push("async");
      });
    });
    await drainDeferred(context);

    expect(done).toEqual(["async"]);
  });

  it("lets the rest run when one task fails", async () => {
    // Deferred work is by definition not what the response depended on, so one
    // broken analytics call should not take the others with it.
    const done: Array<string> = [];
    const context = contextFor(request({}));

    runWithContext(context, () => {
      after(() => {
        throw new Error("boom");
      });
      after(() => {
        done.push("survived");
      });
    });
    await drainDeferred(context);

    expect(done).toEqual(["survived"]);
  });

  it("drains once, so a second drain runs nothing again", async () => {
    let runs = 0;
    const context = contextFor(request({}));

    runWithContext(context, () => {
      after(() => {
        runs += 1;
      });
    });
    await drainDeferred(context);
    await drainDeferred(context);

    expect(runs).toBe(1);
  });

  it("throws outside a request", () => {
    expect(() =>
      after(() => {
        // never reached
      }),
    ).toThrow();
  });
});

describe("isolation between requests", () => {
  it("keeps two requests in flight apart", async () => {
    // The case a module-level variable gets wrong, and only under load: one
    // request suspends, another arrives, and the first sees the second's
    // headers when it resumes.
    const slow = runWithContext(contextFor(request({ "x-who": "slow" })), async () => {
      await Promise.resolve();
      await Promise.resolve();
      return headers().get("x-who");
    });
    const fast = runWithContext(contextFor(request({ "x-who": "fast" })), async () => {
      return headers().get("x-who");
    });

    expect(await fast).toBe("fast");
    expect(await slow).toBe("slow");
  });

  it("ends the context when the request does", () => {
    handling({ "x-uf": "1" }, () => {
      expect(headers().get("x-uf")).toBe("1");
    });

    expect(() => headers()).toThrow();
  });
});
