// @flow
//
// `@uniflowed/fetch`.
//
// Every test uses an injected `fetch`, so the suite never touches the network
// and can produce the awkward cases on demand — a 500, a hang, a body that is
// not what the header said.

import { describe, expect, it } from "@uniflowed/test";
import { FetchError, createFetch } from "@uniflowed/fetch";
import { parser, v } from "@uniflowed/validator";

/** A `fetch` that answers from a script, recording what it was asked. */
function scripted(answers) {
  const calls = [];
  let at = 0;
  const impl = async (url, init) => {
    calls.push({ url, init });
    const answer = answers[Math.min(at, answers.length - 1)];
    at += 1;
    if (typeof answer === "function") {
      return answer(url, init);
    }
    return answer;
  };
  impl.calls = calls;
  return impl;
}

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

describe("requests", () => {
  it("parses a JSON body", async () => {
    const client = createFetch({ fetch: scripted([json({ id: 1 })]) });
    await expect(client.request("/users/1")).resolves.toEqual({ id: 1 });
  });

  it("joins the base URL and the path", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({ baseURL: "https://api.example.com/v1/", fetch: impl });
    await client.request("/users");
    expect(impl.calls[0].url).toBe("https://api.example.com/v1/users");
  });

  it("leaves an absolute URL alone", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({ baseURL: "https://api.example.com", fetch: impl });
    await client.request("https://elsewhere.example.com/thing");
    expect(impl.calls[0].url).toBe("https://elsewhere.example.com/thing");
  });

  it("appends search parameters", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({ fetch: impl });
    await client.request("/search", { searchParams: { q: "flow", page: 2 } });
    expect(impl.calls[0].url).toBe("/search?q=flow&page=2");
  });

  it("sends a plain object as JSON, and says so", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({ fetch: impl });
    await client.request("/users", { method: "POST", body: { name: "ada" } });
    expect(impl.calls[0].init.body).toBe('{"name":"ada"}');
    expect(impl.calls[0].init.headers["content-type"]).toBe("application/json");
  });

  it("leaves a body the platform already accepts", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({ fetch: impl });
    const form = new URLSearchParams({ a: "1" });
    await client.request("/x", { method: "POST", body: form });
    expect(impl.calls[0].init.body).toBe(form);
    expect(impl.calls[0].init.headers["content-type"]).toBe(undefined);
  });

  it("merges headers, with the request's winning", async () => {
    const impl = scripted([json({})]);
    const client = createFetch({
      fetch: impl,
      headers: { authorization: "token", accept: "application/json" },
    });
    await client.request("/x", { headers: { accept: "text/plain" } });
    expect(impl.calls[0].init.headers.authorization).toBe("token");
    expect(impl.calls[0].init.headers.accept).toBe("text/plain");
  });

  it("returns text for a text body and nothing for a 204", async () => {
    const text = createFetch({
      fetch: scripted([new Response("hello", { headers: { "content-type": "text/plain" } })]),
    });
    await expect(text.request("/x")).resolves.toBe("hello");

    const empty = createFetch({ fetch: scripted([new Response(null, { status: 204 })]) });
    await expect(empty.request("/x")).resolves.toBe(undefined);
  });

  it("hands the raw Response back when asked", async () => {
    const client = createFetch({ fetch: scripted([json({ id: 1 })]) });
    const response = await client.raw("/x");
    expect(response.status).toBe(200);
  });
});

describe("failures", () => {
  it("rejects a response that is not ok, rather than resolving with it", async () => {
    const client = createFetch({ fetch: scripted([json({ error: "no" }, 500)]) });
    // `fetch` resolves for a 500. Code that forgets `response.ok` treats an
    // error page as data, and the failure surfaces somewhere unrelated.
    await expect(client.request("/x")).rejects.toThrow("answered 500");
  });

  it("says what kind of failure it was", async () => {
    const client = createFetch({ fetch: scripted([json({}, 404)]) });
    try {
      await client.request("/x");
      throw new Error("expected a rejection");
    } catch (error) {
      expect(error instanceof FetchError).toBe(true);
      expect(error.failure.kind).toBe("http");
      expect(error.failure.status).toBe(404);
      expect(error.retriable).toBe(false);
    }
  });

  it("reports a network failure as one", async () => {
    const client = createFetch({
      fetch: scripted([
        () => {
          throw new TypeError("connection refused");
        },
      ]),
    });
    try {
      await client.request("/x");
      throw new Error("expected a rejection");
    } catch (error) {
      expect(error.failure.kind).toBe("network");
      expect(error.retriable).toBe(true);
    }
  });

  it("times out a request that never answers", async () => {
    const client = createFetch({
      timeout: 20,
      fetch: (url, init) =>
        new Promise((resolve, reject) => {
          // Answers the abort, the way the platform's fetch does.
          init.signal.addEventListener("abort", () => reject(new Error("aborted")));
        }),
    });
    try {
      await client.request("/x");
      throw new Error("expected a rejection");
    } catch (error) {
      expect(error.failure.kind).toBe("timeout");
      expect(error.failure.millis).toBe(20);
    }
  });

  it("rejects a body the schema does not accept, at the boundary", async () => {
    const client = createFetch({ fetch: scripted([json({ id: "not a number" })]) });
    const parse = parser(v.object({ id: v.number() }));
    try {
      await client.request("/x", { parse });
      throw new Error("expected a rejection");
    } catch (error) {
      // Here, where the value came from outside — not three frames later as a
      // TypeError about a property of undefined.
      expect(error.failure.kind).toBe("invalid");
      expect(error.failure.issues.length > 0).toBe(true);
    }
  });

  it("returns the parsed value when the schema accepts it", async () => {
    const client = createFetch({ fetch: scripted([json({ id: 7 })]) });
    const parse = parser(v.object({ id: v.number() }));
    await expect(client.request("/x", { parse })).resolves.toEqual({ id: 7 });
  });
});

describe("retries", () => {
  it("retries a 500 and returns the answer that worked", async () => {
    const impl = scripted([json({}, 500), json({ ok: true })]);
    const client = createFetch({ fetch: impl, retries: 2, retryDelay: 1 });
    await expect(client.request("/x")).resolves.toEqual({ ok: true });
    expect(impl.calls.length).toBe(2);
  });

  it("does not retry a 400", async () => {
    const impl = scripted([json({}, 400)]);
    const client = createFetch({ fetch: impl, retries: 3, retryDelay: 1 });
    await expect(client.request("/x")).rejects.toThrow("answered 400");
    // A 400 will be a 400 next time; sending it again is only slower.
    expect(impl.calls.length).toBe(1);
  });

  it("retries 429 and 408", async () => {
    for (const status of [429, 408]) {
      const impl = scripted([json({}, status), json({ ok: true })]);
      const client = createFetch({ fetch: impl, retries: 1, retryDelay: 1 });
      await expect(client.request("/x")).resolves.toEqual({ ok: true });
      expect(impl.calls.length).toBe(2);
    }
  });

  it("never retries a POST, however retriable the failure looks", async () => {
    const impl = scripted([json({}, 500), json({ ok: true })]);
    const client = createFetch({ fetch: impl, retries: 3, retryDelay: 1 });
    await expect(client.request("/x", { method: "POST" })).rejects.toThrow("answered 500");
    // A POST that failed may have been applied. Sending it again is how an
    // order is placed twice.
    expect(impl.calls.length).toBe(1);
  });

  it("gives up after the retries are used", async () => {
    const impl = scripted([json({}, 503)]);
    const client = createFetch({ fetch: impl, retries: 2, retryDelay: 1 });
    await expect(client.request("/x")).rejects.toThrow("answered 503");
    expect(impl.calls.length).toBe(3);
  });
});

describe("extend", () => {
  it("layers headers on top of the client's", async () => {
    const impl = scripted([json({})]);
    const base = createFetch({ fetch: impl, headers: { authorization: "token" } });
    const child = base.extend({ headers: { "x-trace": "1" } });
    await child.request("/x");
    expect(impl.calls[0].init.headers).toEqual({
      authorization: "token",
      "x-trace": "1",
    });
  });
});
