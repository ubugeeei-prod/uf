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

import { currentNonce, noteRoute } from "@uniflowed/server/host";
import { traceLoader } from "./internal/server-instrumentation.js";

import * as React from "react";

import {
  type DocumentBody,
  type PrerenderedShell,
  type WritableLike,
  prerenderDocument,
  renderDocument,
} from "./internal/stream.js";

export type { PrerenderedShell } from "./internal/stream.js";

import {
  type AppProps,
  type ResolvedRoute,
  type RouteTable,
  RedirectError,
  installRoutes,
  resolveFailure,
  resolveMatch,
} from "./internal/runtime.js";

import type { FormState } from "./internal/form-action.js";
import { type StreamDiagnostic, streamReporter } from "./internal/inspector.js";

import { redirectDocument, redirectResult, shellFor } from "./internal/shell.js";

/** Asset URLs to reference from the document. */
export type RenderAssets = {|
  readonly scripts: $ReadOnlyArray<string>,
  readonly styles: $ReadOnlyArray<string>,
  readonly preloads: $ReadOnlyArray<string>,
  /**
   * The build the document belongs to, written into its head as
   * `<meta name="uf:deployment">`. Absent under `uf dev`, where there is no
   * other build to be skewed against. See `./internal/deployment.js`.
   */
  readonly deployment?: string,
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
  /**
   * The page's static shell, when the prerender was partial and the page read
   * the request inside a `<Suspense>` boundary.
   *
   * `html` is then the shell's markup and not a document to write at the
   * page's URL: a server answers the page by sending the shell and rendering
   * the holes per request, through [`Renderer`]'s `resume`. No payload comes
   * with it, because the browser hydrates from the request's. Only a renderer
   * for React Server Components writes one.
   */
  readonly shell?: PrerenderedShell,
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
   * React's `formState` for a page rendered in answer to a form posted before
   * hydration; see `internal/form-action.js`. Only a host's `postback` passes
   * one.
   */
  readonly formState?: FormState,
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
  /**
   * For `prerender` only: whether a read of the request may be left for the
   * request rather than fail the page.
   *
   * `uf build` passes it when `app.rendering.modes` allows `ppr` and the build
   * leaves a server behind. A page that reads `cookies()`, `headers()` or
   * `draftMode()` inside a `<Suspense>` boundary is then written as a static
   * shell with that boundary as a hole — [`PrerenderResult`]'s `shell` — and a
   * read outside every boundary still fails the page, naming what it read.
   * The renderer for React Server Components honours it; a route rendered from
   * its modules (`app.rsc: false`) is prerendered whole or not at all.
   */
  readonly partial?: boolean,
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
 * `app.router.basePath` and `trailingSlash`, for `virtual:uf/server` to install
 * before the first render, and `basePath()` for a middleware or a route handler
 * that builds an address itself. See `./internal/base-path.js`.
 */
export type { RoutingSettings, TrailingSlash } from "./internal/base-path.js";
export { basePath, installRouting } from "./internal/base-path.js";

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
    options?: {|
      readonly onError?: (error: mixed) => void,
      readonly interceptedFrom?: string,
    |},
  ) => Promise<FlightResponse>,
  /**
   * A page `prerender` wrote as a static shell: the shell first, then its holes
   * as this request renders them. Only a renderer for React Server Components
   * has one, because only it writes a shell.
   */
  readonly resume?: (
    url: string,
    assets: RenderAssets,
    shell: PrerenderedShell,
    options?: RenderOptions,
  ) => Promise<RenderResult>,
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
        route: await resolveMatch(table, url, {
          defer,
          onMatch: noteRoute,
          runLoader: traceLoader,
        }),
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
    const report = settings?.onError ?? ((_error: mixed) => {});
    // Built once and shared by both renders below, so a page that threw its
    // shell away and rendered its error boundary instead reports the stream the
    // browser was actually sent rather than the one that was abandoned.
    const send = settings?.onStream;
    const onStream = send == null ? undefined : streamReporter(url, send);
    // Read rather than minted: a project that has not asked for a nonce gets
    // `null` and the document it has always had. Read once for both renders
    // below, so the error document a failed shell falls back to carries the
    // same nonce as the policy already on the response.
    const nonce = currentNonce();

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
        shell: shellFor(assets, nonce, settings?.formState),
        onError,
        transformHead: settings?.transformHead,
        onStream,
        nonce,
        formState: settings?.formState,
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
        shell: shellFor(assets, nonce, settings?.formState),
        onError,
        transformHead: settings?.transformHead,
        onStream,
        nonce,
        formState: settings?.formState,
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
    const report = settings?.onError ?? ((_error: mixed) => {});

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

export {
  createInstrumentation,
  instrumentRender,
  traceRequestPhase,
} from "@uniflowed/server/instrumentation";
