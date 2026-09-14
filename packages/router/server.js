// @flow
//
// Rendering one URL to an HTML document.
//
// `virtual:uf/server` calls `createRenderer` with the app root and the route
// table, and `uf dev` streams every document request through `render` while
// `uf build` writes every static route through `prerender`. Both produce the
// same markup from the same code, which is the point.
//
// # Two entry points, because there are two questions
//
// `render` streams and `prerender` waits, and which one a host wants is not a
// detail of how it was called — it is what the host is *for*. A server has a
// browser on the other end and a reason to send the layouts and the fallbacks
// now; a build writes a file that something may later serve to a crawler, and a
// file whose content is a `<template>` waiting for a script to move it is a
// file that is blank to everything but a browser.
//
// They were one function with `renderToString` behind it, which answered the
// first question by giving up on it: nothing streamed, so nothing could
// usefully suspend, so `$loading.js` had nothing to be. Making the split
// explicit is the point of ubugeeei-prod/uf#254 rather than a side effect —
// `internal/stream.js` holds the mechanics and says which React renderer serves
// which.

import { noteRoute } from "@uniflowed/server/host";
import { ROOT_ID } from "./internal/document.js";
import * as React from "react";

import {
  type DocumentBody,
  type DocumentShell,
  type WritableLike,
  bodyOfText,
  prerenderDocument,
  renderDocument,
} from "./internal/stream.js";

import {
  type AppProps,
  type ResolvedRoute,
  type RouteTable,
  RedirectError,
  installRoutes,
  resolveFailure,
  resolveMatch,
} from "./internal/runtime.js";

import { type StreamDiagnostic, type StreamRecord, streamReporter } from "./internal/inspector.js";

import {
  type ClientModuleLoader,
  installServerModules,
  readPayload,
} from "./internal/flight-ssr.js";
import { FLIGHT_CONTENT_TYPE, flightUrl } from "./internal/flight.js";
import type { FlightRenderer } from "./rsc.js";

/** Asset URLs to reference from the document. */
export type RenderAssets = {|
  readonly scripts: $ReadOnlyArray<string>,
  readonly styles: $ReadOnlyArray<string>,
  readonly preloads: $ReadOnlyArray<string>,
|};

/**
 * A document that has begun.
 *
 * `status` and `headers` are known once the shell is ready, which is the moment
 * this resolves and is why a streaming renderer can still answer with a status
 * line. The body arrives afterwards, through exactly one of `pipe`, `stream`
 * and `text` — they are three views of one pass over the same chunks, not three
 * copies of the document.
 */
export type RenderResult = {|
  readonly status: number,
  readonly headers?: { readonly [string]: string },
  /** Write the document into a Node response. */
  readonly pipe: (destination: WritableLike) => Promise<void>,
  /** The document as a web stream, for `new Response(…)`. */
  readonly stream: () => ReadableStream,
  /** The whole document, once it has finished streaming. */
  readonly text: () => Promise<string>,
  /**
   * The exception this render fell back to its error boundary for.
   *
   * The document is still a document — the boundary rendered — and this is how
   * the caller learns that it is an error page rather than the page it asked
   * for. `uf build` fails the route it names; `uf dev` reports it in the
   * terminal. Without it, containment would mean a build that quietly wrote a
   * directory of error pages and exited 0.
   *
   * `forbidden()` and `unauthorized()` do not set it: those are answers an
   * application chose, and a build that prerendered one has not failed.
   *
   * Only the failures known before the first byte: a loader that threw, or a
   * shell that did. An exception inside a `<Suspense>` boundary happens after
   * this has been read, so it is reported through `render`'s `onError` instead
   * — a streaming renderer cannot put a late failure in a value the caller
   * already has.
   */
  readonly error?: mixed,
|};

/** A document that is finished: every boundary resolved, nothing left to wait for. */
export type PrerenderResult = {|
  readonly status: number,
  readonly html: string,
  readonly headers?: { readonly [string]: string },
  /** The exception this render fell back to its error boundary for; see [`RenderResult`]. */
  readonly error?: mixed,
  /**
   * The Flight payload the document was rendered from, for a document React
   * Server Components rendered.
   *
   * `uf build` writes it beside the document as the route's payload file, so a
   * browser that navigates to a prerendered route fetches a file rather than
   * asking a server — which is what makes a static host able to serve client
   * navigation at all. Absent for a document rendered from its modules.
   */
  readonly payload?: Uint8Array,
|};

/**
 * A route's payload, as a browser navigating to it is answered.
 *
 * `stream` is `null` for a redirect, whose `location` is already the target's
 * payload URL when the target is on this origin: `fetch` follows it and lands
 * on a payload.
 */
export type FlightResponse = {|
  readonly status: number,
  readonly headers: { readonly [string]: string },
  readonly stream: ReadableStream<Uint8Array> | null,
  /** The exception the route resolved to its error boundary for; see [`RenderResult`]. */
  readonly error?: mixed,
|};

/** What a host may tell the renderer about one request. */
export type RenderOptions = {|
  /**
   * Every exception React recovered from, including the ones it answered by
   * streaming a boundary's fallback after the response had begun.
   *
   * A callback rather than a field on the result, because that is the shape of
   * the truth: by the time one of these happens the caller is already writing
   * bytes. `uf dev` reports them in the terminal; a production host logs them.
   */
  readonly onError?: (error: mixed) => void,
  /**
   * Rewrite the document opening — the head and, when present, the body start
   * tag — before it goes out.
   *
   * For `uf dev` and nothing else. Vite's `transformIndexHtml` injects
   * `/@vite/client` and the refresh preamble and rewrites asset URLs, and it
   * is a *whole document* hook, so the development server used to collect the
   * page and transform it at the end. That made the one place a developer
   * would notice streaming the one place it did not happen: a slow page showed
   * nothing until it was finished, and `$loading.js` looked broken.
   * See ubugeeei-prod/uf#374.
   *
   * A production host passes nothing here and streams as it always did.
   *
   * # What a plugin that injects into the body gets
   *
   * `transformIndexHtml` is a whole-document hook and this hands it only the
   * parseable opening of the document. Measured against Vite 8.2.2, injecting
   * all four positions into a whole document and into the streamed opening:
   *
   * | `injectTo`     | whole document      | streamed |
   * | -------------- | ------------------- | -------- |
   * | `head-prepend` | after `<head>`      | same     |
   * | `head`         | before `</head>`    | same     |
   * | `body-prepend` | after `<body>`      | same     |
   * | `body`         | before `</body>`    | after `<body>`     |
   *
   * Nothing is dropped — every tag still reaches the document — but a `body`
   * tag lands at the top of the body rather than after the content, because
   * the content is deliberately not passed to the hook. That keeps Vite's
   * parser away from chunk boundaries that may sit inside an attribute.
   *
   * uf's own injections are `head` and `head-prepend`, and Vite's client is
   * head-injected, so this is about a third-party plugin.
   * `packages/vite/dev-head-transform.test.js` pins the table above, so the day
   * it changes is a failing test rather than a surprise.
   */
  readonly transformHead?: (html: string) => Promise<string>,
  /**
   * Told, in words, when a document streamed differently than it did last time.
   *
   * For `uf dev` and nothing else, like `transformHead` above. It answers the
   * half of ubugeeei-prod/uf#520 that is about the wire — what arrived, in what
   * order, and which part of the tree each chunk built — for the stream uf has
   * today, which is a document whose Suspense boundaries resolve independently.
   * `internal/inspector.js` is what it is and what it deliberately is not.
   *
   * A host that passes nothing here records nothing: no recorder is
   * constructed, and the chunks a production stream yields are untouched.
   *
   * It is handed a message and its detail lines rather than the record they
   * came from, because the caller is `@uniflowed/vite` — plain JavaScript, run
   * by Vite before any Flow transform exists, which is why `DEVTOOLS_HOOK` and
   * `DIAGNOSTIC_ENDPOINT` are spelled twice rather than imported. The
   * vocabulary of the report belongs on this side of that line.
   */
  readonly onStream?: (diagnostic: StreamDiagnostic) => void,
|};

/** The two ids the server writes and the client reads. */
export { DATA_ID, ROOT_ID } from "./internal/document.js";

/**
 * How a host begins the request everything below runs inside.
 *
 * Re-exported rather than left to the host to import, and the reason is the
 * one thing about `@uniflowed/server` that is easy to get wrong: the request
 * store is shared by every copy of one *release* of that package, and no more.
 * A host that resolved `@uniflowed/server/host` for itself — from its own
 * `node_modules`, or from outside the bundle a build produced — may hold a
 * different release, and would begin a request in a store the application
 * never reads, so every `cookies()` in it would still be outside one, silently.
 * Handing it out from here makes the copy the host begins with the copy this
 * module dispatches and renders with, because it is the same import.
 *
 * `run` wraps everything that decides the response; `settle` is called once
 * the response has been *written*, which is a different line in every host.
 * `createMiddlewareRunner` and `createDispatcher` refuse to run outside it.
 * See ubugeeei-prod/uf#389.
 */
export type { RequestLifecycle } from "@uniflowed/server/host";
export { beginRequest } from "@uniflowed/server/host";

export type { Handler, HandlerContext, HandlerModule, HandlerRecord } from "./handler.js";
export { createDispatcher } from "./handler.js";

export type {
  Middleware,
  MiddlewareContext,
  MiddlewareModule,
  MiddlewareRecord,
} from "./middleware.js";
export { createMiddlewareRunner } from "./middleware.js";

/**
 * The endpoint a `"use server"` export is dialled at.
 *
 * Here rather than beside `@uniflowed/router/action`, which is the browser's
 * half of the same feature and must stay reachable from a client component:
 * this one refuses outside a request, so it imports `internal/request.js` and
 * through it `node:async_hooks`. The two halves share `internal/action-wire.js`
 * and nothing else, which is what keeps one grammar rather than two.
 *
 * `virtual:uf/server` calls it with the table `virtual:uf/actions` built from
 * the RSC manifest, and every host runs it between the middleware and the
 * route handlers. See `internal/action-endpoint.js` for what the endpoint
 * refuses and why.
 */
export type { ActionModule, ActionRecord } from "./internal/action-endpoint.js";
export { createActionDispatcher } from "./internal/action-endpoint.js";

/**
 * What a URL turned out to be: a route to render, or a redirect to answer with.
 *
 * Tagged, and returned rather than thrown, because both entry points need the
 * same answer and a redirect is the one thing `resolveMatch` lets out. Without
 * the tag this would be a union of two exact objects and reading either field
 * would be a type error on the branch that does not have it.
 */
type Resolution =
  | {| readonly kind: "route", readonly route: ResolvedRoute |}
  | {| readonly kind: "redirect", readonly error: RedirectError |};

/** A redirect, as the finished document `prerender` answers with. */
async function redirectResult(document: RenderResult): Promise<PrerenderResult> {
  return {
    status: document.status,
    headers: document.headers,
    html: await document.text(),
  };
}

/** The two ways one app answers for a URL. */
export type Renderer = {|
  readonly render: (
    url: string,
    assets: RenderAssets,
    options?: RenderOptions,
  ) => Promise<RenderResult>,
  readonly prerender: (
    url: string,
    assets: RenderAssets,
    options?: RenderOptions,
  ) => Promise<PrerenderResult>,
  /**
   * A route's payload, for a browser that is navigating rather than loading a
   * document. Only a renderer for React Server Components has one.
   */
  readonly flight?: (
    url: string,
    options?: {| readonly onError?: (error: mixed) => void |},
  ) => Promise<FlightResponse>,
|};

export function createRenderer(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
  readonly errors: RouteTable["errors"],
|}): Renderer {
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };
  installRoutes(table);
  const { App } = options;

  /**
   * The route to render, or the redirect to answer with instead.
   *
   * Shared by both entry points, because *what* a URL resolves to has nothing
   * to do with how the answer is delivered — with one exception, which is
   * `defer` and is the exception that proves it. Whether the router may hand
   * the page a loader that has not answered yet *is* a question about delivery:
   * only a renderer with a `<Suspense>` fallback to send first has anywhere to
   * put the wait. `render` says yes and `prerender` says no; see
   * `ResolveOptions.defer` and ubugeeei-prod/uf#373.
   *
   * Returning the redirect rather than throwing it keeps the two callers from
   * each having to remember that a redirect is the one thing `resolveMatch`
   * lets out.
   *
   * `onMatch` is what makes the request's log line say `/orders/:id` rather
   * than `/orders/8813`. It is handed to `resolveMatch` rather than read off
   * the route this returns, because a loader runs *inside* that call and a
   * loader has things to log: recording the route afterwards would leave every
   * line the loader wrote claiming to belong to no route at all. `noteRoute`
   * does nothing outside a request, which is what lets `prerender` — a build,
   * with no request anywhere — call the same function.
   */
  async function resolve(url: string, defer: boolean): Promise<Resolution> {
    try {
      return {
        kind: "route",
        route: await resolveMatch(table, url, { defer, onMatch: noteRoute }),
      };
    } catch (error) {
      if (error instanceof RedirectError) {
        return { kind: "redirect", error };
      }
      throw error;
    }
  }

  async function render(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<RenderResult> {
    const resolution = await resolve(url, true);
    if (resolution.kind === "redirect") {
      return redirectDocument(resolution.error);
    }
    let resolved: ResolvedRoute = resolution.route;
    const report = settings?.onError ?? (() => {});
    // Built once and shared by both renders below, so a page that threw its
    // shell away and rendered its error boundary instead reports the stream the
    // browser was actually sent rather than the one that was abandoned.
    const send = settings?.onStream;
    const onStream = send == null ? undefined : streamReporter(url, send);

    // React reports an exception to `onError` *and*, if it was in the shell, to
    // `onShellError` — so forwarding both would tell the host about one failure
    // twice, once through `onError` and once as `result.error` after this
    // re-renders. Errors are held until the shell is known to have survived;
    // if it did not, they are the failure the caller is about to be handed, and
    // the render they came from is being thrown away with them.
    let streaming = false;
    let held: Array<mixed> = [];
    const onError = (error: mixed) => {
      if (streaming) {
        report(error);
        return;
      }
      held.push(error);
    };

    let body: DocumentBody;
    try {
      body = await renderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(assets),
        onError,
        transformHead: settings?.transformHead,
        onStream,
      });
      streaming = true;
      // Recovered before the shell was ready: a `<Suspense>` boundary whose
      // content threw while the shell was still rendering. The response is
      // fine and the host still has to hear about it.
      for (const error of held) {
        report(error);
      }
      held = [];
    } catch (error) {
      // The server's half of the error boundary. React runs a class boundary
      // inside a `<Suspense>` and not outside one, so a throw in the shell —
      // the layouts, or a page with no boundary above it — still reaches here
      // rather than `RouteView`'s. Nothing has been written yet, which is what
      // makes answering with a different document possible at all: `onShellError`
      // fires before the first byte, and once it has not, this is unreachable.
      // See ubugeeei-prod/uf#257.
      if (error instanceof RedirectError) {
        return redirectDocument(error);
      }
      held = [];
      resolved = await resolveFailure(table, url, error);
      // Deliberately not caught again: this render is the boundary's own
      // component, and a boundary that throws has nothing left to answer with.
      // It reaches `uf dev`'s overlay and fails `uf build`'s route, which is
      // where somebody can fix it.
      streaming = true;
      body = await renderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(assets),
        onError,
        transformHead: settings?.transformHead,
        onStream,
      });
    }

    return {
      status: resolved.status,
      pipe: body.pipe,
      stream: body.stream,
      text: body.text,
      error: renderFailure(resolved),
    };
  }

  async function prerender(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<PrerenderResult> {
    const resolution = await resolve(url, false);
    if (resolution.kind === "redirect") {
      return redirectResult(redirectDocument(resolution.error));
    }
    let resolved: ResolvedRoute = resolution.route;
    const report = settings?.onError ?? (() => {});

    let html: string;
    try {
      html = await prerenderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(assets),
        onError: report,
      });
    } catch (error) {
      if (error instanceof RedirectError) {
        return redirectResult(redirectDocument(error));
      }
      resolved = await resolveFailure(table, url, error);
      html = await prerenderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(assets),
        onError: report,
      });
    }

    return { status: resolved.status, html, error: renderFailure(resolved) };
  }

  return { render, prerender };
}

/** What `virtual:uf/server` hands the renderer for React Server Components. */
export type DocumentRendererOptions = {|
  readonly App: React.ComponentType<AppProps>,
  /** The Flight renderer, from the module graph resolved under `react-server`. */
  readonly renderFlight: FlightRenderer,
  /** The server copy of the client module at a browser chunk URL. */
  readonly loadClientModule: ClientModuleLoader,
|};

/**
 * The renderer for an application rendered by React Server Components.
 *
 * [`createRenderer`] renders a route's modules into HTML. This one renders no
 * route module at all: the Flight renderer (`./rsc.js`), in the module graph
 * resolved under `react-server`, renders the route into a payload, and this
 * reads that payload with React's own Flight client and renders what comes
 * out. The same bytes are written into the document as they arrive, so the
 * browser hydrates the tree the server rendered rather than rendering the
 * route's modules a second time — which is what keeps a Server Component's
 * module out of the browser. See ubugeeei-prod/uf#519.
 *
 * The contract with a host is [`Renderer`]'s, unchanged: `render` resolves
 * when the shell is ready with a status and a body, `prerender` resolves with a
 * finished document, and `error` is the exception a route fell back to its
 * error boundary for. It adds `flight`, which answers a browser that is
 * navigating, and `prerender` adds the payload a static host serves for one.
 *
 * # When the shell throws
 *
 * The HTML renderer only ever sees React's serialisation of what a Server
 * Component threw — in a build, a sentence and a digest — and resolving the
 * error boundary needs the exception itself: `forbidden()` is a 403 and a
 * `RedirectError` is a redirect. So the first exception the Flight renderer
 * reports is kept, as thrown, and it is what the error route is resolved for.
 * An exception only the HTML renderer saw — a client component that threw
 * while it rendered on the server — is resolved for as it is.
 */
export function createDocumentRenderer(options: DocumentRendererOptions): Renderer {
  const { App, renderFlight } = options;
  installServerModules(options.loadClientModule);

  /**
   * The document for one payload: React's Flight client reads one copy of the
   * stream while the other copy is written into the document.
   */
  async function documentOf(
    stream: ReadableStream<Uint8Array>,
    url: string,
    assets: RenderAssets,
    settings: {|
      readonly onError: (error: mixed) => void,
      readonly transformHead?: (html: string) => Promise<string>,
      readonly onStream?: (record: StreamRecord) => void,
    |},
  ): Promise<DocumentBody> {
    const [forHtml, forBrowser] = stream.tee();
    try {
      return await renderDocument(<App url={url} flight={readPayload(forHtml)} />, {
        shell: shellFor(assets),
        onError: settings.onError,
        transformHead: settings.transformHead,
        onStream: settings.onStream,
        payload: forBrowser,
      });
    } catch (error) {
      void forBrowser.cancel();
      throw error;
    }
  }

  async function render(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<RenderResult> {
    const report = settings?.onError ?? (() => {});
    const send = settings?.onStream;
    const onStream = send == null ? undefined : streamReporter(url, send);
    const transformHead = settings?.transformHead;
    const ledger = createErrorLedger();

    // Held until the shell is known to have survived, for the reason
    // [`createRenderer`] holds them — and dropped rather than reported once the
    // render they came from has been given up for the error boundary. That
    // render goes on reporting while it stops, and nothing it says then is
    // about the response.
    let streaming = false;
    let abandoned = false;
    let held: Array<mixed> = [];
    const onError = (error: mixed) => {
      if (abandoned) {
        return;
      }
      if (streaming) {
        report(error);
        return;
      }
      held.push(error);
    };

    const stop = new AbortController();
    const rendered = await renderFlight(url, {
      onError: (error: mixed) => {
        onError(error);
        return ledger.record(error);
      },
      signal: stop.signal,
    });
    if (rendered.kind === "redirect") {
      return redirectDocument(new RedirectError(rendered.location, rendered.status === 308));
    }
    let status = rendered.status;
    let failure = rendered.failure;
    let body: DocumentBody;
    try {
      body = await documentOf(rendered.stream, url, assets, {
        onError: (error: mixed) => {
          if (!ledger.isCopy(error)) {
            onError(error);
          }
        },
        transformHead,
        onStream,
      });
      streaming = true;
      for (const error of held) {
        report(error);
      }
      held = [];
    } catch (error) {
      abandoned = true;
      held = [];
      stop.abort();
      const cause = ledger.originalOf(error);
      if (cause instanceof RedirectError) {
        return redirectDocument(cause);
      }
      // Deliberately not caught again: this render is the boundary's own, and a
      // boundary that throws has nothing left to answer with. It reaches
      // `uf dev`'s overlay and fails `uf build`'s route, which is where somebody
      // can fix it.
      const recovered = await renderFlight(url, {
        failure: { error: cause },
        onError: (late: mixed) => {
          report(late);
          return ledger.record(late);
        },
      });
      if (recovered.kind === "redirect") {
        return redirectDocument(new RedirectError(recovered.location, recovered.status === 308));
      }
      status = recovered.status;
      failure = recovered.failure;
      body = await documentOf(recovered.stream, url, assets, {
        onError: (late: mixed) => {
          if (!ledger.isCopy(late)) {
            report(late);
          }
        },
        transformHead,
        onStream,
      });
    }

    return { status, pipe: body.pipe, stream: body.stream, text: body.text, error: failure };
  }

  async function prerender(
    url: string,
    assets: RenderAssets,
    settings?: RenderOptions,
  ): Promise<PrerenderResult> {
    const report = settings?.onError ?? (() => {});
    const ledger = createErrorLedger();
    // The Flight renderer's copy of an exception is the one reported, because it
    // is the exception as thrown; the HTML renderer's copy of the same one is
    // recognised by its digest and dropped. See [`createErrorLedger`].
    const onServerError = (error: mixed): string => {
      report(error);
      return ledger.record(error);
    };
    const onHtmlError = (error: mixed) => {
      if (!ledger.isCopy(error)) {
        report(error);
      }
    };

    // `defer: false`, for the reason [`createRenderer`]'s prerender passes it:
    // a file has no fallback to show first.
    const rendered = await renderFlight(url, { defer: false, onError: onServerError });
    if (rendered.kind === "redirect") {
      return redirectResult(
        redirectDocument(new RedirectError(rendered.location, rendered.status === 308)),
      );
    }
    let status = rendered.status;
    let failure = rendered.failure;
    let payload: Uint8Array;
    let html: string;
    try {
      // Inside the `try`, with the document: a payload the Flight renderer
      // could not finish — a function handed to a client component, say —
      // fails the stream itself, and that is this route's failure like any
      // other.
      payload = await bytesOf(rendered.stream);
      html = await staticDocumentOf(payload, url, assets, onHtmlError);
    } catch (error) {
      const cause = ledger.originalOf(error);
      if (cause instanceof RedirectError) {
        return redirectResult(redirectDocument(cause));
      }
      const recovered = await renderFlight(url, {
        defer: false,
        failure: { error: cause },
        onError: onServerError,
      });
      if (recovered.kind === "redirect") {
        return redirectResult(
          redirectDocument(new RedirectError(recovered.location, recovered.status === 308)),
        );
      }
      status = recovered.status;
      failure = recovered.failure;
      payload = await bytesOf(recovered.stream);
      html = await staticDocumentOf(payload, url, assets, onHtmlError);
    }
    return { status, html, error: failure, payload };
  }

  /** A finished document for a finished payload, with the payload written into it. */
  function staticDocumentOf(
    payload: Uint8Array,
    url: string,
    assets: RenderAssets,
    onError: (error: mixed) => void,
  ): Promise<string> {
    return prerenderDocument(<App url={url} flight={readPayload(streamOf(payload))} />, {
      shell: shellFor(assets),
      onError,
      payload: streamOf(payload),
    });
  }

  async function flight(
    url: string,
    settings?: {| readonly onError?: (error: mixed) => void |},
  ): Promise<FlightResponse> {
    const rendered = await renderFlight(url, { onError: settings?.onError });
    if (rendered.kind === "redirect") {
      const { location } = rendered;
      // A redirect on this origin points at its target's payload, so the
      // browser's `fetch` follows it and lands on one. A redirect elsewhere is
      // left as written: what the browser finds there is not a payload, and it
      // loads the URL as a document instead.
      const onThisOrigin = location.startsWith("/") && !location.startsWith("//");
      return {
        status: rendered.status,
        headers: { location: onThisOrigin ? flightUrl(location) : location },
        stream: null,
      };
    }
    return {
      status: rendered.status,
      headers: { "content-type": FLIGHT_CONTENT_TYPE },
      stream: rendered.stream,
      error: rendered.failure,
    };
  }

  return { render, prerender, flight };
}

/**
 * Pairs each exception the Flight renderer reports with the copy of it the HTML
 * renderer sees.
 *
 * A Server Component that throws is seen twice. The Flight renderer catches the
 * exception as it was thrown and writes an error row where that part of the
 * tree would have been; React's Flight client reads the row and throws an error
 * rebuilt from it, which is what the HTML renderer sees at the same spot — and
 * in a build, that error's message is a sentence saying the real one was
 * omitted. Reporting both tells a host about one failure twice, once uselessly.
 * Resolving the error boundary for the rebuilt one is worse: `forbidden()`
 * would become a 500 and `redirect()` an error page, because neither survives
 * being rebuilt as a plain `Error`.
 *
 * The row carries a digest, and that is React's field for exactly this: what
 * the Flight renderer's `onError` returns is written into the row and set on the
 * rebuilt error as `digest`. So every exception is recorded here under a digest
 * of its own, and an error the HTML renderer reports with a recorded digest is a
 * copy — dropped from the report, and exchanged for the original when the
 * boundary is resolved.
 *
 * The digest reaches the browser inside the payload, and says nothing: it counts
 * the exceptions one render has recorded.
 */
function createErrorLedger(): ErrorLedger {
  const originals: Map<string, mixed> = new Map();
  const recorded = (error: mixed): string | null => {
    const digest = digestOf(error);
    return digest != null && originals.has(digest) ? digest : null;
  };
  return {
    record(error: mixed): string {
      const digest = `uf:${originals.size + 1}`;
      originals.set(digest, error);
      return digest;
    },
    isCopy(error: mixed): boolean {
      return recorded(error) != null;
    },
    originalOf(error: mixed): mixed {
      const digest = recorded(error);
      return digest == null ? error : originals.get(digest);
    },
  };
}

/** See [`createErrorLedger`]. */
type ErrorLedger = {|
  /** Keep an exception the Flight renderer reported, and return its digest. */
  readonly record: (error: mixed) => string,
  /** Whether an error the HTML renderer reported is the copy of a kept one. */
  readonly isCopy: (error: mixed) => boolean,
  /** The exception as thrown, for a copy; any other error, as it is. */
  readonly originalOf: (error: mixed) => mixed,
|};

/** The digest React set on an error, when it set one. */
function digestOf(error: mixed): string | null {
  if (error == null || typeof error !== "object") {
    return null;
  }
  const tagged: { +digest?: mixed, ... } = (error: $FlowFixMe);
  return typeof tagged.digest === "string" ? tagged.digest : null;
}

/** Every byte of a stream, as one array. */
async function bytesOf(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  while (true) {
    const step = await reader.read();
    if (step.done === true) {
      break;
    }
    const chunk = step.value;
    if (chunk != null) {
      chunks.push(chunk);
      total += chunk.byteLength;
    }
  }
  const bytes = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.byteLength;
  }
  return bytes;
}

/** A stream of one array, read as many times as a caller constructs one. */
function streamOf(bytes: Uint8Array): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(bytes);
      controller.close();
    },
  });
}

/** The exception a resolved route fell back to its error boundary for. */
function renderFailure(resolved: ResolvedRoute): mixed {
  if (resolved.error == null) {
    return undefined;
  }
  return match (resolved.error) {
    {kind: "thrown", error: const error} => error,
    {kind: "unauthorized"} => undefined,
    {kind: "forbidden"} => undefined,
  };
}

function redirectDocument(error: RedirectError): RenderResult {
  const target = escapeAttribute(error.to);
  // A document rather than an empty body, because a redirect is still an answer
  // a browser may be shown; it goes through the same three methods as a
  // rendered one so that a host has one shape to write, not two.
  const body = bodyOfText(
    `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=${target}"><title>Redirecting</title></head><body><a href="${target}">Redirecting…</a></body></html>\n`,
  );
  return {
    status: error.permanent ? 308 : 307,
    headers: { Location: error.to },
    pipe: body.pipe,
    stream: body.stream,
    text: body.text,
  };
}

/**
 * The document uf writes around the app's markup.
 *
 * The same two shapes `assemble` chose between, decided from the same evidence
 * — whether the markup opens with `<html>` — but stated up front instead of
 * afterwards, because a stream has no "afterwards" in which to splice a head.
 * An app whose root layout renders `<html>` owns the whole document and the
 * client hydrates `document`, so uf contributes only the tags that go before
 * `</head>`. An app that renders only content is wrapped in a minimal shell
 * around `<div id="uf-root">`, which is what the client hydrates instead.
 *
 * `internal/stream.js` picks between them on the opening bytes React writes;
 * everything either shape is made of is here, so what a uf document contains is
 * still readable in one place.
 *
 * # Why the shell is three strings and not one
 *
 * Because uf's own `<head>` has to still be open when React's head tags arrive.
 * React hoists a `<title>`, a `<meta>` and a `<link>` into the head it wrote
 * itself, and here it wrote none — so with one string this shell closed its
 * head before the app had rendered a byte, and every `og:` tag and the
 * `<link rel="canonical">` landed in the body, where a crawler ignores them.
 * `open` is uf's head up to that point, `body` is the rest of it and the
 * wrapper, and what goes between them is whatever `assembled` lifts out of the
 * app's own markup. See ubugeeei-prod/uf#547.
 *
 * That is also why no `<title>` is written here any more. It was, from
 * `resolved.metadata.title` — the same string `Head` renders — so a document
 * carried two of them, one in each place, and only one was where a browser
 * looks. Hoisting the rendered one leaves the metadata with a single source.
 */
/**
 * The document a single-page build writes, and the only one it writes.
 *
 * `app.rendering.modes: ["csr"]` renders no route at build time: the client
 * router resolves and renders every one of them in the browser, so what the
 * build has to leave behind is the *chrome* — the stylesheets, the module
 * script, and the empty root the client renders into. That is exactly
 * [`shellFor`]'s three strings with nothing between them, which is why this is
 * three concatenations rather than a fourth shape of document to keep in step
 * with the other three.
 *
 * No React runs. There is nothing to render: no URL has been asked for, and
 * whatever this document is served for is decided by the host rather than by
 * this build.
 *
 * # What it costs, said here because it is not visible from the file
 *
 * The document has no `<title>`, no `<meta name="description">` and no content.
 * A crawler that runs no JavaScript sees an empty page for **every** URL, and a
 * reader sees nothing until the bundle has loaded and the route has resolved.
 * That is what a single-page application is, and it is why `modes: ["csr"]` is
 * a declaration a project makes rather than something a build falls back to.
 * A project that wants a document per route has `ssg`, and one that wants a
 * document per request has `ssr`.
 */
export function shellDocument(assets: RenderAssets): string {
  const shell = shellFor(assets);
  return `${shell.open}${shell.body}${shell.close}`;
}

function shellFor(assets: RenderAssets): DocumentShell {
  const head = headTags(assets);
  return {
    head,
    open: `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">`,
    body: `${head}</head><body><div id="${ROOT_ID}">`,
    close: `</div></body></html>\n`,
  };
}

function headTags(assets: RenderAssets): string {
  let tags = "";
  for (const href of assets.styles) {
    tags += `<link rel="stylesheet" href="${escapeAttribute(href)}">`;
  }
  for (const href of assets.preloads) {
    tags += `<link rel="modulepreload" href="${escapeAttribute(href)}">`;
  }
  for (const src of assets.scripts) {
    tags += `<script type="module" src="${escapeAttribute(src)}"></script>`;
  }
  return tags;
}

function escapeAttribute(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
