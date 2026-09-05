// @flow
//
// `@uniflowed/mock`: request mocking at the level MSW works at.
//
// The premise MSW established, and it is the right one: a test should not know
// how its subject makes a request. Stubbing a client's method proves the test's
// stub matches the test's expectation and nothing about the URL, the verb, the
// body or the status. Answering the *request* leaves all of that in the code
// under test, so the test breaks when the endpoint's contract does.
//
//   const api = mock(
//     http.get("/users/:id", ({ params }) => HttpResponse.json({ id: params.id })),
//   );
//
//   beforeAll(() => api.listen());
//   afterEach(() => {
//     api.resetHandlers();
//     api.clearRequests();
//   });
//   afterAll(() => api.close());
//
//   it("shows the user", async () => {
//     render(<Profile id="42" />);
//     expect(await screen.findByText("42")).toBeTruthy();
//     expect(api.requests[0].pathname).toBe("/users/42");
//   });
//
// It used to be a declaration whose every function threw. This is the
// implementation, in Flow, with no native binding behind it: interception is
// one property on `globalThis` and matching is a walk over path segments, and
// neither is a hot path — a suite installs its handlers once and makes tens of
// requests, not millions.
//
// # What it intercepts, and what it does not
//
// `globalThis.fetch`, and nothing else. `uf test` runs on Node.js, Bun and Deno
// through the Capability JS Host contract, and `fetch` is the one request API
// all three have; anything lower is per-host. `internal/fetch-interceptor.js`
// is the only module that writes to a global, and it puts back exactly what it
// took.
//
// That is less than MSW covers and the gap is listed under **Readiness**
// rather than left to be discovered. The short version: `XMLHttpRequest`,
// `node:http`, `WebSocket` and `sendBeacon` are not intercepted, and a request
// made through one of them is not seen at all — not answered, not recorded, and
// not reported as unhandled.
//
// # An unhandled request fails
//
// The default is `onUnhandledRequest: "error"`, which rejects the caller's
// `fetch` with an `UnhandledRequestError` naming the method, the URL and every
// handler that was in force. MSW warns; uf does not, and `registry.js` sets out
// why at length. The short version is that `uf test` interleaves parallel
// workers' output, so a warning is a line nobody reads, and the failure it
// precedes lands in an unrelated file. `"warn"` and `"bypass"` are both there —
// explicitly, which is the part that matters.
//
// # How the package is laid out
//
// Three modules beside this one, split by what each decides:
//
// - `handler.js` — what answers a request: `http.get` and its siblings, and
//   what a resolver is handed. A handler is a value; declaring one installs
//   nothing.
// - `response.js` — what a resolver hands back: `HttpResponse`, `delay`,
//   `passthrough`. Useful without a registry, which is why it is not inside
//   one.
// - `registry.js` — `mock()`: the handler stack, the lifetime of the
//   interception, and the record. Everything here is about *time*.
//
// `internal/` holds the three a consumer has no business calling:
// `path.js` (the path grammar), `request-log.js` (capturing a body without
// taking it from the resolver) and `fetch-interceptor.js` (the global swap).
// Their *types* are public, because `RecordedRequest` is in the signature of
// `registry.requests` and a package whose public types cannot be named is a
// package nobody can write a helper for.
//
// # Readiness
//
// **Implemented and tested.** Handlers by method and path, with `:param`
// segments, a trailing `*` catch-all, and an optional origin so a pattern can
// name one host; `http.all` for any method; `{ once: true }`; a resolver that
// returns nothing falling through to the next handler. `HttpResponse.json`,
// `.text` and `.error`, all producing a real `Response` subclass. `delay(ms)`
// and `delay("infinite")` for asserting a loading state. `passthrough()`.
// Per-test `use()` overrides and a `resetHandlers()` that restores the declared
// set — including handlers a `once` had spent. A request log with method, URL,
// headers and body, in request order, with `json()` on it. `"error"`, `"warn"`
// and `"bypass"` for unhandled requests. `listen()`/`close()`, which restore
// `globalThis.fetch` and refuse to nest.
//
// **Experimental.** Relative URLs. Because this replaces `fetch` outright it
// can resolve `fetch("/api/users")` itself, against `location.origin` when a
// DOM is installed and `http://localhost` otherwise — a real convenience, and a
// guess about the host. A suite that cares should pass `listen({ origin })`;
// the default may change. `HttpResponse.error()` rejects with a `TypeError`, as
// `fetch` does, but the message is the platform's wording and is not the same
// string on every host.
//
// **Not implemented.** Any transport other than `globalThis.fetch`:
// `XMLHttpRequest` (including happy-dom's, which `@uniflowed/react-testing`
// installs), `node:http`/`node:https` and so axios's Node adapter, `node-fetch`
// and `got`, plus `WebSocket`, `EventSource` and `navigator.sendBeacon`. MSW
// reaches those through `@mswjs/interceptors`; uf does not yet, and until it
// does a request through one of them is invisible here rather than failing
// loudly, which is the one gap in this package's own rule about silence. A
// `fetch` a module captured into a `const` before `listen()` ran is likewise
// not intercepted. There are no GraphQL handlers (`graphql.query`,
// `graphql.mutation`), no browser service worker (`setupWorker`), no lifecycle
// event emitter (`server.events`) — `requests` is what there is instead — and
// no cookie store. `HttpResponse` has no `formData`, `arrayBuffer` or `xml`
// constructor; `new HttpResponse(body, init)` takes any `BodyInit` and covers
// them. Nothing here imports `@uniflowed/test`, so there is no automatic
// `beforeAll`/`afterEach` wiring: those four lines are written out above, and a
// mocking library that reached into the runner to install hooks would be one
// this repository could not honestly call runtime-agnostic.

export type { PathParams } from "./internal/path.js";
export type { RecordedRequest } from "./internal/request-log.js";
export type {
  HandlerOptions,
  Http,
  MockHandler,
  Resolver,
  ResolverInfo,
  ResolverResult,
} from "./handler.js";
export type { HttpResponseInit } from "./response.js";
export type { MockOptions, MockRegistry, UnhandledPolicy } from "./registry.js";

export { http } from "./handler.js";
export { HttpResponse, delay, passthrough } from "./response.js";
export { UnhandledRequestError, mock } from "./registry.js";
