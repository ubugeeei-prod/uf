// @flow
//
// Internal to `@uniflowed/mock`: the one global this package touches.
//
// Every write to `globalThis` is here, in a module small enough to read in a
// sitting, because a test tool that swaps a global and restores it badly is
// worse than no test tool at all — the damage lands in an unrelated file, an
// hour later, and looks like that file's bug.
//
// # What is intercepted
//
// `globalThis.fetch`, and nothing else. `uf test` runs on Node.js, Bun and
// Deno through the Capability JS Host contract, and `fetch` is the one request
// API all three have, so this is the layer that is portable across the hosts uf
// actually supports.
//
// It is genuinely less than MSW covers, and the difference is stated rather
// than glossed. MSW intercepts through `@mswjs/interceptors`, which also patches
// `XMLHttpRequest`, `node:http`/`node:https` (and so axios's Node adapter,
// `node-fetch`, `got`, `superagent`), `WebSocket` and `navigator.sendBeacon`.
// None of those are touched here. A request made through one of them reaches
// the network, is not recorded, and is not reported as unhandled — the registry
// never sees it. `index.js` says so in its Readiness section.
//
// # Why replacing `fetch` is enough for what it does cover
//
// A test's `fetch` may be the global, or a reference captured at module load
// (`const f = globalThis.fetch`), or one injected into a client. Only the first
// two are covered, and the second only when the module was loaded after
// `listen()`. That is the same limitation every fetch-patching tool has, and
// the answer for an injected `fetch` is to inject this one — which is why the
// registry keeps its dispatch reachable rather than hiding it inside the swap.
//
// # Resolving a relative URL
//
// `fetch("/users/1")` works in a browser and throws in Node, because there is
// no document to resolve against. Since this replaces `fetch` outright it can
// resolve one itself, against `location.origin` when a DOM is installed and a
// stated origin otherwise. A component that fetches a relative path therefore
// works under `uf test` unchanged, which is the whole point of testing it.

// uf-lint-disable fetch/no-global-override
//
// `fetch/no-global-override` exists to stop a package from silently unhooking
// the client the rest of the toolchain is instrumented around. This module is
// the one place in the repository where replacing `fetch` *is* the feature: it
// is entered only from `listen()`, it refuses to nest, and it puts back what it
// took. Disabled for the whole file rather than line by line because the rule
// also fires on *reading* `globalThis.fetch` and on the name appearing inside
// an error message, and four scattered suppressions would say less than one
// explained one.

import { isNetworkError } from "../response.js";

/**
 * The interceptor currently installed, or `null`.
 *
 * Module state, deliberately: there is one `globalThis.fetch` per host, so
 * "is something already intercepting" is a question about the process and not
 * about a registry. Two registries listening at once would nest, and the second
 * one's `close()` would restore the first one's interceptor as if it were the
 * platform's — so it is refused instead.
 */
let installed: typeof fetch | null = null;

/**
 * What the registry does with a request.
 *
 * A `Response` answers it. `null` means nobody claimed it and it should go to
 * the network. Throwing rejects the caller's `fetch`, which is what an
 * unhandled request does by default.
 */
export type Dispatch = (request: Request) => Promise<Response | null>;

/** Whether this process already has an interceptor installed. */
function isIntercepting(): boolean {
  return installed != null && globalThis.fetch === installed;
}

/**
 * Turn whatever `fetch` was called with into a `Request`.
 *
 * A `Request` passed with an `init` has to be rebuilt, because that is what
 * `fetch` itself does with the pair; passed alone it is used as it stands, so a
 * caller's own subclass and signal survive.
 */
function toRequest(input: RequestInfo, init: RequestOptions | void, origin: string): Request {
  if (input instanceof Request) {
    return init == null ? input : new Request(input, init);
  }
  const href = input instanceof URL ? input.href : String(input);
  // `new URL(href, origin)` leaves an absolute `href` alone and resolves a
  // relative one, so both cases are this single line.
  return new Request(new URL(href, origin).href, init);
}

/**
 * Replace `globalThis.fetch` until the returned function is called.
 *
 * Restoring is by assignment to the same property that was read, which is
 * correct here for a reason worth stating: `fetch` is an own property of
 * `globalThis` on every host uf supports, so there is no inherited value for an
 * assignment to shadow. The interceptor is compared by identity before it is
 * removed, so a suite that swapped `fetch` for its own after `listen()` does
 * not have that swap silently reverted by `close()`.
 */
export function installFetch(dispatch: Dispatch, origin: string): () => void {
  const original = globalThis.fetch;
  if (typeof original !== "function") {
    throw new Error(
      "@uniflowed/mock: this host has no globalThis.fetch to intercept; " +
        "uf supports Node.js 18+, Bun and Deno, all of which have one",
    );
  }
  if (isIntercepting()) {
    throw new Error(
      "@uniflowed/mock: fetch is already intercepted. A second listen() would " +
        "nest, and the inner close() would restore the outer interceptor as if " +
        "it were the platform's. Close the first registry first.",
    );
  }

  const intercepted: typeof fetch = async (input, init) => {
    const request = toRequest(input, init, origin);
    const answer = await dispatch(request);
    if (answer != null) {
      // `fetch` never *resolves* with a network error, it rejects — so a
      // handler that returned `HttpResponse.error()` has to reject here, or the
      // code under test would take a branch it can never take in production.
      if (isNetworkError(answer)) {
        throw new TypeError("Failed to fetch");
      }
      return answer;
    }
    // Passthrough hands on the normalised `Request` rather than the original
    // arguments: building it did not consume anything (the log clones), and
    // passing the `Request` keeps the resolved absolute URL a relative call
    // was given.
    return original(request);
  };

  // The one write to a global in this package. `globalThis` is not modelled as
  // a writable record by Flow, so the assignment needs the cast; the value
  // being assigned is fully typed as `typeof fetch` above, which is where the
  // guarantee actually lives.
  const host = globalThis as $FlowFixMe;
  host.fetch = intercepted;
  installed = intercepted;

  return () => {
    if (globalThis.fetch === intercepted) {
      host.fetch = original;
    }
    if (installed === intercepted) {
      installed = null;
    }
  };
}
