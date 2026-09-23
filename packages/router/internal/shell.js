// @flow
//
// Internal to `@uniflowed/router`: the documents an HTML renderer answers with
// that are not a route's own markup — uf's shell around that markup, and a
// redirect.
//
// `../server.js` renders a route from its modules, and `../rsc-ssr.js` renders
// one from the payload React Server Components wrote. They are two entries so
// that the second, which loads React's Flight client, stays out of every bundle
// that renders no Server Component (ubugeeei-prod/uf#992). Both write the same
// documents around what they render, which is why those documents live here and
// not in either entry.

import type { PrerenderResult, RenderAssets, RenderResult } from "../server.js";
import { addressOf } from "./base-path.js";
import { DEPLOYMENT_META } from "./deployment.js";
import { ROOT_ID } from "./document.js";
import { type FormState, formStateScript } from "./form-action.js";
import type { RedirectError } from "./routing.js";
import { type DocumentShell, bodyOfText } from "./stream.js";

/** A redirect, as the finished document `prerender` answers with. */
export async function redirectResult(document: RenderResult): Promise<PrerenderResult> {
  return {
    status: document.status,
    headers: document.headers,
    html: await document.text(),
  };
}

export function redirectDocument(error: RedirectError): RenderResult {
  // Under `app.router.basePath`, and in the trailing-slash policy's spelling:
  // `redirect("/sign-in")` names an application path, and a browser follows
  // an address.
  const address = addressOf(error.to);
  const target = escapeAttribute(address);
  // A document rather than an empty body, because a redirect is still an answer
  // a browser may be shown; it goes through the same three methods as a
  // rendered one so that a host has one shape to write, not two.
  const body = bodyOfText(
    `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=${target}"><title>Redirecting</title></head><body><a href="${target}">Redirecting…</a></body></html>\n`,
  );
  return {
    status: error.permanent ? 308 : 307,
    headers: { Location: address },
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
export function shellFor(
  assets: RenderAssets,
  nonce?: string | null,
  formState?: FormState,
): DocumentShell {
  // A postback's form state goes first, before the client entry that reads it:
  // data rather than a script, so it needs no nonce. See `./form-action.js`.
  const head = (formState == null ? "" : formStateScript(formState)) + headTags(assets, nonce);
  return {
    head,
    open: `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">`,
    body: `${head}</head><body><div id="${ROOT_ID}">`,
    close: `</div></body></html>\n`,
  };
}

function headTags(assets: RenderAssets, nonce?: string | null): string {
  let tags = "";
  // Which build this document is, first, because everything after it is a URL
  // that build wrote. The browser reads it once and names it on every action
  // call and payload request, so a server on another build can refuse rather
  // than answer with ids and chunks this page does not have. See
  // `./deployment.js`.
  const deployment = assets.deployment;
  if (deployment != null && deployment !== "") {
    tags += `<meta name="${DEPLOYMENT_META}" content="${escapeAttribute(deployment)}">`;
  }
  for (const href of assets.styles) {
    tags += `<link rel="stylesheet" href="${escapeAttribute(href)}">`;
  }
  for (const href of assets.preloads) {
    tags += `<link rel="modulepreload" href="${escapeAttribute(href)}">`;
  }
  // The nonce goes on the client entry even though it is `src` rather than
  // inline, because a policy of `script-src 'nonce-…'` admits *no* script
  // without one — a nonce policy is not an inline policy with an exception in
  // it. A project whose policy also names `'self'` pays nothing for the
  // attribute being here, and one whose policy is `'strict-dynamic'` needs it:
  // that is the directive under which this script is the root of trust every
  // chunk it imports inherits from.
  const carried = nonce == null ? "" : ` nonce="${escapeAttribute(nonce)}"`;
  for (const src of assets.scripts) {
    tags += `<script type="module" src="${escapeAttribute(src)}"${carried}></script>`;
  }
  return tags;
}

function escapeAttribute(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
