// @flow
//
// Internal to `@uniflowed/server`: what a built uf application is, as a type.
//
// Three shapes, and every front door onto a build needs all three — the asset
// URLs a rendered document references, what one render answers with, and the
// module `uf build` writes to `.uf/build/server/server.js`.
//
// They are here rather than in whichever module happened to be written first
// because there are now three readers of them: [`../fetch.js`] for a host that
// speaks `Request` and `Response`, [`../standalone.js`] for a compiled binary,
// and `@uniflowed/vite`'s preview and start servers through the first of
// those. A type re-declared once per reader is how two of them come to
// disagree about what `render` returns, which is a disagreement no test sees
// until a deployment answers differently from the preview it was checked with.

import type { RequestLifecycle } from "./context.js";
import type { RoutingRules } from "./routing.js";

/**
 * React's `ReactFormState`: a form action's result, the `useActionState` key
 * that submitted, the action's id, and how many arguments the action was bound
 * with besides the state. The same tuple `@uniflowed/router` builds in
 * `internal/form-action.js`; spelled here so this package does not import the
 * router for a type.
 */
export type FormState = [mixed, string, string, number];

export type { RequestLifecycle } from "./context.js";
export type { RoutingRules } from "./routing.js";

/**
 * Where a document is written, when the host has a Node stream.
 *
 * The two methods React's own `pipe` uses and nothing else, declared
 * structurally so this module holds no Node types: `../fetch.js` is bundled
 * for hosts that have no `node:stream` to import one from.
 */
export type WritableLike = {
  readonly write: (chunk: string | Uint8Array) => mixed,
  readonly end: () => mixed,
  ...
};

/** The script, stylesheet and preload URLs a rendered document references. */
export type DocumentAssets = {|
  readonly scripts: $ReadOnlyArray<string>,
  readonly styles: $ReadOnlyArray<string>,
  readonly preloads: $ReadOnlyArray<string>,
  /**
   * The build these URLs belong to, as the document publishes it.
   *
   * Written by `uf build` and absent everywhere else — `uf dev`, and a build
   * recorded before it existed — which means "no skew check". A document
   * carries it as `<meta name="uf:deployment">`, the browser sends it back on
   * every action call and payload request, and a front door answers one that
   * names another build with a `409` rather than running anything. See
   * `./deployment.js`.
   */
  readonly deployment?: string,
|};

/**
 * A document that has begun.
 *
 * `status` and `headers` are known once the shell is ready, which is why a
 * streaming renderer can still answer with a status line. The body arrives
 * afterwards through exactly one of `pipe` and `stream` — each is a single
 * pass over the same chunks, so calling both would read a document twice.
 */
export type RenderedDocument = {|
  readonly status: number,
  readonly headers?: { readonly [string]: string },
  readonly pipe: (destination: WritableLike) => mixed,
  readonly stream: () => ReadableStream<Uint8Array>,
|};

/**
 * A page's static shell, as `uf build` recorded it and `@uniflowed/router`
 * resumes it.
 *
 * Stated here rather than imported, for the reason [`RenderedDocument`] is:
 * this package links no renderer, and what crosses between the two is data.
 * `@uniflowed/router`'s `internal/stream.js` is where each field is argued.
 */
export type PrerenderedShell = {|
  readonly html: string,
  readonly close: string,
  readonly rootDepth: number,
  readonly postponed: mixed,
|};

/** What the project's server bundle exports; see `virtual:uf/server`. */
export type Application = {|
  /** Render `url`, resolving when the shell is ready. */
  readonly render: (
    url: string,
    assets: DocumentAssets,
    options?: {|
      readonly onError?: (error: mixed) => void,
      /**
       * React's `formState` for a page rendered in answer to a form posted
       * before hydration, so the `useActionState` that submitted starts from
       * the action's result. Only `callAction`'s `postback` passes one.
       */
      readonly formState?: FormState,
    |},
  ) => Promise<RenderedDocument>,
  /**
   * A route's Flight payload, for a browser that is navigating.
   *
   * Present on a bundle React Server Components render and absent on one
   * rendered from its modules (`app.rsc: false`), whose browser navigates by
   * importing routes and never asks. `url` is the document's path and query,
   * not the payload's. `stream` is `null` for a redirect, whose `location`
   * already names the target's payload. See `./flight.js`.
   */
  readonly flight?: (
    url: string,
    options?: {|
      readonly onError?: (error: mixed) => void,
      readonly interceptedFrom?: string,
      /** Render the URL's not-found page rather than its route. */
      readonly notFound?: boolean,
    |},
  ) => Promise<{|
    readonly status: number,
    readonly headers: { readonly [string]: string },
    readonly stream: ReadableStream<Uint8Array> | null,
    readonly error?: mixed,
  |}>,
  /**
   * Answer a page `uf build` prerendered partially: its static shell first,
   * then its holes as this request renders them.
   *
   * Present on a bundle React Server Components render, which is the only kind
   * that writes a shell. `shell` is what the build recorded for the page; see
   * `../fetch.js`'s `PartialPrerenders`.
   */
  readonly resume?: (
    url: string,
    assets: DocumentAssets,
    shell: PrerenderedShell,
    options?: {| readonly onError?: (error: mixed) => void |},
  ) => Promise<RenderedDocument>,
  /** The route handler for this request, or `null` when no handler claims it. */
  readonly dispatch: (request: Request) => Promise<Response | null>,
  /**
   * The server action this request names, or `null` when it names none.
   *
   * Between the guard and the handlers in every host, and it answers every
   * request carrying an action id — including every refusal — so a `POST`
   * naming an action can never fall through to a route handler at the same
   * path. See `npm/router/internal/action-endpoint.js` for what it
   * refuses and why, and `docs/security.md` for the boundary those refusals
   * are keeping.
   *
   * Called rather than tested for, for the reason `runMiddleware` is: a
   * server bundle without it is a `TypeError` on the first request rather than
   * an application whose actions quietly stopped being reachable once it was
   * built.
   */
  readonly callAction: (
    request: Request,
    settings?: {|
      /**
       * Render the page this request is for with `formState`: the answer to a
       * `useActionState` form posted before its page hydrated. A host passes
       * it; without one such a post is answered with a `303` back to the page.
       */
      readonly postback?: (formState: FormState) => Promise<Response>,
    |},
  ) => Promise<Response | null>,
  /**
   * The guard on the path, run before anything under it answers.
   *
   * Called rather than tested for: a server bundle without it is a `TypeError`
   * on the first request, not an application whose auth check quietly stopped
   * running once it was built. See ubugeeei-prod/uf#260.
   *
   * Three answers: a `Response` answers, `null` carries on, and a `Request` is
   * a middleware's `rewrite()` — the same request at another path, which the
   * host carries on with instead and which has already been past that path's
   * own middleware. See `npm/router/middleware.js`.
   */
  readonly runMiddleware: (request: Request) => Promise<Response | Request | null>,
  /**
   * `app.router.redirects`, `rewrites` and `headers` from `uf.config.js`, as the
   * build read them.
   *
   * On the bundle rather than read from the config where a host starts, so a
   * served build answers with the rules it was built with. Optional, because a
   * bundle from before the rules existed has none and means none. `./routing.js`
   * is what every host asks about them.
   */
  readonly routing?: RoutingRules,
  /**
   * Begin the request everything above runs inside.
   *
   * A host calls this, runs the whole of answering the request inside `run`,
   * and calls `settle` once the response has been written — which is what
   * `after()` means by "sent" and is a different line in every host.
   *
   * It is on the bundle rather than importable beside this type, and that is
   * the one thing about it that looks wrong and is not: the request store is
   * shared by every copy of one release of this package (see
   * `./process-state.js`), and the release the application reads is the one
   * bundled into its own `server.js`. A host that began a request through a
   * copy of another release would fail silently — the guard would run, the page
   * would render, and every `cookies()` in it would throw as though no host had
   * run at all. See ubugeeei-prod/uf#389.
   */
  readonly beginRequest: (request: Request) => RequestLifecycle,
|};
