// @flow
//
// Internal to `@uniflowed/mock`: the record of what was asked.
//
// Its own module for one reason, and it is the reason this is harder than it
// looks: a `Request` body is a stream that can be read exactly once. A log that
// reads the body to record it would hand the resolver an empty request, and a
// log that kept the `Request` and read it later would find the body already
// consumed by the code under test. So the request is cloned the moment it
// arrives and the *clone* is drained, which costs one tee per request and buys
// an assertion nobody can otherwise write:
//
//   expect(api.requests[0].json()).toEqual({ name: "ada" });
//
// Entries are appended when the request arrives, not when it is answered, so
// the log is in request order even when a delayed handler settles after a fast
// one that came later.

/** One request the registry saw, with its body already captured. */
export type RecordedRequest = {|
  readonly method: string,
  /** The absolute URL, after a relative one was resolved. */
  readonly url: string,
  readonly pathname: string,
  /** Header names lower-cased, as the platform gives them. */
  readonly headers: { readonly [string]: string },
  /** The body as text, or `""` for a request that cannot have one. */
  readonly body: string,
  /** Whether a handler answered it. `false` is an unhandled request. */
  readonly handled: boolean,
  /** The body parsed as JSON. Throws for a body that is not JSON. */
  readonly json: () => mixed,
|};

/**
 * The same entry before its verdict is in.
 *
 * `handled` and `body` are written after the object is built, so they are the
 * two fields that are not read-only here. `RecordedRequest` is the read-only
 * view of the same shape, which is what leaves the log immutable to a test
 * while still being writable by the dispatcher that owns it.
 *
 * `body` is written late on purpose: see `begin`.
 */
type Entry = {|
  readonly method: string,
  readonly url: string,
  readonly pathname: string,
  readonly headers: { readonly [string]: string },
  body: string,
  handled: boolean,
  readonly json: () => mixed,
|};

/** The log a registry keeps. */
export type RequestLog = {|
  /** Live, in request order. Truncated in place by `clear`. */
  readonly entries: $ReadOnlyArray<RecordedRequest>,
  /** Record a request, and hand back the callback that files its verdict. */
  readonly begin: (request: Request) => Promise<(handled: boolean) => void>,
  readonly clear: () => void,
|};

/**
 * Read a request's body without taking it away from the resolver.
 *
 * `GET` and `HEAD` are answered without touching the request at all: the
 * standard forbids them a body, and `clone()` on one is pure cost.
 */
async function bodyOf(request: Request): Promise<string> {
  if (request.method === "GET" || request.method === "HEAD") {
    return "";
  }
  return request.clone().text();
}

/** Everything about a request that can be read without awaiting anything. */
function describe(request: Request): Entry {
  const headers: { [string]: string } = {};
  for (const [name, value] of request.headers) {
    headers[name] = value;
  }
  const entry: Entry = {
    method: request.method,
    url: request.url,
    pathname: new URL(request.url).pathname,
    headers,
    body: "",
    handled: false,
    json: () => JSON.parse(entry.body),
  };
  return entry;
}

/**
 * A fresh, empty log.
 *
 * There is no cap on what it holds. A test suite makes tens of requests, not
 * millions, and a ring buffer that silently dropped the first one would break
 * the assertion this exists for.
 */
export function createRequestLog(): RequestLog {
  const entries: Array<Entry> = [];

  return {
    entries,
    async begin(request: Request) {
      // The entry takes its place in the log before the body is read, and the
      // body is written into it afterwards. Reading first and appending after
      // put a request with a fast body in front of one that arrived earlier
      // with a slow one — which is exactly the order this log promises not to
      // report.
      const entry = describe(request);
      entries.push(entry);
      entry.body = await bodyOf(request);
      return (handled: boolean) => {
        entry.handled = handled;
      };
    },
    clear() {
      // In place, so a test that held on to `api.requests` keeps looking at
      // the live log rather than at a detached snapshot of the last one.
      entries.length = 0;
    },
  };
}
