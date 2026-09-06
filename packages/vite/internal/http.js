// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// Node's request and response objects on one side, the platform's `Request`
// and `Response` on the other.
//
// uf's server contracts are the platform's — a route handler and a middleware
// both take a `Request` and return a `Response`, because that is what runs
// unchanged on Node.js, Bun, Deno and a Cloudflare Worker. Node's dev server
// speaks `IncomingMessage` and `ServerResponse`, so exactly one place has to
// translate.
//
// It is a module rather than two functions in `driver.js` because there are
// two dev servers: `driver.js` is what `uf dev` spawns, and the `uf:flow`
// plugin's own `configureServer` is what a project using Vite directly gets.
// Both have to run the same middleware before the same request, and a second
// copy of this translation is how the two would come to disagree about, say,
// whether a repeated header is joined or appended.

/**
 * A Node request as a `Request`.
 *
 * The body is read as a stream where the host supports it, because a handler
 * that accepts an upload should not need the whole thing buffered before it
 * starts.
 *
 * @param {import("node:http").IncomingMessage} incoming
 * @param {{server?: {https?: unknown}} | undefined} config the resolved Vite config
 */
export async function toRequest(incoming, config) {
  const host = incoming.headers.host ?? "localhost";
  const protocol = config?.server?.https == null ? "http" : "https";
  const url = new URL(incoming.originalUrl ?? incoming.url ?? "/", `${protocol}://${host}`);

  const headers = new Headers();
  for (const [name, value] of Object.entries(incoming.headers)) {
    if (value == null) continue;
    for (const entry of Array.isArray(value) ? value : [value]) {
      headers.append(name, entry);
    }
  }

  const method = (incoming.method ?? "GET").toUpperCase();
  const init = { method, headers };
  if (method !== "GET" && method !== "HEAD") {
    // `duplex` is required by the specification whenever a body is a stream,
    // and Node throws without it.
    init.body = incoming;
    init.duplex = "half";
  }
  return new Request(url, init);
}

/**
 * Write a `Response` to a Node response.
 *
 * @param {import("node:http").ServerResponse} outgoing
 * @param {Response} result
 */
export async function send(outgoing, result) {
  outgoing.statusCode = result.status;
  if (result.statusText !== "") {
    outgoing.statusMessage = result.statusText;
  }
  for (const [name, value] of result.headers) {
    outgoing.setHeader(name, value);
  }
  if (result.body == null) {
    outgoing.end();
    return;
  }
  // Streamed rather than buffered, so a handler returning a large or
  // open-ended body is not read into memory first.
  //
  // Which was only half true while this loop read as fast as the body would
  // give: `write` answers `false` when the kernel buffer is full and the rest
  // is being held in *this process's* memory, and a reader that ignores that
  // turns a slow client into a heap the size of everything it has not
  // acknowledged. Streaming in shape and buffering in fact — the same failure
  // `ChunkQueue` exists to avoid a layer up, in the renderer.
  const reader = result.body.getReader();
  // And a client that hangs up is the other half. Nothing written after that
  // goes anywhere, and the producer behind the body — a render, a proxied
  // upstream, an event stream — keeps producing for a reader that is never
  // coming back. `cancel()` is what tells it to stop; `releaseLock()` would
  // only detach this end.
  let open = true;
  const onClose = () => {
    open = false;
  };
  outgoing.on("close", onClose);
  try {
    while (open) {
      const { done, value } = await reader.read();
      if (done || !open) break;
      if (outgoing.write(value) === false) {
        await writable(outgoing);
      }
    }
  } finally {
    outgoing.off("close", onClose);
  }
  if (open) {
    outgoing.end();
    return;
  }
  // Best effort, and the only place in this file where a rejection is dropped:
  // the connection is already gone, so there is nobody left to report to and
  // no response left to fail.
  await reader.cancel().catch(() => {});
}

/**
 * Resolve once `outgoing` can take more — or once it cannot ever again.
 *
 * `drain` alone would be a deadlock waiting to happen: a client that hangs up
 * while the buffer is full emits `close` and never `drain`, and a writer
 * waiting only for the latter waits for the life of the process, holding the
 * body's producer open with it.
 *
 * @param {import("node:http").ServerResponse} outgoing
 */
function writable(outgoing) {
  return new Promise((resolve) => {
    const settle = () => {
      outgoing.off("drain", settle);
      outgoing.off("close", settle);
      resolve();
    };
    outgoing.once("drain", settle);
    outgoing.once("close", settle);
  });
}
