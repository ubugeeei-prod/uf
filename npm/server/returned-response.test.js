// @flow
//
// When a host that returns its `Response` (Bun, Deno) settles the request: once
// the body has gone, not before the response does. The route-cache case that
// found it (#1552) is in `cache.test.js`. These cases are about the body
// itself: what counts as having gone, and that the response is otherwise the
// one the application answered.

import { describe, expect, it } from "@uniflowed/test";

import { settledAfterBody } from "./internal/returned-response.js";

/** A body whose chunks the test hands over one at a time. */
function heldBody(): {|
  body: ReadableStream<Uint8Array>,
  send: (text: string) => void,
  end: () => void,
|} {
  let controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  const body = new ReadableStream({
    start(opened: ReadableStreamDefaultController<Uint8Array>) {
      controller = opened;
    },
  });
  return {
    body,
    send: (text) => controller?.enqueue(new TextEncoder().encode(text)),
    end: () => controller?.close(),
  };
}

/** A settle that counts, as `RequestLifecycle.settle` would be called. */
function counted(): {| settle: () => Promise<void>, calls: () => number |} {
  let calls = 0;
  return {
    settle: async () => {
      calls += 1;
    },
    calls: () => calls,
  };
}

describe("a response a host returns", () => {
  it("settles once the body has been read to the end, and not before", async () => {
    const held = heldBody();
    const settled = counted();
    const response = settledAfterBody(new Response(held.body), settled.settle);
    const reader = response.body?.getReader();

    held.send("shell");
    expect(new TextDecoder().decode((await reader?.read())?.value)).toBe("shell");
    expect(settled.calls()).toBe(0);

    held.send("rest");
    held.end();
    expect(new TextDecoder().decode((await reader?.read())?.value)).toBe("rest");
    expect((await reader?.read())?.done).toBe(true);
    expect(settled.calls()).toBe(1);
  });

  it("settles when the client goes away, and stops the body behind it", async () => {
    let cancelled: mixed = null;
    const body = new ReadableStream({
      cancel(reason: mixed) {
        cancelled = reason;
      },
    });
    const settled = counted();
    const response = settledAfterBody(new Response(body), settled.settle);

    await response.body?.cancel("gone");
    expect(settled.calls()).toBe(1);
    expect(cancelled).toBe("gone");
  });

  it("settles when the body fails, and passes the failure on", async () => {
    const body = new ReadableStream({
      pull() {
        throw new Error("the render failed");
      },
    });
    const settled = counted();
    const response = settledAfterBody(new Response(body), settled.settle);

    let raised = null;
    try {
      await response.text();
    } catch (error) {
      raised = error;
    }
    expect(String(raised)).toContain("the render failed");
    expect(settled.calls()).toBe(1);
  });

  it("settles a response with no body at once", () => {
    const settled = counted();
    const response = new Response(null, { status: 204 });
    expect(settledAfterBody(response, settled.settle)).toBe(response);
    expect(settled.calls()).toBe(1);
  });

  it("keeps the status and every header, each cookie on its own", async () => {
    const headers = new Headers({ "content-type": "text/html; charset=utf-8" });
    headers.append("set-cookie", "a=1; Path=/");
    headers.append("set-cookie", "b=2; Path=/");
    const response = settledAfterBody(
      new Response("body", { status: 201, statusText: "Created", headers }),
      counted().settle,
    );

    expect(response.status).toBe(201);
    expect(response.statusText).toBe("Created");
    expect(response.headers.get("content-type")).toBe("text/html; charset=utf-8");
    const cookies = [...response.headers]
      .filter(([header]) => header === "set-cookie")
      .map(([, value]) => value);
    expect(cookies).toEqual(["a=1; Path=/", "b=2; Path=/"]);
    expect(await response.text()).toBe("body");
  });
});
