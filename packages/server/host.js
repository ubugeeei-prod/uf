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
// # `asResponder`, `insideRequest` and `noteRoute` are called from inside one
//
// All three are here anyway, because the audience is what decides the subpath
// and the audience for all three is a framework rather than an application. They are
// what `@uniflowed/router` needs and a page must never touch: one asks whether
// a host did its half, one records which route pattern claimed the request so
// that the host's log line can say `/orders/:id` instead of `/orders/8813`, and
// `asResponder` marks the call that owns the response — which is what makes
// `draftMode().enable()` legal in a route handler and refused in a render.
// None of the three answers anything *about* the request, which is the test for
// whether something belongs in the root instead.
//
// # Which module's copy
//
// The request store behind these functions is the process's rather than this
// module's: `./internal/process-state.js` keeps one per process, so every copy
// of this release of the package reads the request any other copy began —
// including the second copy held by the module graph that renders React Server
// Components. It used to be one store per copy, and a host that began a request
// through its own copy began it somewhere no page could see.
//
// A host that serves a bundled application still takes `beginRequest` from that
// bundle — `virtual:uf/server` re-exports it — and not from its own
// `node_modules`, because its own copy is not necessarily this *release*. Two
// releases whose request context differs in shape keep separate stores on
// purpose, and the bundle's copy is the one that is certainly the release the
// application reads.

export type {
  CookieStore,
  DraftMode,
  HeaderStore,
  RequestContext,
  RequestLifecycle,
} from "./internal/context.js";

export {
  asResponder,
  beginRequest,
  contextFor,
  drainDeferred,
  insideRequest,
  noteRoute,
  parseCookies,
  runWithContext,
} from "./internal/context.js";
