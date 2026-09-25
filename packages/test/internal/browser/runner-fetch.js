// @flow
//
// Internal to `@uniflowed/test`: the `fetch` the browser runner talks to `uf`
// with.
//
// A page reports its results over HTTP — `POST /uf-test/events`, and
// `POST /uf-test/browser` for `createBrowser` — and the file under test runs
// in that same page. A file that installs a request mock replaces
// `globalThis.fetch`, and `@uniflowed/mock`'s default policy rejects every
// request no handler claimed. Reached through the global, the runner's own
// requests would be answered by the test's mock: rejected, and the rejection
// swallowed by the reporter, so the results would never reach `uf test`
// (ubugeeei-prod/uf#1394).
//
// So the harness captures the platform's `fetch` in a classic script that runs
// before any module — before the runner, and so before any test file could
// replace it — and the runner's requests go through that capture. A constant
// captured in this module would depend on when it was first evaluated and on
// `page.js` and the import map reaching it by the same URL; the harness
// script depends on neither.

/** Where `server.js`'s harness leaves the platform's `fetch`. */
export const RUNNER_FETCH: symbol = Symbol.for("uf.test.browser.fetch");

/**
 * `fetch`, as it was before any test file ran.
 *
 * Falls back to the global when nothing was captured, which is a page not
 * served by `uf test --browser` — there is no test file ahead of it to have
 * replaced anything.
 */
export function runnerFetch(input: RequestInfo, init?: RequestOptions): Promise<Response> {
  const captured = Reflect.get(globalThis, RUNNER_FETCH);
  if (typeof captured === "function") {
    return captured(input, init);
  }
  return fetch(input, init);
}
