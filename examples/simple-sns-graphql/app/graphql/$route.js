// @flow
import { endpoint, sessionCookie } from "../upstream.js";

// Flow's DOM libdef predates these standard server Web APIs. Keep the
// compatibility casts at this boundary and expose only their actual signatures.
const signals = AbortSignal as $FlowFixMe as {
  any(Array<AbortSignal>): AbortSignal,
  timeout(number): AbortSignal,
};

/** A bounded same-origin BFF. The GraphQL service owns authorization and data. */
export async function POST(request: Request): Promise<Response> {
  if (request.headers.get("origin") !== new URL(request.url).origin) {
    return new Response("same-origin request required", { status: 403 });
  }
  if (!request.headers.get("content-type")?.startsWith("application/json")) {
    return new Response("application/json required", { status: 415 });
  }
  // These standard server APIs are absent from the upstream DOM libdef.
  const stream = (request as $FlowFixMe as { readonly body: ReadableStream | null }).body;
  const reader = stream?.getReader();
  if (reader == null) return new Response("request body required", { status: 400 });
  const chunks = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!(value instanceof Uint8Array)) throw new TypeError("Expected a byte stream");
      length += value.byteLength;
      if (length > 65536) {
        await reader.cancel("request too large");
        return new Response("request too large", { status: 413 });
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const body = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  const response = await fetch(endpoint(), {
    method: "POST",
    headers: {
      "content-type": "application/json",
      cookie: sessionCookie(request.headers.get("cookie") ?? ""),
    },
    body,
    signal: signals.any([request.signal, signals.timeout(10000)]),
    redirect: "error",
  });
  const resultHeaders = new Headers({
    "content-type": "application/json",
    "cache-control": "private, no-store",
  });
  for (let cookie of (
    response.headers as $FlowFixMe as { getSetCookie(): Array<string> }
  ).getSetCookie()) {
    if (!cookie.startsWith("commonplace_session=")) continue;
    if (new URL(request.url).protocol === "https:" && !/;\s*secure(?:;|$)/i.test(cookie))
      cookie += "; Secure";
    resultHeaders.append("set-cookie", cookie);
  }
  return new Response(response.body, { status: response.status, headers: resultHeaders });
}
