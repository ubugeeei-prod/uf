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

import { DATA_ID, ROOT_ID } from "./internal/document.js";
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
|};

/** The two ids the server writes and the client reads. */
export { DATA_ID, ROOT_ID } from "./internal/document.js";

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
   * to do with how the answer is delivered. Returning the redirect rather than
   * throwing it keeps the two callers from each having to remember that a
   * redirect is the one thing `resolveMatch` lets out.
   */
  async function resolve(url: string): Promise<Resolution> {
    try {
      return { kind: "route", route: await resolveMatch(table, url) };
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
    const resolution = await resolve(url);
    if (resolution.kind === "redirect") {
      return redirectDocument(resolution.error);
    }
    let resolved: ResolvedRoute = resolution.route;
    const report = settings?.onError ?? (() => {});

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
        shell: shellFor(resolved, assets),
        onError,
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
        shell: shellFor(resolved, assets),
        onError,
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
    const resolution = await resolve(url);
    if (resolution.kind === "redirect") {
      return redirectResult(redirectDocument(resolution.error));
    }
    let resolved: ResolvedRoute = resolution.route;
    const report = settings?.onError ?? (() => {});

    let html: string;
    try {
      html = await prerenderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(resolved, assets),
        onError: report,
      });
    } catch (error) {
      if (error instanceof RedirectError) {
        return redirectResult(redirectDocument(error));
      }
      resolved = await resolveFailure(table, url, error);
      html = await prerenderDocument(<App url={url} initial={resolved} />, {
        shell: shellFor(resolved, assets),
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
 */
function shellFor(resolved: ResolvedRoute, assets: RenderAssets): DocumentShell {
  const head = headTags(assets) + dataScript(resolved.data);
  const title =
    resolved.metadata.title != null ? `<title>${escapeText(resolved.metadata.title)}</title>` : "";
  return {
    head,
    open: `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">${title}${head}</head><body><div id="${ROOT_ID}">`,
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

/**
 * The loader data, embedded for hydration.
 *
 * `<` is escaped inside the JSON so a string holding `</script>` cannot end
 * the element early, and the script's type keeps the browser from executing
 * it.
 */
function dataScript(data: mixed): string {
  if (data === undefined) {
    return "";
  }
  const json = JSON.stringify(data)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
  return `<script id="${DATA_ID}" type="application/json">${json}</script>`;
}

function escapeAttribute(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

function escapeText(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
