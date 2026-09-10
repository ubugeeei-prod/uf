// @flow
//
// A route that suspends while it renders.
//
// The parameter is what keeps it out of the prerender — `staticPaths` skips a
// route with parameters and no `generateStaticParams`, exactly as
// `app/posts/[slug]` does — so `uf start` has to render it per request, which
// is the only way a test can watch it stream.
//
// It waits by `use()`ing a promise rather than in a loader, and that is the
// difference the route exists to show. The router awaits a page's `loader`
// before it renders anything, so a slow loader would delay the *shell* and this
// route would demonstrate the opposite of what it is for. Deferring a loader is
// ubugeeei-prod/uf#373.

import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";

/** Long enough that a document sent in one piece is obvious from the timing. */
const DELAY = 500;

/**
 * The promise for one `id`, created once and kept.
 *
 * `use()` needs the *same* promise object every time the component runs, and a
 * component that suspends runs more than once: React throws, waits, and calls
 * it again. A `new Promise(…)` written inline in the body would be a different,
 * still-pending promise on every retry, so the boundary would never resolve and
 * the page would hang rather than stream — which looks, from the outside,
 * exactly like a renderer that does not stream.
 *
 * Kept rather than deleted when it settles, for the same reason: the retry
 * happens *after* it resolves, and an entry removed on resolution would send
 * the retry back to a fresh promise and the same hang. A second request for the
 * same `id` in one server therefore answers immediately, so the test asks each
 * server for an `id` of its own.
 */
const waiting: Map<string, Promise<string>> = new Map();

function waited(id: string): Promise<string> {
  const existing = waiting.get(id);
  if (existing != null) {
    return existing;
  }
  const promise = new Promise<string>((resolve) => {
    setTimeout(() => resolve(`slow: ${id}`), DELAY);
  });
  waiting.set(id, promise);
  return promise;
}

export default component Slow(params: { readonly id: string }) {
  // One text node, like `app/posts/[slug]`: React puts a `<!-- -->` between two
  // children, and the test is looking for this string in a half-received
  // document.
  return <h1>{use(waited(params.id))}</h1>;
}
