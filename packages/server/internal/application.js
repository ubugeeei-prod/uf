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

export type { RequestLifecycle } from "./context.js";

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

/** What the project's server bundle exports; see `virtual:uf/server`. */
export type Application = {|
  /** Render `url`, resolving when the shell is ready. */
  readonly render: (
    url: string,
    assets: DocumentAssets,
    options?: {| readonly onError?: (error: mixed) => void |},
  ) => Promise<RenderedDocument>,
  /** The route handler for this request, or `null` when no handler claims it. */
  readonly dispatch: (request: Request) => Promise<Response | null>,
  /**
   * The guard on the path, run before anything under it answers.
   *
   * Called rather than tested for: a server bundle without it is a `TypeError`
   * on the first request, not an application whose auth check quietly stopped
   * running once it was built. See ubugeeei-prod/uf#260.
   */
  readonly runMiddleware: (request: Request) => Promise<Response | null>,
  /**
   * Begin the request everything above runs inside.
   *
   * A host calls this, runs the whole of answering the request inside `run`,
   * and calls `settle` once the response has been written — which is what
   * `after()` means by "sent" and is a different line in every host.
   *
   * It is on the bundle rather than importable beside this type, and that is
   * the one thing about it that looks wrong and is not: the request lives in an
   * `AsyncLocalStorage` belonging to a module *instance*, and the instance the
   * application reads is the one bundled into its own `server.js`. A host that
   * began a request in any other copy would fail silently — the guard would
   * run, the page would render, and every `cookies()` in it would throw as
   * though no host had run at all. See ubugeeei-prod/uf#389.
   */
  readonly beginRequest: (request: Request) => RequestLifecycle,
|};
