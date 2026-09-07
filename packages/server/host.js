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
// reach another's internals: `@uniflowed/router` renders and dispatches inside
// a request, and it is a different npm package.
//
// # `beginRequest` is the one a host calls
//
// The other exports are what it is made of, and they are public because the
// suite drives them one at a time and because a host with an unusual shape may
// need them. A host that reaches for them separately is nonetheless doing the
// thing that produced ubugeeei-prod/uf#389: `@uniflowed/router` used to build a
// context in its middleware runner and drain it there, and build a second one
// in its dispatcher, so a request had up to two contexts and `after()` ran
// before the response existed. One request is one `beginRequest`, and the pair
// it returns is deliberately awkward to call from a single place — `run` wraps
// deciding the response, `settle` follows writing it.
//
// # `insideRequest` and `noteRoute` are called from inside one
//
// Both are here anyway, because the audience is what decides the subpath and
// the audience for both is a framework rather than an application. They are
// what `@uniflowed/router` needs and a page must never touch: one asks whether
// a host did its half, and the other records which route pattern claimed the
// request so that the host's log line can say `/orders/:id` instead of
// `/orders/8813`. Neither answers anything *about* the request, which is the
// test for whether something belongs in the root instead.
//
// # Which module's copy
//
// This one holds an `AsyncLocalStorage`, so the context is only shared by code
// that resolved to the *same* copy of it. A host that serves a bundled
// application must therefore take `beginRequest` from that bundle —
// `virtual:uf/server` re-exports it for exactly this reason — and not from its
// own `node_modules`, where it would be a second storage that sees nothing.

export type {
  CookieStore,
  DraftMode,
  HeaderStore,
  RequestContext,
  RequestLifecycle,
} from "./internal/context.js";

export {
  beginRequest,
  contextFor,
  drainDeferred,
  insideRequest,
  noteRoute,
  parseCookies,
  runWithContext,
} from "./internal/context.js";
