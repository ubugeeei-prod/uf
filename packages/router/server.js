// @flow
//
// Rendering one URL to an HTML document.
//
// `virtual:uf/server` calls `createRenderer` with the app root and the route
// table, and `uf dev` renders every document request through the result while
// `uf build` renders every static route through it once. Both produce the
// same markup from the same code, which is the point.

import { DATA_ID, ROOT_ID } from "./internal/document.js";
import * as React from "react";
import { renderToString } from "react-dom/server";

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

/** A rendered document. */
export type RenderResult = {|
  readonly status: number,
  readonly html: string,
  readonly headers?: { readonly [string]: string },
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
   */
  readonly error?: mixed,
|};

/**
 * Build a `render(url, assets)` for one app.
 */
export { DATA_ID, ROOT_ID } from "./internal/document.js";

export type { Handler, HandlerContext, HandlerModule, HandlerRecord } from "./handler.js";
export { createDispatcher } from "./handler.js";

export function createRenderer(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
  readonly errors: RouteTable["errors"],
|}): (url: string, assets: RenderAssets) => Promise<RenderResult> {
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };
  installRoutes(table);
  const { App } = options;

  return async function render(url: string, assets: RenderAssets): Promise<RenderResult> {
    let resolved: ResolvedRoute;
    try {
      resolved = await resolveMatch(table, url);
    } catch (error) {
      // A redirect is the only thing `resolveMatch` lets out, because a
      // redirect is a response rather than a page.
      if (error instanceof RedirectError) {
        return redirectDocument(error);
      }
      throw error;
    }

    let markup: string;
    try {
      markup = renderToString(<App url={url} initial={resolved} />);
    } catch (error) {
      // The server's half of the error boundary. React does not run class
      // boundaries in `renderToString` — Fizz has no `getDerivedStateFromError`
      // step outside a Suspense boundary — so `RouteView`'s boundary is the
      // browser's containment and this is the server's. Without it one
      // component that throws is the whole response, and during `uf build` the
      // whole build. See ubugeeei-prod/uf#257.
      if (error instanceof RedirectError) {
        return redirectDocument(error);
      }
      resolved = await resolveFailure(table, url, error);
      // Deliberately not caught again: this render is the boundary's own
      // component, and a boundary that throws has nothing left to answer with.
      // It reaches `uf dev`'s overlay and fails `uf build`'s route, which is
      // where somebody can fix it.
      markup = renderToString(<App url={url} initial={resolved} />);
    }

    const html = assemble(markup, resolved, assets);
    return { status: resolved.status, html, error: renderFailure(resolved) };
  };
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
  return {
    status: error.permanent ? 308 : 307,
    headers: { Location: error.to },
    html: `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=${target}"><title>Redirecting</title></head><body><a href="${target}">Redirecting…</a></body></html>\n`,
  };
}

/**
 * Turn the app's markup into a complete document.
 *
 * An app whose root layout renders `<html>` owns the whole document, and the
 * client hydrates `document`; the scripts and stylesheets are inserted before
 * `</head>`. An app that renders only content is wrapped in a minimal shell
 * around `<div id="uf-root">`, which is what the client hydrates instead.
 */
function assemble(markup: string, resolved: ResolvedRoute, assets: RenderAssets): string {
  const head = headTags(assets) + dataScript(resolved.data);
  if (/^\s*<html[\s>]/i.test(markup)) {
    const document = markup.includes("</head>")
      ? markup.replace("</head>", `${head}</head>`)
      : markup.replace(/<html([^>]*)>/i, `<html$1><head>${head}</head>`);
    return `<!doctype html>\n${document}\n`;
  }
  const title =
    resolved.metadata.title != null ? `<title>${escapeText(resolved.metadata.title)}</title>` : "";
  return `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">${title}${head}</head><body><div id="${ROOT_ID}">${markup}</div></body></html>\n`;
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
