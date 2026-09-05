// @flow
//
// The thing a suite holds: handlers, the lifetime of the interception, and the
// record of what was asked.
//
// A separate module from `handler.js` because those three are one decision each
// and they are all about *time* — when interception starts, which handlers are
// in force at this instant, what has happened so far. A handler has no time in
// it at all.
//
//   const api = mock(...handlers);
//
//   beforeAll(() => api.listen());
//   afterEach(() => {
//     api.resetHandlers();
//     api.clearRequests();
//   });
//   afterAll(() => api.close());
//
// # `mock()` rather than `setupServer()`
//
// The same reasoning `@uniflowed/test` gives for `uft` rather than `vi`:
// borrowing another tool's brand for uf's function would be claiming something
// uf has not earned. `http.get` and `HttpResponse.json` keep MSW's spelling
// because those names *describe* — they are what a reader would call them
// anyway — and a suite ported from MSW has that spelling at every call site.
// `setupServer` is the one name in that surface that is neither descriptive nor
// accurate here: nothing is set up, and there is no server. `mock()` is also
// what `@uniflowed/story`'s `withMocks` was already written against.
//
// The contract this package was sketched with also had a bare `use(handler)`.
// It cannot say *which* registry, so it is a method now. That is not a
// simplification of the sketch, it is the question the sketch left open.
//
// # An unhandled request fails, loudly
//
// The default is `"error"`: a request no handler claimed rejects the caller's
// `fetch` with an `UnhandledRequestError` naming the method, the URL and every
// handler that was in force. MSW defaults to `"warn"` and uf does not, on
// purpose.
//
// A warning is the wrong shape for this. `uf test` runs files in parallel
// workers and interleaves their output, so a warning is a line in a scroll
// nobody reads on a green run — and the failure it precedes lands somewhere
// else entirely, as a component rendering an error state or an assertion about
// a value that never arrived. The afternoon goes into the wrong file. An
// unhandled request is a mistake in the test's own setup, and a mistake in the
// setup should fail at the line that made it.
//
// The other two are there for the cases where reaching the network is the
// point: `"bypass"` for a suite that mocks one host and talks to a local
// server for the rest, `"warn"` for the middle of a migration. Both are
// explicit, which is the difference that matters.

import type { MockHandler } from "./handler.js";
import { matchHandler } from "./handler.js";
import type { RecordedRequest } from "./internal/request-log.js";
import { createRequestLog } from "./internal/request-log.js";
import { installFetch } from "./internal/fetch-interceptor.js";
import { isPassthrough } from "./response.js";

/** What happens to a request no handler claimed. */
export type UnhandledPolicy =
  /** Reject the caller's `fetch`. The default. */
  | "error"
  /** Warn once and let it reach the network. */
  | "warn"
  /** Let it reach the network, silently. */
  | "bypass";

/** How a registry behaves while it is listening. */
export type MockOptions = {|
  readonly onUnhandledRequest?: UnhandledPolicy,
  /**
   * What a relative URL is resolved against.
   *
   * Defaults to the document's origin when a DOM is installed — which is what
   * `@uniflowed/react-testing` does — and to `http://localhost` otherwise, so
   * a component calling `fetch("/api/users")` works under `uf test` without
   * being rewritten to an absolute URL it would never use in production.
   */
  readonly origin?: string,
|};

/** A suite's mock server. */
export type MockRegistry = {|
  /** Start intercepting. Throws if something already is. */
  readonly listen: (options?: MockOptions) => void,
  /** Stop, and put the platform's `fetch` back. */
  readonly close: () => void,
  /** Add handlers that win over the declared set, until the next reset. */
  readonly use: (...handlers: $ReadOnlyArray<MockHandler>) => void,
  /**
   * Drop every override, restoring the set `mock()` was given — or, when
   * handlers are passed, replace that set with them.
   */
  readonly resetHandlers: (...next: $ReadOnlyArray<MockHandler>) => void,
  /** Every request seen since the last `clearRequests`, in request order. */
  readonly requests: $ReadOnlyArray<RecordedRequest>,
  readonly clearRequests: () => void,
|};

/**
 * A request nobody claimed, under the default policy.
 *
 * Carries the method and URL as fields as well as in the message, so a test
 * that means to provoke one can assert on them rather than on prose.
 */
export class UnhandledRequestError extends Error {
  readonly method: string;
  readonly url: string;

  constructor(method: string, url: string, handlers: $ReadOnlyArray<MockHandler>) {
    super(describeUnhandled(method, url, handlers));
    this.name = "UnhandledRequestError";
    this.method = method;
    this.url = url;
  }
}

/** The message an unhandled request produces: what was asked, and what existed. */
function describeUnhandled(
  method: string,
  url: string,
  handlers: $ReadOnlyArray<MockHandler>,
): string {
  const declared =
    handlers.length === 0
      ? "  (no handlers are in force)"
      : handlers.map((handler) => `  ${handler.method} ${handler.path}`).join("\n");
  return (
    `@uniflowed/mock: no handler for ${method} ${url}\n\n` +
    `In force:\n${declared}\n\n` +
    "Declare a handler for it, or pass " +
    'listen({ onUnhandledRequest: "bypass" }) to let it reach the network.'
  );
}

/** Where a relative URL is resolved when the caller did not say. */
function defaultOrigin(): string {
  // `typeof` rather than a property read: a host with no DOM has no `location`
  // binding at all, so reading one would be a `ReferenceError` and not
  // `undefined`. A DOM the test installed — which is what
  // `@uniflowed/react-testing` does — has a real origin, and it is a better
  // guess than a constant. `null` is what an opaque origin serialises to, and
  // is not one.
  if (typeof location === "undefined") {
    return "http://localhost";
  }
  const origin = location.origin;
  return origin !== "" && origin !== "null" ? origin : "http://localhost";
}

/**
 * A registry over these handlers.
 *
 * Nothing is intercepted until `listen()`. The handlers given here are the
 * *declared* set: `use()` layers over it and `resetHandlers()` takes those
 * layers away, which is what makes a per-test override actually disappear
 * between tests rather than at the end of the file.
 */
export function mock(...handlers: $ReadOnlyArray<MockHandler>): MockRegistry {
  let declared: Array<MockHandler> = [...handlers];
  let overrides: Array<MockHandler> = [];
  let spent: Set<MockHandler> = new Set();
  let policy: UnhandledPolicy = "error";
  let stop: (() => void) | null = null;

  const log = createRequestLog();

  /**
   * The handlers a request is offered, most specific first.
   *
   * Overrides before declared handlers, and the most recent override first, so
   * `use()` inside a test wins over `use()` in a `beforeEach` which wins over
   * the suite's default. A handler that has spent its `once` is not offered.
   */
  const inForce = (): Array<MockHandler> =>
    [...overrides]
      .reverse()
      .concat(declared)
      .filter((handler) => !spent.has(handler));

  const dispatch = async (request: Request): Promise<Response | null> => {
    const url = new URL(request.url);
    // Recorded before it is answered, so the log is in request order even when
    // a delayed handler settles after a later, faster one.
    const settle = await log.begin(request);
    const offered = inForce();

    for (const handler of offered) {
      const params = matchHandler(handler, request.method, url);
      if (params == null) {
        continue;
      }
      const answer = await handler.resolve({
        request,
        params,
        query: url.searchParams,
      });
      // Nothing back means "not mine after all": try the next handler rather
      // than treating a silent resolver as an empty response.
      if (answer == null) {
        continue;
      }
      if (handler.once) {
        spent.add(handler);
      }
      settle(true);
      return isPassthrough(answer) ? null : answer;
    }

    settle(false);
    if (policy === "bypass") {
      return null;
    }
    if (policy === "warn") {
      console.warn(describeUnhandled(request.method, request.url, offered));
      return null;
    }
    throw new UnhandledRequestError(request.method, request.url, offered);
  };

  return {
    listen(options?: MockOptions) {
      policy = options?.onUnhandledRequest ?? "error";
      stop = installFetch(dispatch, options?.origin ?? defaultOrigin());
    },
    close() {
      // A no-op when nothing is listening, so an `afterAll` still runs cleanly
      // after a `listen()` that threw — and the error that suite reports is the
      // one from `listen`, not a second one from the teardown.
      if (stop != null) {
        stop();
        stop = null;
      }
    },
    use(...next: $ReadOnlyArray<MockHandler>) {
      overrides = overrides.concat(next);
    },
    resetHandlers(...next: $ReadOnlyArray<MockHandler>) {
      overrides = [];
      // A spent `once` is spent against the run, not against the handler, so a
      // reset makes the declared set whole again.
      spent = new Set();
      if (next.length > 0) {
        declared = [...next];
      }
    },
    requests: log.entries,
    clearRequests: log.clear,
  };
}
