// @flow
//
// `@uniflowed/server/host`: how a host establishes a request.
//
// The other half of this package. `@uniflowed/server` is what an application
// calls *inside* a request; this is what a renderer, a route dispatcher or a
// server-action bridge calls to say that a request has begun and, later, that
// the response has gone.
//
// Two subpaths rather than one module, because they have opposite audiences and
// opposite rules. Everything in the root is safe to call from a component and
// meaningless outside a request; everything here is meaningless *inside* one
// and must be called exactly once around it. Mixing them would put
// `runWithContext` in the same import a page reaches for, which is an invitation
// to nest one request inside another.
//
// It is a subpath rather than `internal/` because a sibling package cannot
// reach another's internals: `@uniflowed/router` is where a request begins, and
// it is a different npm package.

export type { CookieStore, DraftMode, HeaderStore, RequestContext } from "./internal/context.js";

export { contextFor, drainDeferred, parseCookies, runWithContext } from "./internal/context.js";
