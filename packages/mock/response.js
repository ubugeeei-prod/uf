// @flow
//
// What a resolver hands back.
//
// A separate module from `handler.js` because these three are the vocabulary a
// test writes *inside* a handler, and they are useful without one: a component
// test that injects its own `fetch` still wants `HttpResponse.json` and
// `delay`, and neither of them needs to know that a registry exists.
//
// `HttpResponse` is a real `Response` — a subclass, not a look-alike — so
// everything downstream of the mock (`response.ok`, `await response.json()`,
// a `Response` handed to `@uniflowed/fetch`) behaves the way it will in
// production. The statics exist because the two lines every test writes are
// "this JSON, 200" and "this JSON, 404", and `new Response(JSON.stringify(x),
// { headers: { "content-type": "application/json" } })` is not a thing anybody
// should type twice.

/** The subset of `ResponseInit` a mocked response needs. */
export type HttpResponseInit = {|
  readonly status?: number,
  readonly statusText?: string,
  readonly headers?: { readonly [string]: string },
|};

/**
 * The header that marks a response as "not an answer — send it on".
 *
 * A marked `Response` rather than a sentinel of some other type, which is also
 * how MSW spells it, and the reason is Flow rather than compatibility: a
 * resolver's return type stays `Response | void | null` and the registry never
 * has to narrow a union it cannot narrow. A response that reaches the caller
 * carrying this header would be a bug in this package, not in an application —
 * the registry consumes it and returns nothing in its place.
 */
const PASSTHROUGH_HEADER = "x-uf-mock-intention";

/**
 * Merge a content type into a caller's headers without overriding one they set.
 *
 * A test that says `{ "content-type": "application/problem+json" }` means it,
 * so the default only fills a gap. The comparison is lower-cased because header
 * names are case-insensitive and an object literal is not.
 */
function withContentType(init: HttpResponseInit | void, fallback: string): { [string]: string } {
  const headers: { [string]: string } = {};
  const given = init?.headers;
  if (given != null) {
    for (const name of Object.keys(given)) {
      headers[name] = given[name];
    }
  }
  const named = Object.keys(headers).some((name) => name.toLowerCase() === "content-type");
  if (!named) {
    headers["content-type"] = fallback;
  }
  return headers;
}

/**
 * A `Response`, with the constructors a test actually writes.
 *
 * Subclassing rather than wrapping is the whole point: a handler returns one of
 * these and the code under test receives something that passes `instanceof
 * Response`, streams, clones and reads exactly like the response it will get
 * from the real endpoint. A mock that hands back a plain object shaped like a
 * response is a mock that stops agreeing with production the first time
 * somebody calls `.clone()`.
 */
export class HttpResponse extends Response {
  /** A JSON body, with `content-type` already set. */
  static json(body: mixed, init?: HttpResponseInit): HttpResponse {
    return new HttpResponse(JSON.stringify(body), {
      status: init?.status ?? 200,
      statusText: init?.statusText ?? "",
      headers: withContentType(init, "application/json"),
    });
  }

  /** A plain-text body, with `content-type` already set. */
  static text(body: string, init?: HttpResponseInit): HttpResponse {
    return new HttpResponse(body, {
      status: init?.status ?? 200,
      statusText: init?.statusText ?? "",
      headers: withContentType(init, "text/plain;charset=UTF-8"),
    });
  }

  /**
   * The response a *failed* request produces — a rejected `fetch`, not a 500.
   *
   * The two are different failures and applications handle them in different
   * places: a 500 is a response with a status, a network error is a `TypeError`
   * out of `fetch` itself. Only `Response.error()` can build the second, so
   * this delegates rather than constructing one.
   */
  static error(): Response {
    return Response.error();
  }
}

/**
 * Whether a response stands for a network error rather than an HTTP one.
 *
 * Lives here, next to `HttpResponse.error`, so the registry does not have to
 * know how a network error is spelled — only that it must reject instead of
 * resolving.
 */
export function isNetworkError(response: Response): boolean {
  return response.type === "error";
}

/**
 * Wait, inside a resolver, before answering.
 *
 * `delay("infinite")` never settles, which is how a test observes a loading
 * state: the component renders its spinner, the assertion runs, and the request
 * is still in flight. Nothing is scheduled for it, so a pending one holds
 * nothing open and the runner exits normally.
 *
 * `globalThis.setTimeout` is read at call time rather than captured at import,
 * so `uft.useFakeTimers()` reaches a delay declared before the clock was
 * installed. A delay that quietly ignored the fake clock would be a test that
 * waits for real milliseconds while claiming not to.
 */
export function delay(ms: number | "infinite" = 0): Promise<void> {
  if (ms === "infinite") {
    return new Promise<void>(() => {});
  }
  return new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, ms);
  });
}

/**
 * Let this request reach the real network after all.
 *
 * Returned from a resolver that matched but decided not to answer — an
 * endpoint a test wants to hit for real while everything around it is mocked.
 * Distinct from returning nothing, which falls through to the next handler.
 */
export function passthrough(): Response {
  return new Response(null, { headers: { [PASSTHROUGH_HEADER]: "passthrough" } });
}

/** Whether a resolver's return value was `passthrough()`. */
export function isPassthrough(response: Response): boolean {
  return response.headers.get(PASSTHROUGH_HEADER) === "passthrough";
}
