// @flow
//
// `@uniflowed/server/events`, `/socket` and `/queue`.
//
// Three features and one seam, which is why they are one file. Each of them is
// a route handler asking the host for something the host may not have, and
// what is actually under test is that the answer is a named refusal rather
// than a connection nobody serves — a hung request on a Lambda, an upgrade
// dropped on a target that cannot hold one, a job pushed into a process that
// ends with the response.
//
// Everything here drives the modules directly. There is no socket, no worker
// and no clock: a capability is a value a host sets on the request, so a test
// can be any host by setting it, and `memoryQueue` takes its `now` so a
// backoff is asserted rather than waited for.

import { describe, expect, it } from "@uniflowed/test";
import { encodeEvent, eventStream } from "@uniflowed/server/events";
import { edgeCapabilities } from "@uniflowed/server/edge";
import { beginRequest } from "@uniflowed/server/host";
import { lambdaCapabilities } from "@uniflowed/server/lambda";
import { nodeCapabilities } from "@uniflowed/server/node";
import type { JobRecord } from "@uniflowed/server/queue";
import { backoffFor, defineJob, enqueue, memoryQueue } from "@uniflowed/server/queue";
import type { ServerCapabilities } from "@uniflowed/server/socket";
import { canUpgrade, isUpgradeRequest, upgradeWebSocket } from "@uniflowed/server/socket";

/** A request to the one path every case here uses, with these headers. */
function request(headers?: { [string]: string }): Request {
  const url = "https://uniflowed.dev/api/thing";
  return headers == null ? new Request(url) : new Request(url, { headers });
}

/** A response's reader, which is only ever absent for a body-less response. */
function readerFor(response: Response) {
  const body = response.body;
  if (body == null) {
    throw new Error("the response under test has no body");
  }
  return body.getReader();
}

/**
 * Run `body` as a host that declared `capabilities`.
 *
 * The whole of what a host does for these three features: begin the request,
 * put what it can do on it, and settle. `undefined` is a host that said
 * nothing, which is what every request looked like before capabilities
 * existed and is a case each module has to answer for.
 */
async function hosted<T>(
  capabilities: ServerCapabilities | void,
  body: () => T | Promise<T>,
): Promise<T> {
  const lifecycle = beginRequest(request());
  if (capabilities !== undefined) {
    lifecycle.context.capabilities = capabilities;
  }
  try {
    return await lifecycle.run(async () => body());
  } finally {
    await lifecycle.settle();
  }
}

/** Every chunk of a response body, as text. */
function readAll(response: Response): Promise<string> {
  return new Response(response.body).text();
}

describe("encoding one event", () => {
  it("writes the fields in the order the format defines", () => {
    expect(encodeEvent({ id: "7", event: "tick", data: "one" })).toBe(
      "id: 7\nevent: tick\ndata: one\n\n",
    );
  });

  it("turns a payload's newlines into further data lines", () => {
    // A reader joins consecutive `data:` lines with a newline. One `data:`
    // carrying an embedded newline is a message that ends early, and the rest
    // of it becomes a field nothing understands.
    expect(encodeEvent({ data: "one\ntwo" })).toBe("data: one\ndata: two\n\n");
  });

  it("normalises a carriage return before it decides where the lines are", () => {
    // The format ends a line on `\n`, `\r\n` or a bare `\r`, so leaving either
    // in makes one message into two on some clients and not on others.
    expect(encodeEvent({ data: "one\r\ntwo\rthree" })).toBe(
      "data: one\ndata: two\ndata: three\n\n",
    );
  });

  it("refuses an id with a newline in it rather than truncating one", () => {
    // A truncated `id:` is a cursor that silently means somewhere else, and
    // the next reconnect resumes from there.
    expect(() => encodeEvent({ id: "a\nb", data: "x" })).toThrow();
  });

  it("refuses an event name that would end its own field", () => {
    expect(() => encodeEvent({ event: "a\nb", data: "x" })).toThrow();
  });

  it("writes a retry as its own field", () => {
    expect(encodeEvent({ data: "x", retry: 2500 })).toBe("retry: 2500\ndata: x\n\n");
  });
});

describe("an event stream as a response", () => {
  it("says what it is, and that nothing on the path may hold it", async () => {
    const response = await hosted(undefined, () =>
      eventStream((sink) => sink.close(), { heartbeat: 0 }),
    );

    expect(response.headers.get("content-type")).toBe("text/event-stream; charset=utf-8");
    // Each of these is a way an event stream that works locally does nothing
    // in production: a shared cache replaying the first few events forever, a
    // proxy that gzips and decides for itself when to flush, and nginx
    // buffering the whole response until it ends.
    expect(response.headers.get("cache-control")).toBe("no-store, no-transform");
    expect(response.headers.get("x-accel-buffering")).toBe("no");
  });

  it("delivers what the source sent, in order", async () => {
    const response = await hosted(undefined, () =>
      eventStream(
        (sink) => {
          sink.send("first");
          sink.send({ event: "named", data: "second", id: "2" });
          sink.close();
        },
        { heartbeat: 0 },
      ),
    );

    expect(await readAll(response)).toBe("data: first\n\nid: 2\nevent: named\ndata: second\n\n");
  });

  it("sends the retry before anything the source does", async () => {
    // A reader that never connects again has no way to be told the delay
    // later, so it goes out with the first flush or it is not worth having.
    const response = await hosted(undefined, () =>
      eventStream((sink) => sink.close(), { heartbeat: 0, retry: 5000 }),
    );

    expect(await readAll(response)).toBe("retry: 5000\n");
  });

  it("runs the source's cleanup when the client hangs up", async () => {
    // Without this a stream's producer — an interval, a database
    // subscription, a proxied upstream — keeps producing for a reader that is
    // never coming back, for the life of the process.
    let stopped = false;
    const response = await hosted(undefined, () =>
      eventStream(
        (sink) => {
          sink.send("hello");
          return () => {
            stopped = true;
          };
        },
        { heartbeat: 0 },
      ),
    );

    const reader = readerFor(response);
    await reader.read();
    await reader.cancel("the client hung up");
    // The cleanup is registered from an awaited call, so it is one turn behind
    // the source returning.
    await Promise.resolve();
    expect(stopped).toBe(true);
  });

  it("aborts the sink's signal once the stream is over", async () => {
    let aborted = false;
    const response = await hosted(undefined, () =>
      eventStream(
        (sink) => {
          sink.signal.addEventListener("abort", () => {
            aborted = true;
          });
          sink.send("x");
        },
        { heartbeat: 0 },
      ),
    );

    await readerFor(response).cancel("the client hung up");
    expect(aborted).toBe(true);
  });

  it("ends the stream with a source's own error rather than losing it", async () => {
    // A rejected source used to be an unhandled rejection and a response that
    // simply stopped. The reader is the only place the client can be told
    // anything went wrong at all.
    const response = await hosted(undefined, () =>
      eventStream(
        async () => {
          throw new Error("the source gave up");
        },
        { heartbeat: 0 },
      ),
    );

    await expect(readAll(response)).rejects.toThrow("the source gave up");
  });

  it("disconnects a reader that falls far enough behind rather than buffering it", async () => {
    // `docs/security.md` rule 4: no unbounded anything. The producer here is
    // an application and the consumer is somebody's phone on a train, so the
    // queue between them has a ceiling — twice the high-water mark — and past
    // it the connection is closed instead of held in this process's memory.
    const response = await hosted(undefined, () =>
      eventStream(
        (sink) => {
          for (let index = 0; index < 20; index += 1) {
            sink.send(`event ${String(index)}`);
          }
        },
        { heartbeat: 0, buffer: 2 },
      ),
    );

    await expect(readAll(response)).rejects.toThrow("events behind");
  });

  it("is refused by a host that would hold every event until the stream ended", async () => {
    // A Lambda answers with a JSON value, so the body is read to the end
    // before the invocation returns — which for a stream that stays open is a
    // timeout rather than a slow response. Refused where the handler builds
    // it, with the target named.
    await expect(
      hosted(lambdaCapabilities(), () => eventStream((sink) => sink.close(), { heartbeat: 0 })),
    ).rejects.toThrow("serverless");
  });

  it("is allowed by a host that declared nothing", async () => {
    // Absence is silence rather than a claim: a `Response` streams by default
    // everywhere except where somebody said otherwise, so a host that has not
    // been taught to describe itself must not lose the feature.
    const response = await hosted(undefined, () =>
      eventStream((sink) => sink.close(), { heartbeat: 0 }),
    );
    expect(response.status).toBe(200);
  });
});

describe("recognising an upgrade request", () => {
  it("accepts a Connection header a proxy has added to", () => {
    // `Connection: keep-alive, Upgrade` is what several proxies send, and an
    // equality test rejects exactly the requests that arrived through
    // infrastructure.
    expect(
      isUpgradeRequest(request({ upgrade: "WebSocket", connection: "keep-alive, Upgrade" })),
    ).toBe(true);
  });

  it("does not treat an ordinary GET as one", () => {
    expect(isUpgradeRequest(request())).toBe(false);
    expect(isUpgradeRequest(request({ connection: "Upgrade" }))).toBe(false);
  });
});

describe("taking a socket", () => {
  /**
   * A host's upgrade, of the shape every runtime's reduces to.
   *
   * The response is a `200` carrying a marker rather than the `101` a real
   * handshake answers with, and that is a fact about the platform rather than
   * about this test: `new Response(null, { status: 101 })` throws, because the
   * standard constructor refuses anything under 200. Every runtime that can
   * upgrade builds that response itself — which is the other reason the
   * upgrade is the host's and not uf's.
   */
  const upgrader =
    (seen: Array<Request> = []) =>
    (given: Request) => {
      seen.push(given);
      return {
        response: new Response(null, { headers: { "x-upgraded": "1" } }),
        socket: { send: () => {}, close: () => {}, addEventListener: () => {} },
      };
    };

  it("hands the host's upgrader the request the handler was given", async () => {
    // Not a request rebuilt from the context: `Deno.upgradeWebSocket` and
    // Bun's `server.upgrade` find the connection *through* the object they
    // are passed, and a copy carrying the same headers has no socket behind
    // it.
    const seen: Array<Request> = [];
    const given = request({ upgrade: "websocket", connection: "Upgrade" });
    const lifecycle = beginRequest(given);
    lifecycle.context.capabilities = nodeCapabilities({ websocket: upgrader(seen) });

    const result = await lifecycle.run(async () => upgradeWebSocket(given));
    await lifecycle.settle();

    expect(result.response.headers.get("x-upgraded")).toBe("1");
    expect(seen[0]).toBe(given);
  });

  it("names the host rather than dropping the connection when it has none", async () => {
    await expect(hosted(nodeCapabilities(), () => upgradeWebSocket(request()))).rejects.toThrow(
      "node",
    );
  });

  it("says there is no request at all when it is called outside one", async () => {
    expect(() => upgradeWebSocket(request())).toThrow("not one here");
  });

  it("reports what the host can do without asking for it", async () => {
    expect(await hosted(nodeCapabilities(), () => canUpgrade())).toBe(false);
    expect(await hosted(nodeCapabilities({ websocket: upgrader() }), () => canUpgrade())).toBe(
      true,
    );
    // A host that said nothing has no upgrader, which is the one direction
    // silence answers: there is no such object to be silent about.
    expect(await hosted(undefined, () => canUpgrade())).toBe(false);
  });

  it("refuses an upgrader on a target that cannot hold a socket, where it is wired", () => {
    // Before a request, not on the first connection: a serverless deployment
    // given an upgrader would accept handshakes all day and drop every one,
    // and the moment to say so is while somebody is looking at the wiring.
    expect(() => lambdaCapabilities({ websocket: upgrader() })).toThrow("serverless");
    expect(() => nodeCapabilities({ websocket: upgrader() })).not.toThrow();
    expect(() => edgeCapabilities({ websocket: upgrader() })).not.toThrow();
  });
});

describe("queueing work", () => {
  /** A backend that keeps what it was pushed, so a test can read it. */
  const collecting = () => {
    const pushed: Array<JobRecord> = [];
    return {
      pushed,
      backend: {
        durable: true,
        name: "collecting",
        push: async (given: JobRecord) => {
          pushed.push(given);
        },
      },
    };
  };

  const greet = defineJob<mixed>({ name: "greet", run: (payload: mixed) => payload });

  it("gives a job three attempts a second apart unless it says otherwise", () => {
    expect(greet.retry.attempts).toBe(3);
    expect(backoffFor(greet.retry, 1)).toBe(1_000);
    expect(backoffFor(greet.retry, 2)).toBe(2_000);
    // Doubling stops at the ceiling rather than reaching a day on attempt 17.
    expect(backoffFor(greet.retry, 40)).toBe(greet.retry.maxBackoff);
  });

  it("refuses a job with no name, because a record carries one", () => {
    expect(() => defineJob<mixed>({ name: "  ", run: () => {} })).toThrow();
  });

  it("writes the job's name and the payload as text", async () => {
    const { pushed, backend } = collecting();
    const id = await hosted(nodeCapabilities({ queue: backend }), () =>
      enqueue(greet, { to: "ada" }),
    );

    expect(pushed).toHaveLength(1);
    expect(pushed[0].job).toBe("greet");
    expect(pushed[0].payload).toBe('{"to":"ada"}');
    expect(pushed[0].attempt).toBe(1);
    expect(pushed[0].id).toBe(id);
  });

  it("refuses a payload that cannot survive the trip, where it was written", async () => {
    // Every backend worth having puts the record through a process boundary,
    // so the boundary is enforced here — otherwise `memoryQueue` is the one
    // place a cycle works and production is where it is found.
    const cyclic: { self?: mixed } = {};
    cyclic.self = cyclic;
    const { backend } = collecting();

    await expect(
      hosted(nodeCapabilities({ queue: backend }), () => enqueue(greet, cyclic)),
    ).rejects.toThrow("cannot be JSON");
  });

  it("refuses a payload that serialises to nothing", async () => {
    // `JSON.stringify` answers `undefined` for a function rather than
    // throwing, so a job would be queued with no payload at all and fail in a
    // worker, hours later, with nothing pointing back here.
    const { backend } = collecting();

    await expect(
      hosted(nodeCapabilities({ queue: backend }), () => enqueue(greet, () => {})),
    ).rejects.toThrow("serialises to nothing");
  });

  it("names the host when the deployment supplied no queue", async () => {
    await expect(hosted(nodeCapabilities(), () => enqueue(greet, {}))).rejects.toThrow("node");
  });

  it("says there is no request when it is called outside one", async () => {
    await expect(enqueue(greet, {})).rejects.toThrow("not one here");
  });
});

describe("the queue that runs in this process", () => {
  it("runs a job on the next drain and says it is not durable", async () => {
    const seen: Array<mixed> = [];
    const job = defineJob<mixed>({
      name: "seen",
      run: (payload: mixed) => {
        seen.push(payload);
      },
    });
    const queue = memoryQueue({ jobs: [job], tick: 0, now: () => 0 });

    // Said as a value rather than in a paragraph, because an adapter refuses
    // on it and a paragraph cannot be refused on.
    expect(queue.durable).toBe(false);

    await queue.push({ id: "a", job: "seen", payload: '{"n":1}', attempt: 1, notBefore: 0 });
    expect(queue.size()).toBe(1);
    await queue.drain();

    expect(seen).toEqual([{ n: 1 }]);
    expect(queue.size()).toBe(0);
  });

  it("hands the job a copy, so it cannot reach back into the request", async () => {
    // The payload is JSON on the way in and parsed on the way out, in this
    // backend exactly as in a durable one — so a job that mutates what it was
    // given is invisible to whoever queued it, in every deployment rather
    // than in most of them.
    const original = { items: [1, 2] };
    let received: mixed = null;
    const job = defineJob<{ items: Array<number> }>({
      name: "mutating",
      run: (payload: { items: Array<number> }) => {
        received = payload;
        payload.items.push(3);
      },
    });
    // The real clock, unlike the cases below: `enqueue` stamps `notBefore`
    // from `Date.now()`, so a queue told the time is zero would consider a
    // record from today due in about fifty-five years.
    const queue = memoryQueue({ jobs: [job], tick: 0 });

    await hosted(nodeCapabilities({ queue }), () => enqueue(job, original));
    await queue.drain();

    expect(received).toEqual({ items: [1, 2, 3] });
    expect(original.items).toEqual([1, 2]);
  });

  it("waits out the backoff before trying a failed job again", async () => {
    // A retry that ignores the clock is a hot loop against whatever was
    // already failing, so the record is pushed back with a later `notBefore`
    // rather than run again in the same pass.
    let attempts = 0;
    const job = defineJob<mixed>({
      name: "flaky",
      retry: { attempts: 3, backoff: 1_000, maxBackoff: 10_000 },
      run: () => {
        attempts += 1;
        if (attempts < 3) throw new Error("not yet");
      },
    });
    let clock = 0;
    const queue = memoryQueue({ jobs: [job], tick: 0, now: () => clock });

    await queue.push({ id: "a", job: "flaky", payload: "null", attempt: 1, notBefore: 0 });
    await queue.drain();
    expect(attempts).toBe(1);

    // Still inside the first delay: a drain now must do nothing at all.
    clock = 999;
    await queue.drain();
    expect(attempts).toBe(1);

    clock = 1_000;
    await queue.drain();
    expect(attempts).toBe(2);

    // And the second delay is twice the first, not the same again.
    clock = 2_999;
    await queue.drain();
    expect(attempts).toBe(2);

    clock = 3_000;
    await queue.drain();
    expect(attempts).toBe(3);
    expect(queue.size()).toBe(0);
  });

  it("reports a job that has run out of attempts, once", async () => {
    const failures: Array<[string, string]> = [];
    const job = defineJob<mixed>({
      name: "doomed",
      retry: { attempts: 1, backoff: 1, maxBackoff: 1 },
      run: () => {
        throw new Error("no");
      },
    });
    const queue = memoryQueue({
      jobs: [job],
      tick: 0,
      now: () => 0,
      onFailure: (given: JobRecord, error: mixed) => {
        failures.push([given.job, String(error)]);
      },
    });

    await queue.push({ id: "a", job: "doomed", payload: "null", attempt: 1, notBefore: 0 });
    await queue.drain();
    await queue.drain();

    expect(failures).toHaveLength(1);
    expect(queue.size()).toBe(0);
  });

  it("reports a record naming a job it does not have rather than losing it", async () => {
    // A durable queue outliving the deployment that could run its records is
    // what a durable queue is *for*, so this is a rename or a rollback and it
    // has to be visible as one.
    const failures: Array<string> = [];
    const queue = memoryQueue({
      jobs: [],
      tick: 0,
      now: () => 0,
      onFailure: (given: JobRecord, error: mixed) => {
        failures.push(String(error));
      },
    });

    await queue.push({ id: "a", job: "gone", payload: "null", attempt: 1, notBefore: 0 });
    await queue.drain();

    expect(failures).toHaveLength(1);
    expect(failures[0]).toContain("gone");
  });
});

describe("what a target will accept", () => {
  const durable = { durable: true, name: "sqs", push: async () => {} };

  it("refuses an in-process queue where the process ends with the response", () => {
    // The serverless case is the obvious one. The worker is the one that
    // needs saying: it streams a body perfectly and is still an isolate the
    // platform may tear down the moment the response is out, which is why
    // `after()` there goes through `ctx.waitUntil`.
    const queue = memoryQueue({ jobs: [], tick: 0 });

    expect(() => lambdaCapabilities({ queue })).toThrow("memoryQueue");
    expect(() => edgeCapabilities({ queue })).toThrow("memoryQueue");
    expect(() => nodeCapabilities({ queue })).not.toThrow();
    queue.stop();
  });

  it("accepts a durable queue everywhere, because pushing is not draining", () => {
    // Pushing to SQS from a Lambda is ordinary and correct. What cannot be
    // there is the consumer, which is a second function or a container —
    // `@uniflowed/server/queue` says which half is whose.
    expect(() => lambdaCapabilities({ queue: durable })).not.toThrow();
    expect(() => edgeCapabilities({ queue: durable })).not.toThrow();
    expect(() => nodeCapabilities({ queue: durable })).not.toThrow();
  });

  it("states the two facts each target is asked about", () => {
    expect(nodeCapabilities()).toMatchObject({ target: "node", stream: true, persistent: true });
    expect(edgeCapabilities()).toMatchObject({ target: "edge", stream: true, persistent: false });
    expect(lambdaCapabilities()).toMatchObject({
      target: "serverless",
      stream: false,
      persistent: false,
    });
  });
});
