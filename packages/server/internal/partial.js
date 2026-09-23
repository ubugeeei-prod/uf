// @flow
//
// Internal to `@uniflowed/server`: a read of the request, during a build that
// can leave it for later.
//
// A prerender has no request, and `cookies()`, `headers()` and `draftMode()`
// throw there: a document written once is about nobody, and a page that reads
// the request cannot be one. Partial prerendering is the exception that keeps
// the rule. A page whose read sits inside a `<Suspense>` boundary has a static
// shell around it — everything outside the boundary — and `uf build` writes
// that shell, leaves the boundary as a hole, and a server renders the hole per
// request. So while `uf build` prerenders a page that way, the three reads do
// not throw [`OutsideRequestError`]; they throw [`PostponedReadError`], which
// `@uniflowed/router` recognises and turns into the hole.
//
// A throw rather than a promise that never settles, and the reason is when the
// build may stop. A read that waits forever leaves the renderer with work that
// never ends, and "everything else has finished" is then a guess about timing.
// A read that throws lets the Flight render run to its end, deterministically,
// with every `await` the rest of the page makes answered; the router then takes
// the rows the throws produced out of the payload before the HTML renderer
// reads it, and it is *those* rows that never arrive. See
// `@uniflowed/router`'s `internal/flight-rows.js`.
//
// The scope is the process's, like the request context, because the page that
// reads is rendered by the module graph React Server Components run in, which
// holds a second copy of this package. `./process-state.js` says why.

import { AsyncLocalStorage } from "node:async_hooks";

import { processWide } from "./process-state.js";

/** One partial prerender: what it read, in the order it read it. */
export type PartialPrerender = {|
  /** The bindings that were left for the request: `cookies`, `headers`, `draftMode`. */
  readonly reads: Array<string>,
|};

const scopes: AsyncLocalStorage<PartialPrerender> = processWide(
  "partial-prerender@1",
  () => new AsyncLocalStorage(),
);

/**
 * How an error says it is a read left for the request, whichever copy of this
 * package threw it. A registered symbol rather than `instanceof`, because the
 * class the router would test against is its own copy's.
 */
const POSTPONED: symbol = Symbol.for("@uniflowed/server:postponed-read");

/**
 * A read of the request, made while `uf build` was prerendering a page it may
 * finish per request.
 *
 * Nothing should catch it. A component that does has rendered something other
 * than what it would render with a request, into a document every request is
 * then sent, and the boundary it was meant to leave as a hole is not one.
 */
export class PostponedReadError extends Error {
  /** The binding that was called, e.g. `cookies`. */
  binding: string;

  constructor(binding: string) {
    super(
      `@uniflowed/server: ${binding}() was called while \`uf build\` prerendered this page's ` +
        "static shell. The part of the page that reads it is rendered per request; " +
        "this error is how the build finds that part, and nothing should catch it.",
    );
    this.name = "PostponedReadError";
    this.binding = binding;
    // $FlowFixMe[prop-missing] the marker is keyed by a registered symbol on purpose.
    this[POSTPONED] = true;
  }
}

/** A scope that has read nothing yet. */
export function newPartialPrerender(): PartialPrerender {
  return { reads: [] };
}

/** Run `body` as a partial prerender, recording into `scope`. */
export function runPartialPrerender<T>(scope: PartialPrerender, body: () => T): T {
  return scopes.run(scope, body);
}

/**
 * Throw a [`PostponedReadError`] for `binding` if this call is inside a partial
 * prerender, and return otherwise.
 */
export function postponeInPartialPrerender(binding: string): void {
  const scope = scopes.getStore();
  if (scope == null) {
    return;
  }
  scope.reads.push(binding);
  throw new PostponedReadError(binding);
}

/** Whether `error` is a [`PostponedReadError`], from any copy of this package. */
export function isPostponedRead(error: mixed): boolean {
  if (error == null || typeof error !== "object") {
    return false;
  }
  // $FlowFixMe[invalid-computed-prop] read by the symbol the constructor wrote.
  return error[POSTPONED] === true;
}
