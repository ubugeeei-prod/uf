// @flow
//
// Internal to `@uniflowed/router`: the endpoint a server action is dialled at.
//
// `createActionDispatcher` is the fourth thing `virtual:uf/server` exports and
// the third thing a host calls, between the middleware and the route handlers.
// It takes a table of `{ id, module, export, load }` built from the RSC
// manifest, and answers a request that carries an action id — or declines,
// with `null`, so that everything else about the request is somebody else's.
//
// # This is a public endpoint, and it is treated as one
//
// A `"use server"` export is reachable by anything that can make an HTTP
// request from the moment the build contains it. Every rule below is one of
// `docs/security.md`'s applied to that fact, and the table there has the row.
//
// **1. Only the build's own actions are dialable.** The table is the
// manifest's `serverActions`, which `uf_rsc` writes only for an action some
// module that can hand it across a client boundary reaches
// (`ActionExposure::CallableEndpoint`). A `"use server"` function nothing
// exposes has no row and therefore no endpoint. There is no path from a
// request to a module specifier, a file name or an export name: the id selects
// a row, and the row was decided at build time.
//
// **2. The id is not guessable.** `uf_rsc::ActionId` is
// `HMAC-SHA256(build id, module ‖ export ‖ kind)`, so the whole repository plus
// a published sourcemap is not enough to derive the id of a function the
// interface does not offer, and a rebuild changes every id.
//
// **3. One answer for every failed lookup.** `404`, with no body, for a
// malformed id, a well-formed id nobody has, and an id that names something not
// callable. `select` compares against every row whatever happens, and compares
// with `sameId`, so neither the answer nor the time taken says which. This is
// `ServerActionRegistry::resolve`'s contract, kept on the other side of the
// wire.
//
// **4. Cross-site calls cannot happen by accident, three times over.** The
// call is a `POST` carrying `uf-action`, which is not a header a simple request
// may set, so a cross-origin caller needs a preflight and nothing here answers
// one. It must be `application/json`, which is not a content type a `<form>`
// can produce. And `Origin` must be present and must equal `Host`. Any one of
// the three would do; all three are here because the cost is three `if`s and
// the failure is somebody's account.
//
// `Origin` is compared against `Host` and against nothing else. `Host` is what
// the browser was talking to, which is exactly the question being asked, and
// `X-Forwarded-Host` is a string the caller wrote (`docs/security.md`, rule 2)
// — so a proxy in front of a uf application has to preserve `Host`, and one
// that rewrites it turns every action call into a `403` rather than into a
// hole. A refusal is the right way for that to be discovered.
//
// **5. The argument boundary is `./action-wire.js` and only that.** Bounded
// bytes, bounded depth, bounded count, valid UTF-8, plain JSON data, no
// prototype keys, no constructor named by the payload. See that module's
// header for what is excluded and why.
//
// A submitted form is inside that boundary and does not widen it: `<form
// action={fn}>` on a hydrated page reaches here as the same `application/json`
// body, with the form's entries beside the values under a `form` key, bounded
// in count and in field-name length and holding strings only. Nothing about it
// is multipart and nothing about it is a content type a cross-origin `<form>`
// could produce, so rule 4 is exactly as true of a form call as of any other.
//
// # The second door: a form posted before the page hydrated
//
// React's progressive enhancement is a *native* form post, and a native post is
// the one request rule 4 was written to keep out. So it is not let in through
// that door. It has its own, narrower than the first, and it is worth being
// exact about what it keeps of the six rules above:
//
// - **Rules 1, 2, 3, 5 and 6 hold unchanged.** The id selects a row and is
//   compared the same way; a failed lookup is the same `404`; the bound
//   arguments and the form cross through `decodeActionArguments`, so the
//   grammar, the counts, the depth and the field limits are the JSON call's
//   own; a throw is the same fixed `500`.
// - **Rule 4 keeps one of its three guards.** A native post is a simple
//   request, so there is no custom header and no JSON content type to lean on.
//   What is left is `Origin`, which every browser sends on a `POST` and which
//   must equal `Host` exactly as before — and `Sec-Fetch-Site`, which must say
//   `same-origin` when the browser sends it. A cross-site form, a sandboxed
//   frame (`Origin: null`) and a request with no `Origin` are `403`s.
// - **It accepts one content type**, `application/x-www-form-urlencoded`, which
//   is what `$$FORM_ACTION` asks React to write. Not multipart: a file cannot
//   cross to an action, and a multipart parser is surface uf does not open.
// - **It is recognised by its fields, read from a copy of the body.** A post
//   that carries no `$uf_ref_` field is somebody else's — an ordinary form to a
//   route handler — and is declined with `null` and its body untouched. So is
//   one larger than an action accepts or not UTF-8, because it cannot be known
//   to be an action post without reading it; it goes on to the route handlers,
//   and a page answers a `POST` with a `404`.
//
// The answer is a document rather than JSON, because a person is looking at it:
// a `303` back to the page for a plain form action, the page rendered again
// with the action's result as the submitting `useActionState`'s state (the
// host supplies that render as `postback`), or a `303` to wherever
// `redirect()` pointed. See `./form-action.js` for the fields and
// ubugeeei-prod/uf#1358.
//
// **6. Nothing about a failure goes back.** An action that throws is a `500`
// with a fixed body; the exception goes to the host's error reporting. A
// message, a name or a stack in that response is an application's internals
// published to whoever asked for them, and the same is true in development —
// `uf dev` and `uf build` have to agree about what this endpoint answers, and
// the terminal is where a developer reads the exception anyway.
//
// # What a middleware does and does not do for an action
//
// The call is a `POST` to the page's own URL rather than to a path uf reserves,
// so the middleware that guards that path runs above it exactly as it does for
// the page — no new route to collide with a project's own, and no second
// spelling of "which guard applies here".
//
// That is a convenience and it is not a boundary, because the URL is the
// caller's to choose: a client that wants to skip the guard on `/dashboard`
// posts the same id to `/`. **A server action authorizes itself.** It is the
// unit of authorization, the way a route handler is, and a `"use server"`
// function that relies on a path guard having run is a function with a hole in
// it. Written here because this is the file somebody reads before deciding
// otherwise.

import { traceRequestPhase, reportRequestError } from "@uniflowed/server/instrumentation";

import { asResponder, nativeActionAllowed } from "@uniflowed/server/host";

import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  ACTION_OUTCOME_HEADER,
  type ActionArgument,
  ActionValueError,
  MAX_ACTION_BODY_BYTES,
  decodeActionArguments,
  encodeActionResult,
  isActionId,
} from "./action-wire.js";
import { addressOf } from "./base-path.js";
import {
  FORM_ACTION_CONTENT_TYPE,
  type FormPost,
  type FormState,
  readFormPost,
} from "./form-action.js";
import { requireRequest } from "./request.js";
import { ForbiddenError, NotFoundError, RedirectError, UnauthorizedError } from "./routing.js";

export type { FormState } from "./form-action.js";

/**
 * What a host gives the dispatcher beyond the request.
 *
 * `postback` renders the page the request is for, with a `useActionState`'s
 * result as React's `formState`, and is how a form posted before hydration
 * gets the page back with its answer in it. A host that supplies none still
 * serves native posts: the answer is a `303` back to the page, which runs the
 * action and loses only the state.
 */
export type ActionDispatchOptions = {|
  readonly postback?: (formState: FormState) => Promise<Response>,
|};

/** A module holding server actions, as the generated table loads it. */
export type ActionModule = { readonly [name: string]: mixed };

/**
 * One callable action, as `virtual:uf/actions` writes it.
 *
 * `module` and `export` are here for the error a broken build produces, never
 * for dispatch: dispatch is `id` against `id`. Nothing a request carries is
 * ever joined onto a path or used to name an export.
 */
export type ActionRecord = {|
  /** The keyed id, 64 lowercase hexadecimal characters. */
  readonly id: string,
  /** Declaring module, relative to the project root. */
  readonly module: string,
  /** The exported binding, or `default`. */
  readonly export: string,
  /** Import the declaring module. */
  readonly load: () => Promise<ActionModule>,
|};

/** Headers every answer carries, whatever it says. */
const ANSWER_HEADERS: { readonly [string]: string } = {
  "content-type": "application/json; charset=utf-8",
  // An action call is a `POST` and is not cached by anything by default. Said
  // anyway, because "not cached by default" is the sentence in front of every
  // cache-poisoning advisory in `docs/security.md`.
  "cache-control": "no-store",
};

/**
 * Match a request against the action table and run what it names.
 *
 * Returns `null` when the request carries no action id, which is the caller's
 * signal to carry on: everything that is not an action call is a page, a
 * handler or a 404, and this declines all of them. Every other outcome —
 * including every refusal — is a `Response`, because a request that named an
 * action and did not get to call one must not fall through to something else
 * that might answer it.
 */
export function createActionDispatcher(options: {|
  readonly actions: $ReadOnlyArray<ActionRecord>,
|}): (request: Request, settings?: ActionDispatchOptions) => Promise<Response | null> {
  const table = options.actions;

  return async function callAction(
    request: Request,
    settings?: ActionDispatchOptions,
  ): Promise<Response | null> {
    // The host's half of the contract, checked rather than assumed, exactly as
    // `dispatch` and `runMiddleware` check it: an action that calls `cookies()`
    // has to answer about the request it is inside.
    requireRequest("callAction");

    const id = request.headers.get(ACTION_HEADER);
    if (id == null) {
      // Not a JSON call. It may still be a form posted before its page
      // hydrated, which is the second door in the header.
      return nativeFormPost(table, request, settings?.postback);
    }

    // Cheapest and most protective first, and all of it before a byte of the
    // body is read.
    if (request.method.toUpperCase() !== "POST") {
      return new Response(null, { status: 405, headers: { ...ANSWER_HEADERS, allow: "POST" } });
    }
    const native = request.headers.has("uf-native-action");
    if (native ? !nativeActionAllowed(request) : !sameOrigin(request)) {
      return refusal(403);
    }
    if (!isJson(request.headers.get("content-type"))) {
      return refusal(415);
    }

    const body = await readBoundedText(request, MAX_ACTION_BODY_BYTES);
    if (body == null) {
      return refusal(413);
    }

    let args: Array<ActionArgument>;
    try {
      args = decodeActionArguments(body);
    } catch (error) {
      // The reason is the sender's own payload described back to them, which
      // is a thing to write in a log and not a thing to answer with.
      if (!(error instanceof ActionValueError)) {
        throw error;
      }
      return refusal(400);
    }

    // Decoded before the lookup, so that a malformed payload and an unknown id
    // cannot be told apart by trying one against the other.
    const record = isActionId(id) ? select(table, id) : null;
    if (record == null) {
      return refusal(404);
    }

    let action: mixed;
    try {
      const module = await record.load();
      action = module[record.export];
    } catch (error) {
      report(record, error);
      return refusal(500);
    }
    if (typeof action !== "function") {
      // The RSC graph rejects a `"use server"` export that is not an async
      // function at build time, so reaching this means the manifest and the
      // modules disagree — a stale `.uf/rsc/uf-rsc-manifest.json`, or a build
      // half-written. It is uf's bug, not the caller's, so it is reported and
      // answered as a `500`.
      report(record, new Error(`export \`${record.export}\` is not a function`));
      return refusal(500);
    }

    // Everything from here to the answer runs as the thing that owns this
    // response, which is what makes `draftMode().enable()` legal in an action:
    // a `"use server"` function is one of the two places uf lets draft mode be
    // changed, and the `Set-Cookie` it decides on is written onto the response
    // this returns rather than onto an object the host discards. See
    // ubugeeei-prod/uf#282 and `asResponder`.
    //
    // The refusals stay outside it. A `403` for a cross-origin call must not
    // carry a cookie the caller asked for, and a scope that covered them would
    // be a scope in which nothing ran that could have asked.
    return asResponder("a server action", async () => {
      let result: mixed;
      try {
        // The build-time contract says this is `async (...ActionArgument) => …`
        // (`ServerActionBoundary` in `../action.js`), and Flow cannot read that
        // through a module loaded by a thunk. The arguments are the ones
        // `decodeActionArguments` produced, so what is unchecked here is the
        // shape of the function and not the shape of the payload.
        const call = action as $FlowFixMe;
        result = await traceRequestPhase("action", () => call(...args));
      } catch (error) {
        // `redirect()` and its three siblings are where the visitor goes next,
        // not something that went wrong: answered as themselves, and never
        // reported. See `ACTION_OUTCOME_HEADER`.
        const outcome = routingOutcome(error);
        if (outcome != null) {
          return outcome;
        }
        report(record, error);
        return refusal(500);
      }

      let answer: string;
      try {
        answer = encodeActionResult(result);
      } catch (error) {
        // The action ran and its return value cannot cross. Flow says so at
        // build time — `ServerActionBoundary` in `../action.js` holds every
        // action's return type against the grammar — so this is the case where
        // it was reached anyway, and half a value is worse than none.
        report(record, error);
        return refusal(500);
      }
      return new Response(answer, { status: 200, headers: { ...ANSWER_HEADERS } });
    });
  };
}

/**
 * Answer a form the browser posted natively, or decline with `null`.
 *
 * The header's second door. Declines everything that is not a urlencoded
 * `POST` carrying a `$uf_ref_` field, and answers everything that is,
 * refusals included — the same promise the JSON door makes, for the same
 * reason.
 */
async function nativeFormPost(
  table: $ReadOnlyArray<ActionRecord>,
  request: Request,
  postback: ?(formState: FormState) => Promise<Response>,
): Promise<Response | null> {
  if (request.method.toUpperCase() !== "POST") {
    return null;
  }
  if (!isMedia(request.headers.get("content-type"), FORM_ACTION_CONTENT_TYPE)) {
    return null;
  }
  // A copy, so that a form which turns out not to be an action post reaches the
  // route handler with its body unread.
  const text = await readBoundedText(request.clone(), MAX_ACTION_BODY_BYTES);
  if (text == null) {
    return null;
  }
  const post = readFormPost(new URLSearchParams(text));
  if (post == null) {
    return null;
  }

  // An action post from here on, and every outcome is an answer.
  if (!sameOrigin(request) || !sameSiteFetch(request)) {
    return refusal(403);
  }

  let args: Array<ActionArgument>;
  let bound: number;
  try {
    ({ args, bound } = formArguments(post));
  } catch (error) {
    if (!(error instanceof ActionValueError)) {
      throw error;
    }
    return refusal(400);
  }

  const id = post.id;
  const record = id != null && isActionId(id) ? select(table, id) : null;
  if (record == null) {
    return refusal(404);
  }

  let action: mixed;
  try {
    const module = await record.load();
    action = module[record.export];
  } catch (error) {
    report(record, error);
    return refusal(500);
  }
  if (typeof action !== "function") {
    report(record, new Error(`export \`${record.export}\` is not a function`));
    return refusal(500);
  }

  const url = new URL(request.url);
  const stateKey = post.stateKey;
  return asResponder("a server action", async () => {
    let result: mixed;
    try {
      const call = action as $FlowFixMe;
      result = await traceRequestPhase("action", () => call(...args));
    } catch (error) {
      if (error instanceof RedirectError) {
        return seeOther(addressOf(error.to));
      }
      // A person is looking at this answer and there is no page to render the
      // boundary into, so it is the status with a fixed line of text — the same
      // three statuses the JSON door answers, and nothing of the exception.
      const status = routingStatus(error);
      if (status != null) {
        return new Response(`${status.text}\n`, {
          status: status.code,
          headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
        });
      }
      report(record, error);
      return refusal(500);
    }
    if (stateKey == null || postback == null) {
      // Post/redirect/get: the page again, by a `GET`, so a reload does not
      // submit the form a second time.
      return seeOther(addressOf(withOneLeadingSlash(url.pathname) + url.search));
    }
    try {
      // Held to the grammar for the reason the JSON door holds a result to it:
      // this value is written into a document a browser parses.
      encodeActionResult(result);
    } catch (error) {
      report(record, error);
      return refusal(500);
    }
    // `bound - 1`: `useActionState` bound the previous state itself, and
    // React compares the count of the bindings the *action* had.
    return postback([result, stateKey, record.id, bound - 1]);
  });
}

/**
 * The arguments a native post calls its action with: the bound ones, then
 * the form.
 *
 * Built into the JSON call's own envelope and decoded by the JSON call's own
 * decoder, so there is one grammar and one set of limits, not a second one for
 * forms that could drift from the first.
 */
function formArguments(post: FormPost): {|
  readonly args: Array<ActionArgument>,
  readonly bound: number,
|} {
  let bound: Array<ActionArgument> = [];
  if (post.bound != null) {
    bound = decodeActionArguments(post.bound);
    if (bound.some((value) => value instanceof FormData)) {
      throw new ActionValueError("the bound arguments", "carry a form");
    }
  }
  const envelope = JSON.stringify({
    args: [...bound, null],
    form: { at: bound.length, entries: post.entries },
  });
  return { args: decodeActionArguments(envelope), bound: bound.length };
}

/**
 * Whether the browser says the request came from this origin, when it says.
 *
 * `Sec-Fetch-Site` is a forbidden header name, so a page cannot set it; a
 * browser that sends it and says anything but `same-origin` is describing a
 * cross-site form. Absent is not a refusal on its own — older browsers do not
 * send it, and `Origin` has already been required.
 */
function sameSiteFetch(request: Request): boolean {
  const site = request.headers.get("sec-fetch-site");
  return site == null || site === "same-origin";
}

/** The kind, status and text of a routing error other than a redirect. */
type RoutingStatus = {|
  readonly kind: "not-found" | "unauthorized" | "forbidden",
  readonly code: 404 | 401 | 403,
  readonly text: string,
|};

/**
 * What `notFound()`, `unauthorized()` or `forbidden()` answers with, or `null`
 * for anything else.
 *
 * The statuses are the ones a page gets for the same call (`routeErrorStatus`
 * and the not-found page), so a client that reads only the status reads the
 * same thing a document request would have told it.
 */
function routingStatus(error: mixed): RoutingStatus | null {
  if (error instanceof NotFoundError) {
    return { kind: "not-found", code: 404, text: "not found" };
  }
  if (error instanceof UnauthorizedError) {
    return { kind: "unauthorized", code: 401, text: "unauthorized" };
  }
  if (error instanceof ForbiddenError) {
    return { kind: "forbidden", code: 403, text: "forbidden" };
  }
  return null;
}

/**
 * The JSON door's answer for a routing error, or `null` for any other throw.
 *
 * A redirect is a `204` carrying `Location`, the address `addressOf` makes of
 * it (under the base path for a path on this application). It is not a `3xx`,
 * because `fetch` would follow that itself and fetch the page's HTML for
 * nothing, and the reference could not read where it pointed. The other three
 * are their page statuses. Each one carries `ACTION_OUTCOME_HEADER`, so a `404`
 * from `notFound()` is not mistaken for the `404` of an unknown id. The body
 * names the kind and nothing else.
 */
function routingOutcome(error: mixed): Response | null {
  if (error instanceof RedirectError) {
    const headers: { [string]: string } = {
      "cache-control": "no-store",
      location: addressOf(error.to),
    };
    headers[ACTION_OUTCOME_HEADER] = "redirect";
    return new Response(null, { status: 204, headers });
  }
  const status = routingStatus(error);
  if (status == null) {
    return null;
  }
  const headers: { [string]: string } = { ...ANSWER_HEADERS };
  headers[ACTION_OUTCOME_HEADER] = status.kind;
  return new Response(JSON.stringify({ outcome: status.kind }), {
    status: status.code,
    headers,
  });
}

/**
 * `path` with a leading run of `/` counted down to one.
 *
 * The post/redirect/get above answers with the path the form was posted to,
 * and that path is the request's: `https://app.example//evil.example/notes` is
 * a link anybody can write, its page posts its forms back to the same address,
 * and a URL parser reads `/\evil.example` as the same two slashes. Written
 * into `Location` as it stands, `//evil.example/notes` is a network-path
 * reference and the browser follows it to another host — after the person
 * submitted a form on this one. `@uniflowed/server` makes the same repair to
 * the trailing-slash redirect (`withOneLeadingSlash` in its `routing.js`); the
 * router cannot import it, so it is spelled here as a counted loop, never a
 * pattern (`docs/security.md`, rule 5).
 */
function withOneLeadingSlash(path: string): string {
  let start = 0;
  while (start + 1 < path.length && path.charCodeAt(start + 1) === 47) {
    start += 1;
  }
  return path.charCodeAt(0) === 47 ? path.slice(start) : path;
}

/** A `303 See Other`, which a browser follows with a `GET`. */
function seeOther(location: string): Response {
  return new Response(null, {
    status: 303,
    headers: { location, "cache-control": "no-store" },
  });
}

/**
 * The row with this id, or `null`, without saying which by taking longer.
 *
 * Mirrors `ServerActionRegistry::lookup`: every row is visited whatever
 * happens, the comparison is over the whole id, and the selected index is
 * carried in arithmetic rather than in a branch. Two rows can never share an
 * id, so the last match is the only match.
 */
function select(table: $ReadOnlyArray<ActionRecord>, id: string): ActionRecord | null {
  let selected = 0;
  let found = 0;
  for (let index = 0; index < table.length; index += 1) {
    const matches = sameId(table[index].id, id);
    const mask = -matches;
    selected = (selected & ~mask) | (index & mask);
    found |= matches;
  }
  return found === 1 ? table[selected] : null;
}

/**
 * Whether two ids are the same, in time that does not depend on how much of
 * one matched.
 *
 * The length is compared first and that is not a leak: every action id is
 * exactly 64 characters and `isActionId` has already said so about this one.
 */
function sameId(left: string, right: string): number {
  if (left.length !== right.length) {
    return 0;
  }
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0 ? 1 : 0;
}

/**
 * Whether the request came from the page it claims to have come from.
 *
 * Deny by default: a call with no `Origin` is refused rather than trusted,
 * because "the header was absent" and "the header matched" are not the same
 * fact and only one of them is a browser saying where it was.
 */
function sameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  const host = request.headers.get("host");
  if (origin == null || host == null) {
    return false;
  }
  let parsed: URL;
  try {
    // `Origin: null` — a sandboxed frame, a `data:` document, some redirects —
    // is not a URL and is refused here rather than being special-cased into a
    // value that could match something.
    parsed = new URL(origin);
  } catch {
    return false;
  }
  // `host` on both sides, so the port is part of the comparison: `:5173` and
  // `:5174` on one machine are two origins and a cookie tells them apart.
  return parsed.host === host;
}

/**
 * Whether the declared content type is the one this endpoint accepts.
 *
 * The parameters after `;` are ignored — `application/json; charset=utf-8` is
 * the same media type — and the media type itself must match exactly. A
 * `+json` suffix is not accepted: the point of the check is that a form cannot
 * produce this string, and a widened match is a widened set of things that
 * can.
 */
function isJson(declared: string | null): boolean {
  return isMedia(declared, ACTION_CONTENT_TYPE);
}

/** Whether `declared` is exactly the media type `expected`, parameters aside. */
function isMedia(declared: string | null, expected: string): boolean {
  if (declared == null) {
    return false;
  }
  const semicolon = declared.indexOf(";");
  const media = (semicolon === -1 ? declared : declared.slice(0, semicolon)).trim().toLowerCase();
  return media === expected;
}

/**
 * The body as text, or `null` when it is larger than `limit`.
 *
 * Counted as it arrives rather than trusted from `Content-Length`, because a
 * chunked request declares no length and a declared one is the sender's claim.
 * The declared length is still read first, so an oversized request that is
 * honest about it is refused before its body is transferred at all.
 *
 * `fatal: true` refuses input that is not valid UTF-8 instead of replacing
 * each bad sequence with U+FFFD, which is `docs/security.md`'s row about
 * non-UTF-8 input applied at the one place uf decodes bytes a client sent.
 */
async function readBoundedText(request: Request, limit: number): Promise<string | null> {
  const declared = request.headers.get("content-length");
  if (declared != null) {
    const size = Number(declared);
    if (!Number.isInteger(size) || size < 0 || size > limit) {
      return null;
    }
  }

  // Flow's `Request` predates a body one can read as a stream, so the property
  // is reached through a cast and the shape it is used at is checked below.
  // `ReadableStream` is not polymorphic in the DOM declarations uf checks
  // against, so the element type is asserted at the one place it is read
  // rather than written here.
  const body: ReadableStream | null = (request as $FlowFixMe).body;
  if (body == null) {
    return "";
  }

  const reader = body.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  try {
    for (;;) {
      const step = await reader.read();
      if (step.done === true) {
        break;
      }
      const chunk: Uint8Array = step.value as $FlowFixMe;
      total += chunk.byteLength;
      if (total > limit) {
        // Cancelled rather than dropped, so the sender stops instead of
        // filling a queue nobody is reading. The reason is given because the
        // declarations uf checks against require one, and "too large" is what
        // a cancelled read of an oversized body is about.
        await reader.cancel("the body is larger than a server action accepts");
        return null;
      }
      chunks.push(chunk);
    }
  } catch {
    return null;
  }

  const joined = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    joined.set(chunk, at);
    at += chunk.byteLength;
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(joined);
  } catch {
    return null;
  }
}

/**
 * Every refusal, with the same body whatever it was.
 *
 * The status says what a caller may usefully do differently — retry with a
 * smaller body, send the header, look elsewhere — and the body says nothing at
 * all, because everything that could go in it is about the build.
 */
function refusal(status: number): Response {
  return new Response('{"error":"server action refused"}', {
    status,
    headers: { ...ANSWER_HEADERS },
  });
}

/**
 * Report an action that failed, where the host's error reporting can see it.
 *
 * The console, for the reason `createFetchHandler` gives about its own
 * `onError`: this runs inside a worker or a serverless invocation as often as
 * in a terminal, and losing the exception entirely is worse than putting it
 * somewhere a platform is expected to collect. The module and export are named
 * here and nowhere the caller can read.
 */
function report(record: ActionRecord, error: mixed): void {
  reportRequestError(error, "action");
  // eslint-disable-next-line no-console
  console.error(`uf: server action \`${record.export}\` in \`${record.module}\` failed`, error);
}
