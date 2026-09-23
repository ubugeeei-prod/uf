// @flow
//
// Streaming SSR: a page that suspends for `DELAY` ms while it renders.
//
// Rendered per request (a parameter, no `generateStaticParams`), and waiting
// through `use()` rather than a loader, because the router awaits a loader
// before it sends anything — a slow loader delays the shell, which is the
// opposite of what this page exists to show. A target that streams sends the
// layout and `$loading.js`'s fallback first and this text about `DELAY` later;
// one that buffers sends both at once, and the check tells them apart by when
// each chunk arrived.

import * as React from "@uniflowed/react";
import { use } from "@uniflowed/react";

/** Long enough that a body sent in one piece is obvious from the timing. */
const DELAY = 1200;

/**
 * The promise for one `id`, created once and kept.
 *
 * `use()` needs the same promise every time the component runs, and a
 * component that suspends runs more than once; an inline `new Promise` would be
 * a fresh, pending one on every retry and the page would never resolve. Kept
 * after it settles for the same reason — the retry comes after the resolution —
 * so the check asks for an `id` of its own each time.
 */
const waiting: Map<string, Promise<string>> = new Map();

function waited(id: string): Promise<string> {
  const existing = waiting.get(id);
  if (existing != null) {
    return existing;
  }
  const promise = new Promise<string>((resolve) => {
    setTimeout(() => resolve(`stream: ${id}`), DELAY);
  });
  waiting.set(id, promise);
  return promise;
}

export default component Streamed(params: { readonly id: string }) {
  return <h1>{use(waited(params.id))}</h1>;
}
