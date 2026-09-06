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
 * One implementation, reached late. This was a second copy of the loop in
 * `@uniflowed/server`'s `node.js`, and the two drifted the moment the shared
 * one moved: `uf start` and every adapter lost the socket pacing and the
 * hang-up cancel while `uf dev` and `uf preview` kept them, which is a
 * deployment whose memory profile differs from the one that was checked. See
 * ubugeeei-prod/uf#400.
 *
 * The import is inside the function, and that is not a style choice.
 * `driver.js` imports this module *statically* and registers the Flow loader
 * hooks in its own body, so anything reachable from a static import here is
 * read by Node before there is anything to compile Flow with —
 * `@uniflowed/server/node` is Flow source, and a static re-export of it makes
 * every `uf build` die on `import type` with a `SyntaxError`. `loadBuild` in
 * `internal/serve.js` defers for the same reason and says so.
 *
 * @param {import("node:http").ServerResponse} outgoing
 * @param {Response} result
 */
export async function send(outgoing, result) {
  const { send: write } = await import("@uniflowed/server/node");
  await write(outgoing, result);
}
