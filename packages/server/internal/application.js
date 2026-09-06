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
|};
