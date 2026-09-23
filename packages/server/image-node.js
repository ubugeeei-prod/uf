// @flow
//
// `@uniflowed/server/image/node`: the image endpoint's fetch, for a runtime
// with a resolver and a socket.
//
// `./image.js` decides which URLs may be fetched; this decides which
// *addresses*, which is the half only a runtime that resolves names can do.
// `uf start`, `uf preview`, `uf dev` and every directory `--adapter node`,
// `container`, `bun`, `deno` and `serverless` writes use it.
//
// # Resolve, judge, then connect to what was judged
//
// The name is resolved once, with every address it has. If *any* of them is
// loopback, private, link-local or otherwise not public
// (`./internal/addresses.js`), the request is refused — not only the one that
// would have been chosen, because a resolver that answers with one public and
// one private address is the shape of a rebinding attempt, and which of the
// two a connection would have used is not something to bet the network on.
//
// The connection is then made **to that address**, with the name carried as
// the TLS server name and the `Host` header. Handing the name to `https.get`
// instead would resolve it a second time, and the answer to the second lookup
// is whatever the name's owner wanted it to be by then: that gap between the
// check and the use is DNS rebinding, and closing it is the reason this module
// makes its own request rather than calling the platform's `fetch`. The
// certificate is still checked against the name, so a pinned address serves
// only a server that can prove it is the host the allow-list named.
//
// A redirect is answered as the `3xx` it is. `./image.js` follows it, which
// means it is asked of the allow-list and comes back through here — resolved
// and judged again — like any other request.

import { lookup as resolveName } from "node:dns/promises";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { isIP } from "node:net";

import type { ImageFetch } from "./image.js";
import { ImageRefusal } from "./image.js";
import { isPublicAddress } from "./internal/addresses.js";

/** One answer from a resolver: an address and its family. */
export type ResolvedAddress = {| readonly address: string, readonly family: number |};

export type NodeImageFetchOptions = {|
  /**
   * `app.builtins.images.dangerouslyAllowPrivateAddresses`: connect to
   * loopback and private addresses too. For a test that serves its own
   * images, and for nothing that anybody else can reach.
   */
  readonly allowPrivateAddresses?: boolean,
  /**
   * The resolver. `node:dns`'s `lookup` with `all: true` when absent;
   * injectable so the refusal of a private answer is tested without a DNS
   * server that gives one.
   */
  readonly lookup?: (hostname: string) => Promise<$ReadOnlyArray<ResolvedAddress>>,
|};

/** The endpoint's fetch over `node:http` and `node:https`. */
export function nodeImageFetch(options?: NodeImageFetchOptions): ImageFetch {
  const allowPrivate = options?.allowPrivateAddresses === true;
  const lookup =
    options?.lookup ?? ((hostname: string) => resolveName(hostname, { all: true, verbatim: true }));

  return async function fetchImage(url: URL, signal: AbortSignal): Promise<Response> {
    // `URL` writes an IPv6 literal in brackets, which no resolver and no
    // socket takes.
    const hostname =
      url.hostname.startsWith("[") && url.hostname.endsWith("]")
        ? url.hostname.slice(1, -1)
        : url.hostname;
    const addresses =
      isIP(hostname) === 0
        ? await lookup(hostname)
        : [{ address: hostname, family: isIP(hostname) }];
    if (addresses.length === 0) {
      throw new ImageRefusal(502, `${url.host} does not resolve`);
    }
    if (!allowPrivate && addresses.some(({ address }) => !isPublicAddress(address))) {
      // The address is not in the message: what a name resolves to inside
      // this network is exactly what a probe through this endpoint is after.
      throw new ImageRefusal(
        403,
        `${url.host} resolves to a loopback, private or link-local address, which the ` +
          "image endpoint does not connect to",
      );
    }
    const pinned = addresses[0];
    return await get(url, pinned, signal);
  };
}

/** One `GET` to `url`, connected to `pinned` rather than to a fresh lookup. */
function get(url: URL, pinned: ResolvedAddress, signal: AbortSignal): Promise<Response> {
  const secure = url.protocol === "https:";
  const send = secure ? httpsRequest : httpRequest;
  return new Promise((resolve, reject) => {
    const outgoing = send(
      {
        host: pinned.address,
        family: pinned.family,
        port: url.port === "" ? (secure ? 443 : 80) : Number(url.port),
        path: `${url.pathname}${url.search}`,
        method: "GET",
        // The name the allow-list admitted, for the virtual host and — below —
        // for the certificate. An IPv6 literal keeps its brackets in `Host`.
        headers: { host: url.host, accept: "image/png,image/jpeg,image/webp,image/*;q=0.8" },
        ...(secure && isIP(url.hostname) === 0 ? { servername: url.hostname } : {}),
        signal,
      },
      (incoming) => {
        const headers = new Headers();
        for (const [name, value] of Object.entries(incoming.headers)) {
          if (Array.isArray(value)) {
            for (const each of value) headers.append(name, each);
          } else if (typeof value === "string") {
            headers.set(name, value);
          }
        }
        const status = incoming.statusCode ?? 502;
        // A `Response` cannot carry a body for these, and the endpoint reads
        // none from them.
        const bodiless = status === 204 || status === 304 || (status >= 300 && status < 400);
        if (bodiless) {
          incoming.resume();
          resolve(new Response(null, { status, headers }));
          return;
        }
        // Once the reader has gone — the endpoint cancels at the byte bound —
        // nothing more is handed to the stream: a controller that is closed
        // throws on `enqueue`, and the socket is already being torn down.
        let open = true;
        const body = new ReadableStream({
          start(controller) {
            incoming.on("data", (chunk: Uint8Array) => {
              if (open) {
                controller.enqueue(new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.length));
              }
            });
            incoming.on("end", () => {
              if (open) controller.close();
              open = false;
            });
            incoming.on("error", (error) => {
              if (open) controller.error(error);
              open = false;
            });
          },
          cancel() {
            open = false;
            incoming.destroy();
          },
        });
        resolve(new Response(body, { status, headers }));
      },
    );
    outgoing.on("error", reject);
    outgoing.end();
  });
}
