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
// usefully suspend, so `_uf.loading.js` had nothing to be. Making the split
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

import { type StreamDiagnostic, streamReporter } from "./internal/inspector.js";

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
   * Rewrite the opening chunk — everything up to and including the head —
   * before it goes out.
   *
   * For `uf dev` and nothing else. Vite's `transformIndexHtml` injects
   * `/@vite/client` and the refresh preamble and rewrites asset URLs, and it
   * is a *whole document* hook, so the development server used to collect the
   * page and transform it at the end. That made the one place a developer
   * would notice streaming the one place it did not happen: a slow page showed
   * nothing until it was finished, and `_uf.loading.js` looked broken.
   * See ubugeeei-prod/uf#374.
   *
   * A production host passes nothing here and streams as it always did.
   *
   * # What a plugin that injects into the body gets
   *
   * `transformIndexHtml` is a whole-document hook and this hands it one chunk,
   * so `injectTo` is answered against a document that stops inside `<body>`.
   * Measured against Vite 8.2.2, injecting all four positions into a whole
   * document and into a head-only one:
   *
   * | `injectTo`     | whole document      | streamed |
   * | -------------- | ------------------- | -------- |
   * | `head-prepend` | after `<head>`      | same     |
   * | `head`         | before `</head>`    | same     |
   * | `body-prepend` | after `<body>`      | same     |
   * | `body`         | before `</body>`    | **after `<body>`** |
   *
   * Nothing is dropped — every tag still reaches the document — but a `body`
   * tag lands at the *top* of the body rather than after the content, because
   * the content has not been rendered yet when the hook runs. For a `<script>`
   * that expects a complete DOM that is a real difference, and it is the price
   * of streaming: a hook that wants the whole document and a server that sends
   * the head first cannot both be satisfied.
   *
   * uf's own injections are `head` and `head-prepend`, and Vite's client is
   * head-injected, so this is about a third-party plugin. `dev-head-transform`
   * in `tests/library` pins the table above, so the day it changes is a failing
   * test rather than a surprise.
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
 * lives in an `AsyncLocalStorage` belonging to *that module instance*. A host
 * that resolved `@uniflowed/server/host` for itself — from its own
 * `node_modules`, or from outside the bundle a build produced — would begin a
 * request in a second storage, and every `cookies()` in the application would
 * still be outside one, silently. Handing it out from here makes the copy the
 * host begins with the copy this module dispatches and renders with, because
 * it is the same import.
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
